use super::*;
use crate::new_id;

#[test]
fn manifest_migration_configures_new_source_root_without_accepting_legacy_layout() {
    let base = std::path::absolute(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../.runtime/tests/manifest-migration/configure").join(new_id("case"))).unwrap();
    let root = base.join("product");
    let archive = base.join("archive");
    fs::create_dir_all(root.join("source/forge")).unwrap();
    crate::store::Store::init(&archive).unwrap();
    fs::write(root.join(bridgeforge_core::product_manifest::LEGACY_FILE), b"legacy").unwrap();
    assert!(crate::configuration::configure(&root, &archive).is_err());
    assert!(!root.join(".runtime").exists());
    fs::remove_file(root.join(bridgeforge_core::product_manifest::LEGACY_FILE)).unwrap();
    fs::write(root.join(bridgeforge_core::product_manifest::FILE), b"current").unwrap();
    assert!(!root.join(bridgeforge_core::product_manifest::LEGACY_FILE).exists());
    let result = crate::configuration::configure(&root, &archive).unwrap();
    assert_eq!(result["status"], "configured");
    assert!(root.join(".runtime/config/bridgepilot.json").is_file());
    assert!(crate::configuration::configure(&root.join("source/forge"), &archive).is_err());
    assert!(!root.join("source/forge/.runtime").exists());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn manifest_migration_prevents_installation_over_both_source_layouts() {
    for relative in [bridgeforge_core::product_manifest::FILE, bridgeforge_core::product_manifest::LEGACY_FILE] {
        let root = std::path::absolute(Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/manifest-migration/install").join(new_id("case"))).unwrap();
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"source marker").unwrap();
        let result = install_before_lock(&root.join("missing-package"), &root, None, false, false,
            || panic!("source protection must precede installation"));
        assert!(result.unwrap_err().contains("source/factory checkout"));
        assert_eq!(fs::read(path).unwrap(), b"source marker");
        assert!(!root.join(".runtime").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
