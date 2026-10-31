//! Explicit controller custody and verified code checkpoints. A transfer never
//! rewrites the old Run's machine identity, confirmation or recorded evidence.
use crate::{
    Result, digest,
    domain::*,
    new_id,
    store::{Store, ordinary},
    sync::{Engine, changed_files, head_from_files},
    valid_id,
};
use bridgeforge_core::{ProcessRequest, ProcessRunner, SystemProcessRunner, persistence};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub project: String,
    pub commit: String,
    pub branch: String,
    pub remote: String,
    pub reference: String,
    pub repository: String,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transfer {
    pub id: String,
    pub from: String,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Continuation {
    pub previous_run: String,
    pub previous_phase: TaskState,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Custody {
    pub epoch: u64,
    pub owner: Option<String>,
    pub handoff: Option<Transfer>,
    pub checkpoints: BTreeMap<String, Checkpoint>,
    pub verified_bindings: BTreeMap<String, String>,
    pub continuations: BTreeMap<String, Continuation>,
    pub source: String,
}
pub fn validate(s: &State) -> Result<()> {
    let Some(c) = &s.custody else {
        return Ok(());
    };
    if c.source.trim().is_empty()
        || c.owner.as_deref().is_some_and(|m| !valid_id(m))
        || c.owner.is_none() != c.handoff.is_some()
    {
        return Err("invalid archive custody".into());
    }
    if let Some(t) = &c.handoff {
        if !valid_id(&t.id) || !valid_id(&t.from) || t.source.trim().is_empty() {
            return Err("invalid handoff identity".into());
        }
    }
    for (id, p) in &c.checkpoints {
        if id != &p.project
            || !s.projects.contains_key(id)
            || !commit_id(&p.commit)
            || p.source.trim().is_empty()
            || !p.reference.starts_with("refs/heads/")
        {
            return Err("invalid code checkpoint".into());
        }
    }
    for (task, k) in &c.continuations {
        let r = s
            .runs
            .get(&k.previous_run)
            .ok_or("continuation source run missing")?;
        if !s.tasks.contains_key(task) || &r.task != task || k.source.trim().is_empty() {
            return Err("invalid continuation record".into());
        }
    }
    Ok(())
}
pub fn require_owner(s: &State, machine: &str) -> Result<()> {
    if let Some(c) = &s.custody {
        if c.owner.as_deref() != Some(machine) || c.handoff.is_some() {
            return Err(
                "archive is frozen or owned by another machine; explicit handoff required".into(),
            );
        }
    }
    Ok(())
}
pub fn binding_key(binding: &Binding) -> String {
    digest(&serde_json::to_vec(binding).expect("binding JSON"))
}
pub fn require_binding(s: &State, project: &str, binding: &Binding) -> Result<()> {
    if let Some(c) = &s.custody {
        if c.checkpoints.contains_key(project)
            && c.verified_bindings.get(project) != Some(&binding_key(binding))
        {
            return Err(
                "new-machine project binding has not passed checkpoint verification".into(),
            );
        }
    }
    Ok(())
}
pub fn enrol(s: &Store, expected: u64, source: &str) -> Result<Value> {
    if source.trim().is_empty() {
        return Err("actual user enrolment source required".into());
    }
    let machine = s.machine()?;
    let before = s.read()?;
    if before.state.custody.is_some() {
        return Err("archive already enrolled; do not replace custody".into());
    }
    if !before.state.projects.is_empty()
        && !before.state.bindings.values().any(|b| b.machine == machine)
    {
        return Err(
            "legacy archive has no binding on this machine; cannot claim a copied archive".into(),
        );
    }
    if before.state.runs.values().any(|r| r.state.active()) {
        return Err("active or unknown runs must be reconciled before archive enrolment".into());
    }
    let (snapshot, ()) = s.transact(expected, |state| {
        if state.custody.is_some() {
            return Err("custody changed".into());
        }
        state.custody = Some(Custody {
            epoch: 0,
            owner: Some(machine.clone()),
            handoff: None,
            checkpoints: BTreeMap::new(),
            verified_bindings: BTreeMap::new(),
            continuations: BTreeMap::new(),
            source: source.into(),
        });
        Ok(())
    })?;
    Ok(
        json!({"status":"archive_custody_enrolled","revision":snapshot.revision,"owner":machine,"legacy_reader_compatible":false}),
    )
}
fn stopped(store: &Store, s: &State, machine: &str) -> Result<()> {
    if s.runs.values().any(|r| r.state.active()) {
        return Err(
            "all active or unknown runs must be stopped and reconciled before handoff".into(),
        );
    }
    for r in s.runs.values().filter(|r| r.machine == machine) {
        let receipt = s
            .operations
            .values()
            .find(|o| o.run == r.id && o.kind == "resume_execution")
            .and_then(|o| o.result.as_ref());
        for (pid, key, pid_key) in [
            (r.owner_pid, "owner_identity", "owner_pid"),
            (r.server_pid, "server_identity", "server_pid"),
        ] {
            if let Some(pid) = pid {
                let identity = receipt
                    .filter(|v| v[pid_key] == pid)
                    .and_then(|v| v[key].as_str());
                let alive = if let Some(identity) = identity {
                    crate::owned::same_process(pid, identity)?
                } else {
                    crate::codex::pid_exists(pid, &r.binding.root)?
                };
                if alive && !(key == "server_identity" && legacy_server_proven(store, s, r)?) {
                    return Err("original execution process is still alive".into());
                }
            }
        }
    }
    Ok(())
}

fn legacy_key(s: &State, r: &Run) -> Result<String> {
    let ops: Vec<_> = s
        .operations
        .values()
        .filter(|o| o.run == r.id && o.kind == "resume_execution")
        .collect();
    Ok(digest(
        &serde_json::to_vec(&(r, ops)).map_err(|e| e.to_string())?,
    ))
}
fn legacy_eligible(s: &State, r: &Run) -> Result<()> {
    if r.state.active()
        || r.result.as_ref().is_none_or(|v| {
            v["status"] != "reconciled_before_start" || v["task_turn_was_not_attempted"] != true
        })
        || s.operations
            .values()
            .any(|o| o.run == r.id && o.kind == "start_turn" && o.state != OperationState::Intent)
    {
        return Err(
            "legacy server proof is limited to terminal runs reconciled before any task dispatch"
                .into(),
        );
    }
    let owner = r.owner_pid.ok_or("legacy owner PID missing")?;
    if crate::codex::pid_exists(owner, &r.binding.root)? {
        return Err("legacy original owner still exists; no PID exception".into());
    }
    Ok(())
}
fn legacy_path(store: &Store, id: &str) -> PathBuf {
    store
        .workspace
        .join(format!(".runtime/bridgepilot/ownership-proofs/{id}.json"))
}
fn legacy_server_proven(store: &Store, s: &State, r: &Run) -> Result<bool> {
    let path = legacy_path(store, &r.id);
    if !path.exists() {
        return Ok(false);
    }
    legacy_eligible(s, r)?;
    let proof: Value = crate::store::read_json(&path)?;
    let machine = store.machine()?;
    if proof["schema"] != 1
        || proof["archive"] != store.head()?.archive_id
        || proof["machine"] != machine
        || r.machine != machine
        || proof["run"] != r.id
        || proof["run_key"] != legacy_key(s, r)?
        || proof["server_pid"].as_u64() != r.server_pid.map(u64::from)
        || proof["source"].as_str().is_none_or(|v| v.trim().is_empty())
    {
        return Err("legacy process proof does not match immutable run boundaries".into());
    }
    let exe = Path::new(
        proof["executable"]
            .as_str()
            .ok_or("legacy executable source missing")?,
    );
    ordinary(exe)?;
    if digest(&fs::read(exe).map_err(|e| e.to_string())?) != proof["executable_sha256"] {
        return Err("legacy executable evidence changed".into());
    }
    let expected = exe
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("legacy executable name missing")?;
    verify_launch_evidence(
        store,
        r,
        exe,
        Path::new(
            proof["launch_evidence"]
                .as_str()
                .ok_or("launch evidence missing")?,
        ),
        Path::new(
            proof["supervisor_log"]
                .as_str()
                .ok_or("supervisor log missing")?,
        ),
    )?;
    for (path_key, hash_key) in [
        ("launch_evidence", "launch_evidence_sha256"),
        ("supervisor_log", "supervisor_log_sha256"),
    ] {
        let path = Path::new(
            proof[path_key]
                .as_str()
                .ok_or("legacy launch source missing")?,
        );
        ordinary(path)?;
        if digest(&fs::read(path).map_err(|e| e.to_string())?) != proof[hash_key] {
            return Err("legacy launch evidence changed".into());
        }
    }
    let evidence = Path::new(
        proof["historical_receipt"]
            .as_str()
            .ok_or("legacy historical source missing")?,
    );
    ordinary(evidence)?;
    if digest(&fs::read(evidence).map_err(|e| e.to_string())?) != proof["historical_receipt_sha256"]
    {
        return Err("legacy historical source changed".into());
    }
    Ok(
        crate::owned::image_name(r.server_pid.ok_or("legacy server PID missing")?)?
            .is_none_or(|current| !current.eq_ignore_ascii_case(expected)),
    )
}
/// Independent read-only proof for early, already-reconciled records lacking a
/// birth identity. It never rewrites a Run or grants an Unknown-run exception.
pub fn legacy_server_probe(
    store: &Store,
    id: &str,
    executable: &Path,
    historical: &Path,
    launch: &Path,
    supervisor: &Path,
    source: &str,
) -> Result<Value> {
    if [executable, historical, launch, supervisor]
        .iter()
        .any(|p| !p.is_absolute())
    {
        return Err("actual absolute legacy evidence paths required".into());
    }
    let _lock = store.writer_lock()?;
    let snapshot = store.read()?;
    let machine = store.machine()?;
    require_owner(&snapshot.state, &machine)?;
    if source.trim().is_empty() || snapshot.state.runs.values().any(|r| r.state.active()) {
        return Err("actual source and idle archive required".into());
    }
    let run = snapshot.state.runs.get(id).ok_or("legacy run missing")?;
    if run.machine != machine {
        return Err("legacy proof belongs to another machine".into());
    }
    legacy_eligible(&snapshot.state, run)?;
    ordinary(executable)?;
    ordinary(historical)?;
    let expected = executable
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Codex executable name missing")?;
    if !expected.eq_ignore_ascii_case("codex.exe")
        && !(cfg!(feature = "test-fixtures") && expected == "bp-rpc-fixture.exe")
    {
        return Err("legacy source must be the actual Codex executable".into());
    }
    verify_launch_evidence(store, run, executable, launch, supervisor)?;
    let historical_bytes = fs::read(historical).map_err(|e| e.to_string())?;
    let old: Value =
        bridgeforge_core::json::parse_unique_json(&historical_bytes, "legacy native receipt")?;
    let thread = run
        .thread_id
        .as_ref()
        .ok_or("legacy native thread missing")?;
    if old["status"] != "reconciled_before_start"
        || old["run"] != run.id
        || old["thread_id"] != *thread
        || old["task_turn_was_not_attempted"] != true
    {
        return Err("historical native receipt does not prove this original run was reconciled before dispatch".into());
    }
    let image = crate::owned::image_name(run.server_pid.ok_or("legacy server PID missing")?)?;
    if image
        .as_ref()
        .is_some_and(|n| n.eq_ignore_ascii_case(expected))
    {
        return Err("server PID still has the Codex image; remain blocked".into());
    }
    let native = crate::codex::read_idle_thread(executable, &run.binding.root, thread)?;
    let proof = json!({"schema":1,"archive":snapshot.archive_id,"machine":machine,"run":run.id,"run_key":legacy_key(&snapshot.state,run)?,"server_pid":run.server_pid,"observed_image":image,"executable":executable,"executable_sha256":digest(&fs::read(executable).map_err(|e|e.to_string())?),"historical_receipt":historical,"historical_receipt_sha256":digest(&historical_bytes),"launch_evidence":launch,"launch_evidence_sha256":digest(&fs::read(launch).map_err(|e|e.to_string())?),"supervisor_log":supervisor,"supervisor_log_sha256":digest(&fs::read(supervisor).map_err(|e|e.to_string())?),"native_read_sha256":digest(&serde_json::to_vec(&native).map_err(|e|e.to_string())?),"source":source});
    persistence::atomic_write_json(&legacy_path(store, id), &proof).map_err(|e| e.to_string())?;
    Ok(
        json!({"status":"legacy_server_pid_reuse_verified","proof":proof,"archive_revision_unchanged":snapshot.revision,"execution_started":false}),
    )
}
fn verify_launch_evidence(
    store: &Store,
    r: &Run,
    exe: &Path,
    launch: &Path,
    supervisor: &Path,
) -> Result<()> {
    ordinary(launch)?;
    ordinary(supervisor)?;
    let body = fs::read_to_string(launch).map_err(|e| e.to_string())?;
    let normalized = body.replace("\\\\", "/").replace('\\', "/");
    let exe = exe
        .to_str()
        .ok_or("launch executable path invalid")?
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/");
    let workspace = store
        .workspace
        .to_str()
        .ok_or("workspace path invalid")?
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/");
    if !normalized.contains("--codex")
        || !normalized.contains(&exe)
        || !normalized.contains("supervise")
        || !normalized.contains(&workspace)
    {
        return Err("original launch evidence does not bind the actual Codex and workspace".into());
    }
    let log = fs::read_to_string(supervisor).map_err(|e| e.to_string())?;
    if !log.lines().any(|line| {
        serde_json::from_str::<Value>(line).ok().is_some_and(|v| {
            v["event"] == "worker_started"
                && v["run"] == r.id
                && v["pid"].as_u64() == r.owner_pid.map(u64::from)
        })
    }) {
        return Err(
            "actual supervisor receipt does not associate the original Run and owner".into(),
        );
    }
    Ok(())
}
fn command(
    runner: &dyn ProcessRunner,
    root: &Path,
    program: &Path,
    args: &[String],
) -> Result<String> {
    let mut q = ProcessRequest::new(program.as_os_str(), root);
    q.args = args.iter().map(Into::into).collect();
    q.timeout = Duration::from_secs(90);
    q.env.insert("GIT_TERMINAL_PROMPT".into(), "0".into());
    let o = runner.run(&q).map_err(|e| e.to_string())?;
    if o.timed_out || o.code != 0 {
        return Err(format!(
            "checkpoint command failed ({}, timeout {}): {}",
            o.code,
            o.timed_out,
            String::from_utf8_lossy(&o.stderr)
        ));
    }
    String::from_utf8(o.stdout)
        .map(|s| s.trim_end().into())
        .map_err(|e| e.to_string())
}
fn git(runner: &dyn ProcessRunner, root: &Path, args: &[&str]) -> Result<String> {
    let mut all = vec!["--no-optional-locks".into()];
    all.extend(args.iter().map(|a| a.to_string()));
    command(runner, root, Path::new("git"), &all)
}
fn commit_id(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn repo_key(s: &str) -> String {
    s.trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_start_matches("https://")
        .replace("git@github.com:", "github.com/")
}
fn root_git(runner: &dyn ProcessRunner, root: &Path) -> Result<PathBuf> {
    ordinary(root)?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let actual = fs::canonicalize(git(runner, &root, &["rev-parse", "--show-toplevel"])?)
        .map_err(|e| e.to_string())?;
    if root != actual {
        return Err("checkpoint root must be the exact project repository".into());
    }
    Ok(root)
}
fn local_checkpoint(
    runner: &dyn ProcessRunner,
    root: &Path,
    project: &Project,
    remote: &str,
) -> Result<(PathBuf, String, String, String)> {
    let root = root_git(runner, root)?;
    if !git(
        runner,
        &root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?
    .is_empty()
    {
        return Err("project has uncheckpointed changes; use its managed git-sync on an authorized work branch first".into());
    }
    let commit = git(runner, &root, &["rev-parse", "--verify", "HEAD"])?;
    let branch = git(runner, &root, &["symbolic-ref", "--short", "HEAD"])?;
    let url = git(
        runner,
        &root,
        &["remote", "get-url", "--push", "--all", remote],
    )?;
    if url.lines().count() != 1 || repo_key(&url) != repo_key(&project.repository) {
        return Err("checkpoint push endpoint differs from registered project repository".into());
    }
    Ok((root, commit, branch, url))
}
pub(crate) fn verify_local_checkpoints(
    runner: &dyn ProcessRunner,
    state: &State,
    machine: &str,
) -> Result<()> {
    let custody = state.custody.as_ref().ok_or("custody missing")?;
    for (project, expected) in &custody.checkpoints {
        let binding = state
            .bindings
            .get(&format!("{machine}:{project}"))
            .ok_or("checkpoint local binding missing")?;
        let (_, commit, branch, _) = local_checkpoint(
            runner,
            &binding.root,
            &state.projects[project],
            &expected.remote,
        )?;
        if commit != expected.commit || branch != expected.branch {
            return Err("checkpoint changed before publication".into());
        }
    }
    Ok(())
}
pub fn probe_checkpoint(
    runner: &dyn ProcessRunner,
    root: &Path,
    project: &Project,
    remote: &str,
    reference: &str,
    source: &str,
) -> Result<Checkpoint> {
    if source.trim().is_empty()
        || !reference.starts_with("refs/heads/")
        || remote.is_empty()
        || remote.starts_with('-')
    {
        return Err("explicit checkpoint remote/ref and source required".into());
    }
    let before = local_checkpoint(runner, root, project, remote)?;
    let (root, commit, branch, url) = &before;
    let observed = git(
        runner,
        root,
        &["ls-remote", "--refs", "--", &url, reference],
    )?;
    if observed.split_whitespace().collect::<Vec<_>>() != vec![commit.as_str(), reference] {
        return Err("checkpoint commit is not confirmed at the exact remote ref".into());
    }
    if local_checkpoint(runner, root, project, remote)? != before {
        return Err("project changed during checkpoint verification".into());
    }
    Ok(Checkpoint {
        project: project.id.clone(),
        commit: commit.clone(),
        branch: branch.clone(),
        remote: remote.into(),
        reference: reference.into(),
        repository: project.repository.clone(),
        source: source.into(),
    })
}
pub fn verify_binding(runner: &dyn ProcessRunner, s: &State, binding: &Binding) -> Result<()> {
    let Some(checkpoint) = s
        .custody
        .as_ref()
        .and_then(|c| c.checkpoints.get(&binding.project))
    else {
        return Ok(());
    };
    let p = &s.projects[&binding.project];
    let actual = probe_checkpoint(
        runner,
        &binding.root,
        p,
        &checkpoint.remote,
        &checkpoint.reference,
        "new-machine checkpoint verification",
    )?;
    if actual.commit != checkpoint.commit
        || actual.branch != checkpoint.branch
        || actual.repository != checkpoint.repository
    {
        return Err("new-machine code differs from the released checkpoint".into());
    }
    Ok(())
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseRequest {
    pub checkpoints: Vec<Checkpoint>,
    pub source: String,
}
pub fn release(engine: &Engine<'_>, input: ReleaseRequest) -> Result<Value> {
    let _lock = engine.store.writer_lock()?;
    let mut snapshot = engine.store.read()?;
    let machine = engine.store.machine()?;
    require_owner(&snapshot.state, &machine)?;
    stopped(engine.store, &snapshot.state, &machine)?;
    if input.source.trim().is_empty() || snapshot.state.custody.is_none() {
        return Err("enrolled custody and actual handoff source required".into());
    }
    if snapshot.state.projects.values().any(|p| !p.paused) {
        return Err(
            "explicitly pause every registered project before transferring the controller".into(),
        );
    }
    let mut checkpoints = BTreeMap::new();
    for expected in input.checkpoints {
        let project = snapshot
            .state
            .projects
            .get(&expected.project)
            .ok_or("unknown checkpoint project")?;
        let binding = snapshot
            .state
            .bindings
            .get(&format!("{machine}:{}", project.id))
            .ok_or("missing old-machine binding")?;
        let actual = probe_checkpoint(
            engine.runner,
            &binding.root,
            project,
            &expected.remote,
            &expected.reference,
            &expected.source,
        )?;
        if actual.commit != expected.commit
            || actual.branch != expected.branch
            || actual.repository != expected.repository
            || checkpoints.insert(project.id.clone(), actual).is_some()
        {
            return Err("duplicate or changed code checkpoint".into());
        }
    }
    if checkpoints.keys().collect::<BTreeSet<_>>()
        != snapshot.state.projects.keys().collect::<BTreeSet<_>>()
    {
        return Err("controller handoff requires checkpoints for every registered project; no implicit partial-machine transfer".into());
    }
    let c = snapshot.state.custody.as_mut().unwrap();
    c.epoch = c.epoch.checked_add(1).ok_or("custody epoch overflow")?;
    c.owner = None;
    c.handoff = Some(Transfer {
        id: new_id("handoff"),
        from: machine,
        source: input.source,
    });
    c.checkpoints = checkpoints;
    c.verified_bindings.clear();
    engine.publish(
        changed_files(engine.store, snapshot.state)?,
        "handoff-release",
        true,
    )
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptRequest {
    pub transfer: String,
    pub bindings: Vec<Binding>,
    pub source: String,
}
pub fn accept(engine: &Engine<'_>, input: AcceptRequest) -> Result<Value> {
    let _lock = engine.store.writer_lock()?;
    let mut snapshot = engine.store.read()?;
    let machine = engine.store.machine()?;
    stopped(engine.store, &snapshot.state, &machine)?;
    if input.source.trim().is_empty() {
        return Err("actual acceptance source required".into());
    }
    let custody = snapshot.state.custody.as_ref().ok_or("missing custody")?;
    let transfer = custody.handoff.as_ref().ok_or("archive was not released")?;
    if custody.owner.is_some() || transfer.id != input.transfer || transfer.from == machine {
        return Err("handoff ticket is not available to this new machine".into());
    }
    let Some((_, remote, _)) = engine.fetch()? else {
        return Err("handoff not published".into());
    };
    if head_from_files(&remote)? != engine.store.head()? {
        return Err("download the exact released remote head before acceptance".into());
    }
    let mut verified = BTreeMap::new();
    for mut binding in input.bindings {
        if binding.machine != machine
            || binding.desktop_project_id.trim().is_empty()
            || binding.host.trim().is_empty()
            || verified.contains_key(&binding.project)
        {
            return Err("actual unique new-machine binding required".into());
        }
        ordinary(&binding.root)?;
        binding.root = fs::canonicalize(&binding.root).map_err(|e| e.to_string())?;
        verify_binding(engine.runner, &snapshot.state, &binding)?;
        verified.insert(binding.project.clone(), binding_key(&binding));
        snapshot
            .state
            .bindings
            .insert(format!("{machine}:{}", binding.project), binding);
    }
    if verified.keys().collect::<BTreeSet<_>>()
        != snapshot.state.projects.keys().collect::<BTreeSet<_>>()
    {
        return Err("all project bindings must pass checkpoint verification before accepting the controller".into());
    }
    let c = snapshot.state.custody.as_mut().unwrap();
    c.epoch = c.epoch.checked_add(1).ok_or("custody epoch overflow")?;
    c.owner = Some(machine);
    c.handoff = None;
    c.verified_bindings = verified;
    c.source = input.source;
    engine.publish(
        changed_files(engine.store, snapshot.state)?,
        "handoff-accept",
        false,
    )
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinueRequest {
    pub tasks: Vec<String>,
    pub source: String,
}
pub fn continue_tasks(store: &Store, expected: u64, input: ContinueRequest) -> Result<Value> {
    let machine = store.machine()?;
    if input.tasks.is_empty()
        || input.source.trim().is_empty()
        || input.tasks.iter().collect::<BTreeSet<_>>().len() != input.tasks.len()
    {
        return Err("explicit unique continuation tasks and user source required".into());
    }
    let(snapshot,())=store.transact(expected,|s|{
        require_owner(s,&machine)?;
        if s.custody.as_ref().is_none_or(|c|c.epoch==0||c.handoff.is_some()){return Err("continuation requires an accepted machine handoff".into());}
        for id in &input.tasks {
            let task=s.tasks.get(id).ok_or("unknown continuation task")?;
            if task.state.delivered()||s.projects[&task.input.project].paused||s.runs.values().any(|r|r.task==*id&&r.state.active()){return Err("continuation task is delivered, paused or still owned; explicit resume/reconcile first".into());}
            let binding=s.bindings.get(&format!("{machine}:{}",task.input.project)).ok_or("new machine project binding missing")?;require_binding(s,&task.input.project,binding)?;
            let prior=s.runs.values().filter(|r|r.task==*id&&r.machine!=machine&&r.spec_hash==task.spec_hash).max_by_key(|r|r.ordinal).ok_or("no old-machine run to continue")?;
            let record=Continuation{previous_run:prior.id.clone(),previous_phase:task.state.clone(),source:input.source.clone()};
            s.custody.as_mut().unwrap().continuations.insert(id.clone(),record);
            let task=s.tasks.get_mut(id).unwrap();task.state=TaskState::Ready;task.resume_state=None;
        }Ok(())
    })?;
    Ok(
        json!({"status":"handoff_tasks_ready","revision":snapshot.revision,"tasks":input.tasks,"new_chat_or_execution_started":false}),
    )
}

pub fn release_native(store: &Store, input: ReleaseRequest) -> Result<Value> {
    release(&Engine::new(store, &SystemProcessRunner)?, input)
}
pub fn accept_native(store: &Store, input: AcceptRequest) -> Result<Value> {
    accept(&Engine::new(store, &SystemProcessRunner)?, input)
}
pub fn cancel(engine: &Engine<'_>, source: &str) -> Result<Value> {
    let _lock = engine.store.writer_lock()?;
    let mut snapshot = engine.store.read()?;
    let machine = engine.store.machine()?;
    stopped(engine.store, &snapshot.state, &machine)?;
    if source.trim().is_empty() {
        return Err("actual handoff cancellation source required".into());
    }
    let c = snapshot.state.custody.as_mut().ok_or("custody missing")?;
    if c.owner.is_some() || c.handoff.as_ref().is_none_or(|h| h.from != machine) {
        return Err("only the released source machine can request cancellation".into());
    }
    let Some((_, files, _)) = engine.fetch()? else {
        return Err("release not confirmed remotely; reconcile first".into());
    };
    if head_from_files(&files)? != engine.store.head()? {
        return Err("remote already changed; cancellation cannot overwrite another owner".into());
    }
    c.epoch = c.epoch.checked_add(1).ok_or("custody epoch overflow")?;
    c.owner = Some(machine);
    c.handoff = None;
    c.source = source.into();
    engine.publish(
        changed_files(engine.store, snapshot.state)?,
        "handoff-cancel",
        false,
    )
}
pub fn cancel_native(store: &Store, source: &str) -> Result<Value> {
    cancel(&Engine::new(store, &SystemProcessRunner)?, source)
}
