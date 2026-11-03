#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

use bridgeforge_lifecycle::release::{
    FileReleasePlan, schema4, verify_release_inputs, verify_release_inventory,
    verify_release_status,
};
#[path = "../support/release_plan.rs"]
mod release_plan;
use bridgeforge_core::{ProcessOutput, ProcessRequest, ProcessRunner, SystemProcessRunner};
use release_plan::apply_file_release_plan;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct ReadOnlyProcesses;
impl ProcessRunner for ReadOnlyProcesses {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        if request.program == "git" {
            for forbidden in [
                "fetch", "push", "pull", "add", "commit", "reset", "checkout",
            ] {
                assert!(!request.args.iter().any(|a| a == forbidden));
            }
        } else {
            assert!(request.program == "cargo" || request.program == "rustc");
            assert!(request.args.iter().all(|a| a == "-Vv" || a == "-vV"));
        }
        SystemProcessRunner.run(request)
    }
}
fn factory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}
fn copy_native_manifests(source: &Path, target: &Path, directory: &Path) {
    for entry in fs::read_dir(source.join(directory)).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if matches!(name.to_str(), Some("target" | ".git" | ".runtime")) {
            continue;
        }
        let relative = directory.join(&name);
        if entry.file_type().unwrap().is_dir() {
            copy_native_manifests(source, target, &relative);
        } else if matches!(name.to_str(), Some("Cargo.toml" | "Cargo.lock")) {
            fs::create_dir_all(target.join(directory)).unwrap();
            fs::copy(entry.path(), target.join(relative)).unwrap();
        }
    }
}

