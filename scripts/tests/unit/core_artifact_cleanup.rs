use super::*;
use std::collections::BTreeMap;

#[path = "core_cleanup_short_paths.rs"]
mod short_paths;
#[cfg(windows)]
#[path = "core_cleanup_junctions.rs"]
mod junctions;
#[cfg(windows)]
#[path = "core_cleanup_performance.rs"]
mod performance;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "cleanup")
            .join(format!("{:x}-{nonce:x}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn put(&self, path: &str, bytes: &[u8]) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn tree(&self) -> BTreeMap<String, Vec<u8>> {
        walkdir::WalkDir::new(&self.0)
            .into_iter()
            .map(|e| {
                let e = e.unwrap();
                let path = e
                    .path()
                    .strip_prefix(&self.0)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                (
                    path,
                    if e.file_type().is_file() {
                        fs::read(e.path()).unwrap()
                    } else {
                        Vec::new()
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

fn keeping(paths: &[&str]) -> BuildPolicy {
    BuildPolicy {
        keep: paths.iter().map(|s| (*s).into()).collect(),
        ..BuildPolicy::default()
    }
}

#[cfg(windows)]
#[test]
fn cleanup_build_keep_list_removes_unknown_tests_snapshots_and_tool_siblings() {
    let f = Fixture::new();
    let keep = [
        ".build/bridgeforge/bin/current.exe",
        ".build/bridgeforge/bin/current.json",
    ];
    let remove = [
        ".build/tests/case/output.bin",
        ".build/bridgeforge/check-target/cache.bin",
        ".build/bridgeforge/integration-target/cache.bin",
        ".build/bridgeforge/cargo/public/source/lib.rs",
        ".build/bridgeforge/cargo/public/inputs.json",
        ".build/bridgeforge/artifact-cache/owner.json",
        ".build/bridgeforge/artifact-cache/cache.lock",
        ".build/bridgeforge/bin/old.exe",
        ".build/bridgeforge/bin/old.json",
        ".build/unregistered/[draft].md",
    ];
    for path in keep.into_iter().chain(remove) {
        f.put(path, b"fixture bytes");
    }
    fs::create_dir_all(f.0.join(".build/empty")).unwrap();
    fs::create_dir_all(f.0.join(".build/missing-product")).unwrap();
    let mut policy = keeping(&keep);
    policy
        .keep
        .push(".build/missing-product/current.exe".into());
    let report = Plan::capture_with_build(&f.0, policy).unwrap().execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_files, remove.len());
    assert_eq!(report.remaining_entries, Some(0));
    assert!(report.skipped_build_paths.is_empty());
    for path in keep {
        assert_eq!(fs::read(f.0.join(path)).unwrap(), b"fixture bytes");
    }
    for path in remove {
        assert!(!f.0.join(path).exists(), "{path}");
    }
    assert!(!f.0.join(".build/empty").exists());
    assert!(!f.0.join(".build/missing-product").exists());
}

#[test]
fn cleanup_preview_is_zero_write_and_protects_exact_subtrees() {
    let f = Fixture::new();
    for path in [
        ".runtime/config/配置.txt",
        ".runtime/download/.partial",
        ".deps/tool.exe",
        "src/keep.rs",
        ".build/bridgeforge/bin/bridgeforge.exe",
        ".build/bridgeforge/bin/build-receipt-cli.json",
        ".build/unknown.bin",
        ".runtime/tests/unit/report.txt",
        ".runtime/config-old/x",
        ".build/bridgeforge/cargo/local/config/x",
    ] {
        f.put(path, b"preserve or remove by exact scope");
    }
    let before = f.tree();
    let policy = keeping(&[
        ".build/bridgeforge/bin/bridgeforge.exe",
        ".build/bridgeforge/bin/build-receipt-cli.json",
    ]);
    let plan = Plan::capture_with_build(&f.0, policy.clone()).unwrap();
    assert_eq!(plan.preview().planned_files, 4);
    assert_eq!(plan.preview().status, "preview");
    assert_eq!(f.tree(), before);
    #[cfg(windows)]
    {
        let report = plan.execute();
        assert_eq!(report.status, "completed", "{:?}", report.failures);
        assert_eq!(report.deleted_files, 4);
        for path in [
            ".runtime/config/配置.txt",
            ".runtime/download/.partial",
            ".deps/tool.exe",
            "src/keep.rs",
            ".build/bridgeforge/bin/bridgeforge.exe",
            ".build/bridgeforge/bin/build-receipt-cli.json",
        ] {
            assert_eq!(
                fs::read(f.0.join(path)).unwrap(),
                b"preserve or remove by exact scope"
            );
        }
        assert!(!f.0.join(".build/unknown.bin").exists());
        assert_eq!(
            Plan::capture_with_build(&f.0, policy)
                .unwrap()
                .execute()
                .deleted_files,
            0
        );
    }
}

#[cfg(windows)]
#[test]
fn cleanup_active_lock_blocks_all_deletion_and_partial_failure_is_reported() {
    let f = Fixture::new();
    f.put(".runtime/a.txt", b"a");
    f.put(".build/bridgeforge/cargo/local/b.txt", b"b");
    let held = crate::file_lock::FileLock::acquire(&f.0.join(".runtime/work.lock")).unwrap();
    let before = Plan::capture(&f.0).unwrap().entries;
    let report = Plan::capture(&f.0).unwrap().execute();
    assert_eq!(report.status, "blocked");
    assert_eq!(report.deleted_files, 0);
    assert_eq!(Plan::capture(&f.0).unwrap().entries, before);
    drop(held);
    let report = Plan::capture(&f.0)
        .unwrap()
        .execute_observed(&mut || Err("injected interruption".into()));
    assert_eq!(report.status, "partial");
    assert_eq!(report.deleted_files, 1);
    assert!(report.remaining_entries.unwrap() > 0);
}

#[cfg(windows)]
#[test]
fn cleanup_hardlink_anchor_denies_writers_between_delete_handles() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::*;
    let f = Fixture::new();
    f.put(".runtime/original", b"original");
    let original = f.0.join(".runtime/original");
    let alias = f.0.join(".runtime/alias");
    fs::hard_link(&original, &alias).unwrap();
    let anchor = windows::read_anchor(&original, windows::path_identity(&original).unwrap()).unwrap();
    let writer = || fs::OpenOptions::new().write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).open(&alias);
    assert_eq!(writer().unwrap_err().raw_os_error(), Some(32));
    drop(anchor);
    assert!(writer().is_ok());
    assert_eq!(fs::read(original).unwrap(), b"original");
}

#[cfg(windows)]
#[test]
fn cleanup_new_entries_before_execution_block_before_first_deletion() {
    let f = Fixture::new();
    f.put(".runtime/original", b"original");
    let plan = Plan::capture(&f.0).unwrap();
    f.put(".runtime/new", b"appeared after planning");
    let before = f.tree();
    let report = plan.execute();
    assert_eq!(report.status, "blocked", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 0);
    assert!(report.failures.iter().any(|e| e.contains("changed during preflight")));
    assert_eq!(f.tree(), before);
}

#[cfg(windows)]
#[test]
fn cleanup_parallel_preflight_failure_deletes_nothing_and_releases_acquired_handles() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::*;
    let f = Fixture::new();
    for index in 0..512 { f.put(&format!(".runtime/{index:03}.bin"), b"payload"); }
    let held = fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ)
        .open(f.0.join(".runtime/511.bin")).unwrap();
    let before = f.tree();
    let report = Plan::capture(&f.0).unwrap().execute();
    assert_eq!(report.status, "blocked", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 0);
    assert_eq!(f.tree(), before);
    // Failed preflight cannot leave another worker's DELETE handle behind.
    let writer = fs::OpenOptions::new().write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(f.0.join(".runtime/000.bin")).unwrap();
    drop(writer);
    drop(held);
    let report = Plan::capture(&f.0).unwrap().execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 512);
}

