mod memory_commands;

use bridgeforge_core::{CommandOutcome, EXIT_BLOCKED, ProjectContext, SystemProcessRunner};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::fs;
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
        && root.join("bridgeforge-manifest.json").is_file()
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
    let product = fs::read_to_string(root.join("VERSION")).map_err(|e| e.to_string())?;
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
    args.windows(2)
        .filter(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .collect()
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
    let mut outcome = CommandOutcome::with_receipt(json!({
        "schema": 1,
        "name": "bridgeforge",
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "capabilities": ["git-sync-explicit-release-v1", "git-sync-release-only-v1", "git-sync-release-readiness-v1", "git-sync-direct-release-v1", "git-sync-timing-v1", "git-sync-setup-v1", "git-sync-package-lifecycle-v1", "user-cli-reuse-v1", "project-sync-build-reuse-v1", "git-sync-remote-context-v1", "git-sync-preflight-v1", "git-sync-phase-timing-v1", "public-artifacts-v1"]
    }));
    if let Some(capabilities) = outcome
        .receipt
        .as_mut()
        .and_then(|r| r["capabilities"].as_array_mut())
    {
        capabilities.push(json!("git-sync-schema4-v1"));
        capabilities.push(json!("git-sync-product-packages-v1"));
        capabilities.push(json!("git-sync-baseline-repair-v1"));
    }
    if let Some(capabilities) = outcome
        .receipt
        .as_mut()
        .and_then(|r| r["capabilities"].as_array_mut())
    {
        capabilities.push(json!("factory-runtime-prepare-v1"));
        capabilities.push(json!("artifact-cleanup-v1"));
        capabilities.push(json!("three-directory-layout-v1"));
        capabilities.push(json!("package-development-v1"));
        capabilities.push(json!("package-stability-v1"));
        capabilities.push(json!("git-sync-development-closure-v1"));
        capabilities.extend(
            [
                "rust-mixed-workspace-v1",
                "package-adoption-v1",
                "scoped-rust-evidence-v1",
                "scoped-release-evidence-v1",
                "task-scoped-development-v1",
                "resumable-development-checks-v1",
                "task-selected-development-checks-v1",
                "staged-development-validation-v1",
                "direct-development-apply-v1",
                "version-coverage-v1",
                "release-external-analysis-preview-v1",
                "public-source-projection-v1",
            ]
            .map(|v| json!(v)),
        );
    }
    outcome
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
                return match bridgeforge_skeleton::baseline::verify_index(
                    context.root(),
                    &SystemProcessRunner,
                ) {
                    Ok(report) => {
                        CommandOutcome::with_receipt(serde_json::to_value(report).unwrap())
                    }
                    Err(error) => blocked("current-baseline", error),
                };
            }
            match bridgeforge_skeleton::baseline::verify(
                context.root(),
                None,
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
                let mut roots = vec![context.root().join(".codex/skills")];
                if context
                    .root()
                    .join("bridgeforge-manifest.json")
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

fn with_build_metrics(
    mut out: CommandOutcome,
    metrics: bridgeforge_core::build_metrics::BuildMetrics,
    format: &str,
) -> CommandOutcome {
    if format != "human" {
        let receipt = out
            .receipt
            .get_or_insert_with(|| json!({"status":"blocked"}));
        let machine = if format == "combined" {
            &mut receipt["machine"]
        } else {
            receipt
        };
        machine["build_metrics"] = serde_json::to_value(metrics).unwrap();
    }
    out
}

fn project_sync(args: &[String]) -> CommandOutcome {
    let started = std::time::Instant::now();
    let mut out = project_sync_inner(args);
    let format = value(args, "--output-format").unwrap_or_else(|| "machine".into());
    if format != "human" {
        if let Some(receipt) = out.receipt.as_mut() {
            let machine = if format == "combined" {
                &mut receipt["machine"]
            } else {
                receipt
            };
            machine["elapsed_ms"] = json!(started.elapsed().as_millis());
        }
    }
    out
}

fn project_sync_inner(args: &[String]) -> CommandOutcome {
    if args.iter().any(|arg| {
        matches!(
            arg.split('=').next(),
            Some("--asset-migration-manifest" | "--confirmed-asset-migration")
        )
    }) {
        return blocked(
            "project-sync",
            "legacy Rule/Memory migration is retired; remove migration arguments",
        );
    }
    let output_format = value(args, "--output-format").unwrap_or_else(|| "machine".into());
    if !matches!(output_format.as_str(), "machine" | "human" | "combined") {
        return blocked(
            "project-sync",
            "invalid --output-format; expected machine|human|combined",
        );
    }
    let project_root = match path_value(args, "--project-root") {
        Ok(value) => value,
        Err(error) => return blocked("project-sync", error),
    };
    let template_root = match path_value(args, "--template-root") {
        Ok(value) => value,
        Err(error) => return blocked("project-sync", error),
    };
    let mode = match value(args, "--mode").as_deref().unwrap_or("auto") {
        "auto" => bridgeforge_workflow::project_sync::SyncMode::Auto,
        "init" => bridgeforge_workflow::project_sync::SyncMode::Init,
        "adopt" => bridgeforge_workflow::project_sync::SyncMode::Adopt,
        "update" => bridgeforge_workflow::project_sync::SyncMode::Update,
        other => return blocked("project-sync", format!("invalid mode: {other}")),
    };
    let preserved = values(args, "--preserve-project-asset");
    let deleted = values(args, "--delete-project-asset");
    let preservation = (!preserved.is_empty() || !deleted.is_empty())
        .then(|| json!({"preserve": preserved, "delete": deleted}));
    let planning_started = std::time::Instant::now();
    let plan_result = bridgeforge_workflow::project_sync::build_plan_with_inputs(
        &project_root,
        &template_root,
        mode,
        preservation.as_ref(),
    );
    let mut plan = match plan_result {
        Ok(value) => value,
        Err(error) => {
            return bridgeforge_workflow::project_sync::outcome_receipt_with_format(
                Err(error),
                &output_format,
            );
        }
    };
    let planning_ms = planning_started.elapsed().as_millis();
    let applying = has(args, "--apply");
    if !applying {
        plan.build_metrics
            .timings_ms
            .insert("planning".into(), planning_ms);
        return bridgeforge_workflow::project_sync::outcome_plan_with_format(
            Ok(plan),
            &output_format,
        );
    }
    let fingerprint = if applying {
        let Some(value) = value(args, "--plan-fingerprint") else {
            return bridgeforge_workflow::project_sync::outcome_receipt_with_format(
                Err(
                    "--apply requires --plan-fingerprint from the immediately preceding plan"
                        .into(),
                ),
                &output_format,
            );
        };
        if plan.aggregate_fingerprint != value {
            return bridgeforge_workflow::project_sync::outcome_receipt_with_format(
                Err("aggregate fingerprint drifted; regenerate the plan".into()),
                &output_format,
            );
        }
        Some(value)
    } else {
        None
    };
    let preservation_decided = plan.preservation_manifest["entries"]
        .as_array()
        .is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| matches!(entry["disposition"].as_str(), Some("preserve" | "delete")))
        });
    if applying && preservation_decided && !has(args, "--confirmed-preservation-manifest") {
        return blocked(
            "project-sync",
            "project asset decisions require --confirmed-preservation-manifest",
        );
    }
    let contract_bytes = match bridgeforge_core::product_manifest::project_bytes(&template_root) {
        Ok(bytes) => bytes,
        Err(error) => return blocked("project-sync", error),
    };
    let contract: Value = match serde_json::from_slice(&contract_bytes) {
        Ok(value) => value,
        Err(error) => {
            return blocked(
                "project-sync",
                format!("cannot read generated asset contract: {error}"),
            );
        }
    };
    let prepared = if plan.public_installation_required {
        let path = match value(args, "--public-artifacts")
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(|| {
                bridgeforge_workflow::public_artifacts::codex_home().and_then(|root| {
                    bridgeforge_workflow::public_artifacts::prepared_path(&root, &contract)
                })
            }) {
            Ok(path) => path,
            Err(error) => return blocked("project-sync", error),
        };
        bridgeforge_workflow::project_sync::attach_prepared_public(
            &mut plan,
            &contract_bytes,
            &path,
        )
    } else if contract["generated_assets"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
    {
        Err("target product lacks the prepared-public-artifact contract; refresh the official product first".into())
    } else {
        Ok(())
    };
    if let Err(error) = prepared {
        plan.build_metrics
            .timings_ms
            .insert("planning".into(), planning_ms);
        return with_build_metrics(
            bridgeforge_workflow::project_sync::outcome_receipt_with_format(
                Err(error),
                &output_format,
            ),
            plan.build_metrics.clone(),
            &output_format,
        );
    }
    plan.build_metrics
        .timings_ms
        .insert("planning".into(), planning_ms);
    let mut failed_metrics = plan.build_metrics.clone();
    let applying_started = std::time::Instant::now();
    let result = bridgeforge_workflow::project_sync::apply_plan(
        plan,
        fingerprint
            .as_deref()
            .expect("apply fingerprint was validated"),
        has(args, "--confirmed-risk"),
    )
    .map(|mut receipt| {
        receipt
            .build_metrics
            .timings_ms
            .insert("apply".into(), applying_started.elapsed().as_millis());
        receipt
    });
    failed_metrics
        .timings_ms
        .insert("apply".into(), applying_started.elapsed().as_millis());
    if result.is_err() {
        return with_build_metrics(
            bridgeforge_workflow::project_sync::outcome_receipt_with_format(result, &output_format),
            failed_metrics,
            &output_format,
        );
    }
    bridgeforge_workflow::project_sync::outcome_receipt_with_format(result, &output_format)
}

