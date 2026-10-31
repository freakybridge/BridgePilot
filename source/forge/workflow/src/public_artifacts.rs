//! Product preparation owns public builds; project installation only consumes
//! immutable, contract-bound cache references. No project discovery lives here.
use crate::{CommandOutcome, ProcessRunner, artifact_cache::Cache, managed_paths::safe_join};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

const OWNER: &str = "bridgeforge-public-artifacts-v1";
const READY: &str = ".cache/bridgeforge/public-ready";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepared {
    schema: u32,
    owner: String,
    input: String,
    version: String,
    artifacts: BTreeMap<String, Value>,
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn input(contract: &Value) -> Result<String, String> {
    crate::manifest::canonical_sha(&json!({"schema":1,"version":contract["release_version"],
        "generated_assets":contract["generated_assets"]}))
}

fn assets(contract: &Value) -> Result<&Vec<Value>, String> {
    contract["generated_assets"]
        .as_array()
        .ok_or("public assets contract is missing".into())
}

pub fn codex_home() -> Result<PathBuf, String> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|p| PathBuf::from(p).join(".codex"))
        })
        .ok_or("Codex home is unavailable".into())
}

pub fn prepared_path(codex_root: &Path, contract: &Value) -> Result<PathBuf, String> {
    safe_join(
        codex_root,
        &format!("{READY}/{}.json", &input(contract)?[7..]),
        "public artifact receipt",
    )
}

/// Read-only bootstrap lookup used by an already installed compatible CLI.
/// A cache miss is not an installation failure; the updater may compile once.
pub fn locate_cli(
    product_root: &Path,
    codex_root: &Path,
    runner: &dyn ProcessRunner,
) -> Result<Value, String> {
    let contract = bridgeforge_core::product_manifest::project(product_root)?;
    let path = prepared_path(codex_root, &contract)?;
    let bytes =
        crate::managed_paths::read_metadata(&path, 128 * 1024)?.ok_or("no prepared public CLI")?;
    let prepared: Prepared = serde_json::from_value(crate::json::parse_unique_json(
        &bytes,
        "prepared public CLI",
    )?)
    .map_err(|e| e.to_string())?;
    let _ = read(&path, &contract)?;
    let reference = prepared
        .artifacts
        .get("codex.bridgeforge-cli")
        .ok_or("public CLI reference missing")?;
    let environment = crate::artifact_cache::environment(
        &crate::source_projection::source_root(product_root),
        runner,
    )
    .ok_or("public compiler environment unavailable")?;
    if reference["identity"]["environment"].as_str() != Some(environment.as_str()) {
        return Err("public compiler environment changed".into());
    }
    let key = reference["key"]
        .as_str()
        .ok_or("public cache key missing")?;
    let binary = safe_join(
        codex_root,
        &format!(
            ".cache/bridgeforge/public-build/{key}/bridgeforge{}",
            std::env::consts::EXE_SUFFIX
        ),
        "public CLI bootstrap",
    )?;
    let identity = crate::runtime::build_environment(
        &crate::source_projection::source_root(product_root),
        runner,
    )
    .receipt
    .ok_or("public compilation identity missing")?;
    if identity["status"] != "cacheable" {
        return Err("public compilation identity unavailable".into());
    }
    Ok(
        json!({"schema":1,"status":"available","binary":binary,"binary_sha256":reference["binary_sha256"],"identity":identity["identity"]}),
    )
}