#[cfg(windows)]
#[test]
fn cleanup_parallel_preflight_deduplicates_hardlinks_across_chunks() {
    let f = Fixture::new();
    for index in 0..512 { f.put(&format!(".runtime/{index:03}.bin"), b"payload"); }
    f.put(".runtime/000-hardlink.bin", b"shared payload");
    fs::hard_link(f.0.join(".runtime/000-hardlink.bin"), f.0.join(".runtime/z-hardlink.bin")).unwrap();
    let report = Plan::capture(&f.0).unwrap().execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 514);
    assert_eq!(report.remaining_entries, Some(0));
}

#[cfg(windows)]
#[test]
fn cleanup_recreated_path_is_preserved_and_counted_as_partial() {
    let f = Fixture::new();
    f.put(".runtime/only.txt", b"original");
    let report = Plan::capture(&f.0).unwrap().execute_observed(&mut || {
        fs::write(f.0.join(".runtime/only.txt"), b"new").map_err(|e| e.to_string())
    });
    assert_eq!(report.status, "partial");
    assert_eq!(report.deleted_files, 1);
    assert_eq!(report.remaining_entries, Some(1));
    assert_eq!(fs::read(f.0.join(".runtime/only.txt")).unwrap(), b"new");
}

#[cfg(windows)]
#[test]
#[ignore = "isolated occupancy helper, started by parent regression"]
fn cleanup_occupancy_child() {
    use std::io::{BufRead, Write};
    println!("cleanup-occupancy-ready");
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).unwrap();
}

