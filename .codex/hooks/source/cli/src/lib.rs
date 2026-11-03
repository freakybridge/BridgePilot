mod memory_commands;

use bridgeforge_core::{CommandOutcome, EXIT_BLOCKED, ProjectContext, SystemProcessRunner};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use bridgeforge_core::output::emit;

fn blocked(label: &str, error: impl std::fmt::Display) -> CommandOutcome {
    CommandOutcome::blocked(format!("[{label}] BLOCKED: {error}\n"))
}

/// Preserve factory command entry names without linking factory implementation
/// into the public runtime. Only a measured, current factory image is invoked.
fn factory_command(args: &[String]) -> CommandOutcome {
    factory_command_with_runner(args, &SystemProcessRunner)
}

fn is_factory_repository(root: &Path) -> bool {
    root.join(bridgeforge_core::source_projection::physical_path(root, "source/factory/Cargo.toml")).is_file()
        && root.join(bridgeforge_core::product_manifest::FILE).is_file()
}

fn read_factory_file(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    bridgeforge_core::managed_paths::read_metadata(path, limit)?.ok_or_else(|| {
        format!(
            "factory runtime is not prepared: {}; prepare it from the current source workspace",
            path.display()
        )
    })
}

fn factory_spec(root: &Path) -> Result<(Value, Value), String> {
    let projected = bridgeforge_core::source_projection::render(root, false)?;
    let package = std::str::from_utf8(&projected["source/factory/Cargo.toml"])
        .map_err(|e| e.to_string())?
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.to_string())?;
    let version = package
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(toml_edit::Item::as_str)
        .ok_or("factory package must own its version")?;
    let product = bridgeforge_core::product_version::forge(root)?;
    let test = json!({"args":["self-test","--json"],"expected_json":{
        "schema":1,"name":"bridgeforge-factory","status":"ok","version":version,"product_version":product.trim()}});
    let metadata = bridgeforge_core::build_manifest::canonical_sha;
    let asset = json!({"id":"codex.factory-cli",
        "source_tree_sha256":bridgeforge_core::source_projection::tree_sha(&projected)?,
        "lockfile_sha256":bridgeforge_core::build_manifest::normalized_hash(&projected["Cargo.lock"]),
        "build_recipe_sha256":metadata(&bridgeforge_core::build_manifest::generated_build_recipe("bridgeforge-factory"))?,
        "self_test_sha256":metadata(&test)?});
    Ok((asset, test["expected_json"].clone()))
}

struct FactoryImage {
    bytes: Vec<u8>,
    receipt: Vec<u8>,
    expected_self_test: Value,
}
fn verified_factory_image(
    root: &Path,
    binary: &Path,
    receipt: &Path,
) -> Result<FactoryImage, String> {
    let bytes = read_factory_file(binary, 512 * 1024 * 1024)?;
    let receipt = read_factory_file(receipt, 1024 * 1024)?;
    let (asset, expected_self_test) = factory_spec(root)?;
    bridgeforge_core::build_receipt::verify_generated_payload(&asset, &bytes, &receipt)?;
    Ok(FactoryImage {
        bytes,
        receipt,
        expected_self_test,
    })
}

fn factory_command_with_runner(
    args: &[String],
    runner: &dyn bridgeforge_core::ProcessRunner,
) -> CommandOutcome {
    let result = (|| -> Result<CommandOutcome, String> {
        let root = value(args, "--root")
            .or_else(|| value(args, "--project-root"))
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !is_factory_repository(&root) {
            return Err("this command requires the factory repository".into());
        }
        let binary = bridgeforge_core::artifact_paths::binary(&root, true, "bridgeforge-factory");
        let receipt = root
            .join(bridgeforge_core::artifact_paths::FACTORY_BIN)
            .join("build-receipt-factory.json");
        let before = verified_factory_image(&root, &binary, &receipt)?;
        let mut request = bridgeforge_core::ProcessRequest::new(binary.as_os_str(), &root);
        request.args = args.iter().map(OsString::from).collect();
        request.timeout = Duration::from_secs(14_400);
        let output = runner.run(&request).map_err(|e| e.to_string())?;
        if output.timed_out {
            return Err("factory command timed out".into());
        }
        if read_factory_file(&binary, 512 * 1024 * 1024)? != before.bytes {
            // A managed release can refresh its own running factory executable.
            // Verify the final source/receipt/self-test instead of reporting an
            // already completed Git transaction as a failed command.
            let after = verified_factory_image(&root, &binary, &receipt)?;
            let mut probe = bridgeforge_core::ProcessRequest::new(binary.as_os_str(), &root);
            probe.args = vec!["self-test".into(), "--json".into()];
            probe.timeout = Duration::from_secs(30);
            let tested = runner.run(&probe).map_err(|e| e.to_string())?;
            let actual = bridgeforge_core::json::parse_unique_json(
                &tested.stdout,
                "refreshed factory self-test",
            )?;
            if tested.code != 0
                || tested.timed_out
                || !after
                    .expected_self_test
                    .as_object()
                    .unwrap()
                    .iter()
                    .all(|(key, value)| actual.get(key) == Some(value))
            {
                return Err("refreshed factory self-test identity mismatch".into());
            }
            if read_factory_file(&binary, 512 * 1024 * 1024)? != after.bytes
                || read_factory_file(&receipt, 1024 * 1024)? != after.receipt
            {
                return Err(
                    "refreshed factory image or receipt changed during verification".into(),
                );
            }
        }
        let parsed = serde_json::from_slice::<Value>(&output.stdout).ok();
        Ok(CommandOutcome {
            code: output.code,
            stdout: if parsed.is_none() {
                String::from_utf8_lossy(&output.stdout).into_owned()
            } else {
                String::new()
            },
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            receipt: parsed,
        })
    })();
    result.unwrap_or_else(|error| blocked("factory", error))
}

