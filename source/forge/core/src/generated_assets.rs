//! Shared isolated builds. Returns payloads; callers own installation and transactions.
use crate::{ProcessRequest, ProcessRunner, build_inputs, managed_paths::safe_join};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn sha_raw(payload: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(payload))
}

fn json_contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => expected.iter().all(|(key, value)| {
            actual
                .get(key)
                .is_some_and(|item| json_contains(item, value))
        }),
        (Value::Array(actual), Value::Array(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(left, right)| json_contains(left, right))
        }
        _ => actual == expected,
    }
}

/// Receipt, bytes and live self-test must all agree before an installed hit.
pub fn installed_current(
    root: &Path,
    asset: &Value,
    binary: &Path,
    runner: &dyn ProcessRunner,
) -> bool {
    if crate::build_receipt::verify_generated(root, asset).is_err() {
        return false;
    }
    let Some(args) = asset["self_test"]["args"].as_array().and_then(|args| {
        args.iter()
            .map(|arg| arg.as_str().map(OsString::from))
            .collect::<Option<Vec<_>>>()
    }) else {
        return false;
    };
    let mut request = ProcessRequest::new(binary.as_os_str(), root);
    request.args = args;
    request.timeout = Duration::from_secs(60);
    let Ok(output) = runner.run(&request) else {
        return false;
    };
    let Ok(actual) = serde_json::from_slice::<Value>(&output.stdout) else {
        return false;
    };
    !output.timed_out
        && output.code == 0
        && asset["self_test"]["expected_json"].is_object()
        && json_contains(&actual, &asset["self_test"]["expected_json"])
        && crate::build_receipt::verify_generated(root, asset).is_ok()
}

pub fn generated_writes(
    source_base: &Path,
    source_key: &str,
    project_root: &Path,
    contract: &Value,
    runner: &dyn ProcessRunner,
) -> Result<(BTreeMap<PathBuf, Vec<u8>>, Vec<Value>), String> {
    generated_writes_internal(
        source_base,
        source_key,
        project_root,
        contract,
        runner,
        false,
        None,
        None,
        None,
        None,
        None,
    )
}

pub fn generated_writes_reusing(
    source_base: &Path,
    source_key: &str,
    project_root: &Path,
    contract: &Value,
    runner: &dyn ProcessRunner,
) -> Result<(BTreeMap<PathBuf, Vec<u8>>, Vec<Value>), String> {
    generated_writes_internal(
        source_base,
        source_key,
        project_root,
        contract,
        runner,
        true,
        None,
        None,
        None,
        None,
        None,
    )
}

/// Reuse snapshots already captured for the same transaction's cache lookup.
/// Original bytes, isolated bytes, source ownership and every asset contract
/// are still verified. The caller retains ownership of the supplied snapshots.
pub fn generated_writes_captured(
    source_base: &Path,
    source_key: &str,
    project_root: &Path,
    contract: &Value,
    runner: &dyn ProcessRunner,
    captured: &BTreeMap<(PathBuf, Option<String>), &build_inputs::BuildInputs>,
) -> Result<(BTreeMap<PathBuf, Vec<u8>>, Vec<Value>), String> {
    generated_writes_internal(
        source_base,
        source_key,
        project_root,
        contract,
        runner,
        false,
        None,
        None,
        None,
        None,
        Some(captured),
    )
}

pub fn prepare_public(
    source_base: &Path,
    codex_root: &Path,
    contract: &Value,
    runner: &dyn ProcessRunner,
    bootstrap_cli: Option<&Path>,
    bootstrap_target: Option<&Path>,
) -> Result<
    (
        BTreeMap<PathBuf, Vec<u8>>,
        Vec<Value>,
        BTreeMap<String, Value>,
    ),
    String,
> {
    let mut references = BTreeMap::new();
    let (writes, receipts) = generated_writes_internal(
        source_base,
        "source_root",
        codex_root,
        contract,
        runner,
        true,
        Some(codex_root),
        bootstrap_cli,
        bootstrap_target,
        Some(&mut references),
        None,
    )?;
    Ok((writes, receipts, references))
}

