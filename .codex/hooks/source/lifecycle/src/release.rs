use chrono::Local;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::{ProcessRequest, ProcessRunner};

mod config;
#[path = "release_history.rs"]
mod history;
#[path = "release_history_cache.rs"]
mod history_cache;

pub fn planning_history_state(root: &Path, runner: &dyn ProcessRunner) -> Result<String, String> {
    history_cache::state(root, runner).map(|(state, _)| state)
}
mod independent;
mod lifecycle;
mod model;
mod polyglot;
mod python;
mod python_dependencies;
mod python_lock;
mod python_metadata;
#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_release_python_native.rs"]
mod python_native_tests;
mod rust;
pub mod schema4;
pub mod setup;
pub use history::{explicit_release_policy, validate_release_baseline};
pub use model::{PackageChange, PackageEvent};

pub fn version_strategy(root: &Path) -> Result<&'static str, String> {
    if let Some(cfg) = schema4::load(root)? {
        return Ok(if cfg.strategy == "synchronized" {
            "synchronized"
        } else {
            "independent"
        });
    }
    Ok(if polyglot::load_config(root)?.is_some() {
        "independent"
    } else if config::load(root)?.is_some() {
        "rust_independent"
    } else {
        "synchronized"
    })
}

pub use bridgeforge_core::SemVer;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitInfo {
    pub kind: String,
    pub description: String,
    pub section: String,
    pub breaking: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseKind {
    None,
    SkeletonOnly,
    Business,
    Factory,
}

#[derive(Clone, Debug)]
pub struct FileReleasePlan {
    pub inputs: BTreeMap<PathBuf, Option<Vec<u8>>>,
    pub build_directories: schema4::BuildDirectoryGuard,
    pub old_version: SemVer,
    pub new_version: SemVer,
    pub kind: ReleaseKind,
    pub writes: BTreeMap<PathBuf, Vec<u8>>,
    pub package_changes: Vec<PackageChange>,
    pub package_events: Vec<PackageEvent>,
    pub lifecycle_events: Vec<Value>,
    pub release_inventory: Option<Vec<String>>,
    pub release_status: Option<String>,
    pub details: Option<Value>,
    pub environment: Option<schema4::EnvironmentGuard>,
}

pub fn parse_commit_message(message: &str) -> Result<CommitInfo, String> {
    let header = message.replace("\r\n", "\n");
    let first = header.lines().next().unwrap_or("").trim();
    let expression =
        Regex::new(r"^(feat|fix|docs|refactor|chore|perf)(?:\([^\)\r\n]+\))?(!)?:\s+(.+?)\s*$")
            .map_err(|error| error.to_string())?;
    let captures = expression.captures(first).ok_or(
        "commit message must use feat/fix/docs/refactor/chore/perf with Conventional Commits",
    )?;
    let kind = captures[1].to_string();
    let breaking = captures.get(2).is_some()
        || Regex::new(r"(?m)^BREAKING CHANGE:\s*\S")
            .map_err(|error| error.to_string())?
            .is_match(&header);
    let section = match kind.as_str() {
        "feat" => "Added",
        "fix" => "Fixed",
        _ => "Changed",
    };
    Ok(CommitInfo {
        kind,
        description: captures[3].to_string(),
        section: section.into(),
        breaking,
    })
}

fn safe_relative(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let relative = Path::new(raw);
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("version manifest escapes repository: {raw}"));
    }
    Ok(root.join(relative))
}

fn configured_manifests(root: &Path) -> Result<Vec<PathBuf>, String> {
    let config = root.join(".codex/bridgeforge-version.json");
    if config.is_file() {
        let document: Value =
            serde_json::from_slice(&fs::read(&config).map_err(|error| error.to_string())?)
                .map_err(|error| format!("invalid version sync config: {error}"))?;
        if document["schema_version"].as_u64() != Some(1) {
            return Err("version sync config must use schema_version=1".into());
        }
        if config::auto_manifests(&document)? {
            return Ok(["Cargo.toml", "package.json"]
                .iter()
                .map(|p| root.join(p))
                .filter(|p| p.is_file())
                .collect());
        }
        let items = document["manifests"]
            .as_array()
            .ok_or("version sync config manifests are missing")?;
        if items.is_empty() {
            return Err("version sync config manifests cannot be empty".into());
        }
        let mut result = Vec::new();
        for item in items {
            let raw = item
                .as_str()
                .ok_or("configured manifest path must be a string")?;
            let path = safe_relative(root, raw)?;
            if !path.is_file()
                || !matches!(
                    path.file_name().and_then(|value| value.to_str()),
                    Some("Cargo.toml" | "package.json")
                )
            {
                return Err(format!("unsupported or missing configured manifest: {raw}"));
            }
            result.push(path);
        }
        let unique = result.iter().collect::<BTreeSet<_>>();
        if unique.len() != result.len() {
            return Err("configured manifests contain duplicates".into());
        }
        return Ok(result);
    }
    Ok(["Cargo.toml", "package.json"]
        .iter()
        .map(|name| root.join(name))
        .filter(|path| path.is_file())
        .collect())
}

