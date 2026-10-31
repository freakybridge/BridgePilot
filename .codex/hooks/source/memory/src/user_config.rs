use super::ownership::{
    ExpectedGroup, ExpectedHooksState, MANAGED_ID_KEY, canonical_json_sha256, expected_groups,
    load_document, managed_document_healthy, merge_hooks_file, merge_managed_document,
};
use super::{
    Authorization, MemoryResult, MemorySyncError, atomic_write, record_native_memories_consent,
};
use serde_json::{Map, Value, json};
use std::fs;
use std::path::Path;

pub const HOOK_ID: &str = "bridgeforge-codex.native-memory-sync.v1";
pub const HOOK_EVENTS: &[&str] = &["SessionStart", "Stop", "SessionEnd"];

struct UserHooksLock {
    _file: fs::File,
}

impl UserHooksLock {
    fn acquire(codex_home: &Path) -> MemoryResult<Self> {
        let directory = codex_home.join(".bridgeforge-codex");
        fs::create_dir_all(&directory)?;
        let path = directory.join("user-hooks.lock");
        super::worker::try_lock_file(&path)?
            .map(|file| Self { _file: file })
            .ok_or_else(|| {
                MemorySyncError::new("another native memory user-hooks update is running")
            })
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn windows_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn windows_hook_matches(actual: &str, binary: &Path, home: &Path, event: &str) -> bool {
    actual
        .strip_prefix("$ErrorActionPreference = 'Stop'; & ")
        .and_then(|value| value.strip_suffix("; exit $LASTEXITCODE"))
        .is_some_and(|value| hook_command_matches(value, binary, home, event, powershell_quote))
}

fn ordinary_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    cfg!(windows)
        && bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

fn command_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if ordinary_windows_absolute_path(&text) {
        text.replace('\\', "/")
    } else {
        text.into_owned()
    }
}

fn quoted_path_matches(actual: &str, path: &Path, quote: fn(&str) -> String) -> bool {
    let canonical = command_path(path);
    let expected = quote(&canonical);
    actual == expected
        || (ordinary_windows_absolute_path(&canonical)
            // A trailing backslash escapes the closing Windows quote. Paths
            // containing literal quotes also cannot use separator aliases.
            && (!expected.starts_with('"')
                || (!canonical.contains('"') && !actual.ends_with("\\\"")))
            && actual.len() == expected.len()
            && actual
                .bytes()
                .zip(expected.bytes())
                .all(|(actual, expected)| {
                    actual == expected || (expected == b'/' && actual == b'\\')
                }))
}

fn hook_command_matches(
    actual: &str,
    binary: &Path,
    codex_home: &Path,
    event: &str,
    quote: fn(&str) -> String,
) -> bool {
    let binary_len = quote(&command_path(binary)).len();
    let Some(binary_part) = actual.get(..binary_len) else {
        return false;
    };
    let Some(rest) = actual.get(binary_len..) else {
        return false;
    };
    let middle = format!(" memory-sync hook-run --event {event} --codex-home ");
    let Some(home_part) = rest.strip_prefix(&middle) else {
        return false;
    };
    quoted_path_matches(binary_part, binary, quote)
        && quoted_path_matches(home_part, codex_home, quote)
}

pub fn expected_document(binary: &Path, codex_home: &Path) -> Value {
    let binary_text = command_path(binary);
    let home_text = command_path(codex_home);
    let mut hooks = Map::new();
    for event in HOOK_EVENTS {
        let mut handler = json!({
            "type": "command",
            "command": format!(
                "{} memory-sync hook-run --event {} --codex-home {}",
                shell_quote(&binary_text),
                event,
                shell_quote(&home_text),
            ),
            "commandWindows": format!(
                "$ErrorActionPreference = 'Stop'; & {} memory-sync hook-run --event {} --codex-home {}; exit $LASTEXITCODE",
                powershell_quote(&binary_text),
                event,
                powershell_quote(&home_text),
            ),
            MANAGED_ID_KEY: format!("{HOOK_ID}:{event}"),
        });
        if *event == "Stop" {
            handler["async"] = Value::Bool(true);
            handler["timeout"] = Value::Number(120.into());
        } else if *event == "SessionStart" {
            handler["timeout"] = Value::Number(120.into());
        } else {
            handler["timeout"] = Value::Number(3.into());
        }
        hooks.insert((*event).into(), json!([{"hooks": [handler]}]));
    }
    json!({"hooks": hooks})
}

fn expected_user_groups(
    binary: &Path,
    codex_home: &Path,
    existing: Option<&[u8]>,
) -> MemoryResult<Vec<ExpectedGroup>> {
    let mut expected = expected_groups(
        &expected_document(binary, codex_home),
        &format!("{HOOK_ID}:"),
    )?;
    let Some(payload) = existing else {
        return Ok(expected);
    };
    let document = load_document(payload, "native memory hooks")?;
    for spec in &mut expected {
        let handler = document["hooks"][&spec.event]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|group| group["hooks"].as_array())
            .flatten()
            .find(|handler| handler[MANAGED_ID_KEY].as_str() == Some(spec.id.as_str()));
        let Some(handler) = handler else {
            continue;
        };
        // Only the two quoted path slots may differ. The ownership checker still
        // checks every other field, command token, group, and managed identity.
        for (key, quote) in [
            ("command", shell_quote as fn(&str) -> String),
            ("commandWindows", windows_quote as fn(&str) -> String),
        ] {
            if let Some(command) = handler[key].as_str()
                && if key == "commandWindows" {
                    windows_hook_matches(command, binary, codex_home, &spec.event)
                } else {
                    hook_command_matches(command, binary, codex_home, &spec.event, quote)
                }
            {
                spec.handler[key] = Value::String(command.into());
            }
        }
        spec.handler_sha256 = canonical_json_sha256(&spec.handler)?;
        spec.group["hooks"][0] = spec.handler.clone();
    }
    Ok(expected)
}

