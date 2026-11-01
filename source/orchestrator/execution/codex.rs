//! Owned stdio execution. Desktop chat creation/grouping is a client action,
//! never an undocumented desktop database write or a second execution backend.
use crate::owned::OwnedProcess;
use crate::{
    Result,
    domain::{OperationState, RunState, TaskState},
    store::Store,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

pub struct Rpc {
    process: OwnedProcess,
    deadline: Option<Instant>,
    input: ChildStdin,
    messages: mpsc::Receiver<Result<Value>>,
    pending: VecDeque<Value>,
    next: u64,
    stderr: Arc<Mutex<Vec<u8>>>,
}

fn emit(value: &Value) {
    println!("{value}");
    let _ = std::io::stdout().flush();
}

impl Rpc {
    pub fn start(executable: &Path, cwd: &Path) -> Result<Self> {
        Self::start_limited(executable, cwd, None)
    }
    pub(crate) fn start_limited(
        executable: &Path,
        cwd: &Path,
        deadline: Option<Instant>,
    ) -> Result<Self> {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Err("execution time limit reached before spawn".into());
        }
        if !executable.is_absolute() || !executable.is_file() {
            return Err("explicit absolute Codex executable required".into());
        }
        crate::store::ordinary(executable)?;
        crate::store::ordinary(cwd)?;
        let mut command = Command::new(executable);
        command
            .args(["app-server", "--listen", "stdio://"])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let process = OwnedProcess::spawn(&mut command, deadline)?;
        let crate::owned::Pipes {
            input,
            output,
            mut errors,
        } = process.pipes()?;
        let (send, messages) = mpsc::sync_channel(256);
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let value = line.map_err(|e| e.to_string()).and_then(|line| {
                    if line.len() > 4 * 1024 * 1024 {
                        return Err("oversized App Server message".into());
                    }
                    serde_json::from_str(&line).map_err(|e| format!("invalid App Server JSON: {e}"))
                });
                if send.send(value).is_err() {
                    break;
                }
            }
        });
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let saved = stderr.clone();
        thread::spawn(move || {
            let mut buffer = [0u8; 2048];
            while let Ok(n) = errors.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                if let Ok(mut bytes) = saved.lock() {
                    bytes.extend_from_slice(&buffer[..n]);
                    if bytes.len() > 8192 {
                        let excess = bytes.len() - 8192;
                        bytes.drain(..excess);
                    }
                }
            }
        });
        let mut rpc = Self {
            process,
            deadline,
            input,
            messages,
            pending: VecDeque::new(),
            next: 1,
            stderr,
        };
        rpc.call("initialize", json!({"clientInfo":{"name":"bridgepilot","title":"BridgePilot","version":crate::program_version()},"capabilities":{"experimentalApi":true}}))?;
        rpc.send(&json!({"method":"initialized"}))?;
        Ok(rpc)
    }

    pub fn pid(&self) -> u32 {
        self.process.pid()
    }
    fn send(&mut self, value: &Value) -> Result<()> {
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err("execution time limit reached; outcome requires reconciliation".into());
        }
        writeln!(self.input, "{value}")
            .and_then(|_| self.input.flush())
            .map_err(|e| e.to_string())
    }
    fn next_wire(&self, timeout: Duration) -> Result<Option<Value>> {
        let result = match self.messages.recv_timeout(timeout) {
            Ok(value) => value.map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(_) => {
                let stderr = self
                    .stderr
                    .lock()
                    .map(|v| String::from_utf8_lossy(&v).into_owned())
                    .unwrap_or_default();
                Err(format!("App Server connection ended: {stderr}"))
            }
        };
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err("execution time limit reached before response was observed".into());
        }
        result
    }
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"id":id,"method":method,"params":params}))?;
        let deadline = (Instant::now() + Duration::from_secs(45)).min(
            self.deadline
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(45)),
        );
        while Instant::now() < deadline {
            let Some(message) = self.next_wire(
                Duration::from_millis(250).min(deadline.saturating_duration_since(Instant::now())),
            )?
            else {
                continue;
            };
            if message.get("method").is_none() && message["id"] == id {
                if !message["error"].is_null() {
                    return Err(format!("{method}: {}", message["error"]));
                }
                let result = message["result"].clone();
                if Instant::now() >= deadline {
                    return Err(format!(
                        "{method}: response observed after deadline; outcome unknown"
                    ));
                }
                return Ok(result);
            }
            if message.get("method").is_some() && message.get("id").is_some() {
                self.send(&json!({"id":message["id"],"error":{"code":-32601,"message":"approval must be handled by the user-facing client"}}))?;
                return Err(format!(
                    "client action required during {method}: {}",
                    message["method"]
                ));
            }
            if self.pending.len() >= 1024 {
                return Err("App Server notification queue exceeded limit".into());
            }
            self.pending.push_back(message);
        }
        Err(format!("{method}: response timeout; outcome unknown"))
    }
    fn event(&mut self) -> Result<Option<Value>> {
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err("execution time limit reached before terminal acknowledgement".into());
        }
        if let Some(value) = self.pending.pop_front() {
            if self.deadline.is_some_and(|d| Instant::now() >= d) {
                return Err("execution time limit reached before queued event was observed".into());
            }
            return Ok(Some(value));
        }
        self.next_wire(Duration::from_millis(250))
    }
    fn decline(&mut self, request: &Value) -> Result<()> {
        let method = request["method"].as_str().unwrap_or("");
        if method.ends_with("requestApproval") {
            self.send(&json!({"id":request["id"],"result":{"decision":"decline"}}))
        } else {
            self.send(&json!({"id":request["id"],"error":{"code":-32601,"message":"BridgePilot client action unavailable"}}))
        }
    }
}

fn update<T>(
    store: &Store,
    change: impl FnMut(&mut crate::domain::State) -> Result<T>,
) -> Result<T> {
    store.update(change)
}