fn validate_native_versions(root: &Path, version: SemVer) -> Result<(), String> {
    for path in configured_manifests(root)? {
        release_input(&path)?;
        let cargo = path.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml");
        let current = if cargo {
            cargo_version(&path)?
        } else {
            json_version(&path)?
        }
        .ok_or_else(|| {
            format!(
                "configured manifest has no static version: {}",
                path.display()
            )
        })?;
        if current != version {
            return Err(format!(
                "native manifest disagrees with VERSION: {}",
                path.display()
            ));
        }
        if cargo {
            render_cargo(&path, version, version)?;
            let lock = path.with_file_name("Cargo.lock");
            if release_input(&lock)?.is_some() {
                render_cargo_lock(&lock, version, version)?;
            }
        } else {
            let lock = path.with_file_name("package-lock.json");
            if release_input(&lock)?.is_some() {
                render_package_lock(&lock, version, version)?;
            }
            for unsupported in ["pnpm-lock.yaml", "yarn.lock"] {
                if release_input(&path.with_file_name(unsupported))?.is_some() {
                    return Err(format!("unsupported JavaScript lock file: {unsupported}"));
                }
            }
        }
    }
    Ok(())
}

fn cargo_version(path: &Path) -> Result<Option<SemVer>, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    cargo_version_text(&text, &path.display().to_string())
}

fn cargo_version_text(text: &str, label: &str) -> Result<Option<SemVer>, String> {
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("{label}: invalid Cargo TOML: {e}"))?;
    let mut versions = Vec::new();
    for item in [
        document.get("package"),
        document.get("workspace").and_then(|w| w.get("package")),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(version) = item.get("version").and_then(toml_edit::Item::as_str) {
            versions.push(version.parse::<SemVer>()?);
        }
    }
    match versions.as_slice() {
        [] => Ok(None),
        [only] => Ok(Some(*only)),
        _ => Err(format!("ambiguous Cargo version fields in {label}")),
    }
}

fn json_version(path: &Path) -> Result<Option<SemVer>, String> {
    let payload = fs::read(path).map_err(|error| error.to_string())?;
    let document: Value = serde_json::from_slice(&payload)
        .map_err(|error| format!("invalid JSON {}: {error}", path.display()))?;
    document
        .get("version")
        .and_then(Value::as_str)
        .map(str::parse)
        .transpose()
}

fn render_cargo(path: &Path, old: SemVer, new: SemVer) -> Result<Vec<u8>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.to_string())?;
    let package = doc
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(toml_edit::Item::as_str);
    let workspace = doc
        .get("workspace")
        .and_then(|p| p.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(toml_edit::Item::as_str);
    let mapped = match (package, workspace) {
        (Some(version), None) => (false, version.to_string()),
        (None, Some(version)) => (true, version.to_string()),
        _ => {
            return Err(format!(
                "Cargo manifest has no unique version {old}: {}",
                path.display()
            ));
        }
    };
    if mapped.1.parse::<SemVer>()? != old {
        return Err(format!(
            "Cargo manifest version differs from product {old}: {}",
            path.display()
        ));
    }
    if mapped.0 {
        doc["workspace"]["package"]["version"] = toml_edit::value(new.to_string());
    } else {
        doc["package"]["version"] = toml_edit::value(new.to_string());
    }
    Ok(doc.to_string().into_bytes())
}

