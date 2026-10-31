//! Factory-only command adaptation. Public CLI dispatch stays in its own package.
use crate::runner::FactoryRunner;
use crate::{
    CommandOutcome, ProcessRunner, ProjectContext, SystemProcessRunner, factory_runtime,
    factory_version, manifest,
};
use serde_json::json;
use std::path::{Path, PathBuf};

fn blocked(label: &str, error: impl std::fmt::Display) -> CommandOutcome {
    CommandOutcome::blocked(format!("[{label}] BLOCKED: {error}\n"))
}

#[cfg(test)]
mod task_package_tests {
    #[test]
    fn task_development_factory_rejects_malformed_selection_before_context() {
        for flags in [
            vec!["--preview", "--packages"],
            vec!["--preview", "--packages", "a", "--packages"],
            vec!["--preview", "--packages=a"],
            vec!["--preview", "--packages", "a,a"],
            vec!["--recover", "--packages", "a"],
        ] {
            let mut args = vec!["develop".to_string()];
            args.extend(flags.into_iter().map(str::to_string));
            let result = super::run(&args);
            assert_ne!(result.code, 0);
            assert!(result.stderr.contains("packages"), "{}", result.stderr);
        }
    }
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
        .join("bridgeforge-manifest.json")
        .is_file()
    {
        return Err("factory commands require the factory repository".into());
    }
    Ok(context)
}

pub fn self_test() -> CommandOutcome {
    let product = include_str!(env!("BRIDGEFORGE_WORKSPACE_MANIFEST"))
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| error.to_string())
        .and_then(|manifest| {
            manifest["workspace"]["package"]["version"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "compiled product version is missing".into())
        });
    match product {
        Ok(product) => CommandOutcome::with_receipt(json!({
            "schema":1,"name":"bridgeforge-factory","status":"ok",
            "version":env!("CARGO_PKG_VERSION"),"product_version":product,
            "capabilities":["artifact-cleanup-v1","three-directory-layout-v1","factory-runtime-prepare-v1","factory-transaction-v1","factory-source-projection-v1",
                "package-development-v1","package-stability-v1","git-sync-development-closure-v1",
                "rust-mixed-workspace-v1","package-adoption-v1","scoped-rust-evidence-v1",
                "release-external-analysis-preview-v1",
                "scoped-release-evidence-v1","task-scoped-development-v1","resumable-development-checks-v1",
                "task-selected-development-checks-v1","version-coverage-v1",
                "staged-development-validation-v1","direct-development-apply-v1",
                "git-sync-release-only-v1","git-sync-explicit-release-v1","git-sync-direct-release-v1","git-sync-remote-context-v1", "git-sync-preflight-v1"]
        })),
        Err(error) => blocked("factory-self-test", error),
    }
}

pub fn run(args: &[String]) -> CommandOutcome {
    run_with_runner(args, &SystemProcessRunner)
}

pub fn run_with_runner(args: &[String], inner: &dyn ProcessRunner) -> CommandOutcome {
    let runner = FactoryRunner::new(inner);
    let Some(command) = args.first().map(String::as_str) else {
        return blocked("factory", "a command is required");
    };
    match command {
        "cleanup" => bridgeforge_workflow::artifact_cleanup::command_with_contract(
            args,
            &runner,
            &manifest::runtime_contract,
        ),
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
                    &runner,
                ),
                Err(error) => blocked("build-assets", error),
            }
        }
        "git-sync" => match factory_context(args) {
            Ok(_) => bridgeforge_workflow::git_command::run_with_identity(
                args,
                &runner,
                self_test().receipt,
            ),
            Err(error) => blocked("git-sync", error),
        },
        "develop" => {
            let task_packages =
                match crate::release::schema4::development::task_packages_argument(args) {
                    Ok(packages) => packages,
                    Err(error) => return blocked("develop", error),
                };
            let root = match factory_context(args) {
                Ok(context) => context,
                Err(error) => return blocked("develop", error),
            };
            if ["--preview", "--apply", "--recover", "--validate"]
                .iter()
                .filter(|flag| has(args, flag))
                .count()
                != 1
                || ((has(args, "--apply") || has(args, "--recover") || has(args, "--validate"))
                    && (has(args, "--check") || has(args, "--dry-run")))
            {
                return blocked(
                    "develop",
                    "choose exactly one of --preview, --validate, --apply, --recover; preview flags cannot authorize writes",
                );
            }
            let request = crate::release::schema4::Request {
                analysis_file: value(args, "--analysis-file").map(PathBuf::from),
            };
            if has(args, "--validate") {
                return crate::release::schema4::development::validate_for_packages(
                    root.root(),
                    &request,
                    task_packages.as_deref(),
                    &runner,
                    Some(&manifest::development_projection),
                );
            }
            if let Some(packages) = task_packages {
                return if has(args, "--apply") {
                    crate::release::schema4::development::apply_for_packages(
                        root.root(),
                        &request,
                        &value(args, "--fingerprint").unwrap_or_default(),
                        &packages,
                        &runner,
                        Some(&manifest::development_projection),
                    )
                } else {
                    crate::release::schema4::development::preview_for_packages(
                        root.root(),
                        &request,
                        &packages,
                        &runner,
                        Some(&manifest::development_projection),
                    )
                };
            }
            if has(args, "--recover") {
                crate::release::schema4::development::recover(root.root())
            } else if has(args, "--apply") {
                crate::release::schema4::development::apply_with_materializer(
                    root.root(),
                    &request,
                    &value(args, "--fingerprint").unwrap_or_default(),
                    &runner,
                    Some(&manifest::development_projection),
                )
            } else {
                crate::release::schema4::development::preview_with_materializer(
                    root.root(),
                    &request,
                    &runner,
                    Some(&manifest::development_projection),
                )
            }
        }
        _ => blocked(
            "factory",
            "usage: bridgeforge-factory <self-test|manifest|check|build-assets|git-sync|develop|cleanup>",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_identity_reports_separate_package_and_product_versions() {
        let outcome = self_test();
        assert_eq!(outcome.code, 0, "{}", outcome.stderr);
        let receipt = outcome.receipt.unwrap();
        assert_eq!(receipt["name"], "bridgeforge-factory");
        assert_eq!(receipt["version"], env!("CARGO_PKG_VERSION"));
        let product = include_str!(env!("BRIDGEFORGE_WORKSPACE_MANIFEST"))
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(
            receipt["product_version"],
            product["workspace"]["package"]["version"].as_str().unwrap()
        );
    }

    #[test]
    fn public_build_and_unknown_factory_commands_fail_before_execution() {
        let outcome = run(&["build-assets".into(), "--project-root".into(), ".".into()]);
        assert_eq!(outcome.code, crate::EXIT_BLOCKED);
        assert!(outcome.stderr.contains("requires --factory"));
        assert_eq!(run(&["unknown".into()]).code, crate::EXIT_BLOCKED);
    }
}
