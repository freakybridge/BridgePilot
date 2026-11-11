use crate::{generated_assets::generated_writes, managed_paths::safe_join};
use bridgeforge_skeleton::content;

use crate::{CommandOutcome, ProcessRunner};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[path = "source_retirement.rs"]
mod source_retirement;

pub use bridgeforge_core::project_lock::ProjectLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncMode {
    Auto,
    Init,
    Adopt,
    Update,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncAction {
    pub id: String,
    pub target: String,
    pub operation: String,
    pub risk: bool,
    pub before_sha256: Option<String>,
    pub after_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncPlan {
    #[serde(default)]
    pub build_metrics: crate::build_metrics::BuildMetrics,
    pub schema: u32,
    pub status: String,
    pub readiness: String,
    #[serde(default)]
    pub release_setup: Value,
    pub mode: SyncMode,
    pub previous_version: Option<String>,
    pub current_version: String,
    pub safe: Vec<SyncAction>,
    pub risk: Vec<SyncAction>,
    pub gaps: Vec<String>,
    pub blockers: Vec<String>,
    pub preservation_manifest: Value,
    pub confirmation_required: bool,
    pub aggregate_fingerprint: String,
    #[serde(default)]
    pub public_installation_required: bool,
    #[serde(skip)]
    project_root: PathBuf,
    #[serde(skip)]
    writes: BTreeMap<PathBuf, Vec<u8>>,
    #[serde(skip)]
    deletes: Vec<PathBuf>,
    #[serde(skip)]
    generated_source_fingerprints: BTreeMap<String, String>,
    #[serde(skip)]
    project_hook_reads: BTreeMap<String, Option<Vec<u8>>>,
    #[serde(skip)]
    public_actions: Vec<SyncAction>,
    #[serde(skip)]
    release_upgrade: Option<crate::release::schema4::ReleaseUpgrade>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncReceipt {
    #[serde(default)]
    pub build_metrics: crate::build_metrics::BuildMetrics,
    pub schema: u32,
    pub status: String,
    pub execution_status: String,
    pub mode: SyncMode,
    pub previous_version: Option<String>,
    pub current_version: String,
    pub aggregate_fingerprint: String,
    pub applied: Vec<SyncAction>,
    pub rollback_performed: bool,
    pub stamp_written_last: bool,
    pub project_readiness: String,
    #[serde(default)]
    pub release_setup: Value,
    pub preserved_asset_ids: Vec<String>,
}

fn sha_raw(payload: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(payload))
}

const LEGACY_RECEIPT: &str = ".codex/bin/build-receipt.json";
const ARTIFACT_LAYOUT_INPUT: &str = "artifact-layout:legacy-runtime";

pub fn legacy_artifact_retirements(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    // Independent project programs and receipts remain opaque, including those
    // in the old public-tool directory. No private registry proof is required.
    let mut project = BTreeMap::new();
    if let Some((path, bytes)) = legacy_receipt(root)? {
        project.insert(path, bytes);
    }
    let mut files =
        bridgeforge_core::artifact_migration::legacy_runtime_with_owned(root, &project)?;
    files.extend(bridgeforge_core::artifact_migration::legacy_build_outputs(
        root,
    )?);
    files.extend(bridgeforge_core::artifact_migration::legacy_test_outputs(
        root,
    )?);
    Ok(files)
}

/// Post-retirement invariant, shared with the factory's post-deletion transaction.
pub fn verify_legacy_artifact_retirement(root: &Path, ignore_retired: bool) -> Result<(), String> {
    if !legacy_artifact_retirements(root)?.is_empty() {
        return Err("legacy artifact contents appeared during migration".into());
    }
    if ignore_retired {
        let directory = root.join(".codex/bin");
        match fs::read_dir(&directory) {
            Ok(mut entries) => {
                if entries
                    .next()
                    .transpose()
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    return Err("project files appeared during ignore retirement; preserve the ignore policy".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

pub fn legacy_artifact_relocations(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    let mut writes = BTreeMap::new();
    for (old, bytes) in bridgeforge_core::artifact_migration::legacy_test_outputs(root)? {
        let target = bridgeforge_core::artifact_migration::relocated_test_output(root, &old)?;
        if transaction_file_state(&target)?.is_some_and(|present| present != bytes) {
            return Err(format!(
                "legacy test relocation conflict: {}",
                target.display()
            ));
        }
        writes.insert(target, bytes);
    }
    for (old, bytes) in bridgeforge_core::artifact_migration::legacy_build_outputs(root)? {
        let target = bridgeforge_core::artifact_migration::relocated_build_output(root, &old)?;
        if transaction_file_state(&target)?.is_some_and(|present| present != bytes) {
            return Err(format!(
                "legacy build relocation conflicts with existing output: {}",
                target.display()
            ));
        }
        writes.insert(target, bytes);
    }
    Ok(writes)
}

fn legacy_runtime_fingerprint(
    root: &Path,
    files: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<String, String> {
    let records = files.iter().map(|(path, bytes)| {
        Ok(json!({"path":path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/"),"sha256":sha_raw(bytes)}))
    }).collect::<Result<Vec<_>, String>>()?;
    crate::manifest::canonical_sha(&json!(records))
}
const RETIRED_SYNC_ROLE: &str = "mechanical-sync-worker";
const RETIRED_SYNC_TARGET: &str = ".codex/agents/mechanical-sync-worker.toml";
const RETIRED_SYNC_INPUT: &str = "retired:mechanical-sync-worker:inputs";
// Published payloads from 3645472, d3af932, and 0dea502; never trust a downstream manifest.
const RETIRED_SYNC_HASHES: &[&str] = &[
    "sha256:57315c89fc965f1fb13dfa611242658a17391a7ee379f7aee753a22deb989cc0",
    "sha256:3561c215ce3610421734e5173f911e38dc43657eee93272ec28ab520e6f8b871",
    "sha256:4c553960c7733ad76bed8bdb078709836df6a75560390e99f348d19918e80ecf",
];
const RETIRED_PROJECT_MAPS: &[(&str, &str)] = &[
    ("retired:project-map:find-doc", ".codex/find-doc.map.md"),
    ("retired:project-map:sync-docs", ".codex/sync-docs.map.md"),
];

fn legacy_receipt(root: &Path) -> Result<Option<(PathBuf, Vec<u8>)>, String> {
    let path = safe_join(root, LEGACY_RECEIPT, "legacy build receipt")?;
    for ancestor in path.ancestors().filter(|path| path.exists()) {
        if crate::persistence::is_link_or_reparse(ancestor).map_err(|error| error.to_string())? {
            return Err("legacy build receipt traverses a link; refusing to delete".into());
        }
    }
    let metadata = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file()
        || crate::persistence::is_link_or_reparse(&path).map_err(|error| error.to_string())?
    {
        return Err(
            "legacy build receipt is not a plain file; preserve it for manual inspection".into(),
        );
    }
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    let value = crate::json::parse_unique_json(&bytes, "legacy build receipt")
        .map_err(|_| "unknown legacy build receipt; refusing to delete")?;
    let valid_hash = |key: &str| {
        value[key].as_str().is_some_and(|hash| {
            hash.strip_prefix("sha256:").is_some_and(|raw| {
                raw.len() == 64 && raw.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        })
    };
    if value.as_object().is_none_or(|object| object.len() != 9)
        || value["schema_version"] != 1
        || value["generated_asset_id"] != "codex.hooks"
        || !matches!(
            value["platform"].as_str(),
            Some("windows-x86_64" | "linux-x86_64" | "macos-aarch64" | "macos-x86_64")
        )
        || !value["cargo_version"]
            .as_str()
            .is_some_and(|text| text.starts_with("cargo ") && text.len() > 6)
        || ![
            "source_tree_sha256",
            "cargo_lock_sha256",
            "build_recipe_sha256",
            "self_test_sha256",
            "binary_sha256",
        ]
        .iter()
        .all(|key| valid_hash(key))
    {
        return Err("unknown legacy build receipt; refusing to delete".into());
    }
    Ok(Some((path, bytes)))
}

fn plan_legacy_receipt_retirement(
    root: &Path,
    safe: &mut Vec<SyncAction>,
    deletes: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if let Some((path, bytes)) = legacy_receipt(root)? {
        safe.push(SyncAction {
            id: "retired:codex.hooks.build-receipt.v1".into(),
            target: LEGACY_RECEIPT.into(),
            operation: "delete".into(),
            risk: false,
            before_sha256: Some(sha_git(&bytes)),
            after_sha256: "sha256:deleted".into(),
        });
        deletes.push(path);
    }
    Ok(())
}

fn plan_project_map_retirement(
    root: &Path,
    safe: &mut Vec<SyncAction>,
    deletes: &mut Vec<PathBuf>,
) -> Result<(), String> {
    for (id, target) in RETIRED_PROJECT_MAPS {
        let path = safe_join(root, target, "retired project map")?;
        let Some(bytes) = transaction_file_state(&path)? else {
            continue;
        };
        safe.push(SyncAction {
            id: (*id).into(),
            target: (*target).into(),
            operation: "delete".into(),
            risk: false,
            before_sha256: Some(sha_git(&bytes)),
            after_sha256: "sha256:deleted".into(),
        });
        deletes.push(path);
    }
    Ok(())
}

fn plan_config_retirement(
    root: &Path,
    safe: &mut Vec<SyncAction>,
    deletes: &mut Vec<PathBuf>,
    reads: &mut BTreeMap<String, Option<Vec<u8>>>,
) -> Result<(), String> {
    let version_bytes = transaction_file_state(&root.join(".codex/bridgeforge-version.json"))?;
    reads.insert(".codex/bridgeforge-version.json".into(), version_bytes.clone());
    let version_config = version_bytes
        .map(|bytes| crate::json::parse_unique_json(&bytes, "current version configuration"))
        .transpose()?;
    for (id, target) in crate::config_retirement::TARGETS {
        let path = safe_join(root, target, "retired configuration")?;
        let before = transaction_file_state(&path)?;
        reads.insert((*target).into(), before.clone());
        let Some(bytes) = before else { continue; };
        if !crate::config_retirement::known_payload(target, &bytes) {
            return Err(format!(
                "retired configuration has unrecognized or project-modified content; preserved: {target}"
            ));
        }
        if version_config.as_ref().is_some_and(|config|
            crate::config_retirement::referenced_by_current_config(config, target)
        ) {
            return Err(format!(
                "retired configuration is still a current version input; update the configuration through the managed entry before retirement: {target}"
            ));
        }
        safe.push(SyncAction {
            id: (*id).into(),
            target: (*target).into(),
            operation: "delete".into(),
            risk: false,
            before_sha256: Some(sha_git(&bytes)),
            after_sha256: "sha256:deleted".into(),
        });
        if !deletes.contains(&path) { deletes.push(path); }
    }
    Ok(())
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

fn sync_role_input(path: &Path) -> Result<Option<(String, bool)>, String> {
    validate_transaction_path(path)?;
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(format!(
            "role input is not a plain file: {}",
            path.display()
        ));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut tail = Vec::new();
    let mut referenced = false;
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        tail.extend_from_slice(&buffer[..count]);
        referenced |= tail
            .windows(RETIRED_SYNC_ROLE.len())
            .any(|window| window == RETIRED_SYNC_ROLE.as_bytes());
        let keep = tail.len().saturating_sub(RETIRED_SYNC_ROLE.len() - 1);
        tail.drain(..keep);
    }
    Ok(Some((
        format!("sha256:{:x}", digest.finalize()),
        referenced,
    )))
}

fn sync_role_inputs(root: &Path) -> Result<BTreeMap<String, (String, bool)>, String> {
    let mut inputs = BTreeMap::new();
    for target in ["AGENTS.md", ".codex/config.toml", RETIRED_SYNC_TARGET] {
        if let Some(input) = sync_role_input(&safe_join(root, target, "role input")?)? {
            inputs.insert(target.into(), input);
        }
    }
    for directory in [".codex/skills", ".codex/agents"] {
        let path = safe_join(root, directory, "role references")?;
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!(
                    "role reference directory is not a plain directory: {directory}"
                ));
            }
            Ok(_) => {}
        }
        for entry in WalkDir::new(&path).sort_by_file_name() {
            let entry = entry.map_err(|error| error.to_string())?;
            if crate::persistence::is_link_or_reparse(entry.path()).map_err(|e| e.to_string())? {
                return Err(format!(
                    "linked role input is unsafe: {}",
                    entry.path().display()
                ));
            }
            if entry.file_type().is_file() {
                let extension = entry
                    .path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !matches!(
                    extension.as_str(),
                    "md" | "toml"
                        | "json"
                        | "yaml"
                        | "yml"
                        | "txt"
                        | "rs"
                        | "py"
                        | "ps1"
                        | "sh"
                        | "cmd"
                        | "bat"
                        | "js"
                        | "ts"
                ) {
                    continue;
                }
                let target = relative_posix(root, entry.path())?;
                let input = sync_role_input(entry.path())?
                    .ok_or_else(|| format!("role input disappeared: {target}"))?;
                inputs.insert(target, input);
            }
        }
    }
    Ok(inputs)
}

fn sync_role_references(inputs: &BTreeMap<String, (String, bool)>) -> Vec<String> {
    inputs
        .iter()
        .filter_map(|(path, (_, referenced))| {
            (path != RETIRED_SYNC_TARGET && *referenced).then(|| path.clone())
        })
        .collect()
}

fn sync_role_fingerprint(inputs: &BTreeMap<String, (String, bool)>) -> Result<String, String> {
    Ok(sha_raw(
        &serde_json::to_vec(inputs).map_err(|e| e.to_string())?,
    ))
}

fn plan_sync_role_retirement(
    root: &Path,
    safe: &mut Vec<SyncAction>,
    deletes: &mut Vec<PathBuf>,
    gaps: &mut Vec<String>,
    fingerprints: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    let inputs = sync_role_inputs(root)?;
    fingerprints.insert(RETIRED_SYNC_INPUT.into(), sync_role_fingerprint(&inputs)?);
    let references = sync_role_references(&inputs);
    for path in &references {
        gaps.push(format!("retired role {RETIRED_SYNC_ROLE} is referenced by {path}; preserve and resolve before update"));
    }
    if !inputs.contains_key(RETIRED_SYNC_TARGET) {
        return Ok(());
    }
    let path = safe_join(root, RETIRED_SYNC_TARGET, "retired role")?;
    // Known templates are below 4 KiB. Larger custom files need no in-memory payload.
    let hash = if fs::metadata(&path).map_err(|e| e.to_string())?.len() <= 4096 {
        let bytes = transaction_file_state(&path)?.ok_or("retired role disappeared")?;
        if sha_raw(&bytes) != inputs[RETIRED_SYNC_TARGET].0 {
            return Err("retired role changed while planning".into());
        }
        sha_git(&bytes)
    } else {
        String::new()
    };
    if !RETIRED_SYNC_HASHES.contains(&hash.as_str()) {
        gaps.push(format!(
            "retired role has custom or unknown content: {RETIRED_SYNC_TARGET}; preserve for review"
        ));
    } else if references.is_empty() {
        safe.push(SyncAction {
            id: "retired:codex.agent.mechanical-sync-worker".into(),
            target: RETIRED_SYNC_TARGET.into(),
            operation: "delete".into(),
            risk: false,
            before_sha256: Some(hash),
            after_sha256: "sha256:deleted".into(),
        });
        deletes.push(safe_join(root, RETIRED_SYNC_TARGET, "retired role")?);
    }
    Ok(())
}

fn plan_fingerprint(
    mode: &SyncMode,
    version: &str,
    safe: &[SyncAction],
    risk: &[SyncAction],
    preservation_manifest: &Value,
    generated_source_fingerprints: &BTreeMap<String, String>,
) -> Result<String, String> {
    let mut material = format!("schema=1\nmode={mode:?}\nversion={version}\n");
    for (key, value) in generated_source_fingerprints
        .iter()
        .filter(|(key, _)| key.starts_with("prepared:") || key.starts_with("project-migration:"))
    {
        material.push_str(&format!("{key}={value}\n"));
    }
    for action in safe.iter().chain(risk.iter()) {
        material.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            action.id,
            action.target,
            action.before_sha256.as_deref().unwrap_or("missing"),
            generated_source_fingerprints
                .get(&action.id)
                .unwrap_or(&action.after_sha256)
        ));
    }
    material.push_str(
        &serde_json::to_string(preservation_manifest).map_err(|error| error.to_string())?,
    );
    if let Some(registry) = generated_source_fingerprints.get("project-hook:registry-input") {
        material.push_str(&format!("\nproject-hook:registry-input={registry}\n"));
    }
    for (key, value) in generated_source_fingerprints
        .iter()
        .filter(|(key, _)| key.starts_with("project-hook:input:"))
    {
        material.push_str(&format!("\n{key}={value}\n"));
    }
    if let Some(inputs) = generated_source_fingerprints.get(RETIRED_SYNC_INPUT) {
        material.push_str(&format!("\n{RETIRED_SYNC_INPUT}={inputs}\n"));
    }
    Ok(sha_raw(material.as_bytes()))
}

fn atomic_write(path: &Path, payload: &[u8]) -> Result<(), String> {
    crate::persistence::atomic_write(path, payload).map_err(|error| error.to_string())
}

fn render_asset(
    project_root: &Path,
    template_root: &Path,
    asset: &Value,
    allow_structure_upgrade: bool,
) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    let id = asset["id"].as_str().ok_or("asset id is missing")?;
    let target_raw = asset["target"]
        .as_str()
        .ok_or_else(|| format!("asset target is missing: {id}"))?;
    let target = safe_join(project_root, target_raw, "asset target")?;
    let source_payload = if let Some(bytes) = content::inline_source(asset)? {
        bytes.to_vec()
    } else {
        let source_raw = asset["source"]
            .as_str()
            .ok_or_else(|| format!("asset source is missing: {id}"))?;
        let source = safe_join(template_root, source_raw, "asset source")?;
        if matches!(
            asset["render"].as_str(),
            Some("public-runtime-workspace" | "public-runtime-lock")
        ) {
            crate::source_projection::render(template_root, true)?
                .remove(source_raw)
                .ok_or("projected asset source is missing")?
        } else {
            fs::read(&source)
                .map_err(|error| format!("cannot read asset source {source_raw}: {error}"))?
        }
    };
    let current = if target.is_file() {
        Some(fs::read(&target).map_err(|error| error.to_string())?)
    } else {
        None
    };
    let rendered = content::render(
        id,
        &source_payload,
        current.as_deref(),
        asset,
        project_root,
        allow_structure_upgrade,
    )?;
    Ok((rendered, current))
}

fn read_contract(template_root: &Path) -> Result<(Value, Vec<u8>), String> {
    let bytes = bridgeforge_core::product_manifest::project_bytes(template_root)?;
    let value = crate::json::parse_unique_json(&bytes, "managed contract")?;
    if value["schema_version"].as_u64() != Some(4)
        || value["baseline_model"].as_str() != Some("current-only")
    {
        return Err("managed contract must use schema 4 current-only".into());
    }
    crate::baseline::compatibility_baseline(&value)?;
    if value.get("retired_assets").is_some() {
        crate::baseline::validate_contract(&value)?;
    }
    Ok((value, bytes))
}

fn resolved_mode(project_root: &Path, requested: SyncMode) -> Result<SyncMode, String> {
    let has_stamp = crate::baseline::project_version(project_root)?.is_some();
    let has_assets =
        project_root.join(".codex").exists() || project_root.join("AGENTS.md").exists();
    match requested {
        SyncMode::Auto => Ok(if has_stamp {
            SyncMode::Update
        } else if has_assets {
            SyncMode::Adopt
        } else {
            SyncMode::Init
        }),
        SyncMode::Init if has_assets || has_stamp => {
            Err("init requires a project with no existing skeleton identity".into())
        }
        SyncMode::Update if !has_stamp => {
            Err("update requires one valid version stamp; use adopt for unstamped assets".into())
        }
        SyncMode::Adopt if has_stamp || !has_assets => {
            Err("adopt requires unstamped existing skeleton assets".into())
        }
        mode => Ok(mode),
    }
}

fn current_platform() -> &'static str {
    if cfg!(windows) {
        "windows-x86_64"
    } else if cfg!(target_os = "linux") {
        "linux-x86_64"
    } else {
        "macos-x86_64"
    }
}

fn relative_posix(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map_err(|_| format!("path escaped project root: {}", path.display()))
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn tree_sha(root: &Path) -> Result<String, String> {
    let mut material = Vec::new();
    for entry in WalkDir::new(root).sort_by_file_name() {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_symlink() {
            return Err(format!(
                "linked project asset is unsafe: {}",
                entry.path().display()
            ));
        }
        if entry.file_type().is_file() {
            let relative = relative_posix(root, entry.path())?;
            material.extend_from_slice(relative.as_bytes());
            material.push(0);
            material.extend_from_slice(&fs::read(entry.path()).map_err(|error| error.to_string())?);
            material.push(0xff);
        }
    }
    Ok(sha_raw(&material))
}

struct DestructiveInventory {
    entries: Vec<Value>,
    actions: Vec<SyncAction>,
    deletes: Vec<PathBuf>,
    blockers: Vec<String>,
    candidate_ids: BTreeSet<String>,
    deleted_hook_prefixes: Vec<String>,
}

fn remove_hook_registrations(payload: &[u8], prefixes: &[String]) -> Result<Vec<u8>, String> {
    let mut document: Value = serde_json::from_slice(payload)
        .map_err(|error| format!("project hooks target is invalid: {error}"))?;
    let hooks = document["hooks"]
        .as_object_mut()
        .ok_or("project hooks target has no hooks object")?;
    for groups in hooks.values_mut() {
        let groups = groups
            .as_array_mut()
            .ok_or("project hook event is not an array")?;
        for group in groups.iter_mut() {
            let handlers = group["hooks"]
                .as_array_mut()
                .ok_or("project hook group has no hooks array")?;
            handlers.retain(|handler| {
                !["command", "commandWindows"].iter().any(|key| {
                    handler[*key].as_str().is_some_and(|command| {
                        let normalized = command.replace('\\', "/");
                        prefixes.iter().any(|prefix| {
                            let (command, prefix) = if *key == "commandWindows" || cfg!(windows) {
                                (normalized.to_lowercase(), prefix.to_lowercase())
                            } else {
                                (normalized.clone(), prefix.clone())
                            };
                            command.match_indices(&prefix).any(|(index, _)| {
                                let boundary = |ch: char| {
                                    ch.is_whitespace()
                                        || matches!(ch, '/' | '\'' | '"' | '=')
                                        || ch as u32 == 96
                                };
                                command[..index].chars().next_back().is_none_or(boundary)
                                    && command[index + prefix.len()..]
                                        .chars()
                                        .next()
                                        .is_none_or(boundary)
                            })
                        })
                    })
                })
            });
        }
        groups.retain(|group| {
            group["hooks"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        });
    }
    let mut encoded = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
    encoded.push(b'\n');
    Ok(encoded)
}

fn destructive_inventory(
    project_root: &Path,
    contract: &Value,
    known_existing: &[String],
    preserve_ids: &BTreeSet<String>,
    delete_ids: &BTreeSet<String>,
) -> Result<DestructiveInventory, String> {
    let mut entries = Vec::new();
    let mut actions = Vec::new();
    let mut deletes = Vec::new();
    let mut blockers = Vec::new();
    let mut candidate_ids = BTreeSet::new();
    let mut deleted_hook_prefixes = Vec::new();
    let mut exact = BTreeSet::from([
        RETIRED_SYNC_TARGET.to_string(),
        ".codex/.bridgeforge_version".to_string(),
        contract["stamp"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase(),
        contract["contract_target"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase(),
    ]);
    exact.extend(
        RETIRED_PROJECT_MAPS
            .iter()
            .map(|(_, target)| target.to_lowercase()),
    );
    for asset in contract["assets"].as_array().into_iter().flatten() {
        if let Some(target) = asset["target"].as_str() {
            exact.insert(target.to_lowercase());
        }
    }
    for generated in contract["generated_assets"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(target) = generated["binary_targets"][current_platform()].as_str() {
            exact.insert(target.to_lowercase());
        }
        if let Some(target) = generated["receipt_target"].as_str() {
            exact.insert(target.to_lowercase());
        }
    }
    exact.extend(known_existing.iter().map(|target| target.to_lowercase()));

    let mut required_prefixes = Vec::<String>::new();
    let registry_target = crate::project_hooks::REGISTRY_PATH;
    if let Some(payload) = crate::project_hooks::read(project_root, registry_target)? {
        if !exact.contains(&registry_target.to_lowercase()) {
            entries.push(json!({
                "id": "R:project-hook-registry", "kind": "project-hook-registry",
                "target": registry_target, "before_sha256": sha_git(&payload),
                "disposition": "required-preserve"
            }));
            exact.insert(registry_target.to_lowercase());
        }
    }
    let skills = project_root.join(".codex/skills");
    if skills.exists() {
        if !skills.is_dir()
            || fs::symlink_metadata(&skills)
                .map_err(|error| error.to_string())?
                .file_type()
                .is_symlink()
        {
            blockers.push("required-preserve .codex/skills is not a plain directory".into());
        } else {
            entries.push(json!({
                "id": "R:skills",
                "kind": "skills",
                "target": ".codex/skills",
                "before_sha256": tree_sha(&skills)?,
                "disposition": "required-preserve"
            }));
            required_prefixes.push(".codex/skills".into());
        }
    }

    let mut selectable_prefixes = Vec::<(String, String)>::new();
    let hooks_root = project_root.join(".codex/hooks");
    if hooks_root.is_dir() {
        for child in fs::read_dir(&hooks_root).map_err(|error| error.to_string())? {
            let child = child.map_err(|error| error.to_string())?.path();
            let Some(name) = child.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !name.starts_with("project_") {
                continue;
            }
            let relative = relative_posix(project_root, &child)?;
            let linked = crate::persistence::is_link_or_reparse(&child)
                .map_err(|error| error.to_string())?;
            if child.is_file() && !linked {
                // Plain project_* files are handled by the exact-file inventory below.
                continue;
            }
            if !child.is_dir() || linked {
                blockers.push(format!(
                    "project hook bundle must be a plain project_* directory: {relative}"
                ));
                continue;
            }
            let id = format!("P:project-hook-bundle:{relative}");
            candidate_ids.insert(id.clone());
            let before_sha = tree_sha(&child)?;
            if delete_ids.contains(&id) {
                deleted_hook_prefixes.push(relative.clone());
                for item in WalkDir::new(&child).sort_by_file_name() {
                    let item = item.map_err(|error| error.to_string())?;
                    if !item.file_type().is_file() {
                        continue;
                    }
                    let target = relative_posix(project_root, item.path())?;
                    let payload = fs::read(item.path()).map_err(|error| error.to_string())?;
                    deletes.push(item.path().to_path_buf());
                    actions.push(SyncAction {
                        id: format!("rebuild.remove:{id}:{target}"),
                        target,
                        operation: "delete".into(),
                        risk: true,
                        before_sha256: Some(sha_git(&payload)),
                        after_sha256: "sha256:deleted".into(),
                    });
                }
            }
            entries.push(json!({
                "id": id,
                "kind": "project-hook-bundle",
                "target": relative.clone(),
                "before_sha256": before_sha,
                "disposition": if delete_ids.contains(&id) {"delete"} else if preserve_ids.contains(&id) {"preserve"} else {"user-decision"}
            }));
            selectable_prefixes.push((id, relative));
        }
    }
    let rules_root = project_root.join(".codex/rules");
    if rules_root.is_dir() {
        for entry in WalkDir::new(&rules_root).sort_by_file_name() {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = relative_posix(project_root, entry.path())?;
            if relative.ends_with(".md") || exact.contains(&relative.to_lowercase()) {
                continue;
            }
            if !relative.ends_with(".rules") {
                blockers.push(format!(
                    "unknown .codex rule structure must be classified before rebuild: {relative}"
                ));
                continue;
            }
            let id = format!("P:rule:{relative}");
            candidate_ids.insert(id.clone());
            exact.insert(relative.to_lowercase());
            let before_sha = sha_git(&fs::read(entry.path()).map_err(|error| error.to_string())?);
            let disposition = if delete_ids.contains(&id) {
                deletes.push(entry.path().to_path_buf());
                actions.push(SyncAction {
                    id: format!("rebuild.remove:{id}"),
                    target: relative.clone(),
                    operation: "delete".into(),
                    risk: true,
                    before_sha256: Some(before_sha.clone()),
                    after_sha256: "sha256:deleted".into(),
                });
                "delete"
            } else if preserve_ids.contains(&id) {
                "preserve"
            } else {
                "user-decision"
            };
            entries.push(json!({
                "id": id,
                "kind": "rule",
                "target": relative,
                "before_sha256": before_sha,
                "disposition": disposition
            }));
        }
    }

    if project_root.join(".codex").is_dir() {
        for entry in WalkDir::new(project_root.join(".codex")).sort_by_file_name() {
            let entry = entry.map_err(|error| error.to_string())?;
            let relative = relative_posix(project_root, entry.path())?;
            if relative == ".codex" {
                continue;
            }
            if entry.file_type().is_symlink() {
                blockers.push(format!(
                    "unknown or unsafe .codex structure blocks rebuild: {relative}"
                ));
                continue;
            }
            let folded = relative.to_lowercase();
            let covered = exact.contains(&folded)
                || required_prefixes.iter().any(|prefix| {
                    relative == *prefix || relative.starts_with(&format!("{prefix}/"))
                })
                || selectable_prefixes.iter().any(|(_, prefix)| {
                    relative == *prefix || relative.starts_with(&format!("{prefix}/"))
                });
            let ancestor = exact
                .iter()
                .any(|target| target.starts_with(&(folded.clone() + "/")))
                || required_prefixes
                    .iter()
                    .any(|target| target.starts_with(&(relative.clone() + "/")))
                || selectable_prefixes
                    .iter()
                    .any(|(_, target)| target.starts_with(&(relative.clone() + "/")));
            if !covered && !ancestor && entry.file_type().is_file() {
                // Only recognized retired configuration can be removed by the
                // dedicated transaction planner; unknown content blocks there.
                if crate::config_retirement::TARGETS.iter().any(|(_, target)| *target == relative) {
                    continue;
                }
                // Existing files are project decisions, never ownership proved by an old manifest.
                let id = format!("P:project-file:{relative}");
                candidate_ids.insert(id.clone());
                let before_sha = sha_git(&fs::read(entry.path()).map_err(|e| e.to_string())?);
                let disposition = if delete_ids.contains(&id) {
                    if relative.starts_with(".codex/hooks/project_")
                        && project_root.join(".codex/hooks.json").is_file()
                    {
                        deleted_hook_prefixes.push(relative.clone());
                    }
                    deletes.push(entry.path().to_path_buf());
                    actions.push(SyncAction {
                        id: format!("rebuild.remove:{id}"),
                        target: relative.clone(),
                        operation: "delete".into(),
                        risk: true,
                        before_sha256: Some(before_sha.clone()),
                        after_sha256: "sha256:deleted".into(),
                    });
                    "delete"
                } else if preserve_ids.contains(&id) {
                    "preserve"
                } else {
                    "user-decision"
                };
                entries.push(json!({"id": id, "kind": "project-file", "target": relative,
                    "before_sha256": before_sha, "disposition": disposition}));
            }
        }
    }
    Ok(DestructiveInventory {
        entries,
        actions,
        deletes,
        blockers,
        candidate_ids,
        deleted_hook_prefixes,
    })
}

pub fn build_plan(
    project_root: &Path,
    template_root: &Path,
    requested_mode: SyncMode,
) -> Result<SyncPlan, String> {
    build_plan_with_inputs(project_root, template_root, requested_mode, None)
}

pub fn build_plan_with_inputs(
    project_root: &Path,
    template_root: &Path,
    requested_mode: SyncMode,
    preservation_decisions: Option<&Value>,
) -> Result<SyncPlan, String> {
    let project_root = project_root
        .canonicalize()
        .map_err(|error| format!("project root is unavailable: {error}"))?;
    let template_root = template_root
        .canonicalize()
        .map_err(|error| format!("template root is unavailable: {error}"))?;
    let mode = resolved_mode(&project_root, requested_mode)?;
    let (contract, contract_bytes) = read_contract(&template_root)?;
    let version = contract["release_version"]
        .as_str()
        .ok_or("managed contract release_version is missing")?
        .to_string();
    let current_semver = version.parse::<crate::release::SemVer>()?;
    if current_semver < crate::baseline::MIN_SUPPORTED_VERSION.parse()? {
        return Err("target skeleton is unsupported; minimum supported version is 8.0.0".into());
    }
    let compatibility_baseline = crate::baseline::compatibility_baseline(&contract)?;
    let stamp_target = safe_join(
        &project_root,
        contract["stamp"]
            .as_str()
            .ok_or("contract stamp is missing")?,
        "contract stamp",
    )?;
    let contract_target = safe_join(
        &project_root,
        contract["contract_target"]
            .as_str()
            .ok_or("contract target is missing")?,
        "contract target",
    )?;
    crate::baseline::require_supported_project(&project_root)?;
    let previous_version = crate::baseline::project_version(&project_root)?;
    let previous_semver = previous_version
        .as_deref()
        .map(str::parse::<crate::release::SemVer>)
        .transpose()?;
    if previous_semver
        .as_ref()
        .is_some_and(|value| value > &current_semver)
    {
        return Err(format!(
            "project version {} is newer than {version}",
            previous_version.as_deref().unwrap_or_default()
        ));
    }
    let destructive_rebuild = mode == SyncMode::Adopt
        || previous_semver
            .as_ref()
            .is_some_and(|value| value < &compatibility_baseline);
    let allow_structure_upgrade = previous_semver
        .as_ref()
        .is_some_and(|value| value < &current_semver);
    let mut safe = Vec::new();
    let mut risk = Vec::new();
    let mut writes = BTreeMap::new();
    let mut deletes = Vec::new();
    let mut gaps = Vec::new();
    let mut blockers = Vec::new();
    let mut generated_source_fingerprints = BTreeMap::<String, String>::new();
    generated_source_fingerprints.insert("prepared:contract".into(), sha_raw(&contract_bytes));
    let mut target_reads = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut targets = BTreeSet::new();
    let assets = contract["assets"]
        .as_array()
        .ok_or("contract assets are missing")?;
    for asset in assets {
        let id = asset["id"]
            .as_str()
            .ok_or("asset id is missing")?
            .to_string();
        let target_raw = asset["target"]
            .as_str()
            .ok_or("asset target is missing")?
            .to_string();
        if !ids.insert(id.clone()) || !targets.insert(target_raw.to_lowercase()) {
            return Err(format!("managed contract has duplicate asset: {id}"));
        }
        let target = safe_join(&project_root, &target_raw, "asset target")?;
        let (payload, before) = render_asset(
            &project_root,
            &template_root,
            asset,
            allow_structure_upgrade,
        )?;
        target_reads.insert(target_raw.to_string(), before.clone());
        if before.as_deref() == Some(payload.as_slice()) {
            continue;
        }
        let strategy = asset["strategy"].as_str().unwrap_or("whole");
        let fully_whole_owned = strategy == "whole"
            && asset.get("agents_zones").is_none()
            && asset.get("managed_blocks").is_none();
        let is_risk = before.is_some() && fully_whole_owned && mode != SyncMode::Init;
        let action = SyncAction {
            id,
            target: target_raw,
            operation: if before.is_some() {
                "replace"
            } else {
                "create"
            }
            .into(),
            risk: is_risk,
            before_sha256: before.as_deref().map(sha_git),
            after_sha256: sha_git(&payload),
        };
        writes.insert(target, payload);
        if is_risk {
            risk.push(action);
        } else {
            safe.push(action);
        }
    }
    let preserve_ids = preservation_decisions
        .and_then(|value| value["preserve"].as_array())
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let delete_ids = preservation_decisions
        .and_then(|value| value["delete"].as_array())
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if !preserve_ids.is_disjoint(&delete_ids) {
        return Err("a project asset cannot be both preserved and deleted".into());
    }
    let mut preservation_entries = Vec::<Value>::new();
    source_retirement::plan(
        &project_root,
        &contract,
        &mut safe,
        &mut deletes,
        &mut gaps,
        &mut target_reads,
        &mut preservation_entries,
    )?;
    let mut preservation_candidates = BTreeSet::new();
    if destructive_rebuild {
        let known_existing = preservation_entries
            .iter()
            .filter_map(|entry| entry["target"].as_str().map(str::to_string))
            .collect::<Vec<_>>();
        let inventory = destructive_inventory(
            &project_root,
            &contract,
            &known_existing,
            &preserve_ids,
            &delete_ids,
        )?;
        for entry in &inventory.entries {
            if entry["disposition"].as_str() == Some("user-decision") {
                gaps.push(format!(
                    "project asset requires preserve/delete decision: {} ({})",
                    entry["id"].as_str().unwrap_or("<unknown>"),
                    entry["target"].as_str().unwrap_or("<unknown>")
                ));
            }
        }
        if !inventory.deleted_hook_prefixes.is_empty() {
            let hooks_target =
                safe_join(&project_root, ".codex/hooks.json", "project hooks target")?;
            let before = fs::read(&hooks_target).ok();
            let base = writes
                .get(&hooks_target)
                .cloned()
                .or_else(|| before.clone())
                .ok_or("project hook bundle has no hooks.json registration")?;
            let filtered = remove_hook_registrations(&base, &inventory.deleted_hook_prefixes)?;
            let after = sha_git(&filtered);
            writes.insert(hooks_target, filtered);
            if let Some(action) = safe
                .iter_mut()
                .chain(risk.iter_mut())
                .find(|action| action.target == ".codex/hooks.json")
            {
                action.after_sha256 = after;
            } else {
                risk.push(SyncAction {
                    id: "rebuild.remove:project-hook-registration".into(),
                    target: ".codex/hooks.json".into(),
                    operation: "replace".into(),
                    risk: true,
                    before_sha256: before.as_deref().map(sha_git),
                    after_sha256: after,
                });
            }
        }
        preservation_entries.extend(inventory.entries);
        preservation_candidates.extend(inventory.candidate_ids);
        risk.extend(inventory.actions);
        deletes.extend(inventory.deletes);
        blockers.extend(inventory.blockers);
    }
    let supplied = preserve_ids
        .union(&delete_ids)
        .cloned()
        .collect::<BTreeSet<_>>();
    let unknown = supplied
        .difference(&preservation_candidates)
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(format!(
            "unknown project asset decision(s): {}",
            unknown.join(", ")
        ));
    }
    let preservation_manifest = json!({
        "status": if preservation_entries.is_empty() { "not-required" } else if preservation_entries.iter().any(|entry| entry["disposition"].as_str() == Some("user-decision")) { "awaiting-confirmation" } else { "confirmed" },
        "destructive_rebuild": destructive_rebuild,
        "compatibility_baseline": contract["compatibility_baseline"],
        "entries": preservation_entries,
    });
    target_reads.insert(
        contract["contract_target"].as_str().unwrap().into(),
        fs::read(&contract_target).ok(),
    );
    target_reads.insert(
        contract["stamp"].as_str().unwrap().into(),
        fs::read(&stamp_target).ok(),
    );
    if fs::read(&contract_target).ok().as_deref() != Some(contract_bytes.as_slice()) {
        writes.insert(contract_target.clone(), contract_bytes.clone());
        safe.push(SyncAction {
            id: "managed.contract".into(),
            target: contract["contract_target"].as_str().unwrap().into(),
            operation: if contract_target.exists() {
                "replace"
            } else {
                "create"
            }
            .into(),
            risk: false,
            before_sha256: fs::read(&contract_target).ok().as_deref().map(sha_git),
            after_sha256: sha_git(&contract_bytes),
        });
    }
    let stamp = format!("{version}\n").into_bytes();
    if fs::read(&stamp_target).ok().as_deref() != Some(stamp.as_slice()) {
        writes.insert(stamp_target.clone(), stamp.clone());
        safe.push(SyncAction {
            id: "managed.stamp".into(),
            target: contract["stamp"].as_str().unwrap().into(),
            operation: if stamp_target.exists() {
                "replace"
            } else {
                "create"
            }
            .into(),
            risk: false,
            before_sha256: fs::read(&stamp_target).ok().as_deref().map(sha_git),
            after_sha256: sha_git(&stamp),
        });
    }
    plan_legacy_receipt_retirement(&project_root, &mut safe, &mut deletes)?;
    if contract["generated_assets"]
        .as_array()
        .is_some_and(|assets| {
            !assets.is_empty()
                && assets.iter().all(|a| {
                    a["receipt_target"].as_str().is_some_and(|p| {
                        bridgeforge_core::artifact_paths::within(
                            p,
                            bridgeforge_core::artifact_paths::PROJECT_BIN,
                        )
                    })
                })
        })
    {
        let old = legacy_artifact_retirements(&project_root)?;
        generated_source_fingerprints.insert(
            ARTIFACT_LAYOUT_INPUT.into(),
            legacy_runtime_fingerprint(&project_root, &old)?,
        );
        for (path, bytes) in legacy_artifact_relocations(&project_root)? {
            let relative = relative_posix(&project_root, &path)?;
            safe.push(SyncAction {
                id: format!("relocated:runtime:{relative}"),
                target: relative,
                operation: "relocate".into(),
                risk: false,
                before_sha256: transaction_file_state(&path)?.as_deref().map(sha_raw),
                after_sha256: sha_raw(&bytes),
            });
            writes.insert(path, bytes);
        }
        for (path, bytes) in old {
            let relative = path
                .strip_prefix(&project_root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            safe.push(SyncAction {
                id: format!("retired:runtime:{relative}"),
                target: relative,
                operation: "delete".into(),
                risk: false,
                before_sha256: Some(sha_git(&bytes)),
                after_sha256: sha_git(&[]),
            });
            if !deletes.contains(&path) {
                deletes.push(path);
            }
        }
    }
    plan_project_map_retirement(&project_root, &mut safe, &mut deletes)?;
    plan_config_retirement(&project_root, &mut safe, &mut deletes, &mut target_reads)?;
    for target in std::iter::once(".codex/bridgeforge-version.json")
        .chain(crate::config_retirement::TARGETS.iter().map(|(_, target)| *target))
    {
        generated_source_fingerprints.insert(
            format!("retired-config-input:{target}"),
            target_reads[target].as_deref().map(sha_raw).unwrap_or_else(|| "absent".into()),
        );
    }
    plan_sync_role_retirement(
        &project_root,
        &mut safe,
        &mut deletes,
        &mut gaps,
        &mut generated_source_fingerprints,
    )?;
    let mut project_hook_reads = target_reads;
    // A fresh Init may not have a committed VERSION baseline yet. Its first
    // release setup establishes that proof; existing projects must not skip it.
    let release_upgrade =
        if contract["release_model"] == "product-packages" && mode != SyncMode::Init {
            crate::release::schema4::prepare_project_upgrade(&project_root)?
        } else {
            None
        };
    if let Some(upgrade) = &release_upgrade {
        let target = ".codex/bridgeforge-version.json";
        let path = project_root.join(target);
        if writes.contains_key(&path) || deletes.contains(&path) {
            return Err("release cutover overlaps another project update action".into());
        }
        let payload = upgrade.payload()?;
        risk.push(SyncAction {
            id: "migration.release-model".into(),
            target: target.into(),
            operation: "cutover".into(),
            risk: true,
            before_sha256: transaction_file_state(&path)?.as_deref().map(sha_raw),
            after_sha256: sha_raw(&payload),
        });
        generated_source_fingerprints
            .insert("release-cutover".into(), upgrade.fingerprint().into());
        for (path, bytes) in upgrade.inputs() {
            project_hook_reads.insert(relative_posix(&project_root, path)?, bytes.clone());
        }
        writes.insert(path, payload);
    }
    safe.sort_by(|left, right| left.target.cmp(&right.target));
    risk.sort_by(|left, right| left.target.cmp(&right.target));
    let fingerprint = plan_fingerprint(
        &mode,
        &version,
        &safe,
        &risk,
        &preservation_manifest,
        &generated_source_fingerprints,
    )?;
    let confirmation_required = !risk.is_empty();
    Ok(SyncPlan {
        build_metrics: Default::default(),
        schema: 1,
        status: if safe.is_empty()
            && risk.is_empty()
            && !contract["generated_assets"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
        {
            "current"
        } else {
            "planned"
        }
        .into(),
        readiness: if confirmation_required || !gaps.is_empty() {
            "action_required"
        } else {
            "ready"
        }
        .into(),
        release_setup: crate::git_sync::release_setup_status(&project_root),
        mode,
        previous_version,
        current_version: version,
        safe,
        risk,
        gaps,
        blockers,
        preservation_manifest,
        confirmation_required,
        aggregate_fingerprint: fingerprint,
        public_installation_required: contract["generated_assets"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        project_root,
        writes,
        deletes,
        generated_source_fingerprints,
        project_hook_reads,
        public_actions: Vec::new(),
        release_upgrade,
    })
}

fn verify_project_hook_reads(plan: &SyncPlan) -> Result<(), String> {
    for (relative, expected) in &plan.project_hook_reads {
        if crate::project_hooks::read(&plan.project_root, relative)? != *expected {
            return Err(format!("project input changed after plan: {relative}"));
        }
    }
    Ok(())
}

pub fn apply_plan(
    plan: SyncPlan,
    confirmed_plan_fingerprint: &str,
    confirmed_risk: bool,
) -> Result<SyncReceipt, String> {
    apply_plan_internal(plan, confirmed_plan_fingerprint, confirmed_risk, None)
}

/// Fixed public deliveries are chosen by product preparation. They do not
/// participate in project planning or compilation, but enter the same protected
/// write set and rollback snapshots when Apply consumes them.
pub fn attach_prepared_public(
    plan: &mut SyncPlan,
    contract_bytes: &[u8],
    prepared: &Path,
) -> Result<(), String> {
    if plan.generated_source_fingerprints.get("prepared:contract") != Some(&sha_raw(contract_bytes))
    {
        return Err("target contract changed after project planning".into());
    }
    if !plan.public_installation_required {
        return Ok(());
    }
    let contract = crate::json::parse_unique_json(contract_bytes, "prepared public contract")?;
    let payloads = crate::public_artifacts::read(prepared, &contract)?;
    for asset in contract["generated_assets"]
        .as_array()
        .ok_or("public assets missing")?
    {
        let binary_relative = asset["binary_targets"][current_platform()]
            .as_str()
            .ok_or("public binary target missing")?;
        let receipt_relative = asset["receipt_target"]
            .as_str()
            .ok_or("public receipt target missing")?;
        let binary_path = safe_join(&plan.project_root, binary_relative, "public installation")?;
        let receipt_path = safe_join(
            &plan.project_root,
            receipt_relative,
            "public installation receipt",
        )?;
        let binary = transaction_file_state(&binary_path)?;
        let receipt = transaction_file_state(&receipt_path)?;
        let desired_binary = payloads
            .get(binary_relative)
            .ok_or("prepared public binary missing")?;
        let desired_receipt = payloads
            .get(receipt_relative)
            .ok_or("prepared public receipt missing")?;
        if binary.as_deref() == Some(desired_binary.as_slice())
            && receipt.as_deref() == Some(desired_receipt.as_slice())
        {
            continue;
        }
        let old = receipt
            .as_deref()
            .map(|b| crate::json::parse_unique_json(b, "installed public receipt"))
            .transpose()?;
        let owned = old.as_ref().is_some_and(|r| {
            r["schema_version"] == 2
                && r["generated_asset_id"] == asset["id"]
                && binary
                    .as_deref()
                    .is_none_or(|b| r["binary_sha256"].as_str() == Some(sha_raw(b).as_str()))
        });
        if (binary.is_some() || receipt.is_some()) && !owned {
            return Err(format!(
                "unowned public installation target is preserved: {binary_relative}"
            ));
        }
        for (relative, path, before, payload) in [
            (binary_relative, binary_path, binary, desired_binary),
            (receipt_relative, receipt_path, receipt, desired_receipt),
        ] {
            if before.as_deref() == Some(payload.as_slice()) {
                continue;
            }
            if plan.writes.contains_key(&path) || plan.deletes.contains(&path) {
                return Err("public installation overlaps project plan".into());
            }
            plan.public_actions.push(SyncAction {
                id: format!("public:{}:{relative}", asset["id"].as_str().unwrap_or("")),
                target: relative.into(),
                operation: "install".into(),
                risk: false,
                before_sha256: before.as_deref().map(sha_raw),
                after_sha256: sha_raw(payload),
            });
            plan.writes.insert(path, payload.clone());
        }
    }
    Ok(())
}

fn validate_transaction_path(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                if crate::persistence::is_link_or_reparse(ancestor)
                    .map_err(|error| error.to_string())?
                {
                    return Err(format!(
                        "transaction path traverses a link: {}",
                        ancestor.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

pub fn transaction_file_state(path: &Path) -> Result<Option<Vec<u8>>, String> {
    validate_transaction_path(path)?;
    match fs::read(path) {
        Ok(payload) => Ok(Some(payload)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "cannot read transaction target {}: {error}",
            path.display()
        )),
    }
}

fn delete_unchanged_file(path: &Path, before: Option<&[u8]>) -> Result<(), String> {
    if transaction_file_state(path)?.as_deref() != before {
        return Err(format!(
            "transaction delete target changed before removal: {}",
            path.display()
        ));
    }
    if before.is_some() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_unchanged_file(path: &Path, before: Option<&[u8]>, payload: &[u8]) -> Result<(), String> {
    if transaction_file_state(path)?.as_deref() != before {
        return Err(format!(
            "target changed before write; external state preserved: {}",
            path.display()
        ));
    }
    atomic_write(path, payload)
}

fn apply_plan_internal(
    plan: SyncPlan,
    confirmed_plan_fingerprint: &str,
    confirmed_risk: bool,
    before_stamp: Option<&dyn Fn(&Path, &Path) -> Result<(), String>>,
) -> Result<SyncReceipt, String> {
    crate::baseline::require_supported_project(&plan.project_root)?;
    // Rejected confirmation must not even create a lock file in the project.
    if !plan.gaps.is_empty() || !plan.blockers.is_empty() {
        return Err("unresolved gap or blocker prevents apply".into());
    }
    if !plan.risk.is_empty() && !confirmed_risk {
        return Err("risk actions require --confirmed-risk".into());
    }
    let _lock = ProjectLock::acquire(&plan.project_root)?;
    let _legacy_cache = bridgeforge_core::artifact_cache::lock_legacy(&plan.project_root)?;
    if let Some(expected) = plan
        .generated_source_fingerprints
        .get(ARTIFACT_LAYOUT_INPUT)
    {
        let current = legacy_artifact_retirements(&plan.project_root)?;
        if !current.is_empty() {
            bridgeforge_core::process_activity::legacy_clients_quiet(&plan.project_root)?;
        }
        if &legacy_runtime_fingerprint(&plan.project_root, &current)? != expected {
            return Err("legacy runtime changed after plan; preserve it and regenerate".into());
        }
    }
    if let Some(upgrade) = &plan.release_upgrade {
        upgrade.verify(&plan.project_root)?;
    }
    if let Some(expected) = plan.generated_source_fingerprints.get(RETIRED_SYNC_INPUT)
        && sync_role_fingerprint(&sync_role_inputs(&plan.project_root)?)? != *expected
    {
        return Err("retired role or reference inputs drifted; regenerate the plan".into());
    }
    let legacy = legacy_receipt(&plan.project_root)?;
    if legacy
        .as_ref()
        .is_some_and(|(path, _)| !plan.deletes.contains(path))
    {
        return Err("legacy build receipt appeared after plan; regenerate the plan".into());
    }
    for (_, target) in RETIRED_PROJECT_MAPS {
        let path = safe_join(&plan.project_root, target, "retired project map")?;
        if transaction_file_state(&path)?.is_some() && !plan.deletes.contains(&path) {
            return Err(format!(
                "retired project map appeared after plan; regenerate the plan: {target}"
            ));
        }
    }
    let current_fingerprint = plan_fingerprint(
        &plan.mode,
        &plan.current_version,
        &plan.safe,
        &plan.risk,
        &plan.preservation_manifest,
        &plan.generated_source_fingerprints,
    )?;
    if plan.aggregate_fingerprint != confirmed_plan_fingerprint
        || plan.aggregate_fingerprint != current_fingerprint
    {
        return Err("aggregate fingerprint drifted; regenerate the plan".into());
    }
    for entry in plan.preservation_manifest["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            matches!(
                entry["disposition"].as_str(),
                Some("preserve" | "required-preserve")
            ) || (entry["disposition"] == "delete" && entry["kind"] == "project-hook-bundle")
        })
    {
        let relative = entry["target"]
            .as_str()
            .ok_or("preservation entry target is missing")?;
        let target = safe_join(&plan.project_root, relative, "preserved project asset")?;
        let actual = if target.is_dir() {
            tree_sha(&target)?
        } else {
            sha_git(
                &fs::read(&target)
                    .map_err(|_| format!("preserved project asset disappeared: {relative}"))?,
            )
        };
        if entry["before_sha256"].as_str() != Some(actual.as_str()) {
            return Err(format!(
                "preserved project asset drifted; regenerate the plan: {relative}"
            ));
        }
    }
    let mut snapshots = plan
        .writes
        .keys()
        .chain(plan.deletes.iter())
        .map(|path| transaction_file_state(path).map(|before| (path.clone(), before)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    for (relative, expected) in &plan.project_hook_reads {
        let path = safe_join(&plan.project_root, relative, "planned input")?;
        let actual = match snapshots.get(&path) {
            Some(v) => v.clone(),
            None => transaction_file_state(&path)?,
        };
        if &actual != expected {
            return Err(format!("project input changed after plan: {relative}"));
        }
    }
    for action in plan
        .safe
        .iter()
        .chain(plan.risk.iter())
        .chain(plan.public_actions.iter())
    {
        let target = safe_join(&plan.project_root, &action.target, "transaction target")?;
        let current = snapshots.get(&target).ok_or_else(|| {
            format!(
                "planned target is missing from transaction: {}",
                action.target
            )
        })?;
        let digest = if action.id.starts_with("migration.")
            || action.id.starts_with("migration-status:")
            || action.id.starts_with("public:")
            || action.id.starts_with("relocated:runtime:")
        {
            sha_raw
        } else {
            sha_git
        };
        let current_hash = current.as_deref().map(digest);
        if current_hash != action.before_sha256 {
            return Err(format!(
                "aggregate fingerprint drifted at {}; regenerate the plan",
                action.target
            ));
        }
        if let Some(payload) = plan.writes.get(&target)
            && digest(payload) != action.after_sha256
        {
            return Err(format!(
                "planned payload drifted internally: {}",
                action.target
            ));
        }
    }
    let contract_path = plan.project_root.join(".codex/managed-skeleton.json");
    let contract_payload = plan
        .writes
        .get(&contract_path)
        .cloned()
        .or_else(|| fs::read(&contract_path).ok())
        .ok_or("prospective managed-skeleton contract is missing")?;
    let prospective_contract: Value =
        serde_json::from_slice(&contract_payload).map_err(|error| error.to_string())?;
    let stamp_relative = prospective_contract["stamp"]
        .as_str()
        .ok_or("prospective baseline stamp is missing")?;
    let stamp_path = safe_join(
        &plan.project_root,
        stamp_relative,
        "prospective baseline stamp",
    )?;
    let stamp_payload = plan
        .writes
        .get(&stamp_path)
        .cloned()
        .or_else(|| fs::read(&stamp_path).ok())
        .ok_or("version stamp is missing from the transaction")?;
    if !plan.writes.is_empty() || !plan.deletes.is_empty() {
        if !snapshots.contains_key(&stamp_path) {
            snapshots.insert(stamp_path.clone(), transaction_file_state(&stamp_path)?);
        }
    }
    let mut retired_directories = Vec::new();
    for entry in plan.preservation_manifest["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["disposition"] == "delete" && entry["kind"] == "project-hook-bundle")
    {
        let directory = safe_join(
            &plan.project_root,
            entry["target"]
                .as_str()
                .ok_or("retired bundle target missing")?,
            "retired hook bundle",
        )?;
        for item in WalkDir::new(directory).contents_first(true) {
            let item = item.map_err(|error| error.to_string())?;
            if item.file_type().is_dir() {
                retired_directories.push(item.path().to_path_buf());
            }
        }
    }
    let mut removed_directories = Vec::new();
    let mut expected_after = BTreeMap::<PathBuf, Option<Vec<u8>>>::new();
    let apply_result = (|| {
        if plan.writes.is_empty() && plan.deletes.is_empty() {
            crate::baseline::verify(&plan.project_root, None, true)?;
            return Ok::<(), String>(());
        }
        for (path, payload) in &plan.writes {
            if path == &stamp_path {
                continue;
            }
            write_unchanged_file(
                path,
                snapshots.get(path).and_then(|bytes| bytes.as_deref()),
                payload,
            )?;
            expected_after.insert(path.clone(), Some(payload.clone()));
        }
        verify_written_payloads(&plan.writes, Some(&stamp_path))?;
        for path in &plan.deletes {
            let original = legacy
                .as_ref()
                .filter(|(legacy_path, _)| legacy_path == path)
                .map(|(_, bytes)| Some(bytes.as_slice()))
                .unwrap_or_else(|| snapshots.get(path).and_then(|bytes| bytes.as_deref()));
            delete_unchanged_file(path, original)?;
            expected_after.insert(path.clone(), None);
        }
        for directory in &retired_directories {
            fs::remove_dir(directory).map_err(|error| {
                format!(
                    "cannot retire hook directory {}: {error}",
                    directory.display()
                )
            })?;
            removed_directories.push(directory.clone());
        }
        let mut evidence = Some(crate::baseline::verify_prospective_recorded(
            &plan.project_root,
            &contract_path,
            &plan.current_version,
        )?);
        if let Some(observer) = before_stamp {
            observer(&plan.project_root, &stamp_path)?;
        }
        verify_written_payloads(&plan.writes, Some(&stamp_path))?;
        if plan
            .generated_source_fingerprints
            .contains_key(ARTIFACT_LAYOUT_INPUT)
        {
            verify_legacy_artifact_retirement(
                &plan.project_root,
                plan.deletes
                    .contains(&plan.project_root.join(".codex/bin/.gitignore")),
            )?;
        }
        if plan
            .generated_source_fingerprints
            .contains_key(RETIRED_SYNC_INPUT)
        {
            let inputs = sync_role_inputs(&plan.project_root)?;
            if inputs.contains_key(RETIRED_SYNC_TARGET) || !sync_role_references(&inputs).is_empty()
            {
                return Err(
                    "retired role or reference appeared during apply; regenerate the plan".into(),
                );
            }
        }
        write_unchanged_file(
            &stamp_path,
            snapshots
                .get(&stamp_path)
                .and_then(|bytes| bytes.as_deref()),
            &stamp_payload,
        )?;
        expected_after.insert(stamp_path.clone(), Some(stamp_payload.clone()));
        verify_written_payloads(&plan.writes, None)?;
        if let Some(evidence) = evidence.as_mut() {
            for (path, bytes) in &expected_after {
                evidence.include(path.clone(), bytes.clone());
            }
            evidence.verify_unchanged(&plan.project_root)?;
        } else {
            crate::baseline::verify(&plan.project_root, None, true)?;
        }
        if crate::baseline::project_version(&plan.project_root)?
            != Some(plan.current_version.clone())
        {
            return Err("final version identity does not match the installed baseline".into());
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = apply_result {
        let mut rollback_errors = Vec::new();
        for directory in removed_directories.iter().rev() {
            if let Err(item) = fs::create_dir_all(directory) {
                rollback_errors.push(format!(
                    "cannot restore directory {}: {item}",
                    directory.display()
                ));
            }
        }
        for (path, before) in snapshots.into_iter().rev() {
            let actual = match transaction_file_state(&path) {
                Ok(value) => value,
                Err(item) => {
                    rollback_errors.push(format!(
                        "{}: {item}; external state preserved",
                        path.display()
                    ));
                    continue;
                }
            };
            if actual == before {
                continue;
            }
            if expected_after.get(&path) != Some(&actual) {
                rollback_errors.push(format!(
                    "{}: target changed outside the transaction; external state preserved",
                    path.display()
                ));
                continue;
            }
            let result = match before {
                Some(payload) => atomic_write(&path, &payload),
                None if path.exists() => fs::remove_file(&path).map_err(|item| item.to_string()),
                None => Ok(()),
            };
            if let Err(item) = result {
                rollback_errors.push(format!("{}: {item}", path.display()));
            }
        }
        return if rollback_errors.is_empty() {
            Err(format!("transaction rolled back: {error}"))
        } else {
            Err(format!(
                "transaction rollback incomplete after {error}: {}",
                rollback_errors.join(", ")
            ))
        };
    }
    let mut applied = plan.safe.clone();
    applied.extend(plan.risk.clone());
    applied.extend(plan.public_actions.clone());
    let stamp_written_last = !applied.is_empty();
    Ok(SyncReceipt {
        build_metrics: plan.build_metrics,
        schema: 1,
        status: "applied".into(),
        execution_status: "succeeded".into(),
        mode: plan.mode,
        previous_version: plan.previous_version,
        current_version: plan.current_version,
        aggregate_fingerprint: plan.aggregate_fingerprint,
        applied,
        rollback_performed: false,
        stamp_written_last,
        project_readiness: "ready".into(),
        release_setup: crate::git_sync::release_setup_status(&plan.project_root),
        preserved_asset_ids: plan.preservation_manifest["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| {
                matches!(
                    entry["disposition"].as_str(),
                    Some("preserve" | "required-preserve")
                )
            })
            .filter_map(|entry| entry["id"].as_str().map(str::to_string))
            .collect(),
    })
}

fn human_plan(plan: &SyncPlan) -> Value {
    let (conclusion, next_step) = if !plan.blockers.is_empty() {
        ("未完成", "先处理同步器报告的阻断项，再重新生成计划")
    } else if !plan.gaps.is_empty() || plan.confirmation_required {
        ("等待确认", "完成当前计划中的用户决定后重新生成计划")
    } else if plan.status == "current" {
        ("无需处理", "本次操作已结束，无需继续处理")
    } else {
        ("可直接执行", "按当前计划执行骨架事务")
    };
    let pending = if !plan.blockers.is_empty() {
        vec![format!("仍有 {} 项阻断需要处理", plan.blockers.len())]
    } else if !plan.gaps.is_empty() {
        vec![format!("仍有 {} 项决定需要确认", plan.gaps.len())]
    } else if plan.confirmation_required {
        vec![format!("有 {} 项风险动作等待统一确认", plan.risk.len())]
    } else {
        Vec::new()
    };
    json!({
        "conclusion": conclusion,
        "pending": pending,
        "next_step": next_step,
        "current_version": plan.current_version,
        "release_setup": plan.release_setup
    })
}

fn formatted_outcome(machine: Value, human: Value, format: &str, code: i32) -> CommandOutcome {
    let receipt = match format {
        "machine" => machine,
        "human" => human,
        "combined" => json!({"machine": machine, "human": human}),
        _ => {
            return CommandOutcome::blocked(
                "[project-sync] BLOCKED: invalid --output-format; expected machine|human|combined\n",
            );
        }
    };
    CommandOutcome {
        code,
        receipt: Some(receipt),
        ..CommandOutcome::default()
    }
}

pub fn outcome_plan_with_format(result: Result<SyncPlan, String>, format: &str) -> CommandOutcome {
    match result {
        Ok(plan) => {
            let code = if plan.blockers.is_empty() { 0 } else { 2 };
            let human = human_plan(&plan);
            let machine = serde_json::to_value(plan)
                .unwrap_or_else(|error| json!({"error": error.to_string()}));
            formatted_outcome(machine, human, format, code)
        }
        Err(error) if format == "machine" => {
            CommandOutcome::blocked(format!("[project-sync] BLOCKED: {error}\n"))
        }
        Err(error) => formatted_outcome(
            json!({"status": "blocked", "error": error}),
            json!({
                "conclusion": "未完成",
                "pending": ["同步器在写入前停止"],
                "next_step": "先处理停止原因，再重新生成计划"
            }),
            format,
            2,
        ),
    }
}

pub fn outcome_plan(result: Result<SyncPlan, String>) -> CommandOutcome {
    outcome_plan_with_format(result, "machine")
}

pub fn outcome_receipt_with_format(
    result: Result<SyncReceipt, String>,
    format: &str,
) -> CommandOutcome {
    match result {
        Ok(receipt) => {
            let noop = receipt.applied.is_empty();
            let human = json!({
                "conclusion": if noop {"无需处理"} else {"已完成"},
                "pending": [],
                "next_step": if noop {"本次骨架操作已结束，无需继续处理"}else{"需要保存到 GitHub 时运行 $git-sync"},
                "current_version": receipt.current_version,
                "release_setup": receipt.release_setup,
                "changed_count": receipt.applied.len()
            });
            let machine = serde_json::to_value(receipt)
                .unwrap_or_else(|error| json!({"error": error.to_string()}));
            formatted_outcome(machine, human, format, 0)
        }
        Err(error) if format == "machine" => {
            CommandOutcome::blocked(format!("[project-sync] BLOCKED: {error}\n"))
        }
        Err(error) => formatted_outcome(
            json!({"status": "blocked", "error": error}),
            json!({
                "conclusion": "未完成",
                "pending": ["同步事务没有完成"],
                "next_step": "保留现场并先处理停止原因"
            }),
            format,
            2,
        ),
    }
}

pub fn attach_generated_assets(
    plan: &mut SyncPlan,
    template_root: &Path,
    project_root: &Path,
    contract: &Value,
    runner: &dyn ProcessRunner,
) -> Result<Vec<Value>, String> {
    // The planner owns a canonical root. Keep legacy callers' relative or
    // Windows verbatim-path spellings from creating a second key for a target.
    let canonical_root = project_root.canonicalize().map_err(|e| e.to_string())?;
    if canonical_root != plan.project_root {
        return Err("generated assets belong to a different project than the plan".into());
    }
    let project_root = canonical_root.as_path();
    let observed = crate::build_metrics::Observed {
        runner,
        metrics: Default::default(),
    };
    let runner = &observed as &dyn ProcessRunner;
    let preparation_started = std::time::Instant::now();
    let result = (|| -> Result<Vec<Value>, String> {
        let started = std::time::Instant::now();
        let mut receipts = Vec::new();
        if plan
            .safe
            .iter()
            .any(|action| action.id.starts_with("generated:"))
        {
            let generated = crate::generated_assets::generated_writes_reusing(
                template_root,
                "source_root",
                project_root,
                contract,
                runner,
            );
            observed
                .metrics
                .borrow_mut()
                .timings_ms
                .insert("managed_assets".into(), started.elapsed().as_millis());
            let (writes, generated_receipts) = generated?;
            plan.writes.extend(writes);
            receipts.extend(generated_receipts);
        }
        observed
            .metrics
            .borrow_mut()
            .timings_ms
            .insert("managed_assets".into(), started.elapsed().as_millis());
        verify_project_hook_reads(plan)?;
        for action in plan
            .safe
            .iter_mut()
            .chain(plan.risk.iter_mut())
            .filter(|action| action.id.starts_with("generated:"))
        {
            let target = safe_join(project_root, &action.target, "generated action target")?;
            let payload = plan.writes.get(&target).ok_or_else(|| {
                format!("generated build did not attach payload: {}", action.target)
            })?;
            action.after_sha256 = sha_git(payload);
        }
        plan.aggregate_fingerprint = plan_fingerprint(
            &plan.mode,
            &plan.current_version,
            &plan.safe,
            &plan.risk,
            &plan.preservation_manifest,
            &plan.generated_source_fingerprints,
        )?;
        Ok(receipts)
    })();
    observed.metrics.borrow_mut().timings_ms.insert(
        "build_preparation".into(),
        preparation_started.elapsed().as_millis(),
    );
    plan.build_metrics = observed.metrics.into_inner();
    result
}

pub fn build_generated_assets(
    project_root: &Path,
    contract: &Value,
    runner: &dyn ProcessRunner,
) -> Result<Vec<Value>, String> {
    crate::baseline::require_supported_project(project_root)?;
    let _lock = ProjectLock::acquire(project_root)?;
    let legacy = legacy_receipt(project_root)?;
    let (writes, receipts) = generated_writes(
        project_root,
        "target_source_root",
        project_root,
        contract,
        runner,
    )?;
    let snapshots = writes
        .keys()
        .map(|path| transaction_file_state(path).map(|state| (path.clone(), state)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    install_generated_transaction(project_root, &writes, &snapshots, &BTreeSet::new(), || {
        if let Some((path, before)) = &legacy {
            if fs::read(path).map_err(|error| error.to_string())? != *before {
                return Err("legacy build receipt changed during build".into());
            }
            fs::remove_file(path).map_err(|error| error.to_string())?;
        }
        Ok::<(), String>(())
    })?;
    Ok(receipts)
}

/// Shared generated-asset install/rollback. The caller holds the project lock,
/// captures targets before expensive work and verifies its source identity.
pub fn install_generated_transaction(
    root: &Path,
    writes: &BTreeMap<PathBuf, Vec<u8>>,
    snapshots: &BTreeMap<PathBuf, Option<Vec<u8>>>,
    binaries: &BTreeSet<PathBuf>,
    verify_after: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    install_generated_transaction_retiring(
        root,
        writes,
        snapshots,
        binaries,
        &BTreeMap::new(),
        verify_after,
    )
}

pub fn install_generated_transaction_retiring(
    root: &Path,
    writes: &BTreeMap<PathBuf, Vec<u8>>,
    snapshots: &BTreeMap<PathBuf, Option<Vec<u8>>>,
    binaries: &BTreeSet<PathBuf>,
    retirements: &BTreeMap<PathBuf, Vec<u8>>,
    verify_after: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    install_transaction(root, writes, snapshots, binaries, retirements, true, verify_after)
}

/// Template-only transactions verify their final state after all retirements.
pub fn install_template_transaction(
    root: &Path,
    writes: &BTreeMap<PathBuf, Vec<u8>>,
    snapshots: &BTreeMap<PathBuf, Option<Vec<u8>>>,
    binaries: &BTreeSet<PathBuf>,
    retirements: &BTreeMap<PathBuf, Vec<u8>>,
    verify_after: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    install_transaction(root, writes, snapshots, binaries, retirements, false, verify_after)
}

fn install_transaction(
    root: &Path,
    writes: &BTreeMap<PathBuf, Vec<u8>>,
    snapshots: &BTreeMap<PathBuf, Option<Vec<u8>>>,
    binaries: &BTreeSet<PathBuf>,
    retirements: &BTreeMap<PathBuf, Vec<u8>>,
    legacy: bool,
    verify_after: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    for (path, bytes) in retirements {
        if writes.contains_key(path)
            || snapshots.get(path).and_then(Option::as_ref) != Some(bytes)
            || transaction_file_state(path)?.as_ref() != Some(bytes)
        {
            return Err(format!("retirement input drifted: {}", path.display()));
        }
    }
    for path in writes.keys() {
        let before = snapshots
            .get(path)
            .ok_or("missing generated target snapshot")?;
        if &transaction_file_state(path)? != before {
            return Err(format!(
                "generated target changed before installation: {}",
                path.display()
            ));
        }
    }
    let write = |path: &Path, payload: &[u8]| {
        if binaries.contains(path) {
            crate::runtime::write_binary(root, path, payload)
        } else {
            atomic_write(path, payload)
        }
    };
    let mut deleted = BTreeSet::new();
    let result = (|| {
        for (path, payload) in writes {
            if &transaction_file_state(path)?
                != snapshots.get(path).ok_or("missing target snapshot")?
            {
                return Err(format!(
                    "generated target changed during installation: {}",
                    path.display()
                ));
            }
            // A healthy or byte-identical target must retain its timestamp.
            if snapshots[path].as_deref() != Some(payload) {
                write(path, payload)?;
            }
        }
        let mut verify_after = Some(verify_after);
        if legacy { verify_after.take().unwrap()()?; }
        verify_written_payloads(writes, None)?;
        for (path, bytes) in retirements {
            delete_unchanged_file(path, Some(bytes))?;
            deleted.insert(path.clone());
        }
        verify_written_payloads(writes, None)?;
        if legacy && !retirements.is_empty() {
            verify_legacy_artifact_retirement(
                root,
                retirements.contains_key(&root.join(".codex/bin/.gitignore")),
            )?;
        }
        if legacy && !retirements.is_empty() {
            bridgeforge_core::process_activity::legacy_clients_quiet(root)?;
        }
        if let Some(verify) = verify_after {
            verify()?;
            verify_written_payloads(writes, None)?;
            for path in retirements.keys() {
                if transaction_file_state(path)?.is_some() {
                    return Err(format!("retired target was recreated during verification: {}", path.display()));
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        for path in deleted.iter().rev() {
            match transaction_file_state(path) {
                Ok(None) => {
                    if let Err(error) = write(path, &retirements[path]) {
                        failures.push(format!("{}: {error}", path.display()));
                    }
                }
                Ok(Some(current)) if current == retirements[path] => (),
                _ => failures.push(format!(
                    "{}: external change preserved during retirement rollback",
                    path.display()
                )),
            }
        }
        for (target, before) in snapshots
            .iter()
            .rev()
            .filter(|(target, _)| writes.contains_key(*target))
        {
            let current = match transaction_file_state(&target) {
                Ok(state) => state,
                Err(error) => {
                    failures.push(format!(
                        "{}: {error}; external target preserved",
                        target.display()
                    ));
                    continue;
                }
            };
            if &current == before {
                continue;
            }
            if current.as_deref() != writes.get(target).map(Vec::as_slice) {
                failures.push(format!(
                    "{}: external change preserved during rollback",
                    target.display()
                ));
                continue;
            }
            let restored = match before {
                Some(value) => write(target, value),
                None if target.exists() => {
                    fs::remove_file(&target).map_err(|error| error.to_string())
                }
                None => Ok(()),
            };
            if let Err(error) = restored {
                failures.push(format!("{}: {error}", target.display()));
            }
        }
        return Err(if failures.is_empty() {
            format!("generated asset transaction rolled back: {error}")
        } else {
            format!(
                "generated asset rollback incomplete after {error}: {}",
                failures.join("; ")
            )
        });
    }
    Ok(())
}

fn verify_written_payloads(
    writes: &BTreeMap<PathBuf, Vec<u8>>,
    excluded: Option<&Path>,
) -> Result<(), String> {
    for (path, bytes) in writes {
        if excluded == Some(path.as_path()) {
            continue;
        }
        if transaction_file_state(path)?.as_ref() != Some(bytes) {
            return Err(format!(
                "installed payload changed during transaction: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_project_sync.rs"]
mod tests;