fn render_cargo_lock(path: &Path, old: SemVer, new: SemVer) -> Result<Vec<u8>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.to_string())?;
    let entries = doc
        .get_mut("package")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
        .ok_or("Cargo.lock local package table missing")?;
    let mut changed = 0;
    for entry in entries.iter_mut() {
        if entry.get("source").is_none()
            && entry.get("version").and_then(toml_edit::Item::as_str)
                == Some(old.to_string().as_str())
        {
            entry["version"] = toml_edit::value(new.to_string());
            changed += 1;
        }
    }
    if changed == 0 {
        return Err(format!(
            "Cargo.lock has no local package at version {old}: {}",
            path.display()
        ));
    }
    Ok(doc.to_string().into_bytes())
}

fn render_package_lock(path: &Path, old: SemVer, new: SemVer) -> Result<Vec<u8>, String> {
    let payload = fs::read(path).map_err(|error| error.to_string())?;
    let mut document: Value =
        serde_json::from_slice(&payload).map_err(|error| error.to_string())?;
    let old_text = old.to_string();
    if document["version"].as_str() != Some(old_text.as_str())
        || document["packages"][""]["version"].as_str() != Some(old_text.as_str())
    {
        return Err(format!(
            "package-lock root version is missing or inconsistent: {}",
            path.display()
        ));
    }
    document["version"] = Value::String(new.to_string());
    document["packages"][""]["version"] = Value::String(new.to_string());
    let mut encoded = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
    encoded.push(b'\n');
    Ok(encoded)
}

fn git_bytes(payload: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(payload)
        .replace("\r\n", "\n")
        .into_bytes()
}

fn digest(payload: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(payload))
}

fn marker_projection(payload: &[u8], begin: &str, end: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    let text = String::from_utf8(git_bytes(payload)).map_err(|_| "managed text is not UTF-8")?;
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let starts = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_end_matches('\n') == begin)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let ends = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_end_matches('\n') == end)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if starts.len() != 1 || ends.len() != 1 || starts[0] >= ends[0] {
        return Err(format!(
            "managed markers are missing or duplicated: {begin} / {end}"
        ));
    }
    let block = lines[starts[0]..=ends[0]].concat().into_bytes();
    let mut outside = Vec::new();
    outside.extend_from_slice(lines[..starts[0]].concat().as_bytes());
    outside.extend_from_slice(lines[ends[0] + 1..].concat().as_bytes());
    Ok((block, outside))
}

fn heading_section(payload: &[u8], heading: &str) -> Result<Vec<u8>, String> {
    let text =
        String::from_utf8(git_bytes(payload)).map_err(|_| "managed Markdown is not UTF-8")?;
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let heading_level = heading.chars().take_while(|value| *value == '#').count();
    let matches = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_end_matches('\n') == heading)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if matches.len() != 1 || heading_level == 0 {
        return Err(format!(
            "managed Markdown heading is missing or duplicated: {heading}"
        ));
    }
    let start = matches[0];
    let stop = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| {
            let trimmed = line.trim_start();
            let level = trimmed.chars().take_while(|value| *value == '#').count();
            level > 0 && level <= heading_level && trimmed.chars().nth(level) == Some(' ')
        })
        .map(|(index, _)| index)
        .unwrap_or(lines.len());
    Ok(lines[start..stop].concat().into_bytes())
}

fn table_rows(section: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let text = String::from_utf8_lossy(section);
    let lines = text
        .lines()
        .filter(|line| line.trim_start().starts_with('|'))
        .collect::<Vec<_>>();
    if lines.len() < 2 {
        return Err("managed Markdown table is missing".into());
    }
    let link = Regex::new(r"^\[`[^`]+`\]\(([^)]+)\)$").map_err(|error| error.to_string())?;
    let mut result = BTreeMap::new();
    for line in lines.into_iter().skip(2) {
        let cells = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        let Some(raw_key) = cells.first() else {
            continue;
        };
        let mut key = raw_key.to_string();
        if let Some(captures) = link.captures(&key) {
            key = captures[1].to_string();
        }
        key = key.trim_matches('`').to_lowercase();
        if result
            .insert(key.clone(), format!("{line}\n").into_bytes())
            .is_some()
        {
            return Err(format!("managed Markdown table key is duplicated: {key}"));
        }
    }
    Ok(result)
}

