use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::StepResult;
use crate::post::source_tree_hash;
use crate::util::{codex_root, repo_root, run_command};

fn git(args: &[String], timeout: Duration) -> Option<std::process::Output> {
    let root = repo_root();
    let mut safe_args = vec![
        "--no-optional-locks".to_string(),
        "-c".to_string(),
        format!(
            "safe.directory={}",
            root.to_string_lossy().replace('\\', "/")
        ),
    ];
    safe_args.extend(args.iter().cloned());
    run_command("git", safe_args, &root, timeout).ok()
}

fn skeleton_version() -> String {
    fs::read_to_string(codex_root().join(".bridgeforge_codex_version"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "?".into())
}

fn hooks_table(text: &str) -> Result<bool, String> {
    let header = Regex::new(r#"(?m)^\s*\[+\s*(?P<body>[^\]\r\n]+)\]+\s*(?:#.*)?$"#).unwrap();
    for line in text.lines() {
        if line.trim_start().starts_with('[') && !line.contains(']') {
            return Err("unclosed table header".into());
        }
    }
    for captures in header.captures_iter(text) {
        let body = captures
            .name("body")
            .unwrap()
            .as_str()
            .trim()
            .replace(['\'', '"', ' '], "");
        if body == "hooks" || body.starts_with("hooks.") {
            return Ok(true);
        }
    }
    let dotted = Regex::new(r"(?m)^\s*hooks(?:\.[A-Za-z0-9_-]+)+\s*=").unwrap();
    let inline = Regex::new(r"(?m)^\s*hooks\s*=\s*\{").unwrap();
    Ok(dotted.is_match(text) || inline.is_match(text))
}

pub fn config_health(strict: bool) -> StepResult {
    let root = repo_root();
    let mut failures: Vec<(&str, String)> = Vec::new();
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    for (label, path) in [
        ("~/.codex/settings.json", home.join(".codex/settings.json")),
        (".codex/settings.json", root.join(".codex/settings.json")),
        (
            ".codex/settings.local.json",
            root.join(".codex/settings.local.json"),
        ),
    ] {
        if path.is_file()
            && fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .is_none()
        {
            failures.push((
                "settings-json-valid",
                format!("settings.json invalid JSON: {label}. FIX: repair the JSON syntax."),
            ));
        }
    }
    let mut source = Vec::new();
    for (label, path) in [
        (".codex/settings.json", root.join(".codex/settings.json")),
        (
            ".codex/settings.local.json",
            root.join(".codex/settings.local.json"),
        ),
    ] {
        if let Ok(text) = fs::read_to_string(path)
            && serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|value| value.get("hooks").cloned())
                .is_some()
        {
            source.push(format!("{label} contains hooks"));
        }
    }
    let config = root.join(".codex/config.toml");
    if let Ok(text) = fs::read_to_string(config) {
        match hooks_table(&text) {
            Ok(true) => source.push(".codex/config.toml contains a hooks table".into()),
            Err(error) => source.push(format!(".codex/config.toml table header invalid ({error})")),
            _ => {}
        }
    }
    let hooks = root.join(".codex/hooks.json");
    if fs::read_to_string(hooks)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("hooks").cloned())
        .and_then(|value| value.as_object().cloned())
        .is_none()
    {
        source.push(".codex/hooks.json invalid JSON or has no hooks object".into());
    }
    if !source.is_empty() {
        failures.push(("single-hook-source", format!("Codex hook registration is not single-source: {}. FIX: merge project hooks into .codex/hooks.json and remove all other hook blocks.", source.join("; "))));
    }
    if failures.is_empty() {
        return StepResult::ok();
    }
    let mut stdout = format!(
        "[health-check] {} skeleton setting(s) need attention (check-only, nothing changed):\n",
        failures.len()
    );
    for (_, message) in &failures {
        stdout.push_str(&format!("  - {message}\n"));
    }
    let code = if strict
        && failures.iter().any(|(name, _)| {
            matches!(
                *name,
                "project-runtime" | "settings-json-valid" | "single-hook-source"
            )
        }) {
        2
    } else {
        0
    };
    StepResult {
        code,
        stdout,
        stderr: String::new(),
    }
}

pub fn githooks_path() -> StepResult {
    githooks_path_with_runner(&repo_root(), &bridgeforge_core::SystemProcessRunner)
}

