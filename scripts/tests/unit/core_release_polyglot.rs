use super::*;
use crate::SystemProcessRunner;
use std::process::Command;
pub(super) struct Repo(pub(super) PathBuf);
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(super) fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
pub(super) fn write(root: &Path, path: &str, bytes: &[u8]) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(root.join(path), bytes).unwrap();
}
pub(super) fn identity(id: &str, path: &str) -> Value {
    json!({"id":id,"language":"python","manifest":path,"name":id})
}
pub(super) fn config() -> Value {
    json!({"schema_version":3,"release_policy":"explicit_release","strategy":"independent","adapters":{"python":{"projects":["a/pyproject.toml","b/pyproject.toml"],"local_sources":{"b/pyproject.toml":{"a":"a/pyproject.toml"}}}},"packages":[identity("a","a/pyproject.toml"),identity("b","b/pyproject.toml")],"product_inputs":["README.md",".gitignore"]})
}

#[test]
fn product_input_redundancy_preserves_declared_bytes_and_owner() {
    let mut value = config();
    value["product_inputs"] = json!(["scripts/", "scripts/start.ps1", "scripts/tools/", "scripts/tools/run.ps1"]);
    let bytes = serde_json::to_vec(&value).unwrap();
    let parsed = parse(&bytes).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&parsed.product_inputs).unwrap(), value["product_inputs"]);
    let snapshot = Snapshot::default();
    for path in ["scripts/start.ps1", "scripts/tools/run.ps1", "scripts/future.ps1"] {
        assert_eq!(owner(&snapshot, path, &parsed).unwrap(), Vec::<String>::new());
    }
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), value);
}

#[test]
fn product_input_redundancy_does_not_hide_shared_conflicts() {
    let mut product_shared = config();
    product_shared["product_inputs"] = json!(["scripts/", "scripts/start.ps1"]);
    product_shared["shared_inputs"] = json!([{"path":"scripts/start.ps1","packages":["a"]}]);
    assert!(parse(&serde_json::to_vec(&product_shared).unwrap()).unwrap_err().contains("overlapping"));
    for owners in [json!(["a"]), json!(["b"])] {
        let mut shared_shared = config();
        shared_shared["shared_inputs"] = json!([
            {"path":"scripts/","packages":["a"]},
            {"path":"scripts/start.ps1","packages":owners}
        ]);
        assert!(parse(&serde_json::to_vec(&shared_shared).unwrap()).unwrap_err().contains("overlapping"));
    }
}

#[test]
fn product_input_redundancy_keeps_duplicate_and_unsafe_path_guards() {
    for paths in [vec!["scripts/", "scripts/"], vec!["scripts/", "scripts/start.ps1", "scripts/start.ps1"]] {
        let mut duplicate = config();
        duplicate["product_inputs"] = json!(paths);
        assert!(parse(&serde_json::to_vec(&duplicate).unwrap()).unwrap_err().contains("overlapping"));
    }
    let mut unsafe_path = config();
    unsafe_path["product_inputs"] = json!(["scripts/", "scripts/../private"]);
    assert!(parse(&serde_json::to_vec(&unsafe_path).unwrap()).unwrap_err().contains("unsafe"));
}
pub(super) fn repo_uninitialized() -> Repo {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "release-polyglot").join(format!(
        "bf-polyglot-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init"]);
    git(&dir, &["config", "user.name", "BridgeForge Test"]);
    git(&dir, &["config", "user.email", "test@example.invalid"]);
    write(&dir, "VERSION", b"1.0.0\n");
    write(&dir, "CHANGELOG.md", b"# Changelog\n");
    write(&dir, CONFIG, &serde_json::to_vec(&config()).unwrap());
    write(&dir, ".gitignore", b".runtime/\n");
    write(
        &dir,
        "a/pyproject.toml",
        b"[project]\nname='a'\nversion='1.2.3'\n",
    );
    write(&dir, "a/a.py", b"VALUE=1\n");
    write(
        &dir,
        "b/pyproject.toml",
        b"[project]\nname='b'\nversion='2.0.0'\ndependencies=['a>=1.2.3']\n",
    );
    write(&dir, "b/b.py", b"VALUE=2\n");
    // Empty but valid ownership contract; all ordinary files remain business.
    write(
        &dir,
        ".codex/managed-skeleton.json",
        br#"{"schema_version":1,"release_version":"8.0.0","assets":[]}"#,
    );
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "chore: initial"]);
    Repo(dir)
}
pub(super) fn repo() -> Repo {
    let r = repo_uninitialized();
    super::super::schema4::fixture::migrate(&r.0, &SystemProcessRunner).unwrap();
    r
}

