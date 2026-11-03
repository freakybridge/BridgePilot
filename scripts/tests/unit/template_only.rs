use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

#[test]
fn factory_mirrors_share_entrypoint_rules_without_overwriting_its_document_index() {
    let assets = template_standard::assets();
    let index = assets.iter().find(|a| a.rule["id"] == "codex.doc.readme").unwrap();
    assert!(!bridgeforge_skeleton::template::factory_managed_asset(&index.rule));
    for id in ["root.agents","codex.config","codex.hooks-config","codex.precommit","codex.doc.project-rust-hooks"] {
        let asset=assets.iter().find(|a|a.rule["id"]==id).unwrap();
        assert!(bridgeforge_skeleton::template::factory_managed_asset(&asset.rule));
    }
    let hooks=assets.iter().find(|a|a.rule["id"]=="codex.hooks-config").unwrap();
    let converted=bridgeforge_skeleton::template::factory_entrypoint_payload(&hooks.payload).unwrap();
    let document: Value=serde_json::from_slice(&converted).unwrap();
    for (_,record) in bridgeforge_skeleton::baseline::hook_handlers(&document).unwrap() {
        assert!(record["handler"]["commandWindows"].as_str().unwrap().starts_with(".build\\bridgeforge\\bin\\"));
        assert!(record["handler"]["command"].as_str().unwrap().starts_with(".build/bridgeforge/bin/"));
    }
}

#[test]
fn template_runtime_installation_rejects_unknown_or_modified_bytes() {
    let id = "codex.bridgeforge-cli";
    let receipt = serde_json::to_vec(&json!({"schema_version":2,"generated_asset_id":id,
        "platform":bridgeforge_core::build_receipt::platform_key().unwrap(),"binary_sha256":hash(b"owned")})).unwrap();
    verify_owned_runtime(id, None, None).unwrap();
    verify_owned_runtime(id, Some(b"owned"), Some(&receipt)).unwrap();
    verify_owned_runtime(id, None, Some(&receipt)).unwrap();
    for (binary, proof) in [(Some(b"unknown".as_slice()),None), (Some(b"modified".as_slice()),Some(receipt.as_slice())),
        (Some(b"owned".as_slice()),Some(b"invalid".as_slice()))] {
        assert!(verify_owned_runtime(id, binary, proof).unwrap_err().contains("preserved"));
    }
    assert!(verify_owned_runtime("codex.hooks",Some(b"owned"),Some(&receipt)).is_err());
}

#[test]
fn retired_cleanup_does_not_consult_config_or_execute_any_process() {
    struct Never;
    impl bridgeforge_core::ProcessRunner for Never {
        fn run(&self, _: &bridgeforge_core::ProcessRequest) -> std::io::Result<bridgeforge_core::ProcessOutput> {
            panic!("retired cleanup must not run processes");
        }
    }
    let out = crate::artifact_cleanup::command_with_contract(&["cleanup".into(),"--root".into(),"missing".into()], &Never,
        &|_,_| panic!("retired cleanup must not read old config"));
    assert_eq!(out.code, 2); assert!(out.stderr.contains("retired"));
}