fn split_json_ownership(
    required: &Value,
    actual: &Value,
    path: &str,
) -> Result<(Value, Value), String> {
    let Some(required) = required.as_object() else {
        if required != actual {
            return Err(format!("managed JSON value drifted: {path}"));
        }
        return Ok((actual.clone(), Value::Null));
    };
    let actual = actual
        .as_object()
        .ok_or_else(|| format!("managed JSON value drifted: {path}"))?;
    let mut public = serde_json::Map::new();
    let mut project = actual
        .iter()
        .filter(|(key, _)| !required.contains_key(*key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<serde_json::Map<_, _>>();
    for (key, expected) in required {
        let value = actual
            .get(key)
            .ok_or_else(|| format!("managed JSON key is missing: {path}.{key}"))?;
        let (managed, extra) = split_json_ownership(expected, value, &format!("{path}.{key}"))?;
        public.insert(key.clone(), managed);
        if !matches!(&extra, Value::Null)
            && extra.as_object().is_none_or(|value| !value.is_empty())
            && extra.as_array().is_none_or(|value| !value.is_empty())
        {
            project.insert(key.clone(), extra);
        }
    }
    Ok((Value::Object(public), Value::Object(project)))
}

fn hooks_projection(document: &Value) -> Result<(Value, Value), String> {
    let object = document.as_object().ok_or("hooks.json must be an object")?;
    let hooks = object
        .get("hooks")
        .and_then(Value::as_object)
        .ok_or("hooks.json has no hooks object")?;
    let mut managed = Vec::new();
    let mut project = Vec::new();
    let mut seen = BTreeSet::new();
    for (event, entries) in hooks {
        for entry in entries
            .as_array()
            .ok_or_else(|| format!("hooks.json event is invalid: {event}"))?
        {
            let group = entry
                .as_object()
                .ok_or_else(|| format!("hooks.json group is invalid: {event}"))?;
            let matcher = group.get("matcher").and_then(Value::as_str).unwrap_or("");
            for handler in group
                .get("hooks")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("hooks.json group is invalid: {event}"))?
            {
                let handler_id = handler["bridgeforgeCodexId"].as_str();
                let record = json!({"event": event, "matcher": matcher, "handler": handler});
                if handler_id.is_some_and(|value| value.starts_with("bridgeforge-codex.")) {
                    let handler_id = handler_id.unwrap();
                    if !seen.insert(handler_id.to_string()) {
                        return Err(format!("managed hook handler is duplicated: {handler_id}"));
                    }
                    managed.push(record);
                } else {
                    project.push(record);
                }
            }
        }
    }
    let top_level = object
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "description" | "hooks"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<serde_json::Map<_, _>>();
    Ok((
        json!({"description": object.get("description"), "handlers": managed}),
        json!({"top_level": top_level, "handlers": project}),
    ))
}

fn ownership_projection(asset: &Value, payload: &[u8]) -> Result<(Value, Value), String> {
    if let Some(zones) = asset.get("agents_zones").and_then(Value::as_object) {
        let public = &zones["public"];
        let project = &zones["project"];
        let (public_block, without_public) = marker_projection(
            payload,
            public["begin"]
                .as_str()
                .ok_or("public begin marker is missing")?,
            public["end"]
                .as_str()
                .ok_or("public end marker is missing")?,
        )?;
        let (project_block, outside) = marker_projection(
            &without_public,
            project["begin"]
                .as_str()
                .ok_or("project begin marker is missing")?,
            project["end"]
                .as_str()
                .ok_or("project end marker is missing")?,
        )?;
        return Ok((
            json!(digest(&public_block)),
            json!({"project": digest(&project_block), "outside": digest(&outside)}),
        ));
    }
    if let Some(managed) = asset.get("managed_blocks").and_then(Value::as_object) {
        let mut headings = serde_json::Map::new();
        let mut tables = serde_json::Map::new();
        let mut project = git_bytes(payload);
        for heading in managed
            .get("headings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let heading = heading.as_str().ok_or("managed heading is invalid")?;
            let section = heading_section(payload, heading)?;
            headings.insert(heading.to_string(), json!(digest(&section)));
            if let Some(index) = project
                .windows(section.len())
                .position(|item| item == section)
            {
                project.drain(index..index + section.len());
            }
        }
        for table in managed
            .get("keyed_tables")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let heading = table["heading"]
                .as_str()
                .ok_or("managed table heading is invalid")?;
            let section = heading_section(payload, heading)?;
            let rows = table_rows(&section)?;
            let mut projected = serde_json::Map::new();
            for key in table["managed_keys"].as_array().into_iter().flatten() {
                let raw = key.as_str().ok_or("managed table key is invalid")?;
                let normalized = raw.trim().trim_matches('`').to_lowercase();
                let row = rows.get(&normalized).ok_or_else(|| {
                    format!("managed Markdown row is missing: {heading} :: {raw}")
                })?;
                projected.insert(normalized, json!(digest(row)));
                if let Some(index) = project.windows(row.len()).position(|item| item == row) {
                    project.drain(index..index + row.len());
                }
            }
            tables.insert(heading.to_string(), Value::Object(projected));
        }
        return Ok((
            json!({"headings": headings, "keyed_tables": tables}),
            json!(digest(&project)),
        ));
    }
    match asset["strategy"].as_str().unwrap_or("") {
        "region" => {
            let region = &asset["region"];
            let (managed, outside) = marker_projection(
                payload,
                region["begin"]
                    .as_str()
                    .ok_or("region begin marker is missing")?,
                region["end"]
                    .as_str()
                    .ok_or("region end marker is missing")?,
            )?;
            Ok((json!(digest(&managed)), json!(digest(&outside))))
        }
        "merge" if asset["merge_policy"].as_str() == Some("git-attributes-default-lf") => {
            let (required, project) =
                bridgeforge_skeleton::attributes::projection(asset, &git_bytes(payload))?;
            Ok((json!(required), json!(digest(&project))))
        }
        "merge" => {
            let document: Value = serde_json::from_slice(payload)
                .map_err(|error| format!("managed JSON is invalid: {error}"))?;
            if asset["merge_policy"].as_str() == Some("codex-hooks") {
                hooks_projection(&document)
            } else {
                let required = &asset["merge_validation"]["required"];
                split_json_ownership(required, &document, "managed JSON")
            }
        }
        "seed" => Ok((json!(null), json!(digest(&git_bytes(payload))))),
        "whole" => Ok((json!(digest(&git_bytes(payload))), json!(null))),
        strategy => Err(format!("unsupported ownership strategy: {strategy}")),
    }
}

