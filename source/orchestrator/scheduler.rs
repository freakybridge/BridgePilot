//! Bounded resident dispatch. Native chat creation remains a client action.
use crate::{
    Result,
    domain::{OperationState, RunState, TaskState},
    new_id,
    owned::OwnedProcess,
    store::{Store, ordinary},
};
use bridgeforge_core::{file_lock::FileLock, persistence};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Supervision {
    pub projects: Vec<String>,
    pub max_parallel: usize,
    pub timeout_seconds: u64,
    #[serde(default)]
    pub stop_when_idle: bool,
}
impl Supervision {
    pub fn validate(&self, store: &Store) -> Result<()> {
        let snapshot = store.read()?;
        let mut seen = std::collections::BTreeSet::new();
        if self.projects.is_empty()
            || self.max_parallel == 0
            || self.max_parallel > 8
            || self.timeout_seconds == 0
            || self.timeout_seconds > 86400
        {
            return Err(
                "explicit projects, parallelism 1..8 and time limit 1..86400 required".into(),
            );
        }
        let machine = store.machine()?;
        for id in &self.projects {
            if !seen.insert(id)
                || !snapshot.state.projects.contains_key(id)
                || !snapshot
                    .state
                    .bindings
                    .contains_key(&format!("{machine}:{id}"))
            {
                return Err("unknown, duplicate or unbound supervision project".into());
            }
        }
        Ok(())
    }
}

struct Worker {
    run: String,
    project: String,
    operation: String,
    process: OwnedProcess,
    out: Arc<Mutex<Vec<u8>>>,
    err: Arc<Mutex<Vec<u8>>>,
}
fn drain(mut stream: impl Read + Send + 'static) -> Arc<Mutex<Vec<u8>>> {
    let saved = Arc::new(Mutex::new(Vec::new()));
    let sink = saved.clone();
    thread::spawn(move || {
        let mut bytes = [0; 4096];
        while let Ok(n) = stream.read(&mut bytes) {
            if n == 0 {
                break;
            }
            if let Ok(mut tail) = sink.lock() {
                tail.extend_from_slice(&bytes[..n]);
                if tail.len() > 65536 {
                    let excess = tail.len() - 65536;
                    tail.drain(..excess);
                }
            }
        }
    });
    saved
}
fn tail(value: &Arc<Mutex<Vec<u8>>>) -> String {
    value
        .lock()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .unwrap_or_default()
}
fn event(value: Value) {
    println!("{value}");
}

fn unresolved(store: &Store, run: &str, pid: Option<u32>, reason: &str) -> Result<()> {
    store.update(|s| {
        let r = s.runs.get_mut(run).ok_or("missing owned run")?;
        // A caller which claimed this run independently is never overwritten.
        if r.state.active() && (r.owner_pid.is_none() || r.owner_pid == pid) {
            r.state = RunState::Unknown;
            let mut result = r.result.take().unwrap_or_else(|| json!({}));
            if !result.is_object() {
                result = json!({"previous_result":result});
            }
            result["supervisor_diagnostic"] = json!({"reason":reason,"worker_pid":pid});
            r.result = Some(result);
            s.tasks.get_mut(&r.task).unwrap().state = TaskState::Unknown;
            for op in s
                .operations
                .values_mut()
                .filter(|op| op.run == run && op.state == OperationState::Attempted)
            {
                op.state = OperationState::Unknown;
            }
        }
        Ok(())
    })
}