pub fn githooks_path_with_runner(
    root: &Path,
    runner: &dyn bridgeforge_core::ProcessRunner,
) -> StepResult {
    if !root.join(".git").exists() || !root.join(".githooks/pre-commit").is_file() {
        return StepResult::ok();
    }
    let run = |args: &[&str]| -> Result<bridgeforge_core::ProcessOutput, String> {
        let mut request = bridgeforge_core::ProcessRequest::new("git", root);
        request.args = vec![
            "--no-optional-locks".into(),
            "-c".into(),
            format!(
                "safe.directory={}",
                root.to_string_lossy().replace('\\', "/")
            )
            .into(),
        ];
        request
            .args
            .extend(args.iter().map(std::ffi::OsString::from));
        request.timeout = Duration::from_secs(10);
        let result = runner.run(&request).map_err(|error| error.to_string())?;
        if result.timed_out {
            return Err("Git query timed out".into());
        }
        Ok(result)
    };
    let query = [
        "config",
        "--null",
        "--show-origin",
        "--path",
        "--get",
        "core.hooksPath",
    ];
    let current = match run(&query) {
        Ok(result) => result,
        Err(error) => return StepResult::failed("githooks", error),
    };
    if current.code == 0 {
        let fields: Vec<_> = current.stdout.split(|b| *b == 0).collect();
        if fields.len() != 3 || !fields[2].is_empty() {
            return StepResult::failed(
                "githooks",
                "invalid Git configuration response; no configuration changed",
            );
        }
        let value = match std::str::from_utf8(fields[1]) {
            Ok(value) if !value.is_empty() => value,
            _ => return StepResult::output("[githooks] Existing empty/non-UTF8 core.hooksPath preserved; inspect Git configuration.".into()),
        };
        if crate::util::normalize_path(root, value)
            == crate::util::normalize_path(root, ".githooks")
        {
            return StepResult::ok();
        }
        return StepResult::output(format!(
            "[githooks] Existing core.hooksPath preserved: {value} ({}); managed pre-commit is not selected by this setting.",
            String::from_utf8_lossy(fields[0])
        ));
    }
    if current.code != 1 || !current.stdout.is_empty() || !current.stderr.is_empty() {
        return StepResult::failed(
            "githooks",
            "Git configuration query failed; no configuration changed",
        );
    }
    let hooks = match run(&["rev-parse", "--git-path", "hooks"]) {
        Ok(result) if result.code == 0 => result,
        _ => {
            return StepResult::failed(
                "githooks",
                "cannot resolve default hooks directory; no configuration changed",
            );
        }
    };
    let hooks_path = match std::str::from_utf8(&hooks.stdout) {
        Ok(value) if !value.trim().is_empty() => crate::util::normalize_path(root, value.trim()),
        _ => return StepResult::failed("githooks", "invalid default hooks directory"),
    };
    match fs::read_dir(&hooks_path) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => return StepResult::failed("githooks", error),
                };
                // Preserve anything except Git's inert sample files, including links.
                if !entry.file_name().to_string_lossy().ends_with(".sample") {
                    return StepResult::output(format!(
                        "[githooks] Default Git hooks preserved at {}; automatic core.hooksPath initialization skipped.",
                        hooks_path.display()
                    ));
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return StepResult::failed("githooks", error),
    }
    // Do not overwrite a configuration introduced while inspecting the directory.
    match run(&query) {
        Ok(result) if result.code == 1 && result.stdout.is_empty() && result.stderr.is_empty() => {}
        _ => {
            return StepResult::failed(
                "githooks",
                "Git configuration changed during inspection; no configuration changed",
            );
        }
    }
    let result = match run(&["config", "--local", "core.hooksPath", ".githooks"]) {
        Ok(result) => result,
        Err(error) => return StepResult::failed("githooks", error),
    };
    if result.code == 0 {
        StepResult::output("[githooks] 已设 core.hooksPath=.githooks（提交前闸已生效）".into())
    } else {
        StepResult::failed("githooks", String::from_utf8_lossy(&result.stderr).trim())
    }
}

pub struct GitState {
    pub branch: String,
    pub head: String,
    pub ahead: String,
    pub changes: String,
    pub known: bool,
}

pub fn parse_git_state(text: Option<&str>) -> GitState {
    let mut state = GitState {
        branch: "unknown".into(),
        head: "unknown".into(),
        ahead: "unknown".into(),
        changes: String::new(),
        known: false,
    };
    let Some(text) = text else {
        return state;
    };
    let mut changes = Vec::new();
    let mut upstream = false;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("# branch.head ") {
            state.branch = value.into();
        } else if let Some(value) = line.strip_prefix("# branch.oid ") {
            state.head = value.into();
        } else if line.starts_with("# branch.upstream ") {
            upstream = true;
        } else if let Some(value) = line.strip_prefix("# branch.ab ") {
            state.ahead = value.into();
        } else if !line.starts_with("# ") && !line.is_empty() {
            changes.push(line);
        }
    }
    state.known = !matches!(state.branch.as_str(), "" | "unknown")
        && !matches!(state.head.as_str(), "" | "unknown");
    if state.known && !upstream {
        state.ahead = "no-upstream".into();
    }
    state.changes = changes.join("\n");
    state
}

