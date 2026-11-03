//! Separate project-owned roots for test runtime data and compiled probes.
use std::path::{Path, PathBuf};

pub fn root(manifest_dir: &str, task: &str) -> PathBuf {
    task_root(manifest_dir, task, ".runtime")
}

#[allow(dead_code)]
pub fn build_root(manifest_dir: &str, task: &str) -> PathBuf {
    task_root(manifest_dir, task, ".build")
}

fn task_root(manifest_dir: &str, task: &str, area: &str) -> PathBuf {
    assert!(!task.is_empty() && task.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
    let repository = Path::new(manifest_dir)
        .ancestors()
        .find(|path| {
            path.join("source/forge/bridgeforge-manifest.json").is_file()
                && path.join("Cargo.toml").is_file()
        })
        .expect("test task must locate the factory workspace");
    let path = repository.join(area).join("tests").join(task);
    std::fs::create_dir_all(&path).expect("create project test task root");
    path
}