fn spawn_worker(
    store: &Store,
    program: &Path,
    codex: &Path,
    run: &str,
    session: &str,
    deadline: Instant,
) -> Result<Worker> {
    let remaining = deadline.saturating_duration_since(Instant::now()).as_secs();
    if remaining < 1 {
        return Err("supervision time limit reached before launch".into());
    }
    let machine = store.machine()?;
    let (project, operation) = store.update(|s| {
        let r = s.runs.get(run).ok_or("unknown run")?;
        let t = &s.tasks[&r.task];
        if r.state != RunState::Prepared
            || r.machine != machine
            || r.thread_id.is_none()
            || !s.chat_ready(run)
            || t.state != TaskState::Ready
            || s.projects[&t.input.project].paused
            || s.operations
                .values()
                .any(|op| op.run == run && op.kind == "launch_worker")
        {
            return Err("run not launchable or a worker was already attempted".into());
        }
        let project = t.input.project.clone();
        let op = s.operation(run, "launch_worker")?;
        let o = s.operations.get_mut(&op).unwrap();
        o.state = OperationState::Attempted;
        o.result = Some(json!({"session":session,"supervisor_pid":std::process::id()}));
        Ok((project, op))
    })?;
    let mut command = Command::new(program);
    command
        .args(["execute", "--workspace"])
        .arg(&store.workspace)
        .args(["--run", run, "--codex"])
        .arg(codex)
        .args([
            "--timeout-seconds",
            &remaining.to_string(),
            "--launch-operation",
            &operation,
        ])
        .current_dir(&store.workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let process = match OwnedProcess::spawn(&mut command, Some(deadline)) {
        Ok(p) => p,
        Err(error) => {
            unresolved(
                store,
                run,
                None,
                &format!("spawn result requires reconciliation: {error}"),
            )?;
            return Err(error);
        }
    };
    let pid = process.pid();
    let pipes = process.pipes()?;
    drop(pipes.input);
    let out = drain(pipes.output);
    let err = drain(pipes.errors);
    if let Err(error) = store.update(|s| {
        let o = s
            .operations
            .get_mut(&operation)
            .ok_or("missing worker launch")?;
        o.state = OperationState::Observed;
        o.result =
            Some(json!({"session":session,"supervisor_pid":std::process::id(),"worker_pid":pid,"worker_identity":process.identity()}));
        Ok(())
    }) {
        drop(process);
        let _ = unresolved(
            store,
            run,
            Some(pid),
            "launch occurred but receipt could not be committed",
        );
        return Err(error);
    }
    event(
        json!({"event":"worker_started","session":session,"run":run,"project":project,"pid":pid}),
    );
    Ok(Worker {
        run: run.into(),
        project,
        operation,
        process,
        out,
        err,
    })
}

/// Callers must use the actual BP executable; production CLI supplies current_exe.
pub fn supervise(
    store: &Store,
    codex: &Path,
    program: &Path,
    request: Supervision,
) -> Result<Value> {
    let started = Instant::now();
    request.validate(store)?;
    for executable in [codex, program] {
        ordinary(executable)?;
        if !executable.is_absolute() || !executable.is_file() {
            return Err("explicit existing absolute executable required".into());
        }
    }
    let deadline = started + Duration::from_secs(request.timeout_seconds);
    let runtime = store.workspace.join(".runtime/bridgepilot");
    ordinary(&runtime)?;
    let _lock = FileLock::acquire(&runtime.join("locks/supervisor.lock"))?;
    let session = new_id("supervisor");
    let machine = store.machine()?;
    let status_path = runtime.join("supervisor.json");
    ordinary(&status_path)?;
    persistence::atomic_write_json(&status_path,&json!({"schema":1,"session":session,"pid":std::process::id(),"machine":machine,"projects":request.projects,"state":"running","time_limit_seconds":request.timeout_seconds})).map_err(|e|e.to_string())?;
    let mut workers: Vec<Worker> = Vec::new();
    let mut launched = Vec::new();
    let mut finished = Vec::new();
    let mut cursor = 0;
    let mut blocked = BTreeMap::new();
    let mut remembered = BTreeMap::new();
    event(
        json!({"event":"supervision_started","session":session,"projects":request.projects,"max_parallel":request.max_parallel}),
    );
    let outcome = (|| -> Result<&str> {
        loop {
            let mut index = 0;
            while index < workers.len() {
                if let Some(exit) = workers[index].process.try_wait()? {
                    let worker = workers.remove(index);
                    let pid = worker.process.pid();
                    drop(worker.process);
                    unresolved(
                        store,
                        &worker.run,
                        Some(pid),
                        "worker exited without a durable terminal result",
                    )?;
                    let state = store.read()?.state.runs[&worker.run].state;
                    event(
                        json!({"event":"worker_finished","session":session,"run":worker.run,"project":worker.project,"exit_code":exit.code(),"run_state":state}),
                    );
                    finished.push(json!({"run":worker.run,"project":worker.project,"operation":worker.operation,"pid":pid,"exit_code":exit.code(),"state":state,"stdout_tail":tail(&worker.out),"stderr_tail":tail(&worker.err)}));
                } else {
                    index += 1;
                }
            }
            if Instant::now() >= deadline {
                return Ok("time_limit");
            }
            // Observe new tasks and binding receipts every tick, without a new
            // user prompt or a second project writer. Project order rotates.
            blocked.clear();
            for offset in 0..request.projects.len() {
                let project = &request.projects[(cursor + offset) % request.projects.len()];
                if workers.iter().any(|w| &w.project == project) {
                    continue;
                }
                let snapshot = store.read()?;
                if snapshot.state.projects[project].paused {
                    continue;
                }
                let active = snapshot.state.runs.values().find(|r| {
                    r.state.active() && snapshot.state.tasks[&r.task].input.project == *project
                });
                if let Some(run) = active {
                    if run.machine == machine
                        && run.state == RunState::Prepared
                        && run.thread_id.is_some()
                        && !snapshot.state.chat_ready(&run.id)
                    {
                        if !snapshot
                            .state
                            .operations
                            .values()
                            .any(|op| op.run == run.id && op.kind == "prepare_execution_chat")
                        {
                            store.update(|s| s.ensure_chat_preparation(&run.id))?;
                        }
                        blocked.insert(project.clone(), "native chat handoff awaits client archive acknowledgement and data-only restore".into());
                        continue;
                    }
                    if run.machine == machine
                        && run.state == RunState::Prepared
                        && run.thread_id.is_some()
                        && !snapshot
                            .state
                            .operations
                            .values()
                            .any(|op| op.run == run.id && op.kind == "launch_worker")
                        && workers.len() < request.max_parallel
                    {
                        match spawn_worker(store, program, codex, &run.id, &session, deadline) {
                            Ok(worker) => {
                                launched.push(worker.run.clone());
                                workers.push(worker);
                            }
                            Err(error) => {
                                blocked.insert(project.clone(), error);
                            }
                        }
                    } else if run.state != RunState::Prepared
                        || run.machine != machine
                        || run.thread_id.is_some()
                    {
                        blocked.insert(
                            project.clone(),
                            "existing active/unknown execution is not adopted".into(),
                        );
                    }
                    continue;
                }
                if workers.len() >= request.max_parallel {
                    continue;
                }
                let ready = snapshot.state.tasks.values().find(|t| {
                    t.input.project == *project
                        && t.state == TaskState::Ready
                        && t.input
                            .dependencies
                            .iter()
                            .all(|dep| snapshot.state.tasks[dep].state.delivered())
                });
                if let Some(task) = ready {
                    match store.update(|s| s.prepare_dispatch(&task.id, &machine)) {
                        Ok((run, operation)) => {
                            event(
                                json!({"event":"run_prepared","session":session,"project":project,"run":run,"client_operation":operation}),
                            );
                            if operation.is_none() {
                                match spawn_worker(store, program, codex, &run, &session, deadline)
                                {
                                    Ok(worker) => {
                                        launched.push(worker.run.clone());
                                        workers.push(worker);
                                    }
                                    Err(error) => {
                                        blocked.insert(project.clone(), error);
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            blocked.insert(project.clone(), error);
                        }
                    }
                }
            }
            cursor = (cursor + 1) % request.projects.len();
            if blocked != remembered {
                for (project, reason) in &blocked {
                    event(json!({"event":"project_blocked","project":project,"reason":reason}));
                }
                remembered = blocked.clone();
            }
            let snapshot = store.read()?;
            let waiting = snapshot.state.runs.values().any(|r| {
                r.machine == machine
                    && r.state == RunState::Prepared
                    && request
                        .projects
                        .contains(&snapshot.state.tasks[&r.task].input.project)
            });
            let ready = snapshot.state.tasks.values().any(|t| {
                request.projects.contains(&t.input.project)
                    && t.state == TaskState::Ready
                    && !snapshot.state.projects[&t.input.project].paused
                    && t.input.authorization.allow_execution_chat
                    && t.input
                        .dependencies
                        .iter()
                        .all(|dep| snapshot.state.tasks[dep].state.delivered())
                    && !snapshot.state.runs.values().any(|r| {
                        r.state.active()
                            && snapshot.state.tasks[&r.task].input.project == t.input.project
                    })
            });
            if request.stop_when_idle && workers.is_empty() && !waiting && !ready {
                return Ok("idle");
            }
            thread::sleep(
                Duration::from_millis(200).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    })();
    // Tear down only processes spawned here. Never turn an unobserved stop into
    // Paused; durable unresolved runs block subsequent launches until reconcile.
    for worker in workers.drain(..) {
        let pid = worker.process.pid();
        drop(worker.process);
        let _ = unresolved(
            store,
            &worker.run,
            Some(pid),
            "supervisor stopped before worker terminal acknowledgement",
        );
    }
    let reason = outcome.as_ref().copied().unwrap_or("error");
    let receipt = json!({"schema":1,"status":"supervision_stopped","session":session,"reason":reason,"error":outcome.as_ref().err(),"projects":request.projects,"launched":launched,"finished":finished,"blocked":blocked,"elapsed_millis":started.elapsed().as_millis(),"automatic_wakeup":false,"publication_verified":false});
    persistence::atomic_write_json(&status_path, &receipt).map_err(|e| e.to_string())?;
    outcome?;
    Ok(receipt)
}
