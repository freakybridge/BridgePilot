use super::*;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/manifest-migration/core")
            .join(format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(root.join("source/forge")).unwrap();
        Self(root)
    }
    fn product(&self, path: &str, marker: &str) {
        let value = json!({"schema_version":1,"user_instructions":{"marker":marker},"shared_skills":{},"project_skeleton":{}});
        fs::write(self.0.join(path), serde_json::to_vec(&value).unwrap()).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

#[test]
fn nested_manifest_is_resolved_from_repository_root_without_legacy_fallback() {
    let f = Fixture::new();
    f.product(LEGACY_FILE, "legacy");
    assert!(load(&f.0).unwrap_err().contains("legacy root manifest"));
    f.product(FILE, "current");
    assert_eq!(section(&f.0, "user_instructions").unwrap()["marker"], "current");
    assert!(load(&f.0.join("source/forge")).is_err());
    fs::write(f.0.join(FILE), b"invalid").unwrap();
    assert!(load(&f.0).is_err(), "corrupt current manifest must not fall back");
    assert_eq!(fs::read(f.0.join(FILE)).unwrap(), b"invalid");
}

#[test]
fn source_checkout_guard_preserves_both_layouts_without_loading_the_manifest() {
    let f = Fixture::new();
    assert!(!is_source_checkout(&f.0));
    f.product(LEGACY_FILE, "legacy");
    assert!(is_source_checkout(&f.0));
    fs::remove_file(f.0.join(LEGACY_FILE)).unwrap();
    fs::write(f.0.join(FILE), b"broken but still a source checkout").unwrap();
    assert!(is_source_checkout(&f.0));
}