#[cfg(windows)]
#[test]
fn cleanup_process_references_and_protected_images_do_not_block_but_target_images_do() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    for location in [None, Some(".deps/worker.exe"), Some(".runtime/worker.exe")] {
        let f = Fixture::new();
        f.put(".runtime/remove.txt", b"remove");
        f.put(".runtime/config/keep.txt", b"keep");
        let current = std::env::current_exe().unwrap();
        let executable = if let Some(location) = location {
            let path = f.0.join(location);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::copy(&current, &path).unwrap();
            path
        } else {
            current
        };
        let mut child = Child(
            Command::new(&executable)
                .creation_flags(0x08000000)
                .args([
                    "artifact_cleanup::tests::cleanup_occupancy_child",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                // An additional test filter puts an absolute project path in the
                // command line without running any extra test.
                .arg(&f.0)
                .current_dir(&f.0)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let mut output = BufReader::new(child.0.stdout.take().unwrap());
        loop {
            let mut line = String::new();
            assert_ne!(
                output.read_line(&mut line).unwrap(),
                0,
                "helper exited before ready"
            );
            if line.contains("cleanup-occupancy-ready") {
                break;
            }
        }
        let report = Plan::capture(&f.0).unwrap().execute();
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "cleanup must not stop a process"
        );
        child
            .0
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"finish\n")
            .unwrap();
        assert!(child.0.wait().unwrap().success());
        if location == Some(".runtime/worker.exe") {
            assert_eq!(report.status, "blocked", "{:?}", report.failures);
            assert_eq!(report.deleted_files, 0);
            assert!(
                report
                    .failures
                    .iter()
                    .any(|e| e.contains("uses cleanup target"))
            );
            assert!(f.0.join(".runtime/remove.txt").exists());
        } else {
            assert_eq!(report.status, "completed", "{:?}", report.failures);
            assert!(!f.0.join(".runtime/remove.txt").exists());
            assert!(executable.exists());
        }
        assert_eq!(
            fs::read(f.0.join(".runtime/config/keep.txt")).unwrap(),
            b"keep"
        );
    }
}

#[cfg(windows)]
#[test]
fn cleanup_actual_target_handle_blocks_before_deletion_then_allows_retry() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
    let f = Fixture::new();
    f.put(".runtime/a.txt", b"first");
    f.put(".runtime/z.txt", b"held");
    make_readonly(&f.0.join(".runtime/z.txt"));
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(f.0.join(".runtime/z.txt"))
        .unwrap();
    let report = Plan::capture(&f.0).unwrap().execute();
    assert_eq!(report.status, "blocked");
    assert_eq!(report.deleted_files, 0);
    assert!(report.failures.iter().any(|e| e.contains("z.txt")));
    assert_eq!(fs::read(f.0.join(".runtime/a.txt")).unwrap(), b"first");
    assert!(
        fs::metadata(f.0.join(".runtime/z.txt"))
            .unwrap()
            .permissions()
            .readonly()
    );
    drop(held);
    let report = Plan::capture(&f.0).unwrap().execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 2);
}

