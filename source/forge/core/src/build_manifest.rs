use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
use walkdir::WalkDir;
fn sha_raw(payload: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(payload))
}

fn sha_git(payload: &[u8]) -> String {
    if payload.contains(&0) {
        sha_raw(payload)
    } else {
        sha_raw(
            String::from_utf8_lossy(payload)
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .as_bytes(),
        )
    }
}

pub fn normalized_hash(payload: &[u8]) -> String {
    sha_git(payload)
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted = object
                .iter()
                .map(|(key, value)| (key.clone(), canonical(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect::<Map<_, _>>())
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        value => value.clone(),
    }
}

pub fn canonical_sha(value: &Value) -> Result<String, String> {
    let _profile = crate::release_metrics::span("serialization.canonical_hash");
    serde_json::to_vec(&canonical(value))
        .map(|payload| sha_raw(&payload))
        .map_err(|error| error.to_string())
}

pub fn generated_sources(source_root: &Path) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    for entry in WalkDir::new(source_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() != 1
                || (entry.file_name() != "target"
                    && ![".runtime", ".deps", ".build"]
                        .iter()
                        .any(|p| entry.file_name() == *p)
                    && !entry.file_name().to_string_lossy().starts_with("project_"))
        })
    {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_symlink() {
            return Err(format!(
                "generated source tree contains a link: {}",
                entry.path().display()
            ));
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source_root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "target" || relative.starts_with("target/") {
            continue;
        }
        result.push(relative);
    }
    result.sort();
    if !result.iter().any(|path| path == "Cargo.toml")
        || !result.iter().any(|path| path == "Cargo.lock")
    {
        return Err("managed source workspace must contain Cargo.toml and Cargo.lock".into());
    }
    Ok(result)
}

pub fn generated_source_sha(root: &Path, sources: &[String]) -> Result<String, String> {
    let mut records = Vec::new();
    for relative in sources.iter().filter(|path| path.as_str() != "Cargo.lock") {
        let payload = fs::read(root.join(relative)).map_err(|error| error.to_string())?;
        records.push(json!({"path": relative, "sha256": sha_git(&payload)}));
    }
    canonical_sha(&Value::Array(records))
}

pub fn generated_build_recipe(binary: &str) -> Value {
    json!({
        "tool": "cargo",
        "args": ["build", "--locked", "--profile", "release", "--manifest-path", "{manifest}", "--target-dir", "{target_dir}", "--bin", "{binary_name}"],
        "profile": "release",
        "binary_name": binary,
    })
}
