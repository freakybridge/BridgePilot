//! Non-blocking, per-conversation cost reminders. No model requests or setting writes.
use chrono::DateTime;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[cfg(windows)]
#[path = "high_cost_windows.rs"]
mod windows;

const MAX_STATE: u64 = 1024 * 1024;
const MAX_TAIL: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum Setting {
    High,
    Normal,
    Unknown,
}

#[derive(Serialize, Deserialize)]
struct State {
    schema: u8,
    session_id: String,
    streak: u64,
    last_time: i64,
    processed: BTreeSet<String>,
    setting: Setting,
    #[serde(default)]
    diagnostic: Option<String>,
}

impl State {
    fn new(session: &str) -> Self {
        Self {
            schema: 1,
            session_id: session.into(),
            streak: 0,
            last_time: 0,
            processed: BTreeSet::new(),
            setting: Setting::Unknown,
            diagnostic: None,
        }
    }

    fn complete(&mut self, turn: &str, time: Option<i64>, setting: Setting) -> bool {
        if !self.processed.insert(turn.into()) {
            return false;
        }
        if time.is_some_and(|time| time <= self.last_time) {
            return false;
        }
        if let Some(time) = time {
            self.last_time = time;
        }
        self.setting = setting;
        self.streak = if setting == Setting::High {
            self.streak.saturating_add(1)
        } else {
            0
        };
        self.streak > 0 && self.streak.is_multiple_of(10)
    }
}

fn plain(path: &Path) -> Result<(), String> {
    for part in path.ancestors().filter(|p| p.exists()) {
        if crate::persistence::is_link_or_reparse(part).map_err(|e| e.to_string())? {
            return Err("reminder path traverses a link".into());
        }
    }
    Ok(())
}

fn identifier(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|s| {
        !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

pub fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|p| PathBuf::from(p).join(".codex"))
        })
}

// Head identity and bounded tail come from the same file handle.
fn context(
    home: &Path,
    payload: &Value,
    session: &str,
    turn: &str,
) -> Result<Option<(Value, i64)>, String> {
    let path = Path::new(
        payload["transcript_path"]
            .as_str()
            .ok_or("missing transcript")?,
    );
    plain(path)?;
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let home = home.canonicalize().map_err(|e| e.to_string())?;
    let relative = path
        .strip_prefix(&home)
        .map_err(|_| "transcript outside Codex home")?;
    if !matches!(
        relative
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str()),
        Some("sessions" | "archived_sessions")
    ) {
        return Err("transcript outside session directories".into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("transcript is not a file".into());
    }
    let mut head = String::new();
    BufReader::new((&mut file).take(131072))
        .read_line(&mut head)
        .map_err(|e| e.to_string())?;
    let meta: Value = serde_json::from_str(&head).map_err(|_| "invalid transcript identity")?;
    if meta["type"] != "session_meta"
        || meta["payload"]["id"] != session
        || !meta["payload"]["source"].is_string()
    {
        return Err("not a matching main conversation".into());
    }
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    let offset = length.saturating_sub(MAX_TAIL);
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_TAIL)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let tail = String::from_utf8_lossy(&bytes);
    let lines = tail.lines().skip(usize::from(offset != 0));
    let mut found = None;
    for line in lines {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event["type"] == "turn_context" && event["payload"]["turn_id"] == turn {
            found = event["timestamp"]
                .as_str()
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                .map(|t| (event["payload"].clone(), t.timestamp_micros()));
        }
    }
    Ok(found)
}

fn astra_high(model: &str, effort: Option<&str>) -> Option<bool> {
    if model.is_empty() {
        return None;
    }
    if model != "gpt-6-astra" && !model.starts_with("gpt-6-astra-") {
        return Some(false);
    }
    match effort {
        Some("high" | "xhigh" | "max" | "ultra") => Some(true),
        Some("none" | "minimal" | "low" | "medium") => Some(false),
        _ => None,
    }
}

