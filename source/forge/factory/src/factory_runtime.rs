//! Factory-only preparation, separate from downstream project adoption and Git publication.
use crate::{CommandOutcome, ProcessRunner, generated_assets, manifest, project_sync};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn identity(root: &Path) -> Result<(Value, Vec<u8>), String> {
    identity_mode(root, false)
}

fn identity_mode(root: &Path, zero_write: bool) -> Result<(Value, Vec<u8>), String> {
    if !root.join(bridgeforge_core::product_manifest::FILE).is_file() {
        return Err("factory runtime preparation requires the factory repository".into());
    }
    if root.join(".codex/.bridgeforge_codex_version").exists() {
        return Err("factory must not carry a downstream skeleton version stamp".into());
    }
    bridgeforge_core::product_manifest::load(root)?;
    // The owning product manifest and native source define builds. A stale
    // project-side contract must neither prevent repair nor be recreated.
    if zero_write && manifest::rebuild(root, true)? {
        return Err("factory manifests or dogfood are stale".into());
    }
    let bytes = manifest::render_managed_contract(root)?;
    let public: Value = crate::json::parse_unique_json(&bytes, "factory runtime contract")?;
    // Preparation owns the manifest update in its transaction. Validate the
    // freshly rendered candidate, not the installed manifest it is replacing.
    let versions = crate::factory_version::check_contract(root,&public);
    if !versions.healthy {return Err(versions.issues.join("; "));}
    let contract = manifest::runtime_contract(root, &public)?;
    // The manifests bind every product input. Rechecking also validates the
    // current source inventory, normalized source bytes and dogfood mirror.
    let mut bound: BTreeMap<String, Vec<u8>> = manifest::render_targets(root)?
        .into_iter()
        .map(|(p, bytes)| {
            Ok((
                p.strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/"),
                bytes,
            ))
        })
        .collect::<Result<_, String>>()?;
    bound.insert(
        "factory.runtime-contract".into(),
        serde_json::to_vec(&contract).map_err(|e| e.to_string())?,
    );
    Ok((
        contract,
        serde_json::to_vec(&bound).map_err(|e| e.to_string())?,
    ))
}

fn paths(root: &Path, asset: &Value) -> Result<(PathBuf, PathBuf), String> {
    let platform = if cfg!(windows) {
        "windows-x86_64"
    } else if cfg!(target_os = "linux") {
        "linux-x86_64"
    } else {
        "macos-x86_64"
    };
    let join = |value: &Value| {
        crate::managed_paths::safe_join(
            root,
            value.as_str().ok_or("missing generated target")?,
            "factory generated target",
        )
    };
    Ok((
        join(&asset["binary_targets"][platform])?,
        join(&asset["receipt_target"])?,
    ))
}

pub fn prepare(root: &Path, check: bool, runner: &dyn ProcessRunner) -> CommandOutcome {
    match prepare_inner(root, check, runner) {
        Ok(result) => CommandOutcome::with_receipt(result),
        Err(error) => CommandOutcome::blocked(format!("[factory-runtime] {error}\n")),
    }
}