pub(super) fn plan(root: &Path) -> Result<Option<FileReleasePlan>, String> {
    struct ReadOnlyPreview;
    impl ProcessRunner for ReadOnlyPreview {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<crate::ProcessOutput> {
            let index = request.cwd.join(".git/index");
            let before = fs::read(&index).ok();
            let output = SystemProcessRunner.run(request)?;
            assert!(
                fs::read(&index).ok() == before,
                "release inspection changed index: {:?}",
                request.args
            );
            Ok(output)
        }
    }
    let request = super::super::schema4::fixture::analysis(root, &ReadOnlyPreview, "patch", &[])?;
    crate::release::schema4::build(
        root,
        "fix: explicit fixture repair",
        &request,
        &ReadOnlyPreview,
    )
}

#[test]
fn polyglot_python_direct_change_propagates_and_preview_is_read_only() {
    let repo = repo();
    let index = fs::read(repo.0.join(".git/index")).unwrap();
    write(&repo.0, "a/a.py", b"VALUE=3\n");
    let p = plan(&repo.0).unwrap().unwrap();
    assert_eq!(p.package_changes.len(), 1);
    assert!(
        p.package_changes
            .iter()
            .any(|c| c.name == "a" && c.version_after == "1.2.4")
    );
    assert!(
        p.details.as_ref().unwrap()["packages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "b" && p["version_after"] == "2.0.0")
    );
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(repo.0.join("VERSION")).unwrap(), b"1.0.0\n");
}
#[test]
fn polyglot_python_remove_updates_surviving_dependency_and_emits_tombstone() {
    let repo = repo();
    let base = git(&repo.0, &["rev-parse", "HEAD"]);
    let mut cfg: Value = serde_json::from_slice(&fs::read(repo.0.join(CONFIG)).unwrap()).unwrap();
    cfg["adapters"]["python"]["projects"] = json!(["b/pyproject.toml"]);
    cfg["adapters"]["python"]["local_sources"] = json!({});
    cfg["packages"] = json!([identity("b", "b/pyproject.toml")]);
    cfg["lifecycle"] = json!([{"id":"retire-a","base":base,"operation":"remove","compatibility":"compatible","from":[identity("a","a/pyproject.toml")],"to":[]}]);
    fs::remove_dir_all(repo.0.join("a")).unwrap();
    write(
        &repo.0,
        "b/pyproject.toml",
        b"[project]\nname='b'\nversion='2.0.0'\n",
    );
    write(&repo.0, CONFIG, &serde_json::to_vec(&cfg).unwrap());
    let p = plan(&repo.0).unwrap().unwrap();
    assert_eq!(p.package_changes.len(), 1);
    let old = p.package_events.iter().find(|e| e.id == "a").unwrap();
    assert_eq!(old.operation, "retire");
    assert!(old.version_after.is_none());
    assert!(
        p.package_changes
            .iter()
            .any(|e| e.name == "b" && e.version_after == "2.0.1")
    );
}
#[test]
fn polyglot_missing_lifecycle_and_manual_version_are_rejected() {
    let repo = repo();
    write(
        &repo.0,
        "a/pyproject.toml",
        b"[project]\nname='a'\nversion='9.0.0'\n",
    );
    assert!(plan(&repo.0).unwrap_err().contains("version"));
}
#[test]
fn polyglot_config_rejects_duplicate_ids_and_unknown_fields() {
    let mut c = config();
    c["packages"][1]["id"] = json!("a");
    assert!(parse(&serde_json::to_vec(&c).unwrap()).is_err());
    let mut c = config();
    c["guess"] = json!(true);
    assert!(parse(&serde_json::to_vec(&c).unwrap()).is_err());
}

#[test]
fn polyglot_explicit_target_cannot_bypass_a_business_impact_floor() {
    let r = repo();
    write(&r.0, "a/a.py", b"VALUE=3\n");
    let request = super::super::schema4::fixture::analysis(
        &r.0,
        &SystemProcessRunner,
        "minor",
        &[("a", "minor")],
    )
    .unwrap();
    let path = r.0.join(request.analysis_file.as_ref().unwrap());
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["targets"] = json!({"product":"1.0.1"});
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(
        super::super::schema4::build(&r.0, "feat: fixture", &request, &SystemProcessRunner)
            .unwrap_err()
            .contains("below suggested")
    );
}

