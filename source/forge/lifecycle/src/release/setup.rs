//! Project-scoped, durable first-use choice. No stdin prompts and no Git writes in inspection.
use super::*;
use crate::CommandOutcome;

const CONFIG: &str = ".codex/bridgeforge-version.json";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Confirmation {
    schema: u32,
    confirmed: bool,
    selection: String,
}

pub(super) fn confirmation(value: &Value) -> Result<bool, String> {
    let Some(record) = value.get("git_sync_setup") else {
        return Ok(false);
    };
    let record: Confirmation = serde_json::from_value(record.clone())
        .map_err(|e| format!("invalid git_sync_setup: {e}"))?;
    if record.schema != 1
        || !record.confirmed
        || !matches!(
            record.selection.as_str(),
            "current" | "rust_independent" | "independent" | "synchronized"
        )
    {
        return Err(
            "git_sync_setup must contain schema=1, confirmed=true and a supported selection".into(),
        );
    }
    Ok(true)
}

fn read_config(root: &Path) -> Result<(Option<Vec<u8>>, Value), String> {
    crate::baseline::require_supported_project(root)?;
    let bytes = config::read_current(root, CONFIG)?;
    let value = if let Some(bytes) = &bytes {
        let value = crate::json::parse_unique_json(bytes, "version sync config")?;
        if value["schema_version"].as_u64() != Some(4) {
            return Err("legacy configuration migration is retired; skeleton 8.0.0 or newer requires current configuration".into());
        }
        config::parse(bytes)?;
        explicit_release_policy(root)?;
        value
    } else {
        json!({"schema_version":4,"release_policy":"explicit_release","strategy":"independent","adapters":{},"packages":[]})
    };
    Ok((bytes, value))
}

/// Fast path for a normal sync: durable consent is independent of product version.
pub fn is_confirmed(root: &Path) -> Result<bool, String> {
    let (_, value) = read_config(root)?;
    confirmation(&value)
}

pub fn status(root: &Path, runner: &dyn ProcessRunner) -> CommandOutcome {
    status_with(root, runner, false)
}
pub fn status_with(root: &Path, _runner: &dyn ProcessRunner, reconfigure: bool) -> CommandOutcome {
    let result = (|| {
        let (_, value) = read_config(root)?;
        let confirmed = confirmation(&value)?;
        let strategy = version_strategy(root)?;
        let mut report = json!({"schema":1,"status":if confirmed{"configured"}else{"configuration-required"},"configuration":CONFIG,"current_strategy":strategy,"release_model":"product-packages","migration_required":value["release_model"]!="product-packages"});
        if reconfigure || !confirmed {
            report["current_configuration"] = value;
            report["options"] = json!([{"id":"independent","label":"Product and changed Rust/Python packages","available":true}]);
        }
        Ok::<_, String>(report)
    })();
    match result {
        Ok(v) => CommandOutcome::with_receipt(v),
        Err(e) => CommandOutcome::blocked(format!("[git-sync-setup] {e}\n")),
    }
}

pub fn plan_configuration(root: &Path, runner: &dyn ProcessRunner, path: &str) -> CommandOutcome {
    schema4::configuration_plan(root, runner, Some(path), None)
}
pub fn apply_configuration(
    root: &Path,
    runner: &dyn ProcessRunner,
    path: &str,
    expected: &str,
    confirmed: bool,
) -> CommandOutcome {
    schema4::configuration_apply(root, runner, Some(path), None, expected, confirmed)
}
pub fn plan(root: &Path, runner: &dyn ProcessRunner, strategy: &str) -> CommandOutcome {
    schema4::configuration_plan(
        root,
        runner,
        None,
        if strategy == "current" || strategy.is_empty() {
            None
        } else {
            Some(strategy)
        },
    )
}
pub fn apply(
    root: &Path,
    runner: &dyn ProcessRunner,
    strategy: &str,
    expected: &str,
    confirmed: bool,
) -> CommandOutcome {
    schema4::configuration_apply(
        root,
        runner,
        None,
        if strategy == "current" || strategy.is_empty() {
            None
        } else {
            Some(strategy)
        },
        expected,
        confirmed,
    )
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../../scripts/tests/unit/core_release_setup.rs"]
mod tests;
