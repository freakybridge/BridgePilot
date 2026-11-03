use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().find(|p| p.join("source/forge/bridgeforge-manifest.json").is_file()).expect("factory repository");
        let root = repository.join(".runtime/tests/build-inputs").join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("original")).unwrap();
        fs::write(
            root.join("original/Cargo.toml"),
            b"[package]\nname='fixture'\nversion='1.0.0'\n",
        )
        .unwrap();
        fs::write(root.join("original/Cargo.lock"), b"version=4\n").unwrap();
        fs::write(root.join("original/keep.rs"), b"// keep\n").unwrap();
        Self(root)
    }
    fn asset(&self) -> Value {
        let root = self.0.join("original");
        let recipe = generated_build_recipe("bridgeforge");
        let test = json!({"args":["self-test"],"expected_json":{"status":"ok"}});
        json!({"manifest":"Cargo.toml","lockfile":"Cargo.lock","build":recipe,"self_test":test,
            "build_recipe_sha256":canonical_sha(&recipe).unwrap(),"self_test_sha256":canonical_sha(&test).unwrap(),
            "source_tree_sha256":generated_source_sha(&root,&generated_sources(&root).unwrap()).unwrap(),
            "lockfile_sha256":lock_sha(&fs::read(root.join("Cargo.lock")).unwrap())})
    }
    fn capture(&self, environment: &str) -> Result<BuildInputs, String> {
        BuildInputs::capture_persistent(
            &self.0.join("original"),
            &self.0.join(".build/bridgeforge/cargo/public"),
            &self.asset(),
            environment,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn persistent_inputs_preserve_mtime_and_remove_only_owned_deleted_sources() {
    let f = Fixture::new();
    fs::write(f.0.join("original/deleted.rs"), b"// deleted\n").unwrap();
    let first = f.capture("compiler-one").unwrap();
    let kept = first.snapshot.join("keep.rs");
    let timestamp = fs::metadata(&kept).unwrap().modified().unwrap();
    let target = first.target.as_ref().unwrap();
    fs::create_dir_all(target).unwrap();
    fs::write(target.join("dependency.rlib"), b"existing compiler output").unwrap();
    f.capture("compiler-one").unwrap();
    assert_eq!(fs::metadata(&kept).unwrap().modified().unwrap(), timestamp);
    fs::remove_file(f.0.join("original/deleted.rs")).unwrap();
    fs::write(
        f.0.join("original/Cargo.toml"),
        b"[package]\nname='fixture'\nversion='1.0.1'\n",
    )
    .unwrap();
    f.capture("compiler-one").unwrap();
    assert!(!first.snapshot.join("deleted.rs").exists());
    assert_eq!(fs::metadata(&kept).unwrap().modified().unwrap(), timestamp);
    assert!(target.join("dependency.rlib").exists());
    f.capture("compiler-two").unwrap();
    assert!(!target.exists());
    assert_eq!(fs::metadata(&kept).unwrap().modified().unwrap(), timestamp);
}

#[test]
fn persistent_inputs_reject_unknown_content_and_changed_captured_bytes() {
    for unknown in [true, false] {
        let f = Fixture::new();
        let captured = f.capture("compiler").unwrap();
        let path = captured
            .snapshot
            .join(if unknown { "unexpected.rs" } else { "keep.rs" });
        fs::write(&path, b"external change").unwrap();
        assert!(captured.verify_unchanged().is_err());
        assert!(
            f.capture("compiler")
                .err()
                .unwrap()
                .contains("ownership or bytes drifted")
        );
        assert_eq!(fs::read(path).unwrap(), b"external change");
    }
}

#[test]
fn persistent_inputs_refuse_unowned_target_and_slot_entries() {
    let f = Fixture::new();
    let slot = f.0.join(".build/bridgeforge/cargo/public");
    fs::create_dir_all(slot.join("target")).unwrap();
    fs::write(slot.join("target/external.bin"), b"preserve").unwrap();
    assert!(f.capture("compiler").err().unwrap().contains("unowned"));
    assert_eq!(
        fs::read(slot.join("target/external.bin")).unwrap(),
        b"preserve"
    );
}

#[cfg(windows)]
#[test]
fn persistent_inputs_reject_nested_target_junction() {
    use std::os::windows::process::CommandExt;
    let f = Fixture::new();
    let inputs = f.capture("compiler").unwrap();
    let target = inputs.target.unwrap();
    fs::create_dir_all(&target).unwrap();
    let external = f.0.join("external");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("keep.txt"), b"preserve").unwrap();
    let link = target.join("linked");
    let result = std::process::Command::new("cmd")
        .creation_flags(0x08000000)
        .args(["/c", "mklink", "/J"])
        .arg(link.to_string_lossy().replace('/', "\\"))
        .arg(external.to_string_lossy().replace('/', "\\"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(f.capture("compiler").err().unwrap().contains("linked"));
    fs::remove_dir(link).unwrap();
}

#[test]
fn persistent_sync_rolls_back_write_and_metadata_failures_then_retries() {
    for fail_at in [2, 4] {
        let f = Fixture::new();
        let captured = f.capture("compiler").unwrap();
        let before = inventory(&captured.snapshot).unwrap();
        let metadata = captured.snapshot.parent().unwrap().join("inputs.json");
        let old = fs::read(&metadata).unwrap();
        let mut next = captured.files.clone();
        for name in ["a.rs", "b.rs", "keep.rs"] {
            next.insert(name.into(), b"// changed\n".to_vec());
        }
        let mut count = 0;
        let result = sync_slot_with(&captured.snapshot, &next, "compiler", &mut |path, bytes| {
            count += 1;
            if count == fail_at {
                return Err("injected I/O failure".into());
            }
            crate::persistence::atomic_write(path, bytes).map_err(|e| e.to_string())
        });
        assert!(result.unwrap_err().contains("rollback: complete"));
        assert_eq!(inventory(&captured.snapshot).unwrap(), before);
        assert_eq!(fs::read(metadata).unwrap(), old);
        sync_slot(&captured.snapshot, &next, "compiler").unwrap();
        assert_eq!(
            inventory(&captured.snapshot).unwrap(),
            next.iter().map(|(p, b)| (p.clone(), raw_hash(b))).collect()
        );
    }
}
