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
fn project_workflow_ownership_distinguishes_current_history_and_conflicts() {
    let fixture = Fixture::new("workflow-ownership");
    let root = &fixture.0;
    let target = ".agents/skills/develop/SKILL.md";
    let workflow = template_standard::assets().into_iter().find(|asset| asset.rule["target"] == target).unwrap();
    let mut rule = workflow.rule;
    rule["previous_sha256"] = json!([build_manifest::normalized_hash(b"previous\n")]);
    assert!(verify_workflow_asset(target, &rule, None).is_ok());
    assert!(verify_workflow_asset(target, &rule, Some(&workflow.payload)).is_ok());
    assert!(verify_workflow_asset(target, &rule, Some(b"previous\r\n")).is_ok());
    assert!(verify_workflow_asset(target, &rule, Some(b"local edits")).unwrap_err().contains("preserved"));

    assert!(verify_workflow_root(root, target).is_ok());
    let references = root.join(".agents/skills/develop/references");
    fs::create_dir_all(&references).unwrap();
    fs::write(references.join("private.md"), b"local").unwrap();
    assert!(verify_workflow_root(root, target).unwrap_err().contains("unowned nonempty"));
    fs::write(root.join(target), workflow.payload).unwrap();
    assert!(verify_workflow_root(root, target).is_ok());
    let legacy = root.join(".codex/skills/develop/SKILL.md");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, b"legacy private").unwrap();
    assert!(verify_workflow_root(root, target).unwrap_err().contains("same-name"));
    assert_eq!(fs::read(&legacy).unwrap(), b"legacy private");
}

#[test]
fn cached_plan_revalidation_still_checks_new_transaction_guards_first() {
    for relative in [".runtime/bridgeforge-codex/develop-transaction.json",".runtime/bridgeforge-codex/release-preparation/commit-guard.json"] {
        let fixture=Fixture::new("new-guard");let root=&fixture.0;
        let path=root.join(relative);fs::create_dir_all(path.parent().unwrap()).unwrap();fs::write(&path,b"{}").unwrap();
        let plan=Plan{root:root.clone(),product:root.join("missing-product"),product_bytes:vec![],before:BTreeMap::new(),writes:BTreeMap::new(),retirements:BTreeMap::new(),
            fingerprint:String::new(),report:json!({}),release_recipe:None,prepared:root.join("missing-ready"),binaries:BTreeSet::new(),source_inputs:String::new(),identity_recovery:None};
        assert!(revalidate(&plan).unwrap_err().contains("unfinished"));assert_eq!(fs::read(path).unwrap(),b"{}");
    }
}

#[test]
fn identity_recovery_installation_serializes_with_native_release() {
    let fixture=Fixture::new("identity-release-lock"); let root=&fixture.0;
    let _lock=bridgeforge_core::file_lock::FileLock::acquire(&root.join(".runtime/bridgeforge-codex/native-release/operation.lock")).unwrap();
    let plan=Plan{root:root.clone(),product:root.join("missing-product"),product_bytes:vec![],before:BTreeMap::new(),writes:BTreeMap::new(),retirements:BTreeMap::new(),
        fingerprint:"reviewed".into(),report:json!({"gaps":[],"risk":[]}),release_recipe:None,prepared:root.join("missing-ready"),binaries:BTreeSet::new(),source_inputs:String::new(),identity_recovery:Some(json!({}))};
    assert!(apply(plan,"reviewed",false,&root.join("missing-ready")).unwrap_err().contains("lock"));
}

#[test]
fn cached_plan_rejects_new_factory_identity_and_rolls_back_same_repository_target_drift() {
    let fixture=Fixture::new("factory-and-target-drift");let root=&fixture.0;
    let mut plan=Plan{root:root.clone(),product:root.join("missing-product"),product_bytes:vec![],before:BTreeMap::new(),writes:BTreeMap::new(),retirements:BTreeMap::new(),
        fingerprint:String::new(),report:json!({}),release_recipe:None,prepared:root.join("missing-ready"),binaries:BTreeSet::new(),source_inputs:String::new(),identity_recovery:None};
    fs::create_dir_all(root.join("source/forge/factory")).unwrap();fs::write(root.join("source/forge/factory/Cargo.toml"),b"marker").unwrap();
    assert!(revalidate(&plan).unwrap_err().contains("factory dogfood"));
    let target=json!({"url":"https://github.com/fixture/project.git","branch":"refs/heads/main","push_ref":"refs/remotes/origin/main","head":"old","config_sha256":"old"});
    plan.report=json!({"release_installation":{"target":target}});
    assert!(verify_target(&plan,&target).is_ok());
    let path=root.join("owned.txt");fs::write(&path,b"before").unwrap();
    let writes=BTreeMap::from([(path.clone(),b"after".to_vec())]);let before=BTreeMap::from([(path.clone(),Some(b"before".to_vec()))]);
    for field in ["branch","push_ref","head","config_sha256"] {
        let mut changed=target.clone();changed[field]=json!("changed");
        let result=crate::project_sync::install_template_transaction(root,&writes,&before,&BTreeSet::new(),&BTreeMap::new(),||verify_target(&plan,&changed));
        assert!(result.unwrap_err().contains("rolled back"));assert_eq!(fs::read(&path).unwrap(),b"before");
    }
}

#[test]
fn cleanup_without_product_review_stops_before_any_process() {
    struct Never;
    impl bridgeforge_core::ProcessRunner for Never {
        fn run(&self, _: &bridgeforge_core::ProcessRequest) -> std::io::Result<bridgeforge_core::ProcessOutput> {
            panic!("unreviewed cleanup must not run processes");
        }
    }
    let out = crate::artifact_cleanup::command(&["cleanup".into(),"--root".into(),"missing".into()], &Never);
    assert_eq!(out.code, 2);
    assert!(out.receipt.unwrap()["failures"][0].as_str().unwrap().contains("products-reviewed"));
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
