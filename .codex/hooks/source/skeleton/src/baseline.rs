use crate::json::parse_unique_json;
use crate::managed_markdown::{
    projection_rows as table_rows, projection_section as heading_section,
};
use crate::{BaselineReport, BaselineState, ProcessRequest, ProcessRunner, SystemProcessRunner};
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Skeleton identity only: business and independently versioned packages may be 0.x.
pub const MIN_SUPPORTED_VERSION: &str = "8.0.0";

pub fn project_version(root: &Path) -> Result<Option<String>, String> {
    let mut version = None;
    for name in [".bridgeforge_codex_version", ".bridgeforge_version"] {
        let path = bridgeforge_core::managed_paths::safe_join(
            root,
            &format!(".codex/{name}"),
            "skeleton identity",
        )?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        if !metadata.is_file()
            || crate::persistence::is_link_or_reparse(&path).map_err(|e| e.to_string())?
        {
            return Err("version stamp must be a plain file".into());
        }
        if version.is_some() {
            return Err("multiple version stamps are not allowed".into());
        }
        let value = fs::read_to_string(&path)
            .map_err(|e| e.to_string())?
            .trim()
            .to_owned();
        require_supported_version(&value)?;
        if name == ".bridgeforge_version" {
            return Err(
                "legacy version stamp is unsupported; skeleton 8.0.0 or newer is required".into(),
            );
        }
        version = Some(value);
    }
    Ok(version)
}

fn require_supported_version(value: &str) -> Result<(), String> {
    if value.parse::<bridgeforge_core::SemVer>()? < MIN_SUPPORTED_VERSION.parse()? {
        return Err(format!(
            "skeleton {value} is unsupported; minimum supported version is {MIN_SUPPORTED_VERSION}"
        ));
    }
    Ok(())
}

/// Refuse known old identities before planning, locking or writing anything.
/// Unstamped new projects remain eligible for first-use initialization.
pub fn require_supported_project(root: &Path) -> Result<(), String> {
    let stamped = project_version(root)?;
    let path = bridgeforge_core::managed_paths::safe_join(
        root,
        ".codex/managed-skeleton.json",
        "installed skeleton identity",
    )?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if crate::persistence::is_link_or_reparse(&path).map_err(|e| e.to_string())? {
        return Err("managed skeleton identity must be a plain file".into());
    }
    // Supported projects can need repair of their installed contract. The
    // transaction still validates every replacement; this check only owns the
    // support floor, not the repair eligibility or deletion authorization.
    let contract = match parse_unique_json(&bytes, "installed skeleton identity") {
        Ok(value) => value,
        Err(_) if stamped.is_some() => return Ok(()),
        Err(error) => return Err(error),
    };
    if let Some(value) = contract["release_version"].as_str() {
        require_supported_version(value)?;
    } else if stamped.is_none() {
        return Err("installed skeleton identity is unknown; refusing unstamped configuration without a valid release_version".into());
    }
    Ok(())
}

pub fn compatibility_baseline(contract: &Value) -> Result<bridgeforge_core::SemVer, String> {
    let floor = contract["compatibility_baseline"]
        .as_str()
        .ok_or("managed contract compatibility_baseline is missing")?
        .parse::<bridgeforge_core::SemVer>()?;
    let release = contract["release_version"]
        .as_str()
        .ok_or("managed contract release_version is missing")?
        .parse::<bridgeforge_core::SemVer>()?;
    if floor > release {
        return Err("compatibility_baseline is newer than the product release".into());
    }
    Ok(floor)
}

fn sha(payload: &[u8]) -> String {
    let normalized = if payload.contains(&0) {
        payload.to_vec()
    } else {
        String::from_utf8_lossy(payload)
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .into_bytes()
    };
    format!("sha256:{:x}", Sha256::digest(normalized))
}

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

fn marker_block(payload: &[u8], begin: &str, end: &str) -> Result<Vec<u8>, String> {
    let text = String::from_utf8(payload.to_vec())
        .map_err(|_| "managed text is not UTF-8")?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let starts = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_end_matches('\n') == begin)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let stops = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_end_matches('\n') == end)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if starts.len() != 1 || stops.len() != 1 || starts[0] >= stops[0] {
        return Err(format!(
            "managed markers must appear exactly once: {begin} / {end}"
        ));
    }
    Ok(lines[starts[0]..=stops[0]].concat().into_bytes())
}

fn exact_keys(value: &Value, expected: &[&str], label: &str) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{label} must be an object"))?;
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!("{label} fields are not schema exact"));
    }
    Ok(())
}

