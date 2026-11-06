#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn unified_manifest_rejects_ambiguous_sections_before_staging() {
    let f = Fixture::new();
    let original: Value = serde_json::from_slice(&fs::read(f.0.join(MANIFEST)).unwrap()).unwrap();
    for name in ["user_instructions", "project_skeleton"] {
        let mut invalid = original.clone();
        invalid.as_object_mut().unwrap().remove(name);
        fs::write(f.0.join(MANIFEST), serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(f.run().is_err());
        assert!(!f.profile().join(".codex").exists());
    }
    let duplicate = br#"{"schema_version":1,"user_instructions":{},"shared_skills":{},"project_skeleton":{},"project_skeleton":{}}"#;
    assert!(crate::product_manifest::parse(duplicate).is_err());
    let mut extra = original;
    extra["unexpected"] = json!({});
    assert!(crate::product_manifest::parse(&serde_json::to_vec(&extra).unwrap()).is_err());
}

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "user-agents").join(format!(
            "bfc-user-agents-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("templates/user")).unwrap();
        fs::create_dir_all(root.join("profile")).unwrap();
        fs::write(root.join(SOURCE), "# 偏好\n内容\n").unwrap();
        fixture_product::write_section(root.join(MANIFEST), "user_instructions", render_manifest(&root).unwrap()).unwrap();
        Self(root)
    }
    fn profile(&self) -> PathBuf {
        self.0.join("profile")
    }
    fn target(&self) -> PathBuf {
        self.profile().join(".codex/AGENTS.md")
    }
    fn run(&self) -> Result<Value, String> {
        stage(&self.0, &self.profile(), "1234567890abcdef1234567890abcdef")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn missing_target_is_staged_without_installing() {
    let f = Fixture::new();
    let plan = f.run().unwrap();
    assert_eq!(plan["needs_swap"], true);
    assert_eq!(plan["had_original"], false);
    assert!(!f.target().exists());
    assert_eq!(
        fs::read(plan["stage"].as_str().unwrap()).unwrap(),
        fs::read(f.0.join(SOURCE)).unwrap()
    );
    assert!(!Path::new(plan["backup"].as_str().unwrap()).exists());
}

#[test]
fn equal_text_is_zero_write_even_with_crlf() {
    let f = Fixture::new();
    fs::create_dir_all(f.target().parent().unwrap()).unwrap();
    let original = "# 偏好\r\n内容\r\n";
    fs::write(f.target(), original).unwrap();
    let modified = fs::metadata(f.target()).unwrap().modified().unwrap();
    let plan = f.run().unwrap();
    assert_eq!(plan["needs_swap"], false);
    assert_eq!(fs::read(f.target()).unwrap(), original.as_bytes());
    assert_eq!(
        fs::metadata(f.target()).unwrap().modified().unwrap(),
        modified
    );
    assert!(!Path::new(plan["stage"].as_str().unwrap()).exists());
    assert!(!Path::new(plan["backup"].as_str().unwrap()).exists());
}

#[test]
fn edited_and_non_utf8_originals_get_exact_byte_witnesses() {
    for original in [b"local edit\r\n".as_slice(), &[0xff, 0, 0x12]] {
        let f = Fixture::new();
        fs::create_dir_all(f.target().parent().unwrap()).unwrap();
        fs::write(f.target(), original).unwrap();
        let plan = f.run().unwrap();
        assert_eq!(plan["needs_swap"], true);
        assert_eq!(plan["original_hash"], hash(original));
        assert_eq!(fs::read(f.target()).unwrap(), original);
    }
}

#[test]
fn invalid_source_contract_and_non_file_target_block_before_staging() {
    let f = Fixture::new();
    fs::write(f.0.join(SOURCE), "changed").unwrap();
    assert!(f.run().unwrap_err().contains("hash differs"));
    assert!(!f.profile().join(".codex").exists());
    fixture_product::write_section(f.0.join(MANIFEST), "user_instructions", render_manifest(&f.0).unwrap()).unwrap();
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(f.0.join(MANIFEST)).unwrap()).unwrap();
    manifest["user_instructions"]["target"] = json!("~/.codex/config.toml");
    fs::write(f.0.join(MANIFEST), serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(f.run().unwrap_err().contains("identity"));
    fs::create_dir_all(f.target()).unwrap();
    assert!(f.run().unwrap_err().contains("ordinary file"));
}

#[test]
fn existing_transaction_path_and_invalid_id_are_not_overwritten() {
    let f = Fixture::new();
    let plan = f.run().unwrap();
    assert!(f.run().unwrap_err().contains("already exist"));
    assert!(stage(&f.0, &f.profile(), "../escape").is_err());
    assert!(Path::new(plan["stage"].as_str().unwrap()).exists());
}

#[test]
fn override_is_reported_and_never_modified() {
    let f = Fixture::new();
    fs::create_dir_all(f.target().parent().unwrap()).unwrap();
    let override_path = f.profile().join(".codex/AGENTS.override.md");
    fs::write(&override_path, "override").unwrap();
    assert_eq!(f.run().unwrap()["override_present"], true);
    assert_eq!(fs::read_to_string(override_path).unwrap(), "override");
}

#[cfg(windows)]
#[test]
fn junction_to_another_profile_is_rejected() {
    let f = Fixture::new();
    let outside = f.0.join("outside");
    fs::create_dir(&outside).unwrap();
    let request = crate::ProcessRequest {
        args: vec![
            "/d".into(),
            "/c".into(),
            "mklink".into(),
            "/J".into(),
            f.profile().join(".codex").into_os_string(),
            outside.clone().into_os_string(),
        ],
        ..crate::ProcessRequest::new("cmd.exe", &f.0)
    };
    let output = crate::ProcessRunner::run(&crate::SystemProcessRunner, &request).unwrap();
    assert_eq!(
        output.code,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(f.run().unwrap_err().contains("reparse"));
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    fs::remove_dir(f.profile().join(".codex")).unwrap();
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
