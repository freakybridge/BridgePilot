use bridgepilot_orchestrator::{codex, domain::*, new_id, store::Store};
#[path = "delivery.rs"]
mod delivery_cases;
#[path = "grouping.rs"]
mod grouping_cases;
#[path = "portable.rs"]
mod portable_cases;
#[path = "scheduler.rs"]
mod scheduler_cases;
#[path = "wakeup.rs"]
mod wakeup_cases;
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

struct Fixture {
    root: PathBuf,
    store: Store,
    run: String,
    task: String,
}
#[test]
fn wall_limit_includes_handshake_start_and_missing_stop_acknowledgement() {
    use bridgeforge_core::{ProcessRequest, ProcessRunner, SystemProcessRunner};
    for mode in [
        "hang-initialize",
        "hang-turn-start",
        "hang-interrupt",
        "hang-reading-input",
    ] {
        let f = Fixture::new(mode);
        let limit = if mode == "hang-reading-input" {
            let prior = f.store.read().unwrap();
            f.store
                .transact(prior.revision, |s| {
                    let t = s.tasks.get_mut(&f.task).unwrap();
                    t.input.goal = "bounded input pressure ".repeat(16384);
                    t.spec_hash =
                        bridgepilot_orchestrator::digest(&serde_json::to_vec(&t.input).unwrap());
                    s.runs.get_mut(&f.run).unwrap().spec_hash = t.spec_hash.clone();
                    Ok(())
                })
                .unwrap();
            "4"
        } else {
            "2"
        };
        let began = std::time::Instant::now();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_bp"))
            .args([
                "execute",
                "--workspace",
                f.root.to_str().unwrap(),
                "--run",
                &f.run,
                "--codex",
                Fixture::executable().to_str().unwrap(),
                "--timeout-seconds",
                limit,
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            began.elapsed() < Duration::from_secs(limit.parse::<u64>().unwrap() + 4),
            "wall limit was extended by protocol waits"
        );
        let snapshot = f.store.read().unwrap();
        let run = &snapshot.state.runs[&f.run];
        assert_eq!(run.state, RunState::Unknown);
        assert_eq!(
            run.result.as_ref().unwrap()["time_limit_seconds"],
            limit.parse::<u64>().unwrap()
        );
        if mode == "hang-reading-input" {
            assert!(
                snapshot
                    .state
                    .operations
                    .values()
                    .any(|op| op.kind == "start_turn" && op.state == OperationState::Unknown)
            );
            assert!(!run.binding.root.join(".bp-fixture-started").exists());
        }
        let pid = fs::read_to_string(run.binding.root.join(".bp-fixture-pid")).unwrap();
        #[cfg(windows)]
        {
            let mut request = ProcessRequest::new("tasklist.exe", &run.binding.root);
            request.args = vec![
                "/FI".into(),
                format!("PID eq {pid}").into(),
                "/FO".into(),
                "CSV".into(),
                "/NH".into(),
            ];
            request.timeout = Duration::from_secs(5);
            let output = SystemProcessRunner.run(&request).unwrap();
            assert_eq!(output.code, 0);
            assert!(
                !String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line
                        .split(',')
                        .nth(1)
                        .is_some_and(|v| v.trim_matches('"') == pid))
            );
        }
        #[cfg(not(windows))]
        {
            let mut request = ProcessRequest::new("ps", &run.binding.root);
            request.args = vec!["-p".into(), pid.into(), "-o".into(), "pid=".into()];
            let output = SystemProcessRunner.run(&request).unwrap();
            assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
        }
    }
}
#[test]
fn recovery_cannot_commit_a_stale_observation_over_a_new_execution_owner() {
    let fresh = Fixture::new("complete");
    assert!(codex::reconcile(&fresh.store, &fresh.run, Fixture::executable()).is_err());
    for mode in ["wrong-thread", "lost-response"] {
        let f = Fixture::new(mode);
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_bp"))
            .args([
                "execute",
                "--workspace",
                f.root.to_str().unwrap(),
                "--run",
                &f.run,
                "--codex",
                Fixture::executable().to_str().unwrap(),
                "--timeout-seconds",
                "30",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let old = f.store.read().unwrap();
        let project = old.state.runs[&f.run].binding.root.clone();
        fs::write(project.join(".bp-fixture-mode"), "read-barrier").unwrap();
        let root = f.root.clone();
        let run = f.run.clone();
        let worker = thread::spawn(move || {
            codex::reconcile(&Store::open(&root).unwrap(), &run, Fixture::executable())
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while !project.join(".bp-fixture-reading").exists() {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        f.store
            .transact(old.revision, |s| {
                let r = s.runs.get_mut(&f.run).unwrap();
                r.state = RunState::Running;
                r.owner_pid = Some(std::process::id());
                s.tasks.get_mut(&f.task).unwrap().state = TaskState::Running;
                Ok(())
            })
            .unwrap();
        fs::write(project.join(".bp-fixture-release"), "release").unwrap();
        assert!(worker.join().unwrap().is_err());
        let current = f.store.read().unwrap();
        assert_eq!(current.state.runs[&f.run].state, RunState::Running);
        assert_eq!(
            current.state.runs[&f.run].owner_pid,
            Some(std::process::id())
        );
        assert_eq!(current.state.tasks[&f.task].state, TaskState::Running);
    }
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let root = std::path::absolute(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.runtime/tests/bridgepilot-stage2/execute")
                .join(new_id("case")),
        )
        .unwrap();
        let store = Store::init(&root).unwrap();
        let machine = store.machine().unwrap();
        let project = root.join("project");
        fs::create_dir(&project).unwrap();
        let project = fs::canonicalize(project).unwrap();
        fs::write(project.join(".bp-fixture-mode"), mode).unwrap();
        fs::write(
            project.join("AGENTS.md"),
            "Controlled fixture rules: no publication.\n",
        )
        .unwrap();
        let (_, (task, run)) = store
            .transact(0, |s| {
                s.projects.insert(
                    "p".into(),
                    Project {
                        id: "p".into(),
                        name: "fixture".into(),
                        repository: "fixture".into(),
                        paused: false,
                    },
                );
                s.bindings.insert(
                    format!("{machine}:p"),
                    Binding {
                        project: "p".into(),
                        machine: machine.clone(),
                        root: project,
                        desktop_project_id: "fixture-project".into(),
                        host: "local".into(),
                    },
                );
                let task = s.add_task(TaskInput {
                    project: "p".into(),
                    title: "controlled execution".into(),
                    goal: "protocol verification".into(),
                    non_goals: vec!["no publication".into()],
                    acceptance: vec!["actual terminal receipt".into()],
                    dependencies: vec![],
                    complexity: Complexity::Simple,
                    authorization: Authorization {
                        human_source: "test:human".into(),
                        allow_write: false,
                        allow_publish: false,
                        allow_execution_chat: true,
                    },
                })?;
                let run = s.prepare_run(&task, &machine)?;
                let r = s.runs.get_mut(&run).unwrap();
                r.thread_id = Some("fixture-native-thread".into());
                r.desktop_binding_source = Some("test:actual-client-receipt".into());
                let op = s.ensure_chat_preparation(&run)?;
                let op = s.operations.get_mut(&op).unwrap();
                op.state = OperationState::Observed;
                op.result = Some(
                    serde_json::json!({"thread_id":"fixture-native-thread","handoff_ready":true}),
                );
                Ok((task, run))
            })
            .unwrap();
        Self {
            root,
            store,
            run,
            task,
        }
    }
    fn executable() -> &'static Path {
        Path::new(env!("CARGO_BIN_EXE_bp-rpc-fixture"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn completed_model_turn_requires_reconcile_not_automatic_publication() {
    let f = Fixture::new("complete");
    let result = codex::execute(&f.store, &f.run, Fixture::executable(), 30).unwrap();
    assert_eq!(result["publication_verified"], false);
    let s = f.store.read().unwrap();
    assert_eq!(s.state.tasks[&f.task].state, TaskState::Reviewing);
    assert_eq!(s.state.runs[&f.run].model.as_deref(), Some("fixture-model"));
    assert!(codex::execute(&f.store, &f.run, Fixture::executable(), 30).is_err());
}
#[test]
fn explicit_pause_is_acknowledged_before_paused_terminal() {
    let f = Fixture::new("pause");
    let root = f.root.clone();
    let run = f.run.clone();
    let worker = thread::spawn(move || {
        codex::execute(
            &Store::open(&root).unwrap(),
            &run,
            Fixture::executable(),
            30,
        )
    });
    for _ in 0..200 {
        if f.store.read().unwrap().state.runs[&f.run].state == RunState::Running {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    loop {
        let s = f.store.read().unwrap();
        if f.store
            .transact(s.revision, |s| s.pause(false, &["p".into()]))
            .is_ok()
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let result = worker.join().unwrap().unwrap();
    assert_eq!(result["turn_status"], "interrupted");
    let s = f.store.read().unwrap();
    assert_eq!(s.state.tasks[&f.task].state, TaskState::Paused);
    assert_eq!(s.state.runs[&f.run].state, RunState::Interrupted);
}
#[test]
fn approval_is_not_automatically_granted() {
    let f = Fixture::new("approval");
    let result = codex::execute(&f.store, &f.run, Fixture::executable(), 30).unwrap();
    assert!(!result["approval_required"].is_null());
    assert_eq!(
        f.store.read().unwrap().state.tasks[&f.task].state,
        TaskState::Blocked
    );
}
#[test]
fn lost_start_response_is_unknown_and_cannot_be_replayed() {
    let f = Fixture::new("lost-response");
    assert!(codex::execute(&f.store, &f.run, Fixture::executable(), 30).is_err());
    let s = f.store.read().unwrap();
    assert_eq!(s.state.runs[&f.run].state, RunState::Unknown);
    assert_eq!(s.state.tasks[&f.task].state, TaskState::Unknown);
    assert!(codex::execute(&f.store, &f.run, Fixture::executable(), 30).is_err());
    assert!(
        s.state
            .operations
            .values()
            .any(|op| op.state == OperationState::Unknown)
    );
}
#[test]
fn missing_desktop_binding_does_not_spawn_or_commit_an_attempt() {
    let f = Fixture::new("complete");
    let s = f.store.read().unwrap();
    f.store
        .transact(s.revision, |s| {
            let r = s.runs.get_mut(&f.run).unwrap();
            r.thread_id = None;
            r.desktop_binding_source = None;
            Ok(())
        })
        .unwrap();
    let before = f.store.read().unwrap().revision;
    assert!(codex::execute(&f.store, &f.run, Fixture::executable(), 30).is_err());
    assert_eq!(f.store.read().unwrap().revision, before);
}
#[test]
fn recovery_queries_history_after_owner_exit_without_repeating_a_turn() {
    let f = Fixture::new("lost-response");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bp"))
        .args([
            "execute",
            "--workspace",
            f.root.to_str().unwrap(),
            "--run",
            &f.run,
            "--codex",
            Fixture::executable().to_str().unwrap(),
            "--timeout-seconds",
            "30",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        f.store.read().unwrap().state.runs[&f.run].state,
        RunState::Unknown
    );
    let result = codex::reconcile(&f.store, &f.run, Fixture::executable()).unwrap();
    assert_eq!(result["status"], "reconciled");
    let s = f.store.read().unwrap();
    assert_eq!(s.state.tasks[&f.task].state, TaskState::Reviewing);
    assert_eq!(s.state.runs.len(), 1);
}
#[test]
fn invalid_native_idle_receipts_never_start_a_task_turn() {
    for mode in ["missing-turns", "wrong-thread", "unknown-status"] {
        let f = Fixture::new(mode);
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_bp"))
            .args([
                "execute",
                "--workspace",
                f.root.to_str().unwrap(),
                "--run",
                &f.run,
                "--codex",
                Fixture::executable().to_str().unwrap(),
                "--timeout-seconds",
                "30",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let s = f.store.read().unwrap();
        let run = &s.state.runs[&f.run];
        assert_eq!(run.state, RunState::Unknown);
        assert!(!run.binding.root.join(".bp-fixture-started").exists());
        assert!(
            !s.state
                .operations
                .values()
                .any(|op| op.kind == "start_turn")
        );
        let receipt = codex::reconcile(&f.store, &f.run, Fixture::executable()).unwrap();
        assert_eq!(receipt["status"], "reconciled_before_start");
        assert_eq!(
            f.store.read().unwrap().state.tasks[&f.task].state,
            TaskState::Paused
        );
    }
}
#[test]
fn quoted_or_wrong_operation_markers_cannot_release_unknown_execution() {
    for mode in ["assistant-marker", "setup-marker", "wrong-operation"] {
        let f = Fixture::new(mode);
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_bp"))
            .args([
                "execute",
                "--workspace",
                f.root.to_str().unwrap(),
                "--run",
                &f.run,
                "--codex",
                Fixture::executable().to_str().unwrap(),
                "--timeout-seconds",
                "30",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(codex::reconcile(&f.store, &f.run, Fixture::executable()).is_err());
        let s = f.store.read().unwrap();
        assert_eq!(s.state.runs[&f.run].state, RunState::Unknown);
        assert_eq!(s.state.tasks[&f.task].state, TaskState::Unknown);
        assert!(
            s.state
                .operations
                .values()
                .any(|op| op.kind == "start_turn" && op.state == OperationState::Unknown)
        );
    }
}
