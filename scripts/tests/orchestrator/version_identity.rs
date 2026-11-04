use super::*;
use crate::versions::{Component, Components};

fn components(forge: &str, orchestrator: &str) -> Components {
    Components {
        forge: Component { version: forge.into(), delivery: "source-reference".into() },
        orchestrator: Component { version: orchestrator.into(), delivery: "embedded".into() },
    }
}
fn version(bp: &str, components: Option<Components>, hash: &str) -> Version {
    Version { version: bp.into(), components, executable: format!(".build/bridgepilot/versions/{bp}-{hash}/bp-core.exe"), sha256: hash.into() }
}
fn installed(current: Version, previous: Option<Version>) -> Installation {
    Installation { schema: if current.components.is_some() { 2 } else { 1 }, product: "BridgePilot".into(), current, previous, files: vec![], launcher_sha256: "launcher".into() }
}

#[test]
fn namespace_is_explicit_and_component_delivery_is_not_an_installed_forge_claim() {
    let c = components("17.0.0", "0.14.0");
    assert!(validate_product_identity(2, "1.0.0", Some(&c)).is_ok());
    assert!(validate_product_identity(1, "0.13.0", None).is_ok());
    assert!(validate_product_identity(1, "1.0.0", None).is_err());
    assert!(validate_product_identity(2, "1.0.0", None).is_err());
    assert!(validate_product_identity(2, "0.14.0", Some(&c)).is_err());
    let mut fake = c.clone();
    fake.forge.delivery = "embedded".into();
    assert!(validate_product_identity(2, "1.0.0", Some(&fake)).is_err());
}

#[test]
fn legacy_upgrade_compares_orchestrator_instead_of_bp_namespace() {
    let legacy = installed(version("0.13.0", None, "old"), None);
    assert!(validate_transition(&legacy, &version("1.0.0", Some(components("17.0.0", "0.14.0")), "new"), false).is_ok());
    assert!(validate_transition(&legacy, &version("1.0.0", Some(components("17.0.0", "0.12.0")), "new"), false).is_err());
}

#[test]
fn modern_upgrade_rejects_reused_bp_version_and_implicit_component_downgrade() {
    let current = version("1.0.0", Some(components("17.0.0", "0.14.0")), "a");
    let old = installed(current.clone(), None);
    assert!(validate_transition(&old, &current, false).is_ok());
    assert!(validate_transition(&old, &version("1.0.0", current.components.clone(), "other"), false).is_err());
    assert!(validate_transition(&old, &version("1.0.0", Some(components("17.1.0", "0.14.0")), "a"), false).is_err());
    assert!(validate_transition(&old, &version("1.0.1", Some(components("17.0.0", "0.13.0")), "b"), false).is_err());
    assert!(validate_transition(&old, &version("1.0.1", Some(components("16.1.0", "0.14.0")), "b"), false).is_err());
    assert!(validate_transition(&old, &version("1.0.1", Some(components("17.0.0", "0.14.0")), "b"), false).is_ok());
}

#[test]
fn rollback_requires_exact_retained_identity_and_cannot_activate_legacy_reader() {
    let prior = version("1.0.0", Some(components("17.0.0", "0.14.0")), "a");
    let current = version("1.1.0", Some(components("17.1.0", "0.15.0")), "b");
    let old = installed(current, Some(prior.clone()));
    assert!(validate_transition(&old, &prior, false).is_err());
    assert!(validate_transition(&old, &prior, true).is_ok());
    let mut tampered = prior;
    tampered.sha256 = "different".into();
    assert!(validate_transition(&old, &tampered, true).is_err());
    let legacy = version("0.13.0", None, "legacy");
    let modern = installed(old.current, Some(legacy.clone()));
    assert!(validate_transition(&modern, &legacy, true).is_err());
}

#[test]
fn legacy_json_roundtrips_without_new_fields_and_modern_json_keeps_components() {
    let legacy = installed(version("0.13.0", None, "a"), None);
    let bytes = serde_json::to_vec(&legacy).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("components"));
    assert_eq!(serde_json::from_slice::<Installation>(&bytes).unwrap(), legacy);
    let modern = installed(version("1.0.0", Some(components("17.0.0", "0.14.0")), "b"), Some(legacy.current));
    assert_eq!(serde_json::from_slice::<Installation>(&serde_json::to_vec(&modern).unwrap()).unwrap(), modern);
}