fn git_sync(args: &[String]) -> CommandOutcome {
    git_sync_with_runner(args, &SystemProcessRunner)
}

pub fn git_sync_with_runner(
    args: &[String],
    runner: &dyn bridgeforge_core::ProcessRunner,
) -> CommandOutcome {
    bridgeforge_workflow::git_command::run_with_identity(args, runner, self_test().receipt)
}

pub fn run(args: &[String]) -> CommandOutcome {
    if args.first().is_some_and(|c| c == "develop") {
        if let Err(error) =
            bridgeforge_lifecycle::release::schema4::development::task_packages_argument(args)
        {
            return blocked("develop", error);
        }
    }
    if args.first().is_some_and(|c| c == "develop")
        && (["--preview", "--apply", "--recover", "--validate"]
            .iter()
            .filter(|f| has(args, f))
            .count()
            != 1
            || ((has(args, "--apply") || has(args, "--recover") || has(args, "--validate"))
                && (has(args, "--check") || has(args, "--dry-run"))))
    {
        return blocked(
            "develop",
            "choose exactly one of --preview, --validate, --apply, --recover; preview flags cannot authorize writes",
        );
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
        "project-sync" => project_sync(args),
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
        "develop" => {
            let root=value(args,"--root").map(PathBuf::from);
            let context=match ProjectContext::discover(root.as_deref()) { Ok(c)=>c,Err(e)=>return blocked("develop",e) };
            if ["--preview","--apply","--recover","--validate"].iter().filter(|f|has(args,f)).count()!=1
                || ((has(args,"--apply") || has(args,"--recover") || has(args,"--validate")) && (has(args,"--check") || has(args,"--dry-run"))) {
                return blocked("develop","choose exactly one of --preview, --validate, --apply, --recover; preview flags cannot authorize writes");
            }
            let request=bridgeforge_lifecycle::release::schema4::Request{analysis_file:value(args,"--analysis-file").map(PathBuf::from)};
            let task_packages = match bridgeforge_lifecycle::release::schema4::development::task_packages_argument(args) {
                Ok(packages) => packages,
                Err(error) => return blocked("develop", error),
            };
            if has(args, "--validate") {
                return bridgeforge_lifecycle::release::schema4::development::validate_for_packages(
                    context.root(), &request, task_packages.as_deref(), &SystemProcessRunner, None);
            }
            if let Some(packages) = task_packages {
                return if has(args, "--apply") {
                    bridgeforge_lifecycle::release::schema4::development::apply_for_packages(context.root(), &request,
                        &value(args, "--fingerprint").unwrap_or_default(), &packages, &SystemProcessRunner, None)
                } else {
                    bridgeforge_lifecycle::release::schema4::development::preview_for_packages(context.root(), &request,
                        &packages, &SystemProcessRunner, None)
                };
            }
            if has(args,"--recover") { bridgeforge_lifecycle::release::schema4::development::recover(context.root()) }
            else if has(args,"--apply") { bridgeforge_lifecycle::release::schema4::development::apply(context.root(),&request,&value(args,"--fingerprint").unwrap_or_default(),&SystemProcessRunner) }
            else { bridgeforge_lifecycle::release::schema4::development::preview(context.root(),&request,&SystemProcessRunner) }
        }
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
            let contract_path = project.join(".codex/managed-skeleton.json");
            let contract: Value = match fs::read(&contract_path)
                .map_err(|error| error.to_string())
                .and_then(|payload| serde_json::from_slice(&payload).map_err(|error| error.to_string()))
            {
                Ok(value) => value,
                Err(error) => return blocked("build-assets", error),
            };
            match bridgeforge_workflow::project_sync::build_generated_assets(
                &project,
                &contract,
                &SystemProcessRunner,
            ) {
                Ok(receipts) => CommandOutcome::with_receipt(json!({"schema": 1, "status": "built", "receipts": receipts})),
                Err(error) => blocked("build-assets", error),
            }
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
