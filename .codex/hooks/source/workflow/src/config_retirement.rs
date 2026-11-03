//! Recognize exact retired configuration, without adopting project-owned data.
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(crate) const TARGETS: &[(&str, &str)] = &[
    ("retired:codex.settings", ".codex/settings.json"),
    (
        "retired:codex.project-hook-policy",
        ".codex/project-hook-policy.json",
    ),
    (
        "retired:codex.project-hook-migrations",
        ".codex/project-hook-migrations.json",
    ),
];

/// Be conservative about any current policy reference, including command and
/// environment inputs. The generated historical baseline is not current policy.
pub(crate) fn referenced_by_current_config(config: &Value, target: &str) -> bool {
    fn matches_path(input: &str, target: &str) -> bool {
        let normalized = input.replace('\\', "/");
        let input = normalized.trim_start_matches("./");
        input == target || (input.ends_with('/') && target.starts_with(input))
    }
    fn references(value: &Value, target: &str) -> bool {
        match value {
            Value::String(input) => matches_path(input, target),
            Value::Array(values) => values.iter().any(|value| references(value, target)),
            Value::Object(values) => values
                .iter()
                .any(|(key, value)| matches_path(key, target) || references(value, target)),
            _ => false,
        }
    }
    config.as_object().is_some_and(|values| {
        values
            .iter()
            .any(|(key, value)| key != "baseline" && references(value, target))
    })
}

pub(crate) fn known_payload(target: &str, bytes: &[u8]) -> bool {
    if target == ".codex/settings.json" {
        let normalized = String::from_utf8_lossy(bytes)
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        return format!("{:x}", Sha256::digest(normalized.as_bytes()))
            == "9fd894e278d8edbe7ac6169b579a5cb7bd19a15e2d7c568f5d97b02ca5bbb6b8";
    }
    let Ok(value) = bridgeforge_core::json::parse_unique_json(bytes, target) else {
        return false;
    };
    if target == ".codex/project-hook-policy.json" {
        return value
            == serde_json::json!({
                "schema": 1,
                "owner": "bridgeforge-project-hook-policy-v1",
                "migration_id": "project-hooks-independent-v1",
                "legacy_versions_through": "2.6.2",
                "runtime_compatible": true
            });
    }
    if target != ".codex/project-hook-migrations.json" {
        return false;
    }
    let Some(object) = value.as_object() else {
        return false;
    };
    let keys = [
        "schema",
        "owner",
        "migration_id",
        "registration",
        "state",
        "evidence",
    ];
    object.len() == keys.len()
        && keys.iter().all(|key| object.contains_key(*key))
        && value["schema"] == 1
        && value["owner"] == "bridgeforge-project-hook-migration-v1"
        && value["migration_id"] == "project-hooks-independent-v1"
        && matches!(value["state"].as_str(), Some("pending" | "not_applicable"))
        && value["evidence"] == Value::Null
        && value["registration"]
            .as_str()
            .and_then(|hash| hash.strip_prefix("sha256:"))
            .is_some_and(|hash| {
                hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
}