#[test]
fn launcher_and_claim_recovery_preserves_external_edits_then_restores_owned_bytes() {
    let home = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.runtime/tests/version-governance").join(crate::new_id("launcher-recovery"));
    let targets = [".build/bridgepilot/bin/bp.exe", ".runtime/config/bridgepilot-home.json"];
    let mut changes = vec![];
    for target in targets {
        let path = home.join(target);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"old owned bytes").unwrap();
        changes.push(Change { target: target.into(), old: Some(b"old owned bytes".to_vec()), new_hash: digest(b"new owned bytes") });
    }
    let next = installed(version("1.0.0", Some(components("17.0.0", "0.14.0")), "core"), None);
    let transaction = Journal { schema:1, old:None, new:next, changes, retirements: Default::default() };
    persistence::atomic_write_json(&journal(&home), &transaction).unwrap();
    apply_change(&home, &transaction.changes[0], b"new owned bytes").unwrap();
    fs::write(home.join(targets[1]), b"external edit").unwrap();
    assert!(apply_change(&home, &transaction.changes[1], b"new owned bytes").is_err());
    assert_eq!(fs::read(home.join(targets[1])).unwrap(), b"external edit");
    assert!(recover(&home).is_err());
    assert!(journal(&home).exists());
    fs::write(home.join(targets[1]), b"old owned bytes").unwrap();
    assert_eq!(recover(&home).unwrap()["status"], "installation_rolled_back");
    for target in targets { assert_eq!(fs::read(home.join(target)).unwrap(), b"old owned bytes"); }
    fs::remove_dir_all(&home).unwrap();
}

#[test]
fn rebuild_reuse_rechecks_program_assets_identity_and_pending_under_the_lock() {
    let id = crate::new_id("rebuild-snapshot");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let root = repo.join(".build/bridgepilot/equivalence-fixtures").join(&id);
    let home = repo.join(".runtime/tests/bp-rebuild-snapshot").join(&id);
    let mut p = Package { schema:2, product:"BridgePilot".into(), version:"2.2.0".into(),
        components:Some(components("18.0.0", "0.18.0")), launcher_protocol:2, assets:Vec::new() };
    let mut old = installed(version(&p.version, p.components.clone(), &digest(b"core")), None);
    old.launcher_sha256 = digest(b"launcher");
    for (index, (id, source, target, _)) in assets().into_iter().enumerate() {
        let body: &[u8] = match index { 0 => b"core", 1 => b"launcher", _ => id.as_bytes() };
        persistence::atomic_write(&root.join(source), body).unwrap();
        p.assets.push(Asset { id:id.into(), source:source.into(), target:target.into(), sha256:digest(body) });
        let target = match index { 0 => old.current.executable.as_str(), 1 => ".build/bridgepilot/bin/bp.exe", _ => target };
        persistence::atomic_write(&home.join(target), body).unwrap();
        if index >= 2 { old.files.push(OwnedFile { id:id.into(), target:target.into(), sha256:digest(body) }); }
    }
    persistence::atomic_write_json(&config(&home), &old).unwrap();
    assert!(recheck_reuse(&root, &home, &p, &old, None, &None).is_ok());
    // These are the observations made again after acquiring the real writer
    // lock. Every drift must fail without repairing or overwriting the files.
    for target in [old.current.executable.as_str(), ".build/bridgepilot/bin/bp.exe", old.files[0].target.as_str()] {
        let path = home.join(target);
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"external edit").unwrap();
        assert!(recheck_reuse(&root, &home, &p, &old, None, &None).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external edit");
        fs::write(path, original).unwrap();
    }
    for relative in [".runtime/bridgepilot-install/journal.json", ".runtime/bridgepilot-skill/journal.json", ".runtime/config/bridgepilot-user-skill.json"] {
        let path = home.join(relative);
        persistence::atomic_write(&path, b"concurrent transaction").unwrap();
        assert!(recheck_reuse(&root, &home, &p, &old, None, &None).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"concurrent transaction");
        fs::remove_file(path).unwrap();
    }
    let path = root.join(&p.assets[0].source);
    fs::write(&path, b"candidate changed").unwrap();
    assert!(recheck_reuse(&root, &home, &p, &old, None, &None).is_err());
    assert_eq!(fs::read(path).unwrap(), b"candidate changed");
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(home).unwrap();
}
