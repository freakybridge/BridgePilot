//! Ordinary synchronization and explicit single-call native publication.
use crate::{CommandOutcome, ProjectContext};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
pub fn run(args: &[String], runner: &dyn bridgeforge_core::ProcessRunner) -> CommandOutcome {
    run_with_identity(args, runner, None)
}
pub fn run_with_identity(
    args: &[String],
    runner: &dyn bridgeforge_core::ProcessRunner,
    identity: Option<Value>,
) -> CommandOutcome {
    let result = (|| -> Result<CommandOutcome, String> {
        let mut index = usize::from(args.first().is_some_and(|a| a == "git-sync"));
        let release = args.get(index).is_some_and(|a| a == "release");
        if release {
            index += 1;
        }
        let mut values = BTreeMap::new();
        let mut flags = BTreeSet::new();
        while index < args.len() {
            let arg = args[index].as_str();
            if ["--root", "--message", "--message-file", "--remote", "-m"].contains(&arg) {
                let name = if arg == "-m" { "--message" } else { arg };
                let value = args
                    .get(index + 1)
                    .filter(|v| !v.is_empty() && !v.starts_with("--"))
                    .ok_or_else(|| format!("missing value for {arg}"))?;
                if values.insert(name, value.clone()).is_some() {
                    return Err(format!("duplicate option: {arg}"));
                }
                index += 2;
            } else if [
                "--skip-fetch",
                "--skip-push",
                "--preflight",
                "--sync-remote",
                "--publication-required",
                "--recover-author-identity",
            ]
            .contains(&arg)
            {
                if !flags.insert(arg) {
                    return Err(format!("duplicate option: {arg}"));
                }
                index += 1;
            } else {
                return Err(format!(
                    "unsupported or retired git-sync argument: {arg}; use 'git-sync release' for native publication; legacy configuration-based release is retired"
                ));
            }
        }
        if values.contains_key("--message") && values.contains_key("--message-file") {
            return Err("choose one commit message input".into());
        }
        if flags.contains("--publication-required") && !release {
            return Err("--publication-required requires the release operation".into());
        }
        if flags.contains("--recover-author-identity") && (!release || flags.contains("--preflight")) {
            return Err("--recover-author-identity requires an executing release operation".into());
        }
        if flags.contains("--preflight") && flags.contains("--sync-remote") {
            return Err("choose one Git mode".into());
        }
        if release
            && flags
                .iter()
                .any(|s| ["--skip-fetch", "--skip-push", "--sync-remote"].contains(s))
        {
            return Err(
                "native release cannot skip required Git stages or select remote-only mode".into(),
            );
        }
        let root = values.get("--root").map(PathBuf::from);
        let context = ProjectContext::discover(root.as_deref())?;
        if flags.contains("--preflight") {
            let mut out = if release {
                crate::native_release::preflight(context.root(), runner)
            } else {
                crate::git_sync::preflight(context.root(), runner)
            };
            if let (Some(receipt), Some(identity)) = (out.receipt.as_mut(), identity) {
                receipt["tool"] = identity;
            }
            return Ok(out);
        }
        let options = crate::git_sync::GitSyncOptions {
            recover_author_identity: flags.contains("--recover-author-identity"),
            message: values.get("--message").cloned(),
            message_file: values.get("--message-file").map(PathBuf::from),
            remote: values
                .get("--remote")
                .cloned()
                .unwrap_or_else(|| "origin".into()),
            skip_fetch: flags.contains("--skip-fetch"),
            skip_push: flags.contains("--skip-push"),
            ..Default::default()
        };
        if release {
            return Ok(crate::native_release::run(context.root(), runner, options));
        }
        if flags.contains("--sync-remote") {
            if options.skip_fetch || options.skip_push {
                return Err("remote integration does not accept skip flags".into());
            }
            Ok(crate::git_sync::sync_remote(
                context.root(),
                runner,
                options,
            ))
        } else {
            Ok(crate::git_sync::sync(context.root(), runner, options))
        }
    })();
    result.unwrap_or_else(|e| CommandOutcome::blocked(format!("[git-sync] BLOCKED: {e}\n")))
}