fn hash_string(value: &Value) -> bool {
    value.as_str().is_some_and(|value| {
        value.len() == 71
            && value.starts_with("sha256:")
            && value[7..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn safe_relative_string(value: &Value) -> bool {
    let Some(raw) = value.as_str() else {
        return false;
    };
    !raw.is_empty()
        && !raw.contains('\x5c')
        && !raw.contains(['*', '?', '['])
        && !Path::new(raw).is_absolute()
        && !Path::new(raw).components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

/// Derive the executable retirement list from traceable asset history.
pub fn retired_assets_from_history(history: &Value) -> Result<Value, String> {
    exact_keys(
        history,
        &["schema", "format", "relocations", "retirements"],
        "asset history",
    )?;
    if history["schema"] != 1 || history["format"] != "bridgeforge-asset-history-v1" {
        return Err("invalid asset history identity".into());
    }
    let id_pattern = Regex::new(r"^[a-z0-9][a-z0-9._-]*$").map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    let mut paths = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for (section, path_key) in [
        ("relocations", "previous_target"),
        ("retirements", "target"),
    ] {
        for row in history[section]
            .as_array()
            .ok_or("asset history entries must be arrays")?
        {
            let mut fields = vec!["id", path_key, "known_hashes"];
            if section == "relocations"
                && (row.get("source").is_some() || row.get("target").is_some())
            {
                fields.extend(["source", "target"]);
                if !safe_relative_string(&row["source"])
                    || !safe_relative_string(&row["target"])
                    || !sources.insert(row["source"].as_str().unwrap().to_owned())
                {
                    return Err("invalid or duplicate asset history source".into());
                }
            }
            exact_keys(row, &fields, "asset history entry")?;
            let target = row[path_key].as_str().ok_or("asset history path missing")?;
            if !row["id"].as_str().is_some_and(|id| id_pattern.is_match(id))
                || !safe_relative_string(&row[path_key])
                || !target.starts_with(".codex/hooks/")
                || !paths.insert(target.to_ascii_lowercase())
            {
                return Err("invalid or duplicate asset history identity".into());
            }
            let records = row["known_hashes"]
                .as_array()
                .filter(|r| !r.is_empty())
                .ok_or("asset history requires historical hashes")?;
            let mut hashes = Vec::new();
            for record in records {
                exact_keys(
                    record,
                    &["sha256", "revision", "source"],
                    "asset history provenance",
                )?;
                if !hash_string(&record["sha256"])
                    || !safe_relative_string(&record["source"])
                    || !record["revision"]
                        .as_str()
                        .is_some_and(|r| r.len() == 40 && r.bytes().all(|b| b.is_ascii_hexdigit()))
                {
                    return Err("invalid asset history provenance".into());
                }
                hashes.push(record["sha256"].clone());
            }
            if section == "retirements" || row["target"] != row["previous_target"] {
                result.push(
                    serde_json::json!({"id":row["id"],"target":target,"known_hashes":hashes}),
                );
            }
        }
    }
    Ok(Value::Array(result))
}

pub fn validate_contract(value: &Value) -> Result<(), String> {
    let mut keys = vec![
        "schema_version",
        "release_version",
        "host",
        "stamp",
        "contract_target",
        "assets",
        "baseline_model",
        "compatibility_baseline",
        "generated_assets",
    ];
    if value.get("release_model").is_some() {
        if value["release_model"] != "product-packages" {
            return Err("unsupported managed release_model".into());
        }
        keys.push("release_model");
    }
    if value.get("retired_assets").is_some() {
        keys.push("retired_assets");
    }
    if value.get("asset_history").is_some() {
        keys.push("asset_history");
    }
    if let Some(layout) = value.get("artifact_layout") {
        if layout != bridgeforge_core::artifact_paths::CONTRACT {
            return Err("unsupported managed artifact_layout".into());
        }
        keys.push("artifact_layout");
    }
    exact_keys(value, &keys, "current baseline top-level")?;
    if value["schema_version"].as_u64() != Some(4)
        || value["host"].as_str() != Some("codex")
        || value["baseline_model"].as_str() != Some("current-only")
        || value["contract_target"].as_str() != Some(".codex/managed-skeleton.json")
        || !safe_relative_string(&value["stamp"])
    {
        return Err("current baseline identity is invalid".into());
    }
    value["release_version"]
        .as_str()
        .ok_or("current baseline release_version is missing")?
        .parse::<bridgeforge_core::SemVer>()?;
    compatibility_baseline(value)?;
    let assets = value["assets"]
        .as_array()
        .filter(|assets| !assets.is_empty())
        .ok_or("current baseline has no assets")?;
    let allowed_asset = [
        "id",
        "source",
        "content",
        "target",
        "strategy",
        "current_sha256",
        "render",
        "agents_zones",
        "managed_blocks",
        "merge_policy",
        "merge_validation",
        "region",
    ];
    let id_pattern = Regex::new(r"^[a-z0-9][a-z0-9._-]*$").map_err(|error| error.to_string())?;
    if let Some(retired) = value.get("retired_assets") {
        let retired = retired
            .as_array()
            .ok_or("retired_assets must be an array")?;
        let mut retired_targets = BTreeSet::new();
        for item in retired {
            exact_keys(item, &["id", "target", "known_hashes"], "retired asset")?;
            let target = item["target"].as_str().ok_or("retired target missing")?;
            if !item["id"]
                .as_str()
                .is_some_and(|id| id_pattern.is_match(id))
                || !safe_relative_string(&item["target"])
                || !target.starts_with(".codex/hooks/")
                || target
                    .strip_prefix(".codex/hooks/")
                    .and_then(|p| p.split('/').next())
                    .is_some_and(|part| part.to_ascii_lowercase().starts_with("project_"))
                || assets.iter().any(|a| {
                    a["target"]
                        .as_str()
                        .is_some_and(|p| p.eq_ignore_ascii_case(target))
                })
                || !retired_targets.insert(target.to_lowercase())
            {
                return Err(format!("invalid retired source asset: {target}"));
            }
            let hashes = item["known_hashes"]
                .as_array()
                .filter(|h| !h.is_empty())
                .ok_or("retired asset requires verified hashes")?;
            if hashes.iter().any(|h| !hash_string(h))
                || hashes
                    .iter()
                    .map(Value::as_str)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != hashes.len()
            {
                return Err("retired asset hashes are invalid or duplicated".into());
            }
        }
    }
    let mut ids = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for asset in assets {
        let object = asset
            .as_object()
            .ok_or("current baseline contains a non-object asset")?;
        if object
            .keys()
            .any(|key| !allowed_asset.contains(&key.as_str()))
        {
            return Err("current baseline asset has non-schema fields".into());
        }
        let id = asset["id"].as_str().ok_or("asset id is missing")?;
        let target = asset["target"].as_str().ok_or("asset target is missing")?;
        if !id_pattern.is_match(id)
            || !ids.insert(id.to_string())
            || (crate::content::inline_source(asset)?.is_none()
                && !safe_relative_string(&asset["source"]))
            || !safe_relative_string(&asset["target"])
            || !targets.insert(target.to_lowercase())
            || !matches!(
                asset["strategy"].as_str(),
                Some("whole" | "merge" | "region" | "seed")
            )
            || !hash_string(&asset["current_sha256"])
        {
            return Err(format!("invalid or duplicate asset identity: {id}"));
        }
        if let Some(bytes) = crate::content::inline_source(asset)? {
            if asset["current_sha256"].as_str() != Some(sha(bytes).as_str()) {
                return Err(format!("inline asset content hash drifted: {id}"));
            }
        }
        if let Some(managed) = asset.get("managed_blocks") {
            if !managed.is_object()
                || !managed["headings"].is_array()
                || !managed["keyed_tables"].is_array()
                || !hash_string(&managed["current_projection_sha256"])
            {
                return Err(format!("asset {id} Markdown ownership is invalid"));
            }
        }
        if let Some(zones) = asset.get("agents_zones") {
            exact_keys(
                zones,
                &["format", "public", "project"],
                &format!("asset {id} AGENTS zones"),
            )?;
            exact_keys(
                &zones["public"],
                &["begin", "end", "current_sha256"],
                &format!("asset {id} AGENTS public zone"),
            )?;
            exact_keys(
                &zones["project"],
                &[
                    "begin",
                    "end",
                    "required_headings",
                    "required_content_headings",
                ],
                &format!("asset {id} AGENTS project zone"),
            )?;
            if zones["format"].as_str() != Some("bridgeforge-agents-zones")
                || !zones["public"]["begin"].is_string()
                || !zones["public"]["end"].is_string()
                || !hash_string(&zones["public"]["current_sha256"])
                || !zones["project"]["begin"].is_string()
                || !zones["project"]["end"].is_string()
                || !zones["project"]["required_headings"].is_array()
                || !zones["project"]["required_content_headings"].is_array()
            {
                return Err(format!("asset {id} AGENTS ownership is invalid"));
            }
        }
        if asset["strategy"].as_str() == Some("region") {
            let region = &asset["region"];
            if !region["begin"].is_string()
                || !region["end"].is_string()
                || !hash_string(&region["current_sha256"])
            {
                return Err(format!("asset {id} region ownership is invalid"));
            }
        }
        if asset["strategy"].as_str() == Some("merge") {
            let validation = &asset["merge_validation"];
            match asset["merge_policy"].as_str() {
                Some("artifact-roots")
                    if validation["format"].as_str()
                        == Some(bridgeforge_core::artifact_paths::CONTRACT) => {}
                Some("git-attributes-default-lf")
                    if validation["format"].as_str() == Some("git-attributes-default-lf-v1")
                        && validation["required"]
                            == serde_json::json!({"pattern": "*", "text": "auto", "eol": "lf"}) => {
                }
                Some("codex-hooks")
                    if validation["format"].as_str() == Some("codex-hooks-current-v1")
                        && validation["required_handlers"].is_array() => {}
                Some(_) | None
                    if validation["format"].as_str() == Some("json-subset-current-v1")
                        && validation["required"].is_object() => {}
                _ => return Err(format!("asset {id} merge ownership is invalid")),
            }
        }
    }
    if let Some(history) = value.get("asset_history") {
        if retired_assets_from_history(history)? != value["retired_assets"] {
            return Err("retired assets differ from asset history".into());
        }
    }
    let generated = value["generated_assets"]
        .as_array()
        .ok_or("current baseline generated assets are missing")?;
    let generated_fields = [
        "id",
        "source_root",
        "target_source_root",
        "manifest",
        "lockfile",
        "binary_targets",
        "receipt_target",
        "build",
        "self_test",
        "source_tree_sha256",
        "lockfile_sha256",
        "build_recipe_sha256",
        "self_test_sha256",
    ];
    for item in generated {
        let mut fields = generated_fields.to_vec();
        if let Some(mode) = item.get("source_projection") {
            if !matches!(
                mode.as_str(),
                Some(crate::source_projection::PUBLIC | crate::source_projection::FACTORY)
            ) {
                return Err("unsupported generated source projection".into());
            }
            fields.push("source_projection");
        }
        exact_keys(item, &fields, "generated asset")?;
        let id = item["id"].as_str().ok_or("generated asset id is missing")?;
        if !id_pattern.is_match(id)
            || !ids.insert(id.to_string())
            || !safe_relative_string(&item["source_root"])
            || !safe_relative_string(&item["target_source_root"])
            || !safe_relative_string(&item["receipt_target"])
            || !item["binary_targets"].is_object()
            || !hash_string(&item["source_tree_sha256"])
            || !hash_string(&item["lockfile_sha256"])
            || !hash_string(&item["build_recipe_sha256"])
            || !hash_string(&item["self_test_sha256"])
        {
            return Err(format!("generated asset is invalid: {id}"));
        }
    }
    Ok(())
}

fn load(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(path).map_err(|error| format!("cannot read current baseline: {error}"))?;
    let value = parse_unique_json(&bytes, "current baseline")?;
    validate_contract(&value)?;
    Ok(value)
}

struct TempDirectory(PathBuf);

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn isolated_git_request(cwd: &Path, args: &[&str]) -> ProcessRequest {
    let mut request = ProcessRequest::new("git", cwd);
    request.args = args.iter().map(OsString::from).collect();
    request.timeout = Duration::from_secs(30);
    request.env.insert("GIT_ATTR_NOSYSTEM".into(), "1".into());
    request.env_remove = std::env::vars_os()
        .filter_map(|(key, _)| {
            key.to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("GIT_")
                .then_some(key)
        })
        .collect();
    request
}

fn verify_gitattributes_read_only(payload: &[u8]) -> Result<(), String> {
    let text = std::str::from_utf8(payload).map_err(|_| ".gitattributes is not valid UTF-8")?;
    let mut found = false;
    for line in text.lines() {
        let line = line.trim_matches([' ', '\t', '\r']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
        if fields.len() != 3
            || fields[0] != "*"
            || fields[1..].iter().copied().collect::<BTreeSet<_>>()
                != BTreeSet::from(["text=auto", "eol=lf"])
        {
            return Err("zero-write attribute inspection cannot prove complex syntax; no Git fallback was executed".into());
        }
        found = true;
    }
    if !found {
        return Err(".gitattributes default LF policy is missing".into());
    }
    Ok(())
}

fn verify_gitattributes(payload: &[u8], project: Option<&Path>) -> Result<(), String> {
    std::str::from_utf8(payload).map_err(|_| ".gitattributes is not valid UTF-8".to_string())?;
    if verify_gitattributes_read_only(payload).is_ok() {
        return Ok(());
    }
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = if let Some(project) = project {
        bridgeforge_core::artifact_paths::temporary_build(project, "attributes")?
    } else {
        std::env::temp_dir().join(format!(
            "bridgeforge-gitattributes-{}-{token}",
            std::process::id()
        ))
    };
    if project.is_none() {
        fs::create_dir(&root)
            .map_err(|error| format!("cannot create attribute sandbox: {error}"))?;
    }
    let temporary = TempDirectory(root.clone());
    fs::write(root.join(".gitattributes"), payload)
        .map_err(|error| format!("cannot prepare attribute sandbox: {error}"))?;
    let global = root.join("global-attributes");
    fs::write(&global, b"").map_err(|error| error.to_string())?;
    let initialized = SystemProcessRunner
        .run(&isolated_git_request(&root, &["init", "-q"]))
        .map_err(|error| format!("cannot initialize attribute sandbox: {error}"))?;
    if initialized.code != 0 || initialized.timed_out {
        return Err(format!(
            "cannot initialize attribute sandbox: {}",
            String::from_utf8_lossy(&initialized.stderr).trim()
        ));
    }
    let probes = [
        "BRIDGEFORGE_DEFAULT_EOL_PROBE",
        "nested/BRIDGEFORGE_DEFAULT_EOL_PROBE",
        ".codex/BRIDGEFORGE_DEFAULT_EOL_PROBE.py",
        "doc/BRIDGEFORGE_DEFAULT_EOL_PROBE.md",
    ];
    let global_arg = format!("core.attributesFile={}", global.display());
    let mut args = vec!["-c", global_arg.as_str(), "check-attr", "text", "eol", "--"];
    args.extend(probes);
    let checked = SystemProcessRunner
        .run(&isolated_git_request(&root, &args))
        .map_err(|error| format!("cannot evaluate .gitattributes with Git: {error}"))?;
    drop(temporary);
    if checked.code != 0 || checked.timed_out {
        return Err(format!(
            "cannot evaluate .gitattributes with Git: {}",
            String::from_utf8_lossy(&checked.stderr).trim()
        ));
    }
    let mut states = probes
        .iter()
        .map(|probe| ((*probe).to_string(), BTreeMap::<String, String>::new()))
        .collect::<BTreeMap<_, _>>();
    for line in String::from_utf8_lossy(&checked.stdout).lines() {
        let fields = line.rsplitn(3, ": ").collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err("Git returned an invalid .gitattributes evaluation".into());
        }
        let (value, attribute, path) = (fields[0], fields[1], fields[2]);
        let state = states
            .get_mut(path)
            .ok_or("Git returned an unknown .gitattributes probe")?;
        state.insert(attribute.to_string(), value.to_string());
    }
    if states.values().all(|state| {
        state.get("text").map(String::as_str) == Some("auto")
            && state.get("eol").map(String::as_str) == Some("lf")
    }) {
        Ok(())
    } else {
        Err(".gitattributes default LF policy is missing or overridden".into())
    }
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        value => value.clone(),
    }
}

fn canonical_sha(value: &Value) -> Result<String, String> {
    serde_json::to_vec(&canonical_json(value))
        .map(|payload| raw_sha(&payload))
        .map_err(|error| error.to_string())
}

fn deep_subset(expected: &Value, actual: &Value, path: &str) -> Result<(), String> {
    if let Some(expected) = expected.as_object() {
        let actual = actual
            .as_object()
            .ok_or_else(|| format!("managed JSON value drifted: {path}"))?;
        for (key, value) in expected {
            let child = actual
                .get(key)
                .ok_or_else(|| format!("managed JSON key is missing: {path}.{key}"))?;
            deep_subset(value, child, &format!("{path}.{key}"))?;
        }
    } else if expected != actual {
        return Err(format!("managed JSON value drifted: {path}"));
    }
    Ok(())
}

pub fn markdown_projection(payload: &[u8], managed: &Value) -> Result<Value, String> {
    let mut headings = serde_json::Map::new();
    for heading in managed["headings"].as_array().into_iter().flatten() {
        let heading = heading.as_str().ok_or("managed heading is invalid")?;
        headings.insert(
            heading.to_string(),
            Value::String(sha(&heading_section(payload, heading)?)),
        );
    }
    let mut tables = serde_json::Map::new();
    for table in managed["keyed_tables"].as_array().into_iter().flatten() {
        let heading = table["heading"]
            .as_str()
            .ok_or("managed table heading is invalid")?;
        let rows = table_rows(&heading_section(payload, heading)?)?;
        let mut projected = serde_json::Map::new();
        for raw in table["managed_keys"].as_array().into_iter().flatten() {
            let key = raw
                .as_str()
                .ok_or("managed Markdown key is invalid")?
                .trim()
                .trim_matches('`')
                .to_lowercase();
            let row = rows
                .get(&key)
                .ok_or_else(|| format!("managed Markdown row is missing: {heading} :: {key}"))?;
            projected.insert(key, Value::String(sha(row)));
        }
        tables.insert(heading.to_string(), Value::Object(projected));
    }
    Ok(serde_json::json!({"headings": headings, "keyed_tables": tables}))
}

pub fn hook_handlers(document: &Value) -> Result<BTreeMap<String, Value>, String> {
    let hooks = document["hooks"]
        .as_object()
        .ok_or("hooks.json has no hooks object")?;
    let mut handlers = BTreeMap::new();
    for (event, groups) in hooks {
        let groups = groups
            .as_array()
            .ok_or_else(|| format!("hooks.json event is invalid: {event}"))?;
        for group in groups {
            let matcher = group["matcher"].as_str().unwrap_or("");
            let entries = group["hooks"]
                .as_array()
                .ok_or_else(|| format!("hooks.json group is invalid: {event}"))?;
            for handler in entries {
                let Some(id) = handler["bridgeforgeCodexId"].as_str() else {
                    continue;
                };
                let record =
                    serde_json::json!({"event": event, "matcher": matcher, "handler": handler});
                if handlers.insert(id.to_string(), record).is_some() {
                    return Err(format!("managed hook handler is duplicated: {id}"));
                }
            }
        }
    }
    Ok(handlers)
}

fn verify_hooks(payload: &[u8], asset: &Value) -> Result<(), String> {
    let actual = parse_unique_json(payload, "hooks.json")?;
    let required = asset["merge_validation"]["required_handlers"]
        .as_array()
        .ok_or("hooks required_handlers is missing")?;
    let handlers = hook_handlers(&actual)?;
    let expected = required
        .iter()
        .map(|record| {
            record["id"]
                .as_str()
                .map(str::to_string)
                .ok_or("managed hook id is missing")
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let actual_managed = handlers
        .keys()
        .filter(|id| id.starts_with("bridgeforge-codex.project-hook.v1:"))
        .cloned()
        .collect::<BTreeSet<_>>();
    if expected.len() != required.len() || actual_managed != expected {
        return Err("managed hook handler identity set drifted".into());
    }
    for required in required {
        let id = required["id"]
            .as_str()
            .ok_or("managed hook id is missing")?;
        let record = handlers
            .get(id)
            .ok_or_else(|| format!("managed hook is missing: {id}"))?;
        if record["event"] != required["event"] {
            return Err(format!("managed hook event drifted: {id}"));
        }
        if record["matcher"] != required["matcher"] {
            return Err(format!("managed hook matcher drifted: {id}"));
        }
        if canonical_sha(&record["handler"])? != required["sha256"].as_str().unwrap_or("") {
            return Err(format!("managed hook payload drifted: {id}"));
        }
    }
    if let Some(top) = asset["merge_validation"].get("managed_top_level") {
        deep_subset(top, &actual, "hooks.json")?;
    }
    Ok(())
}

pub fn verify_asset_payload(asset: &Value, payload: &[u8]) -> Result<String, String> {
    verify_asset_payload_mode(asset, payload, false, None)
}

fn verify_asset_payload_mode(
    asset: &Value,
    payload: &[u8],
    zero_write: bool,
    project: Option<&Path>,
) -> Result<String, String> {
    let id = asset["id"]
        .as_str()
        .ok_or("asset id is missing")?
        .to_string();
    let strategy = asset["strategy"]
        .as_str()
        .ok_or("asset strategy is missing")?;
    if let Some(blocks) = asset
        .get("managed_blocks")
        .filter(|value| value.is_object())
    {
        let actual = canonical_sha(&markdown_projection(payload, blocks)?)?;
        if actual != blocks["current_projection_sha256"].as_str().unwrap_or("") {
            return Err(format!("managed Markdown projection drifted: {id}"));
        }
        return Ok(id);
    }
    if strategy == "seed" {
        return Ok(format!("skip:{id}"));
    }
    if let Some(zones) = asset.get("agents_zones") {
        let public = &zones["public"];
        let block = marker_block(
            payload,
            public["begin"]
                .as_str()
                .ok_or("AGENTS public begin is missing")?,
            public["end"]
                .as_str()
                .ok_or("AGENTS public end is missing")?,
        )?;
        if sha(&block) != public["current_sha256"].as_str().unwrap_or("") {
            return Err(format!("managed AGENTS public zone drifted: {id}"));
        }
        let project = &zones["project"];
        marker_block(
            payload,
            project["begin"]
                .as_str()
                .ok_or("AGENTS project begin is missing")?,
            project["end"]
                .as_str()
                .ok_or("AGENTS project end is missing")?,
        )?;
    } else if strategy == "region" {
        let region = &asset["region"];
        let block = marker_block(
            payload,
            region["begin"].as_str().ok_or("region begin is missing")?,
            region["end"].as_str().ok_or("region end is missing")?,
        )?;
        if sha(&block) != region["current_sha256"].as_str().unwrap_or("") {
            return Err(format!("managed region drifted: {id}"));
        }
    } else if strategy == "merge" {
        match asset["merge_policy"].as_str() {
            Some("artifact-roots") => {
                let problems = bridgeforge_core::artifact_paths::ignore_problems(Some(payload))?;
                if !problems.is_empty() {
                    return Err(problems.join("; "));
                }
            }
            Some("git-attributes-default-lf") => {
                if zero_write {
                    verify_gitattributes_read_only(payload)?;
                } else {
                    verify_gitattributes(payload, project)?;
                }
            }
            Some("codex-hooks") => verify_hooks(payload, asset)?,
            _ => {
                let actual = parse_unique_json(payload, "managed JSON")?;
                deep_subset(
                    &asset["merge_validation"]["required"],
                    &actual,
                    asset["target"].as_str().unwrap_or("managed JSON"),
                )?;
            }
        }
    } else if sha(payload) != asset["current_sha256"].as_str().unwrap_or("") {
        return Err(format!("managed asset drifted: {id}: {}", asset["target"]));
    }
    Ok(id)
}

use bridgeforge_core::build_receipt::platform_key;
pub use bridgeforge_core::build_receipt::{verify_generated, verify_generated_payload};

pub fn verify(
    root: &Path,
    contract_path: Option<&Path>,
    verify_generated_runtime: bool,
) -> Result<BaselineReport, String> {
    verify_mode(root, contract_path, verify_generated_runtime, false)
}

/// Strict inspection never creates an attribute sandbox or launches Git.
/// Complex attribute syntax remains unproven instead of falling back to writes.
pub fn verify_read_only(
    root: &Path,
    contract_path: Option<&Path>,
    verify_generated_runtime: bool,
) -> Result<BaselineReport, String> {
    verify_mode(root, contract_path, verify_generated_runtime, true)
}

fn verify_mode(
    root: &Path,
    contract_path: Option<&Path>,
    verify_generated_runtime: bool,
    zero_write: bool,
) -> Result<BaselineReport, String> {
    let contract_path = contract_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(".codex/managed-skeleton.json"));
    let contract = load(&contract_path)?;
    let version = contract["release_version"].as_str().unwrap().to_string();
    let stamp = safe_target(root, &contract["stamp"], "current baseline stamp")?;
    let factory = root.join("bridgeforge-manifest.json").is_file();
    let stamp_version = if stamp.is_file() {
        fs::read_to_string(&stamp)
            .map_err(|error| error.to_string())?
            .trim()
            .to_string()
    } else if factory {
        fs::read_to_string(root.join("VERSION"))
            .map_err(|_| "current baseline version stamp is missing")?
            .trim()
            .to_string()
    } else {
        return Err("current baseline version stamp is missing".into());
    };
    if stamp_version != version {
        return Err(format!(
            "current baseline identity mismatch: contract={version}, stamp={stamp_version}"
        ));
    }
    verify_contract_assets(
        root,
        &contract_path,
        &contract,
        &version,
        verify_generated_runtime,
        None,
        zero_write,
    )
}

pub fn verify_prospective(
    root: &Path,
    contract_path: &Path,
    expected_version: &str,
    verify_generated_runtime: bool,
) -> Result<BaselineReport, String> {
    let contract = load(contract_path)?;
    let version = contract["release_version"]
        .as_str()
        .ok_or("current baseline release_version is missing")?;
    if version != expected_version {
        return Err(format!(
            "prospective baseline identity mismatch: contract={version}, expected={expected_version}"
        ));
    }
    verify_contract_assets(
        root,
        contract_path,
        &contract,
        version,
        verify_generated_runtime,
        None,
        false,
    )
}

/// Exact file bytes already subjected to semantic validation. Reuse is local
/// to this transaction, never persisted as a claim about mutable future files.
pub struct InstalledEvidence {
    files: BTreeMap<PathBuf, Option<Vec<u8>>>,
}

impl InstalledEvidence {
    pub fn include(&mut self, path: PathBuf, bytes: Option<Vec<u8>>) {
        self.files.insert(path, bytes);
    }
    pub fn verify_unchanged(&self, root: &Path) -> Result<(), String> {
        for (path, expected) in &self.files {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "verification evidence escaped project")?
                .to_string_lossy()
                .replace('\\', "/");
            let path = crate::managed_paths::safe_join(
                root,
                &relative,
                "installed verification evidence",
            )?;
            let actual = match fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.to_string()),
            };
            if &actual != expected {
                return Err(format!(
                    "installed asset changed after verification: {relative}"
                ));
            }
        }
        Ok(())
    }
}

pub fn verify_prospective_recorded(
    root: &Path,
    contract_path: &Path,
    version: &str,
) -> Result<InstalledEvidence, String> {
    let contract = load(contract_path)?;
    if contract["release_version"].as_str() != Some(version) {
        return Err("prospective version identity mismatch".into());
    }
    let mut evidence = InstalledEvidence {
        files: BTreeMap::new(),
    };
    verify_contract_assets(
        root,
        contract_path,
        &contract,
        version,
        true,
        Some(&mut evidence),
        false,
    )?;
    Ok(evidence)
}

fn verify_contract_assets(
    root: &Path,
    contract_path: &Path,
    contract: &Value,
    version: &str,
    verify_generated_runtime: bool,
    mut evidence: Option<&mut InstalledEvidence>,
    zero_write: bool,
) -> Result<BaselineReport, String> {
    let contract_bytes = fs::read(contract_path).map_err(|error| error.to_string())?;
    if &parse_unique_json(&contract_bytes, "verification contract")? != contract {
        return Err("contract changed during verification".into());
    }
    if let Some(e) = evidence.as_deref_mut() {
        e.include(contract_path.to_path_buf(), Some(contract_bytes.clone()));
    }
    let mut checked = Vec::new();
    let mut skipped = Vec::new();
    let factory = root.join("bridgeforge-manifest.json").is_file();
    for asset in contract["assets"].as_array().unwrap() {
        if factory
            && asset["id"].as_str() == Some("codex.doc.readme")
            && asset["managed_blocks"].is_object()
        {
            skipped.push("codex.doc.readme".into());
            continue;
        }
        let result = if asset["strategy"] == "seed" {
            format!("skip:{}", asset["id"].as_str().unwrap_or(""))
        } else {
            let target = safe_target(root, &asset["target"], "managed verification asset")?;
            let payload = fs::read(&target)
                .map_err(|e| format!("managed asset is missing: {}: {e}", asset["id"]))?;
            let checked = verify_asset_payload_mode(asset, &payload, zero_write, Some(root))?;
            if let Some(e) = evidence.as_deref_mut() {
                e.include(target, Some(payload));
            }
            checked
        };
        if let Some(id) = result.strip_prefix("skip:") {
            skipped.push(id.to_string())
        } else {
            checked.push(result)
        }
    }
    for generated in contract["generated_assets"].as_array().unwrap() {
        if verify_generated_runtime {
            let binary = safe_target(
                root,
                &generated["binary_targets"][platform_key()?],
                "generated binary",
            )?;
            let receipt = safe_target(root, &generated["receipt_target"], "generated receipt")?;
            let payload =
                fs::read(&binary).map_err(|e| format!("generated binary is missing: {e}"))?;
            let receipt_payload =
                fs::read(&receipt).map_err(|e| format!("generated receipt is missing: {e}"))?;
            checked.push(verify_generated_payload(
                generated,
                &payload,
                &receipt_payload,
            )?);
            if let Some(e) = evidence.as_deref_mut() {
                e.include(binary, Some(payload));
                e.include(receipt, Some(receipt_payload));
            }
        } else {
            skipped.push(generated["id"].as_str().unwrap_or("").to_string())
        }
    }
    let fingerprint = sha(format!(
        "{}\n{}\n{}",
        version,
        sha(&contract_bytes),
        checked.join("\n")
    )
    .as_bytes());
    Ok(BaselineReport {
        schema: 1,
        state: BaselineState::Clean,
        project_version: Some(version.to_string()),
        incoming_version: None,
        fingerprint: Some(fingerprint),
        reasons: Vec::new(),
        details: serde_json::json!({"checked": checked, "skipped": skipped}),
    })
}

fn git_index_blob(root: &Path, path: &str, runner: &dyn ProcessRunner) -> Result<Vec<u8>, String> {
    let mut request = ProcessRequest::new("git", root);
    request.args = vec![OsString::from("show"), OsString::from(format!(":{path}"))];
    request.timeout = std::time::Duration::from_secs(45);
    let output = runner
        .run(&request)
        .map_err(|error| format!("cannot launch git index reader: {error}"))?;
    if output.timed_out || output.code != 0 {
        return Err(format!(
            "managed asset is missing from staged index: {path}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

pub fn verify_index(root: &Path, runner: &dyn ProcessRunner) -> Result<BaselineReport, String> {
    let contract_bytes = git_index_blob(root, ".codex/managed-skeleton.json", runner)?;
    let contract = parse_unique_json(&contract_bytes, "staged current baseline")?;
    validate_contract(&contract)?;
    let version = contract["release_version"]
        .as_str()
        .ok_or("staged current baseline release_version is missing")?
        .to_string();
    let stamp_path = contract["stamp"]
        .as_str()
        .ok_or("staged stamp path is missing")?;
    let staged_identity = if root.join("bridgeforge-manifest.json").is_file() {
        "VERSION"
    } else {
        stamp_path
    };
    let stamp = String::from_utf8(git_index_blob(root, staged_identity, runner)?)
        .map_err(|_| "staged version stamp is not UTF-8")?;
    if stamp.trim() != version {
        return Err(format!(
            "staged current baseline identity mismatch: contract={version}, stamp={}",
            stamp.trim()
        ));
    }
    let mut checked = Vec::new();
    let mut skipped = Vec::new();
    let factory = root.join("bridgeforge-manifest.json").is_file();
    for asset in contract["assets"]
        .as_array()
        .ok_or("staged assets are missing")?
    {
        let id = asset["id"].as_str().ok_or("staged asset id is missing")?;
        if factory && id == "codex.doc.readme" && asset["managed_blocks"].is_object() {
            skipped.push(id.to_string());
            continue;
        }
        if asset["strategy"].as_str() == Some("seed") {
            skipped.push(id.to_string());
            continue;
        }
        let target = asset["target"]
            .as_str()
            .ok_or("staged asset target is missing")?;
        let payload = git_index_blob(root, target, runner)?;
        let result = verify_asset_payload_mode(asset, &payload, false, Some(root))?;
        if let Some(id) = result.strip_prefix("skip:") {
            skipped.push(id.to_string());
        } else {
            checked.push(result);
        }
    }
    if let Some(generated_assets) = contract["generated_assets"].as_array() {
        for generated in generated_assets {
            skipped.push(generated["id"].as_str().unwrap_or("").to_string());
        }
    }
    Ok(BaselineReport {
        schema: 1,
        state: BaselineState::Clean,
        project_version: Some(version.clone()),
        incoming_version: None,
        fingerprint: Some(sha(format!(
            "index\n{version}\n{}\n{}",
            sha(&contract_bytes),
            checked.join("\n")
        )
        .as_bytes())),
        reasons: Vec::new(),
        details: serde_json::json!({"source": "index", "checked": checked, "skipped": skipped}),
    })
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_baseline.rs"]
mod tests;