fn managed_looking(handler: &Map<String, Value>) -> bool {
    handler
        .get(MANAGED_ID_KEY)
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with(&format!("{HOOK_ID}:")))
}

fn migrate_python_handlers(
    payload: &[u8],
    codex_home: &Path,
    binary: &Path,
) -> MemoryResult<Option<Vec<u8>>> {
    let Some(parent) = codex_home.parent() else {
        return Ok(None);
    };
    let scripts = parent.join(".bridgeforge-codex/scripts");
    let mut script = command_path(&scripts.join("codex_memory_sync.py"));
    let mut wrapper = command_path(&scripts.join("codex_memory_sync_hook.ps1"));
    if cfg!(windows) {
        script = script.replace('/', "\\");
        wrapper = wrapper.replace('/', "\\");
    }
    let current = expected_document(binary, codex_home);
    let mut document = load_document(payload, "native memory hooks")?;
    let mut changed = false;
    // Exact official Python handler from fc94635; never infer ownership from a
    // command substring or accept altered flags, timeout, matcher, or identity.
    for event in HOOK_EVENTS {
        let desired = &current["hooks"][event][0]["hooks"][0];
        let mut legacy = desired.clone();
        legacy["command"] = Value::String(format!(
            "root=\"$(git rev-parse --show-toplevel)\" && \"$root/.venv/Scripts/python.exe\" {} hook-run --event {event} --project-root \"$root\"",
            shell_quote(&script),
        ));
        legacy["commandWindows"] = Value::String(format!(
            "powershell.exe -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File {} {event}",
            windows_quote(&wrapper),
        ));
        let mut rust_legacy = desired.clone();
        rust_legacy["commandWindows"] = Value::String(format!(
            "{} memory-sync hook-run --event {event} --codex-home {}",
            windows_quote(&command_path(binary)),
            windows_quote(&command_path(codex_home)),
        ));
        if let Some(groups) = document
            .get_mut("hooks")
            .and_then(|hooks| hooks.get_mut(event))
            .and_then(Value::as_array_mut)
        {
            for group in groups {
                if let Some(handlers) = group["hooks"].as_array_mut() {
                    for handler in handlers {
                        let mut rust_alias = rust_legacy.clone();
                        for key in ["command", "commandWindows"] {
                            let quote = if key == "command" {
                                shell_quote
                            } else {
                                windows_quote
                            };
                            if let Some(command) = handler[key].as_str()
                                && hook_command_matches(command, binary, codex_home, event, quote)
                            {
                                rust_alias[key] = command.into();
                            }
                        }
                        if *handler == legacy || *handler == rust_alias {
                            *handler = desired.clone();
                            changed = true;
                        }
                    }
                }
            }
        }
    }
    if changed {
        Ok(Some(super::ownership::render_document(&document)?))
    } else {
        Ok(None)
    }
}

pub fn merge_user_hooks(codex_home: &Path, binary: &Path) -> MemoryResult<bool> {
    fs::create_dir_all(codex_home)?;
    let _lock = UserHooksLock::acquire(codex_home)?;
    let path = codex_home.join("hooks.json");
    let before = fs::read(&path).ok();
    let expected = expected_user_groups(binary, codex_home, before.as_deref())?;
    if let Some(payload) = before.as_deref()
        && let Some(migrated) = migrate_python_handlers(payload, codex_home, binary)?
    {
        let expected = expected_user_groups(binary, codex_home, Some(&migrated))?;
        let desired = merge_managed_document(
            Some(&migrated),
            &expected,
            &[&format!("{HOOK_ID}:")],
            "native memory hooks",
            Some(&managed_looking),
        )?;
        if fs::read(&path)? != payload {
            return Err(MemorySyncError::new(
                "user hooks changed during the locked CAS",
            ));
        }
        atomic_write(&path, &desired)?;
        return Ok(true);
    }
    if before.as_deref().is_some_and(|payload| {
        managed_document_healthy(
            payload,
            &expected,
            &[&format!("{HOOK_ID}:")],
            "native memory hooks",
            Some(&managed_looking),
        )
    }) {
        return Ok(false);
    }
    merge_hooks_file(
        &path,
        &expected,
        &[&format!("{HOOK_ID}:")],
        Some(&managed_looking),
        match before.as_deref() {
            Some(payload) => ExpectedHooksState::Exact(payload),
            None => ExpectedHooksState::Missing,
        },
    )
}

