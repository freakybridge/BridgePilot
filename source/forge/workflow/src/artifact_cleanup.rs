//! Cleanup uses explicit current-product paths and pinned native evidence.
//! Retired project contracts and registries are never consulted or recreated.
use crate::{CommandOutcome, ProcessRequest, ProcessRunner};
use bridgeforge_core::{
    artifact_cleanup::{BuildPolicy, Plan},
    artifact_paths, managed_paths,
};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub fn command(args: &[String], runner: &dyn ProcessRunner) -> CommandOutcome {
    command_with_products(args, runner, &[])
}

/// The factory supplies only its own exact infrastructure products.
pub fn command_with_products(
    args: &[String],
    runner: &dyn ProcessRunner,
    products: &[String],
) -> CommandOutcome {
    let mut preview = false;
    let result = (|| -> Result<CommandOutcome, String> {
        let mut root = None;
        let mut reviewed = false;
        let mut keep = products.to_vec();
        let mut evidence = Vec::new();
        let mut index = 1;
        while index < args.len() {
            let flag = &args[index];
            match flag.as_str() {
                "--root" | "--keep" | "--evidence" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .filter(|v| !v.starts_with('-'))
                        .ok_or_else(|| format!("{flag} requires a value"))?;
                    match flag.as_str() {
                        "--root" if root.is_none() => root = Some(PathBuf::from(value)),
                        "--keep" => keep.push(value.clone()),
                        "--evidence" => evidence.push(value.clone()),
                        _ => return Err("duplicate --root".into()),
                    }
                }
                "--check" | "--dry-run" | "--preview" if !preview => preview = true,
                "--products-reviewed" if !reviewed => reviewed = true,
                "--json" => (),
                _ => return Err(format!("unknown or duplicate cleanup argument: {flag}")),
            }
            index += 1;
        }
        if !reviewed || evidence.is_empty() {
            return Err("review current build/startup products and runtime dependencies, then pass --products-reviewed, --evidence <native file> and each exact --keep <project-relative path>".into());
        }
        let root = root.ok_or("cleanup requires an explicit --root project path")?;
        managed_paths::safe_join(&root, artifact_paths::RUNTIME, "cleanup project root")?;
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        eligible(&root, runner)?;
        let mut policy = BuildPolicy {
            keep,
            ..BuildPolicy::default()
        };
        for relative in evidence {
            let relative = relative.replace('\\', "/");
            let path = managed_paths::safe_join(&root, &relative, "cleanup native evidence")?;
            let path = path
                .canonicalize()
                .map_err(|e| format!("cleanup native evidence {relative}: {e}"))?;
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| "cleanup evidence escapes project root")?
                .to_string_lossy()
                .replace('\\', "/");
            if [artifact_paths::BUILD, artifact_paths::RUNTIME]
                .iter()
                .any(|p| artifact_paths::within(&relative, p))
            {
                return Err(
                    "cleanup evidence must be outside the removable .build and .runtime trees"
                        .into(),
                );
            }
            let path = managed_paths::safe_join(&root, &relative, "cleanup native evidence")?;
            let bytes = managed_paths::read_metadata(&path, 16 * 1024 * 1024)?
                .ok_or_else(|| format!("cleanup native evidence missing: {relative}"))?;
            policy.witnesses.insert(relative, Some(bytes));
        }
        // The native driver owns these fixed infrastructure paths, not its entire bin.
        policy.keep.extend([
            crate::project_release::PROGRAM.into(),
            crate::project_release::RECEIPT.into(),
        ]);
        eprintln!("[cleanup] scanning {}", root.display());
        let scan_started = Instant::now();
        let plan = Plan::capture_with_build(&root, policy)?;
        let scan_ms = scan_started.elapsed().as_millis();
        let planned = plan.preview();
        eprintln!("[cleanup] scan finished: {} files, {} directories, {} Junctions, {scan_ms} ms",
            planned.planned_files, planned.planned_directories, planned.planned_junctions);
        let execution_started = Instant::now();
        let report = if preview {
            planned
        } else {
            eligible(&root, runner)?;
            eprintln!("[cleanup] checking target handles and deleting");
            plan.execute()
        };
        let execution_ms = execution_started.elapsed().as_millis();
        let failed = !report.failures.is_empty();
        let mut receipt = serde_json::to_value(report).map_err(|e| e.to_string())?;
        receipt["products_reviewed"] = json!(true);
        receipt["timings_ms"] = json!({"scan": scan_ms, "execution": execution_ms});
        let mut out = CommandOutcome::with_receipt(receipt);
        if failed {
            out.code = crate::EXIT_BLOCKED;
        }
        Ok(out)
    })();
    result.unwrap_or_else(|error| CommandOutcome {
        code: crate::EXIT_BLOCKED,
        receipt: Some(json!({"schema":1,"status":"blocked","read_only":preview,"deleted_files":0,"deleted_directories":0,"failures":[error]})),
        ..CommandOutcome::default()
    })
}

fn git(root: &Path, runner: &dyn ProcessRunner, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut request = ProcessRequest::new("git", root);
    request.args = ["-c", "core.fsmonitor=false"]
        .iter()
        .chain(args)
        .map(Into::into)
        .collect();
    request.env_remove = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| key.to_string_lossy().starts_with("GIT_"))
        .collect();
    request.env.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
    request.timeout = Duration::from_secs(15);
    let output = runner.run(&request).map_err(|e| e.to_string())?;
    if output.code != 0 || output.timed_out {
        return Err(
            "cleanup requires an available Git worktree and read-only path verification".into(),
        );
    }
    Ok(output.stdout)
}

fn eligible(root: &Path, runner: &dyn ProcessRunner) -> Result<(), String> {
    let actual = git(root, runner, &["rev-parse", "--show-toplevel"])?;
    let actual = PathBuf::from(
        std::str::from_utf8(&actual)
            .map_err(|e| e.to_string())?
            .trim(),
    )
    .canonicalize()
    .map_err(|e| e.to_string())?;
    if actual != root {
        return Err("cleanup --root must be the exact Git project root".into());
    }
    if !git(
        root,
        runner,
        &["ls-files", "-z", "--", ".runtime", ".build", ".deps"],
    )?
    .is_empty()
    {
        return Err("tracked project files exist inside artifact roots; resolve their ownership before cleanup".into());
    }
    crate::template_sync::require_no_pending_development(root)?;
    crate::git_sync::require_no_cleanup_commit_guard(root)?;
    Ok(())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/workflow_artifact_cleanup.rs"]
mod tests;