#[test]
fn polyglot_version_sources_cannot_alias_other_packages_or_product() {
    for (path, kind, key) in [
        ("VERSION", "text", None),
        ("b/pyproject.toml", "toml", Some("project.version")),
    ] {
        let r = repo();
        let mut c: Value = serde_json::from_slice(&fs::read(r.0.join(CONFIG)).unwrap()).unwrap();
        c["adapters"]["python"]["version_sources"] =
            json!({"a/pyproject.toml":{"path":path,"kind":kind,"key":key}});
        write(
            &r.0,
            "a/pyproject.toml",
            b"[project]\nname='a'\ndynamic=['version']\n",
        );
        write(&r.0, CONFIG, &serde_json::to_vec(&c).unwrap());
        assert!(plan(&r.0).unwrap_err().contains("conflicts"));
    }
}
#[test]
fn polyglot_retired_id_stays_retired_across_release_boundaries() {
    let r = repo();
    let base = git(&r.0, &["rev-parse", "HEAD"]);
    let mut c: Value = serde_json::from_slice(&fs::read(r.0.join(CONFIG)).unwrap()).unwrap();
    c["adapters"]["python"]["projects"] = json!(["b/pyproject.toml"]);
    c["adapters"]["python"]["local_sources"] = json!({});
    c["packages"] = json!([identity("b", "b/pyproject.toml")]);
    c["lifecycle"] = json!([{"id":"retire-a","base":base,"operation":"remove","compatibility":"compatible","from":[identity("a","a/pyproject.toml")],"to":[]}]);
    fs::remove_dir_all(r.0.join("a")).unwrap();
    write(
        &r.0,
        "b/pyproject.toml",
        b"[project]\nname='b'\nversion='2.0.0'\n",
    );
    write(&r.0, CONFIG, &serde_json::to_vec(&c).unwrap());
    let p = plan(&r.0).unwrap().unwrap();
    super::super::apply_file_release_plan(&p).unwrap();
    git(&r.0, &["add", "."]);
    git(&r.0, &["commit", "-m", "chore: release retirement"]);
    c["adapters"]["python"]["projects"] = json!(["a/pyproject.toml", "b/pyproject.toml"]);
    c["packages"] = json!([
        identity("a", "a/pyproject.toml"),
        identity("b", "b/pyproject.toml")
    ]);
    write(&r.0, CONFIG, &serde_json::to_vec(&c).unwrap());
    write(
        &r.0,
        "a/pyproject.toml",
        b"[project]\nname='a'\nversion='1.2.3'\n",
    );
    assert!(plan(&r.0).unwrap_err().contains("retired"));
}
#[test]
fn polyglot_mechanical_move_cannot_hide_source_changes() {
    let r = repo();
    let base = git(&r.0, &["rev-parse", "HEAD"]);
    let mut c: Value = serde_json::from_slice(&fs::read(r.0.join(CONFIG)).unwrap()).unwrap();
    let old = identity("a", "a/pyproject.toml");
    let new = identity("a", "moved/pyproject.toml");
    c["packages"][0] = new.clone();
    c["adapters"]["python"]["projects"] = json!(["moved/pyproject.toml", "b/pyproject.toml"]);
    c["adapters"]["python"]["local_sources"] =
        json!({"b/pyproject.toml":{"a":"moved/pyproject.toml"}});
    c["lifecycle"] = json!([{"id":"move-a","base":base,"operation":"move","compatibility":"mechanical","from":[old],"to":[new]}]);
    fs::rename(r.0.join("a"), r.0.join("moved")).unwrap();
    write(&r.0, "moved/a.py", b"VALUE=99\n");
    write(&r.0, CONFIG, &serde_json::to_vec(&c).unwrap());
    let p = plan(&r.0).unwrap().unwrap();
    assert!(
        p.package_changes
            .iter()
            .any(|p| p.name == "a" && p.version_after == "1.2.4")
    );
}
#[test]
fn polyglot_current_setup_is_fingerprinted_and_never_writes_on_preview() {
    let r = repo_uninitialized();
    let original = None;
    let mut c = config();
    fs::remove_file(r.0.join(CONFIG)).unwrap();
    c["schema_version"] = json!(4);
    write(
        &r.0,
        ".runtime/release-proposal.json",
        &serde_json::to_vec(&c).unwrap(),
    );
    let result =
        setup::plan_configuration(&r.0, &SystemProcessRunner, ".runtime/release-proposal.json");
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fs::read(r.0.join(CONFIG)).ok(), original);
    let receipt = result.receipt.unwrap();
    let key = receipt["fingerprint"].as_str().unwrap();
    write(&r.0, "a/a.py", b"VALUE=7\n");
    let drift = setup::apply_configuration(
        &r.0,
        &SystemProcessRunner,
        ".runtime/release-proposal.json",
        key,
        true,
    );
    assert_ne!(drift.code, 0);
    assert_eq!(fs::read(r.0.join(CONFIG)).ok(), original);
}
#[test]
fn polyglot_mechanical_metadata_proof_keeps_business_entrypoint_paths() {
    assert_ne!(
        native_semantics(b"[package]\nname='a'\n[lib]\npath='src/a.rs'\n").unwrap(),
        native_semantics(b"[package]\nname='a'\n[lib]\npath='src/b.rs'\n").unwrap()
    );
    assert_ne!(
        native_semantics(b"[tool.business]\npath='a'\n").unwrap(),
        native_semantics(b"[tool.business]\npath='b'\n").unwrap()
    );
    assert_ne!(
        native_semantics(b"[package.metadata.dependencies.resource]\npath='a'\n").unwrap(),
        native_semantics(b"[package.metadata.dependencies.resource]\npath='b'\n").unwrap()
    );
    assert_eq!(
        native_semantics(b"[dependencies]\na={path='../a',version='1.0.0'}\n").unwrap(),
        native_semantics(b"[dependencies]\na={path='../moved',version='1.0.0'}\n").unwrap()
    );
}