pub fn user_hooks_healthy(codex_home: &Path, binary: &Path) -> bool {
    let prefix = format!("{HOOK_ID}:");
    let Ok(payload) = fs::read(codex_home.join("hooks.json")) else {
        return false;
    };
    expected_user_groups(binary, codex_home, Some(&payload)).is_ok_and(|expected| {
        managed_document_healthy(
            &payload,
            &expected,
            &[&prefix],
            "native memory hooks",
            Some(&managed_looking),
        )
    })
}

fn parse_config(text: &str) -> MemoryResult<toml_edit::DocumentMut> {
    text.parse().map_err(|error| {
        MemorySyncError::new(format!(
            "invalid Codex config.toml; left unchanged: {error}"
        ))
    })
}

pub fn enable_memories(codex_home: &Path, confirmed: bool) -> MemoryResult<bool> {
    if !confirmed {
        return Err(MemorySyncError::new(
            "native memories remain unchanged without --confirmed-enable",
        ));
    }
    let path = codex_home.join("config.toml");
    let before = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let mut document = parse_config(&before)?;
    let mut changed = false;
    for (section, key) in [
        ("features", "memories"),
        ("memories", "generate_memories"),
        ("memories", "use_memories"),
    ] {
        let item = document
            .entry(section)
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        let table = item.as_table_like_mut().ok_or_else(|| {
            MemorySyncError::new(format!(
                "Codex config.toml {section} must be a table; left unchanged"
            ))
        })?;
        if table.get(key).and_then(toml_edit::Item::as_bool) == Some(true) {
            continue;
        }
        let mut value = toml_edit::value(true);
        if let Some(previous) = table.get(key).and_then(toml_edit::Item::as_value) {
            *value.as_value_mut().unwrap().decor_mut() = previous.decor().clone();
        }
        table.insert(key, value);
        changed = true;
    }
    if !changed {
        return Ok(false);
    }
    let after = document.to_string();
    parse_config(&after)?;
    fs::create_dir_all(codex_home)?;
    atomic_write(&path, after.as_bytes())?;
    Ok(true)
}

pub fn memories_enabled(codex_home: &Path) -> bool {
    let Ok(text) = fs::read_to_string(codex_home.join("config.toml")) else {
        return false;
    };
    let Ok(document) = parse_config(&text) else {
        return false;
    };
    [
        ("features", "memories"),
        ("memories", "generate_memories"),
        ("memories", "use_memories"),
    ]
    .iter()
    .all(|(section, key)| {
        document
            .get(section)
            .and_then(toml_edit::Item::as_table_like)
            .and_then(|table| table.get(key))
            .and_then(toml_edit::Item::as_bool)
            == Some(true)
    })
}

pub fn configure(
    codex_home: &Path,
    binary: &Path,
    remote: &str,
    confirmed_enable: bool,
) -> MemoryResult<Authorization> {
    let state_dir = codex_home.join(".bridgeforge-codex/native-memory-sync");
    let ledger = codex_home.join("bridgeforge-codex-managed.json");
    let targets = [
        codex_home.join("config.toml"),
        codex_home.join("hooks.json"),
        state_dir.join("remote.txt"),
        ledger.clone(),
    ];
    let snapshots = targets
        .iter()
        .map(|path| (path.clone(), fs::read(path).ok()))
        .collect::<Vec<_>>();
    let applied = (|| {
        enable_memories(codex_home, confirmed_enable)?;
        merge_user_hooks(codex_home, binary)?;
        fs::create_dir_all(&state_dir)?;
        atomic_write(
            &state_dir.join("remote.txt"),
            format!("{remote}\n").as_bytes(),
        )?;
        record_native_memories_consent(&ledger, "approved", true, Some(remote))?;
        super::native_memories_authorization(&ledger)?
            .ok_or_else(|| MemorySyncError::new("native memories authorization was not recorded"))
    })();
    if applied.is_ok() {
        return applied;
    }
    let original = applied.unwrap_err();
    let mut rollback = Vec::new();
    for (path, before) in snapshots.into_iter().rev() {
        let restored = match before {
            Some(payload) => atomic_write(&path, &payload),
            None if path.exists() => fs::remove_file(&path).map_err(MemorySyncError::from),
            None => Ok(()),
        };
        if let Err(error) = restored {
            rollback.push(format!("{}: {error}", path.display()));
        }
    }
    if rollback.is_empty() {
        Err(MemorySyncError::new(format!(
            "native memories setup rolled back: {original}"
        )))
    } else {
        Err(MemorySyncError::new(format!(
            "native memories setup failed: {original}; rollback incomplete: {}",
            rollback.join("; ")
        )))
    }
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_memory_user_config.rs"]
mod tests;
