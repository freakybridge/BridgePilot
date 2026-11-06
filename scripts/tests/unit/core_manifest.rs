#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

use super::*;

#[test]
fn consumer_lock_projection_remains_stable_across_repeated_rendering() {
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "consumer-lock-projection")
        .join(format!("{}", std::process::id()));
    fs::create_dir_all(root.join("scripts/tests")).unwrap();
    fs::write(root.join("Cargo.lock"), "version=4\n[[package]]\nname='a-unused'\nversion='1.0.0'\n[[package]]\nname='b-unused'\nversion='1.0.0'\n[[package]]\nname='bridgeforge-core'\nversion='2.0.0'\n[[package]]\nname='z-unused'\nversion='1.0.0'\n").unwrap();
    fs::write(root.join("scripts/tests/Cargo.toml"), "[package]\nname='bridgeforge-factory-tests'\nversion='6.0.0'\n").unwrap();
    let target = root.join("scripts/tests/Cargo.lock");
    fs::write(&target, "version=4\n[[package]]\nname='a-unused'\nversion='1.0.0'\n[[package]]\nname='bridgeforge-core'\nversion='1.0.0'\n[[package]]\nname='bridgeforge-factory-tests'\nversion='6.0.0'\ndependencies=['bridgeforge-core']\n").unwrap();
    let first = consumer_lock(&root, &BTreeMap::new()).unwrap();
    for _ in 0..2 {
        fs::write(&target, &first).unwrap();
        assert_eq!(consumer_lock(&root, &BTreeMap::new()).unwrap(), first);
    }
    let lock: toml_edit::DocumentMut = std::str::from_utf8(&first).unwrap().parse().unwrap();
    let packages = lock["package"].as_array_of_tables().unwrap();
    assert_eq!(packages.len(), 2, "unrelated native packages must stay outside the consumer closure");
    assert_eq!(packages.iter().find(|p| p["name"].as_str()==Some("bridgeforge-core")).unwrap()["version"].as_str(), Some("2.0.0"));
    assert_eq!(packages.iter().find(|p| p["name"].as_str()==Some("bridgeforge-factory-tests")).unwrap()["dependencies"].as_array().unwrap().get(0).unwrap().as_str(), Some("bridgeforge-core"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn factory_project_config_is_generated_from_template_without_writes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors()
        .find(|path| path.join("source/forge/bridgeforge-manifest.json").is_file()).unwrap();
    let target = root.join(".codex/config.toml");
    let before = fs::read(&target).unwrap();
    let expected = fs::read(root.join("source/forge/templates/config.toml")).unwrap();
    let writes = render_targets(root).unwrap();
    assert_eq!(writes.iter().find(|(path, _)| path == &target).map(|(_, bytes)| bytes), Some(&expected));
    assert_eq!(fs::read(&target).unwrap(), before);
}

#[test]
fn unified_manifest_emits_one_nested_source_without_retired_sidecars() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().find(|p| p.join("source/forge/bridgeforge-manifest.json").is_file()).expect("factory repository");
    let before = fs::read(root.join(bridgeforge_core::product_manifest::FILE)).unwrap();
    let writes = render_targets(root).unwrap();
    assert_eq!(writes.iter().filter(|(p, _)| p == &root.join(bridgeforge_core::product_manifest::FILE)).count(), 1);
    for old in ["bridgeforge-manifest.json", "user-agents-manifest.json", "bridgeforge-codex-manifest.json", "templates/managed-skeleton.json", ".codex/managed-skeleton.json"] {
        assert!(!writes.iter().any(|(p, _)| p == &root.join(old)));
    }
    let (_, bytes) = writes.iter().find(|(p, _)| p == &root.join(bridgeforge_core::product_manifest::FILE)).unwrap();
    let combined = bridgeforge_core::product_manifest::parse(bytes).unwrap();
    assert_eq!(combined["project_skeleton"], serde_json::from_slice::<Value>(&render_managed_contract(root).unwrap()).unwrap());
    assert_eq!(combined["user_instructions"], serde_json::from_slice::<Value>(&crate::user_agents::render_manifest(root).unwrap()).unwrap());
    assert_eq!(combined["schema_version"], 2);
    assert!(combined.get("shared_skills").is_none());
    assert_eq!(fs::read(root.join(bridgeforge_core::product_manifest::FILE)).unwrap(), before);
}

