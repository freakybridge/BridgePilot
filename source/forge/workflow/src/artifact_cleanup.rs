//! Cleanup eligibility uses the adopted layout contract, never a migration guess.
use crate::{CommandOutcome, ProcessRequest, ProcessRunner};
use bridgeforge_core::{
    artifact_cleanup::{BuildPolicy, Plan},
    artifact_paths,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub fn command(args: &[String], runner: &dyn ProcessRunner) -> CommandOutcome {
    command_with_contract(args, runner, &|_, contract| Ok(contract.clone()))
}

/// The factory adds its private executable through its owning manifest adapter.
pub fn command_with_contract(
    args: &[String],
    runner: &dyn ProcessRunner,
    contract: &dyn Fn(&Path, &Value) -> Result<Value, String>,
) -> CommandOutcome {
    let result = (|| {
        let mut root = None;
        let mut preview = false;
        let mut index = 1;
        while index < args.len() {
            match args[index].as_str() {
                "--root" if root.is_none() => {
                    index += 1;
                    let value = args
                        .get(index)
                        .filter(|v| !v.starts_with('-'))
                        .ok_or("cleanup --root requires a path")?;
                    root = Some(PathBuf::from(value));
                }
                "--check" | "--dry-run" | "--preview" if !preview => preview = true,
                "--json" => (),
                _ => {
                    return Err(format!(
                        "unknown or duplicate cleanup argument: {}",
                        args[index]
                    ));
                }
            }
            index += 1;
        }
        let root = root.ok_or("cleanup requires an explicit --root project path")?;
        eligible(&root, runner)?;
        let plan = Plan::capture_with_build(&root, build_policy_with_contract(&root, contract)?)?;
        let report = if preview {
            plan.preview()
        } else {
            // Recheck identity and journals immediately before opening the full
            // deletion handle set. The executor rechecks paths under those handles.
            eligible(&root, runner)?;
            plan.execute()
        };
        let failed = !report.failures.is_empty();
        let mut outcome =
            CommandOutcome::with_receipt(serde_json::to_value(report).map_err(|e| e.to_string())?);
        if failed {
            outcome.code = crate::EXIT_BLOCKED;
        }
        Ok(outcome)
    })();
    result.unwrap_or_else(|error: String| CommandOutcome {
        code:crate::EXIT_BLOCKED,
        receipt:Some(json!({"schema":1,"status":"blocked","deleted_files":0,"deleted_directories":0,"failures":[error]})),
        ..CommandOutcome::default()
    })
}

#[cfg(all(test, bridgeforge_factory_tests))]
fn build_policy(root: &Path) -> Result<BuildPolicy, String> {
    build_policy_with_contract(root, &|_, contract| Ok(contract.clone()))
}

fn build_policy_with_contract(
    root: &Path,
    transform: &dyn Fn(&Path, &Value) -> Result<Value, String>,
) -> Result<BuildPolicy, String> {
    use bridgeforge_checks::artifact_layout::{CONFIG, CleanupRole};
    let canonical = root.canonicalize().map_err(|e| e.to_string())?;
    let root = canonical.as_path();
    let path = crate::managed_paths::safe_join(root, CONFIG, "cleanup output roles")?;
    let bytes = crate::managed_paths::read_metadata(&path, 16 * 1024 * 1024)?;
    let mut policy = BuildPolicy::default();
    let contract_path = ".codex/managed-skeleton.json";
    let contract_bytes = crate::managed_paths::read_metadata(
        &crate::managed_paths::safe_join(root, contract_path, "cleanup products")?,
        16 * 1024 * 1024,
    )?
    .ok_or("cleanup product contract missing")?;
    let contract = transform(
        root,
        &crate::json::parse_unique_json(&contract_bytes, "cleanup products")?,
    )?;
    let platform = bridgeforge_core::build_receipt::platform_key()?;
    for asset in contract["generated_assets"]
        .as_array()
        .ok_or("cleanup generated assets missing")?
    {
        for value in [&asset["binary_targets"][platform], &asset["receipt_target"]] {
            let path = value
                .as_str()
                .ok_or("cleanup generated product path missing")?;
            crate::managed_paths::safe_join(root, path, "cleanup generated product")?;
            if artifact_paths::within(path, artifact_paths::BUILD) {
                policy.keep.push(path.to_owned());
                policy.directories.insert(path.to_owned(), false);
            }
        }
    }
    policy
        .witnesses
        .insert(contract_path.into(), Some(contract_bytes));
    let hook_bytes =
        crate::managed_paths::read_metadata(&root.join(".codex/hooks.json"), 16 * 1024 * 1024)?;
    let registry_bytes = crate::managed_paths::read_metadata(
        &root.join(crate::project_hooks::REGISTRY_PATH),
        16 * 1024 * 1024,
    )?;
    let hooks = crate::project_hooks::with_registry(
        &hook_bytes
            .as_ref()
            .map(|b| crate::json::parse_unique_json(b, "cleanup hooks"))
            .transpose()?
            .unwrap_or_else(|| json!({})),
        registry_bytes.as_deref(),
    )?;
    policy
        .witnesses
        .insert(".codex/hooks.json".into(), hook_bytes);
    policy
        .witnesses
        .insert(crate::project_hooks::REGISTRY_PATH.into(), registry_bytes);
    for hook in crate::project_hooks::hooks(&hooks)? {
        policy.directories.insert(hook.binary(), false);
        policy.directories.insert(hook.receipt(), false);
        policy.keep.extend([hook.binary(), hook.receipt()]);
    }
    if let Some(bytes) = &bytes {
        for output in bridgeforge_checks::artifact_layout::parse(bytes)?.outputs {
            let path = output
                .path
                .replace('\\', "/")
                .trim_end_matches('/')
                .to_owned();
            if output.kind == artifact_paths::Kind::Build {
                if policy
                    .directories
                    .insert(path.clone(), output.directory)
                    .is_some_and(|prior| prior != output.directory)
                {
                    return Err(format!("conflicting cleanup output type: {path}"));
                }
            }
            match output.cleanup {
                Some(CleanupRole::Keep) => policy.keep.push(path),
                Some(CleanupRole::Cache | CleanupRole::History) => policy.remove.push(path),
                None => (),
            }
        }
    }
    policy.witnesses.insert(CONFIG.into(), bytes);
    Ok(policy)
}

fn eligible(root: &Path, runner: &dyn ProcessRunner) -> Result<(), String> {
    crate::managed_paths::safe_join(
        root,
        ".codex/managed-skeleton.json",
        "cleanup project identity",
    )?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut request = ProcessRequest::new("git", &root);
    request.args = ["rev-parse", "--show-toplevel"].map(Into::into).to_vec();
    request.env_remove = std::env::vars_os()
        .map(|(k, _)| k)
        .filter(|k| k.to_string_lossy().starts_with("GIT_"))
        .collect();
    request.timeout = Duration::from_secs(15);
    let output = runner.run(&request).map_err(|e| e.to_string())?;
    if output.code != 0 || output.timed_out {
        return Err("cleanup requires an available Git worktree root".into());
    }
    let actual = PathBuf::from(
        std::str::from_utf8(&output.stdout)
            .map_err(|e| e.to_string())?
            .trim(),
    )
    .canonicalize()
    .map_err(|e| e.to_string())?;
    if actual != root {
        return Err("cleanup --root must be the exact Git project root".into());
    }
    let contract_path = root.join(".codex/managed-skeleton.json");
    let bytes = crate::managed_paths::read_metadata(&contract_path, 16 * 1024 * 1024)?
        .ok_or("cleanup requires an adopted managed layout")?;
    let contract: Value = crate::json::parse_unique_json(&bytes, "cleanup layout identity")?;
    if contract["artifact_layout"] != artifact_paths::CONTRACT {
        return Err(
            "legacy or unknown artifact layout; complete managed migration before cleanup".into(),
        );
    }
    crate::baseline::verify_read_only(&root, None, false)?;
    if !crate::project_sync::legacy_artifact_retirements(&root)?.is_empty() {
        return Err("legacy artifacts require migration before cleanup".into());
    }
    let findings = bridgeforge_checks::artifact_layout::worktree(&root);
    if !findings.errors.is_empty() {
        return Err(format!(
            "artifact layout blocks cleanup: {}",
            findings
                .errors
                .iter()
                .map(|e| format!("{}: {}", e.path, e.message))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    crate::release::schema4::development::require_completed_transaction(&root)?;
    crate::git_sync::require_no_cleanup_commit_guard(&root)?;
    Ok(())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/workflow_artifact_cleanup.rs"]
mod tests;
