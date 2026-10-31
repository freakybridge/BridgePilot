use super::*;
use bridgepilot_orchestrator::grouping;

#[test]
fn grouping_does_not_treat_a_reused_owner_pid_as_the_current_execution() {
    let f = Fixture::new("complete");
    f.store.update(|s| {
        let r=s.runs.get_mut(&f.run).unwrap();r.state=RunState::BindingRequired;r.owner_pid=Some(std::process::id());
        let op=s.operation(&f.run,"resume_execution")?;
        s.operations.get_mut(&op).unwrap().result=Some(serde_json::json!({"owner_pid":std::process::id(),"owner_identity":"win-filetime-0000000000000000"}));Ok(())
    }).unwrap();
    let before = f.store.read().unwrap();
    assert!(grouping::ensure(&f.store, &f.run, Fixture::executable(), "test:reused-pid").is_err());
    assert_eq!(before.revision, f.store.read().unwrap().revision);
    assert!(
        !before.state.runs[&f.run]
            .binding
            .root
            .join(".bp-fixture-project-updates")
            .exists()
    );
}

#[test]
fn native_grouping_resolves_legacy_identity_and_reads_back_without_a_model_turn() {
    let f = Fixture::new("complete");
    let proof = grouping::ensure(
        &f.store,
        &f.run,
        Fixture::executable(),
        "test:actual-native-binding",
    )
    .unwrap();
    assert_eq!(proof["desktop_project_id"], "fixture-project");
    assert_eq!(proof["native_project_id"], "fixture-server-project");
    assert_eq!(proof["task_turn_started"], false);
    grouping::ensure(
        &f.store,
        &f.run,
        Fixture::executable(),
        "test:second-read-only-verification",
    )
    .unwrap();
    let s = f.store.read().unwrap();
    let root = &s.state.runs[&f.run].binding.root;
    assert!(!root.join(".bp-fixture-started").exists());
    assert_eq!(
        fs::read_to_string(root.join(".bp-fixture-project-updates"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        s.state
            .operations
            .values()
            .filter(|o| o.kind == "assign_execution_project")
            .count(),
        1
    );
}

#[test]
fn unknown_native_assignment_is_reconciled_by_readback_without_repeating_the_write() {
    let f = Fixture::new("drop-group-response");
    assert!(
        grouping::ensure(
            &f.store,
            &f.run,
            Fixture::executable(),
            "test:actual-native-binding"
        )
        .is_err()
    );
    let s = f.store.read().unwrap();
    let root = s.state.runs[&f.run].binding.root.clone();
    assert!(
        s.state
            .operations
            .values()
            .any(|o| o.kind == "assign_execution_project" && o.state == OperationState::Unknown)
    );
    let receipt = grouping::ensure(
        &f.store,
        &f.run,
        Fixture::executable(),
        "test:readback-after-response-loss",
    )
    .unwrap();
    assert_eq!(receipt["status"], "native_grouping_verified");
    assert_eq!(
        fs::read_to_string(root.join(".bp-fixture-project-updates"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn ambiguous_project_inventory_and_pause_race_cannot_start_or_certify_execution() {
    let f = Fixture::new("ambiguous-project");
    assert!(grouping::ensure(&f.store, &f.run, Fixture::executable(), "test:binding").is_err());
    assert!(
        !f.store.read().unwrap().state.runs[&f.run]
            .binding
            .root
            .join(".bp-fixture-project-updates")
            .exists()
    );
    let f = Fixture::new("pause-group");
    let root = f.root.clone();
    let run = f.run.clone();
    let call = thread::spawn(move || {
        grouping::ensure(
            &Store::open(&root).unwrap(),
            &run,
            Fixture::executable(),
            "test:binding",
        )
    });
    let path = f.store.read().unwrap().state.runs[&f.run]
        .binding
        .root
        .clone();
    let until = std::time::Instant::now() + Duration::from_secs(8);
    while !path.join(".bp-fixture-grouping").exists() {
        assert!(std::time::Instant::now() < until);
        thread::sleep(Duration::from_millis(10));
    }
    f.store.update(|s| s.pause(false, &["p".into()])).unwrap();
    fs::write(path.join(".bp-fixture-group-release"), "1").unwrap();
    assert!(call.join().unwrap().is_err());
    assert!(
        !f.store
            .read()
            .unwrap()
            .state
            .operations
            .values()
            .any(|o| o.kind == "assign_execution_project" && o.state == OperationState::Observed)
    );
    assert!(!path.join(".bp-fixture-started").exists());
}