fn classify(astra: Option<bool>, fast: Option<bool>) -> Setting {
    match (astra, fast) {
        (Some(true), _) | (_, Some(true)) => Setting::High,
        (Some(false), Some(false)) => Setting::Normal,
        _ => Setting::Unknown,
    }
}

#[derive(Debug, PartialEq)]
enum Override {
    Inherit,
    Known(bool),
    Unknown,
}

// Treat quoted debug strings as single tokens so user text cannot spoof fields.
fn tokens(body: &str) -> Option<Vec<String>> {
    static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#""(?:\\.|[^"\\])*"|[A-Za-z_][A-Za-z_0-9]*|[{}():,]"#).unwrap()
    });
    let mut cursor = 0;
    let mut depth = 0i64;
    let mut result = Vec::new();
    for m in TOKEN.find_iter(body) {
        if body[cursor..m.start()].contains('"') {
            return None;
        }
        cursor = m.end();
        let value = m.as_str();
        match value {
            "{" | "(" => depth += 1,
            "}" | ")" => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return None;
        }
        result.push(if value.starts_with('"') && value.len() > 130 {
            "<string>".into()
        } else {
            value.into()
        });
    }
    if body[cursor..].contains('"') || depth != 0 {
        return None;
    }
    Some(result)
}

fn field<'a>(tokens: &'a [String], kind: &str, field: &str) -> Option<&'a [String]> {
    let start = tokens
        .windows(2)
        .position(|w| w[0] == kind && w[1] == "{")?
        + 2;
    let mut braces = 1;
    let mut i = start;
    while i < tokens.len() {
        if tokens[i] == "{" {
            braces += 1;
        }
        if tokens[i] == "}" {
            braces -= 1;
            if braces == 0 {
                return None;
            }
        }
        if braces == 1 && tokens[i] == field && tokens.get(i + 1).is_some_and(|v| v == ":") {
            let begin = i + 2;
            let mut depth = 0;
            for end in begin..tokens.len() {
                match tokens[end].as_str() {
                    "(" | "{" => depth += 1,
                    ")" => depth -= 1,
                    "," | "}" if depth == 0 => return Some(&tokens[begin..end]),
                    "}" => depth -= 1,
                    _ => {}
                }
            }
            return None;
        }
        i += 1;
    }
    None
}

fn tier(value: Option<&[String]>, thread: bool) -> Override {
    let Some(value) = value else {
        return Override::Unknown;
    };
    let text = value.join("");
    match text.as_str() {
        "None" => Override::Inherit,
        "Some(None)" if thread => Override::Known(false),
        "Some(Some(\"default\"))" if thread => Override::Known(false),
        "Some(Some(\"priority\"))" if thread => Override::Known(true),
        "Some(\"default\")" if !thread => Override::Known(false),
        "Some(\"priority\")" if !thread => Override::Known(true),
        _ => Override::Unknown,
    }
}

fn fast_from_rows(rows: &[String], turn: &str) -> Option<bool> {
    let mut current = None;
    let mut result = None;
    for body in rows.iter().rev() {
        let marker = ": Submission sub=Submission {";
        let Some(start) = body.find(marker) else {
            continue;
        };
        let Some(tokens) = tokens(&body[start + 2..]) else {
            current = None;
            result = None;
            continue;
        };
        if field(&tokens, "Submission", "op")
            .and_then(|v| v.first())
            .is_none_or(|v| v != "TurnInput")
        {
            continue;
        }
        let id = field(&tokens, "Submission", "id")
            .and_then(|v| v.first())
            .and_then(|v| serde_json::from_str::<String>(v).ok());
        // A different or truncated request shape cannot provide a setting.
        let thread = tier(
            field(&tokens, "ThreadSettingsOverrides", "service_tier"),
            true,
        );
        let single = tier(field(&tokens, "TurnStartOptions", "service_tier"), false);
        if let Some(expected) = field(&tokens, "Steer", "expected_turn_id") {
            if expected
                .first()
                .is_some_and(|v| v == &format!("\"{turn}\""))
                && (thread != Override::Inherit || single != Override::Inherit)
            {
                result = None;
            }
            continue;
        }
        match thread {
            Override::Known(v) => current = Some(v),
            Override::Unknown => current = None,
            _ => {}
        }
        if id.as_deref() == Some(turn) {
            result = match single {
                Override::Known(v) => Some(v),
                Override::Inherit => current,
                Override::Unknown => None,
            };
        }
    }
    result
}