/// Observe an explicitly registered existing thread without starting/resuming a
/// turn or adopting its writer. Native completion remains an observation only.
pub fn observe_task(
    store: &Store,
    expected: u64,
    id: &str,
    executable: &Path,
    source: &str,
) -> Result<Value> {
    observe_task_until(
        store,
        expected,
        id,
        executable,
        source,
        Instant::now() + Duration::from_secs(20),
    )
}
pub(crate) fn observe_task_until(
    store: &Store,
    expected: u64,
    id: &str,
    executable: &Path,
    source: &str,
    deadline: Instant,
) -> Result<Value> {
    let result = observe_task_inner(store, expected, id, executable, source, deadline);
    if let Err(error) = &result {
        let _ = crate::tracking::observation_failed(store, expected, id, error, source);
    }
    result
}
fn observe_task_inner(
    store: &Store,
    expected: u64,
    id: &str,
    executable: &Path,
    source: &str,
    deadline: Instant,
) -> Result<Value> {
    if source.trim().is_empty() {
        return Err("actual observation source is required".into());
    }
    let before = store.read()?;
    if before.revision != expected {
        return Err("archive revision changed before thread observation".into());
    }
    let task = before.state.tasks.get(id).ok_or("unknown observed task")?;
    let meta = before
        .state
        .tracking
        .get(id)
        .ok_or("task has no explicit thread binding")?;
    if meta.mode != crate::tracking::ExecutionMode::Observed {
        return Err("task is not registered in observed-thread mode".into());
    }
    let thread = meta
        .execution_thread_id
        .as_ref()
        .ok_or("observed thread ID missing")?;
    if before
        .state
        .runs
        .values()
        .any(|r| r.task == id && r.state.active())
    {
        return Err("cannot observe/adopt an existing owned active execution".into());
    }
    let mut rpc = Rpc::start_limited(executable, &store.workspace, Some(deadline))?;
    let response = rpc.call(
        "thread/read",
        json!({"threadId":thread,"includeTurns":true}),
    )?;
    let native = &response["thread"];
    if native["id"].as_str() != Some(thread.as_str()) {
        return Err("native observation returned another thread identity".into());
    }
    if meta.controller_thread_id.as_ref() != Some(thread) {
        let machine = store.machine()?;
        let binding = before
            .state
            .bindings
            .get(&format!("{machine}:{}", task.input.project))
            .ok_or("observed project binding missing")?;
        let cwd = native["cwd"]
            .as_str()
            .ok_or("native observed thread omitted cwd")?;
        if std::fs::canonicalize(cwd).map_err(|e| e.to_string())?
            != std::fs::canonicalize(&binding.root).map_err(|e| e.to_string())?
        {
            return Err("observed execution thread is outside the registered project".into());
        }
    }
    let turns = native["turns"]
        .as_array()
        .ok_or("native observation omitted actual turn history")?;
    let latest = turns
        .iter()
        .find(|t| t["status"] == "inProgress")
        .or_else(|| turns.last());
    let observed = latest.and_then(|t| t["status"].as_str()).unwrap_or("idle");
    let observed = if matches!(
        observed,
        "inProgress" | "completed" | "interrupted" | "failed" | "idle"
    ) {
        observed
    } else {
        "unknown"
    };
    // A separate server's notLoaded status cannot establish the desktop
    // writer's current turn. Its persisted last turn may already be outdated.
    let thread_status = native["status"]["type"].as_str().unwrap_or("unknown");
    let goal = if thread_status != "active" {
        rpc.call("thread/goal/get", json!({"threadId":thread})).ok()
    } else {
        None
    };
    let goal_active = goal
        .as_ref()
        .is_some_and(|g| g["goal"]["status"] == "active");
    let recorded_status = observed;
    let observed = if thread_status == "active" && recorded_status == "inProgress" {
        observed
    } else {
        "unknown"
    };
    let turn = latest.and_then(|t| t["id"].as_str()).map(str::to_owned);
    let commands = latest
        .and_then(|t| t["items"].as_array())
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "commandExecution")
        .collect::<Vec<_>>();
    let completed = commands
        .iter()
        .filter(|item| item["exitCode"].as_i64().is_some())
        .count();
    let failed = commands
        .iter()
        .filter(|item| item["exitCode"].as_i64().is_some_and(|n| n != 0))
        .count();
    let proof = json!({"thread_id":thread,"cwd":native["cwd"],"turn_id":turn,"turn_status":observed,
        "last_recorded_turn_status":recorded_status,"thread_status":thread_status,"goal_status":goal.as_ref().map(|g|&g["goal"]["status"]),
        "command_receipts":completed,"failed_command_receipts":failed,
        "native_readback_sha256":crate::digest(&serde_json::to_vec(&response).map_err(|e|e.to_string())?),"source":source});
    let event = crate::tracking::Event::native(
        task,
        None,
        &format!(
            "{}:{expected}",
            crate::digest(&serde_json::to_vec(&proof).map_err(|e| e.to_string())?)
        ),
        if goal_active {
            "goal_observed_active"
        } else {
            "thread_observed"
        },
        format!(
            "{}；末次持久回合：{recorded_status}；已读回 {completed} 条命令回执，{failed} 条非零退出（业务完成待核验）",
            if goal_active {
                "原生托管目标进行中"
            } else if observed == "unknown" {
                "当前执行回合观察受限"
            } else {
                "已读回关联回合"
            }
        ),
        (observed == "unknown")
            .then(|| "独立原生服务未读到桌面当前回合，仅能核对目标及末次历史记录".into()),
        format!("codex://threads/{thread}"),
        proof.clone(),
    );
    if let Some(latest) = &meta.latest {
        if crate::tracking::latest_receipt_matches(
            &store.read_connection()?,
            &event,
            &latest.event_id,
        )? {
            let current = store.read()?;
            if current.revision != expected {
                return Err("archive revision changed during thread observation".into());
            }
            return Ok(
                json!({"schema":1,"status":"task_observed","revision":current.revision,"proof":proof,"unchanged":true,"task_turn_started":false,"business_verified":false}),
            );
        }
    }
    let (snapshot, ()) = store.transact_with_sql(expected, |s, tx| {
        let m = s
            .tracking
            .get_mut(id)
            .ok_or("observed task binding disappeared")?;
        if m.execution_thread_id.as_ref() != Some(thread) {
            return Err("observed task binding changed".into());
        }
        m.observation = Some(crate::tracking::Observation {
            thread_id: thread.clone(),
            turn_id: turn,
            status: observed.into(),
            observed_at: crate::tracking::now(),
            source: source.into(),
        });
        m.observation_error = None;
        crate::tracking::append(s, tx, &event)?;
        Ok(())
    })?;
    Ok(
        json!({"schema":1,"status":"task_observed","revision":snapshot.revision,"proof":proof,"task_turn_started":false,"business_verified":false}),
    )
}

