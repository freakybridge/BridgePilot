use super::*;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> Fixture {
    let path = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "source-projection").join(format!(
        "bf-projection-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for package in ["core", "factory"] {
        fs::create_dir_all(path.join(format!("source/{package}/src"))).unwrap();
        fs::write(
            path.join(format!("source/{package}/Cargo.toml")),
            format!("[package]\nname='bridgeforge-{package}'\nversion='0.1.0'\nedition='2024'\n"),
        )
        .unwrap();
        fs::write(
            path.join(format!("source/{package}/src/lib.rs")),
            "pub fn run() {}\n",
        )
        .unwrap();
    }
    fs::write(path.join("Cargo.toml"), "[workspace]\nmembers=['source/core','source/factory']\nresolver='3'\n[workspace.metadata.bridgeforge]\npublic-members=['source/core']\n").unwrap();
    fs::write(path.join("Cargo.lock"), "version = 4\n[[package]]\nname='bridgeforge-core'\nversion='0.1.0'\ndependencies=['shared']\n[[package]]\nname='bridgeforge-factory'\nversion='0.1.0'\ndependencies=['factory-only']\n[[package]]\nname='shared'\nversion='1.0.0'\nsource='registry+https://example.invalid/index'\n[[package]]\nname='factory-only'\nversion='1.0.0'\nsource='registry+https://example.invalid/index'\n").unwrap();
    fs::create_dir_all(path.join(".git")).unwrap();
    fs::write(path.join(".git/private"), "not a build input").unwrap();
    Fixture(path)
}

#[test]
fn source_projection_keeps_public_dependency_closure_and_excludes_factory_and_repository() {
    let f = fixture();
    let projected = render(&f.0, true).unwrap();
    assert_eq!(projected.len(), 4);
    assert!(
        projected
            .keys()
            .all(|p| !p.contains("factory") && !p.starts_with(".git"))
    );
    let lock = String::from_utf8(projected["Cargo.lock"].clone()).unwrap();
    assert!(lock.contains("shared"));
    assert!(!lock.contains("factory-only"));
    assert!(!lock.contains("bridgeforge-factory"));
    let all = render(&f.0, false).unwrap();
    assert!(all.contains_key("source/factory/src/lib.rs"));
    assert!(!all.contains_key(".git/private"));
}

#[test]
fn captured_projection_fingerprint_matches_raw_recheck_and_detects_content_and_inventory_drift() {
    let f=fixture();
    let (projected,proof)=render_with_fingerprint(&f.0,true).unwrap();
    assert_eq!(projected,render(&f.0,true).unwrap());
    assert_eq!(proof,input_fingerprint(&f.0,true).unwrap());
    fs::write(f.0.join(".git/private"),"not relevant").unwrap();
    fs::write(f.0.join("source/factory/src/lib.rs"),"private change").unwrap();
    assert_eq!(proof,input_fingerprint(&f.0,true).unwrap());
    let path=f.0.join("source/core/src/lib.rs");let stamp=fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path,"pub fn row() {}\n").unwrap();
    fs::OpenOptions::new().write(true).open(&path).unwrap().set_modified(stamp).unwrap();
    assert_ne!(proof,input_fingerprint(&f.0,true).unwrap());
    let (_,changed)=render_with_fingerprint(&f.0,true).unwrap();assert_eq!(changed,input_fingerprint(&f.0,true).unwrap());
    fs::write(f.0.join("source/core/src/new.rs"),"new input").unwrap();
    assert_ne!(changed,input_fingerprint(&f.0,true).unwrap());
}

#[test]
fn source_projection_rejects_dependency_on_an_excluded_local_package() {
    let f = fixture();
    fs::write(f.0.join("source/core/Cargo.toml"), "[package]\nname='bridgeforge-core'\nversion='0.1.0'\n[dependencies]\nbridgeforge-factory={path='../factory'}\n").unwrap();
    assert!(
        render(&f.0, true)
            .unwrap_err()
            .contains("outside projection")
    );
}

