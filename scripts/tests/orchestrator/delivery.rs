use super::*;
use bridgepilot_execution::delivery::{self, DevelopmentReview, PublicationReview};

#[test]
fn retired_delivery_proofs_cannot_advance_tasks_or_read_old_sidecars() {
    let fixture = Fixture::new("complete");
    let before = fixture.store.read().unwrap();
    let root = before.state.runs[&fixture.run].binding.root.clone();
    let tool = root.join("missing-native-tool.exe");
    let error = delivery::development_probe(&root, &tool, &["p".into()]).unwrap_err();
    assert!(error.contains("retired"));
    let error = delivery::review_development(&fixture.store, before.revision, DevelopmentReview {
        run:fixture.run.clone(), result_sha256:"untrusted".into(), source:"test:retired".into(),
        note:"must preserve task".into(), tool, packages:vec!["p".into()],
    }).unwrap_err();
    assert!(error.contains("retired"));
    let error = delivery::review_publication(&fixture.store, before.revision, PublicationReview {
        run:fixture.run.clone(), result_sha256:"untrusted".into(), source:"test:retired".into(),
        receipt_path:root.join("missing.json"), receipt_sha256:"untrusted".into(),
        remote:"origin".into(), remote_ref:"refs/heads/main".into(),
    }).unwrap_err();
    assert!(error.contains("retired"));
    let after = fixture.store.read().unwrap();
    assert_eq!(before.revision, after.revision);
    assert_eq!(serde_json::to_value(before.state).unwrap(), serde_json::to_value(after.state).unwrap());
    assert!(!root.join(".codex/bridgeforge-version.json").exists());
    assert!(!root.join(".codex/package-closures.json").exists());
}