fn git_state() -> GitState {
    let output = git(
        &["status".into(), "--porcelain=v2".into(), "--branch".into()],
        Duration::from_secs(5),
    );
    let text = output
        .filter(|value| value.status.success())
        .map(|value| String::from_utf8_lossy(&value.stdout).to_string());
    parse_git_state(text.as_deref())
}

fn archive_count(root: &Path) -> Result<usize, String> {
    bridgeforge_checks::archive_scan::scan(root).map(|candidates| candidates.len())
}

pub fn show_state() -> StepResult {
    let root = repo_root();
    let state = git_state();
    let dirty = if state.known {
        state.changes.lines().count().to_string()
    } else {
        "unknown".into()
    };
    let mut lines = vec![format!(
        "[session-start] branch={} | dirty={dirty} | ahead/behind={} | skeleton=v{}",
        state.branch,
        state.ahead,
        skeleton_version()
    )];
    match archive_count(&root) {
        Ok(count) if count > 0 => {
            lines.push(format!(
                "[archive] delivery / bugs 有 {count} 个候选可归档 — 输入 $archive-scan 查看"
            ));
        }
        Err(error) => lines.push(format!(
            "[archive] scan failed; candidates unknown: {error}"
        )),
        _ => {}
    }
    StepResult::output(lines.join("\n"))
}

fn normalize_hash(value: &str) -> Option<String> {
    let mut normalized = value.trim().to_lowercase();
    if let Some(stripped) = normalized.strip_prefix("sha256:") {
        normalized = stripped.to_string();
    }
    (normalized.len() == 64 && normalized.chars().all(|value| value.is_ascii_hexdigit()))
        .then_some(normalized)
}

pub fn skill_sync() -> StepResult {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    let platform = home.join(".codex");
    let shelf = platform.join("skills");
    let entry = shelf.join("bridgeforge-codex/SKILL.md");
    let ledger = platform.join("bridgeforge-codex-managed.json");
    if !entry.is_file() && !ledger.exists() {
        return StepResult::ok();
    }
    let warning = |detail: &str| {
        StepResult::output(format!(
            "[skill-sync] {detail}。请运行无参 $bridgeforge-codex 重新同步。"
        ))
    };
    if !ledger.is_file() {
        return warning("bridgeforge-codex 托管账本缺失");
    }
    let Ok(value) = fs::read_to_string(&ledger)
        .ok()
        .and_then(|text| {
            serde_json::from_str::<Value>(text.strip_prefix('\u{feff}').unwrap_or(&text)).ok()
        })
        .ok_or(())
    else {
        return warning("bridgeforge-codex 托管账本无法读取");
    };
    if value.get("schema_version").and_then(Value::as_i64) != Some(1)
        || value.get("platform").and_then(Value::as_str) != Some("codex")
    {
        return warning("bridgeforge-codex 托管账本版本或平台不匹配");
    }
    let Some(records) = value
        .get("records")
        .and_then(Value::as_object)
        .filter(|records| !records.is_empty())
    else {
        return warning("bridgeforge-codex 托管账本没有有效记录");
    };
    let mut stale = Vec::new();
    for (name, record) in records {
        if name.is_empty() || name.contains(['/', '\\']) || matches!(name.as_str(), "." | "..") {
            return warning("bridgeforge-codex 托管账本包含无效 skill 记录");
        }
        let Some(expected) = record
            .get("content_hash")
            .and_then(Value::as_str)
            .and_then(normalize_hash)
        else {
            return warning("bridgeforge-codex 托管账本包含无效 skill 记录");
        };
        let root = shelf.join(name);
        if !root.is_dir() || source_tree_hash(&root) != expected {
            stale.push(name.clone());
        }
    }
    if stale.is_empty() {
        StepResult::ok()
    } else {
        warning(&format!(
            "{} 个托管 skill 缺失或内容漂移（{}）",
            stale.len(),
            stale.join("、")
        ))
    }
}

pub fn build_identity() -> Value {
    let mut hasher = Sha256::new();
    hasher.update(include_bytes!(env!("BRIDGEFORGE_WORKSPACE_LOCK")));
    json!({"schema":1,"name":"bridgeforge-hook","status":"ok","platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"lock_sha256":format!("{:x}",hasher.finalize())})
}
