#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

use bridgeforge_core::{ProcessOutput, ProcessRequest, ProcessRunner, SystemProcessRunner};
use bridgeforge_memory::remote::{
    apply_conflict_choices, reconcile, resolve_conflict_with_choices,
};
use bridgeforge_memory::worker::mark_pending;
use bridgeforge_workflow::project_sync::{
    SyncMode, apply_plan, build_plan, build_plan_with_inputs,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const MEMORY_REMOTE: &str = "https://github.com/owner/bridgeforge-codex-memories.git";

// Only the factory tests route an approved GitHub identity to an isolated local
// bare repository. Production code has no local-remote or privacy bypass.
struct LocalMemoryRunner(PathBuf, Option<&'static str>);

impl ProcessRunner for LocalMemoryRunner {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        assert!(request.cwd.starts_with(self.0.parent().unwrap()));
        if request.program == "gh" {
            assert_eq!(
                request.args,
                [
                    "repo",
                    "view",
                    "https://github.com/owner/bridgeforge-codex-memories",
                    "--json",
                    "visibility",
                    "--jq",
                    ".visibility"
                ]
            );
            return Ok(ProcessOutput {
                code: 0,
                stdout: b"PRIVATE\n".to_vec(),
                stderr: Vec::new(),
                timed_out: false,
            });
        }
        assert_eq!(request.program, "git");
        if request.args == ["remote", "get-url", "--push", "--all", "origin"] {
            return Ok(ProcessOutput {
                code: 0,
                stdout: self.1.unwrap_or(MEMORY_REMOTE).as_bytes().to_vec(),
                stderr: Vec::new(),
                timed_out: false,
            });
        }
        if request.args == ["ls-remote", "--get-url", MEMORY_REMOTE]
            || request.args == ["remote", "get-url", "--all", "origin"]
        {
            return Ok(ProcessOutput {
                code: 0,
                stdout: MEMORY_REMOTE.as_bytes().to_vec(),
                stderr: Vec::new(),
                timed_out: false,
            });
        }
        let mut local = request.clone();
        for argument in &mut local.args {
            if argument == MEMORY_REMOTE {
                *argument = self.0.clone().into_os_string();
            }
            assert!(
                !argument.to_string_lossy().contains("https://"),
                "no network in fixture"
            );
        }
        SystemProcessRunner.run(&local)
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let token = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "runtime-flows").join(format!(
            "bridgeforge-rust-{label}-{}-{token}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, payload: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, payload).unwrap();
}

fn git_at(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn project_sync_plan_apply_and_fingerprint_gate_are_transactional() {
    let temp = TestDirectory::new("project-sync");
    let factory = temp.0.join("factory");
    let project = temp.0.join("project");
    fs::create_dir_all(factory.join("templates")).unwrap();
    fs::create_dir_all(&project).unwrap();
    write(&factory.join("VERSION"), b"8.0.1\n");
    write(&bridgeforge_core::source_projection::template_path(&factory, "templates/example.txt"), b"managed\n");
    let contract = json!({
        "schema_version": 4,
        "release_version": "8.0.1",
        "host": "codex",
        "stamp": ".codex/.bridgeforge_codex_version",
        "contract_target": ".codex/managed-skeleton.json",
        "baseline_model": "current-only", "compatibility_baseline": "8.0.1",
        "assets": [{
            "id": "example",
            "source": "templates/example.txt",
            "target": ".codex/example.txt",
            "strategy": "whole",
            "current_sha256": format!("sha256:{:x}", Sha256::digest(b"managed\n"))
        }],
        "generated_assets": []
    });
    fixture_product::install(
        &factory.join("bridgeforge-manifest.json"),
        format!("{}\n", serde_json::to_string_pretty(&contract).unwrap()).as_bytes(),
    );

    let plan = build_plan(&project, &factory, SyncMode::Init).unwrap();
    assert!(!plan.confirmation_required);
    let wrong = apply_plan(plan.clone(), "sha256:wrong", false).unwrap_err();
    assert!(wrong.contains("fingerprint"));
    assert!(!project.join(".codex/example.txt").exists());

    write(&project.join(".codex/example.txt"), b"external drift\n");
    let drift = apply_plan(plan.clone(), &plan.aggregate_fingerprint, false).unwrap_err();
    assert!(
        drift.contains("project input changed after plan"),
        "{drift}"
    );
    assert_eq!(
        fs::read(project.join(".codex/example.txt")).unwrap(),
        b"external drift\n"
    );
    assert!(!project.join(".codex/managed-skeleton.json").exists());
    assert!(!project.join(".codex/.bridgeforge_codex_version").exists());
    fs::remove_file(project.join(".codex/example.txt")).unwrap();

    let fingerprint = plan.aggregate_fingerprint.clone();
    let receipt = apply_plan(plan, &fingerprint, false).unwrap();
    assert_eq!(receipt.execution_status, "succeeded");
    assert_eq!(
        fs::read(project.join(".codex/example.txt")).unwrap(),
        b"managed\n"
    );
    assert_eq!(
        fs::read_to_string(project.join(".codex/.bridgeforge_codex_version"))
            .unwrap()
            .trim(),
        "8.0.1"
    );
}

#[test]
fn project_settings_remain_project_owned_across_init_adopt_and_update() {
    let temp = TestDirectory::new("settings-ownership");
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let factory = temp.0.join("factory");
    let mut contract: serde_json::Value = serde_json::from_slice(
        &fixture_product::read(repository.join("bridgeforge-manifest.json")).unwrap(),
    )
    .unwrap();
    for key in ["assets", "retired_assets"] {
        assert!(contract[key].as_array().unwrap().iter().all(|asset| {
            asset["id"] != "codex.settings" && asset["target"] != ".codex/settings.json"
        }));
    }
    // Exercise the real distributed file contract without compiling binaries.
    contract["generated_assets"] = json!([]);
    for asset in contract["assets"].as_array().unwrap() {
        if asset.get("content").is_some() {
            continue;
        }
        let source = asset["source"].as_str().unwrap();
        write(
            &factory.join(source),
            &fs::read(repository.join(source)).unwrap(),
        );
    }
    write(
        &factory.join("VERSION"),
        &fs::read(repository.join("VERSION")).unwrap(),
    );
    let contract_path = factory.join("bridgeforge-manifest.json");
    fixture_product::install(&contract_path,
        &serde_json::to_vec_pretty(&contract).unwrap(),
    );
    let original = b"{\r\n  \"effortLevel\":\"high\", \"permissions\":{}, \"custom\":42\r\n}\r\n";
    for (name, mode, settings) in [
        ("init", SyncMode::Init, None),
        ("adopt", SyncMode::Adopt, Some(original.as_slice())),
        (
            "adopt-invalid",
            SyncMode::Adopt,
            Some(b"project-owned non-JSON".as_slice()),
        ),
    ] {
        let project = temp.0.join(name);
        fs::create_dir_all(&project).unwrap();
        let path = project.join(".codex/settings.json");
        if let Some(bytes) = settings {
            write(&path, bytes);
            write(&path.with_extension("json.bak"), b"previous backup");
        }
        for current_mode in [mode, SyncMode::Update] {
            let plan = build_plan(&project, &factory, current_mode).unwrap();
            if matches!(current_mode, SyncMode::Adopt) {
                for target in [".codex/settings.json", ".codex/settings.json.bak"] {
                    assert!(
                        plan.preservation_manifest["entries"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|item| {
                                item["target"] == target
                                    && item["disposition"] == "required-preserve"
                            })
                    );
                    let decisions = json!({"delete": [format!("P:project-file:{target}")]});
                    let result =
                        build_plan_with_inputs(&project, &factory, current_mode, Some(&decisions));
                    assert!(
                        result
                            .unwrap_err()
                            .contains("unknown project asset decision")
                    );
                }
            }
            assert!(
                plan.safe
                    .iter()
                    .chain(&plan.risk)
                    .all(|a| a.target != ".codex/settings.json")
            );
            let fingerprint = plan.aggregate_fingerprint.clone();
            let receipt = apply_plan(plan, &fingerprint, true).unwrap();
            assert_eq!(receipt.execution_status, "succeeded");
            assert_eq!(fs::read(&path).ok().as_deref(), settings);
            assert_eq!(
                fs::read(path.with_extension("json.bak")).ok().as_deref(),
                settings.map(|_| b"previous backup".as_slice())
            );
        }
    }
    // An old installation declared settings as managed. Dropping ownership must
    // preserve both an unchanged old payload and a locally modified payload.
    let old_payload = b"{\"permissions\":{\"defaultMode\":\"acceptEdits\"}}\n";
    let mut legacy = contract.clone();
    legacy["assets"].as_array_mut().unwrap().push(json!({
        "id":"codex.settings", "source":"templates/settings.json",
        "target":".codex/settings.json", "strategy":"whole",
        "current_sha256": format!("sha256:{:x}", Sha256::digest(old_payload))
    }));
    write(&factory.join("templates/settings.json"), old_payload);
    for (name, bytes) in [
        ("old-default", old_payload.as_slice()),
        ("old-custom", original.as_slice()),
    ] {
        let project = temp.0.join(name);
        fs::create_dir_all(&project).unwrap();
        fixture_product::install(&contract_path, &serde_json::to_vec_pretty(&legacy).unwrap());
        let plan = build_plan(&project, &factory, SyncMode::Init).unwrap();
        let fingerprint = plan.aggregate_fingerprint.clone();
        apply_plan(plan, &fingerprint, true).unwrap();
        let path = project.join(".codex/settings.json");
        write(&path, bytes);
        fixture_product::install(&contract_path,
            &serde_json::to_vec_pretty(&contract).unwrap(),
        );
        let plan = build_plan(&project, &factory, SyncMode::Update).unwrap();
        let fingerprint = plan.aggregate_fingerprint.clone();
        let receipt = apply_plan(plan, &fingerprint, true).unwrap();
        assert_eq!(receipt.execution_status, "succeeded");
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!path.with_extension("json.bak").exists());
    }
}

#[test]
fn prepared_public_artifacts_are_shared_and_update_preserves_project_hooks() {
    use bridgeforge_workflow::{project_sync::attach_prepared_public, public_artifacts};
    let temp = TestDirectory::new("prepared-update");
    let factory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let codex = temp.0.join("profile/.codex");
    fs::create_dir_all(&codex).unwrap();
    let contract_bytes = fixture_product::read(factory.join("bridgeforge-manifest.json")).unwrap();
    let contract: serde_json::Value = serde_json::from_slice(&contract_bytes).unwrap();
    let prepared =
        public_artifacts::prepare(factory, &codex, None, None, None, &SystemProcessRunner).unwrap();
    assert_eq!(prepared["build_metrics"]["cargo_build_invocations"], 2);
    let prepared_path = PathBuf::from(prepared["receipt"].as_str().unwrap());
    for name in ["first", "second"] {
        let project = temp.0.join(name);
        fs::create_dir(&project).unwrap();
        let mut plan = build_plan(&project, factory, SyncMode::Init).unwrap();
        assert!(plan.public_installation_required);
        assert!(
            !project.join(".codex").exists(),
            "project preview must be read only"
        );
        assert_eq!(plan.build_metrics.cargo_build_invocations, 0);
        let fingerprint = plan.aggregate_fingerprint.clone();
        attach_prepared_public(&mut plan, &contract_bytes, &prepared_path).unwrap();
        assert_eq!(plan.aggregate_fingerprint, fingerprint);
        let installed = apply_plan(plan, &fingerprint, false).unwrap();
        assert!(
            installed
                .applied
                .iter()
                .any(|a| a.id.starts_with("public:"))
        );
        bridgeforge_skeleton::baseline::verify(&project, None, true).unwrap();
        // A legacy project's private program and even invalid source must not
        // be touched or compiled by ordinary update.
        let native = project.join(".codex/hooks.json");
        let mut hooks: serde_json::Value =
            serde_json::from_slice(&fs::read(&native).unwrap()).unwrap();
        hooks["hooks"]["Stop"].as_array_mut().unwrap().push(json!({"hooks":[{"type":"command","bridgeforgeProjectHookId":"demo:0","command":".build/bridgeforge/bin/project_demo"}]}));
        write(&native, &serde_json::to_vec_pretty(&hooks).unwrap());
        write(
            &project.join(".codex/hooks/project_demo/entrypoint.rs"),
            b"not Rust; private project data",
        );
        let private_binary = project.join(format!(
            ".build/bridgeforge/bin/project_demo{}",
            std::env::consts::EXE_SUFFIX
        ));
        write(&private_binary, b"project-owned executable");
        let legacy_binary = project.join(".codex/bin/project_demo.exe");
        let legacy_receipt = project.join(".codex/bin/build-receipt-project-demo.json");
        let legacy_ignore = project.join(".codex/bin/.gitignore");
        write(&legacy_binary, b"independent legacy executable");
        write(
            &legacy_receipt,
            b"opaque project receipt, not public evidence",
        );
        write(&legacy_ignore, b"*\r\n!.gitignore\r\n# project policy\r\n");
        write(
            &project.join(".codex/project-hooks.json"),
            b"invalid private registry",
        );
        write(
            &project.join(".codex/hooks/project_demo/Cargo.lock"),
            b"invalid private lock",
        );
        let mut update = build_plan(&project, factory, SyncMode::Update).unwrap();
        assert!(
            serde_json::to_value(&update)
                .unwrap()
                .get("hook_migration")
                .is_none()
        );
        assert!(
            update
                .safe
                .iter()
                .chain(&update.risk)
                .all(|a| !a.id.starts_with("project-hook:generated:"))
        );
        let fingerprint = update.aggregate_fingerprint.clone();
        attach_prepared_public(&mut update, &contract_bytes, &prepared_path).unwrap();
        apply_plan(update, &fingerprint, true).unwrap();
        for (path, expected) in [
            (&legacy_binary, b"independent legacy executable".as_slice()),
            (
                &legacy_receipt,
                b"opaque project receipt, not public evidence".as_slice(),
            ),
            (
                &legacy_ignore,
                b"*\r\n!.gitignore\r\n# project policy\r\n".as_slice(),
            ),
        ] {
            assert_eq!(fs::read(path).unwrap(), expected);
        }
        assert_eq!(
            fs::read(&private_binary).unwrap(),
            b"project-owned executable"
        );
        assert_eq!(
            fs::read(project.join(".codex/project-hooks.json")).unwrap(),
            b"invalid private registry"
        );
        assert_eq!(
            fs::read(project.join(".codex/hooks/project_demo/entrypoint.rs")).unwrap(),
            b"not Rust; private project data"
        );
        let after: serde_json::Value = serde_json::from_slice(&fs::read(&native).unwrap()).unwrap();
        assert_eq!(after, hooks);
        bridgeforge_skeleton::baseline::verify(&project, None, true).unwrap();
        git_at(&project, &["init"]);
        git_at(&project, &["add", "--force", "."]);
        bridgeforge_skeleton::baseline::verify_index(&project, &SystemProcessRunner).unwrap();
        // Public binary corruption must still fail verification.
        let public_binary = project.join(format!(
            ".deps/bridgeforge/bin/bridgeforge-hook{}",
            std::env::consts::EXE_SUFFIX
        ));
        let saved = fs::read(&public_binary).unwrap();
        write(&public_binary, b"corrupt public binary");
        assert!(bridgeforge_skeleton::baseline::verify(&project, None, true).is_err());
        write(&public_binary, &saved);
        let mut again = build_plan(&project, factory, SyncMode::Update).unwrap();
        assert!(!project.join(".codex/project-hook-policy.json").exists());
        assert!(!project.join(".codex/project-hook-migrations.json").exists());
        let fingerprint = again.aggregate_fingerprint.clone();
        attach_prepared_public(&mut again, &contract_bytes, &prepared_path).unwrap();
        let receipt = apply_plan(again, &fingerprint, false).unwrap();
        assert!(receipt.applied.is_empty());
        assert!(!receipt.stamp_written_last);
        assert_eq!(receipt.build_metrics.cargo_build_invocations, 0);
        let mut cli = ProcessRequest::new(
            project
                .join(format!(
                    ".deps/bridgeforge/bin/bridgeforge{}",
                    std::env::consts::EXE_SUFFIX
                ))
                .into_os_string(),
            &project,
        );
        cli.args = vec![
            "check".into(),
            "baseline".into(),
            "--root".into(),
            project.clone().into_os_string(),
        ];
        cli.env.insert(
            "USERPROFILE".into(),
            temp.0.join("profile-without-product").into_os_string(),
        );
        cli.env.insert(
            "CODEX_HOME".into(),
            temp.0
                .join("profile-without-product/.codex")
                .into_os_string(),
        );
        let result = SystemProcessRunner.run(&cli).unwrap();
        assert_eq!(
            result.code,
            0,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let reused =
        public_artifacts::prepare(factory, &codex, None, None, None, &SystemProcessRunner).unwrap();
    assert_eq!(reused["build_metrics"]["cargo_build_invocations"], 0);
    assert_eq!(reused["build_metrics"]["cache_hits"], 2);
    assert_eq!(
        public_artifacts::read(&prepared_path, &contract)
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn project_sync_retired_assets_require_exact_preservation_decisions() {
    let temp = TestDirectory::new("project-sync-preservation");
    let factory = temp.0.join("factory");
    fs::create_dir_all(factory.join("templates")).unwrap();
    write(&factory.join("VERSION"), b"8.0.1\n");
    write(&bridgeforge_core::source_projection::template_path(&factory, "templates/example.txt"), b"managed\n");
    let current_contract = json!({
        "schema_version": 4,
        "release_version": "8.0.1",
        "host": "codex",
        "stamp": ".codex/.bridgeforge_codex_version",
        "contract_target": ".codex/managed-skeleton.json",
        "baseline_model": "current-only", "compatibility_baseline": "8.0.1",
        "assets": [{
            "id": "example",
            "source": "templates/example.txt",
            "target": ".codex/example.txt",
            "strategy": "whole",
            "current_sha256": format!("sha256:{:x}", Sha256::digest(b"managed\n"))
        }],
        "generated_assets": []
    });
    fixture_product::install(
        &factory.join("bridgeforge-manifest.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&current_contract).unwrap()
        )
        .as_bytes(),
    );
    let original = b"original\n";
    let original_sha = format!("sha256:{:x}", Sha256::digest(original));
    let old_contract = json!({
        "schema_version": 4,
        "release_version": "8.0.0",
        "host": "codex",
        "stamp": ".codex/.bridgeforge_codex_version",
        "contract_target": ".codex/managed-skeleton.json",
        "baseline_model": "current-only", "compatibility_baseline": "8.0.1",
        "assets": [{
            "id": "old-python-hook",
            "source": "templates/old.py",
            "target": ".codex/old.py",
            "strategy": "whole",
            "current_sha256": original_sha
        }],
        "generated_assets": []
    });
    let prepare = |project: &Path, content: &[u8]| {
        fs::create_dir_all(project).unwrap();
        write(&project.join(".codex/old.py"), content);
        write(
            &project.join(".codex/.bridgeforge_codex_version"),
            b"8.0.0\n",
        );
        write(
            &project.join(".codex/managed-skeleton.json"),
            format!("{}\n", serde_json::to_string_pretty(&old_contract).unwrap()).as_bytes(),
        );
    };

    let exact = temp.0.join("exact");
    prepare(&exact, original);
    let exact_plan = build_plan(&exact, &factory, SyncMode::Update).unwrap();
    assert!(!exact_plan.gaps.is_empty());
    assert!(
        !exact_plan
            .safe
            .iter()
            .any(|item| item.operation == "delete")
    );
    assert!(exact.join(".codex/old.py").exists());

    let preserve = temp.0.join("preserve");
    prepare(&preserve, b"project custom\n");
    let undecided = build_plan(&preserve, &factory, SyncMode::Update).unwrap();
    assert!(
        undecided
            .gaps
            .iter()
            .any(|gap| gap.contains("P:project-file:.codex/old.py"))
    );
    let preserve_decision = json!({"preserve": ["P:project-file:.codex/old.py"], "delete": []});
    let preserve_plan = build_plan_with_inputs(
        &preserve,
        &factory,
        SyncMode::Update,
        Some(&preserve_decision),
    )
    .unwrap();
    assert!(preserve_plan.gaps.is_empty());
    let fingerprint = preserve_plan.aggregate_fingerprint.clone();
    apply_plan(preserve_plan, &fingerprint, false).unwrap();
    assert_eq!(
        fs::read(preserve.join(".codex/old.py")).unwrap(),
        b"project custom\n"
    );

    let delete = temp.0.join("delete");
    prepare(&delete, b"project custom\n");
    let delete_decision = json!({"preserve": [], "delete": ["P:project-file:.codex/old.py"]});
    let delete_plan =
        build_plan_with_inputs(&delete, &factory, SyncMode::Update, Some(&delete_decision))
            .unwrap();
    assert!(delete_plan.confirmation_required);
    assert!(
        delete_plan
            .risk
            .iter()
            .any(|item| item.id == "rebuild.remove:P:project-file:.codex/old.py")
    );
    let fingerprint = delete_plan.aggregate_fingerprint.clone();
    apply_plan(delete_plan, &fingerprint, true).unwrap();
    assert!(!delete.join(".codex/old.py").exists());
}

#[test]
fn destructive_rebuild_requires_unknown_file_decisions_and_retires_old_project_maps() {
    let temp = TestDirectory::new("project-sync-destructive");
    let factory = temp.0.join("factory");
    let project = temp.0.join("project");
    fs::create_dir_all(factory.join("templates")).unwrap();
    fs::create_dir_all(&project).unwrap();
    write(&factory.join("VERSION"), b"8.0.1\n");
    write(&bridgeforge_core::source_projection::template_path(&factory, "templates/example.txt"), b"managed\n");
    let contract = json!({
        "schema_version": 4,
        "release_version": "8.0.1",
        "host": "codex",
        "stamp": ".codex/.bridgeforge_codex_version",
        "contract_target": ".codex/managed-skeleton.json",
        "baseline_model": "current-only", "compatibility_baseline": "8.0.1",
        "assets": [{
            "id": "example",
            "source": "templates/example.txt",
            "target": ".codex/example.txt",
            "strategy": "whole",
            "current_sha256": format!("sha256:{:x}", Sha256::digest(b"managed\n"))
        }],
        "generated_assets": []
    });
    fixture_product::install(
        &factory.join("bridgeforge-manifest.json"),
        format!("{}\n", serde_json::to_string_pretty(&contract).unwrap()).as_bytes(),
    );
    write(&project.join(".codex/find-doc.map.md"), b"project map\n");
    write(&project.join(".codex/sync-docs.map.md"), b"project map\n");
    write(
        &project.join(".codex/skills/custom/SKILL.md"),
        b"---\nname: custom\ndescription: project skill\n---\n",
    );
    write(
        &project.join(".codex/rules/custom.rules"),
        b"prefix_rule(pattern=[\"custom\"], decision=\"allow\")\n",
    );
    write(&project.join(".codex/unknown.bin"), b"unknown\n");

    let blocked = build_plan(&project, &factory, SyncMode::Adopt).unwrap();
    assert!(blocked.gaps.iter().any(|item| item.contains("unknown.bin")));
    for target in [".codex/find-doc.map.md", ".codex/sync-docs.map.md"] {
        assert!(blocked.safe.iter().any(|action| {
            action.target == target
                && action.operation == "delete"
                && action.id.starts_with("retired:project-map:")
        }));
    }
    assert!(
        blocked.preservation_manifest["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == "R:skills" && item["disposition"] == "required-preserve")
    );
    fs::remove_file(project.join(".codex/unknown.bin")).unwrap();

    let undecided = build_plan(&project, &factory, SyncMode::Adopt).unwrap();
    assert!(undecided.blockers.is_empty(), "{:?}", undecided.blockers);
    assert!(
        undecided
            .gaps
            .iter()
            .any(|item| item.contains("P:rule:.codex/rules/custom.rules"))
    );
    let decisions = json!({
        "preserve": ["P:rule:.codex/rules/custom.rules"],
        "delete": []
    });
    let plan =
        build_plan_with_inputs(&project, &factory, SyncMode::Adopt, Some(&decisions)).unwrap();
    assert!(plan.gaps.is_empty(), "{:?}", plan.gaps);
    let combined =
        bridgeforge_workflow::project_sync::outcome_plan_with_format(Ok(plan.clone()), "combined");
    assert!(combined.receipt.as_ref().unwrap()["machine"].is_object());
    assert_eq!(
        combined.receipt.as_ref().unwrap()["human"]["conclusion"],
        "可直接执行"
    );
    let fingerprint = plan.aggregate_fingerprint.clone();
    apply_plan(plan, &fingerprint, false).unwrap();
    assert!(!project.join(".codex/find-doc.map.md").exists());
    assert!(!project.join(".codex/sync-docs.map.md").exists());
    assert!(project.join(".codex/skills/custom/SKILL.md").is_file());
    assert!(project.join(".codex/rules/custom.rules").is_file());
}

#[test]
fn project_sync_combined_upgrade_preserves_project_assets_and_is_idempotent() {
    let temp = TestDirectory::new("project-sync-real-contract");
    let project = temp.0.join("ExampleProject");
    fs::create_dir_all(project.join("doc")).unwrap();
    let factory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();

    let canonical_agents = fs::read_to_string(bridgeforge_core::source_projection::template_path(&factory, "templates/AGENTS.md")).unwrap();
    let project_begin = "<!-- BRIDGEFORGE:PROJECT:BEGIN -->";
    let project_end = "<!-- BRIDGEFORGE:PROJECT:END -->";
    let start = canonical_agents.find(project_begin).unwrap();
    let stop = canonical_agents.find(project_end).unwrap() + project_end.len();
    let custom_project_zone = format!(
        "{project_begin}\n## 项目级专区\n\n### 项目架构红线\n\n- CUSTOM-PROJECT-RULE\n\n### 项目目录地图\n\n- custom/**\n\n### 项目快速命令\n\n```text\ncustom-check\n```\n{project_end}"
    );
    let agents = format!(
        "{}{}{}",
        &canonical_agents[..start],
        custom_project_zone,
        &canonical_agents[stop..]
    );
    // Synthetic old state, not a complete historical installation or a model run.
    let public_begin = "<!-- BRIDGEFORGE:PUBLIC:BEGIN -->";
    let public_end = "<!-- BRIDGEFORGE:PUBLIC:END -->";
    let public = |text: &str| {
        text.split_once(public_begin)
            .unwrap()
            .1
            .split_once(public_end)
            .unwrap()
            .0
            .to_string()
    };
    let agents = agents.replacen(&public(&agents), "\nPrevious public rules\n", 1);
    write(&project.join("AGENTS.md"), agents.as_bytes());
    let retired = ".codex/agents/mechanical-sync-worker.toml";
    write(
        &project.join(retired),
        include_bytes!("../fixtures/retired-mechanical-sync-worker.txt"),
    );
    let roles = [
        "implementation-worker",
        "light-explorer",
        "review-auditor",
        "xhigh-auditor",
    ];
    for role in roles {
        write(
            &project.join(format!(".codex/agents/{role}.toml")),
            b"previous role\n",
        );
    }
    let local_skill = ".codex/skills/local/SKILL.md";
    let local_content =
        b"---\nname: local\ndescription: Project-owned notes\n---\nKeep local workflow.\n";
    write(&project.join(local_skill), local_content);

    let canonical_doc = fs::read_to_string(bridgeforge_core::source_projection::template_path(&factory, "templates/doc/README.md"))
        .unwrap()
        .replace("{{PROJECT_NAME}}", "ExampleProject");
    let doc = canonical_doc.replace(
        "## 2_bugs/",
        "| project-only/ | 用户项目内容 |\n\n## 2_bugs/",
    );
    write(&project.join("doc/README.md"), doc.as_bytes());

    let mut hooks: serde_json::Value =
        serde_json::from_slice(&fs::read(bridgeforge_core::source_projection::template_path(&factory, "templates/hooks.json")).unwrap()).unwrap();
    hooks["hooks"]["SessionStart"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"matcher": "", "hooks": [{"type": "command", "command": "custom-tool --check"}]}),
            json!({"matcher": "", "hooks": [{"type": "command", "command": ".venv/Scripts/python.exe .codex/hooks/hook_dispatcher.py session-start"}]}),
        ]);
    write(
        &project.join(".codex/hooks.json"),
        format!("{}\n", serde_json::to_string_pretty(&hooks).unwrap()).as_bytes(),
    );
    fs::create_dir_all(project.join(".deps/bridgeforge/bin")).unwrap();
    for name in [
        "bridgeforge.exe",
        "bridgeforge-hook.exe",
        "build-receipt-hook.json",
        "build-receipt-cli.json",
    ] {
        fs::copy(
            factory.join(".build/bridgeforge/bin").join(name),
            project.join(".deps/bridgeforge/bin").join(name),
        )
        .unwrap();
    }

    write(
        &project.join(".codex/.bridgeforge_codex_version"),
        b"8.0.0\n",
    );
    let protected = [
        "AGENTS.md",
        "doc/README.md",
        ".codex/hooks.json",
        local_skill,
        retired,
        ".codex/.bridgeforge_codex_version",
    ];
    let before = protected.map(|path| fs::read(project.join(path)).unwrap());
    let plan = build_plan(&project, &factory, SyncMode::Update).unwrap();
    assert!(plan.gaps.is_empty(), "{:?}", plan.gaps);
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    assert_eq!(
        protected.map(|path| fs::read(project.join(path)).unwrap()),
        before
    );
    for id in ["root.agents", "retired:codex.agent.mechanical-sync-worker"] {
        assert!(
            plan.safe
                .iter()
                .chain(&plan.risk)
                .any(|action| action.id == id)
        );
    }
    let fingerprint = plan.aggregate_fingerprint.clone();
    let receipt = apply_plan(plan, &fingerprint, true).unwrap();
    assert_eq!(receipt.project_readiness, "ready");
    assert!(receipt.stamp_written_last);

    let agents = fs::read_to_string(project.join("AGENTS.md")).unwrap();
    assert_eq!(public(&agents), public(&canonical_agents));
    let start = agents.find(project_begin).unwrap();
    let stop = agents.find(project_end).unwrap() + project_end.len();
    assert_eq!(&agents[start..stop], custom_project_zone);
    assert!(!agents.contains("{{PROJECT_NAME}}"));
    assert!(!project.join(retired).exists());
    assert_eq!(fs::read(project.join(local_skill)).unwrap(), local_content);
    for role in roles {
        assert_eq!(
            fs::read(project.join(format!(".codex/agents/{role}.toml"))).unwrap(),
            fs::read(bridgeforge_core::source_projection::template_path(&factory, &format!("templates/agents/{role}.toml"))).unwrap()
        );
    }
    let doc = fs::read_to_string(project.join("doc/README.md")).unwrap();
    assert!(doc.contains("| project-only/ | 用户项目内容 |"));
    let hooks = fs::read_to_string(project.join(".codex/hooks.json")).unwrap();
    assert!(hooks.contains("custom-tool --check"));
    assert!(!hooks.contains("hook_dispatcher.py"));
    assert!(hooks.contains("bridgeforge-codex.project-hook.v1:session-start"));
    bridgeforge_skeleton::baseline::verify(&project, None, true).unwrap();
    assert_eq!(
        fs::read(project.join(".codex/.bridgeforge_codex_version")).unwrap(),
        fs::read(factory.join("VERSION")).unwrap()
    );
    let again = build_plan(&project, &factory, SyncMode::Update).unwrap();
    assert!(again.public_installation_required);
    assert_eq!(again.status, "planned");
    assert!(again.safe.is_empty() && again.risk.is_empty() && again.gaps.is_empty());
    assert_eq!(fs::read(project.join(local_skill)).unwrap(), local_content);
}

#[test]
fn memory_push_rejects_rewritten_or_multiple_unapproved_destinations() {
    for destination in [
        "https://gitlab.com/owner/bridgeforge-codex-memories",
        "https://github.com/other/bridgeforge-codex-memories",
        "https://github.com/owner/bridgeforge-codex-memories\nhttps://github.com/other/bridgeforge-codex-memories",
    ] {
        let temp = TestDirectory::new("memory-push-destination");
        let remote = temp.0.join("memory.git");
        git_at(&temp.0, &["init", "--bare", remote.to_str().unwrap()]);
        let memories = temp.0.join("memories");
        let state = temp.0.join("state");
        write(&memories.join("note.md"), b"must not be published");
        let runner = LocalMemoryRunner(remote.clone(), Some(destination));
        assert!(reconcile(&memories, &state, MEMORY_REMOTE, &runner).is_err());
        let request = ProcessRequest {
            args: vec!["for-each-ref".into()],
            ..ProcessRequest::new("git", &remote)
        };
        let refs = SystemProcessRunner.run(&request).unwrap();
        assert_eq!(refs.code, 0);
        assert!(
            refs.stdout.is_empty(),
            "unapproved destination must not receive a commit"
        );
        assert_eq!(
            fs::read(memories.join("note.md")).unwrap(),
            b"must not be published"
        );
    }
}

#[test]
fn memory_remote_push_then_restore_uses_real_local_git_remote() {
    let temp = TestDirectory::new("memory-remote");
    let remote = temp.0.join("memory.git");
    let runner = LocalMemoryRunner(remote.clone(), None);
    let result = Command::new("git")
        .args(["init", "--bare", remote.to_string_lossy().as_ref()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let first_memories = temp.0.join("first/memories");
    let first_state = temp.0.join("first/state");
    write(&first_memories.join("note.md"), b"native memory\n");
    mark_pending(&first_state, "test-push").unwrap();
    let action = reconcile(&first_memories, &first_state, MEMORY_REMOTE, &runner).unwrap();
    assert_eq!(action, "push");
    assert!(!first_state.join("pending.json").exists());

    let second_memories = temp.0.join("second/memories");
    let second_state = temp.0.join("second/state");
    mark_pending(&second_state, "test-restore").unwrap();
    let action = reconcile(&second_memories, &second_state, MEMORY_REMOTE, &runner).unwrap();
    assert_eq!(action, "restore");
    assert_eq!(
        fs::read(second_memories.join("note.md")).unwrap(),
        b"native memory\n"
    );
    assert!(!second_state.join("pending.json").exists());
}

#[test]
fn memory_same_path_conflict_requires_explicit_resolution() {
    let temp = TestDirectory::new("memory-conflict");
    let remote = temp.0.join("memory.git");
    let runner = LocalMemoryRunner(remote.clone(), None);
    assert!(
        Command::new("git")
            .args(["init", "--bare", remote.to_string_lossy().as_ref()])
            .status()
            .unwrap()
            .success()
    );
    let first_memories = temp.0.join("first/memories");
    let first_state = temp.0.join("first/state");
    let second_memories = temp.0.join("second/memories");
    let second_state = temp.0.join("second/state");
    write(&first_memories.join("note.md"), b"base\n");
    mark_pending(&first_state, "seed").unwrap();
    reconcile(&first_memories, &first_state, MEMORY_REMOTE, &runner).unwrap();
    mark_pending(&second_state, "restore").unwrap();
    reconcile(&second_memories, &second_state, MEMORY_REMOTE, &runner).unwrap();

    write(&first_memories.join("note.md"), b"first\n");
    mark_pending(&first_state, "first-edit").unwrap();
    reconcile(&first_memories, &first_state, MEMORY_REMOTE, &runner).unwrap();
    write(&second_memories.join("note.md"), b"second\n");
    mark_pending(&second_state, "second-edit").unwrap();
    assert_eq!(
        reconcile(&second_memories, &second_state, MEMORY_REMOTE, &runner,).unwrap(),
        "conflicted"
    );
    let active: serde_json::Value =
        serde_json::from_slice(&fs::read(second_state.join("active-conflict.json")).unwrap())
            .unwrap();
    let conflict_id = active["conflictId"].as_str().unwrap();
    // The remote advances after conflict capture, but converges to the exact
    // bytes that were captured as this device's local side. Resolution must
    // safely replay against the new parent instead of forcing a stale commit.
    write(&first_memories.join("note.md"), b"second\n");
    mark_pending(&first_state, "converge-remote").unwrap();
    assert_eq!(
        reconcile(&first_memories, &first_state, MEMORY_REMOTE, &runner,).unwrap(),
        "push"
    );
    assert_eq!(
        resolve_conflict_with_choices(
            &second_memories,
            &second_state,
            MEMORY_REMOTE,
            conflict_id,
            &[("note.md".into(), "local".into())],
            &runner,
        )
        .unwrap(),
        "resolved"
    );
    assert_eq!(
        fs::read(second_memories.join("note.md")).unwrap(),
        b"second\n"
    );
    assert!(!second_state.join("active-conflict.json").exists());
    assert!(
        apply_conflict_choices(
            &second_state,
            conflict_id,
            &[("note.md".into(), "remote".into())],
        )
        .is_err()
    );
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
