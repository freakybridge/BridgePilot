use crate::{
    Result, codex,
    domain::*,
    store::{Store, read_json},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn args(values: Vec<String>) -> Result<(String, BTreeMap<String, String>)> {
    let command = values
        .first()
        .ok_or("command required; use bp help")?
        .clone();
    let mut options = BTreeMap::new();
    let mut index = 1;
    while index < values.len() {
        let name = &values[index];
        if !name.starts_with("--") {
            return Err(format!("unexpected argument {name}"));
        }
        let value = values
            .get(index + 1)
            .ok_or("option value required (booleans use true/false)")?;
        if value.starts_with("--")
            || options
                .insert(name[2..].to_owned(), value.clone())
                .is_some()
        {
            return Err("missing value or duplicate option".into());
        }
        index += 2;
    }
    let allowed: &[&str] = match command.as_str() {
        "help" | "self-test" => &[],
        "probe" | "project-probe" => &["codex", "project-root"],
        "development-probe" => &["project-root", "tool", "packages"],
        "bundle" => &["output", "core", "launcher"],
        "install" | "install-preview" => &["bundle", "home", "workspace"],
        "install-status" | "install-recover" | "install-rollback" => &["home"],
        "sync-configure" => &["workspace", "input"],
        "sync-enrol" => &["workspace", "expected", "source"],
        "sync-upload" | "sync-download" | "sync-reconcile" | "handoff-status" => &["workspace"],
        "sync-cancel-pending" | "sync-fence-pending" | "handoff-cancel" => &["workspace", "source"],
        "handoff-release" | "handoff-accept" => &["workspace", "input"],
        "legacy-server-probe" => &[
            "workspace",
            "run",
            "codex",
            "historical-receipt",
            "launch-evidence",
            "supervisor-log",
            "source",
        ],
        "handoff-continue" => &["workspace", "expected", "input"],
        "checkpoint-probe" => &["workspace", "project", "remote", "ref", "source"],
        "verify-development" | "verify-publication" => &["workspace", "expected", "input"],
        "configure" => &["product-root", "workspace"],
        "init" | "status" | "actions" | "rebuild-index" => &["workspace"],
        "project-add" | "task-add" | "task-revise" | "run-bind" | "pause" | "resume" => {
            &["workspace", "expected", "input"]
        }
        "task-confirm" => &["workspace", "expected", "task", "spec-hash", "human-source"],
        "run-prepare" => &["workspace", "expected", "task"],
        "operation-attempt" => &["workspace", "expected", "operation"],
        "execute" => &[
            "workspace",
            "run",
            "codex",
            "timeout-seconds",
            "launch-operation",
        ],
        "supervise" => &["workspace", "codex", "input"],
        "report-review" => &["workspace", "expected", "input"],
        "run-reconcile" => &["workspace", "run", "codex"],
        "chat-restore" => &["workspace", "run", "codex", "archive-source"],
        "chat-group" => &["workspace", "run", "codex", "source"],
        "wake-plan" | "wake-bind" | "wake-revise" => &["workspace", "expected", "input"],
        "wake-attempt" => &["workspace", "expected", "wakeup"],
        "wake-check" => &["workspace", "wakeup", "controller-thread"],
        _ => return Err(format!("unknown command {command}; use bp help")),
    };
    if let Some(key) = options.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(format!("unknown option --{key} for {command}"));
    }
    Ok((command, options))
}
fn get<'a>(options: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    options
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("--{key} required"))
}
fn number(options: &BTreeMap<String, String>, key: &str) -> Result<u64> {
    get(options, key)?
        .parse()
        .map_err(|_| format!("invalid --{key}"))
}
fn input<T: serde::de::DeserializeOwned>(options: &BTreeMap<String, String>) -> Result<T> {
    read_json(Path::new(get(options, "input")?))
}
fn receipt(snapshot: crate::store::Snapshot, result: Value) -> Value {
    json!({"schema":1,"status":"committed","archive":snapshot.archive_id,"revision":snapshot.revision,"result":result})
}

