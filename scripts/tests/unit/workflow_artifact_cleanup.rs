use super::*;
use bridgeforge_core::{ProcessOutput, SystemProcessRunner};
use std::{
    cell::Cell,
    collections::BTreeMap,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/cleanup-restoration")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&root).unwrap();
        let result = Self(root.canonicalize().unwrap());
        result.put("Cargo.toml", b"[workspace]\nmembers=[]\n");
        result
    }
    fn put(&self, relative: &str, bytes: &[u8]) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn args(&self, preview: bool) -> Vec<String> {
        let mut args = vec![
            "cleanup".into(),
            "--root".into(),
            self.0.to_string_lossy().into_owned(),
            "--products-reviewed".into(),
            "--evidence".into(),
            "Cargo.toml".into(),
        ];
        if preview {
            args.push("--dry-run".into());
        }
        args
    }
    fn tree(&self) -> BTreeMap<String, Vec<u8>> {
        walkdir::WalkDir::new(&self.0)
            .into_iter()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry
                        .path()
                        .strip_prefix(&self.0)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    if entry.file_type().is_file() {
                        fs::read(entry.path()).unwrap()
                    } else {
                        vec![]
                    },
                )
            })
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Git {
    tracked: bool,
    calls: Cell<usize>,
    drift: bool,
}
impl Git {
    fn clean() -> Self {
        Self {
            tracked: false,
            calls: Cell::new(0),
            drift: false,
        }
    }
}
impl ProcessRunner for Git {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        self.calls.set(self.calls.get() + 1);
        assert_eq!(request.program, "git");
        let stdout = if request.args[2] == "rev-parse" {
            if self.drift && self.calls.get() == 3 {
                fs::write(request.cwd.join("Cargo.toml"), b"changed native policy")?;
            }
            request.cwd.to_string_lossy().as_bytes().to_vec()
        } else {
            assert_eq!(request.args[2], "ls-files");
            if self.tracked {
                b".build/user-data\0".to_vec()
            } else {
                vec![]
            }
        };
        Ok(ProcessOutput {
            code: 0,
            stdout,
            stderr: vec![],
            timed_out: false,
        })
    }
}

#[test]
fn cleanup_preview_is_zero_write_and_ignores_retired_sidecars() {
    let fixture = Fixture::new();
    for path in [
        ".runtime/config/local",
        ".runtime/download/file",
        ".runtime/logs/[draft].md",
        ".deps/current",
        ".build/current/app.exe",
        ".build/current/old.pdb",
        ".build/project-release/bin/bridgeforge.exe",
        ".build/project-release/bin/receipt.json",
        ".build/project-release/bin/old.exe",
        ".codex/managed-skeleton.json",
        ".codex/artifact-layout.json",
        ".codex/project-hooks.json",
    ] {
        fixture.put(path, b"not JSON; must not be interpreted");
    }
    let before = fixture.tree();
    let mut args = fixture.args(true);
    args.extend(["--keep".into(), ".build/current/app.exe".into()]);
    let result = command(&args, &Git::clean());
    assert_eq!(result.code, 0, "{:?}", result.receipt);
    let report = result.receipt.unwrap();
    assert_eq!(report["status"], "preview");
    assert_eq!(report["read_only"], true);
    assert_eq!(report["deleted_files"], 0);
    let preserved = report["preserved"].as_array().unwrap();
    assert!(preserved.contains(&json!(".build/current/app.exe")));
    assert!(preserved.contains(&json!(crate::project_release::PROGRAM)));
    assert!(
        report["targets"]
            .as_array()
            .unwrap()
            .contains(&json!(".build/current/old.pdb"))
    );
    assert!(
        report["targets"]
            .as_array()
            .unwrap()
            .contains(&json!(".build/project-release/bin/old.exe"))
    );
    assert_eq!(fixture.tree(), before);
}

#[test]
fn cleanup_requires_review_evidence_and_safe_exact_keeps() {
    let fixture = Fixture::new();
    #[cfg(windows)]
    for keep in [".runtime/python.", ".runtime/python ", ".build/app:stream"] {
        let mut args = fixture.args(true);
        args.extend(["--keep".into(), keep.into()]);
        assert_eq!(command(&args, &Git::clean()).code, crate::EXIT_BLOCKED);
    }
    for extra in [
        vec!["--keep", ".build"],
        vec!["--keep", ".runtime"],
        vec!["--keep", "../outside"],
        vec!["--keep", ".build/*"],
        vec!["--evidence", ".runtime/policy"],
    ] {
        let mut args = fixture.args(true);
        args.extend(extra.into_iter().map(String::from));
        let result = command(&args, &Git::clean());
        assert_eq!(result.code, crate::EXIT_BLOCKED, "{:?}", result.receipt);
    }
    for args in [
        vec!["cleanup".into()],
        vec!["cleanup".into(), "--products-reviewed".into()],
    ] {
        let git = Git::clean();
        assert_eq!(command(&args, &git).code, crate::EXIT_BLOCKED);
        assert_eq!(git.calls.get(), 0);
    }
}

