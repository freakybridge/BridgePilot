//! Current layout checks through the real CLI in isolated Git repositories.
use bridgeforge_core::{ProcessOutput, ProcessRequest, ProcessRunner, SystemProcessRunner};
use serde_json::Value;
#[path = "../support/current_template.rs"]
mod current_template;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn call(root: &Path, program: &Path, args: &[&str]) -> ProcessOutput {
    let mut request = ProcessRequest::new(program.as_os_str(), root);
    request.args = args.iter().map(Into::into).collect();
    request.timeout = Duration::from_secs(180);
    let output = SystemProcessRunner.run(&request).unwrap();
    assert_eq!(
        output.code,
        0,
        "{program:?} {args:?}: {} {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!output.timed_out);
    output
}
fn git(root: &Path, args: &[&str]) -> String {
    String::from_utf8(call(root, Path::new("git"), args).stdout)
        .unwrap()
        .trim()
        .into()
}
fn write(root: &Path, path: &str, bytes: &[u8]) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn invoke(cli: &Path, root: &Path, args: &[&str]) -> Value {
    let output = call(root, cli, args);
    serde_json::from_slice(&output.stdout).unwrap()
}

fn setup(cli: &Path, label: &str) -> Fixture {
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture = Fixture(
        test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "artifact-layout-runtime")
            .join(format!("bf-layout-{label}-{token}")),
    );
    let root = fixture.0.join("project");
    fs::create_dir_all(&root).unwrap();
    current_template::write(&root);
    write(&root, "VERSION", b"1.0.0\n");
    write(&root, "CHANGELOG.md", b"# Changelog\n");
    write(
        &root,
        "Cargo.toml",
        b"[package]\nname='layout-fixture'\nversion='1.0.0'\nedition='2024'\n",
    );
    write(
        &root,
        "Cargo.lock",
        b"version = 4\n[[package]]\nname = 'layout-fixture'\nversion = '1.0.0'\n",
    );
    write(&root, "src/lib.rs", b"pub fn sample() {}\n");
    write(&root, ".gitignore", b"/.runtime/\n/.deps/\n/.build/\n");
    for layer in [
        "0_architecture",
        "1_delivery",
        "2_bugs",
        "3_reference",
        "4_archive",
        "5_project_knowledgebase",
    ] {
        fs::create_dir_all(root.join("doc").join(layer)).unwrap();
    }
    let index = fs::read_to_string(root.join("doc/README.md")).unwrap();
    let index = index
        .lines()
        .map(|line| {
            if line.starts_with("delivery_layout:") {
                "delivery_layout: flat"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    write(&root, "doc/README.md", index.as_bytes());
    git(&root, &["init"]);
    git(&root, &["config", "user.name", "Layout Fixture"]);
    git(&root, &["config", "user.email", "layout@example.invalid"]);
    git(&root, &["config", "core.hooksPath", ".git/hooks"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "chore: fixture seed"]);
    let cli_path = cli
        .to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "'\\''");
    write(
        &root,
        ".git/hooks/pre-commit",
        format!("#!/bin/sh\nexec '{cli_path}' check project-structure --root . --pre-commit\n")
            .as_bytes(),
    );
    let remote = fixture.0.join("remote.git");
    git(&root, &["init", "--bare", remote.to_str().unwrap()]);
    git(
        &root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&root, &["push", "-u", "origin", "HEAD"]);
    call(&root, cli, &["check", "baseline", "--root", "."]);
    call(&root, cli, &["check", "project-structure", "--root", "."]);
    fixture
}

#[test]
fn artifact_layout_real_git_sync_blocks_only_new_violations_and_keeps_remote() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let cli = source.join(if cfg!(windows) {
        ".build/bridgeforge/bin/bridgeforge.exe"
    } else {
        ".build/bridgeforge/bin/bridgeforge"
    });
    let fixture = setup(&cli, "workflow");
    let root = fixture.0.join("project");
    let original = git(&root, &["rev-parse", "HEAD"]);
    write(&root, "tests/new.rs", b"fn probe() {}\n");
    let mut request = ProcessRequest::new(cli.as_os_str(), &root);
    request.args = ["git-sync", "--message", "fix: rejected layout"]
        .iter()
        .map(Into::into)
        .collect();
    request.timeout = Duration::from_secs(180);
    let failure = SystemProcessRunner.run(&request).unwrap();
    assert_ne!(failure.code, 0);
    assert!(
        String::from_utf8_lossy(&failure.stderr).contains("legacy-test-root"),
        "{}",
        String::from_utf8_lossy(&failure.stderr)
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), original);
    assert_eq!(git(&root, &["rev-parse", "@{push}"]), original);
    fs::remove_file(root.join("tests/new.rs")).unwrap();
    write(&root, "src/lib.rs", b"pub fn changed() {}\n");
    let output = invoke(&cli, &root, &["git-sync", "--message", "fix: valid layout"]);
    assert_eq!(output["status"], "synced");
    assert_eq!(output["version_bumped"], false);
    assert_eq!(
        fs::read_to_string(root.join("VERSION")).unwrap().trim(),
        "1.0.0"
    );
    assert_eq!(
        git(
            &root,
            &["rev-list", "--left-right", "--count", "HEAD...@{push}"]
        ),
        "0\t0"
    );
    // An ignored historical cache is visible to audit, but cannot block A mode.
    write(
        &root,
        ".gitignore",
        b"/.runtime/\n/.deps/\n/.build/\nlegacy-cache/\n",
    );
    git(&root, &["add", ".gitignore"]);
    git(&root, &["commit", "-m", "chore: ignore legacy cache"]);
    write(&root, "legacy-cache/__pycache__/old.pyc", b"old\n");
    write(&root, "src/lib.rs", b"pub fn another() {}\n");
    let output = invoke(
        &cli,
        &root,
        &["git-sync", "--message", "fix: unrelated cache"],
    );
    assert_eq!(output["status"], "synced");
    let mut request = ProcessRequest::new(cli.as_os_str(), &root);
    request.args = ["check", "project-structure", "--root", "."]
        .iter()
        .map(Into::into)
        .collect();
    let audit = SystemProcessRunner.run(&request).unwrap();
    assert_ne!(audit.code, 0);
    assert!(String::from_utf8_lossy(&audit.stdout).contains("misplaced-python-artifact"));
}

