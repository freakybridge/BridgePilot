//! Read-only registry and legacy artifact identity used for preservation and cleanup.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const REGISTRY: &str = "bridgeforgeProjectHooks";
pub const REGISTRY_PATH: &str = ".codex/project-hooks.json";
const HANDLER_ID: &str = "bridgeforgeProjectHookId";

/// Read legacy registry inputs in memory only; callers never rewrite project registration.
pub fn with_registry(document: &Value, sidecar: Option<&[u8]>) -> Result<Value, String> {
    let mut combined = document.clone();
    if let Some(payload) = sidecar {
        let registry = crate::json::parse_unique_json(payload, REGISTRY_PATH)?;
        if let Some(legacy) = document.get(REGISTRY) {
            if legacy != &registry {
                return Err(
                    "conflicting legacy and standalone project Rust hook registries".into(),
                );
            }
        }
        combined
            .as_object_mut()
            .ok_or("hooks config must be an object")?
            .insert(REGISTRY.into(), registry);
    }
    hooks(&combined)?;
    Ok(combined)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    pub id: String,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub event: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub matcher: String,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    150
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    schema_version: u32,
    hooks: Vec<Hook>,
}

fn token(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && text.as_bytes()[0].is_ascii_lowercase()
}

pub fn hooks(document: &Value) -> Result<Vec<Hook>, String> {
    let Some(value) = document.get(REGISTRY) else {
        return Ok(Vec::new());
    };
    let registry: Registry = serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid project Rust hook registry: {error}"))?;
    if registry.schema_version != 1 || registry.hooks.len() > 32 {
        return Err("unsupported project Rust hook registry".into());
    }
    let mut ids = BTreeSet::new();
    for hook in &registry.hooks {
        if !token(&hook.id)
            || !ids.insert(&hook.id)
            || hook.events.is_empty()
            || hook.events.len() > 16
        {
            return Err("invalid or duplicate project Rust hook id/events".into());
        }
        let mut events = BTreeSet::new();
        for event in &hook.events {
            if !matches!(
                event.event.as_str(),
                "SessionStart" | "Stop" | "PreToolUse" | "PostToolUse" | "PostCompact"
            ) || !events.insert((&event.event, &event.matcher))
                || event.timeout == 0
                || event.timeout > 900
                || event.matcher.contains(['\r', '\n'])
                || event.args.iter().any(|arg| !token(arg))
                || event.args.len() > 16
            {
                return Err(format!("invalid project Rust hook event: {}", hook.id));
            }
        }
    }
    Ok(registry.hooks)
}

impl Hook {
    pub fn binary(&self) -> String {
        format!(
            ".build/bridgeforge/bin/project_{}{}",
            self.id,
            std::env::consts::EXE_SUFFIX
        )
    }
    pub fn receipt(&self) -> String {
        format!(
            ".build/bridgeforge/bin/build-receipt-project-{}.json",
            self.id
        )
    }
}