#[test]
fn polyglot_committed_rename_chain_uses_observed_order_in_status_and_preview() {
    let r = repo();
    let base = git(&r.0, &["rev-parse", "HEAD"]);
    write(&r.0, "a/a.py", b"VALUE=42\n");
    git(&r.0, &["add", "."]);
    git(&r.0, &["commit", "-m", "feat: extend original package"]);
    let mut cfg: Value = serde_json::from_slice(&fs::read(r.0.join(CONFIG)).unwrap()).unwrap();
    let old = identity("a", "a/pyproject.toml");
    let mut mid = identity("a", "middle/pyproject.toml");
    mid["name"] = json!("alpha");
    let mut end = identity("a", "last/pyproject.toml");
    end["name"] = json!("beta");
    let first = json!({"id":"rename-1","base":base,"operation":"rename","compatibility":"compatible","from":[old],"to":[mid.clone()]});
    let second = json!({"id":"rename-2","base":base,"operation":"rename","compatibility":"compatible","from":[mid.clone()],"to":[end.clone()]});
    for (dir, name, identity, events) in [
        ("middle", "alpha", mid, json!([first.clone()])),
        ("last", "beta", end, json!([second, first])),
    ] {
        let previous = if dir == "middle" { "a" } else { "middle" };
        fs::rename(r.0.join(previous), r.0.join(dir)).unwrap();
        write(
            &r.0,
            &format!("{dir}/pyproject.toml"),
            format!("[project]\nname='{name}'\nversion='1.2.3'\n").as_bytes(),
        );
        write(
            &r.0,
            "b/pyproject.toml",
            format!("[project]\nname='b'\nversion='2.0.0'\ndependencies=['{name}>=1.2.3']\n")
                .as_bytes(),
        );
        cfg["packages"][0] = identity;
        cfg["adapters"]["python"]["projects"] =
            json!([format!("{dir}/pyproject.toml"), "b/pyproject.toml"]);
        cfg["adapters"]["python"]["local_sources"] = json!({"b/pyproject.toml":{}});
        cfg["adapters"]["python"]["local_sources"]["b/pyproject.toml"][name] =
            json!(format!("{dir}/pyproject.toml"));
        cfg["lifecycle"] = events;
        write(&r.0, CONFIG, &serde_json::to_vec(&cfg).unwrap());
        git(&r.0, &["add", "."]);
        git(&r.0, &["commit", "-m", "refactor: rename package"]);
    }
    super::super::validate_release_baseline(&r.0, &SystemProcessRunner).unwrap();
    let p = plan(&r.0).unwrap().unwrap();
    assert_eq!(p.new_version.to_string(), "1.0.1");
    assert!(
        p.package_changes
            .iter()
            .any(|c| c.name == "beta" && c.version_after == "1.2.4")
    );
    assert_eq!(p.lifecycle_events.len(), 2);
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