#[test]
fn artifact_layout_defaults_block_nested_tests_without_project_configuration() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let cli = source.join(if cfg!(windows) {
        ".build/bridgeforge/bin/bridgeforge.exe"
    } else {
        ".build/bridgeforge/bin/bridgeforge"
    });
    let fixture = setup(&cli, "defaults");
    let root = fixture.0.join("project");
    let original = git(&root, &["rev-parse", "HEAD"]);
    write(&root, "src/tests/new.rs", b"fn probe() {}\n");
    let mut request = ProcessRequest::new(cli.as_os_str(), &root);
    request.args = ["git-sync", "--message", "fix: rejected nested test"]
        .iter()
        .map(Into::into)
        .collect();
    request.timeout = Duration::from_secs(180);
    let output = SystemProcessRunner.run(&request).unwrap();
    assert_ne!(output.code, 0);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("misplaced-test-directory"),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), original);
    assert_eq!(git(&root, &["rev-parse", "@{push}"]), original);
    fs::remove_file(root.join("src/tests/new.rs")).unwrap();
    write(&root, "scripts/tests/new.rs", b"fn probe() {}\n");
    let receipt = invoke(
        &cli,
        &root,
        &["git-sync", "--message", "fix: valid nested test"],
    );
    assert_eq!(receipt["status"], "synced");
    assert!(!root.join(".codex/artifact-layout.json").exists());
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
