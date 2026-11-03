//! Ordinary Git synchronization. Configuration/ledger-based release APIs retired.
use crate::{CommandOutcome, ProjectContext};
use serde_json::Value;
use std::path::PathBuf;
fn value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|p| p[0] == flag).map(|p| p[1].clone())
}
fn has(args: &[String], flag: &str) -> bool { args.iter().any(|s| s == flag) }
pub fn run(args: &[String], runner: &dyn bridgeforge_core::ProcessRunner) -> CommandOutcome {
    run_with_identity(args, runner, None)
}
pub fn run_with_identity(args: &[String], runner: &dyn bridgeforge_core::ProcessRunner, identity: Option<Value>) -> CommandOutcome {
    let result = (|| -> Result<CommandOutcome, String> {
        for arg in args.iter().filter(|a| a.starts_with("--")) {
            if !["--root", "--message", "--message-file", "--remote", "--skip-fetch", "--skip-push", "--preflight", "--sync-remote"].contains(&arg.as_str()) {
                return Err(format!("unsupported or retired git-sync option: {arg}; configuration-based release/setup/development gates are retired"));
            }
        }
        if has(args, "--preflight") && has(args, "--sync-remote") { return Err("choose one Git mode".into()); }
        let root = value(args, "--root").map(PathBuf::from);
        let context = ProjectContext::discover(root.as_deref())?;
        if has(args, "--preflight") {
            let mut out = crate::git_sync::preflight(context.root(), runner);
            if let (Some(receipt), Some(identity)) = (out.receipt.as_mut(), identity) { receipt["tool"] = identity; }
            return Ok(out);
        }
        let options = crate::git_sync::GitSyncOptions {
            message: value(args, "--message").or_else(|| value(args, "-m")),
            message_file: value(args, "--message-file").map(PathBuf::from),
            remote: value(args, "--remote").unwrap_or_else(|| "origin".into()),
            skip_fetch: has(args, "--skip-fetch"), skip_push: has(args, "--skip-push"),
            ..Default::default()
        };
        if has(args, "--sync-remote") {
            if options.skip_fetch || options.skip_push { return Err("remote integration does not accept skip flags".into()); }
            Ok(crate::git_sync::sync_remote(context.root(), runner, options))
        } else { Ok(crate::git_sync::sync(context.root(), runner, options)) }
    })();
    result.unwrap_or_else(|e| CommandOutcome::blocked(format!("[git-sync] BLOCKED: {e}\n")))
}