#[cfg(windows)]
fn make_readonly(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(windows)]
#[test]
fn cleanup_readonly_files_and_hardlinks_preserve_protected_contents_and_attributes() {
    use std::os::windows::fs::MetadataExt;
    let f = Fixture::new();
    let protected = [
        ".runtime/config/keep.txt",
        ".runtime/download/keep.txt",
        ".deps/tool.txt",
        ".build/bridgeforge/bin/current.exe",
        ".build/bridgeforge/bin/receipt.json",
    ];
    for path in protected {
        f.put(path, b"protected readonly content");
        make_readonly(&f.0.join(path));
    }
    let object = f.0.join(".runtime/fixture.git/objects/ab/object");
    f.put(
        ".runtime/fixture.git/objects/ab/object",
        b"readonly Git object",
    );
    let alias = f.0.join(".runtime/config/object-alias");
    fs::hard_link(&object, &alias).unwrap();
    make_readonly(&object);
    let cache = f.0.join(".build/bridgeforge/cargo/local/a.obj");
    f.put(".build/bridgeforge/cargo/local/a.obj", b"readonly cache");
    fs::hard_link(&cache, cache.with_file_name("b.obj")).unwrap();
    make_readonly(&cache);
    let before = protected
        .iter()
        .map(|p| {
            let path = f.0.join(p);
            (
                path.clone(),
                fs::metadata(&path).unwrap().file_attributes(),
                fs::read(&path).unwrap(),
            )
        })
        .chain(std::iter::once((
            alias.clone(),
            fs::metadata(&alias).unwrap().file_attributes(),
            fs::read(&alias).unwrap(),
        )))
        .collect::<Vec<_>>();
    let plan = Plan::capture_with_build(
        &f.0,
        keeping(&[
            ".build/bridgeforge/bin/current.exe",
            ".build/bridgeforge/bin/receipt.json",
        ]),
    )
    .unwrap();
    assert_eq!(plan.preview().planned_files, 3);
    assert!(fs::metadata(&object).unwrap().permissions().readonly());
    let report = plan.execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 3);
    assert!(!object.exists());
    assert!(!cache.exists());
    assert!(!cache.with_file_name("b.obj").exists());
    for (path, attributes, bytes) in before {
        assert_eq!(fs::metadata(&path).unwrap().file_attributes(), attributes);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        // Only test teardown changes the preserved fixture's attributes.
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).unwrap();
    }
}

#[cfg(windows)]
#[test]
fn cleanup_readonly_interruption_preserves_remaining_attributes_and_counts_progress() {
    use std::os::windows::fs::MetadataExt;
    let f = Fixture::new();
    f.put(".runtime/a.txt", b"first");
    f.put(".runtime/b.txt", b"remaining");
    make_readonly(&f.0.join(".runtime/a.txt"));
    let remaining = f.0.join(".runtime/b.txt");
    make_readonly(&remaining);
    let attributes = fs::metadata(&remaining).unwrap().file_attributes();
    let report = Plan::capture(&f.0)
        .unwrap()
        .execute_observed(&mut || Err("injected interruption".into()));
    assert_eq!(report.status, "partial", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 1);
    assert_eq!(report.remaining_entries, Some(1));
    assert_eq!(
        fs::metadata(&remaining).unwrap().file_attributes(),
        attributes
    );
    assert_eq!(fs::read(&remaining).unwrap(), b"remaining");
    let retry = Plan::capture(&f.0).unwrap().execute();
    assert_eq!(retry.status, "completed", "{:?}", retry.failures);
    assert_eq!(retry.deleted_files, 1);
}