#[test]
fn cleanup_rejects_tracked_artifacts_and_pending_transactions() {
    let fixture = Fixture::new();
    fixture.put(".runtime/logs/data", b"keep on failure");
    let mut git = Git::clean();
    git.tracked = true;
    assert_eq!(
        command(&fixture.args(false), &git).code,
        crate::EXIT_BLOCKED
    );
    for guard in [
        ".runtime/bridgeforge-codex/develop-transaction.json",
        ".runtime/bridgeforge-codex/release-preparation/commit-guard.json",
    ] {
        fixture.put(guard, b"{}");
        assert_eq!(
            command(&fixture.args(false), &Git::clean()).code,
            crate::EXIT_BLOCKED
        );
        fs::remove_file(fixture.0.join(guard)).unwrap();
    }
    assert_eq!(
        fs::read(fixture.0.join(".runtime/logs/data")).unwrap(),
        b"keep on failure"
    );
}

#[cfg(windows)]
#[test]
fn cleanup_rejects_short_alias_evidence_inside_deletion_trees() {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetShortPathNameW(input: *const u16, output: *mut u16, capacity: u32) -> u32;
    }
    fn short(path: &Path) -> PathBuf {
        let input = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut output = vec![0u16; 32768];
        let length =
            unsafe { GetShortPathNameW(input.as_ptr(), output.as_mut_ptr(), output.len() as u32) };
        assert!(length > 0 && (length as usize) < output.len());
        PathBuf::from(std::ffi::OsString::from_wide(&output[..length as usize]))
    }
    let fixture = Fixture::new();
    fixture.put(
        ".runtime/native-policy.toml",
        b"must not serve as removable evidence",
    );
    let relative = short(&fixture.0.join(".runtime/native-policy.toml"))
        .strip_prefix(short(&fixture.0))
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let mut args = fixture.args(false);
    args.extend(["--evidence".into(), relative]);
    let result = command(&args, &Git::clean());
    assert_eq!(result.code, crate::EXIT_BLOCKED);
    assert!(
        result.receipt.unwrap()["failures"][0]
            .as_str()
            .unwrap()
            .contains("outside the removable")
    );
    assert!(fixture.0.join(".runtime/native-policy.toml").exists());
}

#[test]
fn cleanup_rejects_nested_directory_as_project_root_with_real_git() {
    let fixture = Fixture::new();
    let result = command(&fixture.args(true), &SystemProcessRunner);
    assert_eq!(result.code, crate::EXIT_BLOCKED);
    assert!(
        result.receipt.unwrap()["failures"][0]
            .as_str()
            .unwrap()
            .contains("exact Git project root")
    );
}

#[cfg(windows)]
#[test]
fn cleanup_execution_preserves_runtime_dependency_and_current_products() {
    let fixture = Fixture::new();
    for path in [
        ".runtime/config/local",
        ".runtime/download/file",
        ".deps/current",
        ".runtime/python/bin/python.exe",
        ".build/current/app.exe",
        ".build/current/assets/icon",
        ".build/project-release/bin/bridgeforge.exe",
        ".build/project-release/bin/receipt.json",
    ] {
        fixture.put(path, b"current");
    }
    fixture.put(".runtime/logs/[draft].md", b"remove");
    fixture.put(".build/current/cache/old.obj", b"remove");
    let mut args = fixture.args(false);
    for keep in [
        ".runtime/python/.",
        ".build/current/app.exe",
        ".build/current/assets//.",
    ] {
        args.extend(["--keep".into(), keep.into()]);
    }
    let result = command(&args, &Git::clean());
    assert_eq!(result.code, 0, "{:?}", result.receipt);
    let report = result.receipt.unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["deleted_files"], 2);
    assert_eq!(report["remaining_entries"], 0);
    assert_eq!(
        fs::read(fixture.0.join(".runtime/python/bin/python.exe")).unwrap(),
        b"current"
    );
    assert_eq!(
        fs::read(fixture.0.join(".build/current/assets/icon")).unwrap(),
        b"current"
    );
    assert_eq!(
        command(&args, &Git::clean()).receipt.unwrap()["deleted_files"],
        0
    );
}

#[cfg(windows)]
#[test]
fn cleanup_native_evidence_drift_blocks_before_first_delete() {
    let fixture = Fixture::new();
    fixture.put(".runtime/logs/data", b"must survive");
    let mut git = Git::clean();
    git.drift = true;
    let result = command(&fixture.args(false), &git);
    assert_eq!(result.code, crate::EXIT_BLOCKED);
    assert_eq!(result.receipt.unwrap()["deleted_files"], 0);
    assert!(fixture.0.join(".runtime/logs/data").exists());
}