fn generated_writes_internal(
    source_base: &Path,
    source_key: &str,
    project_root: &Path,
    contract: &Value,
    runner: &dyn ProcessRunner,
    reuse: bool,
    shared_root: Option<&Path>,
    bootstrap_cli: Option<&Path>,
    bootstrap_target: Option<&Path>,
    mut public_references: Option<&mut BTreeMap<String, Value>>,
    captured: Option<&BTreeMap<(PathBuf, Option<String>), &build_inputs::BuildInputs>>,
) -> Result<(BTreeMap<PathBuf, Vec<u8>>, Vec<Value>), String> {
    let generated = contract["generated_assets"]
        .as_array()
        .ok_or("generated_assets is missing")?;
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let target_dir = if shared_root.is_some() {
        std::env::temp_dir().join(format!(
            "bridgeforge-generated-{}-{token}",
            std::process::id()
        ))
    } else {
        crate::artifact_paths::temporary_build(project_root, "g")?
    };
    fs::create_dir_all(&target_dir).map_err(|error| error.to_string())?;
    let result = (|| {
        let mut writes = BTreeMap::new();
        let mut receipts = Vec::new();
        let mut snapshots = BTreeMap::<(PathBuf, Option<String>), build_inputs::BuildInputs>::new();
        let mut cache = None;
        let mut cache_attempted = false;
        for item in generated {
            let id = item["id"].as_str().ok_or("generated asset id is missing")?;
            let source_root = safe_join(
                source_base,
                item[source_key]
                    .as_str()
                    .ok_or_else(|| format!("generated {source_key} is missing"))?,
                "generated source root",
            )?;
            let snapshot_key = (
                source_root.clone(),
                item.get("source_projection")
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .ok_or("source_projection must be a string")
                    })
                    .transpose()?,
            );
            let snapshot = target_dir.join(format!("s{}", snapshots.len()));
            // Reuse a verified snapshot for the Hook/CLI sharing one workspace.
            let provided = captured
                .and_then(|inputs| inputs.get(&snapshot_key))
                .copied();
            if provided.is_none() && !snapshots.contains_key(&snapshot_key) {
                let inputs = build_inputs::BuildInputs::capture(&source_root, snapshot, item)?;
                snapshots.insert(snapshot_key.clone(), inputs);
            }
            let inputs = provided
                .or_else(|| snapshots.get(&snapshot_key))
                .ok_or("generated snapshot is missing")?;
            inputs.verify_source(&source_root, snapshot_key.1.as_deref())?;
            if bootstrap_cli.is_some()
                && crate::artifact_cache::environment(&source_root, runner)
                    != crate::artifact_cache::environment(&inputs.snapshot, runner)
            {
                return Err("bootstrap and isolated compiler environments differ".into());
            }
            // Validate each asset's recipe/self-test too, including shared workspaces.
            let binary = item["build"]["binary_name"]
                .as_str()
                .ok_or("missing binary_name")?;
            let recipe = crate::manifest::generated_build_recipe(binary);
            let recipe_sha = crate::manifest::canonical_sha(&recipe)?;
            let self_test_sha = crate::manifest::canonical_sha(&item["self_test"])?;
            if item["build"] != recipe
                || item["build_recipe_sha256"] != recipe_sha
                || item["self_test_sha256"] != self_test_sha
                || item["source_tree_sha256"] != inputs.hashes["source_tree_sha256"]
                || item["lockfile_sha256"] != inputs.hashes["lockfile_sha256"]
                || item["manifest"] != "Cargo.toml"
                || item["lockfile"] != "Cargo.lock"
                || !matches!(
                    binary,
                    "bridgeforge" | "bridgeforge-hook" | "bridgeforge-factory"
                )
                || !item["self_test"]["expected_json"].is_object()
            {
                return Err("generated build input contract mismatch".into());
            }
            if reuse
                && (runner.artifact_cache_enabled() || shared_root.is_some())
                && !cache_attempted
            {
                cache = if let Some(root) = shared_root {
                    Some(crate::artifact_cache::Cache::open_shared(
                        root,
                        &inputs.snapshot,
                        runner,
                    )?)
                } else {
                    crate::artifact_cache::Cache::open_temporary(
                        project_root,
                        &inputs.snapshot,
                        runner,
                    )
                };
                cache_attempted = true;
            }
            if let Some((payload, receipt)) = cache.as_mut().and_then(|c| c.load(item, runner)) {
                if let Some(references) = public_references.as_deref_mut() {
                    references.insert(
                        id.into(),
                        cache
                            .as_ref()
                            .ok_or("public cache missing")?
                            .prepared_reference(item)?,
                    );
                }
                inputs.verify_unchanged()?;
                let platform = if cfg!(windows) {
                    "windows-x86_64"
                } else if cfg!(target_os = "linux") {
                    "linux-x86_64"
                } else {
                    "macos-x86_64"
                };
                writes.insert(
                    safe_join(
                        project_root,
                        item["binary_targets"][platform]
                            .as_str()
                            .ok_or("missing binary target")?,
                        "cached binary",
                    )?,
                    payload,
                );
                receipts.push(serde_json::from_slice(&receipt).map_err(|e| e.to_string())?);
                writes.insert(
                    safe_join(
                        project_root,
                        item["receipt_target"]
                            .as_str()
                            .ok_or("missing receipt target")?,
                        "cached receipt",
                    )?,
                    receipt,
                );
                continue;
            }
            let manifest = inputs.snapshot.join("Cargo.toml");
            let binary_name = item["build"]["binary_name"]
                .as_str()
                .ok_or("generated binary_name is missing")?;
            // One target per isolated workspace shares dependency compilation.
            // Remove this asset's final executable first: a successful no-op
            // runner must never be able to validate the previous asset's output.
            let imported = if binary_name == "bridgeforge" {
                bootstrap_cli
            } else {
                None
            };
            let output_dir = if imported.is_some() {
                inputs.snapshot.with_extension("bootstrap-test")
            } else {
                bootstrap_target
                    .map(Path::to_path_buf)
                    .or_else(|| inputs.target.clone())
                    .unwrap_or_else(|| inputs.snapshot.with_extension("t"))
            };
            let built = output_dir.join("release").join(if cfg!(windows) {
                format!("{binary_name}.exe")
            } else {
                binary_name.into()
            });
            match fs::remove_file(&built) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "cannot clear generated binary before build: {error}"
                    ));
                }
            }
            if let Some(imported) = imported {
                // The updater binds this bootstrap to the exact source and
                // compiler identity. Its bytes still undergo the asset self-test.
                let payload = fs::read(imported).map_err(|e| e.to_string())?;
                fs::create_dir_all(built.parent().ok_or("bootstrap target parent missing")?)
                    .map_err(|e| e.to_string())?;
                fs::write(&built, &payload).map_err(|e| e.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&built, fs::Permissions::from_mode(0o700))
                        .map_err(|e| e.to_string())?;
                }
            } else {
                let mut request = ProcessRequest::new("cargo", &inputs.snapshot);
                request.args = vec![
                    OsString::from("build"),
                    OsString::from("--locked"),
                    OsString::from("--profile"),
                    OsString::from("release"),
                    OsString::from("--manifest-path"),
                    manifest.into_os_string(),
                    OsString::from("--target-dir"),
                    output_dir.clone().into_os_string(),
                    OsString::from("--bin"),
                    OsString::from(binary_name),
                ];
                request.timeout = Duration::from_secs(900);
                let output = runner.run(&request).map_err(|error| error.to_string())?;
                if output.timed_out || output.code != 0 {
                    return Err(format!(
                        "generated asset build failed: {id}: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }
            }
            let platform = if cfg!(windows) {
                "windows-x86_64"
            } else if cfg!(target_os = "linux") {
                "linux-x86_64"
            } else {
                "macos-x86_64"
            };
            let target = safe_join(
                project_root,
                item["binary_targets"][platform]
                    .as_str()
                    .ok_or("generated binary target is missing")?,
                "generated binary target",
            )?;
            let payload =
                fs::read(&built).map_err(|error| format!("cannot read built binary: {error}"))?;
            let self_test_args = item["self_test"]["args"]
                .as_array()
                .ok_or("generated self_test args are missing")?
                .iter()
                .map(|argument| {
                    argument
                        .as_str()
                        .map(OsString::from)
                        .ok_or("generated self_test arg must be text")
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut self_test =
                ProcessRequest::new(built.clone().into_os_string(), &inputs.snapshot);
            self_test.args = self_test_args;
            self_test.timeout = Duration::from_secs(60);
            let tested = runner.run(&self_test).map_err(|error| error.to_string())?;
            if tested.timed_out || tested.code != 0 {
                return Err(format!("generated asset self-test failed: {id}"));
            }
            let actual: Value = serde_json::from_slice(&tested.stdout)
                .map_err(|error| format!("generated asset self-test is not JSON: {id}: {error}"))?;
            if !json_contains(&actual, &item["self_test"]["expected_json"]) {
                return Err(format!("generated asset self-test contract mismatch: {id}"));
            }
            inputs.verify_unchanged()?;
            if fs::read(&built).map_err(|error| error.to_string())? != payload {
                return Err(format!("generated binary changed during self-test: {id}"));
            }
            let receipt = json!({
                "schema_version": 2,
                "generated_asset_id": id,
                "platform": platform,
                "binary_sha256": sha_raw(&payload),
                "source_tree_sha256": inputs.hashes["source_tree_sha256"],
                "lockfile_sha256": inputs.hashes["lockfile_sha256"],
                "build_recipe_sha256": recipe_sha,
                "self_test_sha256": self_test_sha,
            });
            let receipt_target = safe_join(
                project_root,
                item["receipt_target"]
                    .as_str()
                    .ok_or("receipt target is missing")?,
                "generated receipt target",
            )?;
            let mut encoded =
                serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?;
            encoded.push(b'\n');
            if let Some(cache) = cache.as_mut() {
                let stored = cache.store(item, &payload, &encoded, runner);
                if shared_root.is_some() {
                    stored?;
                }
                if let Some(references) = public_references.as_deref_mut() {
                    references.insert(id.into(), cache.prepared_reference(item)?);
                }
            }
            writes.insert(target, payload);
            writes.insert(receipt_target, encoded);
            receipts.push(receipt);
        }
        for inputs in snapshots.values() {
            inputs.verify_unchanged()?;
        }
        for inputs in captured.into_iter().flat_map(|inputs| inputs.values()) {
            inputs.verify_unchanged()?;
        }
        Ok((writes, receipts))
    })();
    let cleanup = fs::remove_dir_all(&target_dir);
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(error)) => Err(format!("cannot remove generated build directory: {error}")),
        (Err(error), _) => Err(error),
    }
}
