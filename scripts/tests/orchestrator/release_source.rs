use super::*;
use bridgeforge_core::native_release::PublishedVersion;

struct Repo(std::path::PathBuf);
impl Repo {
    fn new() -> Self {
        let root = std::path::absolute(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.runtime/tests/release-source")
            .join(crate::new_id("case"))).unwrap();
        fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-b", "main"]).unwrap();
        git(&root, &["config", "user.name", "release-test"]).unwrap();
        git(&root, &["config", "user.email", "release@example.invalid"]).unwrap();
        git(&root, &["config", "core.autocrlf", "false"]).unwrap();
        fs::write(root.join("VERSION"), "1.1.1\n").unwrap();
        fs::write(root.join("CHANGELOG.md"), "# Changes\n\n## [BP 1.1.1]\n\nPrevious release.\n").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-m", "chore: initial"]).unwrap();
        Self(root)
    }
    fn context(&self) -> SourceContext {
        SourceContext { schema: 1, identity: identity(&self.0).unwrap(),
            baseline: Some(PublishedVersion { version: "1.1.1".into(), commit: git(&self.0, &["rev-parse", "HEAD"]).unwrap().trim().into() }),
            message: Some("fix: restore publication".into()) }
    }
}
impl Drop for Repo { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

#[test]
fn unchanged_release_keeps_version_and_does_not_write() {
    let repo = Repo::new();
    let result = plan(&repo.0, &repo.context()).unwrap();
    assert_eq!(result.identity.version, "1.1.1");
    assert!(result.edits.is_empty());
    assert!(git(&repo.0, &["status", "--porcelain"]).unwrap().is_empty());
}

#[test]
fn cumulative_feature_beats_last_fix_and_prepared_version_is_not_incremented_again() {
    let repo = Repo::new();
    let mut context = repo.context();
    fs::write(repo.0.join("feature.txt"), "new feature").unwrap();
    git(&repo.0, &["add", "."]).unwrap();
    git(&repo.0, &["commit", "-m", "feat: cumulative addition"]).unwrap();
    fs::write(repo.0.join("fix.txt"), "fix").unwrap();
    let result = plan(&repo.0, &context).unwrap();
    assert_eq!(result.identity.version, "1.2.0");
    assert_eq!(fs::read_to_string(repo.0.join("VERSION")).unwrap(), "1.1.1\n");
    for edit in result.edits { fs::write(repo.0.join(edit.path), edit.after).unwrap(); }
    context.identity = identity(&repo.0).unwrap();
    let repeated = plan(&repo.0, &context).unwrap();
    assert_eq!(repeated.identity.version, "1.2.0");
    assert!(repeated.edits.is_empty());
}

#[test]
fn patch_promotes_unreleased_and_unknown_or_insufficient_version_is_rejected() {
    let repo = Repo::new();
    let mut context = repo.context();
    fs::write(repo.0.join("CHANGELOG.md"), "# Changes\n\n## [Unreleased]\n\n- Fixed recovery.\n\n## [BP 1.1.1]\n\nOld.\n").unwrap();
    let result = plan(&repo.0, &context).unwrap();
    assert_eq!(result.identity.version, "1.1.2");
    assert!(result.edits.iter().find(|e| e.path == "CHANGELOG.md").unwrap().after.contains("## [BP 1.1.2]"));
    context.message = Some("unclassified work".into());
    assert!(plan(&repo.0, &context).is_err());
    fs::write(repo.0.join("VERSION"), "1.1.2\n").unwrap();
    context.identity = identity(&repo.0).unwrap();
    context.message = Some("feat: addition".into());
    assert!(plan(&repo.0, &context).unwrap_err().contains("below"));
}

#[test]
fn semantic_levels_include_breaking_zero_major_and_overflow() {
    assert_eq!(change_level("fix: bug").unwrap(), 1);
    assert_eq!(change_level("feat(api): extension").unwrap(), 2);
    assert_eq!(change_level("fix!: changed contract").unwrap(), 3);
    assert_eq!(change_level("refactor: implementation\n\nBREAKING CHANGE: contract").unwrap(), 3);
    assert_eq!(increment("1.2.3".parse().unwrap(), 3).unwrap().to_string(), "2.0.0");
    assert_eq!(increment("0.2.3".parse().unwrap(), 3).unwrap().to_string(), "0.3.0");
    assert!(increment(SemVer { major: 1, minor: 0, patch: u64::MAX }, 1).is_err());
}

#[test]
fn prepared_notes_cannot_silently_drop_unreleased_changes() {
    let repo = Repo::new();
    let mut context = repo.context();
    fs::write(repo.0.join("VERSION"), "1.2.0\n").unwrap();
    fs::write(repo.0.join("CHANGELOG.md"), "# Changes\n\n## [Unreleased]\n\n- New work.\n\n## [BP 1.2.0]\n\nEarlier candidate.\n").unwrap();
    context.identity = identity(&repo.0).unwrap();
    assert!(plan(&repo.0, &context).unwrap_err().contains("Unreleased"));
}
