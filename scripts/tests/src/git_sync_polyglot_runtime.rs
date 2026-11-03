//! The real managed CLI, with only a temporary local Git remote.
#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

use bridgeforge_core::{ProcessRequest, ProcessRunner, SystemProcessRunner};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        if let (Ok(path), Ok(temp)) = (
            self.0.canonicalize(),
            test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "git-sync-polyglot-runtime")
                .canonicalize(),
        ) {
            if path.parent() == Some(temp.as_path())
                && path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("bf-polyglot-cli-")
            {
                let _ = fs::remove_dir_all(path);
            }
        }
    }
}
fn git(root: &Path, args: &[&str]) -> String {
    let mut r = ProcessRequest::new("git", root);
    r.args = args.iter().map(Into::into).collect();
    let o = SystemProcessRunner.run(&r).unwrap();
    assert_eq!(o.code, 0, "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().into()
}
fn write(root: &Path, path: &str, bytes: &[u8]) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(root.join(path), bytes).unwrap();
}
fn call(cli: &Path, root: &Path, args: &[&str], success: bool) -> Value {
    let mut r = ProcessRequest::new(cli.as_os_str(), root);
    r.args = args.iter().map(Into::into).collect();
    r.timeout = Duration::from_secs(120);
    let o = SystemProcessRunner.run(&r).unwrap();
    assert!(!o.timed_out);
    if success {
        assert_eq!(o.code, 0, "{}", String::from_utf8_lossy(&o.stderr));
    } else {
        assert_ne!(o.code, 0);
    }
    serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|_| json!({"stderr":String::from_utf8_lossy(&o.stderr)}))
}
#[test]
fn real_cli_python_lock_preview_release_rollback_and_repeat() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let cli = source.join(if cfg!(windows) {
        ".build/bridgeforge/bin/bridgeforge.exe"
    } else {
        ".build/bridgeforge/bin/bridgeforge"
    });
    let fixture = Fixture(
        test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "git-sync-polyglot-runtime").join(
            format!(
                "bf-polyglot-cli-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
        ),
    );
    let root = fixture.0.join("project");
    fs::create_dir_all(&root).unwrap();
    write(&root, "VERSION", b"1.0.0\n");
    write(&root, "CHANGELOG.md", b"# Changelog\n");
    write(&root, "app.py", b"VALUE=1\n");
    write(&root, ".gitignore", b".runtime/\n");
    write(&root, "managed.txt", b"managed\n");
    write(&root, ".codex/.bridgeforge_codex_version", b"8.0.0\n");
    write(
        &root,
        "pyproject.toml",
        b"[project]\nname='app'\nversion='1.2.3'\nrequires-python='>=3.10'\n",
    );
    write(&root,"uv.lock",b"version = 1\nrevision = 3\nrequires-python = '>=3.10'\n[[package]]\nname = 'app'\nversion = '1.2.3'\nsource = { editable = '.' }\n");
    let cfg = json!({"schema_version":3,"release_policy":"explicit_release","strategy":"independent","git_sync_setup":{"schema":1,"confirmed":true,"selection":"independent"},"adapters":{"python":{"projects":["pyproject.toml"],"locks":[{"path":"uv.lock","format":"uv","members":["pyproject.toml"]}]}},"packages":[{"id":"app","language":"python","manifest":"pyproject.toml","name":"app"}],"product_inputs":["uv.lock",".gitignore"]});
    write(
        &root,
        ".codex/bridgeforge-version.json",
        &serde_json::to_vec(&cfg).unwrap(),
    );
    use sha2::{Digest, Sha256};
    let contract = json!({"schema_version":4,"release_version":"8.0.0","host":"codex","stamp":".codex/.bridgeforge_codex_version","contract_target":".codex/managed-skeleton.json","assets":[{"id":"managed.asset","source":"templates/managed.txt","target":"managed.txt","strategy":"whole","current_sha256":format!("sha256:{:x}",Sha256::digest(b"managed\n"))}],"generated_assets":[],"baseline_model":"current-only","compatibility_baseline":"8.0.0"});
    write(
        &root,
        ".codex/managed-skeleton.json",
        &serde_json::to_vec(&contract).unwrap(),
    );
    git(&root, &["init"]);
    git(&root, &["config", "user.name", "BridgeForge Test"]);
    git(&root, &["config", "user.email", "test@example.invalid"]);
    git(&root, &["config", "core.hooksPath", ".git/hooks"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "chore: baseline"]);
    let remote = fixture.0.join("origin.git");
    git(&root, &["init", "--bare", remote.to_str().unwrap()]);
    git(
        &root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&root, &["push", "-u", "origin", "HEAD"]);
    let legacy_config = fs::read(root.join(".codex/bridgeforge-version.json")).unwrap();
    let legacy_index = fs::read(root.join(".git/index")).unwrap();
    let denied = call(
        &cli,
        &root,
        &[
            "git-sync",
            "--release-preview",
            "--message",
            "fix: legacy release",
        ],
        false,
    );
    assert!(denied["stderr"].as_str().unwrap().contains("retired"));
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), legacy_index);
    assert_eq!(
        fs::read(root.join(".codex/bridgeforge-version.json")).unwrap(),
        legacy_config
    );
    super::schema4_fixture::migrate(&cli, &root);
    // A skeleton update can already be committed while still being part of
    // the cumulative diff since the last formal business release.
    let mut updated_contract = contract.clone();
    updated_contract["assets"][0]["current_sha256"] =
        json!(format!("sha256:{:x}", Sha256::digest(b"managed update\n")));
    write(&root, "managed.txt", b"managed update\n");
    write(
        &root,
        ".codex/managed-skeleton.json",
        &serde_json::to_vec(&updated_contract).unwrap(),
    );
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "chore: committed skeleton update"]);
    let skeleton_index = fs::read(root.join(".git/index")).unwrap();
    let skeleton_preview = call(
        &cli,
        &root,
        &[
            "git-sync",
            "--release-preview",
            "--message",
            "chore: skeleton only",
        ],
        true,
    );
    assert_eq!(skeleton_preview["status"], "nothing-to-release");
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), skeleton_index);
    assert_eq!(fs::read(root.join("VERSION")).unwrap(), b"1.0.0\n");
    write(&root, "app.py", b"VALUE=2\n");
    super::schema4_fixture::analyze(&cli, &root, "fix: adjust app", "patch", &[], &[]);
    let status = git(&root, &["status", "--porcelain=v1"]);
    let head = git(&root, &["rev-parse", "HEAD"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    let before = ["VERSION", "pyproject.toml", "uv.lock", "CHANGELOG.md"]
        .into_iter()
        .map(|p| (p, fs::read(root.join(p)).unwrap()))
        .collect::<Vec<_>>();
    let preview = call(
        &cli,
        &root,
        &[
            "git-sync",
            "--release-preview",
            "--release-analysis-file",
            ".runtime/schema4-analysis.json",
            "--message",
            "fix: adjust app",
        ],
        true,
    );
    assert_eq!(preview["package_changes"][0]["version_after"], "1.2.4");
    assert_eq!(preview["version_strategy"], "independent");
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    write(&root, ".git/hooks/pre-commit", b"#!/bin/sh\nexit 1\n");
    call(
        &cli,
        &root,
        &[
            "git-sync",
            "--release",
            "--message",
            "fix: adjust app",
            "--release-analysis-file",
            ".runtime/schema4-analysis.json",
        ],
        false,
    );
    for (p, b) in &before {
        assert_eq!(fs::read(root.join(p)).unwrap(), *b);
    }
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&root, &["status", "--porcelain=v1"]), status);
    fs::remove_file(root.join(".git/hooks/pre-commit")).unwrap();
    let result = call(
        &cli,
        &root,
        &[
            "git-sync",
            "--release",
            "--message",
            "fix: adjust app",
            "--release-analysis-file",
            ".runtime/schema4-analysis.json",
        ],
        true,
    );
    assert_eq!(result["package_changes"], preview["package_changes"]);
    assert_eq!(result["version_after"], "1.0.1");
    assert_eq!(result["working_tree"], "clean");
    assert_eq!(result["ahead"], 0);
    assert_eq!(result["behind"], 0);
    assert!(
        String::from_utf8(fs::read(root.join("uv.lock")).unwrap())
            .unwrap()
            .contains("1.2.4")
    );
    let repeated = call(
        &cli,
        &root,
        &["git-sync", "--release", "--message", "feat!: ignored"],
        true,
    );
    assert_eq!(repeated["version_bumped"], false);
    assert_eq!(repeated["commit"], result["commit"]);
}