/// Capture all public payloads before a project transaction. This does not
/// compile, execute binaries, inspect project files or update cache recency.
pub fn read(path: &Path, contract: &Value) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let codex_root = path
        .ancestors()
        .nth(4)
        .ok_or("invalid public receipt location")?;
    if path != prepared_path(codex_root, contract)? {
        return Err("public receipt does not match the target contract".into());
    }
    let before = crate::managed_paths::read_metadata(path, 128 * 1024)?
        .ok_or("public artifacts are not prepared; run the official product updater")?;
    let value = crate::json::parse_unique_json(&before, "public artifact receipt")?;
    let prepared: Prepared = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if prepared.schema != 1
        || prepared.owner != OWNER
        || prepared.input != input(contract)?
        || contract["release_version"].as_str() != Some(prepared.version.as_str())
        || prepared.artifacts.len() != assets(contract)?.len()
    {
        return Err("prepared public artifacts have an invalid identity".into());
    }
    let mut payloads = BTreeMap::new();
    let platform = if cfg!(windows) {
        "windows-x86_64"
    } else if cfg!(target_os = "linux") {
        "linux-x86_64"
    } else {
        "macos-x86_64"
    };
    for asset in assets(contract)? {
        let id = asset["id"].as_str().ok_or("public asset id missing")?;
        let reference = prepared
            .artifacts
            .get(id)
            .ok_or("public artifact reference missing")?;
        let (binary, receipt) = Cache::read_prepared(codex_root, asset, reference)?;
        let target = asset["binary_targets"][platform]
            .as_str()
            .ok_or("public binary target missing")?;
        let receipt_target = asset["receipt_target"]
            .as_str()
            .ok_or("public receipt target missing")?;
        // Validate targets even though actual project installation happens later.
        safe_join(codex_root, target, "public target")?;
        safe_join(codex_root, receipt_target, "public receipt target")?;
        if payloads.insert(target.into(), binary).is_some()
            || payloads.insert(receipt_target.into(), receipt).is_some()
        {
            return Err("duplicate public artifact target".into());
        }
    }
    if fs::read(path).map_err(|e| e.to_string())? != before {
        return Err("public preparation changed during capture".into());
    }
    Ok(payloads)
}

pub fn prepare(
    product_root: &Path,
    codex_root: &Path,
    bootstrap_cli: Option<&Path>,
    bootstrap_identity: Option<&str>,
    bootstrap_target: Option<&Path>,
    runner: &dyn ProcessRunner,
) -> Result<Value, String> {
    if bootstrap_cli.is_some() {
        return Err("bootstrap requires an explicit running product version".into());
    }
    prepare_with_product_version(
        product_root,
        codex_root,
        bootstrap_cli,
        bootstrap_identity,
        bootstrap_target,
        "",
        runner,
    )
}