#[test]
fn retired_release_library_api_rejects_before_remote_integration() {
    struct Never;
    impl bridgeforge_core::ProcessRunner for Never {
        fn run(&self, _: &bridgeforge_core::ProcessRequest) -> std::io::Result<bridgeforge_core::ProcessOutput> {
            panic!("retired release must not inspect Git or start remote integration");
        }
    }
    for remote in [false,true] {
        let options = crate::git_sync::GitSyncOptions {release:true, ..Default::default()};
        let out = if remote { crate::git_sync::sync_remote(Path::new("missing"),&Never,options) }
            else { crate::git_sync::sync(Path::new("missing"),&Never,options) };
        assert_eq!(out.code,2); assert!(out.stderr.contains("retired"));
    }
}
impl Fixture {
    fn new(label: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.runtime/tests/template-only-regression")
            .join(format!("{}-{}-{label}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
}
impl Drop for Fixture { fn drop(&mut self) {
    fn writable(path: &Path) {
        let metadata = fs::symlink_metadata(path).unwrap();
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() { writable(&entry.unwrap().path()); }
        } else if metadata.permissions().readonly() {
            let mut permissions = metadata.permissions(); permissions.set_readonly(false);
            fs::set_permissions(path, permissions).unwrap();
        }
    }
    assert!(self.0.to_string_lossy().contains("template-only-regression"));
    writable(&self.0); fs::remove_dir_all(&self.0).unwrap();
} }

#[test]
fn template_retirement_verifies_after_deletion_and_rolls_back_final_failure() {
    for failure in [false, true] {
        let fixture = Fixture::new("rollback");
        let root = &fixture.0;
        let target = root.join("config.toml");
        let retired = root.join("old.json");
        fs::write(&target, b"before").unwrap(); fs::write(&retired, b"retired").unwrap();
        let writes = BTreeMap::from([(target.clone(), b"after".to_vec())]);
        let snapshots = BTreeMap::from([(target.clone(), Some(b"before".to_vec())), (retired.clone(), Some(b"retired".to_vec()))]);
        let deletes = BTreeMap::from([(retired.clone(), b"retired".to_vec())]);
        let result = crate::project_sync::install_template_transaction(root, &writes, &snapshots, &BTreeSet::new(), &deletes, || {
            assert!(!retired.exists()); assert_eq!(fs::read(&target).unwrap(), b"after");
            if failure { Err("final verification failed".into()) } else { Ok(()) }
        });
        if failure {
            assert!(result.unwrap_err().contains("rolled back"));
            assert_eq!(fs::read(&target).unwrap(), b"before"); assert_eq!(fs::read(&retired).unwrap(), b"retired");
        } else { result.unwrap(); assert!(!retired.exists()); }
    }
}

#[test]
fn template_retirement_preserves_external_write_or_recreated_retirement() {
    for recreate in [false, true] {
        let fixture = Fixture::new("external"); let root = &fixture.0;
        let target = root.join("config.toml"); let retired = root.join("old.json");
        fs::write(&target,b"before").unwrap(); fs::write(&retired,b"retired").unwrap();
        let writes = BTreeMap::from([(target.clone(), b"after".to_vec())]);
        let snapshots = BTreeMap::from([(target.clone(), Some(b"before".to_vec())), (retired.clone(), Some(b"retired".to_vec()))]);
        let deletes = BTreeMap::from([(retired.clone(), b"retired".to_vec())]);
        let error = crate::project_sync::install_template_transaction(root, &writes, &snapshots, &BTreeSet::new(), &deletes, || {
            fs::write(if recreate { &retired } else { &target }, b"external").unwrap(); Ok(())
        }).unwrap_err();
        assert!(error.contains("external change preserved"), "{error}");
        assert_eq!(fs::read(if recreate { &retired } else { &target }).unwrap(), b"external");
        assert_eq!(fs::read(if recreate { &target } else { &retired }).unwrap(), if recreate { b"before".as_slice() } else { b"retired".as_slice() });
    }
}

#[test]
fn template_retirement_rejects_drift_before_any_writes() {
    let fixture = Fixture::new("drift"); let root = &fixture.0;
    let target = root.join("config.toml"); let retired = root.join("old.json");
    fs::write(&target,b"before").unwrap(); fs::write(&retired,b"external").unwrap();
    let writes = BTreeMap::from([(target.clone(), b"after".to_vec())]);
    let snapshots = BTreeMap::from([(target.clone(), Some(b"before".to_vec())), (retired.clone(), Some(b"retired".to_vec()))]);
    let deletes = BTreeMap::from([(retired.clone(), b"retired".to_vec())]);
    assert!(crate::project_sync::install_template_transaction(root, &writes, &snapshots, &BTreeSet::new(), &deletes, || panic!("must not verify")).is_err());
    assert_eq!(fs::read(target).unwrap(),b"before"); assert_eq!(fs::read(retired).unwrap(),b"external");
}

fn install_template(root: &Path) {
    for asset in template_standard::assets() {
        let bytes = bridgeforge_skeleton::content::render(asset.rule["id"].as_str().unwrap(), &asset.payload,
            None, &asset.rule, root, true).unwrap();
        let target = root.join(asset.rule["target"].as_str().unwrap());
        fs::create_dir_all(target.parent().unwrap()).unwrap(); fs::write(target,bytes).unwrap();
    }
}

#[test]
fn template_baseline_ignores_old_policy_contents_but_blocks_any_extra_json() {
    let fixture = Fixture::new("baseline"); let root = &fixture.0;
    install_template(root);
    bridgeforge_skeleton::template::verify(root,false).unwrap();
    let extra = root.join(".codex/hooks/source/nested/unknown.JSON");
    fs::create_dir_all(extra.parent().unwrap()).unwrap(); fs::write(&extra,b"invalid JSON never parsed").unwrap();
    assert!(bridgeforge_skeleton::template::verify(root,false).unwrap_err().contains("unknown.JSON"));
    fs::remove_file(extra).unwrap();
    fs::write(root.join(".codex/bridgeforge-version.json"),b"not a policy").unwrap();
    assert!(bridgeforge_skeleton::template::verify(root,false).unwrap_err().contains("bridgeforge-version.json"));
    fs::remove_file(root.join(".codex/bridgeforge-version.json")).unwrap();
    bridgeforge_skeleton::template::verify(root,false).unwrap();
    assert!(!root.join(".codex/managed-skeleton.json").exists());
}

#[test]
fn template_index_checks_staged_tree_without_mutating_real_or_fixture_index() {
    let fixture = Fixture::new("index"); let root = &fixture.0; install_template(root);
    let git = |args: &[&str]| { let output = std::process::Command::new("git").current_dir(root).args(args).output().unwrap();
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr)); };
    git(&["init", "--quiet"]); git(&["add", "."]);
    let before = fs::read(root.join(".git/index")).unwrap();
    bridgeforge_skeleton::template::verify_index(root,&bridgeforge_core::SystemProcessRunner).unwrap();
    assert_eq!(fs::read(root.join(".git/index")).unwrap(),before);
    fs::write(root.join(".codex/managed-skeleton.json"),b"{invalid").unwrap(); git(&["add", ".codex/managed-skeleton.json"]);
    let before = fs::read(root.join(".git/index")).unwrap();
    let error = bridgeforge_skeleton::template::verify_index(root,&bridgeforge_core::SystemProcessRunner).unwrap_err();
    assert!(error.contains("staged Codex JSON")); assert_eq!(fs::read(root.join(".git/index")).unwrap(),before);
}