/// Execute only a previously created, idle, correctly grouped native chat.
/// A missing UI binding produces a durable client action instead of a hidden chat.
pub fn execute(store: &Store, run_id: &str, executable: &Path, timeout: u64) -> Result<Value> {
    execute_with_launch(store, run_id, executable, timeout, None)
}
pub fn execute_with_launch(
    store: &Store,
    run_id: &str,
    executable: &Path,
    timeout: u64,
    launch: Option<&str>,
) -> Result<Value> {
    let started = Instant::now();
    if timeout == 0 || timeout > 86400 {
        return Err("execution timeout must be 1..86400 seconds".into());
    }
    let hard_deadline = started + Duration::from_secs(timeout);
    let stop_at = hard_deadline - (Duration::from_secs(timeout) / 5).min(Duration::from_secs(10));
    let machine = store.machine()?;
    let snapshot = store.read()?;
    let run = snapshot
        .state
        .runs
        .get(run_id)
        .ok_or("unknown run")?
        .clone();
    let task = snapshot
        .state
        .tasks
        .get(&run.task)
        .ok_or("unknown task")?
        .clone();
    let binding = run.binding.clone();
    let current = snapshot
        .state
        .bindings
        .get(&format!("{}:{}", machine, task.input.project))
        .ok_or("missing local binding")?;
    if current.root != binding.root || current.desktop_project_id != binding.desktop_project_id {
        return Err("run project binding changed; explicit rebind required".into());
    }
    if run.machine != machine
        || run.state != RunState::Prepared
        || !snapshot.state.chat_ready(run_id)
        || task.state != TaskState::Ready
        || snapshot.state.projects[&task.input.project].paused
    {
        return Err("run is not locally ready; no implicit takeover or duplicate launch".into());
    }
    let thread_id = run
        .thread_id
        .clone()
        .ok_or("execution chat required: fulfill the create_execution_chat client action first")?;
    if run
        .desktop_binding_source
        .as_deref()
        .is_none_or(|s| s.trim().is_empty())
    {
        return Err("desktop project binding has not been acknowledged".into());
    }
    if let Some(operation) = launch {
        emit(&json!({"event":"worker_waiting_for_launch","run":run_id,"operation":operation}));
        loop {
            let current = store.read()?;
            let op = current
                .state
                .operations
                .get(operation)
                .ok_or("missing worker launch receipt")?;
            if op.run != run_id || op.kind != "launch_worker" {
                return Err("worker launch operation does not match this run".into());
            }
            match op.state {
                OperationState::Observed => {
                    if op.result.as_ref().and_then(|v| v["worker_pid"].as_u64())
                        != Some(std::process::id() as u64)
                    {
                        return Err("worker launch receipt belongs to a different process".into());
                    }
                    if let Some(identity) = op
                        .result
                        .as_ref()
                        .and_then(|v| v["worker_identity"].as_str())
                    {
                        if crate::owned::current_identity()?.as_deref() != Some(identity) {
                            return Err(
                                "worker launch birth identity does not match this process".into()
                            );
                        }
                    }
                    break;
                }
                OperationState::Attempted if Instant::now() < stop_at => {
                    thread::sleep(Duration::from_millis(20))
                }
                _ => {
                    return Err(
                        "worker launch receipt is unresolved or its time limit expired".into(),
                    );
                }
            }
        }
    }
    let owner_identity = crate::owned::current_identity()?;
    let op = update(store, |s| {
        if Instant::now() >= stop_at {
            return Err("execution time limit exhausted before claim".into());
        }
        if !s.chat_ready(run_id) {
            return Err("native chat handoff must be observed before execution claim".into());
        }
        let launches: Vec<_> = s
            .operations
            .values()
            .filter(|op| op.run == run_id && op.kind == "launch_worker")
            .collect();
        if !launches.is_empty() || launch.is_some() {
            if launches.len() != 1
                || launch != Some(launches[0].id.as_str())
                || launches[0].state != OperationState::Observed
                || launches[0]
                    .result
                    .as_ref()
                    .and_then(|v| v["worker_pid"].as_u64())
                    != Some(std::process::id() as u64)
            {
                return Err("worker launch is owned by a different supervisor operation".into());
            }
            if let Some(identity) = launches[0]
                .result
                .as_ref()
                .and_then(|v| v["worker_identity"].as_str())
            {
                if owner_identity.as_deref() != Some(identity) {
                    return Err("worker launch birth identity changed".into());
                }
            }
        }
        let r = s.runs.get_mut(run_id).ok_or("unknown run")?;
        if r.state != RunState::Prepared {
            return Err("run launch already attempted".into());
        }
        r.state = RunState::BindingRequired;
        r.owner_pid = Some(std::process::id());
        let op = s.operation(run_id, "resume_execution")?;
        let operation = s.operations.get_mut(&op).unwrap();
        operation.state = OperationState::Attempted;
        operation.result =
            Some(json!({"owner_pid":std::process::id(),"owner_identity":owner_identity}));
        Ok(op)
    })?;
    let result = execute_inner(
        store,
        run_id,
        &thread_id,
        &task,
        &binding,
        executable,
        started,
        stop_at,
        hard_deadline,
        &op,
    );
    if let Err(error) = &result {
        let _ = update(store, |s| {
            let r = s.runs.get_mut(run_id).ok_or("unknown run")?;
            if r.state.active() {
                r.state = RunState::Unknown;
                r.result = Some(
                    json!({"error":error,"external_outcome":"unknown","owned_process_tree":"terminated_on_drop","time_limit_seconds":timeout,"elapsed_millis":started.elapsed().as_millis(),"deadline_exceeded":Instant::now()>=hard_deadline}),
                );
                s.tasks.get_mut(&r.task).unwrap().state = TaskState::Unknown;
            }
            for o in s
                .operations
                .values_mut()
                .filter(|o| o.run == run_id && o.state == OperationState::Attempted)
            {
                o.state = OperationState::Unknown;
                let mut diagnostic = o.result.take().unwrap_or_else(|| json!({}));
                diagnostic["error"] = json!(error);
                o.result = Some(diagnostic);
            }
            Ok(())
        });
    }
    result
}