fn prepare_inner(root: &Path, check: bool, runner: &dyn ProcessRunner) -> Result<Value, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let (contract, original) = identity_mode(&root, check)?;
    let legacy_retirements = project_sync::legacy_artifact_retirements(&root)?;
    let workflow_inputs=bridgeforge_skeleton::template::retired_workflow_inputs(&root)?;
    let mut retirements=legacy_retirements.clone();
    retirements.extend(workflow_inputs.iter().filter_map(|(p,b)|b.as_ref().map(|b|(p.clone(),b.clone()))));
    let assets = contract["generated_assets"]
        .as_array()
        .ok_or("missing generated assets")?;
    if check {
        if !retirements.is_empty() {
            return Err("legacy runtime still needs transactional migration".into());
        }
        // Deliberately no locks, Cargo, self-test processes, cache or receipts.
        for asset in assets {
            crate::build_receipt::verify_generated(&root, asset)?;
        }
        let public=bridgeforge_core::product_manifest::project(&root)?;
        bridgeforge_workflow::public_artifacts::read(&root.join(bridgeforge_workflow::public_artifacts::FACTORY_READY),&public)?;
        if identity_mode(&root, true)?.1 != original {
            return Err("factory inputs changed during inspection".into());
        }
        return Ok(
            json!({"schema":1,"status":"current","read_only":true,"self_test_executed":false,"reused":assets.len(),"built":0}),
        );
    }
    // Maintenance lock shared with develop, including linked worktrees.
    let _sync = crate::git_sync::lock_factory_runtime(&root, runner)?;
    let _project = project_sync::ProjectLock::acquire(&root)?;
    let _legacy_cache = crate::artifact_cache::lock_legacy(&root)?;
    if !legacy_retirements.is_empty() {
        bridgeforge_core::process_activity::legacy_clients_quiet(&root)?;
    }
    if identity(&root)?.1 != original {
        return Err("factory inputs changed before preparation".into());
    }
    if !legacy_retirements.is_empty() {
        bridgeforge_core::process_activity::legacy_clients_quiet(&root)?;
    }
    if project_sync::legacy_artifact_retirements(&root)? != legacy_retirements {
        return Err("legacy runtime changed before preparation".into());
    }
    let mut before = BTreeMap::new();
    before.extend(workflow_inputs);
    let projection = manifest::render_targets(&root)?;
    for (path, _) in &projection {
        before.insert(path.clone(), project_sync::transaction_file_state(path)?);
    }
    before.extend(
        retirements
            .iter()
            .map(|(p, bytes)| (p.clone(), Some(bytes.clone()))),
    );
    let mut binaries = BTreeSet::new();
    for path in retirements.keys() {
        if path.extension().is_some_and(|s| s == "exe") || path.extension().is_none() {
            binaries.insert(path.clone());
        }
    }
    let mut pending = Vec::new();
    for asset in assets {
        let (binary, receipt) = paths(&root, asset)?;
        before.insert(
            binary.clone(),
            project_sync::transaction_file_state(&binary)?,
        );
        before.insert(
            receipt.clone(),
            project_sync::transaction_file_state(&receipt)?,
        );
        if !generated_assets::installed_current(&root, asset, &binary, runner) {
            binaries.insert(binary);
            pending.push(asset.clone());
        }
    }
    let rebuilt = pending.len();
    let mut environments = BTreeMap::new();
    if runner.artifact_cache_enabled() {
        for asset in &pending {
            let source = match asset["source_root"].as_str() {
                Some(".") => root.clone(),
                Some(relative) => {
                    crate::managed_paths::safe_join(&root, relative, "factory source root")?
                }
                None => return Err("factory source root is missing".into()),
            };
            if !environments.contains_key(&source) {
                let environment = crate::artifact_cache::environment(&source, runner)
                    .ok_or("cannot establish the factory compiler environment")?;
                environments.insert(source, environment);
            }
        }
    }
    let mut writes: BTreeMap<_, _> = projection.into_iter().collect();
    for (path, bytes) in project_sync::legacy_artifact_relocations(&root)? {
        before.insert(path.clone(), project_sync::transaction_file_state(&path)?);
        if path.extension().is_some_and(|s| s == "exe") || path.extension().is_none() {
            binaries.insert(path.clone());
        }
        writes.insert(path, bytes);
    }
    // Keep public and private workspaces in distinct build groups even though
    // they share an installation transaction and one Git-common-dir lock.
    for private in [false, true] {
        let selected_assets = pending
            .iter()
            .filter(|asset| (asset["id"].as_str() == Some("codex.factory-cli")) == private)
            .cloned()
            .collect::<Vec<_>>();
        if selected_assets.is_empty() {
            continue;
        }
        let mut selected = contract.clone();
        selected["generated_assets"] = Value::Array(selected_assets);
        let (generated, _) = generated_assets::generated_writes_reusing(
            &root,
            "source_root",
            &root,
            &selected,
            runner,
        )?;
        for (path, bytes) in generated {
            if writes.insert(path, bytes).is_some() {
                return Err(
                    "public and private build groups overlap an installation target".into(),
                );
            }
        }
    }
    let manifest_bytes=writes.get(&root.join(bridgeforge_core::product_manifest::FILE)).ok_or("candidate product manifest missing")?;
    let public=bridgeforge_core::product_manifest::parse(manifest_bytes)?["project_skeleton"].clone();
    let platform=bridgeforge_core::build_receipt::platform_key()?;
    let mut public_payloads=BTreeMap::new();
    for asset in public["generated_assets"].as_array().ok_or("candidate public artifacts missing")? {
        for target in [asset["binary_targets"][platform].as_str().ok_or("public target missing")?,asset["receipt_target"].as_str().ok_or("public receipt target missing")?] {
            let name=Path::new(target).file_name().ok_or("public artifact filename missing")?;
            let path=root.join(bridgeforge_core::artifact_paths::FACTORY_BIN).join(name);
            let bytes=writes.get(&path).cloned().or_else(||before.get(&path).and_then(Clone::clone)).ok_or("verified native public artifact missing")?;
            public_payloads.insert(target.into(),bytes);
        }
    }
    let public_receipt=root.join(bridgeforge_workflow::public_artifacts::FACTORY_READY);
    let old_receipt=project_sync::transaction_file_state(&public_receipt)?;
    if old_receipt.as_ref().is_some_and(|b|!bridgeforge_workflow::public_artifacts::factory_ready_owned(b)) {return Err("unowned factory public receipt preserved".into());}
    before.insert(public_receipt.clone(),old_receipt);
    writes.insert(public_receipt.clone(),bridgeforge_workflow::public_artifacts::factory_ready_bytes(&public,&public_payloads)?);
    let written_files=writes.iter().filter(|(path,bytes)|before.get(*path).and_then(Option::as_deref)!=Some(bytes.as_slice())).count();
    if identity(&root)?.1 != original {
        return Err("factory inputs changed during build; installed files preserved".into());
    }
    if project_sync::legacy_artifact_retirements(&root)? != legacy_retirements {
        return Err("legacy runtime changed during build; installed files preserved".into());
    }
    for (source, expected) in &environments {
        if crate::artifact_cache::environment(source, runner).as_ref() != Some(expected) {
            return Err(
                "factory compiler environment changed during build; installed files preserved"
                    .into(),
            );
        }
    }
    for (path, value) in &before {
        if &project_sync::transaction_file_state(path)? != value {
            return Err(format!(
                "factory target changed during build: {}",
                path.display()
            ));
        }
    }
    project_sync::install_template_transaction(
        &root,
        &writes,
        &before,
        &binaries,
        &retirements,
        || {
            if !project_sync::legacy_artifact_retirements(&root)?.is_empty() {
                return Err("legacy runtime changed during installation".into());
            }
            if !legacy_retirements.is_empty() {
                project_sync::verify_legacy_artifact_retirement(&root,legacy_retirements.contains_key(&root.join(".codex/bin/.gitignore")))?;
                bridgeforge_core::process_activity::legacy_clients_quiet(&root)?;
            }
            if identity(&root)?.1 != original {
                return Err("factory inputs changed during installation".into());
            }
            bridgeforge_skeleton::template::verify_workflow_retirements(&root)?;
            for asset in assets {
                let (binary, _) = paths(&root, asset)?;
                if !generated_assets::installed_current(&root, asset, &binary, runner) {
                    return Err(format!(
                        "installed factory asset verification failed: {}",
                        asset["id"]
                    ));
                }
            }
            bridgeforge_workflow::public_artifacts::read(&public_receipt,&public)?;
            if identity(&root)?.1 != original {
                return Err("factory inputs changed during final verification".into());
            }
            Ok(())
        },
    )?;
    let retained_directories=bridgeforge_skeleton::template::prune_workflow_directories(&root);
    Ok(
        json!({"schema":1,"status":if rebuilt == 0 {"current"} else {"prepared"},
        "read_only":false,"self_test_executed":true,"reused":assets.len()-rebuilt,
        "prepared":rebuilt,"written_files":written_files,"public_artifacts":public_receipt,"retired_files":retirements.len(),"retained_directories":retained_directories,"build_policy":"verified-cache-or-locked-cargo"}),
    )
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_factory_runtime.rs"]
mod tests;