#[test]
fn source_projection_rejects_inherited_dependency_on_excluded_factory() {
    let f = fixture();
    let mut root = fs::read_to_string(f.0.join("Cargo.toml")).unwrap();
    root.push_str("\n[workspace.dependencies]\nbridgeforge-factory={path='source/factory'}\n");
    fs::write(f.0.join("Cargo.toml"), root).unwrap();
    fs::write(f.0.join("source/core/Cargo.toml"),"[package]\nname='bridgeforge-core'\nversion='0.1.0'\n[dependencies]\nbridgeforge-factory.workspace=true\n").unwrap();
    assert!(
        render(&f.0, true)
            .unwrap_err()
            .contains("outside projection")
    );
}

#[test]
fn lock_closure_matches_managed_local_priority_and_explicit_registry_selection() {
    let template = "version=4\n[[package]]\nname='consumer'\nversion='1.0.0'\ndependencies=['SELECT']\n[[package]]\nname='shared'\nversion='1.0.0'\n[[package]]\nname='shared'\nversion='1.0.0'\nsource='registry+https://example.invalid/index'\n";
    let seeds = BTreeSet::from(["consumer".into()]);
    let local = String::from_utf8(
        lock_dependency_closure(
            template.replace("SELECT", "shared 1.0.0").as_bytes(),
            &seeds,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(!local.contains("registry+"));
    let registry = String::from_utf8(
        lock_dependency_closure(
            template
                .replace(
                    "SELECT",
                    "shared 1.0.0 (registry+https://example.invalid/index)",
                )
                .as_bytes(),
            &seeds,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(registry.contains("registry+"));
    assert_eq!(registry.matches("name='shared'").count(), 1);
}

#[test]
fn source_projection_rejects_missing_and_ambiguous_pinned_dependencies() {
    let f = fixture();
    let mut lock = fs::read_to_string(f.0.join("Cargo.lock")).unwrap();
    lock.push_str("[[package]]\nname='shared'\nversion='2.0.0'\nsource='registry+https://example.invalid/index'\n");
    fs::write(f.0.join("Cargo.lock"), &lock).unwrap();
    assert!(render(&f.0, true).unwrap_err().contains("ambiguous"));
    fs::write(
        f.0.join("Cargo.lock"),
        lock.replace("dependencies=['shared']", "dependencies=['missing']"),
    )
    .unwrap();
    assert!(render(&f.0, true).unwrap_err().contains("ambiguous"));
}

#[test]
fn source_projection_is_idempotent_and_ignores_factory_only_changes() {
    let f = fixture();
    let first = render(&f.0, true).unwrap();
    fs::write(
        f.0.join("source/factory/src/lib.rs"),
        "pub fn factory_changed() {}\n",
    )
    .unwrap();
    assert_eq!(first, render(&f.0, true).unwrap());
    let projected = f.0.join("projected");
    for (relative, payload) in &first {
        let path = projected.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, payload).unwrap();
    }
    assert_eq!(first, render(&projected, true).unwrap());
}

#[test]
fn actual_public_projection_builds_locked_without_factory_or_factory_test_sources() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().find(|p| p.join("source/forge/bridgeforge-manifest.json").is_file()).expect("factory repository");
    assert!(root.join(physical_path(root, "source/factory/Cargo.toml")).is_file());
    for public in [true, false] {
    let before = render(root, public).unwrap();
    let output = root.join(".runtime/tests/bridgepilot-stage1").join(if public {"public-build"} else {"factory-build"});
    for (relative, payload) in &before {
        let path = output.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, payload).unwrap();
    }
    assert_eq!(before.keys().any(|p| p.starts_with("source/factory/")), !public);
    let mut request = crate::ProcessRequest::new("cargo", &output);
    request.args = vec![
        "check".into(),
        "--quiet".into(),
        "--locked".into(),
        "--offline".into(),
        "--workspace".into(),
        "--target-dir".into(),
        root.join(".build/bridgeforge/test-target").into_os_string(),
    ];
    request.timeout = std::time::Duration::from_secs(300);
    let result = crate::ProcessRunner::run(&crate::SystemProcessRunner, &request).unwrap();
    assert!(
        !result.timed_out && result.code == 0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        before,
        render(root, public).unwrap(),
        "building must not alter canonical inputs"
    );
    assert_eq!(
        fs::read(output.join("Cargo.lock")).unwrap(),
        before["Cargo.lock"]
    );
    }
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;

#[test]
fn nested_sources_keep_logical_targets_and_isolate_orchestrator_in_both_projections() {
    let f = fixture();
    fs::create_dir_all(f.0.join("source/forge")).unwrap();
    for package in ["core", "factory"] {
        fs::rename(f.0.join(format!("source/{package}")), f.0.join(format!("source/forge/{package}"))).unwrap();
    }
    fs::create_dir_all(f.0.join("source/orchestrator")).unwrap();
    fs::write(f.0.join("source/orchestrator/Cargo.toml"), "[package]\nname='bridgepilot-orchestrator'\nversion='0.1.0'\n").unwrap();
    fs::write(f.0.join("Cargo.toml"), "[workspace]\nmembers=['source/forge/core','source/forge/factory','source/orchestrator']\nresolver='3'\n[workspace.metadata.bridgeforge]\npublic-members=['source/forge/core']\nfactory-members=['source/forge/core','source/forge/factory']\n[workspace.dependencies]\nbridgeforge-core={path='source/forge/core'}\nbridgepilot-orchestrator={path='source/orchestrator'}\n").unwrap();
    let mut lock = fs::read_to_string(f.0.join("Cargo.lock")).unwrap();
    lock.push_str("[[package]]\nname='bridgepilot-orchestrator'\nversion='0.1.0'\ndependencies=['pilot-only']\n[[package]]\nname='pilot-only'\nversion='1.0.0'\nsource='registry+https://example.invalid/index'\n");
    fs::write(f.0.join("Cargo.lock"), lock).unwrap();
    for public in [true, false] {
        let projected = render(&f.0, public).unwrap();
        assert!(projected.contains_key("source/core/src/lib.rs"));
        assert!(!projected.keys().any(|path| path.contains("forge/") || path.contains("orchestrator")));
        let manifest = std::str::from_utf8(&projected["Cargo.toml"]).unwrap();
        assert!(!manifest.contains("source/forge/"));
        assert!(!manifest.contains("orchestrator"));
        let lock = std::str::from_utf8(&projected["Cargo.lock"]).unwrap();
        assert!(!lock.contains("pilot-only") && !lock.contains("orchestrator"));
        let snapshot = f.0.join(if public {"public"} else {"factory"});
        for (relative, bytes) in &projected {
            let path = snapshot.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        assert_eq!(projected, render(&snapshot, public).unwrap());
    }
    let overlay = BTreeMap::from([("source/forge/core/src/lib.rs".into(), b"pub fn updated() {}\n".to_vec())]);
    assert_eq!(render_with_overlay(&f.0, true, &overlay).unwrap()["source/core/src/lib.rs"], b"pub fn updated() {}\n");
    let config = fs::read_to_string(f.0.join("Cargo.toml")).unwrap();
    fs::write(f.0.join("Cargo.toml"), config.replace("factory-members=['source/forge/core','source/forge/factory']\n", "")).unwrap();
    assert!(render(&f.0, false).unwrap_err().contains("factory members missing"));
}

#[test]
fn projection_rejects_duplicate_logical_sources() {
    let f = fixture();
    fs::write(f.0.join("Cargo.toml"), "[workspace]\nmembers=['source/core','source/forge/core']\n[workspace.metadata.bridgeforge]\npublic-members=['source/core','source/forge/core']\n").unwrap();
    assert!(render(&f.0, true).unwrap_err().contains("invalid projection member"));
}
