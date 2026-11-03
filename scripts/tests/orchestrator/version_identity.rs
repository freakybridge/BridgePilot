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
    let transaction = Journal { schema:1, old:None, new:next, changes };
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