fn execute_inner(
    store: &Store,
    run_id: &str,
    thread_id: &str,
    task: &crate::domain::Task,
    binding: &crate::domain::Binding,
    executable: &Path,
    started: Instant,
    stop_at: Instant,
    hard_deadline: Instant,
    resume_op: &str,
) -> Result<Value> {
    crate::grouping::ensure_limited(
        store,
        run_id,
        executable,
        "BP owned execution handoff",
        hard_deadline,
    )?;
    let root = std::fs::canonicalize(&binding.root).map_err(|e| e.to_string())?;
    crate::store::ordinary(&root)?;
    let rules = root.join("AGENTS.md");
    crate::store::ordinary(&rules)?;
    let rules_digest = crate::digest(
        &std::fs::read(&rules).map_err(|e| format!("project AGENTS unavailable: {e}"))?,
    );
    let mut rpc = Rpc::start_limited(executable, &root, Some(hard_deadline))?;
    let pid = rpc.pid();
    update(store, |s| {
        s.runs.get_mut(run_id).ok_or("unknown run")?.server_pid = Some(pid);
        let diagnostic = s
            .operations
            .get_mut(resume_op)
            .unwrap()
            .result
            .as_mut()
            .ok_or("missing owner identity receipt")?;
        diagnostic["server_pid"] = json!(pid);
        diagnostic["server_identity"] = json!(rpc.process.identity());
        Ok(())
    })?;
    let sandbox = if task.input.authorization.allow_write {
        "workspace-write"
    } else {
        "read-only"
    };
    let response = rpc.call(
        "thread/resume",
        json!({"threadId":thread_id,"cwd":root,"approvalPolicy":"on-request","sandbox":sandbox}),
    )?;
    let actual = response["cwd"].as_str().ok_or("resume omitted cwd")?;
    if std::fs::canonicalize(actual).map_err(|e| e.to_string())? != root {
        return Err("App Server resumed in a different project".into());
    }
    let policy = response["sandbox"]["type"].as_str().unwrap_or("");
    if !matches!(
        (sandbox, policy),
        ("read-only", "readOnly") | ("workspace-write", "workspaceWrite")
    ) {
        return Err("App Server sandbox does not match task authorization".into());
    }
    let loaded = response["instructionSources"]
        .as_array()
        .is_some_and(|sources| {
            sources.iter().filter_map(Value::as_str).any(|source| {
                std::fs::canonicalize(source).ok() == std::fs::canonicalize(&rules).ok()
            })
        });
    if !loaded || crate::digest(&std::fs::read(&rules).map_err(|e| e.to_string())?) != rules_digest
    {
        return Err("project AGENTS was not loaded or changed during handoff".into());
    }
    idle_turns(&response, thread_id)?;
    update(store, |s| {
        let r = s.runs.get_mut(run_id).ok_or("unknown run")?;
        r.model = response["model"].as_str().map(str::to_owned);
        r.effort = response["reasoningEffort"].as_str().map(str::to_owned);
        r.instruction_sources = response["instructionSources"].clone();
        r.rules_digest = Some(rules_digest.clone());
        s.operations.get_mut(resume_op).unwrap().state = OperationState::Observed;
        Ok(())
    })?;
    let turn_op = update(store, |s| {
        if Instant::now() >= stop_at {
            return Err("execution time limit exhausted before task turn".into());
        }
        if s.projects[&task.input.project].paused {
            return Err("project paused before turn start".into());
        }
        let id = s.operation(run_id, "start_turn")?;
        s.operations.get_mut(&id).unwrap().state = OperationState::Attempted;
        s.runs.get_mut(run_id).unwrap().state = RunState::Running;
        s.tasks.get_mut(&task.id).unwrap().state = TaskState::Running;
        Ok(id)
    })?;
    let dispatch_snapshot = store.read()?;
    let run = &dispatch_snapshot.state.runs[run_id];
    let continuation = dispatch_snapshot
        .state
        .custody
        .as_ref()
        .and_then(|c| c.continuations.get(&task.id));
    let prior_context=continuation.map(|record| {
        let prior=&dispatch_snapshot.state.runs[&record.previous_run];
        let result=serde_json::to_string(&prior.result).unwrap_or_default();
        json!({"previous_run":prior.id,"previous_machine":prior.machine,"previous_phase":record.previous_phase,"result_sha256":crate::digest(result.as_bytes()),"result_excerpt":result.chars().take(16384).collect::<String>(),"excerpt_truncated":result.chars().count()>16384,"instruction":"Continue from the verified code checkpoint. First revalidate prior results; preserve completed work and existing valid confirmation. Do not repeat already completed external actions or infer new publication/chat permissions."})
    });
    let mode = if task.input.complexity == crate::domain::Complexity::Simple {
        "This is an already authorized simple task. Execute directly within its scope; do not start a long-task Goal or repeat confirm/budget questions."
    } else {
        "Use project autopilot/develop for the already confirmed scope. Missing required capabilities must be reported, never simulated."
    };
    let mut prompt = format!(
        "{}\nTask packet (confirmed scope; do not repeat settled decisions).\n{}\nFollow the project AGENTS and existing skills. {mode} A completed reply is not itself verification or publication evidence. Report actual commands, receipts, code identity, remaining blockers. Publication is authorized only if allow_publish is true. Do not modify other projects or infer permission from another agent.\n",
        dispatch_header(run, &turn_op),
        serde_json::to_string_pretty(task).map_err(|e| e.to_string())?
    );
    if let Some(hold) = dispatch_snapshot
        .state
        .tracking
        .get(&task.id)
        .and_then(|m| m.publication_hold.as_ref().or(m.hold.as_ref()))
    {
        prompt.push_str("\nEffective restriction overrides any earlier grant: publication is currently held. Do not submit, push, or release this task. Continue only work already authorized within its non-publication scope. Hold record:\n");
        prompt.push_str(&serde_json::to_string(hold).map_err(|e| e.to_string())?);
    }
    if let Some(context) = prior_context {
        prompt.push_str("\nVerified machine handoff continuation:\n");
        prompt.push_str(&serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?);
    }
    let response = rpc.call(
        "turn/start",
        json!({"threadId":thread_id,"input":[{"type":"text","text":prompt}]}),
    )?;
    let turn = response["turn"]["id"]
        .as_str()
        .ok_or("turn/start omitted id")?
        .to_owned();
    update(store, |s| {
        s.runs.get_mut(run_id).unwrap().turn_id = Some(turn.clone());
        s.operations.get_mut(&turn_op).unwrap().state = OperationState::Observed;
        Ok(())
    })?;
    crate::tracking::record(
        store,
        crate::tracking::Event::native(
            &task,
            Some(run_id),
            &turn,
            "turn_started",
            "原生执行回合已启动".into(),
            None,
            format!("codex://threads/{thread_id}#turn/{turn}"),
            response,
        ),
    )?;
    emit(
        &json!({"event":"run_started","run":run_id,"thread_id":thread_id,"turn_id":turn,"project":task.input.project,"pid":pid}),
    );
    let mut stopping = false;
    let mut stop_deadline = None;
    let mut approval = None;
    let mut final_text = String::new();
    let mut tool_receipts = Vec::new();
    loop {
        let state = store.read()?;
        let pause = state.state.projects[&task.input.project].paused
            || state.state.runs[run_id].state == RunState::Pausing;
        if !stopping && (pause || Instant::now() >= stop_at || approval.is_some()) {
            let stop = update(store, |s| {
                let id = s.operation(run_id, "interrupt_turn")?;
                s.operations.get_mut(&id).unwrap().state = OperationState::Attempted;
                s.runs.get_mut(run_id).unwrap().state = RunState::Pausing;
                s.tasks.get_mut(&task.id).unwrap().state = TaskState::Pausing;
                Ok(id)
            })?;
            let stopped = rpc.call(
                "turn/interrupt",
                json!({"threadId":thread_id,"turnId":turn}),
            );
            update(store, |s| {
                let o = s.operations.get_mut(&stop).unwrap();
                o.state = if stopped.is_ok() {
                    OperationState::Observed
                } else {
                    OperationState::Unknown
                };
                o.result = Some(json!({"request_acknowledged":stopped.is_ok()}));
                Ok(())
            })?;
            // Completion may win the race; terminal notification is authoritative.
            stopping = true;
            stop_deadline = Some((Instant::now() + Duration::from_secs(45)).min(hard_deadline));
        }
        if stop_deadline.is_some_and(|d| Instant::now() >= d) {
            return Err("stop was not confirmed; preserve unknown outcome".into());
        }
        let Some(event) = rpc.event()? else {
            continue;
        };
        if event.get("id").is_some() && event.get("method").is_some() {
            approval = Some(event.clone());
            crate::tracking::record(
                store,
                crate::tracking::Event::native(
                    &task,
                    Some(run_id),
                    &event["id"].to_string(),
                    "approval_required",
                    "执行器收到原生审批请求".into(),
                    Some("需要用户处理原生审批".into()),
                    format!("codex://threads/{thread_id}#turn/{turn}"),
                    event.clone(),
                ),
            )?;
            emit(&json!({"event":"client_approval_required","run":run_id,"request":event}));
            rpc.decline(&event)?;
            continue;
        }
        if event["method"] == "item/completed"
            && event["params"]["threadId"] == thread_id
            && event["params"]["turnId"] == turn
            && event["params"]["item"]["type"] == "agentMessage"
        {
            if let Some(text) = event["params"]["item"]["text"].as_str() {
                final_text = text.chars().take(64000).collect();
            }
        }
        if event["method"] == "item/completed"
            && event["params"]["threadId"] == thread_id
            && event["params"]["turnId"] == turn
            && event["params"]["item"]["type"] == "commandExecution"
        {
            if tool_receipts.len() >= 64 {
                return Err("too many command receipts; preserve unknown result for review".into());
            }
            let item = &event["params"]["item"];
            let raw = serde_json::to_vec(item).map_err(|e| e.to_string())?;
            let output = item["aggregatedOutput"].as_str().unwrap_or("");
            tool_receipts.push(json!({"id":item["id"],"command":item["command"],"cwd":item["cwd"],"status":item["status"],"exit_code":item["exitCode"],"item_sha256":crate::digest(&raw),"output":output.chars().take(16384).collect::<String>(),"output_truncated":output.chars().count()>16384}));
            let command_status = item["exitCode"].as_i64();
            let message = match command_status {
                Some(code) => format!(
                    "命令已结束，退出码 {code}（第 {} 项命令回执）",
                    tool_receipts.len()
                ),
                None => "命令结束状态待核实".into(),
            };
            crate::tracking::record(
                store,
                crate::tracking::Event::native(
                    &task,
                    Some(run_id),
                    item["id"]
                        .as_str()
                        .ok_or("native command omitted event identity")?,
                    "command_completed",
                    message,
                    match command_status {
                        Some(0) => None,
                        Some(code) => Some(format!("最近命令退出码 {code}，尚未证明问题已解决")),
                        None => Some("命令退出码缺失".into()),
                    },
                    format!("codex://threads/{thread_id}#turn/{turn}"),
                    tool_receipts.last().unwrap().clone(),
                ),
            )?;
        }
        if event["method"] == "turn/completed"
            && event["params"]["threadId"] == thread_id
            && event["params"]["turn"]["id"] == turn
        {
            let terminal = event["params"]["turn"]["status"]
                .as_str()
                .unwrap_or("unknown");
            let run_state = match terminal {
                "completed" => RunState::Completed,
                "interrupted" => RunState::Interrupted,
                "failed" => RunState::Failed,
                _ => return Err("unknown turn terminal".into()),
            };
            let receipt = json!({"schema":1,"run":run_id,"thread_id":thread_id,"turn_id":turn,"turn_status":terminal,"approval_required":approval,"text":final_text,"tool_receipts":tool_receipts,"publication_verified":false,"time_limit_seconds":hard_deadline.duration_since(started).as_secs(),"elapsed_millis":started.elapsed().as_millis()});
            let terminal_event = crate::tracking::Event::native(
                &task,
                Some(run_id),
                &turn,
                "turn_completed",
                format!("原生回合结束：{terminal}；业务交付仍需核验"),
                if approval.is_some() {
                    Some("原生审批尚未处理".into())
                } else if run_state == RunState::Failed {
                    Some("执行回合失败".into())
                } else {
                    None
                },
                format!("codex://threads/{thread_id}#turn/{turn}"),
                crate::tracking::receipt_reference(&receipt)?,
            );
            crate::tracking::transition(store, &terminal_event, |s| {
                if Instant::now() >= hard_deadline {
                    return Err(
                        "execution time limit reached before terminal result could be recorded"
                            .into(),
                    );
                }
                let r = s.runs.get_mut(run_id).unwrap();
                r.state = run_state;
                r.result = Some(receipt.clone());
                let t = s.tasks.get_mut(&task.id).unwrap();
                t.state = if approval.is_some() {
                    TaskState::Blocked
                } else if run_state == RunState::Completed {
                    TaskState::Reviewing
                } else if run_state == RunState::Interrupted {
                    t.resume_state = Some(TaskState::Ready);
                    TaskState::Paused
                } else {
                    TaskState::Blocked
                };
                Ok(())
            })?;
            return Ok(receipt);
        }
    }
}