fn fast(home: &Path, session: &str, turn: &str) -> Option<bool> {
    let database = home.join("logs_2.sqlite");
    if plain(&database).is_err() {
        return None;
    }
    #[cfg(windows)]
    {
        windows::rows(&database, session)
            .ok()
            .and_then(|rows| fast_from_rows(&rows, turn))
    }
    #[cfg(not(windows))]
    {
        let _ = (database, session, turn);
        None
    }
}

/// Returns UI text only at a threshold. Errors are advisory and must never block Stop.
pub fn observe(root: &Path, home: &Path, payload: &Value) -> Result<Option<String>, String> {
    if payload["hook_event_name"] != "Stop"
        || payload["stop_hook_active"] != false
        || payload.get("agent_id").is_some_and(|v| !v.is_null())
        || payload["last_assistant_message"]
            .as_str()
            .is_none_or(|s| s.trim().is_empty())
    {
        return Ok(None);
    }
    let session = identifier(payload.get("session_id")).ok_or("invalid session id")?;
    let turn = identifier(payload.get("turn_id")).ok_or("invalid turn id")?;
    let name = format!("{:x}", Sha256::digest(session.as_bytes()));
    let directory = root.join(".runtime/bridgeforge-codex/high-cost-reminder");
    plain(&directory)?;
    let _lock = crate::file_lock::FileLock::acquire(&directory.join(format!("{name}.lock")))?;
    let path = directory.join(format!("{name}.json"));
    plain(&path)?;
    let mut state = if path.exists() {
        let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() > MAX_STATE {
            return Err("invalid reminder state file".into());
        }
        serde_json::from_slice::<State>(&fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|_| "invalid reminder state")?
    } else {
        State::new(session)
    };
    if state.schema != 1 || state.session_id != session {
        return Err("reminder state identity mismatch".into());
    }
    if state.processed.contains(turn) {
        return Ok(None);
    }
    let evidence = match context(home, payload, session, turn) {
        Ok(value) => value,
        Err(error) if error == "not a matching main conversation" => return Ok(None),
        Err(error) => {
            state.diagnostic = Some(error);
            None
        }
    };
    let (setting, time) = match evidence {
        Some((context, time)) => {
            let model = context["model"].as_str().unwrap_or("");
            let astra = if payload["model"].as_str() == Some(model) {
                astra_high(model, context["effort"].as_str())
            } else {
                None
            };
            let speed = if astra == Some(true) {
                None
            } else {
                fast(home, session, turn)
            };
            (classify(astra, speed), Some(time))
        }
        None => (Setting::Unknown, None),
    };
    let notify = state.complete(turn, time, setting);
    if setting != Setting::Unknown {
        state.diagnostic = None;
    } else if state.diagnostic.is_none() {
        state.diagnostic = Some("当前回合的模型、强度或 Fast 状态未知；连续计数已中断".into());
    }
    let bytes = serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_STATE {
        return Err("reminder state size limit reached".into());
    }
    crate::persistence::atomic_write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(notify.then(|| format!("高耗能提醒：本对话已连续 {} 轮使用 GPT-6 Astra＋High 或更高强度，或 Fast。请检查是否仍需要当前设置；模型、强度和速度均未自动更改。", state.streak)))
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_high_cost.rs"]
mod tests;