#[cfg(windows)]
#[test]
#[ignore = "isolated child executor preflight"]
fn cleanup_self_child() {
    let root = PathBuf::from(std::env::var_os("BFC_CLEANUP_FIXTURE").unwrap());
    let unit = std::env::current_exe()
        .unwrap()
        .canonicalize()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut policy = BuildPolicy::default();
    for name in ["receipt.json", "entry.json"] {
        policy.keep.push(
            unit.join(name)
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
    fs::write(unit.join("unused.obj"), b"unused sibling").unwrap();
    let report = Plan::capture_with_build(&root, policy).unwrap().execute();
    println!("{}", serde_json::to_string(&report).unwrap());
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 3);
    assert!(std::env::current_exe().unwrap().exists());
}

#[cfg(windows)]
#[test]
fn cleanup_project_executor_survives_while_runtime_and_caches_are_removed() {
    use crate::ProcessRunner;
    let f = Fixture::new();
    f.put(".runtime/config/keep.txt", b"keep");
    f.put(".runtime/tests/task/report.txt", b"report");
    f.put(".build/bridgeforge/cargo/local/old.obj", b"cache");
    let binary =
        f.0.join(".build/bridgeforge/cargo/local/current-unit/executor.exe");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::copy(std::env::current_exe().unwrap(), &binary).unwrap();
    let alias = f.0.join(".build/bridgeforge/cargo/local/current-alias.exe");
    fs::hard_link(&binary, &alias).unwrap();
    fs::write(binary.with_file_name("receipt.json"), b"current receipt").unwrap();
    fs::write(binary.with_file_name("entry.json"), b"current identity").unwrap();
    let mut request = crate::ProcessRequest::new(binary.as_os_str(), &f.0);
    request.args = [
        "--exact",
        "artifact_cleanup::tests::cleanup_self_child",
        "--ignored",
        "--nocapture",
    ]
    .map(Into::into)
    .to_vec();
    request
        .env
        .insert("BFC_CLEANUP_FIXTURE".into(), f.0.as_os_str().into());
    let result = crate::SystemProcessRunner.run(&request).unwrap();
    assert_eq!(
        result.code,
        0,
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("\"status\":\"completed\""));
    assert!(binary.exists());
    assert!(alias.exists());
    assert_eq!(
        fs::read(binary.with_file_name("receipt.json")).unwrap(),
        b"current receipt"
    );
    assert_eq!(
        fs::read(binary.with_file_name("entry.json")).unwrap(),
        b"current identity"
    );
    assert_eq!(
        fs::read(f.0.join(".runtime/config/keep.txt")).unwrap(),
        b"keep"
    );
}

#[cfg(windows)]
#[test]
fn cleanup_junction_is_removed_without_entering_its_target() {
    use std::os::windows::process::CommandExt;
    let f = Fixture::new();
    f.put(".runtime/remove.txt", b"remove");
    f.put("external/keep.txt", b"keep");
    fs::create_dir(f.0.join(".build")).unwrap();
    let link = f.0.join(".build/link");
    let output = std::process::Command::new("cmd")
        .creation_flags(0x08000000)
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(f.0.join("external"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let report = Plan::capture(&f.0).unwrap().execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_junctions, 1);
    assert!(!f.0.join(".runtime/remove.txt").exists());
    assert_eq!(fs::read(f.0.join("external/keep.txt")).unwrap(), b"keep");
    assert!(fs::symlink_metadata(link).is_err());
}

#[cfg(windows)]
#[test]
fn cleanup_cargo_hardlinks_are_removed_without_self_conflict_and_replacements_survive() {
    let f = Fixture::new();
    let a = ".build/bridgeforge/cargo/local/release/app.exe";
    let b = ".build/bridgeforge/cargo/local/release/deps/app.exe";
    f.put(a, b"old program");
    fs::create_dir_all(f.0.join(b).parent().unwrap()).unwrap();
    fs::hard_link(f.0.join(a), f.0.join(b)).unwrap();
    let report = Plan::capture(&f.0).unwrap().execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 2);
    assert!(!f.0.join(a).exists());
    assert!(!f.0.join(b).exists());

    f.put(a, b"old program");
    fs::create_dir_all(f.0.join(b).parent().unwrap()).unwrap();
    fs::hard_link(f.0.join(a), f.0.join(b)).unwrap();
    let mut replaced = false;
    let report = Plan::capture(&f.0).unwrap().execute_observed(&mut || {
        if !replaced {
            fs::rename(f.0.join(b), f.0.join("preserved-old.bin")).map_err(|e| e.to_string())?;
            fs::write(f.0.join(b), b"external replacement").map_err(|e| e.to_string())?;
            replaced = true;
        }
        Ok(())
    });
    assert_eq!(report.status, "partial", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 1);
    assert_eq!(fs::read(f.0.join(b)).unwrap(), b"external replacement");
    assert_eq!(
        fs::read(f.0.join("preserved-old.bin")).unwrap(),
        b"old program"
    );
}

#[cfg(windows)]
#[test]
fn cleanup_literal_names_are_removed_without_changing_manifest_pattern_policy() {
    let f = Fixture::new();
    let removed = [
        ".runtime/tests/[cache]/[draft].md",
        ".runtime/报告 [1] # $ ` ' (草稿).md",
        ".build/bridgeforge/cargo/local/[crate]/object [v1].o",
    ];
    let preserved = [
        ".runtime/config/[keep].json",
        ".runtime/download/[keep].zip",
        ".deps/[library].dll",
        ".build/bridgeforge/bin/[current].exe",
        "source/[keep].rs",
    ];
    for path in removed.into_iter().chain(preserved) {
        f.put(path, b"literal name");
    }
    let before = f.tree();
    let plan = Plan::capture_with_build(&f.0, keeping(&[".build/bridgeforge/bin"])).unwrap();
    assert_eq!(plan.preview().planned_files, removed.len());
    assert_eq!(f.tree(), before);
    // A configured asset pattern is still prohibited by its separate policy.
    assert!(crate::managed_paths::safe_join(&f.0, removed[0], "asset").is_err());
    let report = plan.execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_files, removed.len());
    assert_eq!(report.remaining_entries, Some(0));
    for path in removed {
        assert!(!f.0.join(path).exists());
    }
    for path in preserved {
        assert_eq!(fs::read(f.0.join(path)).unwrap(), b"literal name");
    }
    assert!(!f.0.join(".runtime/tests/[cache]").exists());
}