#[test]
fn manifest_projection_rejects_out_of_scope_retirement_before_io() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().find(|p| p.join("source/forge/bridgeforge-manifest.json").is_file()).expect("factory repository");
    let writes = render_targets(root).unwrap();
    let contract_path = root.join("source/forge/bridgeforge-manifest.json");
    let bytes = &writes.iter().find(|(p, _)| p == &contract_path).unwrap().1;
    let original = fs::read(root.join(crate::source_projection::physical_path(root, "source/memory/src/lib.rs"))).unwrap();
    for target in [
        crate::source_projection::physical_path(root, "source/memory/src/lib.rs"),
        ".codex/hooks/project_custom/lib.rs".to_owned(),
    ] {
        let mut contract: Value = serde_json::from_slice(bytes).unwrap();
        contract["project_skeleton"]["retired_assets"] =
            json!([{"id":"retired.invalid","target":target,"known_hashes":[sha_git(&original)]}]);
        let invalid = vec![(
            contract_path.clone(),
            serde_json::to_vec(&contract).unwrap(),
        )];
        let error = projection_targets_from_writes(root, invalid).unwrap_err();
        assert!(error.contains("invalid retired source asset"), "{error}");
        assert_eq!(
            fs::read(root.join(crate::source_projection::physical_path(root, "source/memory/src/lib.rs"))).unwrap(),
            original
        );
    }
}

#[test]
fn manifest_retirement_rolls_back_and_preserves_external_recreation() {
    let token = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "manifest")
        .join(format!("bf-manifest-retirement-{token}"));
    fs::create_dir_all(&root).unwrap();
    let retired = root.join("retired.rs");
    for external in [false, true] {
        fs::write(&retired, b"verified-old").unwrap();
        let targets = vec![(retired.clone(), None)];
        let before = BTreeMap::from([(retired.clone(), Some(b"verified-old".to_vec()))]);
        let error = install_targets(&targets, &before, &|index| {
            if index == 1 {
                assert!(!retired.exists());
                if external {
                    fs::write(&retired, b"external").unwrap();
                }
                return Err("injected retirement failure".into());
            }
            Ok(())
        })
        .unwrap_err();
        assert!(error.contains("injected retirement failure"));
        assert_eq!(
            fs::read(&retired).unwrap(),
            if external {
                b"external".as_slice()
            } else {
                b"verified-old".as_slice()
            }
        );
    }
    fs::write(&retired, b"verified-old").unwrap();
    install_targets(
        &[(retired.clone(), None)],
        &BTreeMap::from([(retired.clone(), Some(b"verified-old".to_vec()))]),
        &|_| Ok(()),
    )
    .unwrap();
    assert!(!retired.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hook_asset_ids_are_stable() {
    assert_eq!(
        hook_asset_id("crates/core/src/lib.rs"),
        "codex.hooks.crates-core-src-lib-rs"
    );
}

#[test]
fn canonical_hash_ignores_object_insertion_order() {
    assert_eq!(
        canonical_sha(&json!({"b": 1, "a": 2})).unwrap(),
        canonical_sha(&json!({"a": 2, "b": 1})).unwrap()
    );
}

#[test]
fn factory_and_consumer_workflows_share_explicit_assets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().find(|p| p.join("source/forge/bridgeforge-manifest.json").is_file()).expect("factory repository");
    let public: Value = serde_json::from_slice(&render_managed_contract(root).unwrap()).unwrap();
    let outputs = factory_entrypoints(root).unwrap();
    let mut count = 0;
    for asset in public["assets"].as_array().unwrap() {
        let target = asset["target"].as_str().unwrap();
        if !target.starts_with(".agents/skills/") && !target.starts_with(".codex/maintenance/") { continue; }
        if target.ends_with("/SKILL.md") { count += 1; }
        let source = asset["source"].as_str().unwrap();
        assert_eq!(outputs[target], fs::read(root.join(source)).unwrap());
        assert_eq!(outputs[target], fs::read(root.join(target)).unwrap());
    }
    assert_eq!(count, 19);
    assert!(outputs.contains_key(".agents/skills/git-sync/SKILL.md"));
    assert!(outputs.contains_key(".codex/maintenance/bridgeforge-codex/scripts/prepare-project.ps1"));
}

#[test]
fn asset_history_preserves_asset_ids_and_public_boundary_without_writes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().find(|p| p.join("source/forge/bridgeforge-manifest.json").is_file()).expect("factory repository");
    let before = fixture_product::read(root.join("source/forge/bridgeforge-manifest.json")).unwrap();
    let rendered = render_managed_contract(root).unwrap();
    let contract: Value = serde_json::from_slice(&rendered).unwrap();
    crate::baseline::validate_contract(&contract).unwrap();
    for target in [
        ".codex/hooks/project_custom/entrypoint.rs",
        ".codex/hooks/Project_custom/entrypoint.rs",
    ] {
        let mut invalid = contract.clone();
        invalid["retired_assets"][0]["target"] = json!(target);
        assert!(
            crate::baseline::validate_contract(&invalid)
                .unwrap_err()
                .contains("invalid retired source asset")
        );
    }
    let layout = &contract["asset_history"];
    assert_eq!(
        contract["asset_history"],
        serde_json::from_slice::<Value>(&before).unwrap()["asset_history"]
    );
    assert_eq!(
        render_contract_value(root, &BTreeMap::new(), false, contract.clone()).unwrap(),
        rendered
    );
    let assets = contract["assets"].as_array().unwrap();
    for row in layout["relocations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r.get("source").is_some())
    {
        let item = assets
            .iter()
            .find(|a| a["id"] == row["id"] && a["target"] == row["target"]);
        if let Some(item) = item {
            assert_eq!(item["id"], row["id"]);
            assert_eq!(item["target"], row["target"]);
        } else {
            // A preserved relocation may outlive its source (e.g. retired JSON).
            let source = crate::source_projection::physical_path(root, row["source"].as_str().unwrap());
            assert!(!root.join(source).exists(), "existing source lost its active asset: {row}");
            let retired = contract["retired_assets"].as_array().unwrap().iter()
                .find(|a| a["id"] == row["id"] && a["target"] == row["previous_target"])
                .expect("historical asset must retain its retirement record");
            for known in row["known_hashes"].as_array().unwrap() {
                assert!(retired["known_hashes"].as_array().unwrap().contains(&known["sha256"]));
            }
        }
    }
    assert!(assets.iter().all(|a| {
        !a["source"]
            .as_str()
            .unwrap_or("")
            .starts_with("source/factory/")
    }));
    assert_eq!(contract["generated_assets"].as_array().unwrap().len(), 2);
    assert_eq!(
        fixture_product::read(root.join("source/forge/bridgeforge-manifest.json")).unwrap(),
        before
    );
}