pub fn read(root: &Path, relative: &str) -> Result<Option<Vec<u8>>, String> {
    let target = root.join(relative);
    for path in target.ancestors() {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if crate::persistence::is_link_or_reparse(path).map_err(|e| e.to_string())? {
                    return Err(format!(
                        "project Rust hook path traverses a link: {relative}"
                    ));
                }
                if path == target && !metadata.is_file() {
                    return Err(format!(
                        "project Rust hook target is not a file: {relative}"
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        if path == root {
            break;
        }
    }
    match fs::read(target) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn render(document: &Value) -> Result<Value, String> {
    if document.get(REGISTRY).is_none() {
        if document["hooks"]
            .as_object()
            .into_iter()
            .flat_map(|o| o.values())
            .flat_map(|v| v.as_array().into_iter().flatten())
            .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
            .any(|h| h.get(HANDLER_ID).is_some())
        {
            return Err("project Rust hook handler has no registry".into());
        }
        return Ok(document.clone());
    }
    let entries = hooks(document)?;
    let mut result = document.clone();
    result
        .as_object_mut()
        .ok_or("hooks config must be an object")?
        .remove(REGISTRY);
    let Some(groups) = result.get_mut("hooks").and_then(Value::as_object_mut) else {
        if entries.is_empty() {
            return Ok(result);
        }
        return Err("project Rust hooks require hooks object".into());
    };
    // Only generated handlers bearing our separate namespace are replaced.
    for groups in groups.values_mut() {
        let groups = groups.as_array_mut().ok_or("hook event must be an array")?;
        for group in groups.iter_mut() {
            if let Some(handlers) = group["hooks"].as_array_mut() {
                handlers.retain(|handler| handler.get(HANDLER_ID).is_none());
            }
        }
        groups.retain(|group| {
            group["hooks"]
                .as_array()
                .is_none_or(|handlers| !handlers.is_empty())
        });
    }
    for hook in entries {
        for (index, event) in hook.events.iter().enumerate() {
            let args = if event.args.is_empty() {
                String::new()
            } else {
                format!(" {}", event.args.join(" "))
            };
            let group = json!({"matcher":event.matcher,"hooks":[{
                "type":"command", HANDLER_ID:format!("{}:{index}",hook.id),
                "command":format!(".build/bridgeforge/bin/project_{}{args}",hook.id),
                "commandWindows":format!(".build/bridgeforge/bin/project_{}.exe{args}",hook.id),
                "timeout":event.timeout
            }]});
            groups
                .entry(event.event.clone())
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or("hook event must be an array")?
                .push(group);
        }
    }
    Ok(result)
}

fn sha(payload: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(payload))
}

/// Exact legacy outputs from registered generated handlers. Independent,
/// unregistered programs remain project-owned and are never guessed from names.
pub fn legacy_artifacts(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    let directory = root.join(".codex/bin");
    if !directory.is_dir() {
        return Ok(BTreeMap::new());
    }
    // Names only determine whether identity inspection is needed; ownership
    // still requires matching registry, receipt, registration and exact bytes.
    let possible_private = fs::read_dir(&directory)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            name.starts_with("project_") || name.starts_with("build-receipt-project-")
        });
    if !possible_private {
        return Ok(BTreeMap::new());
    }
    let Some(config) = read(root, ".codex/hooks.json")? else {
        return Ok(BTreeMap::new());
    };
    let document = crate::json::parse_unique_json(&config, "legacy project hooks")?;
    let combined = with_registry(&document, read(root, REGISTRY_PATH)?.as_deref())?;
    let mut owned = BTreeMap::new();
    for hook in hooks(&combined)? {
        let binary = format!(
            ".codex/bin/project_{}{}",
            hook.id,
            std::env::consts::EXE_SUFFIX
        );
        let receipt = format!(".codex/bin/build-receipt-project-{}.json", hook.id);
        let payload = read(root, &binary)?;
        let record = read(root, &receipt)?;
        if payload.is_none() && record.is_none() {
            continue;
        }
        let (payload, record) = payload
            .zip(record)
            .ok_or("legacy project hook pair is incomplete; preserve it")?;
        let value = crate::json::parse_unique_json(&record, "legacy project hook receipt")?;
        let identity = value["input_sha256"]
            .as_str()
            .ok_or("legacy project hook identity missing")?;
        if !identity
            .strip_prefix("sha256:")
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            || value
                != json!({"schema_version":1,"id":hook.id,"input_sha256":identity,"platform":std::env::consts::OS,"binary_sha256":sha(&payload)})
        {
            return Err(format!(
                "legacy project hook ownership unknown: {}",
                hook.id
            ));
        }
        owned.insert(root.join(binary), payload);
        owned.insert(root.join(receipt), record);
    }
    if !owned.is_empty() {
        let migrated = render(&combined)?;
        let mut expected = migrated.clone();
        for groups in expected["hooks"]
            .as_object_mut()
            .ok_or("hook groups missing")?
            .values_mut()
        {
            for group in groups.as_array_mut().ok_or("invalid hook group")? {
                for handler in group["hooks"]
                    .as_array_mut()
                    .ok_or("invalid hook handlers")?
                {
                    if handler.get(HANDLER_ID).is_some() {
                        for key in ["command", "commandWindows"] {
                            if let Some(command) = handler[key].as_str() {
                                handler[key] = json!(command.replacen(
                                    ".build/bridgeforge/bin/",
                                    ".codex/bin/",
                                    1
                                ));
                            }
                        }
                    }
                }
            }
        }
        let mut original = document.clone();
        original
            .as_object_mut()
            .ok_or("invalid hook document")?
            .remove(REGISTRY);
        if expected != original && migrated != original {
            return Err("legacy project hook handlers have external changes; preserve them".into());
        }
    }
    Ok(owned)
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_project_hooks.rs"]
mod tests;
