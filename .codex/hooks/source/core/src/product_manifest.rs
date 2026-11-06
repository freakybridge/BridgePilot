//! One factory source manifest, with independently consumed distribution scopes.
use serde_json::Value;
use std::{fs, path::Path};

/// Relative to the owning repository, not to the manifest's parent directory.
pub const FILE: &str = "source/forge/bridgeforge-manifest.json";
pub const LEGACY_FILE: &str = "bridgeforge-manifest.json";

/// Conservative write protection only; this is not a manifest lookup fallback.
pub fn is_source_checkout(root: &Path) -> bool {
    root.join(FILE).exists() || root.join(LEGACY_FILE).exists()
}

pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    let value = crate::json::parse_unique_json(bytes, FILE)?;
    let object = value.as_object().ok_or("product manifest must be an object")?;
    let sections: &[&str] = match value["schema_version"].as_u64() {
        Some(2) => &["user_instructions", "project_skeleton"],
        // Historical tools and fixtures still read their own v1 source manifests.
        Some(1) => &["user_instructions", "shared_skills", "project_skeleton"],
        _ => return Err("unknown Forge product manifest schema".into()),
    };
    if object.len() != sections.len() + 1 || sections.iter().any(|key| !value[*key].is_object()) {
        return Err("Forge product manifest sections do not match its schema".into());
    }
    Ok(value)
}

pub fn load(root: &Path) -> Result<Value, String> {
    if !root.join(FILE).exists() && root.join(LEGACY_FILE).exists() {
        return Err(format!("legacy root manifest is unsupported; prepare the current source checkout with {FILE}"));
    }
    parse(&fs::read(root.join(FILE)).map_err(|e| format!("{FILE}: {e}"))?)
}

pub fn section(root: &Path, name: &str) -> Result<Value, String> {
    load(root)?.get(name).cloned().filter(Value::is_object)
        .ok_or_else(|| format!("unknown product manifest section: {name}"))
}

pub fn project(root: &Path) -> Result<Value, String> {
    section(root, "project_skeleton")
}

pub fn project_bytes(root: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(&project(root)?).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/product_manifest_location.rs"]
mod tests;