#[test]
fn consumer_lock_follows_candidate_native_versions_and_dependency_closure() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().find(|p| p.join("source/forge/bridgeforge-manifest.json").is_file()).expect("factory repository");
    let original = fs::read(root.join("Cargo.lock")).unwrap();
    let mut lock = String::from_utf8(original)
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    for entry in lock["package"].as_array_of_tables_mut().unwrap().iter_mut() {
        if entry["name"].as_str() == Some("bridgeforge-core") {
            entry["version"] = toml_edit::value("7.0.0");
        }
    }
    let candidate = BTreeMap::from([("Cargo.lock".into(), lock.to_string().into_bytes())]);
    let consumer = consumer_lock(root, &candidate).unwrap();
    let document = String::from_utf8(consumer)
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    let entries = document["package"].as_array_of_tables().unwrap();
    assert_eq!(
        entries
            .iter()
            .find(|e| e["name"].as_str() == Some("bridgeforge-core"))
            .unwrap()["version"]
            .as_str(),
        Some("7.0.0")
    );
    assert!(
        entries
            .iter()
            .any(|e| e["name"].as_str() == Some("bridgeforge-factory-tests"))
    );
    assert!(
        !entries
            .iter()
            .any(|e| e["name"].as_str() == Some("bridgeforge-cli"))
    );
}

#[test]
fn manifest_install_failure_rolls_back_own_writes_and_preserves_external_edits() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "manifest").join(format!(
        "bf-manifest-tx-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let (a, b, c) = (root.join("a"), root.join("b"), root.join("c"));
    let targets = vec![
        (a.clone(), Some(b"new-a".to_vec())),
        (b.clone(), Some(b"new-b".to_vec())),
        (c.clone(), Some(b"new-c".to_vec())),
    ];
    for change_installed in [false, true] {
        fs::write(&a, b"old-a").unwrap();
        fs::write(&b, b"old-b").unwrap();
        let before = BTreeMap::from([
            (a.clone(), Some(b"old-a".to_vec())),
            (b.clone(), Some(b"old-b".to_vec())),
            (c.clone(), None),
        ]);
        let error = install_targets(&targets, &before, &|index| {
            if index == 1 {
                fs::write(if change_installed { &a } else { &b }, b"external").unwrap();
                return Err("injected source drift".into());
            }
            Ok(())
        })
        .unwrap_err();
        assert!(error.contains("injected source drift"), "{error}");
        assert_eq!(
            fs::read(&a).unwrap(),
            if change_installed {
                b"external".as_slice()
            } else {
                b"old-a".as_slice()
            }
        );
        assert_eq!(
            fs::read(&b).unwrap(),
            if change_installed {
                b"old-b".as_slice()
            } else {
                b"external".as_slice()
            }
        );
        assert!(!c.exists());
    }
    fs::write(&a, b"old-a").unwrap();
    fs::write(&b, b"old-b").unwrap();
    let before = BTreeMap::from([
        (a.clone(), Some(b"old-a".to_vec())),
        (b.clone(), Some(b"old-b".to_vec())),
        (c.clone(), None),
    ]);
    let error = install_targets(&targets, &before, &|index| {
        if index == 2 {
            fs::remove_file(&b).unwrap();
            fs::create_dir(&b).unwrap();
            return Err("injected external directory".into());
        }
        Ok(())
    })
    .unwrap_err();
    assert!(
        error.contains("external rollback state preserved"),
        "{error}"
    );
    assert_eq!(
        fs::read(&a).unwrap(),
        b"old-a",
        "one unreadable target must not abort remaining rollback"
    );
    assert!(b.is_dir());
    assert!(!c.exists());
    fs::remove_dir_all(root).unwrap();
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
