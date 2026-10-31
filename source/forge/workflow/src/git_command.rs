use crate::{CommandOutcome, EXIT_BLOCKED, ProjectContext};
use serde_json::Value;
use std::{fs, path::PathBuf};
fn blocked(label: &str, error: impl std::fmt::Display) -> CommandOutcome {
    CommandOutcome::blocked(format!("[{label}] BLOCKED: {error}\n"))
}
fn value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|p| p[0] == flag).map(|p| p[1].clone())
}
fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|s| s == flag)
}
pub fn run(args: &[String], runner: &dyn bridgeforge_core::ProcessRunner) -> CommandOutcome {
    run_with_identity(args, runner, None)
}

pub fn run_with_identity(
    args: &[String],
    runner: &dyn bridgeforge_core::ProcessRunner,
    identity: Option<Value>,
) -> CommandOutcome {
    if args.iter().any(|arg| {
        matches!(
            arg.split('=').next(),
            Some("--prepare-release" | "--development-status")
        )
    }) {
        return blocked(
            "git-sync",
            "prepared-release commands are retired; use develop for validation and git-sync --release for publishing",
        );
    }
    if has(args, "--check") || has(args, "--dry-run") {
        return blocked(
            "git-sync",
            "--check/--dry-run cannot execute synchronization or preparation; use --release-status or --release-preview",
        );
    }
    let root = value(args, "--root").map(PathBuf::from);
    let context = match ProjectContext::discover(root.as_deref()) {
        Ok(value) => value,
        Err(error) => return blocked("git-sync", error),
    };
    if let Err(error) = crate::baseline::require_supported_project(context.root()) {
        return blocked("git-sync", error);
    }
    let modes = [
        "--preflight",
        "--sync-remote",
        "--setup-status",
        "--setup-plan",
        "--setup-apply",
        "--release",
        "--release-preview",
        "--release-status",
        "--migration-plan",
        "--migration-apply",
        "--repair-baseline-plan",
        "--repair-baseline-apply",
    ]
    .iter()
    .filter(|flag| has(args, flag))
    .count();
    if modes > 1 {
        return blocked("git-sync", "release modes are mutually exclusive");
    }
    let release_request = bridgeforge_lifecycle::release::schema4::Request {
        analysis_file: value(args, "--release-analysis-file").map(PathBuf::from),
    };
    if has(args, "--repair-baseline-plan") || has(args, "--repair-baseline-apply") {
        if value(args, "--strategy").is_some() || value(args, "--setup-config-file").is_some() {
            return blocked(
                "git-sync",
                "baseline repair cannot change configuration strategy or accept a proposal",
            );
        }
        return if has(args, "--repair-baseline-plan") {
            bridgeforge_lifecycle::release::schema4::repair_baseline_plan(context.root(), runner)
        } else {
            bridgeforge_lifecycle::release::schema4::repair_baseline_apply(
                context.root(),
                runner,
                &value(args, "--repair-baseline-fingerprint").unwrap_or_default(),
                has(args, "--confirmed-choice"),
            )
        };
    }
    if has(args, "--migration-plan") {
        return bridgeforge_lifecycle::release::schema4::configuration_plan(
            context.root(),
            runner,
            value(args, "--setup-config-file").as_deref(),
            value(args, "--strategy").as_deref(),
        );
    }
    if has(args, "--migration-apply") {
        return bridgeforge_lifecycle::release::schema4::configuration_apply(
            context.root(),
            runner,
            value(args, "--setup-config-file").as_deref(),
            value(args, "--strategy").as_deref(),
            &value(args, "--migration-fingerprint").unwrap_or_default(),
            has(args, "--confirmed-choice"),
        );
    }
    if has(args, "--preflight") {
        let mut outcome = crate::git_sync::preflight(context.root(), runner);
        if let (Some(receipt), Some(identity)) = (
            outcome.receipt.as_mut().and_then(|r| r.as_object_mut()),
            identity,
        ) {
            receipt.insert("tool".into(), identity);
        }
        return outcome;
    }
    if has(args, "--sync-remote") {
        if has(args, "--skip-fetch")
            || has(args, "--skip-push")
            || has(args, "--release-analysis-file")
        {
            return blocked(
                "git-sync",
                "--sync-remote only integrates remote changes; do not supply skip flags or a release analysis",
            );
        }
        return crate::git_sync::sync_remote(
            context.root(),
            runner,
            crate::git_sync::GitSyncOptions {
                remote: value(args, "--remote").unwrap_or_else(|| "origin".into()),
                ..Default::default()
            },
        );
    }
    if has(args, "--setup-status") {
        return bridgeforge_lifecycle::release::setup::status_with(
            context.root(),
            runner,
            has(args, "--reconfigure"),
        );
    }
    if has(args, "--setup-plan") {
        if let Some(path) = value(args, "--setup-config-file") {
            if value(args, "--strategy").is_some_and(|s| s != "independent") {
                return blocked(
                    "git-sync",
                    "release uses product-packages; alternative strategies are retired",
                );
            }
            return bridgeforge_lifecycle::release::setup::plan_configuration(
                context.root(),
                runner,
                &path,
            );
        }
        return bridgeforge_lifecycle::release::setup::plan(
            context.root(),
            runner,
            &value(args, "--strategy").unwrap_or_default(),
        );
    }
    if has(args, "--setup-apply") {
        if let Some(path) = value(args, "--setup-config-file") {
            if value(args, "--strategy").is_some_and(|s| s != "independent") {
                return blocked(
                    "git-sync",
                    "release uses product-packages; alternative strategies are retired",
                );
            }
            return bridgeforge_lifecycle::release::setup::apply_configuration(
                context.root(),
                runner,
                &path,
                &value(args, "--setup-fingerprint").unwrap_or_default(),
                has(args, "--confirmed-choice"),
            );
        }
        return bridgeforge_lifecycle::release::setup::apply(
            context.root(),
            runner,
            &value(args, "--strategy").unwrap_or_default(),
            &value(args, "--setup-fingerprint").unwrap_or_default(),
            has(args, "--confirmed-choice"),
        );
    }
    if has(args, "--release-status") {
        return crate::git_sync::release_status(context.root(), runner);
    }
    if has(args, "--release-preview") {
        let message = if let Some(path) = value(args, "--message-file") {
            match fs::read_to_string(path) {
                Ok(value) => value,
                Err(error) => return blocked("git-sync", error),
            }
        } else {
            value(args, "--message")
                .or_else(|| value(args, "-m"))
                .unwrap_or_else(|| "chore: release accumulated changes".into())
        };
        return bridgeforge_lifecycle::release::schema4::preview(
            context.root(),
            &message,
            &release_request,
            runner,
        );
    }
    if has(args, "--release") {
        match bridgeforge_lifecycle::release::setup::is_confirmed(context.root()) {
            Ok(true) => {}
            Ok(false) => {
                let mut outcome =
                    bridgeforge_lifecycle::release::setup::status(context.root(), runner);
                if outcome.code == 0 {
                    outcome.code = EXIT_BLOCKED;
                }
                return outcome;
            }
            Err(error) => return blocked("git-sync-setup", error),
        }
    }
    crate::git_sync::sync(
        context.root(),
        runner,
        crate::git_sync::GitSyncOptions {
            message: value(args, "--message").or_else(|| value(args, "-m")),
            message_file: value(args, "--message-file").map(PathBuf::from),
            remote: value(args, "--remote").unwrap_or_else(|| "origin".into()),
            skip_fetch: has(args, "--skip-fetch"),
            skip_push: has(args, "--skip-push"),
            release: has(args, "--release"),
            release_request,
        },
    )
}
