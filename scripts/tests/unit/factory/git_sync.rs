//! Factory materialization, cache, preparation and rollback tests.
#[allow(dead_code)]
#[path = "../../support/product_manifest.rs"]
mod fixture_product;

use crate::git_sync_plan as write_plan;
use bridgeforge_core::{
    CommandOutcome, ProcessOutput, ProcessRequest, ProcessRunner, SystemProcessRunner,
};
use bridgeforge_workflow::git_sync::{self, GitSyncOptions, release_status};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[path = "../../support/schema4_agent.rs"]
mod fixture;
#[path = "../../support/git_repository.rs"]
mod repository;
use repository::*;

fn sync(root: &Path, runner: &dyn ProcessRunner, mut options: GitSyncOptions) -> CommandOutcome {
    let factory_runner = crate::runner::FactoryRunner::new(runner);
    if options.release
        && options.release_request.analysis_file.is_none()
        && fs::read(root.join(".codex/bridgeforge-version.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .is_some_and(|v| v["schema_version"] == 4)
    {
        match fixture::analysis(root, &factory_runner, "patch", &[]) {
            Ok(request) => options.release_request = request,
            Err(error) => return CommandOutcome::blocked(error),
        }
    }
    git_sync::sync(root, &factory_runner, options)
}

fn factory_repository(name: &str) -> (RealRepository, PathBuf) {
    let (repo, remote) = managed_repository(name);
    fs::remove_file(repo.0.join(".codex/.bridgeforge_codex_version")).unwrap();
    fs::create_dir_all(repo.0.join("templates/hooks")).unwrap();
    fs::create_dir_all(repo.0.join("templates/user")).unwrap();
    fs::write(
        repo.0.join(crate::user_agents::SOURCE),
        b"user instructions\n",
    )
    .unwrap();
    fs::create_dir_all(repo.0.join(".codex/hooks")).unwrap();
    fs::write(repo.0.join("templates/managed.txt"), b"old\n").unwrap();
    fixture_product::write(
        repo.0.join("source/forge/bridgeforge-manifest.json"),
        managed_contract(b"old\n"),
    )
    .unwrap();
    fixture_product::write_shared(
        repo.0.join("source/forge/bridgeforge-manifest.json"),
        br#"{"platforms":{"windows":{"skills":[]}}}"#,
    )
    .unwrap();
    fs::write(repo.0.join("VERSION"), b"8.0.0\n").unwrap();
    fs::write(repo.0.join("CHANGELOG.md"), b"# Changelog\n").unwrap();
    fs::write(repo.0.join(".codex/bridgeforge-version.json"), br#"{"schema_version":1,"manifests":["templates/hooks/Cargo.toml",".codex/hooks/Cargo.toml"]}"#).unwrap();
    for base in ["templates/hooks", ".codex/hooks"] {
        fs::write(
            repo.0.join(base).join("Cargo.toml"),
            b"[package]\nname = \"bridgeforge-hook\"\nversion = \"8.0.0\"\n",
        )
        .unwrap();
        fs::write(
            repo.0.join(base).join("Cargo.lock"),
            b"version = 4\n[[package]]\nname = \"bridgeforge-hook\"\nversion = \"8.0.0\"\n",
        )
        .unwrap();
    }
    crate::manifest::rebuild(&repo.0, false).unwrap();
    git_ok(&repo.0, &["add", "."]);
    git_ok(&repo.0, &["commit", "-m", "factory baseline"]);
    fixture::migrate(&repo.0, &SystemProcessRunner).unwrap();
    git_ok(&repo.0, &["add", "."]);
    git_ok(&repo.0, &["commit", "-m", "chore: schema4 factory fixture"]);
    fs::remove_dir_all(repo.0.join(".runtime")).unwrap();
    fs::write(repo.0.join("tracked.txt"), b"user change\n").unwrap();
    (repo, remote)
}

struct GeneratedRunner {
    root: PathBuf,
    mode: &'static str,
}

fn is_short_cache_probe(program: &Path) -> bool {
    program.parent().is_some_and(|parent| {
        parent.file_name().is_some_and(|name| {
            name.to_string_lossy().starts_with("bf-probe-")
                || name.to_string_lossy().starts_with("probe-")
        })
    })
}

impl ProcessRunner for GeneratedRunner {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        if request
            .args
            .first()
            .is_some_and(|arg| matches!(arg.to_str(), Some("--version" | "-vV" | "-Vv")))
            && (request.program == "cargo" || request.program == "rustc")
        {
            return Ok(out(
                0,
                &format!("{} 1.88.0 (fixture)", request.program.to_string_lossy()),
            ));
        }
        if self.mode == "policy-drift" && request.args.iter().any(|arg| arg == "check-ignore") {
            fs::write(
                self.root.join(".codex/bridgeforge-version.json"),
                b"external policy edit\n",
            )?;
        }
        if request.program == "cargo" {
            assert_eq!(
                fs::read(self.root.join("VERSION")).unwrap(),
                b"8.0.0\n",
                "build must precede repository writes"
            );
            if self.mode == "fail" {
                return Ok(out(1, ""));
            }
            let args = request
                .args
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let output =
                PathBuf::from(&args[args.iter().position(|p| p == "--target-dir").unwrap() + 1]);
            let binary = &args[args.iter().position(|p| p == "--bin").unwrap() + 1];
            fs::create_dir_all(output.join("release"))?;
            fs::write(
                output.join("release").join(if cfg!(windows) {
                    format!("{binary}.exe")
                } else {
                    binary.clone()
                }),
                format!("new {binary}"),
            )?;
            if self.mode == "drift" {
                fs::write(
                    self.root.join("templates/hooks/Cargo.lock"),
                    b"external edit\n",
                )?;
            }
            if self.mode == "user-agents-drift" {
                fs::write(
                    self.root.join(crate::user_agents::SOURCE),
                    b"external user template edit\n",
                )?;
            }
            return Ok(out(0, ""));
        }
        if request.args.first().is_some_and(|arg| arg == "self-test") {
            let name = if request
                .program
                .to_string_lossy()
                .contains("bridgeforge-hook")
            {
                "bridgeforge-hook"
            } else {
                "bridgeforge"
            };
            return Ok(out(
                0,
                &json!({"schema":1,"name":name,"status":"ok"}).to_string(),
            ));
        }
        SystemProcessRunner.run(request)
    }
}

#[test]
fn factory_build_failure_and_input_drift_do_not_apply_release() {
    for mode in ["fail", "drift", "policy-drift", "user-agents-drift"] {
        let (repo, remote) = factory_repository(mode);
        let manifest = fixture_product::read(repo.0.join("source/forge/bridgeforge-manifest.json")).unwrap();
        let user_manifest = fs::read(repo.0.join(crate::user_agents::MANIFEST)).unwrap();
        let outcome = sync(
            &repo.0,
            &GeneratedRunner {
                root: repo.0.clone(),
                mode,
            },
            GitSyncOptions {
                message: Some("fix: generated planning".into()),
                skip_fetch: true,
                skip_push: true,
                ..Default::default()
            },
        );
        assert_eq!(outcome.code, 2, "{}", outcome.stderr);
        assert!(
            outcome.stderr.contains("before apply"),
            "{}",
            outcome.stderr
        );
        assert_eq!(fs::read(repo.0.join("VERSION")).unwrap(), b"8.0.0\n");
        assert_eq!(
            fixture_product::read(repo.0.join("source/forge/bridgeforge-manifest.json")).unwrap(),
            manifest
        );
        assert!(
            !repo
                .0
                .join(".deps/bridgeforge/bin/build-receipt-cli.json")
                .exists()
        );
        assert_eq!(
            fs::read(repo.0.join(crate::user_agents::MANIFEST)).unwrap(),
            user_manifest
        );
        if mode == "user-agents-drift" {
            assert_eq!(
                fs::read(repo.0.join(crate::user_agents::SOURCE)).unwrap(),
                b"external user template edit\n"
            );
        }
        if mode == "drift" {
            assert_eq!(
                fs::read(repo.0.join("templates/hooks/Cargo.lock")).unwrap(),
                b"external edit\n"
            );
        }
        if mode == "policy-drift" {
            assert_eq!(
                fs::read(repo.0.join(".codex/bridgeforge-version.json")).unwrap(),
                b"external policy edit\n"
            );
        }
        fs::remove_dir_all(remote).unwrap();
    }
}

#[test]
fn factory_rejected_commit_restores_versions_binaries_receipts_and_index() {
    let (repo, remote) = factory_repository("generated-rollback");
    let user_manifest_before = fs::read(repo.0.join(crate::user_agents::MANIFEST)).unwrap();
    fs::write(
        repo.0.join(crate::user_agents::SOURCE),
        b"authorized template edit\n",
    )
    .unwrap();
    fs::create_dir_all(repo.0.join(".deps/bridgeforge/bin")).unwrap();
    let cli = repo.0.join(if cfg!(windows) {
        ".deps/bridgeforge/bin/bridgeforge.exe"
    } else {
        ".deps/bridgeforge/bin/bridgeforge"
    });
    fs::write(&cli, b"original binary").unwrap();
    let receipt = repo.0.join(".deps/bridgeforge/bin/build-receipt-cli.json");
    fs::write(&receipt, b"original receipt").unwrap();
    let before_contract = fixture_product::read(repo.0.join("source/forge/bridgeforge-manifest.json")).unwrap();
    fs::write(
        repo.0.join(".git/hooks/pre-commit"),
        b"#!/bin/sh\necho fixture-reject >&2\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            repo.0.join(".git/hooks/pre-commit"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    git_ok(&repo.0, &["status", "--porcelain=v1"]);
    let index = fs::read(repo.0.join(".git/index")).unwrap();
    let outcome = sync(
        &repo.0,
        &GeneratedRunner {
            root: repo.0.clone(),
            mode: "ok",
        },
        GitSyncOptions {
            message: Some("fix: generated rollback".into()),
            skip_fetch: true,
            skip_push: true,
            ..Default::default()
        },
    );
    assert_eq!(
        fs::read(repo.0.join(crate::user_agents::MANIFEST)).unwrap(),
        user_manifest_before
    );
    assert_eq!(
        fs::read(repo.0.join(crate::user_agents::SOURCE)).unwrap(),
        b"authorized template edit\n"
    );
    assert_eq!(outcome.code, 2, "{}", outcome.stderr);
    assert!(
        outcome.stderr.contains("fixture-reject") && outcome.stderr.contains("rolled back"),
        "{}",
        outcome.stderr
    );
    assert_eq!(fs::read(repo.0.join("VERSION")).unwrap(), b"8.0.0\n");
    assert_eq!(fs::read(&cli).unwrap(), b"original binary");
    assert_eq!(fs::read(&receipt).unwrap(), b"original receipt");
    assert_eq!(
        fixture_product::read(repo.0.join("source/forge/bridgeforge-manifest.json")).unwrap(),
        before_contract
    );
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(repo.0.join("tracked.txt")).unwrap(),
        b"user change\n"
    );
    fs::remove_dir_all(remote).unwrap();
}

fn factory_with_valid_generated_assets(name: &str) -> (RealRepository, PathBuf) {
    let (repo, remote) = factory_repository(name);
    let runner = GeneratedRunner {
        root: repo.0.clone(),
        mode: "ok",
    };
    let plan = write_plan::WritePlan::prepare(
        &repo.0,
        Default::default(),
        Default::default(),
        true,
        &runner,
    )
    .unwrap();
    assert_eq!(plan.generated_built, 2);
    for (path, bytes) in plan.writes {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    (repo, remote)
}

#[test]
fn factory_dirty_sync_and_release_do_not_acquire_maintenance_locks() {
    for release in [false, true] {
        let (repo, remote) = factory_with_valid_generated_assets("sync-without-project-lock");
        let runner = GeneratedRunner {
            root: repo.0.clone(),
            mode: "ok",
        };
        let request = if release {
            fixture::analysis(
                &repo.0,
                &crate::runner::FactoryRunner::new(&runner),
                "patch",
                &[],
            )
            .unwrap()
        } else {
            Default::default()
        };
        let runtime_lock = git_sync::lock_factory_runtime(&repo.0, &SystemProcessRunner).unwrap();
        let project_lock = crate::project_sync::ProjectLock::acquire(&repo.0).unwrap();
        let result = sync(
            &repo.0,
            &runner,
            GitSyncOptions {
                release,
                release_request: request,
                message: Some("fix: synchronize factory change".into()),
                ..Default::default()
            },
        );
        assert_eq!(result.code, 0, "{}", result.stderr);
        let receipt = result.receipt.unwrap();
        assert_eq!(receipt["status"], "synced");
        assert_eq!(receipt["working_tree"], "clean");
        assert_eq!(receipt["version_bumped"], release);
        // Maintenance consumers still serialize while sync itself does not.
        assert!(crate::project_sync::ProjectLock::acquire(&repo.0).is_err());
        drop(project_lock);
        drop(runtime_lock);
        fs::remove_dir_all(remote).unwrap();
    }
}

#[test]
fn factory_build_reuses_captured_snapshot() {
    let (repo, remote) = factory_repository("snapshot-capture-baseline");
    let runner = GeneratedRunner {
        root: repo.0.clone(),
        mode: "ok",
    };
    crate::release_metrics::begin();
    let started = std::time::Instant::now();
    let plan = write_plan::WritePlan::prepare(
        &repo.0,
        Default::default(),
        Default::default(),
        true,
        &runner,
    )
    .unwrap();
    let metrics = crate::release_metrics::finish();
    assert_eq!(plan.generated_built, 2);
    assert_eq!(metrics["counters"]["build_inputs.capture"]["calls"], 1);
    println!(
        "snapshot-capture reused: elapsed_ms={}, metrics={metrics}",
        started.elapsed().as_millis()
    );
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn factory_captured_snapshot_rejects_foreign_sources_and_isolated_drift() {
    struct NoProcesses;
    impl ProcessRunner for NoProcesses {
        fn run(&self, _: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            panic!("invalid captured inputs must be rejected before any process");
        }
    }
    let (repo, remote) = factory_repository("captured-input-guards");
    let contract: serde_json::Value =
        serde_json::from_slice(&fixture_product::read(repo.0.join("source/forge/bridgeforge-manifest.json")).unwrap())
            .unwrap();
    let asset = &contract["generated_assets"][0];
    let source = repo.0.join(asset["source_root"].as_str().unwrap());
    let snapshot = repo.0.join(".runtime/captured-source");
    let inputs =
        crate::build_inputs::BuildInputs::capture(&source, snapshot.clone(), asset).unwrap();
    let runner = NoProcesses;
    let foreign = repo.0.join(".runtime/foreign-base");
    let mut captured = std::collections::BTreeMap::new();
    captured.insert(
        (foreign.join(asset["source_root"].as_str().unwrap()), None),
        &inputs,
    );
    let error = crate::generated_assets::generated_writes_captured(
        &foreign,
        "source_root",
        &repo.0,
        &contract,
        &runner,
        &captured,
    )
    .unwrap_err();
    assert!(error.contains("different source or projection"), "{error}");
    captured.clear();
    captured.insert((source.clone(), None), &inputs);
    let mut projected = contract.clone();
    projected["generated_assets"][0]["source_projection"] = json!(crate::source_projection::PUBLIC);
    captured.insert(
        (source, Some(crate::source_projection::PUBLIC.into())),
        &inputs,
    );
    let error = crate::generated_assets::generated_writes_captured(
        &repo.0,
        "source_root",
        &repo.0,
        &projected,
        &runner,
        &captured,
    )
    .unwrap_err();
    assert!(error.contains("different source or projection"), "{error}");
    let mut malformed = contract.clone();
    malformed["generated_assets"][0]["source_projection"] = json!(123);
    let error = crate::generated_assets::generated_writes_captured(
        &repo.0,
        "source_root",
        &repo.0,
        &malformed,
        &runner,
        &captured,
    )
    .unwrap_err();
    assert!(
        error.contains("source_projection must be a string"),
        "{error}"
    );
    fs::write(snapshot.join("Cargo.lock"), b"isolated external edit\n").unwrap();
    let error = crate::generated_assets::generated_writes_captured(
        &repo.0,
        "source_root",
        &repo.0,
        &contract,
        &runner,
        &captured,
    )
    .unwrap_err();
    assert!(error.contains("isolated build inputs changed"), "{error}");
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn factory_unchanged_generated_inputs_reuse_without_cargo_or_binary_writes() {
    let (repo, remote) = factory_with_valid_generated_assets("reuse");
    let runner = GeneratedRunner {
        root: repo.0.clone(),
        mode: "fail",
    };
    let plan = write_plan::WritePlan::prepare(
        &repo.0,
        Default::default(),
        Default::default(),
        true,
        &runner,
    )
    .unwrap();
    assert_eq!(plan.generated_reused, 2);
    assert_eq!(plan.generated_built, 0);
    assert!(
        !plan
            .writes
            .keys()
            .any(|p| p.starts_with(repo.0.join(".deps/bridgeforge/bin")))
    );
    plan.verify_unchanged(&repo.0).unwrap();
    let cli = repo.0.join(if cfg!(windows) {
        ".deps/bridgeforge/bin/bridgeforge.exe"
    } else {
        ".deps/bridgeforge/bin/bridgeforge"
    });
    fs::write(&cli, b"external concurrent replacement").unwrap();
    assert!(
        plan.verify_unchanged(&repo.0)
            .unwrap_err()
            .contains("changed concurrently")
    );
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn factory_reuse_invalidates_source_lock_binary_and_receipt_mismatches() {
    for mode in ["source", "lock", "binary", "receipt"] {
        let (repo, remote) = factory_with_valid_generated_assets(mode);
        let path = match mode {
            "source" => repo.0.join("templates/hooks/new.rs"),
            "lock" => repo.0.join("templates/hooks/Cargo.lock"),
            "binary" => repo.0.join(if cfg!(windows) {
                ".deps/bridgeforge/bin/bridgeforge.exe"
            } else {
                ".deps/bridgeforge/bin/bridgeforge"
            }),
            _ => repo.0.join(".deps/bridgeforge/bin/build-receipt-cli.json"),
        };
        fs::write(path, b"changed\n").unwrap();
        let runner = GeneratedRunner {
            root: repo.0.clone(),
            mode: "ok",
        };
        let plan = write_plan::WritePlan::prepare(
            &repo.0,
            Default::default(),
            Default::default(),
            true,
            &runner,
        )
        .unwrap();
        // Corrupt installed outputs can be repaired from the independent valid cache.
        let count = if matches!(mode, "source" | "lock") {
            2
        } else {
            0
        };
        assert_eq!(plan.generated_built, count, "{mode}");
        assert_eq!(plan.generated_reused, 2 - count, "{mode}");
        fs::remove_dir_all(remote).unwrap();
    }
}

#[test]
fn factory_release_never_reuses_previous_version_binaries() {
    let (repo, remote) = factory_with_valid_generated_assets("release-miss");
    let runner = GeneratedRunner {
        root: repo.0.clone(),
        mode: "ok",
    };
    let request = fixture::analysis(&repo.0, &runner, "patch", &[]).unwrap();
    let release = crate::release::schema4::build(&repo.0, "fix: release", &request, &runner)
        .unwrap()
        .unwrap();
    let plan =
        write_plan::WritePlan::prepare(&repo.0, release.writes, release.inputs, true, &runner)
            .unwrap();
    assert_eq!(plan.generated_built, 2);
    assert_eq!(plan.generated_reused, 0);
    assert_eq!(fs::read(repo.0.join("VERSION")).unwrap(), b"8.0.0\n");
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn factory_reuse_self_test_failure_rebuilds_and_drift_blocks_apply() {
    struct ReuseRunner {
        inner: GeneratedRunner,
        drift: bool,
    }
    impl ProcessRunner for ReuseRunner {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if Path::new(&request.program)
                .starts_with(self.inner.root.join(".deps/bridgeforge/bin"))
                || Path::new(&request.program)
                    .starts_with(self.inner.root.join(crate::artifact_cache::DIRECTORY))
                || is_short_cache_probe(Path::new(&request.program))
            {
                if self.drift {
                    fs::write(&request.program, b"external change")?;
                    return self.inner.run(request);
                }
                return Ok(out(0, "{}"));
            }
            self.inner.run(request)
        }
    }
    for drift in [false, true] {
        let (repo, remote) = factory_with_valid_generated_assets("self-test-reuse");
        let runner = ReuseRunner {
            inner: GeneratedRunner {
                root: repo.0.clone(),
                mode: "ok",
            },
            drift,
        };
        let result = write_plan::WritePlan::prepare(
            &repo.0,
            Default::default(),
            Default::default(),
            true,
            &runner,
        );
        if drift {
            assert!(result.err().unwrap().contains("changed concurrently"));
        } else {
            let plan = result.unwrap();
            assert_eq!(plan.generated_built, 2);
            assert_eq!(plan.generated_reused, 0);
        }
        fs::remove_dir_all(remote).unwrap();
    }
}

#[test]
fn factory_partial_reuse_rejected_commit_preserves_reused_asset_and_restores_miss() {
    let (repo, remote) = factory_with_valid_generated_assets("partial-reuse-rollback");
    // Keep this scenario as one installed hit plus one real build, not two cache hits.
    fs::remove_dir_all(repo.0.join(crate::artifact_cache::DIRECTORY)).unwrap();
    let config_path = repo.0.join(".codex/bridgeforge-version.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["release_policy"] = json!("explicit_release");
    fs::write(config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let hook = repo.0.join(if cfg!(windows) {
        ".deps/bridgeforge/bin/bridgeforge-hook.exe"
    } else {
        ".deps/bridgeforge/bin/bridgeforge-hook"
    });
    let cli = repo.0.join(if cfg!(windows) {
        ".deps/bridgeforge/bin/bridgeforge.exe"
    } else {
        ".deps/bridgeforge/bin/bridgeforge"
    });
    let receipt = repo.0.join(".deps/bridgeforge/bin/build-receipt-cli.json");
    let hook_before = fs::read(&hook).unwrap();
    let hook_time = fs::metadata(&hook).unwrap().modified().unwrap();
    fs::write(&cli, b"broken original CLI").unwrap();
    fs::write(&receipt, b"{}").unwrap();
    fs::write(repo.0.join(".git/hooks/pre-commit"), b"#!/bin/sh\nexit 1\n").unwrap();
    git_ok(&repo.0, &["status", "--porcelain=v1"]);
    let index = fs::read(repo.0.join(".git/index")).unwrap();
    struct CountingRunner {
        inner: GeneratedRunner,
        cargo: std::cell::Cell<usize>,
    }
    impl ProcessRunner for CountingRunner {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if request.program == "cargo" && request.args.first().is_some_and(|arg| arg == "build")
            {
                self.cargo.set(self.cargo.get() + 1);
            }
            self.inner.run(request)
        }
    }
    let runner = CountingRunner {
        inner: GeneratedRunner {
            root: repo.0.clone(),
            mode: "ok",
        },
        cargo: Default::default(),
    };
    let outcome = sync(
        &repo.0,
        &runner,
        GitSyncOptions {
            message: Some("docs: partial reuse rollback".into()),
            skip_fetch: true,
            skip_push: true,
            ..Default::default()
        },
    );
    assert_eq!(outcome.code, 2, "{}", outcome.stderr);
    assert!(outcome.stderr.contains("rolled back"), "{}", outcome.stderr);
    assert_eq!(runner.cargo.get(), 1);
    assert_eq!(fs::read(&hook).unwrap(), hook_before);
    assert_eq!(fs::metadata(&hook).unwrap().modified().unwrap(), hook_time);
    assert_eq!(fs::read(&cli).unwrap(), b"broken original CLI");
    assert_eq!(fs::read(&receipt).unwrap(), b"{}");
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(repo.0.join("tracked.txt")).unwrap(),
        b"user change\n"
    );
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn independent_cache_release_ignores_missing_or_corrupted_prepared() {
    for corrupt in [false, true] {
        let (repo, remote) = factory_repository("independent-cache-record");
        let runner = GeneratedRunner {
            root: repo.0.clone(),
            mode: "ok",
        };
        cache_release_candidate(&repo.0, &runner);
        let record = repo
            .0
            .join(".runtime/bridgeforge-codex/release-preparation/current.json");
        if corrupt {
            fs::create_dir_all(record.parent().unwrap()).unwrap();
            fs::write(&record, b"{invalid retired prepared record").unwrap();
        } else {
            assert!(!record.exists());
        }
        struct NoBuild<'a>(&'a GeneratedRunner);
        impl ProcessRunner for NoBuild<'_> {
            fn run(&self, r: &ProcessRequest) -> std::io::Result<ProcessOutput> {
                assert!(
                    !(r.program == "cargo" && r.args.first().is_some_and(|a| a == "build")),
                    "cache hit must not compile"
                );
                self.0.run(r)
            }
        }
        let result = sync(
            &repo.0,
            &NoBuild(&runner),
            GitSyncOptions {
                release: true,
                message: Some("fix: new description".into()),
                ..Default::default()
            },
        );
        assert_eq!(result.code, 0, "{}", result.stderr);
        let receipt = result.receipt.unwrap();
        assert_eq!(receipt["generated_assets_built"], 0);
        assert_eq!(receipt["generated_assets_reused"], 2);
        fs::remove_dir_all(remote).unwrap();
    }
}

#[test]
fn mixed_cache_hit_and_build_rejected_commit_restore_all_targets() {
    let (repo, remote) = factory_repository("mixed-cache-rollback");
    let runner = GeneratedRunner {
        root: repo.0.clone(),
        mode: "ok",
    };
    cache_release_candidate(&repo.0, &runner);
    let directory = repo.0.join(crate::artifact_cache::DIRECTORY);
    for item in fs::read_dir(&directory).unwrap() {
        let item = item.unwrap();
        let meta = item.path().join("entry.json");
        if meta.is_file() {
            let value: serde_json::Value =
                serde_json::from_slice(&fs::read(meta).unwrap()).unwrap();
            if value["identity"]["id"] == "codex.bridgeforge-cli" {
                fs::remove_dir_all(item.path()).unwrap();
            }
        }
    }
    fs::write(repo.0.join(".git/hooks/pre-commit"), b"#!/bin/sh\nexit 1\n").unwrap();
    git_ok(&repo.0, &["status", "--porcelain=v1"]);
    let index = fs::read(repo.0.join(".git/index")).unwrap();
    struct MixedRunner {
        inner: GeneratedRunner,
        builds: std::cell::Cell<usize>,
        hits: std::cell::Cell<usize>,
    }
    impl ProcessRunner for MixedRunner {
        fn run(&self, r: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if r.program == "cargo" && r.args.first().is_some_and(|a| a == "build") {
                self.builds.set(self.builds.get() + 1);
            }
            if (Path::new(&r.program)
                .starts_with(self.inner.root.join(crate::artifact_cache::DIRECTORY))
                || is_short_cache_probe(Path::new(&r.program)))
                && r.args.first().is_some_and(|a| a == "self-test")
            {
                self.hits.set(self.hits.get() + 1);
            }
            self.inner.run(r)
        }
    }
    let paths = [
        "VERSION",
        "CHANGELOG.md",
        "templates/hooks/Cargo.toml",
        "templates/hooks/Cargo.lock",
        ".codex/hooks/Cargo.toml",
        ".codex/hooks/Cargo.lock",
        "source/forge/bridgeforge-manifest.json",
        ".codex/managed-skeleton.json",
        ".deps/bridgeforge/bin/build-receipt-cli.json",
        ".deps/bridgeforge/bin/build-receipt-hook.json",
    ];
    let mut before = paths
        .into_iter()
        .map(|p| {
            let p = repo.0.join(p);
            let bytes = fs::read(&p).ok();
            (p, bytes)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    for name in ["bridgeforge", "bridgeforge-hook"] {
        let path = repo.0.join(".deps/bridgeforge/bin").join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        });
        before.insert(path.clone(), fs::read(path).ok());
    }
    let counted = MixedRunner {
        inner: runner,
        builds: Default::default(),
        hits: Default::default(),
    };
    let result = sync(
        &repo.0,
        &counted,
        GitSyncOptions {
            release: true,
            message: Some("fix: mixed cache".into()),
            skip_fetch: true,
            skip_push: true,
            ..Default::default()
        },
    );
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(result.stderr.contains("rolled back"));
    assert_eq!(fs::read(repo.0.join("VERSION")).unwrap(), b"8.0.0\n");
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), index);
    assert!(
        !repo
            .0
            .join(".deps/bridgeforge/bin/bridgeforge.exe")
            .exists()
    );
    assert!(
        !repo
            .0
            .join(".deps/bridgeforge/bin/bridgeforge-hook.exe")
            .exists()
    );
    assert_eq!(
        counted.builds.get(),
        1,
        "mixed scenario must really compile one miss"
    );
    assert_eq!(
        counted.hits.get(),
        1,
        "mixed scenario must really self-test one cache hit"
    );
    for (path, bytes) in before {
        assert_eq!(
            fs::read(&path).ok(),
            bytes,
            "automatic target drift: {}",
            path.display()
        );
    }
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn factory_read_only_status_queries_do_not_initialize_artifact_cache() {
    let (repo, remote) = factory_repository("cache-readonly");
    assert!(!repo.0.join(crate::artifact_cache::DIRECTORY).exists());
    let runner = GeneratedRunner {
        root: repo.0.clone(),
        mode: "fail",
    };
    let _ = release_status(&repo.0, &runner);
    assert!(!repo.0.join(crate::artifact_cache::DIRECTORY).exists());
    assert_eq!(fs::read(repo.0.join("VERSION")).unwrap(), b"8.0.0\n");
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn direct_release_preserves_worktree_and_reuses_cached_tools() {
    let (repo, remote) = factory_repository("prepared-factory");
    let before_version = fs::read(repo.0.join("VERSION")).unwrap();
    let before_index = fs::read(repo.0.join(".git/index")).unwrap();
    cache_release_candidate(
        &repo.0,
        &GeneratedRunner {
            root: repo.0.clone(),
            mode: "ok",
        },
    );
    assert_eq!(fs::read(repo.0.join("VERSION")).unwrap(), before_version);
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), before_index);
    let result = sync(
        &repo.0,
        &GeneratedRunner {
            root: repo.0.clone(),
            mode: "ok",
        },
        GitSyncOptions {
            release: true,
            message: Some("fix: new description".into()),
            ..Default::default()
        },
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt["version_after"], "8.0.1");
    assert_eq!(receipt["generated_assets_built"], 0);
    assert_eq!(receipt["generated_assets_reused"], 2);
    assert!(
        fs::read_to_string(repo.0.join("CHANGELOG.md"))
            .unwrap()
            .contains("new description")
    );
    fs::remove_dir_all(remote).unwrap();
}

fn cache_release_candidate(root: &Path, runner: &dyn ProcessRunner) {
    let request = fixture::analysis(root, runner, "patch", &[]).unwrap();
    let release = crate::release::schema4::build(root, "fix: cache", &request, runner)
        .unwrap()
        .unwrap();
    crate::git_sync_plan::WritePlan::prepare(root, release.writes, release.inputs, true, runner)
        .unwrap();
}
