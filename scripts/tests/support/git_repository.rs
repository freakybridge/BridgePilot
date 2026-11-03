//! Shared isolated Git fixtures; no production dependency on this module.
use bridgeforge_core::{ProcessOutput, ProcessRequest, ProcessRunner, SystemProcessRunner};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[path = "current_template.rs"]
mod current_template;

// Some package contexts only need git_ok; factory assertions also need output.
#[allow(dead_code)]
pub(crate) fn git_output(root: &Path, args: &[&str]) -> String {
    let mut request = ProcessRequest::new("git", root);
    request.args = args.iter().map(std::ffi::OsString::from).collect();
    request.timeout = Duration::from_secs(30);
    let output = SystemProcessRunner.run(&request).unwrap();
    assert!(
        !output.timed_out && output.code == 0,
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

pub(crate) struct RealRepository(pub(crate) PathBuf);

impl Drop for RealRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn git_ok(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(crate) fn real_repository(name: &str) -> RealRepository {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "git-repository").join(format!(
        "bridgeforge-git-sync-{name}-{}-{}-{token}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    git_ok(&root, &["init"]);
    git_ok(&root, &["config", "user.name", "BridgeForge Test"]);
    git_ok(
        &root,
        &["config", "user.email", "bridgeforge@example.invalid"],
    );
    fs::write(root.join("tracked.txt"), b"base\n").unwrap();
    git_ok(&root, &["add", "tracked.txt"]);
    git_ok(&root, &["commit", "-m", "seed"]);
    RealRepository(root)
}

pub(crate) fn managed_repository(name: &str) -> (RealRepository, PathBuf) {
    let repo = real_repository(name);
    let remote = repo.0.with_file_name(format!(
        "{}-remote.git",
        repo.0.file_name().unwrap().to_string_lossy()
    ));
    current_template::write(&repo.0);
    git_ok(&repo.0, &["config", "core.hooksPath", ".git/hooks"]);
    fs::write(repo.0.join("managed.txt"), b"old\n").unwrap();
    fs::write(
        repo.0.join(".gitignore"),
        bridgeforge_core::artifact_paths::IGNORE_RULES,
    )
    .unwrap();
    git_ok(&repo.0, &["add", "."]);
    git_ok(&repo.0, &["commit", "-m", "baseline"]);
    git_ok(
        &repo.0,
        &["init", "--bare", remote.to_string_lossy().as_ref()],
    );
    git_ok(
        &repo.0,
        &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
    );
    git_ok(&repo.0, &["push", "-u", "origin", "HEAD"]);
    (repo, remote)
}

pub(crate) fn out(code: i32, stdout: &str) -> ProcessOutput {
    ProcessOutput {
        code,
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        timed_out: false,
    }
}

#[allow(dead_code)]
#[path = "artifact_paths.rs"]
mod test_artifacts;
