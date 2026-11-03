use bridgeforge_core::{build_manifest, product_version, source_projection};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

fn repository() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap() }

#[test]
fn bp_and_orchestrator_candidates_do_not_change_forge_projections() {
    let root = repository();
    let overlay = BTreeMap::from([
        ("VERSION".into(), b"9.9.9\n".to_vec()),
        ("source/orchestrator/cli/Cargo.toml".into(), b"unrelated orchestrator candidate".to_vec()),
    ]);
    for public in [true, false] {
        assert_eq!(source_projection::render(&root, public).unwrap(), source_projection::render_with_overlay(&root, public, &overlay).unwrap());
    }
    assert_eq!(product_version::forge(&root).unwrap(), product_version::explicit(&fs::read_to_string(root.join("source/forge/cli/Cargo.toml")).unwrap()).unwrap());
    assert_ne!(product_version::forge(&root).unwrap(), fs::read_to_string(root.join("VERSION")).unwrap().trim());
}

#[test]
fn verifier_accepts_each_official_projection_and_rejects_mixed_fingerprints() {
    let root = repository();
    let identity = |public| {
        let source = source_projection::render(&root, public).unwrap();
        json!({"protocol":1,"source_tree_sha256":source_projection::tree_sha(&source).unwrap(),
            "lockfile_sha256":build_manifest::normalized_hash(&source["Cargo.lock"])})
    };
    let public = identity(true);
    let factory = identity(false);
    assert_ne!(public, factory);
    bridgeforge_skeleton::template::verify_factory_identity(&root, &public).unwrap();
    bridgeforge_skeleton::template::verify_factory_identity(&root, &factory).unwrap();
    let mut mixed = factory.clone();
    mixed["lockfile_sha256"] = public["lockfile_sha256"].clone();
    assert!(bridgeforge_skeleton::template::verify_factory_identity(&root, &mixed).is_err());
    mixed["source_tree_sha256"] = json!("sha256:invalid");
    assert!(bridgeforge_skeleton::template::verify_factory_identity(&root, &mixed).is_err());
}

#[test]
fn native_forge_version_is_strict_and_malformed_values_are_rejected() {
    assert_eq!(product_version::explicit("[package]\nversion='17.0.0'\n").unwrap(), "17.0.0");
    for text in ["[package]\nversion.workspace=true\n", "[package]\nversion='oops'\n", "[package]\n"] {
        assert!(product_version::explicit(text).is_err());
    }
}