/// Restore persisted native chat data without loading a model turn.
pub fn restore_chat(store: &Store, run_id: &str, executable: &Path, source: &str) -> Result<Value> {
    if source.trim().is_empty() {
        return Err("actual native archive acknowledgement source required".into());
    }
    let snapshot = store.read()?;
    let run = snapshot
        .state
        .runs
        .get(run_id)
        .ok_or("unknown run")?
        .clone();
    let machine = store.machine()?;
    snapshot.state.validate_chat_handoff(run_id, &machine)?;
    let thread = run
        .thread_id
        .as_deref()
        .ok_or("actual native chat binding required")?;
    let (_, (operation, preparation)) = store.transact(snapshot.revision, |s| {
        s.validate_chat_handoff(run_id, &machine)?;
        if s.operations
            .values()
            .any(|o| o.run == run_id && o.kind == "unarchive_execution_chat")
        {
            return Err(
                "chat restore already attempted; reconcile its actual result, do not repeat".into(),
            );
        }
        let preparation = s.ensure_chat_preparation(run_id)?;
        let prep = s.operations.get_mut(&preparation).unwrap();
        if !matches!(
            prep.state,
            OperationState::Intent | OperationState::Attempted
        ) {
            return Err("handoff already completed or unresolved".into());
        }
        prep.state = OperationState::Attempted;
        prep.result = Some(json!({"thread_id":thread,"native_archive_source":source}));
        let id = s.operation(run_id, "unarchive_execution_chat")?;
        let op = s.operations.get_mut(&id).unwrap();
        op.state = OperationState::Attempted;
        op.result = Some(json!({"thread_id":thread,"native_archive_source":source}));
        Ok((id, preparation))
    })?;
    let result = (|| -> Result<Value> {
        let mut rpc = Rpc::start_limited(
            executable,
            &run.binding.root,
            Some(Instant::now() + Duration::from_secs(30)),
        )?;
        let response = rpc.call("thread/unarchive", json!({"threadId":thread}))?;
        if response["thread"]["id"] != thread {
            return Err("unarchive returned a different thread identity".into());
        }
        if response["thread"]["status"]["type"] != "notLoaded" {
            return Err("unarchive did not establish a data-only, unloaded chat".into());
        }
        Ok(response)
    })();
    store.update(|s|{
        let ready=result.is_ok() && s.validate_chat_handoff(run_id, &machine).is_ok();
        let op=s.operations.get_mut(&operation).ok_or("missing unarchive operation")?;
        op.state=if result.is_ok(){OperationState::Observed}else{OperationState::Unknown};
        op.result=Some(json!({"thread_id":thread,"native_archive_source":source,"response":result.as_ref().ok(),"error":result.as_ref().err()}));
        let prep=s.operations.get_mut(&preparation).ok_or("missing handoff operation")?;
        if prep.state==OperationState::Attempted {
            prep.state=if ready {OperationState::Observed}else{OperationState::Unknown};
            prep.result=Some(json!({"thread_id":thread,"native_archive_source":source,"restore_operation":operation,"handoff_ready":ready}));
        }
        Ok(())
    })?;
    let response = result?;
    if !store.read()?.state.chat_ready(run_id) {
        return Err("chat restored but handoff state changed; execution remains blocked".into());
    }
    Ok(
        json!({"schema":1,"status":"chat_restored","run":run_id,"operation":operation,"thread_id":thread,"response":response,"task_turn_started":false}),
    )
}