/// The executable owns its product identity. A library crate's independent
/// CARGO_PKG_VERSION is never evidence of the running CLI's product version.
pub fn prepare_with_product_version(
    product_root: &Path,
    codex_root: &Path,
    bootstrap_cli: Option<&Path>,
    bootstrap_identity: Option<&str>,
    bootstrap_target: Option<&Path>,
    running_product_version: &str,
    runner: &dyn ProcessRunner,
) -> Result<Value, String> {
    let started = Instant::now();
    let source = crate::source_projection::source_root(product_root);
    let contract_bytes = bridgeforge_core::product_manifest::project_bytes(product_root)?;
    let contract = crate::json::parse_unique_json(&contract_bytes, "public product contract")?;
    let version = fs::read_to_string(product_root.join("VERSION")).map_err(|e| e.to_string())?;
    if contract["release_version"].as_str() != Some(version.trim()) {
        return Err("public product version mismatch".into());
    }
    if let Some(binary) = bootstrap_cli {
        let binary_path = binary.canonicalize().map_err(|e| e.to_string())?;
        let bin_root = codex_root
            .join("bin")
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if binary_path.parent() != Some(bin_root.as_path()) {
            return Err("bootstrap CLI is not in the updater installation scope".into());
        }
        if binary_path
            != std::env::current_exe()
                .and_then(|p| p.canonicalize())
                .map_err(|e| e.to_string())?
        {
            return Err("bootstrap must be the verified running CLI".into());
        }
        let name = binary
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("bootstrap name missing")?;
        if name == format!("bridgeforge{}", std::env::consts::EXE_SUFFIX) {
            let proof = crate::managed_paths::read_metadata(
                &codex_root.join("bin/bridgeforge-cli-provenance.json"),
                64 * 1024,
            )?
            .ok_or("installed bootstrap CLI has no provenance")?;
            let proof = crate::json::parse_unique_json(
                proof.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&proof),
                "installed bootstrap provenance",
            )?;
            if proof["schema"] != 1
                || proof["owner"] != "bridgeforge-user-cli-v1"
                || proof["identity"].as_str() != bootstrap_identity
                || proof["binary_sha256"].as_str()
                    != Some(digest(&fs::read(binary).map_err(|e| e.to_string())?)[7..].as_ref())
            {
                return Err("installed bootstrap CLI provenance changed".into());
            }
        } else {
            let operation = name
                .strip_prefix(".bridgeforge-stage-")
                .and_then(|n| n.strip_suffix(".exe"))
                .ok_or("bootstrap is not an updater stage")?;
            if operation.len() != 32 || !operation.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("invalid bootstrap operation identity".into());
            }
        }
        if version.trim() != running_product_version {
            return Err("bootstrap CLI version mismatch".into());
        }
        let result = crate::runtime::build_environment(&source, runner);
        let actual = result.receipt.as_ref().and_then(|r| r["identity"].as_str());
        if result.code != 0 || actual.is_none() || actual != bootstrap_identity {
            return Err("bootstrap compiler/source identity changed".into());
        }
    } else if bootstrap_identity.is_some() {
        return Err("bootstrap identity requires a CLI".into());
    }
    let observed = crate::build_metrics::Observed {
        runner,
        metrics: Default::default(),
    };
    // A cache scope is the user, not the downstream project. Build inputs are
    // captured and checked by the shared builder; no source directory is installed.
    if let Some(target) = bootstrap_target {
        if bootstrap_cli.is_none() || !target.is_dir() {
            return Err("bootstrap build directory is invalid".into());
        }
        let canonical = target.canonicalize().map_err(|e| e.to_string())?;
        let temporary = std::env::temp_dir()
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let name = target
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("bootstrap directory name missing")?;
        let operation = name
            .strip_prefix("bridgeforge-cli-build-")
            .ok_or("bootstrap directory is not an updater build")?;
        if canonical.parent() != Some(temporary.as_path())
            || operation.len() != 32
            || !operation.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("bootstrap build directory is outside the updater temporary scope".into());
        }
        safe_join(target, "release", "bootstrap build directory")?;
    }
    let (_, _, references) = crate::generated_assets::prepare_public(
        product_root,
        codex_root,
        &contract,
        &observed,
        bootstrap_cli,
        bootstrap_target,
    )?;
    let prepared = Prepared {
        schema: 1,
        owner: OWNER.into(),
        input: input(&contract)?,
        version: version.trim().into(),
        artifacts: references,
    };
    let path = prepared_path(codex_root, &contract)?;
    if path.exists() {
        let old = crate::json::parse_unique_json(
            &crate::managed_paths::read_metadata(&path, 128 * 1024)?
                .ok_or("public receipt disappeared")?,
            "old public receipt",
        )?;
        if old["owner"] != OWNER || old["schema"] != 1 {
            return Err("unowned public receipt is preserved".into());
        }
    }
    if bridgeforge_core::product_manifest::project_bytes(product_root)?
        != contract_bytes
    {
        return Err("public contract changed during preparation".into());
    }
    crate::persistence::atomic_write_json(&path, &prepared).map_err(|e| e.to_string())?;
    prune_receipts(&path)?;
    // Verify exact captured bytes once, and return their location only after all
    // outputs exist. Project Apply only consumes this receipt, never builds.
    let payloads = read(&path, &contract)?;
    let hashes: BTreeMap<_, _> = payloads
        .iter()
        .map(|(p, b)| (p.clone(), digest(b)))
        .collect();
    Ok(
        json!({"schema":1,"status":"prepared","version":version.trim(),"receipt":path,
        "input":prepared.input,"files":hashes,"build_metrics":observed.metrics.into_inner(),
        "elapsed_ms":started.elapsed().as_millis()}),
    )
}

fn prune_receipts(current: &Path) -> Result<(), String> {
    let directory = current.parent().ok_or("public receipt parent missing")?;
    let mut owned = Vec::new();
    for item in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let path = item.path();
        if path == current {
            continue;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        let Some(key) = name.strip_suffix(".json") else {
            continue;
        };
        if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let Ok(Some(bytes)) = crate::managed_paths::read_metadata(&path, 128 * 1024) else {
            continue;
        };
        let Ok(value) = crate::json::parse_unique_json(&bytes, "public receipt cleanup") else {
            continue;
        };
        let Ok(record) = serde_json::from_value::<Prepared>(value) else {
            continue;
        };
        if record.owner != OWNER || record.schema != 1 || record.input != format!("sha256:{key}") {
            continue;
        }
        owned.push((
            item.metadata()
                .map_err(|e| e.to_string())?
                .modified()
                .map_err(|e| e.to_string())?,
            path,
            bytes,
        ));
    }
    owned.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path, bytes) in owned.into_iter().skip(7) {
        if crate::managed_paths::read_metadata(&path, 128 * 1024)?.as_deref()
            == Some(bytes.as_slice())
        {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn outcome(result: Result<Value, String>) -> CommandOutcome {
    match result {
        Ok(value) => CommandOutcome::with_receipt(value),
        Err(error) => CommandOutcome::blocked(format!("[public-artifacts] {error}\n")),
    }
}