fn verify_contract_payload(asset: &Value, payload: &[u8]) -> Result<(), String> {
    crate::baseline::verify_asset_payload(asset, payload).map(|_| ())
}

fn contract_assets(
    contract: &Value,
    label: &str,
) -> Result<(BTreeMap<String, Value>, BTreeMap<String, Value>), String> {
    let mut by_target = BTreeMap::new();
    let mut by_id = BTreeMap::new();
    for asset in contract["assets"]
        .as_array()
        .ok_or_else(|| format!("{label} contract assets are missing"))?
    {
        let id = asset["id"]
            .as_str()
            .ok_or_else(|| format!("{label} asset id is missing"))?;
        let target = asset["target"]
            .as_str()
            .ok_or_else(|| format!("{label} asset target is missing"))?
            .replace('\\', "/");
        if by_target.insert(target, asset.clone()).is_some()
            || by_id.insert(id.to_string(), asset.clone()).is_some()
        {
            return Err(format!(
                "{label} contract asset identity is duplicated: {id}"
            ));
        }
    }
    Ok((by_target, by_id))
}

#[cfg(test)]
fn head_payload(
    root: &Path,
    relative: &str,
    runner: &dyn ProcessRunner,
) -> Result<Option<Vec<u8>>, String> {
    revision_payload(root, "HEAD", relative, runner)
}

