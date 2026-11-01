use super::*;
use crate::{new_id, tracking};
use std::{fs, path::PathBuf};

struct Fixture {
    root: PathBuf,
    store: Store,
}
impl Fixture {
    fn new() -> Self {
        let root = std::path::absolute(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../.runtime/tests/bridgepilot-status-sqlite/status")
                .join(new_id("case")),
        )
        .unwrap();
        let store = Store::init(&root).unwrap();
        store
            .transact(0, |s| {
                s.projects.insert(
                    "p".into(),
                    Project {
                        id: "p".into(),
                        name: "项目|A".into(),
                        repository: "fixture:p".into(),
                        paused: false,
                    },
                );
                Ok(())
            })
            .unwrap();
        Self { root, store }
    }
    fn task(&self, title: &str) -> String {
        let input = TaskInput {
            project: "p".into(),
            title: title.into(),
            goal: format!("verify {title}"),
            non_goals: vec![],
            acceptance: vec!["actual evidence".into()],
            dependencies: vec![],
            complexity: Complexity::Simple,
            authorization: Authorization {
                human_source: "fixture:human".into(),
                allow_write: false,
                allow_publish: false,
                allow_execution_chat: false,
            },
        };
        tracking::register(
            &self.store,
            self.store.read().unwrap().revision,
            serde_json::to_value(input).unwrap(),
        )
        .unwrap()
        .1
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn fixed_columns_numbers_filtering_and_no_runtime_write() {
    let f = Fixture::new();
    let first = f.task("first\n| task");
    let second = f.task("second");
    let before = query(&f.store).unwrap();
    let n = before["view"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["task_id"] == second)
        .unwrap()["number"]
        .clone();
    assert_eq!(
        before["view"]["columns"],
        json!(["序号", "项目", "任务", "阶段", "进展", "阻点"])
    );
    f.store
        .transact(f.store.read().unwrap().revision, |s| {
            s.tasks.get_mut(&first).unwrap().state = TaskState::Cancelled;
            Ok(())
        })
        .unwrap();
    let db_before = fs::read(f.store.database_path()).unwrap();
    fs::remove_dir_all(f.root.join(".runtime/bridgepilot")).unwrap();
    let after = query(&f.store).unwrap();
    assert_eq!(after["view"]["rows"].as_array().unwrap().len(), 1);
    assert_eq!(after["view"]["rows"][0]["number"], n);
    assert!(
        after["view"]["markdown"]
            .as_str()
            .unwrap()
            .contains("项目&#124;A")
    );
    assert_eq!(fs::read(f.store.database_path()).unwrap(), db_before);
    assert!(!f.root.join(".runtime/bridgepilot").exists());
}

#[test]
fn structured_event_is_atomic_idempotent_and_bound_to_current_spec() {
    let f = Fixture::new();
    let id = f.task("events");
    let task = f.store.read().unwrap().state.tasks[&id].clone();
    let event = tracking::Event::native(
        &task,
        None,
        "command-one",
        "command_completed",
        "实际检查失败".into(),
        Some("fixture error".into()),
        "fixture:receipt".into(),
        json!({"exit_code":1}),
    );
    tracking::record(&f.store, event.clone()).unwrap();
    let revision = f.store.read().unwrap().revision;
    tracking::record(&f.store, event.clone()).unwrap();
    assert_eq!(f.store.read().unwrap().revision, revision);
    let view = query(&f.store).unwrap();
    assert_eq!(view["view"]["rows"][0]["progress"], "实际检查失败");
    assert_eq!(view["view"]["rows"][0]["blocker"], "fixture error");
    let mut tampered = event.clone();
    tampered.payload = json!({"exit_code":0});
    assert!(tracking::record(&f.store, tampered).is_err());
    let mut stale = event;
    stale.id = "different-event".into();
    stale.spec_hash = "f".repeat(64);
    assert!(tracking::record(&f.store, stale).is_err());
}

#[test]
fn large_native_receipts_are_referenced_without_duplicating_payload_in_event() {
    let f = Fixture::new();
    let id = f.task("large result");
    let receipt = json!({"output":"中".repeat(1024*1024)});
    let task = f.store.read().unwrap().state.tasks[&id].clone();
    let event = tracking::Event::native(
        &task,
        None,
        "terminal-large",
        "turn_completed",
        "回合已结束".into(),
        None,
        "fixture:native".into(),
        tracking::receipt_reference(&receipt).unwrap(),
    );
    tracking::record(&f.store, event).unwrap();
    assert_eq!(
        query(&f.store).unwrap()["view"]["rows"][0]["progress"],
        "回合已结束"
    );
}

#[test]
fn publication_hold_does_not_pause_project_or_fabricate_publication() {
    let f = Fixture::new();
    let id = f.task("release");
    tracking::hold(
        &f.store,
        f.store.read().unwrap().revision,
        tracking::HoldRequest {
            task: id.clone(),
            scope: HoldScope::Publication,
            reason: "用户暂缓release".into(),
            human_source: "fixture:user-decision".into(),
            release: false,
        },
    )
    .unwrap();
    let state = f.store.read().unwrap().state;
    assert!(!state.projects["p"].paused);
    assert_eq!(state.tasks[&id].state, TaskState::Ready);
    let row = &query(&f.store).unwrap()["view"]["rows"][0];
    assert_eq!(row["phase"], "待执行·发布暂缓");
    assert_eq!(row["blocker"], "用户暂缓release");
}

#[test]
fn task_pause_and_publication_decisions_are_independent() {
    let f = Fixture::new();
    let id = f.task("two decisions");
    let decide = |scope, release| {
        tracking::hold(
            &f.store,
            f.store.read().unwrap().revision,
            tracking::HoldRequest {
                task: id.clone(),
                scope,
                release,
                reason: "fixture decision".into(),
                human_source: "fixture:human".into(),
            },
        )
    };
    decide(HoldScope::Task, false).unwrap();
    assert!(decide(HoldScope::Publication, true).is_err());
    assert_eq!(
        f.store.read().unwrap().state.tasks[&id].state,
        TaskState::Paused
    );
    decide(HoldScope::Publication, false).unwrap();
    decide(HoldScope::Task, true).unwrap();
    let state = f.store.read().unwrap().state;
    assert!(state.tracking[&id].publication_hold.is_some());
    assert!(state.tracking[&id].hold.is_none());
    assert_eq!(
        query(&f.store).unwrap()["view"]["rows"][0]["phase"],
        "待执行·发布暂缓"
    );
}

#[test]
fn prepared_task_pause_cancels_unattempted_execution_and_preserves_unknown() {
    for attempted in [false, true] {
        let f = Fixture::new();
        let id = f.task("prepared");
        let machine = f.store.machine().unwrap();
        f.store
            .transact(f.store.read().unwrap().revision, |s| {
                s.bindings.insert(
                    format!("{machine}:p"),
                    Binding {
                        project: "p".into(),
                        machine: machine.clone(),
                        root: f.root.clone(),
                        desktop_project_id: "native".into(),
                        host: "local".into(),
                    },
                );
                s.tasks
                    .get_mut(&id)
                    .unwrap()
                    .input
                    .authorization
                    .allow_execution_chat = true;
                let t = s.tasks.get_mut(&id).unwrap();
                t.spec_hash = crate::digest(&serde_json::to_vec(&t.input).unwrap());
                let run = s.prepare_run(&id, &machine)?;
                if attempted {
                    let op = s.operation(&run, "create_execution_chat")?;
                    s.operations.get_mut(&op).unwrap().state = OperationState::Attempted;
                }
                Ok(())
            })
            .unwrap();
        tracking::hold(
            &f.store,
            f.store.read().unwrap().revision,
            tracking::HoldRequest {
                task: id.clone(),
                scope: HoldScope::Task,
                reason: "pause".into(),
                human_source: "fixture:human".into(),
                release: false,
            },
        )
        .unwrap();
        let s = f.store.read().unwrap().state;
        assert_eq!(
            s.tasks[&id].state,
            if attempted {
                TaskState::Unknown
            } else {
                TaskState::Paused
            }
        );
        assert_eq!(
            s.runs.values().next().unwrap().state,
            if attempted {
                RunState::Unknown
            } else {
                RunState::Interrupted
            }
        );
    }
}

#[test]
fn observations_cannot_be_dispatched_or_treated_as_verified_completion() {
    let f = Fixture::new();
    let id = f.task("observed");
    f.store
        .transact(f.store.read().unwrap().revision, |s| {
            let m = s.tracking.get_mut(&id).unwrap();
            m.mode = ExecutionMode::Observed;
            m.execution_thread_id = Some("native-thread".into());
            m.observation = Some(tracking::Observation {
                thread_id: "native-thread".into(),
                turn_id: Some("native-turn".into()),
                status: "completed".into(),
                observed_at: tracking::now(),
                source: "fixture:native-readback".into(),
            });
            Ok(())
        })
        .unwrap();
    let mut state = f.store.read().unwrap().state;
    assert!(state.prepare_run(&id, "machine").is_err());
    assert_eq!(
        query(&f.store).unwrap()["view"]["rows"][0]["phase"],
        "观察结束·待核验"
    );
    assert_eq!(state.tasks[&id].state, TaskState::Ready);
}

#[test]
fn failed_registration_does_not_leave_an_orphan_event_or_allocate_number() {
    let f = Fixture::new();
    let id = f.task("one");
    let before = f.store.read().unwrap();
    let mut value = serde_json::to_value(&before.state.tasks[&id].input).unwrap();
    value["project"] = json!("missing");
    assert!(tracking::register(&f.store, before.revision, value).is_err());
    assert_eq!(f.store.read().unwrap().revision, before.revision);
    assert_eq!(
        query(&f.store).unwrap()["view"]["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
