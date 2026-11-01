use super::*;
use bridgepilot_orchestrator::{scheduler, tracking};
use serde_json::json;

fn observed() -> Fixture {
    let f = Fixture::new("complete");
    f.store
        .transact(f.store.read().unwrap().revision, |s| {
            s.runs.get_mut(&f.run).unwrap().state = RunState::Interrupted;
            s.tasks.get_mut(&f.task).unwrap().state = TaskState::Ready;
            s.tracking.insert(
                f.task.clone(),
                tracking::TaskTracking {
                    source_key: "fixture:observed".into(),
                    source_uri: "fixture:human".into(),
                    mode: tracking::ExecutionMode::Observed,
                    controller_thread_id: Some("observed-thread".into()),
                    execution_thread_id: Some("observed-thread".into()),
                    ..Default::default()
                },
            );
            Ok(())
        })
        .unwrap();
    fs::write(f.root.join(".bp-fixture-mode"), "observer-active").unwrap();
    f
}

#[test]
fn observe_is_read_only_native_and_updates_only_for_new_receipts() {
    let f = observed();
    let turn = |items| json!({"id":"observed-turn","status":"inProgress","items":items});
    fs::write(
        f.root.join(".bp-fixture-turn.json"),
        turn(json!([])).to_string(),
    )
    .unwrap();
    let observe = || {
        codex::observe_task(
            &f.store,
            f.store.read().unwrap().revision,
            &f.task,
            Fixture::executable(),
            "fixture:readback",
        )
        .unwrap()
    };
    observe();
    let revision = f.store.read().unwrap().revision;
    assert_eq!(observe()["unchanged"], true);
    assert_eq!(f.store.read().unwrap().revision, revision);
    fs::write(
        f.root.join(".bp-fixture-turn.json"),
        turn(json!([{"id":"command-1","type":"commandExecution","exitCode":1}])).to_string(),
    )
    .unwrap();
    assert_eq!(observe()["proof"]["failed_command_receipts"], 1);
    assert_eq!(f.store.read().unwrap().revision, revision + 1);
    assert!(!f.root.join(".bp-fixture-started").exists());
    assert_eq!(
        f.store.read().unwrap().state.tasks[&f.task].state,
        TaskState::Ready
    );
    fs::write(f.root.join(".bp-fixture-turn.json"), "invalid native data").unwrap();
    assert!(
        codex::observe_task(
            &f.store,
            f.store.read().unwrap().revision,
            &f.task,
            Fixture::executable(),
            "fixture:readback"
        )
        .is_err()
    );
    let status = bridgepilot_orchestrator::status::query(&f.store).unwrap();
    assert_eq!(status["view"]["rows"][0]["phase"], "观察受限·保留最后读回");
    assert_eq!(
        f.store.read().unwrap().state.tracking[&f.task]
            .observation
            .as_ref()
            .unwrap()
            .status,
        "inProgress"
    );
    fs::write(
        f.root.join(".bp-fixture-turn.json"),
        turn(json!([{"id":"command-1","type":"commandExecution","exitCode":1}])).to_string(),
    )
    .unwrap();
    observe();
    assert!(
        f.store.read().unwrap().state.tracking[&f.task]
            .observation_error
            .is_none()
    );
    assert_eq!(
        bridgepilot_orchestrator::status::query(&f.store).unwrap()["view"]["rows"][0]["phase"],
        "观察执行中"
    );
    let recovered = f.store.read().unwrap().revision;
    observe();
    assert_eq!(f.store.read().unwrap().revision, recovered);
}

#[test]
fn supervisor_observes_without_dispatching_an_existing_chat() {
    let f = observed();
    let result = scheduler::supervise(
        &f.store,
        Fixture::executable(),
        Path::new(env!("CARGO_BIN_EXE_bp")),
        scheduler::Supervision {
            projects: vec!["p".into()],
            max_parallel: 1,
            timeout_seconds: 10,
            stop_when_idle: true,
        },
    )
    .unwrap();
    assert_eq!(result["observations"][&f.task]["status"], "task_observed");
    assert_eq!(result["launched"], json!([]));
    assert!(!f.root.join(".bp-fixture-started").exists());
}

#[test]
fn unloaded_server_does_not_treat_historical_terminal_as_current_desktop_state() {
    let f = observed();
    fs::write(
        f.root.join(".bp-fixture-turn.json"),
        json!({"id":"historical","status":"interrupted","items":[]}).to_string(),
    )
    .unwrap();
    fs::write(f.root.join(".bp-fixture-mode"), "observer-not-loaded").unwrap();
    let read = || {
        codex::observe_task(
            &f.store,
            f.store.read().unwrap().revision,
            &f.task,
            Fixture::executable(),
            "fixture:native",
        )
        .unwrap()
    };
    assert_eq!(read()["proof"]["last_recorded_turn_status"], "interrupted");
    assert_eq!(
        bridgepilot_orchestrator::status::query(&f.store).unwrap()["view"]["rows"][0]["phase"],
        "观察受限·当前状态未核实"
    );
    fs::write(f.root.join(".bp-fixture-mode"), "observer-goal-active").unwrap();
    read();
    assert_eq!(
        bridgepilot_orchestrator::status::query(&f.store).unwrap()["view"]["rows"][0]["phase"],
        "托管目标进行中·观察受限"
    );
    fs::write(f.root.join(".bp-fixture-mode"), "observer-goal-paused").unwrap();
    read();
    assert_eq!(
        bridgepilot_orchestrator::status::query(&f.store).unwrap()["view"]["rows"][0]["phase"],
        "观察受限·当前状态未核实"
    );
    fs::write(f.root.join(".bp-fixture-mode"), "observer-goal-active").unwrap();
    read();
    assert_eq!(
        bridgepilot_orchestrator::status::query(&f.store).unwrap()["view"]["rows"][0]["phase"],
        "托管目标进行中·观察受限"
    );
    let revision = f.store.read().unwrap().revision;
    assert_eq!(read()["unchanged"], true);
    assert_eq!(f.store.read().unwrap().revision, revision);
    fs::write(f.root.join(".bp-fixture-mode"), "observer-active").unwrap();
    assert_eq!(read()["proof"]["turn_status"], "unknown");
    assert!(
        tracking::hold(
            &f.store,
            f.store.read().unwrap().revision,
            tracking::HoldRequest {
                task: f.task.clone(),
                scope: tracking::HoldScope::Task,
                reason: "must not certify stop from active server and historical terminal".into(),
                human_source: "fixture:human".into(),
                release: false
            }
        )
        .is_err()
    );
    assert!(!f.root.join(".bp-fixture-started").exists());
}
