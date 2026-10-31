//! Project-owned release policy. Schema 1 deliberately retains its legacy parser.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndependentConfig {
    schema_version: u32,
    release_policy: String,
    strategy: String,
    rust: super::rust::RustConfig,
    #[serde(default)]
    git_sync_setup: Option<super::setup::Confirmation>,
}

pub(super) fn parse(payload: &[u8]) -> Result<Option<super::rust::RustConfig>, String> {
    let value = crate::json::parse_unique_json(payload, "version sync config")?;
    super::setup::confirmation(&value)?;
    match value["schema_version"].as_u64() {
        Some(1) => {
            auto_manifests(&value)?;
            Ok(None)
        }
        Some(2) => {
            let config: IndependentConfig = serde_json::from_value(value)
                .map_err(|e| format!("invalid independent release config: {e}"))?;
            if config.schema_version != 2
                || config.release_policy != "explicit_release"
                || config.strategy != "rust_independent"
            {
                return Err(
                    "schema 2 requires explicit_release and strategy=rust_independent".into(),
                );
            }
            let _ = config.git_sync_setup;
            config.rust.validate()?;
            Ok(Some(config.rust))
        }
        Some(3) => {
            super::polyglot::parse(payload)?;
            Ok(None)
        }
        Some(4) => {
            super::schema4::parse(payload)?;
            Ok(None)
        }
        _ => Err("version sync config must use schema_version=1 or 2".into()),
    }
}

pub(super) fn auto_manifests(value: &Value) -> Result<bool, String> {
    match value.get("manifest_policy") {
        None => Ok(false),
        Some(Value::String(policy)) if policy == "auto" && value.get("manifests").is_none() => {
            Ok(true)
        }
        _ => Err("manifest_policy must be auto without an explicit manifests list".into()),
    }
}

pub(super) fn load(root: &Path) -> Result<Option<super::rust::RustConfig>, String> {
    release_input(&root.join(".codex/bridgeforge-version.json"))?
        .map(|payload| parse(&payload))
        .transpose()
        .map(Option::flatten)
}

pub(super) fn read_current(root: &Path, relative: &str) -> Result<Option<Vec<u8>>, String> {
    if relative.is_empty()
        || relative.contains('\\')
        || relative.starts_with(".git/")
        || Path::new(relative)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("unsafe release input path: {relative}"));
    }
    let path = safe_relative(root, relative)?;
    for ancestor in path.ancestors().filter(|p| p.exists()) {
        if crate::persistence::is_link_or_reparse(ancestor).map_err(|e| e.to_string())? {
            return Err(format!("release input traverses a link: {relative}"));
        }
    }
    release_input(&path)
}
