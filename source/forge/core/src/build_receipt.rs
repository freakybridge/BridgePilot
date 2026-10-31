use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
fn raw_sha(payload: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(payload))
}

fn safe_target(root: &Path, value: &Value, label: &str) -> Result<PathBuf, String> {
    let raw = value
        .as_str()
        .ok_or_else(|| format!("{label} must be a string"))?;
    if raw.is_empty() || raw.contains('\\') || raw.contains(['*', '?', '[']) {
        return Err(format!("{label} is unsafe: {raw}"));
    }
    let relative = Path::new(raw);
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("{label} is unsafe: {raw}"));
    }
    Ok(root.join(relative))
}

pub fn platform_key() -> Result<&'static str, String> {
    if !cfg!(target_arch = "x86_64") {
        return Err("unsupported generated-asset architecture".into());
    }
    if cfg!(windows) {
        Ok("windows-x86_64")
    } else if cfg!(target_os = "linux") {
        Ok("linux-x86_64")
    } else if cfg!(target_os = "macos") {
        Ok("macos-x86_64")
    } else {
        Err("unsupported generated-asset operating system".into())
    }
}

pub fn verify_generated(root: &Path, generated: &Value) -> Result<String, String> {
    let id = generated["id"]
        .as_str()
        .ok_or("generated asset id is missing")?;
    let key = platform_key()?;
    let binary = safe_target(root, &generated["binary_targets"][key], "generated binary")?;
    let receipt = safe_target(root, &generated["receipt_target"], "generated receipt")?;
    let binary_payload =
        fs::read(&binary).map_err(|_| format!("generated binary is missing: {id}"))?;
    let receipt_payload =
        fs::read(&receipt).map_err(|_| format!("generated receipt is missing: {id}"))?;
    verify_generated_payload(generated, &binary_payload, &receipt_payload)
}

pub fn verify_generated_payload(
    generated: &Value,
    binary_payload: &[u8],
    receipt_payload: &[u8],
) -> Result<String, String> {
    let id = generated["id"]
        .as_str()
        .ok_or("generated asset id is missing")?;
    let key = platform_key()?;
    let document: Value = serde_json::from_slice(receipt_payload)
        .map_err(|error| format!("invalid generated receipt: {error}"))?;
    if document["schema_version"].as_u64() != Some(2)
        || document["generated_asset_id"].as_str() != Some(id)
        || document["platform"].as_str() != Some(key)
        || document["binary_sha256"].as_str() != Some(raw_sha(&binary_payload).as_str())
        || document["source_tree_sha256"] != generated["source_tree_sha256"]
        || document["lockfile_sha256"] != generated["lockfile_sha256"]
        || document["build_recipe_sha256"] != generated["build_recipe_sha256"]
        || document["self_test_sha256"] != generated["self_test_sha256"]
    {
        return Err(format!("generated asset receipt drifted: {id}"));
    }
    Ok(id.to_string())
}

pub fn verify_windows_gui(bytes: &[u8]) -> Result<(), String> {
    let offset = bytes
        .get(0x3c..0x40)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or("invalid project hook PE")? as usize;
    if bytes.get(..2) != Some(b"MZ")
        || bytes.get(offset..offset + 4) != Some(b"PE\0\0")
        || bytes.get(offset + 24 + 68..offset + 24 + 70) != Some(&[2, 0])
    {
        return Err("project Rust hook must use Windows GUI subsystem".into());
    }
    Ok(())
}