/// Bounded protocol diagnostic: no task dispatch, no project writes, no hidden production chat.
pub fn project_probe(executable: &Path, root: &Path) -> Result<Value> {
    let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let mut rpc = Rpc::start_limited(
        executable,
        &root,
        Some(Instant::now() + Duration::from_secs(30)),
    )?;
    let response = rpc.call("project/list", json!({"limit":100}))?;
    Ok(
        json!({"schema":1,"status":"project_protocol_ready","cwd":root,"response":response,"task_turn_started":false}),
    )
}

/// Verify native Skill discovery without loading a chat or starting a turn.
pub fn skill_probe(executable: &Path, root: &Path) -> Result<Value> {
    let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let mut rpc = Rpc::start_limited(
        executable,
        &root,
        Some(Instant::now() + Duration::from_secs(30)),
    )?;
    let response = rpc.call("skills/list", json!({"cwds":[root],"forceReload":true}))?;
    Ok(
        json!({"schema":1,"status":"native_skills_observed","cwd":root,"response":response,"task_turn_started":false}),
    )
}

/// Bounded protocol diagnostic: no task dispatch, no project writes, no hidden production chat.
pub fn probe(executable: &Path, root: &Path) -> Result<Value> {
    let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let mut rpc = Rpc::start(executable, &root)?;
    let response = rpc.call(
        "thread/start",
        json!({"cwd":root,"approvalPolicy":"on-request","sandbox":"read-only","ephemeral":true}),
    )?;
    Ok(
        json!({"schema":1,"status":"protocol_ready","persistent_task_created":false,"cwd":response["cwd"],"model":response["model"],"reasoning_effort":response["reasoningEffort"],"sandbox":response["sandbox"],"instruction_sources":response["instructionSources"],"desktop_grouping_verified":false}),
    )
}

