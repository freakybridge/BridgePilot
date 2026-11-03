use super::*;

#[test]
fn release_prepare_rejects_traversal_and_nonbuild_targets_before_writes() {
    let base = std::path::absolute(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/native-release/path-check")
            .join(crate::new_id("case")),
    )
    .unwrap();
    fs::create_dir_all(base.join(".build")).unwrap();
    for path in [
        base.join(".build/../source/escaped"),
        base.join("source/out"),
        base.join(".build/sub/../../escaped"),
    ] {
        assert!(prepare(&base, &path).is_err());
        assert!(!base.join("source").exists());
        assert!(!base.join("escaped").exists());
        assert!(fs::read_dir(base.join(".build")).unwrap().next().is_none());
    }
    fs::remove_dir_all(base).unwrap();
}