#[test]
fn real_cli_release_preview_after_project_sync_source_relocation() {
    use bridgeforge_workflow::project_sync::{SyncMode, apply_plan, build_plan};
    use sha2::{Digest, Sha256};
    const OLD: &str = ".codex/hooks/crates/bridgeforge-core/src/memory/ownership.rs";
    const NEW: &str = ".codex/hooks/source/memory/src/ownership.rs";
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let cli = source.join(if cfg!(windows) {
        ".build/bridgeforge/bin/bridgeforge.exe"
    } else {
        ".build/bridgeforge/bin/bridgeforge"
    });
    let fixture = Fixture(
        test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "git-sync-polyglot-runtime").join(
            format!(
                "bf-polyglot-cli-relocation-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
        ),
    );
    let root = fixture.0.join("project");
    let factory = fixture.0.join("factory");
    let payload = b"pub fn managed() {}\n";
    let hash = format!("sha256:{:x}", Sha256::digest(payload));
    let contract = json!({"schema_version":4,"release_version":"8.0.0","host":"codex",
        "stamp":".codex/.bridgeforge_codex_version","contract_target":".codex/managed-skeleton.json",
        "baseline_model":"current-only","compatibility_baseline":"8.0.0","generated_assets":[],
        "assets":[{"id":"managed.memory","source":"source/memory/src/ownership.rs","target":OLD,
            "strategy":"whole","current_sha256":hash}]});
    write(&root, "VERSION", b"1.0.0\n");
    write(&root, "CHANGELOG.md", b"# Changelog\n");
    write(&root, ".gitignore", b".runtime/\n");
    write(
        &root,
        "pkg/Cargo.toml",
        b"[package]\nname='app'\nversion='1.0.0'\nedition='2021'\n[workspace]\n",
    );
    write(
        &root,
        "pkg/Cargo.lock",
        b"version=4\n[[package]]\nname='app'\nversion='1.0.0'\n",
    );
    write(
        &root,
        "pkg/src/lib.rs",
        b"pub fn value()->u8{1}\n#[test] fn value_contract(){assert_eq!(value(),1);}\n",
    );
    write(&root, OLD, payload);
    write(&root, ".codex/.bridgeforge_codex_version", b"8.0.0\n");
    write(
        &root,
        ".codex/managed-skeleton.json",
        &serde_json::to_vec(&contract).unwrap(),
    );
    let cfg = json!({"schema_version":3,"release_policy":"explicit_release","strategy":"independent",
        "adapters":{"rust":{"workspaces":["pkg/Cargo.toml"]}},
        "packages":[{"id":"app","language":"rust","manifest":"pkg/Cargo.toml","name":"app"}],
        "product_inputs":[".gitignore","pkg/Cargo.lock"]});
    write(
        &root,
        ".codex/bridgeforge-version.json",
        &serde_json::to_vec(&cfg).unwrap(),
    );
    git(&root, &["init"]);
    git(&root, &["config", "user.name", "BridgeForge Test"]);
    git(&root, &["config", "user.email", "test@example.invalid"]);
    git(&root, &["config", "core.hooksPath", ".git/hooks"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "chore: published baseline"]);
    super::schema4_fixture::migrate(&cli, &root);
    let mut updated = contract.clone();
    updated["release_version"] = json!("8.0.1");
    updated["assets"][0]["target"] = json!(NEW);
    updated["retired_assets"] = json!([{"id":"managed.memory","target":OLD,"known_hashes":[hash]}]);
    write(&factory, "VERSION", b"8.0.1\n");
    write(&factory, "source/memory/src/ownership.rs", payload);
    fixture_product::install(
        &factory.join("source/forge/bridgeforge-manifest.json"),
        &serde_json::to_vec(&updated).unwrap(),
    );
    let plan = build_plan(&root, &factory, SyncMode::Update).unwrap();
    assert!(plan.gaps.is_empty(), "{:?}", plan.gaps);
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    assert!(
        plan.safe
            .iter()
            .any(|a| a.target == OLD && a.operation == "delete")
    );
    let fingerprint = plan.aggregate_fingerprint.clone();
    apply_plan(plan, &fingerprint, true).unwrap();
    assert!(!root.join(OLD).exists());
    assert_eq!(fs::read(root.join(NEW)).unwrap(), payload);
    bridgeforge_skeleton::baseline::verify(&root, None, true).unwrap();
    for committed in [false, true] {
        if committed {
            git(&root, &["add", "."]);
            git(&root, &["commit", "-m", "chore: skeleton upgrade"]);
        }
        let status = git(&root, &["status", "--porcelain=v1"]);
        let index = fs::read(root.join(".git/index")).unwrap();
        let config = fs::read(root.join(".codex/bridgeforge-version.json")).unwrap();
        let preview = call(&cli, &root, &["git-sync", "--release-preview"], true);
        assert_eq!(preview["status"], "nothing-to-release");
        assert_eq!(git(&root, &["status", "--porcelain=v1"]), status);
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read(root.join(".codex/bridgeforge-version.json")).unwrap(),
            config
        );
        assert_eq!(fs::read(root.join("VERSION")).unwrap(), b"1.0.0\n");
    }
    write(
        &root,
        "pkg/src/lib.rs",
        b"pub fn value()->u8{2}\n#[test] fn value_contract(){assert_eq!(value(),2);}\n",
    );
    super::schema4_fixture::analyze(&cli, &root, "fix: app", "patch", &[], &[]);
    let preview = call(
        &cli,
        &root,
        &[
            "git-sync",
            "--release-preview",
            "--release-analysis-file",
            ".runtime/schema4-analysis.json",
        ],
        true,
    );
    assert_eq!(preview["status"], "release-planned");
    assert_eq!(preview["version_after"], "1.0.1");
    assert_eq!(preview["package_changes"].as_array().unwrap().len(), 1);
    assert_eq!(preview["package_changes"][0]["version_after"], "1.0.1");
    assert_eq!(fs::read(root.join("VERSION")).unwrap(), b"1.0.0\n");
    // No actual release, push, remote, or real downstream is needed here.
    assert_eq!(git(&root, &["remote"]), "");
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