pub fn pid_exists(pid: u32, cwd: &Path) -> Result<bool> {
    use bridgeforge_core::{ProcessRequest, ProcessRunner, SystemProcessRunner};
    #[cfg(windows)]
    {
        let mut request = ProcessRequest::new("tasklist.exe", cwd);
        request.args = vec![
            "/FI".into(),
            format!("PID eq {pid}").into(),
            "/FO".into(),
            "CSV".into(),
            "/NH".into(),
        ];
        request.timeout = Duration::from_secs(10);
        let output = SystemProcessRunner
            .run(&request)
            .map_err(|e| e.to_string())?;
        if output.timed_out || output.code != 0 {
            return Err("cannot prove prior execution owner has stopped".into());
        }
        let id = pid.to_string();
        Ok(String::from_utf8_lossy(&output.stdout).lines().any(|line| {
            line.starts_with('"')
                && line
                    .split(',')
                    .nth(1)
                    .is_some_and(|v| v.trim_matches('"') == id)
        }))
    }
    #[cfg(not(windows))]
    {
        let mut request = ProcessRequest::new("ps", cwd);
        request.args = vec![
            "-p".into(),
            pid.to_string().into(),
            "-o".into(),
            "pid=".into(),
        ];
        request.timeout = Duration::from_secs(10);
        let output = SystemProcessRunner
            .run(&request)
            .map_err(|e| e.to_string())?;
        if output.timed_out || !matches!(output.code, 0 | 1) {
            return Err("cannot prove prior owner stopped".into());
        }
        Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
    }
}

pub fn read_idle_thread(executable: &Path, cwd: &Path, id: &str) -> Result<Value> {
    let mut rpc = Rpc::start_limited(
        executable,
        cwd,
        Some(Instant::now() + Duration::from_secs(30)),
    )?;
    let response = rpc.call("thread/read", json!({"threadId":id,"includeTurns":true}))?;
    idle_turns(&response, id)?;
    Ok(response)
}
fn idle_turns<'a>(response: &'a Value, thread_id: &str) -> Result<&'a Vec<Value>> {
    if response["thread"]["id"] != thread_id {
        return Err("native response thread identity does not match bound chat".into());
    }
    let turns = response["thread"]["turns"]
        .as_array()
        .ok_or("native response omitted turn inventory; idle not established")?;
    if turns.iter().any(|t| {
        t["id"].as_str().is_none_or(|id| id.is_empty())
            || !matches!(
                t["status"].as_str(),
                Some("completed" | "interrupted" | "failed")
            )
    }) {
        return Err("native chat has active or unknown turn state; idle not established".into());
    }
    Ok(turns)
}

fn dispatch_header(run: &crate::domain::Run, operation: &str) -> String {
    format!(
        "BridgePilot dispatch {}",
        json!({"run":run.id,"operation":operation,"task_spec":run.spec_hash,"ordinal":run.ordinal})
    )
}