#[test]
fn cleanup_literal_paths_still_reject_traversal_roots_and_protected_paths() {
    let f = Fixture::new();
    f.put(".runtime/[draft].md", b"literal");
    f.put("source/keep.txt", b"source");
    f.put(".runtime/config/keep.txt", b"config");
    validate_cleanup_target(&f.0, &f.0.join(".runtime/[draft].md")).unwrap();
    for relative in [
        "",
        ".runtime",
        ".build",
        ".runtime/../source/keep.txt",
        "../outside.txt",
        ".runtime-other/file.txt",
        "source/keep.txt",
        ".runtime/config/keep.txt",
        ".runtime/download/file.txt",
        ".deps/tool.exe",
    ] {
        assert!(
            validate_cleanup_target(&f.0, &f.0.join(relative)).is_err(),
            "{relative}"
        );
    }
    assert!(
        validate_cleanup_target(&f.0, &f.0.parent().unwrap().join(".runtime/outside.txt")).is_err()
    );
    #[cfg(windows)]
    for relative in [
        ".runtime/[draft].md:stream",
        "C:\\outside.txt",
        "\\\\server\\share\\file",
    ] {
        assert!(
            validate_cleanup_target(&f.0, &f.0.join(relative)).is_err(),
            "{relative}"
        );
    }
    assert_eq!(fs::read(f.0.join("source/keep.txt")).unwrap(), b"source");
}

#[cfg(windows)]
#[test]
fn cleanup_literal_brackets_do_not_bypass_link_or_ancestor_checks() {
    use std::os::windows::process::CommandExt;
    let f = Fixture::new();
    f.put(".runtime/remove.txt", b"remove");
    f.put("outside/[draft].md", b"keep");
    let link = f.0.join(".runtime/[linked]");
    let output = std::process::Command::new("cmd")
        .creation_flags(0x08000000)
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(f.0.join("outside"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let leaf_result = validate_cleanup_target(&f.0, &link);
    let child_result = validate_cleanup_target(&f.0, &link.join("[draft].md"));
    let plan_result = Plan::capture(&f.0);
    fs::remove_dir(&link).unwrap();
    assert!(leaf_result.is_ok());
    assert!(child_result.is_err());
    assert_eq!(plan_result.unwrap().preview().planned_junctions, 1);
    assert_eq!(fs::read(f.0.join("outside/[draft].md")).unwrap(), b"keep");
    assert_eq!(
        fs::read(f.0.join(".runtime/remove.txt")).unwrap(),
        b"remove"
    );
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
