use super::*;
use bridgepilot_tasks::digest;
use bridgepilot_execution::review::{ChatGrouping, FileEvidence, ReportReview, review};
use bridgepilot_execution::scheduler::{Supervision, supervise};
use serde_json::json;
use std::time::Instant;

fn fixture_grouping(f: &Fixture) -> Option<ChatGrouping> {
    let s = f.store.read().unwrap();
    let r = &s.state.runs[&f.run];
    Some(ChatGrouping {
        thread_id: r.thread_id.clone().unwrap(),
        project_id: r.binding.desktop_project_id.clone(),
        cwd: r.binding.root.to_str().unwrap().into(),
        source: "test:actual-controlled-native-readback".into(),
    })
}

fn wait_for(mut predicate: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(
            Instant::now() < until,
            "controlled supervisor wait timed out"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
fn request(projects: &[&str], seconds: u64, idle: bool) -> Supervision {
    Supervision {
        projects: projects.iter().map(|p| (*p).into()).collect(),
        max_parallel: 2,
        timeout_seconds: seconds,
        stop_when_idle: idle,
    }
}
fn other_project(f: &Fixture, mode: &str) -> (String, String, PathBuf) {
    let machine = f.store.machine().unwrap();
    let path = f.root.join("project-q");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("AGENTS.md"), "Controlled q project.\n").unwrap();
    fs::write(path.join(".bp-fixture-mode"), mode).unwrap();
    let path = fs::canonicalize(path).unwrap();
    let snapshot = f.store.read().unwrap();
    let (_, ids) = f
        .store
        .transact(snapshot.revision, |s| {
            s.projects.insert(
                "q".into(),
                Project {
                    id: "q".into(),
                    name: "q".into(),
                    repository: "repo:q".into(),
                    paused: false,
                },
            );
            s.bindings.insert(
                format!("{machine}:q"),
                Binding {
                    project: "q".into(),
                    machine: machine.clone(),
                    root: path.clone(),
                    desktop_project_id: "fixture-project-q".into(),
                    host: "local".into(),
                },
            );
            let mut input = s.tasks[&f.task].input.clone();
            input.project = "q".into();
            let task = s.add_task(input)?;
            let run = s.prepare_run(&task, &machine)?;
            let r = s.runs.get_mut(&run).unwrap();
            r.thread_id = Some("fixture-thread-q".into());
            r.desktop_binding_source = Some("test:controlled-client".into());
            let op = s.ensure_chat_preparation(&run)?;
            let op = s.operations.get_mut(&op).unwrap();
            op.state = OperationState::Observed;
            op.result = Some(json!({"thread_id":"fixture-thread-q","handoff_ready":true}));
            Ok((task, run))
        })
        .unwrap();
    (ids.0, ids.1, path)
}

#[test]
fn supervisor_runs_two_projects_and_rejects_a_second_controller() {
    let f = Fixture::new("barrier-complete");
    let (_, q, qpath) = other_project(&f, "barrier-complete");
    let root = f.root.clone();
    let worker = thread::spawn(move || {
        supervise(
            &Store::open(&root).unwrap(),
            Fixture::executable(),
            Path::new(env!("CARGO_BIN_EXE_bp")),
            request(&["p", "q"], 20, true),
        )
    });
    wait_for(|| {
        let s = f.store.read().unwrap();
        s.state.runs[&f.run].state == RunState::Running
            && s.state.runs[&q].state == RunState::Running
    });
    assert!(
        supervise(
            &f.store,
            Fixture::executable(),
            Path::new(env!("CARGO_BIN_EXE_bp")),
            request(&["p", "q"], 2, true)
        )
        .is_err()
    );
    let ppath = f.store.read().unwrap().state.runs[&f.run]
        .binding
        .root
        .clone();
    fs::write(ppath.join(".bp-fixture-release"), "go").unwrap();
    fs::write(qpath.join(".bp-fixture-release"), "go").unwrap();
    let result = worker.join().unwrap().unwrap();
    assert_eq!(result["reason"], "idle");
    assert_eq!(result["launched"].as_array().unwrap().len(), 2);
    let s = f.store.read().unwrap();
    assert_eq!(s.state.runs[&f.run].state, RunState::Completed);
    assert_eq!(s.state.runs[&q].state, RunState::Completed);
    assert_eq!(s.state.tasks[&f.task].state, TaskState::Reviewing);
    assert_eq!(
        s.state
            .operations
            .values()
            .filter(|op| op.kind == "launch_worker")
            .count(),
        2
    );
}

#[test]
fn unknown_execution_blocks_only_its_project_and_is_not_replayed_on_restart() {
    let f = Fixture::new("lost-response");
    let (_, q, _) = other_project(&f, "complete");
    let result = supervise(
        &f.store,
        Fixture::executable(),
        Path::new(env!("CARGO_BIN_EXE_bp")),
        request(&["p", "q"], 10, true),
    )
    .unwrap();
    assert_eq!(result["launched"].as_array().unwrap().len(), 2);
    let s = f.store.read().unwrap();
    assert_eq!(s.state.runs[&f.run].state, RunState::Unknown);
    assert_eq!(s.state.runs[&q].state, RunState::Completed);
    let result = supervise(
        &f.store,
        Fixture::executable(),
        Path::new(env!("CARGO_BIN_EXE_bp")),
        request(&["p", "q"], 2, true),
    )
    .unwrap();
    assert!(result["launched"].as_array().unwrap().is_empty());
    assert_eq!(f.store.read().unwrap().state.runs.len(), 2);
}

#[test]
fn regression_existing_chat_handoff_wait_does_not_append_unchanged_archive_revisions() {
    let f = Fixture::new("complete");
    f.store
        .update(|s| {
            let op = s
                .operations
                .values_mut()
                .find(|o| o.run == f.run && o.kind == "prepare_execution_chat")
                .unwrap();
            op.state = OperationState::Intent;
            op.result = None;
            Ok(())
        })
        .unwrap();
    let before = f.store.read().unwrap();
    let result = supervise(
        &f.store,
        Fixture::executable(),
        Path::new(env!("CARGO_BIN_EXE_bp")),
        request(&["p"], 1, false),
    )
    .unwrap();
    assert!(result["launched"].as_array().unwrap().is_empty());
    let after = f.store.read().unwrap();
    assert_eq!(
        before.revision, after.revision,
        "waiting on an existing client action must remain read-only"
    );
    assert_eq!(after.state.runs[&f.run].state, RunState::Prepared);
}

#[test]
fn idle_supervisor_observes_native_binding_and_new_task_without_another_start() {
    let f = Fixture::new("complete");
    let s = f.store.read().unwrap();
    let (_, op) = f
        .store
        .transact(s.revision, |s| {
            let r = s.runs.get_mut(&f.run).unwrap();
            r.thread_id = None;
            r.desktop_binding_source = None;
            s.operations
                .retain(|_, op| op.run != f.run || op.kind != "prepare_execution_chat");
            s.operation(&f.run, "create_execution_chat")
        })
        .unwrap();
    let root = f.root.clone();
    let worker = thread::spawn(move || {
        supervise(
            &Store::open(&root).unwrap(),
            Fixture::executable(),
            Path::new(env!("CARGO_BIN_EXE_bp")),
            request(&["p"], 5, false),
        )
    });
    wait_for(|| {
        f.root
            .join(".runtime/bridgepilot/supervisor.json")
            .is_file()
    });
    thread::sleep(Duration::from_millis(250));
    assert_eq!(
        f.store.read().unwrap().state.runs[&f.run].state,
        RunState::Prepared
    );
    let s = f.store.read().unwrap();
    bridgepilot_orchestrator::cli::run(vec![
        "operation-attempt".into(),
        "--workspace".into(),
        f.root.to_str().unwrap().into(),
        "--expected".into(),
        s.revision.to_string(),
        "--operation".into(),
        op.clone(),
    ])
    .unwrap();
    let s = f.store.read().unwrap();
    let binding = &s.state.runs[&f.run].binding;
    let input = f.root.join(".runtime/bind.json");
    fs::write(&input,json!({"run":f.run,"operation":op,"thread_id":"fixture-bound-after-start","desktop_project_id":binding.desktop_project_id,"cwd":binding.root,"observed_status":"completed","client_receipt_source":"test:controlled-native-client"}).to_string()).unwrap();
    bridgepilot_orchestrator::cli::run(vec![
        "run-bind".into(),
        "--workspace".into(),
        f.root.to_str().unwrap().into(),
        "--expected".into(),
        s.revision.to_string(),
        "--input".into(),
        input.to_str().unwrap().into(),
    ])
    .unwrap();
    thread::sleep(Duration::from_millis(250));
    assert!(!f.store.read().unwrap().state.chat_ready(&f.run));
    assert!(
        !f.store.read().unwrap().state.runs[&f.run]
            .binding
            .root
            .join(".bp-fixture-started")
            .exists()
    );
    codex::restore_chat(
        &f.store,
        &f.run,
        Fixture::executable(),
        "test:actual-native-archive-ack",
    )
    .unwrap();
    wait_for(|| f.store.read().unwrap().state.runs[&f.run].state == RunState::Completed);
    let s = f.store.read().unwrap();
    let (_, task) = f
        .store
        .transact(s.revision, |s| {
            let mut input = s.tasks[&f.task].input.clone();
            input.title = "arrived after supervisor startup".into();
            s.add_task(input)
        })
        .unwrap();
    wait_for(|| {
        f.store
            .read()
            .unwrap()
            .state
            .runs
            .values()
            .any(|r| r.task == task)
    });
    let result = worker.join().unwrap().unwrap();
    assert_eq!(result["reason"], "time_limit");
    assert_eq!(result["launched"].as_array().unwrap().len(), 1);
    let s = f.store.read().unwrap();
    let runs: Vec<_> = s.state.runs.values().filter(|r| r.task == task).collect();
    assert_eq!(runs.len(), 1);
    assert!(runs[0].thread_id.is_none());
    assert_eq!(
        s.state
            .operations
            .values()
            .filter(|op| op.run == runs[0].id && op.kind == "create_execution_chat")
            .count(),
        1
    );
}

#[test]
fn report_review_binds_result_and_files_before_delivery() {
    let f = Fixture::new("complete");
    codex::execute(&f.store, &f.run, Fixture::executable(), 10).unwrap();
    let s = f.store.read().unwrap();
    let run = &s.state.runs[&f.run];
    let path = run.binding.root.join("AGENTS.md");
    let original = fs::read(&path).unwrap();
    let input = ReportReview {
        command_ids: vec![],
        desktop_grouping: fixture_grouping(&f),
        run: f.run.clone(),
        result_sha256: digest(&serde_json::to_vec(run.result.as_ref().unwrap()).unwrap()),
        source: "test:independent-file-check".into(),
        note: "Checked actual fixture file and command receipt".into(),
        files: vec![FileEvidence {
            relative_path: "AGENTS.md".into(),
            sha256: digest(&original),
        }],
    };
    let mut wrong = input.clone();
    wrong.result_sha256 = "wrong".into();
    assert!(review(&f.store, s.revision, wrong).is_err());
    fs::write(&path, "changed evidence").unwrap();
    assert!(review(&f.store, s.revision, input.clone()).is_err());
    fs::write(&path, &original).unwrap();
    let mut outside = input.clone();
    outside.files[0].relative_path = "../HEAD.json".into();
    assert!(review(&f.store, s.revision, outside).is_err());
    assert_eq!(
        f.store.read().unwrap().state.tasks[&f.task].state,
        TaskState::Reviewing
    );
    let result = review(&f.store, s.revision, input).unwrap();
    assert_eq!(result["status"], "report_delivered");
    assert_eq!(result["publication_verified"], false);
    assert_eq!(
        f.store.read().unwrap().state.tasks[&f.task].state,
        TaskState::ReportDelivered
    );
}

#[test]
fn prepared_outbox_is_unique_and_bad_scope_writes_nothing() {
    let f = Fixture::new("complete");
    let initial = f.store.read().unwrap();
    assert!(
        supervise(
            &f.store,
            Fixture::executable(),
            Path::new(env!("CARGO_BIN_EXE_bp")),
            request(&["p", "p"], 1, true)
        )
        .is_err()
    );
    assert_eq!(f.store.read().unwrap().revision, initial.revision);
    // A fresh dependent task cannot be dispatched until its dependency is delivered.
    let (_, task) = f
        .store
        .transact(initial.revision, |s| {
            let mut input = s.tasks[&f.task].input.clone();
            input.title = "dependent".into();
            input.dependencies = vec![f.task.clone()];
            s.add_task(input)
        })
        .unwrap();
    supervise(
        &f.store,
        Fixture::executable(),
        Path::new(env!("CARGO_BIN_EXE_bp")),
        request(&["p"], 5, true),
    )
    .unwrap();
    assert!(
        !f.store
            .read()
            .unwrap()
            .state
            .runs
            .values()
            .any(|r| r.task == task)
    );
    let s = f.store.read().unwrap();
    let run = &s.state.runs[&f.run];
    let path = run.binding.root.join("AGENTS.md");
    let input = ReportReview {
        command_ids: vec![],
        desktop_grouping: fixture_grouping(&f),
        run: f.run.clone(),
        result_sha256: digest(&serde_json::to_vec(run.result.as_ref().unwrap()).unwrap()),
        source: "test:review".into(),
        note: "actual controlled evidence".into(),
        files: vec![FileEvidence {
            relative_path: "AGENTS.md".into(),
            sha256: digest(&fs::read(path).unwrap()),
        }],
    };
    review(&f.store, s.revision, input).unwrap();
    supervise(
        &f.store,
        Fixture::executable(),
        Path::new(env!("CARGO_BIN_EXE_bp")),
        request(&["p"], 1, false),
    )
    .unwrap();
    let s = f.store.read().unwrap();
    let run = s.state.runs.values().find(|r| r.task == task).unwrap();
    assert_eq!(run.state, RunState::Prepared);
    assert_eq!(
        s.state
            .operations
            .values()
            .filter(|op| op.run == run.id && op.kind == "create_execution_chat")
            .count(),
        1
    );
}

#[test]
fn launch_receipt_must_identify_the_exact_worker_before_it_can_claim() {
    use bridgeforge_core::process::process_tree::ProcessTree;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let f = Fixture::new("complete");
    let s = f.store.read().unwrap();
    let (_, operation) = f
        .store
        .transact(s.revision, |s| {
            let id = s.operation(&f.run, "launch_worker")?;
            let op = s.operations.get_mut(&id).unwrap();
            op.state = OperationState::Attempted;
            op.result =
                Some(json!({"supervisor_pid":std::process::id(),"session":"test:launch-barrier"}));
            Ok(id)
        })
        .unwrap();
    let command = || {
        let mut c = Command::new(env!("CARGO_BIN_EXE_bp"));
        c.args([
            "execute",
            "--workspace",
            f.root.to_str().unwrap(),
            "--run",
            &f.run,
            "--codex",
            Fixture::executable().to_str().unwrap(),
            "--timeout-seconds",
            "5",
            "--launch-operation",
            &operation,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        c
    };
    let mut competitor = ProcessTree::spawn(&mut command()).unwrap();
    let output = competitor.child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        BufReader::new(output).read_line(&mut line).unwrap();
        tx.send(line).unwrap();
    });
    let line = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(line.contains("worker_waiting_for_launch"));
    assert_eq!(
        f.store.read().unwrap().state.runs[&f.run].state,
        RunState::Prepared
    );
    let mut actual = ProcessTree::spawn(&mut command()).unwrap();
    let pid = actual.child.id();
    let s = f.store.read().unwrap();
    f.store
        .transact(s.revision, |s| {
            let op = s.operations.get_mut(&operation).unwrap();
            op.state = OperationState::Observed;
            op.result = Some(json!({"supervisor_pid":std::process::id(),"worker_pid":pid}));
            Ok(())
        })
        .unwrap();
    assert!(actual.child.wait().unwrap().success());
    assert!(!competitor.child.wait().unwrap().success());
    let s = f.store.read().unwrap();
    assert_eq!(s.state.runs[&f.run].owner_pid, Some(pid));
    assert_eq!(s.state.runs[&f.run].state, RunState::Completed);
    let project = &s.state.runs[&f.run].binding.root;
    assert_eq!(
        fs::read_to_string(project.join(".bp-fixture-start-count"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn supervisor_budget_reclaims_a_live_worker_without_stopping_an_outside_tree() {
    use bridgeforge_core::{file_lock::FileLock, process::process_tree::ProcessTree};
    use std::process::{Command, Stdio};
    let f = Fixture::new("hang-interrupt");
    let outside_fixture = Fixture::new("complete");
    let outside_root = outside_fixture.store.read().unwrap().state.runs[&outside_fixture.run]
        .binding
        .root
        .clone();
    let mut outside_command = Command::new(Fixture::executable());
    outside_command
        .current_dir(outside_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut outside = ProcessTree::spawn(&mut outside_command).unwrap();
    let input = f.root.join(".runtime/supervise.json");
    fs::write(
        &input,
        serde_json::to_vec(&request(&["p"], 2, false)).unwrap(),
    )
    .unwrap();
    let began = Instant::now();
    let mut command = Command::new(env!("CARGO_BIN_EXE_bp"));
    command
        .args([
            "supervise",
            "--workspace",
            f.root.to_str().unwrap(),
            "--codex",
            Fixture::executable().to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut supervisor = ProcessTree::spawn(&mut command).unwrap();
    wait_for(|| f.store.read().unwrap().state.runs[&f.run].state == RunState::Running);
    let snapshot = f.store.read().unwrap();
    let run = &snapshot.state.runs[&f.run];
    let worker = run.owner_pid.unwrap();
    let server = run.server_pid.unwrap();
    // A busy archive keeps the BP worker in bookkeeping while the independent
    // supervisor Job timer must still stop it. No foreign PID is terminated.
    let path = f.root.join(".runtime/bridgepilot/locks/writer.lock");
    let lock = loop {
        match FileLock::acquire(&path) {
            Ok(lock) => break lock,
            Err(_) => thread::sleep(Duration::from_millis(5)),
        }
    };
    thread::sleep((began + Duration::from_millis(2400)).saturating_duration_since(Instant::now()));
    drop(lock);
    assert!(supervisor.child.wait().unwrap().success());
    assert!(began.elapsed() < Duration::from_secs(6));
    assert!(outside.child.try_wait().unwrap().is_none());
    let s = f.store.read().unwrap();
    assert_eq!(s.state.runs[&f.run].state, RunState::Unknown);
    assert_eq!(s.state.tasks[&f.task].state, TaskState::Unknown);
    assert_eq!(
        s.state
            .operations
            .values()
            .filter(|op| op.kind == "launch_worker")
            .count(),
        1
    );
    #[cfg(windows)]
    {
        use bridgeforge_core::{ProcessRequest, ProcessRunner, SystemProcessRunner};
        for pid in [worker, server] {
            let mut q = ProcessRequest::new("tasklist.exe", &f.root);
            q.args = vec![
                "/FI".into(),
                format!("PID eq {pid}").into(),
                "/FO".into(),
                "CSV".into(),
                "/NH".into(),
            ];
            q.timeout = Duration::from_secs(5);
            let output = SystemProcessRunner.run(&q).unwrap();
            assert_eq!(output.code, 0);
            assert!(
                !String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line
                        .split(',')
                        .nth(1)
                        .is_some_and(|v| v.trim_matches('"') == pid.to_string()))
            );
        }
    }
}

#[test]
fn report_review_rejects_stale_revision_and_incomplete_command_evidence() {
    let f = Fixture::new("complete");
    codex::execute(&f.store, &f.run, Fixture::executable(), 10).unwrap();
    let initial = f.store.read().unwrap();
    let original = initial.state.runs[&f.run].result.clone().unwrap();
    let path = initial.state.runs[&f.run].binding.root.join("AGENTS.md");
    let base = ReportReview {
        command_ids: vec![],
        desktop_grouping: fixture_grouping(&f),
        run: f.run.clone(),
        result_sha256: digest(&serde_json::to_vec(&original).unwrap()),
        source: "test:review".into(),
        note: "actual controlled evidence".into(),
        files: vec![FileEvidence {
            relative_path: "AGENTS.md".into(),
            sha256: digest(&fs::read(path).unwrap()),
        }],
    };
    assert!(review(&f.store, initial.revision - 1, base.clone()).is_err());
    for mode in ["missing", "failed", "truncated", "wrong-cwd"] {
        let snapshot = f.store.read().unwrap();
        let (mutated, _) = f
            .store
            .transact(snapshot.revision, |s| {
                let result = s.runs.get_mut(&f.run).unwrap().result.as_mut().unwrap();
                match mode {
                    "missing" => result["tool_receipts"] = json!([]),
                    "failed" => result["tool_receipts"][0]["exit_code"] = json!(1),
                    "truncated" => result["tool_receipts"][0]["output_truncated"] = json!(true),
                    _ => result["tool_receipts"][0]["cwd"] = json!(f.root),
                };
                Ok(())
            })
            .unwrap();
        let mut input = base.clone();
        input.result_sha256 = digest(
            &serde_json::to_vec(mutated.state.runs[&f.run].result.as_ref().unwrap()).unwrap(),
        );
        assert!(review(&f.store, mutated.revision, input).is_err());
        assert_eq!(
            f.store.read().unwrap().state.tasks[&f.task].state,
            TaskState::Reviewing
        );
        let snapshot = f.store.read().unwrap();
        f.store
            .transact(snapshot.revision, |s| {
                s.runs.get_mut(&f.run).unwrap().result = Some(original.clone());
                Ok(())
            })
            .unwrap();
    }
    for publishing in [false, true] {
        let snapshot = f.store.read().unwrap();
        let (changed, _) = f
            .store
            .transact(snapshot.revision, |s| {
                let t = s.tasks.get_mut(&f.task).unwrap();
                t.input.authorization.allow_write = !publishing;
                t.input.authorization.allow_publish = publishing;
                t.spec_hash = digest(&serde_json::to_vec(&t.input).unwrap());
                s.runs.get_mut(&f.run).unwrap().spec_hash = t.spec_hash.clone();
                Ok(())
            })
            .unwrap();
        assert!(review(&f.store, changed.revision, base.clone()).is_err());
    }
}

#[test]
fn review_selects_actual_successful_receipts_without_erasing_failed_history() {
    let f = Fixture::new("complete");
    codex::execute(&f.store, &f.run, Fixture::executable(), 10).unwrap();
    let initial = f.store.read().unwrap();
    let (snapshot, ()) = f
        .store
        .transact(initial.revision, |s| {
            let result = s.runs.get_mut(&f.run).unwrap().result.as_mut().unwrap();
            let mut failed = result["tool_receipts"][0].clone();
            failed["id"] = json!("earlier-failed-diagnostic");
            failed["exit_code"] = json!(1);
            failed["output_truncated"] = json!(true);
            result["tool_receipts"].as_array_mut().unwrap().push(failed);
            Ok(())
        })
        .unwrap();
    let r = &snapshot.state.runs[&f.run];
    let result = r.result.as_ref().unwrap();
    let valid = result["tool_receipts"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut input = ReportReview {
        run: f.run.clone(),
        result_sha256: digest(&serde_json::to_vec(result).unwrap()),
        source: "test:actual-review".into(),
        note: "recovered command selected; failed diagnostic retained".into(),
        desktop_grouping: fixture_grouping(&f),
        files: vec![FileEvidence {
            relative_path: "AGENTS.md".into(),
            sha256: digest(&fs::read(r.binding.root.join("AGENTS.md")).unwrap()),
        }],
        command_ids: vec![],
    };
    assert!(review(&f.store, snapshot.revision, input.clone()).is_err());
    input.command_ids = vec!["invented".into()];
    assert!(review(&f.store, snapshot.revision, input.clone()).is_err());
    input.command_ids = vec![valid.clone(), valid.clone()];
    assert!(review(&f.store, snapshot.revision, input.clone()).is_err());
    input.command_ids = vec![valid];
    review(&f.store, snapshot.revision, input).unwrap();
    let final_state = f.store.read().unwrap();
    let result = final_state.state.runs[&f.run].result.as_ref().unwrap();
    assert_eq!(result["tool_receipts"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["review"]["excluded_command_ids"],
        json!(["earlier-failed-diagnostic"])
    );
}

#[test]
fn native_handoff_rejects_identity_mismatch_duplicate_restore_and_pause_race() {
    for mode in ["complete", "wrong-unarchive", "pause-unarchive"] {
        let f = Fixture::new(mode);
        let snapshot = f.store.read().unwrap();
        f.store
            .transact(snapshot.revision, |s| {
                s.operations
                    .retain(|_, o| o.kind != "prepare_execution_chat");
                s.ensure_chat_preparation(&f.run)?;
                Ok(())
            })
            .unwrap();
        let root = f.root.clone();
        let run = f.run.clone();
        let call = thread::spawn(move || {
            codex::restore_chat(
                &Store::open(&root).unwrap(),
                &run,
                Fixture::executable(),
                "test:native-archive-ack",
            )
        });
        if mode == "pause-unarchive" {
            let path = f.store.read().unwrap().state.runs[&f.run]
                .binding
                .root
                .clone();
            wait_for(|| path.join(".bp-fixture-restoring").exists());
            f.store.update(|s| s.pause(false, &["p".into()])).unwrap();
            fs::write(path.join(".bp-fixture-restore-release"), "1").unwrap();
        }
        let result = call.join().unwrap();
        assert_eq!(result.is_ok(), mode == "complete");
        assert_eq!(
            f.store.read().unwrap().state.chat_ready(&f.run),
            mode == "complete"
        );
        assert!(
            codex::restore_chat(&f.store, &f.run, Fixture::executable(), "test:repeat-ack")
                .is_err()
        );
        assert!(
            !f.store.read().unwrap().state.runs[&f.run]
                .binding
                .root
                .join(".bp-fixture-started")
                .exists()
        );
    }
}

#[test]
fn direct_execution_cannot_bypass_native_handoff_or_race_a_restore() {
    for state in [
        OperationState::Intent,
        OperationState::Attempted,
        OperationState::Unknown,
    ] {
        let f = Fixture::new("complete");
        f.store
            .update(|s| {
                let op = s
                    .operations
                    .values_mut()
                    .find(|o| o.run == f.run && o.kind == "prepare_execution_chat")
                    .unwrap();
                op.state = state;
                Ok(())
            })
            .unwrap();
        let before = f.store.read().unwrap();
        assert!(codex::execute(&f.store, &f.run, Fixture::executable(), 10).is_err());
        let after = f.store.read().unwrap();
        assert_eq!(before.revision, after.revision);
        assert!(
            !after.state.runs[&f.run]
                .binding
                .root
                .join(".bp-fixture-pid")
                .exists()
        );
        assert!(
            !after.state.runs[&f.run]
                .binding
                .root
                .join(".bp-fixture-started")
                .exists()
        );
    }
}

#[test]
fn report_can_be_reviewed_but_not_delivered_without_actual_project_readback() {
    let f = Fixture::new("complete");
    codex::execute(&f.store, &f.run, Fixture::executable(), 10).unwrap();
    // Exercise genuinely missing readback; normal execution now obtains the
    // native persisted grouping receipt automatically before starting a turn.
    f.store
        .update(|s| {
            s.operations
                .retain(|_, o| o.run != f.run || o.kind != "assign_execution_project");
            Ok(())
        })
        .unwrap();
    let s = f.store.read().unwrap();
    let r = &s.state.runs[&f.run];
    let mut input = ReportReview {
        run: f.run.clone(),
        result_sha256: digest(&serde_json::to_vec(r.result.as_ref().unwrap()).unwrap()),
        source: "test:independent-review".into(),
        note: "native move ACK exists but project readback is null".into(),
        files: vec![FileEvidence {
            relative_path: "AGENTS.md".into(),
            sha256: digest(&fs::read(r.binding.root.join("AGENTS.md")).unwrap()),
        }],
        command_ids: vec![],
        desktop_grouping: None,
    };
    let report = review(&f.store, s.revision, input.clone()).unwrap();
    assert_eq!(report["status"], "report_reviewed");
    let s = f.store.read().unwrap();
    assert_eq!(s.state.tasks[&f.task].state, TaskState::Reviewing);
    input.result_sha256 =
        digest(&serde_json::to_vec(s.state.runs[&f.run].result.as_ref().unwrap()).unwrap());
    input.desktop_grouping = fixture_grouping(&f);
    input.desktop_grouping.as_mut().unwrap().project_id = "another-project".into();
    assert!(review(&f.store, s.revision, input.clone()).is_err());
    input.desktop_grouping = fixture_grouping(&f);
    let result = review(&f.store, s.revision, input).unwrap();
    assert_eq!(result["status"], "report_delivered");
}

#[cfg(windows)]
#[test]
fn reconcile_keeps_legacy_pid_guard_but_distinguishes_reused_process_birth() {
    let f = Fixture::new("complete");
    let op = f
        .store
        .update(|s| {
            let r = s.runs.get_mut(&f.run).unwrap();
            r.state = RunState::Unknown;
            r.owner_pid = Some(std::process::id());
            s.tasks.get_mut(&f.task).unwrap().state = TaskState::Unknown;
            let op = s.operation(&f.run, "resume_execution")?;
            s.operations.get_mut(&op).unwrap().state = OperationState::Unknown;
            Ok(op)
        })
        .unwrap();
    let before = f.store.read().unwrap();
    assert!(codex::reconcile(&f.store, &f.run, Fixture::executable()).is_err());
    assert_eq!(before.revision, f.store.read().unwrap().revision);
    f.store.update(|s| {
        s.operations.get_mut(&op).unwrap().result=Some(json!({"owner_pid":std::process::id(),"owner_identity":"win-filetime-0000000000000000"}));Ok(())
    }).unwrap();
    let result = codex::reconcile(&f.store, &f.run, Fixture::executable()).unwrap();
    assert_eq!(result["status"], "reconciled_before_start");
    assert_eq!(
        f.store.read().unwrap().state.runs[&f.run].state,
        RunState::Interrupted
    );
}