/// Read existing native history only. No new turn and no automatic cross-machine takeover.
pub fn reconcile(store: &Store, id: &str, executable: &Path) -> Result<Value> {
    let snapshot = store.read()?;
    let r = snapshot.state.runs.get(id).ok_or("unknown run")?.clone();
    if r.state == RunState::Prepared || !r.state.active() {
        return Err("reconcile requires a previously attempted unresolved execution; pause a prepared run instead".into());
    }
    if r.machine != store.machine()? {
        return Err("cross-machine reconciliation requires explicit handoff".into());
    }
    for (pid, kind) in [(r.owner_pid, "owner"), (r.server_pid, "server")]
        .into_iter()
        .filter_map(|(pid, kind)| pid.map(|pid| (pid, kind)))
    {
        let identity = snapshot
            .state
            .operations
            .values()
            .find(|op| op.run == id && op.kind == "resume_execution")
            .and_then(|op| op.result.as_ref())
            .filter(|v| v[format!("{kind}_pid")] == pid)
            .and_then(|v| v[format!("{kind}_identity")].as_str());
        let alive = if let Some(identity) = identity {
            crate::owned::same_process(pid, identity)?
        } else {
            pid_exists(pid, &r.binding.root)?
        };
        if alive {
            return Err(
                "prior owner/server PID still exists; do not take over or terminate it".into(),
            );
        }
    }
    let thread_id = r
        .thread_id
        .as_deref()
        .ok_or("chat creation result unknown; use native client to inspect before binding")?;
    let mut rpc = Rpc::start(executable, &r.binding.root)?;
    let response = rpc.call(
        "thread/read",
        json!({"threadId":thread_id,"includeTurns":true}),
    )?;
    let turns = idle_turns(&response, thread_id)?;
    let start_attempted =
        snapshot.state.operations.values().any(|op| {
            op.run == id && op.kind == "start_turn" && op.state != OperationState::Intent
        });
    if !start_attempted {
        let receipt = json!({"schema":1,"status":"reconciled_before_start","run":id,"thread_id":thread_id,"task_turn_was_not_attempted":true,"publication_verified":false});
        store.transact(snapshot.revision, |s| {
            let run = s.runs.get_mut(id).ok_or("unknown run")?;
            if !run.state.active() {
                return Err("run already terminal".into());
            }
            run.state = RunState::Interrupted;
            run.result = Some(receipt.clone());
            let task = s.tasks.get_mut(&run.task).unwrap();
            task.state = TaskState::Paused;
            task.resume_state = Some(TaskState::Ready);
            for op in s.operations.values_mut().filter(|op| {
                op.run == id
                    && matches!(
                        op.state,
                        OperationState::Attempted | OperationState::Unknown
                    )
            }) {
                op.state = OperationState::Observed;
                let mut diagnostic = op.result.take().unwrap_or_else(|| json!({}));
                diagnostic["task_turn_was_not_attempted"] = json!(true);
                op.result = Some(diagnostic);
            }
            Ok(())
        })?;
        return Ok(receipt);
    }
    let starts: Vec<_> = snapshot
        .state
        .operations
        .values()
        .filter(|op| op.run == id && op.kind == "start_turn" && op.state != OperationState::Intent)
        .collect();
    if starts.len() != 1 {
        return Err("cannot uniquely identify persisted task dispatch operation".into());
    }
    let header = dispatch_header(&r, &starts[0].id) + "\n";
    let matches: Vec<&Value> = turns
        .iter()
        .filter(|t| {
            if let Some(turn) = &r.turn_id {
                return t["id"] == turn.as_str();
            }
            t["items"].as_array().is_some_and(|items| {
                items.iter().any(|item| {
                    item["type"] == "userMessage"
                        && item["content"].as_array().is_some_and(|parts| {
                            parts.iter().any(|part| {
                                part["type"] == "text"
                                    && part["text"]
                                        .as_str()
                                        .is_some_and(|text| text.starts_with(&header))
                            })
                        })
                })
            })
        })
        .collect();
    if matches.len() != 1 {
        return Err("cannot uniquely correlate native result; preserve unknown outcome".into());
    }
    let turn = matches[0];
    let terminal = turn["status"].as_str().ok_or("missing terminal status")?;
    let state = match terminal {
        "completed" => RunState::Completed,
        "interrupted" => RunState::Interrupted,
        "failed" => RunState::Failed,
        _ => return Err("nonterminal history".into()),
    };
    let receipt = json!({"schema":1,"status":"reconciled","run":id,"thread_id":thread_id,"turn":turn,"publication_verified":false});
    let event = crate::tracking::Event::native(
        &snapshot.state.tasks[&r.task],
        Some(id),
        turn["id"]
            .as_str()
            .ok_or("native reconciled turn ID missing")?,
        "turn_reconciled",
        format!("已核销原生回合：{terminal}；业务交付仍需核验"),
        (state == RunState::Failed).then(|| "原生回合失败".into()),
        format!("codex://threads/{thread_id}"),
        crate::tracking::receipt_reference(&receipt)?,
    );
    store.transact_with_sql(snapshot.revision, |s, tx| {
        let run = s.runs.get_mut(id).ok_or("unknown run")?;
        if !run.state.active() {
            return Err("run already has a terminal result".into());
        }
        run.state = state;
        run.turn_id = turn["id"].as_str().map(str::to_owned);
        run.result = Some(receipt.clone());
        let t = s.tasks.get_mut(&run.task).unwrap();
        t.state = match state {
            RunState::Completed => TaskState::Reviewing,
            RunState::Interrupted => {
                t.resume_state = Some(TaskState::Ready);
                TaskState::Paused
            }
            _ => TaskState::Blocked,
        };
        for op in s.operations.values_mut().filter(|op| {
            op.run == id
                && matches!(
                    op.state,
                    OperationState::Attempted | OperationState::Unknown
                )
        }) {
            op.state = OperationState::Observed;
            let mut diagnostic = op.result.take().unwrap_or_else(|| json!({}));
            diagnostic["reconciled_from_turn"] = turn["id"].clone();
            op.result = Some(diagnostic);
        }
        crate::tracking::append(s, tx, &event)?;
        Ok(())
    })?;
    Ok(receipt)
}

#[cfg(test)]
#[path = "../../../scripts/tests/orchestrator/protocol.rs"]
mod tests;
