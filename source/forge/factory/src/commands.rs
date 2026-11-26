//! Factory-only command adaptation. Public CLI dispatch stays in its own package.
use crate::{
    CommandOutcome, ProcessRunner, ProjectContext, SystemProcessRunner, factory_runtime,
    factory_version, manifest,
};
use serde_json::json;
use std::path::{Path, PathBuf};

fn blocked(label: &str, error: impl std::fmt::Display) -> CommandOutcome {
    CommandOutcome::blocked(format!("[{label}] BLOCKED: {error}\n"))
}

fn value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}
fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}
fn path_value(args: &[String], flag: &str) -> Result<PathBuf, String> {
    value(args, flag)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{flag} is required"))
}
fn context(args: &[String]) -> Result<ProjectContext, String> {
    ProjectContext::discover(value(args, "--root").as_deref().map(Path::new))
}
fn factory_context(args: &[String]) -> Result<ProjectContext, String> {
    let context = context(args)?;
    if !context
        .root()
        .join(bridgeforge_core::product_manifest::FILE)
        .is_file()
    {
        return Err("factory commands require the factory repository".into());
    }
    Ok(context)
}

pub fn self_test() -> CommandOutcome {
    let product = bridgeforge_core::product_version::explicit(include_str!(env!("BRIDGEFORGE_PRODUCT_MANIFEST")));
    match product {
        Ok(product) => CommandOutcome::with_receipt(json!({
            "schema":1,"name":"bridgeforge-factory","status":"ok",
            "version":env!("CARGO_PKG_VERSION"),"product_version":product,
            "capabilities":["template-only-v1","compiled-build-provenance-v1","factory-runtime-prepare-v1","factory-source-projection-v1","git-sync-preflight-v1","native-release-v1","explicit-cleanup-v1","junction-cleanup-v1","permission-skip-v1"],
            "build_identity":bridgeforge_core::build_identity::identity()
        })),
        Err(error) => blocked("factory-self-test", error),
    }
}

pub fn run(args: &[String]) -> CommandOutcome {
    run_with_runner(args, &SystemProcessRunner)
}

pub fn run_with_runner(args: &[String], runner: &dyn ProcessRunner) -> CommandOutcome {
    if args.first().is_some_and(|c| c == "develop") {
        return blocked("retired-command", "configuration-based development ledger is retired; use native build/test/version tools; no files changed");
    }
    if args.first().is_some_and(|c| c == "template-standard") {
        return match context(args).and_then(|c| {
            let bytes = crate::manifest::render_template_standard(c.root())?;
            let path = c.root().join("source/forge/core/src/template_assets.rs");
            if has(args, "--check") || has(args, "--dry-run") {
                if std::fs::read(&path).ok().as_ref() != Some(&bytes) { return Err("compiled Template rules need regeneration".into()); }
            } else { crate::persistence::atomic_write(&path, &bytes).map_err(|e| e.to_string())?; }
            Ok(json!({"schema":1,"status":"current","target":path}))
        }) { Ok(r) => CommandOutcome::with_receipt(r), Err(e) => blocked("template-standard", e) };
    }
    let Some(command) = args.first().map(String::as_str) else {
        return blocked("factory", "a command is required");
    };
    match command {
        "cleanup" => {
            let mut products = Vec::new();
            for (binary, receipt) in [("bridgeforge", "cli"), ("bridgeforge-hook", "hook"), ("bridgeforge-factory", "factory")] {
                products.push(format!(".build/bridgeforge/bin/{binary}{}", std::env::consts::EXE_SUFFIX));
                products.push(format!(".build/bridgeforge/bin/build-receipt-{receipt}.json"));
            }
            bridgeforge_workflow::artifact_cleanup::command_with_products(args, runner, &products)
        },
        "self-test" if has(args, "--json") => self_test(),
        "manifest" => match context(args) {
            Ok(context) => match manifest::rebuild(
                context.root(),
                has(args, "--check") || has(args, "--dry-run"),
            ) {
                Ok(true) if has(args, "--check") || has(args, "--dry-run") => blocked(
                    "manifest",
                    "generated manifests or dogfood projection are stale",
                ),
                Ok(changed) => CommandOutcome::with_receipt(
                    json!({"schema":1,"status":"ok","changed":changed}),
                ),
                Err(error) => blocked("manifest", error),
            },
            Err(error) => blocked("manifest", error),
        },
        "check" => {
            let root = match context(args) {
                Ok(context) => context,
                Err(error) => return blocked("check", error),
            };
            match args.get(1).map(String::as_str) {
                Some("factory-version") => {
                    let report = factory_version::check(root.root());
                    CommandOutcome {
                        code: if report.healthy {
                            0
                        } else {
                            crate::EXIT_BLOCKED
                        },
                        receipt: Some(serde_json::to_value(report).unwrap()),
                        ..CommandOutcome::default()
                    }
                }
                _ => blocked("factory-check", "expected factory-version"),
            }
        }
        "build-assets" => {
            if !has(args, "--factory") {
                return blocked("build-assets", "factory preparation requires --factory");
            }
            match path_value(args, "--project-root") {
                Ok(root) => factory_runtime::prepare(
                    &root,
                    has(args, "--check") || has(args, "--dry-run"),
                    runner,
                ),
                Err(error) => blocked("build-assets", error),
            }
        }
        "git-sync" => match factory_context(args) {
            Ok(_) => bridgeforge_workflow::git_command::run_with_identity(
                args,
                runner,
                self_test().receipt,
            ),
            Err(error) => blocked("git-sync", error),
        },
        _ => blocked(
            "factory",
            "usage: bridgeforge-factory <self-test|manifest|check|build-assets|git-sync|cleanup>",
        ),
    }
}