fn value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn values(args: &[String], flag: &str) -> Vec<String> {
    args.windows(2).filter(|pair| pair[0] == flag).map(|pair| pair[1].clone()).collect()
}


fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|item| item == flag)
}

fn path_value(args: &[String], flag: &str) -> Result<PathBuf, String> {
    value(args, flag)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{flag} is required"))
}

fn self_test() -> CommandOutcome {
    CommandOutcome::with_receipt(json!({
        "schema": 1,
        "name": "bridgeforge",
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "capabilities": ["template-only-v1", "compiled-build-provenance-v1", "user-cli-reuse-v1", "git-sync-preflight-v1", "public-artifacts-v1", "native-release-v1"],
        "build_identity": bridgeforge_core::build_identity::identity()
    }))
}


fn instruction_source(root: &Path) -> CommandOutcome {
    let report = bridgeforge_checks::instructions::inspect(root);
    CommandOutcome {
        code: if report.issues.is_empty() {
            0
        } else {
            EXIT_BLOCKED
        },
        stdout: String::new(),
        stderr: report.diagnostics(),
        receipt: None,
    }
}

fn check(command: &str, args: &[String]) -> CommandOutcome {
    let root = value(args, "--root").map(PathBuf::from);
    let context = match ProjectContext::discover(root.as_deref()) {
        Ok(value) => value,
        Err(error) => return blocked(command, error),
    };
    match command {
        "baseline" => {
            if has(args, "--index") {
                return match bridgeforge_skeleton::template::verify_index(
                    context.root(),
                    &SystemProcessRunner,
                ) {
                    Ok(report) => {
                        CommandOutcome::with_receipt(serde_json::to_value(report).unwrap())
                    }
                    Err(error) => blocked("current-baseline", error),
                };
            }
            match bridgeforge_skeleton::template::verify(
                context.root(),
                !has(args, "--skip-generated-runtime"),
            ) {
                Ok(report) => CommandOutcome::with_receipt(serde_json::to_value(report).unwrap()),
                Err(error) => blocked("current-baseline", error),
            }
        }
        "todo" => {
            if has(args, "--contract") {
                return CommandOutcome::with_receipt(bridgeforge_checks::todo::contract());
            }
            let report = bridgeforge_checks::todo::inspect(context.root());
            CommandOutcome {
                code: if report.errors.is_empty() {
                    0
                } else {
                    EXIT_BLOCKED
                },
                receipt: Some(serde_json::to_value(report).unwrap()),
                ..CommandOutcome::default()
            }
        }
        "project-structure" => {
            let report = if has(args, "--pre-commit") {
                bridgeforge_checks::project_structure::pre_commit(context.root())
            } else {
                bridgeforge_checks::project_structure::inspect(context.root())
            };
            CommandOutcome {
                code: if report.errors.is_empty() {
                    0
                } else {
                    EXIT_BLOCKED
                },
                receipt: Some(serde_json::to_value(report).unwrap()),
                ..CommandOutcome::default()
            }
        }
        "skill-metadata" => {
            let roots = if let Some(path) = value(args, "--skill-root") {
                vec![PathBuf::from(path)]
            } else {
                let mut roots = vec![context.root().join(".codex/skills"), context.root().join(".agents/skills")];
                if context
                    .root()
                    .join(bridgeforge_core::product_manifest::FILE)
                    .is_file()
                {
                    roots.push(bridgeforge_core::source_projection::template_path(context.root(), "templates/skills"));
                }
                roots
            };
            let mut report = bridgeforge_checks::skill_metadata::SkillReport::default();
            for skill_root in roots {
                let checked = bridgeforge_checks::skill_metadata::validate_tree(&skill_root);
                report.issues.extend(checked.issues);
                report.warnings.extend(checked.warnings);
            }
            CommandOutcome {
                code: if report.issues.is_empty() {
                    0
                } else {
                    EXIT_BLOCKED
                },
                receipt: Some(serde_json::to_value(report).unwrap()),
                ..CommandOutcome::default()
            }
        }
        "instruction-source" => instruction_source(context.root()),
        "factory-version" => factory_command(args),
        _ => blocked("check", format!("unknown check: {command}")),
    }
}

