#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

use super::*;
use std::fs;

#[test]
fn retired_operations_fail_before_repository_or_process_access() {
    struct NeverRun;
    impl bridgeforge_core::ProcessRunner for NeverRun {
        fn run(
            &self,
            _: &bridgeforge_core::ProcessRequest,
        ) -> std::io::Result<bridgeforge_core::ProcessOutput> {
            panic!("retired operations must not execute processes");
        }
    }
    for flag in [
        "--prepare-release",
        "--development-status",
        "--prepare-release=true",
        "--development-status=true",
    ] {
        let result = git_sync_with_runner(
            &[
                "git-sync".into(),
                flag.into(),
                "--root".into(),
                "does-not-exist".into(),
            ],
            &NeverRun,
        );
        assert_eq!(result.code, EXIT_BLOCKED);
        assert!(result.stderr.contains("retired"), "{}", result.stderr);
    }
    assert_eq!(run(&["batch".into()]).code, EXIT_BLOCKED);
}
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn baseline_uses_compiled_template_and_never_initializes_project_contract() {
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "cli").join(format!("template-baseline-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    for asset in bridgeforge_core::template_standard::assets() {
        let bytes = bridgeforge_skeleton::content::render(asset.rule["id"].as_str().unwrap(), &asset.payload, None, &asset.rule, &root, true).unwrap();
        let path = root.join(asset.rule["target"].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap(); fs::write(path, bytes).unwrap();
    }
    let args = vec!["check".into(),"baseline".into(),"--root".into(),root.display().to_string(),"--skip-generated-runtime".into()];
    let result = run(&args); assert_eq!(result.code,0,"{}",result.stderr);
    assert!(!root.join(".codex/managed-skeleton.json").exists());
    let hooks_path = root.join(".codex/hooks.json"); let bytes = fs::read(&hooks_path).unwrap();
    let mut hooks: Value = serde_json::from_slice(&bytes).unwrap();
    hooks["hooks"]["PostToolUse"][0]["hooks"][0]["command"] = json!("unexpected-hook");
    fs::write(&hooks_path,serde_json::to_vec(&hooks).unwrap()).unwrap();
    assert!(run(&args).stderr.contains("managed hook payload drifted"));
    fs::write(&hooks_path,bytes).unwrap();
    fs::write(root.join(".codex/managed-skeleton.json"),b"invalid never parsed").unwrap();
    assert!(run(&args).stderr.contains("exactly hooks.json"));
    fs::remove_file(root.join(".codex/managed-skeleton.json")).unwrap();
    assert_eq!(run(&args).code,0); fs::remove_dir_all(root).unwrap();
}

#[test]
fn retired_asset_migration_arguments_reject_before_reading_stdin_or_project() {
    for flag in [
        "--asset-migration-manifest",
        "--confirmed-asset-migration",
        "--asset-migration-manifest=-",
    ] {
        let result = run(&[
            "project-sync".into(),
            flag.into(),
            "-".into(),
            "--apply".into(),
        ]);
        assert_eq!(result.code, EXIT_BLOCKED);
        assert!(
            result.stderr.contains("retired"),
            "{}",
            result.stderr
        );
    }
}

#[test]
fn retired_development_capabilities_are_not_advertised() {
    let identity = self_test().receipt.unwrap();
    for capability in ["template-only-v1","compiled-build-provenance-v1","user-cli-reuse-v1","public-artifacts-v1"] {
        assert!(identity["capabilities"].as_array().unwrap().iter().any(|c| c==capability));
    }
    for capability in ["package-development-v1","package-stability-v1","git-sync-development-closure-v1","version-coverage-v1","git-sync-explicit-release-v1"] {
        assert!(!identity["capabilities"].as_array().unwrap().iter().any(|c| c==capability));
    }
    for flags in [vec!["--apply","--dry-run"],vec!["--validate"],vec!["--recover"],vec!["--preview"],vec![]] {
        let mut args=vec!["develop".into()]; args.extend(flags.into_iter().map(String::from));
        let out=run(&args); assert_eq!(out.code,2); assert!(out.stderr.contains("retired"));
    }
}

#[test]
fn task_development_cli_rejects_malformed_selection_before_forwarding() {
    for flags in [
        vec!["--preview", "--packages"],
        vec!["--preview", "--packages", "a", "--packages"],
        vec!["--preview", "--packages", "--root", "missing"],
        vec!["--preview", "--packages=a"],
        vec!["--preview", "--packages", "a,a"],
        vec!["--recover", "--packages", "a"],
    ] {
        let mut args = vec!["develop".to_string()];
        args.extend(flags.into_iter().map(str::to_string));
        let result = run(&args);
        assert_eq!(result.code, EXIT_BLOCKED);
        assert!(result.stderr.contains("retired"), "{}", result.stderr);
    }
}

#[test]
fn todo_contract_query_is_read_only_and_invalid_table_blocks_archive() {
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "cli").join(format!(
        "bf-cli-todo-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("doc/0_architecture")).unwrap();
    let args = vec![
        "check".into(),
        "todo".into(),
        "--root".into(),
        root.display().to_string(),
        "--contract".into(),
    ];
    let result = run(&args);
    assert_eq!(result.code, 0);
    assert_eq!(
        result.receipt.unwrap()["columns"].as_array().unwrap().len(),
        6
    );
    let file = root.join("doc/0_architecture/TODO-INDEX.md");
    assert!(!file.exists(), "contract query must not initialize a table");
    fs::write(&file, "damaged table\n").unwrap();
    let before = fs::read(&file).unwrap();
    let result = run(&[
        "check".into(),
        "todo".into(),
        "--root".into(),
        root.display().to_string(),
    ]);
    assert_eq!(result.code, EXIT_BLOCKED);
    assert!(
        !result.receipt.unwrap()["errors"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let result = run(&[
        "archive-scan".into(),
        "--root".into(),
        root.display().to_string(),
    ]);
    assert_eq!(result.code, EXIT_BLOCKED);
    assert_eq!(fs::read(&file).unwrap(), before);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn user_instruction_staging_rejects_preview_flags_before_writing() {
    for flag in ["--check", "--dry-run"] {
        let args = vec!["user-agents-stage".into(), flag.into()];
        let outcome = run(&args);
        assert_eq!(outcome.code, EXIT_BLOCKED);
        assert!(outcome.stderr.contains("preview flags are not supported"));
    }
}

#[test]
fn default_metadata_gate_checks_native_project_skills() {
    let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "cli").join(format!(
        "bf-skill-gate-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(home.join(".codex/skills/broken")).unwrap();
    fs::write(
        home.join(".codex/skills/broken/SKILL.md"),
        b"# Missing metadata\n",
    )
    .unwrap();
    let result = run(&[
        "check".into(),
        "skill-metadata".into(),
        "--root".into(),
        home.display().to_string(),
    ]);
    assert_eq!(result.code, EXIT_BLOCKED);
    assert!(
        result.receipt.unwrap()["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue.as_str().unwrap().contains("frontmatter"))
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn self_test_has_stable_identity() {
    let receipt = self_test().receipt.expect("receipt");
    assert_eq!(receipt["name"], "bridgeforge");
    assert_eq!(receipt["status"], "ok");
}

#[test]
fn cleanup_factory_routing_requires_the_private_tool_before_deleting_anything() {
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "cli").join(format!(
        "cleanup-routing-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("source/factory")).unwrap();
    fs::create_dir_all(root.join("templates")).unwrap();
    fs::create_dir_all(root.join(".build")).unwrap();
    fs::write(root.join("source/factory/Cargo.toml"), b"factory marker").unwrap();
    fixture_product::write(root.join("source/forge/bridgeforge-manifest.json"), b"{}").unwrap();
    fs::write(root.join(".build/sample.bin"), b"preserved on failure").unwrap();
    let result = run(&[
        "cleanup".into(),
        "--root".into(),
        root.display().to_string(),
    ]);
    assert_eq!(result.code, EXIT_BLOCKED);
    assert!(
        result.stderr.contains("factory runtime") || result.stderr.contains("manifest"),
        "{}",
        result.stderr
    );
    assert_eq!(
        fs::read(root.join(".build/sample.bin")).unwrap(),
        b"preserved on failure"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ordinary_project_with_factory_named_member_keeps_public_routing() {
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "cli").join(format!(
        "bf-ordinary-factory-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("source/factory")).unwrap();
    fs::write(
        root.join("source/factory/Cargo.toml"),
        b"[package]\nname='business-factory'\nversion='0.1.0'\n",
    )
    .unwrap();
    assert!(!is_factory_repository(&root));
    let result = run(&[
        "develop".into(),
        "--root".into(),
        root.display().to_string(),
        "--preview".into(),
    ]);
    assert_eq!(result.code, EXIT_BLOCKED);
    assert!(
        !result.stderr.contains("factory repository"),
        "{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("factory runtime"),
        "{}",
        result.stderr
    );
    assert!(!root.join(".build/bridgeforge/bin").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn factory_relay_requires_current_source_binary_and_receipt_before_running() {
    use bridgeforge_core::{
        ProcessOutput, ProcessRequest, ProcessRunner, build_manifest, source_projection,
    };
    use std::cell::Cell;
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "cli").join(format!(
        "bf-factory-relay-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for directory in [
        "source/core/src",
        "source/cli/src",
        "source/factory/src",
        "templates",
        ".build/bridgeforge/bin",
    ] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    fs::write(root.join("Cargo.toml"),b"[workspace]\nmembers=['source/core','source/cli','source/factory']\n[workspace.package]\nversion='6.0.0'\n").unwrap();
    fs::write(root.join("Cargo.lock"),b"version=4\n[[package]]\nname='bridgeforge-core'\nversion='6.0.0'\n[[package]]\nname='bridgeforge-cli'\nversion='6.0.0'\n[[package]]\nname='bridgeforge-factory'\nversion='0.1.0'\n").unwrap();
    for (directory, version) in [("core", "6.0.0"), ("cli", "6.0.0"), ("factory", "0.1.0")] {
        fs::write(
            root.join(format!("source/{directory}/Cargo.toml")),
            format!("[package]\nname='bridgeforge-{directory}'\nversion='{version}'\n"),
        )
        .unwrap();
        fs::write(
            root.join(format!("source/{directory}/src/lib.rs")),
            b"pub fn run() {}\n",
        )
        .unwrap();
    }
    fs::write(root.join("VERSION"), b"6.0.0\n").unwrap();
    fixture_product::write(root.join("source/forge/bridgeforge-manifest.json"), b"{}").unwrap();
    let binary = root.join(format!(
        ".build/bridgeforge/bin/bridgeforge-factory{}",
        std::env::consts::EXE_SUFFIX
    ));
    let receipt_path = root.join(".build/bridgeforge/bin/build-receipt-factory.json");
    let image = b"fixture-factory-image";
    fs::write(&binary, image).unwrap();
    let projected = source_projection::render(&root, false).unwrap();
    let test = json!({"args":["self-test","--json"],"expected_json":{"schema":1,"name":"bridgeforge-factory","status":"ok","version":"0.1.0","product_version":"6.0.0"}});
    let receipt = json!({"schema_version":2,"generated_asset_id":"codex.factory-cli",
        "platform":bridgeforge_core::build_receipt::platform_key().unwrap(),
        "binary_sha256":build_manifest::normalized_hash(image),
        "source_tree_sha256":source_projection::tree_sha(&projected).unwrap(),
        "lockfile_sha256":build_manifest::normalized_hash(&projected["Cargo.lock"]),
        "build_recipe_sha256":build_manifest::canonical_sha(&build_manifest::generated_build_recipe("bridgeforge-factory")).unwrap(),
        "self_test_sha256":build_manifest::canonical_sha(&test).unwrap()});
    fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    struct Runner(Cell<usize>);
    impl ProcessRunner for Runner {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            assert_eq!(request.args[0], "manifest");
            self.0.set(self.0.get() + 1);
            Ok(ProcessOutput {
                code: 0,
                stdout: b"{\"schema\":1,\"status\":\"ok\"}".to_vec(),
                stderr: vec![],
                timed_out: false,
            })
        }
    }
    let runner = Runner(Cell::new(0));
    let args = vec![
        "manifest".into(),
        "--root".into(),
        root.display().to_string(),
        "--check".into(),
    ];
    let result = factory_command_with_runner(&args, &runner);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(runner.0.get(), 1);
    for changed in [
        root.join("source/core/src/lib.rs"),
        root.join("source/cli/Cargo.toml"),
        binary.clone(),
        receipt_path.clone(),
    ] {
        let before = fs::read(&changed).unwrap();
        fs::write(&changed, b"drifted").unwrap();
        assert_eq!(
            factory_command_with_runner(&args, &runner).code,
            EXIT_BLOCKED
        );
        assert_eq!(runner.0.get(), 1, "drift must block before execution");
        fs::write(changed, before).unwrap();
    }
    struct Refresh<'a> {
        root: &'a Path,
        binary: &'a Path,
        receipt: &'a Path,
        calls: Cell<usize>,
        valid_receipt: bool,
        valid_self_test: bool,
        drift_after_probe: bool,
    }
    impl ProcessRunner for Refresh<'_> {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            self.calls.set(self.calls.get() + 1);
            if request.args[0] == "self-test" {
                let (_, mut identity) = factory_spec(self.root).unwrap();
                if !self.valid_self_test {
                    identity["product_version"] = json!("6.0.0");
                }
                if self.drift_after_probe {
                    fs::write(self.receipt, b"external receipt").unwrap();
                }
                return Ok(ProcessOutput {
                    code: 0,
                    stdout: serde_json::to_vec(&identity).unwrap(),
                    stderr: vec![],
                    timed_out: false,
                });
            }
            assert_eq!(request.args[0], "manifest");
            fs::write(self.root.join("VERSION"), b"7.0.0\n").unwrap();
            let cargo = fs::read_to_string(self.root.join("Cargo.toml"))
                .unwrap()
                .replace("version='6.0.0'", "version='7.0.0'");
            fs::write(self.root.join("Cargo.toml"), cargo).unwrap();
            let cli = self.root.join("source/cli/Cargo.toml");
            let cargo = fs::read_to_string(&cli).unwrap()
                .replace("version='6.0.0'", "version='7.0.0'");
            fs::write(cli, cargo).unwrap();
            let image = b"managed refreshed factory image";
            fs::write(self.binary, image).unwrap();
            if self.valid_receipt {
                let (asset, _) = factory_spec(self.root).unwrap();
                let receipt = json!({"schema_version":2,"generated_asset_id":"codex.factory-cli",
                    "platform":bridgeforge_core::build_receipt::platform_key().unwrap(),
                    "binary_sha256":build_manifest::normalized_hash(image),
                    "source_tree_sha256":asset["source_tree_sha256"],"lockfile_sha256":asset["lockfile_sha256"],
                    "build_recipe_sha256":asset["build_recipe_sha256"],"self_test_sha256":asset["self_test_sha256"]});
                fs::write(self.receipt, serde_json::to_vec(&receipt).unwrap()).unwrap();
            }
            Ok(ProcessOutput {
                code: 0,
                stdout: b"{\"schema\":1,\"status\":\"synced\",\"commit\":\"fixture\"}".to_vec(),
                stderr: vec![],
                timed_out: false,
            })
        }
    }
    let original_cargo = fs::read(root.join("Cargo.toml")).unwrap();
    let original_cli = fs::read(root.join("source/cli/Cargo.toml")).unwrap();
    for (valid_receipt, valid_self_test, drift_after_probe) in [
        (false, true, false),
        (true, false, false),
        (true, true, true),
        (true, true, false),
    ] {
        fs::write(root.join("VERSION"), b"6.0.0\n").unwrap();
        fs::write(root.join("Cargo.toml"), &original_cargo).unwrap();
        fs::write(root.join("source/cli/Cargo.toml"), &original_cli).unwrap();
        fs::write(&binary, image).unwrap();
        fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        let refresh = Refresh {
            root: &root,
            binary: &binary,
            receipt: &receipt_path,
            calls: Cell::new(0),
            valid_receipt,
            valid_self_test,
            drift_after_probe,
        };
        let result = factory_command_with_runner(&args, &refresh);
        if valid_receipt && valid_self_test && !drift_after_probe {
            assert_eq!(result.code, 0, "{}", result.stderr);
            assert_eq!(result.receipt.unwrap()["commit"], "fixture");
            assert_eq!(
                refresh.calls.get(),
                2,
                "managed refresh must validate the replacement self-test"
            );
        } else {
            assert_eq!(result.code, EXIT_BLOCKED, "{}", result.stderr);
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_sync_preflight_is_capability_gated_and_mutually_exclusive() {
    let capabilities = self_test().receipt.unwrap()["capabilities"]
        .as_array()
        .unwrap()
        .clone();
    assert!(capabilities.iter().any(|v| v == "git-sync-preflight-v1"));
    assert!(capabilities.iter().any(|v| v == "template-only-v1"));
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "cli").join(format!(
        "bf-cli-preflight-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join(".codex")).unwrap();
    for mode in ["--release", "--setup-apply"] {
        let result = run(&[
            "git-sync".into(),
            "--root".into(),
            root.display().to_string(),
            "--preflight".into(),
            mode.into(),
        ]);
        assert_eq!(result.code, EXIT_BLOCKED);
        assert!(result.stderr.contains("retired"));
        assert!(!root.join(".git").exists());
        assert!(!root.join(".runtime").exists());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ordinary_sync_does_not_require_release_choice_but_release_does() {
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "cli").join(format!(
        "bf-cli-first-use-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("app.py"), "pass\n").unwrap();
    for args in [
        vec!["init"],
        vec!["config", "user.name", "Fixture"],
        vec!["config", "user.email", "fixture@example.invalid"],
        vec!["config", "core.hooksPath", ".git/hooks"],
        vec!["add", "."],
        vec!["commit", "-m", "chore: initial"],
    ] {
        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let index = fs::read(root.join(".git/index")).unwrap();
    for release in [false, true] {
        let mut args = vec![
            "git-sync".into(),
            "--root".into(),
            root.display().to_string(),
            "--message".into(),
            "chore: test".into(),
        ];
        if release {
            args.push("--release".into());
        }
        let output = run(&args);
        assert_eq!(output.code, EXIT_BLOCKED);
        if release {
            assert!(output.stderr.contains("retired"));
        } else {
            assert_eq!(output.receipt.unwrap()["status"], "blocked");
            assert!(
                output.stderr.contains("no push target"),
                "{}",
                output.stderr
            );
        }
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        assert!(!root.join(".git/FETCH_HEAD").exists());
        assert!(!root.join(".runtime").exists());
    }
    fs::remove_dir_all(root).unwrap();
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;

#[test]
fn factory_skill_metadata_checks_nested_template_sources() {
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "cli")
        .join(format!("nested-skill-metadata-{}", std::process::id()));
    fs::create_dir_all(root.join("source/forge/templates/skills/broken")).unwrap();
    fs::write(root.join("source/forge/bridgeforge-manifest.json"), b"{}").unwrap();
    fs::write(root.join("source/forge/templates/skills/broken/SKILL.md"), b"invalid skill metadata\n").unwrap();
    let result = run(&["check".into(), "skill-metadata".into(), "--root".into(), root.to_string_lossy().into_owned()]);
    assert_ne!(result.code, 0, "invalid source skill must not silently disappear after regrouping");
    assert!(result.receipt.unwrap().to_string().contains("broken"));
    fs::remove_dir_all(root).unwrap();
}