fn revision_payload(
    root: &Path,
    revision: &str,
    relative: &str,
    runner: &dyn ProcessRunner,
) -> Result<Option<Vec<u8>>, String> {
    let mut exists = ProcessRequest::new("git", root);
    exists.args = [
        "-c".into(),
        format!("safe.directory={}", root.display()).into(),
        "ls-tree".into(),
        "-z".into(),
        revision.into(),
        "--".into(),
        relative.into(),
    ]
    .to_vec();
    exists.timeout = Duration::from_secs(30);
    let listed = runner.run(&exists).map_err(|error| error.to_string())?;
    if listed.timed_out {
        return Err(format!("timed out while checking HEAD payload: {relative}"));
    }
    if listed.code != 0 {
        return Err(format!(
            "cannot check HEAD payload {relative}: {}",
            String::from_utf8_lossy(&listed.stderr).trim()
        ));
    }
    if listed.stdout.is_empty() {
        return Ok(None);
    }
    let mut request = ProcessRequest::new("git", root);
    request.args = [
        "-c".into(),
        format!("safe.directory={}", root.display()).into(),
        "show".into(),
        format!("{revision}:{relative}").into(),
    ]
    .to_vec();
    request.timeout = Duration::from_secs(30);
    let output = runner.run(&request).map_err(|error| error.to_string())?;
    if output.timed_out {
        return Err(format!("timed out while reading HEAD payload: {relative}"));
    }
    if output.code != 0 {
        return Err(format!(
            "cannot read HEAD payload {relative}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(Some(output.stdout))
}

#[cfg(test)]
fn classify(
    root: &Path,
    changed: &[String],
    runner: &dyn ProcessRunner,
) -> Result<ReleaseKind, String> {
    if root.join("bridgeforge-manifest.json").is_file() {
        return Ok(ReleaseKind::Factory);
    }
    let contract_path = root.join(".codex/managed-skeleton.json");
    let current_bytes = fs::read(&contract_path).map_err(|_| "current-only baseline is missing")?;
    classify_payloads(
        changed,
        &current_bytes,
        |path| head_payload(root, path, runner),
        |path| release_input(&root.join(path)),
    )
}

#[cfg(test)]
fn classify_payloads(
    changed: &[String],
    current_bytes: &[u8],
    before: impl Fn(&str) -> Result<Option<Vec<u8>>, String>,
    after: impl Fn(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<ReleaseKind, String> {
    PayloadClassifier::new(current_bytes, &before)?.classify(changed, before, after)
}

struct PayloadClassifier {
    current_by_target: BTreeMap<String, Value>,
    current_by_id: BTreeMap<String, Value>,
    head_by_target: BTreeMap<String, Value>,
    head_by_id: BTreeMap<String, Value>,
    transition_paths: BTreeSet<String>,
    contract_transition: bool,
    unverifiable_head: bool,
}

impl PayloadClassifier {
    fn new(
        current_bytes: &[u8],
        before: impl Fn(&str) -> Result<Option<Vec<u8>>, String>,
    ) -> Result<Self, String> {
        let current = crate::json::parse_unique_json(&current_bytes, "current-only baseline")?;
        let (current_by_target, current_by_id) = contract_assets(&current, "current")?;
        let contract_target = current["contract_target"]
            .as_str()
            .unwrap_or(".codex/managed-skeleton.json")
            .replace('\\', "/");
        let head_contract_bytes = before(&contract_target)?;
        let contract_transition = head_contract_bytes
            .as_deref()
            .is_none_or(|payload| git_bytes(payload) != git_bytes(&current_bytes));
        let (head, head_by_target, head_by_id, unverifiable_head) = match head_contract_bytes {
            Some(payload) if contract_transition => {
                match crate::json::parse_unique_json(&payload, "HEAD current-only baseline")
                    .and_then(|value| contract_assets(&value, "HEAD").map(|maps| (value, maps)))
                {
                    Ok((value, (by_target, by_id))) => (value, by_target, by_id, false),
                    Err(_) => (json!({}), BTreeMap::new(), BTreeMap::new(), true),
                }
            }
            Some(_) => (
                current.clone(),
                current_by_target.clone(),
                current_by_id.clone(),
                false,
            ),
            None => (json!({}), BTreeMap::new(), BTreeMap::new(), false),
        };
        let mut transition_paths = BTreeSet::from([contract_target]);
        for candidate in [current.get("stamp"), head.get("stamp")]
            .into_iter()
            .flatten()
        {
            if let Some(value) = candidate.as_str() {
                transition_paths.insert(value.replace('\\', "/"));
            }
        }
        Ok(Self {
            current_by_target,
            current_by_id,
            head_by_target,
            head_by_id,
            transition_paths,
            contract_transition,
            unverifiable_head,
        })
    }

    fn classify(
        &self,
        changed: &[String],
        before: impl Fn(&str) -> Result<Option<Vec<u8>>, String>,
        after: impl Fn(&str) -> Result<Option<Vec<u8>>, String>,
    ) -> Result<ReleaseKind, String> {
        let Self {
            current_by_target,
            current_by_id,
            head_by_target,
            head_by_id,
            transition_paths,
            contract_transition,
            unverifiable_head,
        } = self;
        let contract_transition = *contract_transition;
        let unverifiable_head = *unverifiable_head;
        let mut public_changed = unverifiable_head;
        let mut project_changed = unverifiable_head;
        let mut handled = BTreeSet::new();
        for relative in changed
            .iter()
            .filter(|path| !matches!(path.as_str(), "VERSION" | "CHANGELOG.md"))
        {
            if transition_paths.contains(relative) {
                public_changed = true;
                continue;
            }
            let mut current_asset = current_by_target.get(relative);
            let mut head_asset = head_by_target.get(relative);
            if head_asset.is_none() {
                if let Some(asset) = current_asset {
                    head_asset = asset["id"].as_str().and_then(|id| head_by_id.get(id));
                }
            }
            if current_asset.is_none() {
                if let Some(asset) = head_asset {
                    current_asset = asset["id"].as_str().and_then(|id| current_by_id.get(id));
                }
            }
            if let (Some(asset), None) = (current_asset, head_asset)
                && asset["merge_policy"].as_str() == Some("git-attributes-default-lf")
            {
                let target = asset["target"]
                    .as_str()
                    .ok_or("gitattributes target is missing")?;
                let current_payload = after(target)?.unwrap_or_default();
                verify_contract_payload(asset, &current_payload)
                    .map_err(|error| format!("current ownership baseline is invalid: {error}"))?;
                let before_payload = before(target)?.unwrap_or_default();
                let (_, old_project) = ownership_projection(asset, &before_payload)?;
                let (_, new_project) = ownership_projection(asset, &current_payload)?;
                public_changed = true;
                project_changed |= old_project != new_project;
                continue;
            }
            let (Some(current_asset), Some(head_asset)) = (current_asset, head_asset) else {
                if current_asset.is_none() && head_asset.is_none() {
                    project_changed = true;
                } else {
                    public_changed = true;
                    let asset = current_asset.or(head_asset).unwrap();
                    project_changed |= asset["strategy"].as_str() != Some("whole");
                }
                continue;
            };
            let id = current_asset["id"]
                .as_str()
                .ok_or("current asset id is missing")?;
            if head_asset["id"].as_str() != Some(id) {
                return Err(format!(
                    "HEAD and current ownership identities disagree for one target: {relative}"
                ));
            }
            if !handled.insert(id.to_string()) {
                continue;
            }
            let current_target = current_asset["target"].as_str().unwrap().replace('\\', "/");
            let head_target = head_asset["target"].as_str().unwrap().replace('\\', "/");
            let relocated = current_target != head_target;
            let current_payload = after(&current_target)?;
            if relocated && current_payload.is_none() {
                return Err(format!(
                    "current ownership baseline is invalid: relocated asset is missing: {current_target}"
                ));
            }
            let current_payload = current_payload.unwrap_or_default();
            let Some(before_payload) = before(&head_target)? else {
                public_changed = true;
                project_changed = true;
                continue;
            };
            if let Err(error) = verify_contract_payload(head_asset, &before_payload) {
                if contract_transition {
                    public_changed = true;
                    project_changed = true;
                    continue;
                }
                return Err(format!("HEAD ownership baseline is invalid: {error}"));
            }
            verify_contract_payload(current_asset, &current_payload)
                .map_err(|error| format!("current ownership baseline is invalid: {error}"))?;
            match (
                ownership_projection(head_asset, &before_payload),
                ownership_projection(current_asset, &current_payload),
            ) {
                (Ok((old_public, old_project)), Ok((new_public, new_project))) => {
                    let old_project = if head_asset["merge_policy"].as_str()
                        == Some("git-attributes-default-lf")
                        && current_asset["merge_policy"].as_str()
                            == Some("git-attributes-default-lf")
                    {
                        // Newly public rules may already exist as identical project rules.
                        ownership_projection(current_asset, &before_payload)?.1
                    } else {
                        old_project
                    };
                    // schema 4 classifies one changed path at a time. A verified
                    // asset relocation is a public change even with identical
                    // bytes; the contract path may not be in this invocation.
                    public_changed |= relocated || old_public != new_public;
                    project_changed |= old_project != new_project;
                    if relocated {
                        // Seed files belong to the project after initialization;
                        // moving one changes project ownership even if its bytes
                        // happen to match. Its payload is not contract-verified.
                        project_changed |= head_asset["strategy"].as_str() == Some("seed")
                            || current_asset["strategy"].as_str() == Some("seed");
                        // Do not let stable identity hide a retained old file or
                        // overwrite an unrelated file at the destination. Both
                        // physical paths must prove an unambiguous relocation.
                        project_changed |=
                            after(&head_target)?.is_some() || before(&current_target)?.is_some();
                    }
                }
                _ if contract_transition => {
                    public_changed = true;
                    project_changed = true;
                }
                (Err(error), _) | (_, Err(error)) => return Err(error),
            }
        }
        Ok(if project_changed || !public_changed {
            ReleaseKind::Business
        } else {
            ReleaseKind::SkeletonOnly
        })
    }
}

fn render_changelog(
    root: &Path,
    version: SemVer,
    infos: &[CommitInfo],
    kind: &ReleaseKind,
    changed: &[String],
) -> Result<Vec<u8>, String> {
    let path = root.join("CHANGELOG.md");
    let mut text = fs::read_to_string(&path).unwrap_or_else(|_| "# Changelog\n".into());
    let heading = format!("## [{version}]");
    if text.lines().any(|line| line.starts_with(&heading)) {
        return Err(format!("CHANGELOG already contains version {version}"));
    }
    let prefix = if *kind == ReleaseKind::Factory {
        let mut tags = Vec::new();
        if changed.iter().any(|path| {
            path.starts_with("templates/")
                || path.starts_with("skills/")
                || path.starts_with("source/")
        }) {
            tags.push("[product]");
        }
        if changed.iter().any(|path| {
            path.starts_with(".codex/")
                || path.starts_with("scripts/")
                || path.starts_with(".githooks/")
        }) {
            tags.push("[repo]");
        }
        if changed.iter().any(|path| {
            path.starts_with("doc/")
                || matches!(path.as_str(), "README.md" | "AGENTS.md" | "INSTALL.md")
        }) {
            tags.push("[meta]");
        }
        if tags.is_empty() {
            tags.push("[repo]");
        }
        format!("{} ", tags.concat())
    } else {
        String::new()
    };
    let mut entry = format!("## [{version}] - {}\n\n", Local::now().date_naive());
    for section in ["Added", "Fixed", "Changed"] {
        let matching = infos
            .iter()
            .filter(|info| info.section == section)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        entry.push_str(&format!("### {section}\n\n"));
        for info in matching {
            let breaking = if info.breaking { " **BREAKING:**" } else { "" };
            entry.push_str(&format!("- {prefix}{}{breaking}\n", info.description));
        }
        entry.push('\n');
    }
    let headings = Regex::new(r"(?m)^## \[").map_err(|error| error.to_string())?;
    let positions = headings
        .find_iter(&text)
        .map(|item| item.start())
        .collect::<Vec<_>>();
    let insert = if let Some(first) = positions.first() {
        if text[*first..].starts_with("## [Unreleased]") {
            positions.get(1).copied().unwrap_or(text.len())
        } else {
            *first
        }
    } else {
        text.len()
    };
    let suffix = text.split_off(insert);
    text = format!("{}\n\n{entry}{suffix}", text.trim_end());
    Ok(text.into_bytes())
}

pub fn verify_release_inventory(
    root: &Path,
    runner: &dyn ProcessRunner,
    expected: &[String],
) -> Result<(), String> {
    if independent::inventory(root, runner)? != expected {
        return Err("release package/file inventory changed concurrently; no apply".into());
    }
    Ok(())
}

pub fn verify_release_status(
    root: &Path,
    runner: &dyn ProcessRunner,
    expected: &str,
) -> Result<(), String> {
    if independent::status(root, runner)? != expected {
        return Err("release worktree status changed concurrently; no apply".into());
    }
    Ok(())
}

fn release_input(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(payload) => Ok(Some(payload)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "cannot snapshot release input {}: {error}",
            path.display()
        )),
    }
}

pub fn verify_release_inputs(inputs: &BTreeMap<PathBuf, Option<Vec<u8>>>) -> Result<(), String> {
    for (path, before) in inputs {
        if &release_input(path)? != before {
            return Err(format!(
                "release input changed concurrently: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_release.rs"]
mod tests;

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/support/release_plan.rs"]
mod test_plan;
#[cfg(all(test, bridgeforge_factory_tests))]
pub(crate) use test_plan::apply_file_release_plan;