fn git_sync(args: &[String]) -> CommandOutcome {
    git_sync_with_runner(args, &SystemProcessRunner)
}

fn template_project_sync(args: &[String]) -> CommandOutcome {
    let result = (|| -> Result<Value, String> {
        for arg in args {
            if matches!(arg.split('=').next(), Some("--preserve-project-asset" | "--delete-project-asset" |
                "--asset-migration-manifest" | "--confirmed-asset-migration")) {
                return Err("legacy preservation/migration arguments are retired".into());
            }
        }
        if let Some(mode) = value(args, "--mode") {
            if !["auto", "init", "adopt", "update"].contains(&mode.as_str()) { return Err("invalid project-sync mode".into()); }
        }
        if has(args, "--apply") && (has(args, "--check") || has(args, "--dry-run")) {
            return Err("preview flags cannot authorize apply".into());
        }
        let product = path_value(args, "--template-root")?;
        let plan = bridgeforge_workflow::template_sync::plan(&path_value(args, "--project-root")?, &product)?;
        if !has(args, "--apply") { return Ok(plan.report); }
        let contract = bridgeforge_core::product_manifest::project(&product)?;
        let prepared = match value(args, "--public-artifacts") {
            Some(path) => PathBuf::from(path),
            None => bridgeforge_workflow::public_artifacts::prepared_path(
                &bridgeforge_workflow::public_artifacts::codex_home()?, &contract)?,
        };
        bridgeforge_workflow::template_sync::apply(plan,
            &value(args, "--plan-fingerprint").ok_or("--apply requires --plan-fingerprint")?,
            has(args, "--confirmed-risk"), &prepared)
    })();
    match result { Ok(report) => CommandOutcome::with_receipt(report), Err(error) => blocked("project-sync", error) }
}

pub fn git_sync_with_runner(
    args: &[String],
    runner: &dyn bridgeforge_core::ProcessRunner,
) -> CommandOutcome {
    bridgeforge_workflow::git_command::run_with_identity(args, runner, self_test().receipt)
}