pub fn run(values: Vec<String>) -> Result<Value> {
    let (command, options) = args(values)?;
    if command == "help" {
        return Ok(
            json!({"schema":1,"commands":["init","configure","status","actions","project-add","task-add","task-revise","task-confirm","run-prepare","operation-attempt","run-bind","chat-restore","chat-group","project-probe","execute","supervise","report-review","development-probe","verify-development","verify-publication","wake-plan","wake-attempt","wake-bind","wake-revise","wake-check","run-reconcile","pause","resume","rebuild-index","probe","self-test","bundle","install","install-preview","install-status","install-recover","install-rollback","sync-configure","sync-enrol","sync-upload","sync-download","sync-reconcile","sync-cancel-pending","sync-fence-pending","checkpoint-probe","handoff-status","handoff-release","handoff-accept","handoff-cancel","handoff-continue","legacy-server-probe"],"input":"JSON files; direct mutations require --expected <archive revision>; supervise owns its bounded CAS loop","workspace":"explicit private workspace, separate from product repository"}),
        );
    }
    if command == "self-test" {
        return Ok(
            json!({"schema":1,"program":"bridgepilot","version":env!("CARGO_PKG_VERSION"),"capabilities":["bp-task-archive-v1","bp-confirm-gate-v1","bp-client-action-outbox-v1","bp-owned-stdio-v1","bp-explicit-pause-scope-v1","bp-bounded-supervision-v1","bp-readonly-report-review-v1","bp-native-chat-handoff-v1","bp-native-project-assignment-v1","bp-native-heartbeat-v1","bp-managed-development-review-v1","bp-managed-publication-review-v1","bp-private-archive-sync-v1","bp-explicit-machine-handoff-v1","bp-portable-install-v1"],"archive_reader_features":["custody-v1"],"installed_home_supported":true,"desktop_creation":"native-client-action","native_heartbeat_available":true,"wakeup_requires_observed_registration":true,"automatic_takeover":false}),
        );
    }
    if command == "probe" {
        return codex::probe(
            Path::new(get(&options, "codex")?),
            Path::new(get(&options, "project-root")?),
        );
    }
    if command == "project-probe" {
        return codex::project_probe(
            Path::new(get(&options, "codex")?),
            Path::new(get(&options, "project-root")?),
        );
    }
    if command == "bundle" {
        return crate::install::bundle(
            Path::new(get(&options, "output")?),
            Path::new(get(&options, "core")?),
            Path::new(get(&options, "launcher")?),
        );
    }
    if matches!(command.as_str(), "install" | "install-preview") {
        return crate::install::install(
            Path::new(get(&options, "bundle")?),
            Path::new(get(&options, "home")?),
            options.get("workspace").map(Path::new),
            command == "install-preview",
        );
    }
    if matches!(
        command.as_str(),
        "install-status" | "install-recover" | "install-rollback"
    ) {
        let home = Path::new(get(&options, "home")?);
        return match command.as_str() {
            "install-status" => Ok(
                json!({"status":"installed","home":home,"installation":crate::install::inspect(home)?}),
            ),
            "install-recover" => crate::install::recover(home),
            _ => crate::install::rollback(home),
        };
    }
    if command == "configure" {
        let root = Path::new(get(&options, "product-root")?);
        crate::store::ordinary(root)?;
        if !root.is_absolute()
            || (!root.join("bridgeforge-manifest.json").is_file()
                && crate::install::inspect(root).is_err())
        {
            return Err("explicit product repository root required".into());
        }
        let workspace = Store::open(Path::new(get(&options, "workspace")?))?;
        workspace.read()?;
        let path = root.join(".runtime/config/bridgepilot.json");
        crate::store::ordinary(&path)?;
        bridgeforge_core::persistence::atomic_write_json(
            &path,
            &json!({"schema":1,"workspace":workspace.workspace}),
        )
        .map_err(|e| e.to_string())?;
        return Ok(json!({"schema":1,"status":"configured","path":path}));
    }
    if command == "development-probe" {
        let ids = get(&options, "packages")?
            .split(',')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        return crate::delivery::development_probe(
            Path::new(get(&options, "project-root")?),
            Path::new(get(&options, "tool")?),
            &ids,
        );
    }
    let workspace = Path::new(get(&options, "workspace")?);
    if command == "init" {
        let store = Store::init(workspace)?;
        let machine = store.machine()?;
        let snapshot = store.read()?;
        return Ok(
            json!({"schema":1,"status":"initialized","workspace":store.workspace,"archive":snapshot.archive_id,"revision":snapshot.revision,"machine":machine}),
        );
    }
    let store = Store::open(workspace)?;
    match command.as_str() {
        "sync-configure" => crate::sync::configure(&store, input(&options)?),
        "sync-enrol" => crate::handoff::enrol(
            &store,
            number(&options, "expected")?,
            get(&options, "source")?,
        ),
        "sync-upload" => crate::sync::upload(&store),
        "sync-download" => crate::sync::download(&store),
        "sync-reconcile" => crate::sync::reconcile(&store),
        "sync-cancel-pending" => crate::sync::cancel_pending(&store, get(&options, "source")?),
        "sync-fence-pending" => crate::sync::fence_pending(&store, get(&options, "source")?),
        "handoff-release" => crate::handoff::release_native(&store, input(&options)?),
        "legacy-server-probe" => crate::handoff::legacy_server_probe(
            &store,
            get(&options, "run")?,
            Path::new(get(&options, "codex")?),
            Path::new(get(&options, "historical-receipt")?),
            Path::new(get(&options, "launch-evidence")?),
            Path::new(get(&options, "supervisor-log")?),
            get(&options, "source")?,
        ),
        "handoff-accept" => crate::handoff::accept_native(&store, input(&options)?),
        "handoff-cancel" => crate::handoff::cancel_native(&store, get(&options, "source")?),
        "handoff-continue" => {
            crate::handoff::continue_tasks(&store, number(&options, "expected")?, input(&options)?)
        }
        "handoff-status" => Ok(
            json!({"schema":1,"machine":store.machine()?,"custody":store.read()?.state.custody,"execution_started":false}),
        ),
        "checkpoint-probe" => {
            let snapshot = store.read()?;
            let project = snapshot
                .state
                .projects
                .get(get(&options, "project")?)
                .ok_or("unknown project")?;
            let binding = snapshot
                .state
                .bindings
                .get(&format!("{}:{}", store.machine()?, project.id))
                .ok_or("project not bound on this machine")?;
            Ok(
                json!({"status":"code_checkpoint_verified","checkpoint":crate::handoff::probe_checkpoint(&bridgeforge_core::SystemProcessRunner,&binding.root,project,get(&options,"remote")?,get(&options,"ref")?,get(&options,"source")?)?}),
            )
        }
        "verify-development" => crate::delivery::review_development(
            &store,
            number(&options, "expected")?,
            input(&options)?,
        ),
        "verify-publication" => crate::delivery::review_publication(
            &store,
            number(&options, "expected")?,
            input(&options)?,
        ),
        "wake-revise" => {
            crate::wakeup::revise(&store, number(&options, "expected")?, input(&options)?)
        }
        "wake-plan" => crate::wakeup::plan(&store, number(&options, "expected")?, input(&options)?),
        "wake-attempt" => crate::wakeup::attempt(
            &store,
            number(&options, "expected")?,
            get(&options, "wakeup")?,
        ),
        "wake-bind" => crate::wakeup::bind(&store, number(&options, "expected")?, input(&options)?),
        "wake-check" => crate::wakeup::check(
            &store,
            get(&options, "wakeup")?,
            get(&options, "controller-thread")?,
        ),
        "chat-group" => crate::grouping::ensure(
            &store,
            get(&options, "run")?,
            Path::new(get(&options, "codex")?),
            get(&options, "source")?,
        ),
        "chat-restore" => codex::restore_chat(
            &store,
            get(&options, "run")?,
            Path::new(get(&options, "codex")?),
            get(&options, "archive-source")?,
        ),
        "supervise" => crate::scheduler::supervise(
            &store,
            Path::new(get(&options, "codex")?),
            &std::env::current_exe().map_err(|e| e.to_string())?,
            input(&options)?,
        ),
        "report-review" => {
            crate::review::review(&store, number(&options, "expected")?, input(&options)?)
        }
        "status" => {
            let snapshot = store.read()?;
            Ok(
                json!({"schema":1,"status":"ok","archive":snapshot.archive_id,"revision":snapshot.revision,"state":snapshot.state}),
            )
        }
        "rebuild-index" => {
            Ok(json!({"schema":1,"status":"index_rebuilt","path":store.rebuild_index()?}))
        }
        "actions" => {
            let snapshot = store.read()?;
            let ready: Vec<_> = snapshot
                .state
                .tasks
                .values()
                .filter(|t| {
                    t.state == TaskState::Ready
                        && !snapshot.state.projects[&t.input.project].paused
                        && t.input
                            .dependencies
                            .iter()
                            .all(|id| snapshot.state.tasks[id].state.delivered())
                        && !snapshot.state.runs.values().any(|r| {
                            r.state.active()
                                && snapshot.state.tasks[&r.task].input.project == t.input.project
                        })
                })
                .map(|t| json!({"kind":"prepare_run","task":t.id,"project":t.input.project}))
                .collect();
            let pending: Vec<_> = snapshot
                .state
                .operations
                .values()
                .filter(|o| {
                    matches!(
                        o.state,
                        OperationState::Intent
                            | OperationState::Attempted
                            | OperationState::Unknown
                    )
                })
                .collect();
            let mut client_actions: Vec<_> = snapshot.state.operations.values()
                .filter(|op| matches!(op.kind.as_str(), "create_execution_chat" | "prepare_execution_chat") && matches!(op.state,OperationState::Intent|OperationState::Attempted|OperationState::Unknown))
                .filter_map(|op| {
                    let run=&snapshot.state.runs[&op.run];
                    if !run.state.active() || (op.kind=="create_execution_chat" && run.thread_id.is_some()) {return None;}
                    let task=&snapshot.state.tasks[&run.task];
                    let instructions=if op.kind=="create_execution_chat" {
                        "Record operation-attempt before native create_thread. Initial turn only acknowledges setup, uses no tools and makes no changes. Wait for idle and bind the actual client receipt. Never retry unknown creation. Binding then requires a separate native chat handoff."
                    }else{
                        "Wait for native idle and record operation-attempt before native set_thread_archived(true), which releases the desktop writer. Pass the actual archive acknowledgement source to chat-restore, which only restores persisted data. Never retry an unknown archive/restore or reopen the chat while BP owns execution. No new chat or model turn is authorized by this operation."
                    };
                    Some(json!({"operation":op.id,"state":op.state,"run":run.id,"kind":op.kind,"thread_id":run.thread_id,"project":task.input.project,"native_project_id":run.binding.desktop_project_id,"host":run.binding.host,"cwd":run.binding.root,"task":task.id,"task_spec":task.spec_hash,"human_source":task.input.authorization.human_source,"instructions":instructions}))
                }).collect();
            // This is a read-only client query, not a speculative move or a new
            // execution attempt. A move acknowledgement alone is not readback.
            for run in snapshot.state.runs.values().filter(|r| {
                r.state == RunState::Completed
                    && snapshot.state.tasks[&r.task].state == TaskState::Reviewing
            }) {
                client_actions.push(json!({"kind":"verify_execution_chat_group","run":run.id,"thread_id":run.thread_id,"native_project_id":run.binding.desktop_project_id,"host":run.binding.host,"cwd":run.binding.root,"instructions":"Read actual native project membership after stdio execution stops. Provide matching thread ID, project ID, cwd and actual readback source to report-review.desktop_grouping. A null/unknown project ID or move acknowledgement alone is not proof; omit grouping and keep Reviewing. Do not repeatedly move/unarchive, open an owned execution chat or modify native databases."}));
            }
            Ok(
                json!({"schema":1,"revision":snapshot.revision,"ready":ready,"pending_operations":pending,"client_actions":client_actions,"automatic_dispatch":false,"bounded_supervision_available":true}),
            )
        }
        "execute" => codex::execute_with_launch(
            &store,
            get(&options, "run")?,
            Path::new(get(&options, "codex")?),
            number(&options, "timeout-seconds")?,
            options.get("launch-operation").map(String::as_str),
        ),
        "run-reconcile" => codex::reconcile(
            &store,
            get(&options, "run")?,
            Path::new(get(&options, "codex")?),
        ),
        "project-add" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct ProjectInput {
                id: String,
                name: String,
                repository: String,
                root: PathBuf,
                desktop_project_id: String,
                host: String,
            }
            let value: ProjectInput = input(&options)?;
            crate::store::ordinary(&value.root)?;
            let root = std::fs::canonicalize(&value.root).map_err(|e| e.to_string())?;
            if !root.is_dir() {
                return Err("project root must be a directory".into());
            }
            let machine = store.machine()?;
            let id = value.id.clone();
            let before = store.read()?;
            let repository_rename = before
                .state
                .projects
                .get(&id)
                .filter(|p| p.repository != value.repository)
                .map(|p| {
                    crate::repository::verify_rename(
                        &bridgeforge_core::SystemProcessRunner,
                        &store.workspace,
                        &p.repository,
                        &value.repository,
                    )
                })
                .transpose()?;
            let (snapshot, _) = store.transact(number(&options, "expected")?, |s| {
                if s.runs.values().any(|r| {
                    r.state.active() && s.tasks.get(&r.task).is_some_and(|t| t.input.project == id)
                }) {
                    return Err("cannot rebind an active project".into());
                }
                if let Some(prior) = s.projects.get(&id) {
                    if prior.repository != value.repository
                        && repository_rename
                            .as_ref()
                            .is_none_or(|v| v["previous_repository"] != prior.repository)
                    {
                        return Err("project repository identity cannot be overwritten".into());
                    }
                }
                let paused = s.projects.get(&id).is_some_and(|p| p.paused);
                s.projects.insert(
                    id.clone(),
                    Project {
                        id: id.clone(),
                        name: value.name,
                        repository: value.repository,
                        paused,
                    },
                );
                let binding = Binding {
                    project: id.clone(),
                    machine: machine.clone(),
                    root,
                    desktop_project_id: value.desktop_project_id,
                    host: value.host,
                };
                crate::handoff::verify_binding(
                    &bridgeforge_core::SystemProcessRunner,
                    s,
                    &binding,
                )?;
                if let Some(c) = s.custody.as_mut() {
                    if c.checkpoints.contains_key(&id) {
                        c.verified_bindings
                            .insert(id.clone(), crate::handoff::binding_key(&binding));
                    }
                }
                s.bindings.insert(format!("{machine}:{id}"), binding);
                Ok(())
            })?;
            Ok(receipt(
                snapshot,
                json!({"project":id,"repository_rename":repository_rename}),
            ))
        }
        "task-add" => {
            let value: TaskInput = input(&options)?;
            let (snapshot, id) =
                store.transact(number(&options, "expected")?, |s| s.add_task(value))?;
            let task = snapshot.state.tasks[&id].clone();
            Ok(receipt(snapshot, json!({"task":task})))
        }
        "task-confirm" => {
            let id = get(&options, "task")?;
            let (snapshot, _) = store.transact(number(&options, "expected")?, |s| {
                s.confirm(
                    id,
                    get(&options, "spec-hash")?,
                    get(&options, "human-source")?,
                )
            })?;
            Ok(receipt(snapshot, json!({"task":id})))
        }
        "task-revise" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Revised {
                task: String,
                replacement: TaskInput,
            }
            let value: Revised = input(&options)?;
            let (snapshot, _) = store.transact(number(&options, "expected")?, |s| {
                s.revise(&value.task, value.replacement)
            })?;
            let task = snapshot.state.tasks[&value.task].clone();
            Ok(receipt(snapshot, json!({"task":task})))
        }
        "run-prepare" => {
            let machine = store.machine()?;
            let task = get(&options, "task")?;
            let (snapshot, (run, operation)) =
                store.transact(number(&options, "expected")?, |s| {
                    let (run, operation) = s.prepare_dispatch(task, &machine)?;
                    Ok((run, operation.unwrap_or_default()))
                })?;
            let t = &snapshot.state.tasks[task];
            let binding = &snapshot.state.bindings[&format!("{machine}:{}", t.input.project)];
            if let Some(thread) = &snapshot.state.runs[&run].thread_id {
                let result = json!({"run":run,"operation":operation,"kind":"prepare_execution_chat","thread_id":thread,"native_project_id":binding.desktop_project_id,"cwd":binding.root,"instructions":"Wait for native idle, archive this chat through the native client to release its writer, then chat-restore with the actual archive acknowledgement source. Do not start a task turn or create a replacement chat."});
                return Ok(receipt(snapshot, result));
            }
            let action = json!({"operation":operation,"run":run,"kind":"create_execution_chat","project":t.input.project,
                "native_project_id":binding.desktop_project_id,"host":binding.host,"cwd":binding.root,
                "task":task,"human_source":t.input.authorization.human_source,
                "instructions":"Record operation-attempt, then use the native Codex client to create a chat in this project. Initial turn must only acknowledge setup, use no tools and make no changes. Wait for idle/completed. Bind its actual returned thread ID and client receipt with run-bind. Do not override model or effort. A timeout is unknown; do not create a duplicate chat."});
            Ok(receipt(snapshot, action))
        }
        "operation-attempt" => {
            let id = get(&options, "operation")?;
            let (snapshot, _) = store.transact(number(&options, "expected")?, |s| {
                let op = s.operations.get_mut(id).ok_or("unknown operation")?;
                if op.state != OperationState::Intent {
                    return Err(
                        "operation already attempted or cancelled; reconcile, do not repeat".into(),
                    );
                }
                op.state = OperationState::Attempted;
                Ok(())
            })?;
            Ok(receipt(
                snapshot,
                json!({"operation":id,"state":"attempted"}),
            ))
        }
        "run-bind" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Bound {
                run: String,
                operation: String,
                thread_id: String,
                desktop_project_id: String,
                cwd: PathBuf,
                observed_status: String,
                client_receipt_source: String,
            }
            let value: Bound = input(&options)?;
            if value.thread_id.trim().is_empty()
                || value.client_receipt_source.trim().is_empty()
                || !matches!(value.observed_status.as_str(), "idle" | "completed")
            {
                return Err("actual idle native client receipt required".into());
            }
            crate::store::ordinary(&value.cwd)?;
            let cwd = std::fs::canonicalize(&value.cwd).map_err(|e| e.to_string())?;
            let machine = store.machine()?;
            let (snapshot,_)=store.transact(number(&options,"expected")?,|s| {
                let r=s.runs.get(&value.run).ok_or("unknown run")?;
                let t=s.tasks.get(&r.task).ok_or("unknown task")?;
                let binding=s.bindings.get(&format!("{machine}:{}",t.input.project)).ok_or("missing binding")?;
                let recovering=r.state==RunState::Unknown&&r.owner_pid.is_none()&&r.server_pid.is_none();
                if r.machine!=machine || !(r.state==RunState::Prepared||recovering) || r.thread_id.is_some() || binding.root!=cwd || r.binding.root!=cwd || binding.desktop_project_id!=value.desktop_project_id || r.binding.desktop_project_id!=value.desktop_project_id { return Err("native chat receipt does not match project/run binding".into()); }
                let paused=s.projects[&t.input.project].paused;
                let task_id=t.id.clone();
                if s.runs.values().any(|other| other.thread_id.as_deref()==Some(&value.thread_id)) { return Err("chat is already bound to another run; explicit resume protocol required".into()); }
                let op=s.operations.get_mut(&value.operation).ok_or("unknown create operation")?;
                if op.run!=value.run || op.kind!="create_execution_chat" || !matches!(op.state,OperationState::Attempted|OperationState::Unknown) { return Err("creation was not durably attempted for this run".into()); }
                op.state=OperationState::Observed;
                op.result=Some(json!({"thread_id":value.thread_id,"client_receipt":value.client_receipt_source,"project":value.desktop_project_id,"cwd":cwd}));
                let r=s.runs.get_mut(&value.run).unwrap();
                r.thread_id=Some(value.thread_id.clone());r.desktop_binding_source=Some(value.client_receipt_source.clone());
                if recovering {
                    r.state=if paused{RunState::Interrupted}else{RunState::Prepared};
                    let t=s.tasks.get_mut(&task_id).unwrap();t.state=if paused{t.resume_state=Some(TaskState::Ready);TaskState::Paused}else{TaskState::Ready};
                }
                s.ensure_chat_preparation(&value.run)?;
                Ok(())
            })?;
            Ok(receipt(
                snapshot,
                json!({"run":value.run,"thread_id":value.thread_id}),
            ))
        }
        "pause" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Scope {
                all: bool,
                projects: Vec<String>,
            }
            let scope: Scope = input(&options)?;
            let (snapshot, projects) = store.transact(number(&options, "expected")?, |s| {
                s.pause(scope.all, &scope.projects)
            })?;
            Ok(receipt(
                snapshot,
                json!({"pause_requested":projects,"note":"pausing is not stopped; inspect run terminal receipts"}),
            ))
        }
        "resume" => {
            let projects: Vec<String> = input(&options)?;
            let (snapshot, _) = store.transact(number(&options, "expected")?, |s| {
                s.resume_projects(&projects)
            })?;
            Ok(receipt(snapshot, json!({"resumed_projects":projects})))
        }
        _ => Err(format!("unknown command {command}; use bp help")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parser_rejects_duplicate_or_missing_values() {
        assert!(args(vec!["status".into(), "--workspace".into()]).is_err());
        assert!(
            args(vec![
                "status".into(),
                "--workspace".into(),
                "a".into(),
                "--workspace".into(),
                "b".into()
            ])
            .is_err()
        );
        assert!(
            args(vec![
                "pause".into(),
                "--project".into(),
                "wrong-scope".into()
            ])
            .is_err()
        );
    }
}