#[test]
fn legacy_factory_release_updates_all_three_cargo_manifests_and_locks() {
    let source = factory();
    // Frozen pre-refactor schema-1 mapping remains a supported migration input.
    let native = source.join("scripts/tests/fixtures/legacy-product-workspace");
    let paths = vec![
        Value::from("templates/hooks/Cargo.toml"),
        Value::from(".codex/hooks/Cargo.toml"),
        Value::from("scripts/tests/Cargo.toml"),
    ];
    let root =
        test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "factory-version-config").join(format!(
            "bf-factory-version-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    let temp = Temp(root);
    fs::create_dir_all(temp.0.join(".codex")).unwrap();
    fs::create_dir_all(temp.0.join("templates")).unwrap();
    let legacy = json!({"schema_version":1,"release_policy":"explicit_release","manifests":paths});
    fs::write(
        temp.0.join(".codex/bridgeforge-version.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();
    fs::write(temp.0.join(".gitignore"), b".runtime/\n").unwrap();
    fixture_product::write(temp.0.join("source/forge/bridgeforge-manifest.json"), b"{}").unwrap();
    for name in ["VERSION", "CHANGELOG.md"] {
        fs::copy(source.join(name), temp.0.join(name)).unwrap();
    }
    for path in &paths {
        let relative = Path::new(path.as_str().unwrap());
        fs::create_dir_all(temp.0.join(relative).parent().unwrap()).unwrap();
        for name in ["Cargo.toml", "Cargo.lock"] {
            fs::copy(
                native.join(if relative.starts_with("scripts") {
                    format!("consumer-{name}")
                } else {
                    name.to_string()
                }),
                temp.0.join(relative).with_file_name(name),
            )
            .unwrap();
        }
    }
    for directory in ["templates/hooks", ".codex/hooks"] {
        copy_native_manifests(&native, &temp.0.join(directory), Path::new("crates"));
    }
    for args in [
        vec!["init"],
        vec!["config", "user.name", "Factory version fixture"],
        vec!["config", "user.email", "factory@example.invalid"],
        vec!["add", "."],
        vec!["commit", "-m", "chore: native baseline"],
    ] {
        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(&temp.0)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let migration = schema4::migration_plan(&temp.0, &ReadOnlyProcesses, None);
    assert_eq!(migration.code, 0, "{}", migration.stderr);
    let fp = migration.receipt.unwrap()["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    let migrated = schema4::migration_apply(&temp.0, &ReadOnlyProcesses, None, &fp, true);
    assert_eq!(migrated.code, 0, "{}", migrated.stderr);
    fs::write(temp.0.join("README.md"), b"actual product change\n").unwrap();
    // Product followers are validated through the native graph, including
    // inherited members and locks in the consumer workspace.
    let lock_path = temp.0.join("scripts/tests/Cargo.lock");
    let lock_bytes = fs::read(&lock_path).unwrap();
    let current = fs::read_to_string(temp.0.join("VERSION")).unwrap();
    let stale = String::from_utf8(lock_bytes.clone()).unwrap().replace(
        &format!("version = \"{}\"", current.trim()),
        "version = \"0.0.0\"",
    );
    fs::write(&lock_path, stale).unwrap();
    let blocked = schema4::preview(
        &temp.0,
        "fix: stale native lock",
        &schema4::Request::default(),
        &ReadOnlyProcesses,
    );
    assert_ne!(blocked.code, 0);
    assert!(blocked.stderr.contains("stale"), "{}", blocked.stderr);
    fs::write(&lock_path, &lock_bytes).unwrap();
    let missing_edge = String::from_utf8(lock_bytes.clone())
        .unwrap()
        .lines()
        .filter(|line| line.trim() != "\"bridgeforge-core\",")
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&lock_path, missing_edge).unwrap();
    let blocked = schema4::preview(
        &temp.0,
        "fix: missing local lock edge",
        &schema4::Request::default(),
        &ReadOnlyProcesses,
    );
    assert_ne!(blocked.code, 0);
    assert!(
        blocked.stderr.contains("missing local dependency edge"),
        "{}",
        blocked.stderr
    );
    fs::write(&lock_path, &lock_bytes).unwrap();
    let member = temp
        .0
        .join("templates/hooks/crates/bridgeforge-core/Cargo.toml");
    let member_bytes = fs::read(&member).unwrap();
    fs::write(
        &member,
        String::from_utf8(member_bytes.clone())
            .unwrap()
            .replace("version.workspace = true", "version = \"0.0.0\""),
    )
    .unwrap();
    let blocked = schema4::preview(
        &temp.0,
        "fix: manual follower version",
        &schema4::Request::default(),
        &ReadOnlyProcesses,
    );
    assert_ne!(blocked.code, 0);
    assert!(blocked.stderr.contains("version"), "{}", blocked.stderr);
    fs::write(&member, &member_bytes).unwrap();
    let before = schema4::preview(
        &temp.0,
        "fix: factory mapping",
        &schema4::Request::default(),
        &ReadOnlyProcesses,
    );
    assert_eq!(before.code, 0, "{}", before.stderr);
    let input = before.receipt.unwrap();
    fs::write(temp.0.join(".runtime/analysis.json"),serde_json::to_vec(&json!({"schema":1,"fingerprint":input["fingerprint"],"product":{"level":"patch","reason":"fixture program repair","compatibility":"compatible"},"packages":{}})).unwrap()).unwrap();
    let old_version = fs::read(temp.0.join("VERSION")).unwrap();
    let index = fs::read(temp.0.join(".git/index")).unwrap();
    let plan = schema4::build(
        &temp.0,
        "fix: factory mapping",
        &schema4::Request {
            analysis_file: Some(PathBuf::from(".runtime/analysis.json")),
        },
        &ReadOnlyProcesses,
    )
    .unwrap()
    .unwrap();
    assert_eq!(fs::read(temp.0.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(temp.0.join("VERSION")).unwrap(),
        old_version,
        "planning must not write"
    );
    let next = plan.new_version.to_string();
    for path in paths {
        let target = temp.0.join(path.as_str().unwrap());
        let payload = String::from_utf8(plan.writes[&target].clone()).unwrap();
        assert!(
            payload.contains(&format!("version = \"{next}\"")),
            "{}",
            target.display()
        );
        let lock = target.with_file_name("Cargo.lock");
        assert!(plan.writes.contains_key(&lock), "{}", lock.display());
        if path == "scripts/tests/Cargo.toml" {
            let payload = String::from_utf8(plan.writes[&lock].clone()).unwrap();
            let package = payload
                .split("[[package]]")
                .find(|part| part.contains("name = \"bridgeforge-factory-tests\""))
                .unwrap();
            assert!(package.contains(&format!("version = \"{next}\"")));
        }
    }
    apply_file_release_plan(&plan).unwrap();
    for (path, payload) in &plan.writes {
        assert_eq!(&fs::read(path).unwrap(), payload);
    }
}

#[test]
fn obsolete_venv_contract_is_not_an_active_runtime_instruction() {
    let source = factory();
    let old = fs::read_to_string(source.join("doc/1_delivery/project-venv-hook-runtime-single-rule/requirements_2026-08-19_project-venv-hook-runtime-single-rule.md")).unwrap();
    assert!(
        old.starts_with("---\nlifecycle: superseded\n")
            || old.starts_with("---\r\nlifecycle: superseded\r\n")
    );
    assert!(old.contains("superseded_by: ../rust-only-bridgeforge/"));
    assert!(!old.contains("其他 Python 流程仍继续遵守本卡"));
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