pub fn run(args: &[String]) -> CommandOutcome {
    // Retired commands must stop before factory forwarding, support-floor reads
    // or legacy initialization can recreate project sidecars.
    if args.first().is_some_and(|c| c == "develop" || c == "cleanup") {
        return blocked("retired-command", "configuration-based development ledger and cleanup are retired; use native project build/test/version tools; no files changed");
    }
    if args.first().is_some_and(|c| c == "build-assets") && !has(args, "--factory") {
        return blocked("build-assets", "project-local compilation is retired; use prepared public artifacts through project-sync");
    }
    if args.first().is_some_and(|c| c == "project-sync") {
        return template_project_sync(args);
    }
    let root = value(args, "--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    if matches!(
        args.first().map(String::as_str),
        Some("git-sync" | "develop" | "cleanup")
    ) && is_factory_repository(&root)
    {
        return factory_command(args);
    }
    let Some(command) = args.first().map(String::as_str) else {
        return blocked("bridgeforge", "a command is required");
    };
    match command {
        "self-test" if has(args, "--json") => self_test(),
        "project-hook-migration" => blocked("project-hook-migration", "retired: project hooks are maintained by the project; skeleton updates never build or migrate them"),
        "public-artifacts" => {
            if args.get(1).map(String::as_str) == Some("locate") {
                return bridgeforge_workflow::public_artifacts::outcome((|| {
                    bridgeforge_workflow::public_artifacts::locate_cli(&path_value(args,"--product-root")?,&path_value(args,"--codex-home")?,&SystemProcessRunner)
                })());
            }
            if args.get(1).map(String::as_str) != Some("prepare") {
                return blocked("public-artifacts", "expected prepare");
            }
            if has(args, "--check") || has(args, "--dry-run") {
                return blocked("public-artifacts", "preparation writes a derived cache; preview flags are not supported");
            }
            let result = (|| {
                let product = path_value(args, "--product-root")?;
                let codex = path_value(args, "--codex-home")?;
                let bootstrap = value(args, "--bootstrap-cli").map(PathBuf::from);
                let identity = value(args, "--bootstrap-identity");
                let target = value(args, "--bootstrap-target").map(PathBuf::from);
                bridgeforge_workflow::public_artifacts::prepare_with_product_version(&product, &codex, bootstrap.as_deref(), identity.as_deref(), target.as_deref(), env!("CARGO_PKG_VERSION"), &SystemProcessRunner)
            })();
            bridgeforge_workflow::public_artifacts::outcome(result)
        },
        "build-environment" => match path_value(args, "--source-root") {
            Ok(root) => bridgeforge_core::runtime::build_environment(&root, &SystemProcessRunner),
            Err(e) => blocked("build-environment", e),
        },
        "doctor" => match value(args, "--product-root") {
            Some(root) => match std::env::current_exe().map_err(|e| e.to_string()).and_then(|binary|
                bridgeforge_core::runtime::validate_product(Path::new(&root), &binary, &SystemProcessRunner)) {
                Ok(receipt) => CommandOutcome::with_receipt(serde_json::to_value(receipt).unwrap()),
                Err(error) => blocked("bridgeforge-runtime", error),
            },
            None => bridgeforge_core::runtime::outcome(value(args, "--root").as_deref().map(Path::new), &SystemProcessRunner, false),
        },
        "check" => args
            .get(1)
            .map(|name| check(name, args))
            .unwrap_or_else(|| blocked("check", "a check name is required")),
        "archive-scan" => match ProjectContext::discover(value(args, "--root").as_deref().map(Path::new))
            .and_then(|context| bridgeforge_checks::archive_scan::scan(context.root()))
        {
            Ok(candidates) => CommandOutcome::with_receipt(json!({"schema": 1, "count": candidates.len(), "candidates": candidates})),
            Err(error) => blocked("archive-scan", error),
        },
        "audit-user-allow" => {
            let path = match path_value(args, "--settings") {
                Ok(value) => value,
                Err(error) => return blocked("audit-user-allow", error),
            };
            match bridgeforge_checks::audit_user_allow::audit(&path) {
                Ok(findings) => CommandOutcome::with_receipt(json!({"schema": 1, "count": findings.len(), "findings": findings})),
                Err(error) => blocked("audit-user-allow", error),
            }
        }
        "project-sync" => template_project_sync(args),
        "user-agents-stage" => {
            if has(args, "--check") || has(args, "--dry-run") {
                return blocked("user-agents-stage", "staging is a write operation; preview flags are not supported");
            }
            let result = (|| {
                let root = path_value(args, "--product-root")?;
                let profile = path_value(args, "--user-profile")?;
                let operation = value(args, "--operation-id").ok_or("--operation-id is required")?;
                bridgeforge_core::user_agents::stage(&root, &profile, &operation)
            })();
            match result {
                Ok(plan) => CommandOutcome::with_receipt(plan),
                Err(error) => blocked("user-agents-stage", error),
            }
        }
        "cleanup" => bridgeforge_workflow::artifact_cleanup::command(args, &SystemProcessRunner),
        "git-sync" => git_sync(args),
        "develop" => blocked("develop", "configuration-based development ledger is retired"),
        "memory-sync" => memory_commands::memory_sync(&args[1..]),
        "manifest" => factory_command(args),
        "build-assets" => {
            let project = match path_value(args, "--project-root") {
                Ok(value) => value,
                Err(error) => return blocked("build-assets", error),
            };
            if has(args, "--factory") {
                return factory_command(args);
            }
            if has(args, "--check") || has(args, "--dry-run") {
                return blocked("build-assets", "read-only inspection requires --factory; no build performed");
            }
            let _ = project;
            blocked("build-assets", "project-local compilation is retired; use prepared public artifacts")
        }

        _ => CommandOutcome {
            code: EXIT_BLOCKED,
            stderr: "usage: bridgeforge <self-test|doctor|check|archive-scan|audit-user-allow|project-sync|public-artifacts|develop|git-sync|memory-sync|manifest|build-assets|cleanup>\n".into(),
            ..CommandOutcome::default()
        },
    }
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/cli.rs"]
mod tests;

pub fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let profile = arguments.first().is_some_and(|v| v == "git-sync")
        && std::env::var_os("BRIDGEFORGE_PROFILE").is_some_and(|v| v == "1");
    if profile {
        bridgeforge_core::release_metrics::begin();
    }
    let started = std::time::Instant::now();
    let mut outcome = run(&arguments);
    if profile {
        let mut metrics = bridgeforge_core::release_metrics::finish();
        metrics["cli_elapsed_us"] = json!(started.elapsed().as_micros() as u64);
        outcome.receipt.get_or_insert_with(
            || json!({"status":if outcome.code == 0 {"ok"} else {"blocked"}}),
        )["profile"] = metrics;
    }
    std::process::exit(emit(outcome));
}
