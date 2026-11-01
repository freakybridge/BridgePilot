//! Application operations shared by the CLI and integration callers.
use crate::{
    Result,
    domain::*,
    store::{Snapshot, Store},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

fn receipt(snapshot: Snapshot, result: Value) -> Value {
    json!({"schema":1,"status":"committed","archive":snapshot.archive_id,"revision":snapshot.revision,"result":result})
}

pub fn actions(store: &Store) -> Result<Value> {
    let snapshot = store.read()?;
    let ready: Vec<_> = snapshot
        .state
        .tasks
        .values()
        .filter(|t| {
            t.state == TaskState::Ready
                && !snapshot.state.tracking.get(&t.id).is_some_and(|m| {
                    m.mode == crate::tracking::ExecutionMode::Observed
                        || m.hold
                            .as_ref()
                            .is_some_and(|h| h.scope == crate::tracking::HoldScope::Task)
                })
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
                OperationState::Intent | OperationState::Attempted | OperationState::Unknown
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

pub fn add_task(store: &Store, expected: u64, value: Value) -> Result<Value> {
    let (snapshot, id) = crate::tracking::register(&store, expected, value)?;
    let task = snapshot.state.tasks[&id].clone();
    Ok(receipt(snapshot, json!({"task":task})))
}

pub fn confirm_task(
    store: &Store,
    expected: u64,
    id: &str,
    spec: &str,
    source: &str,
) -> Result<Value> {
    let (snapshot, _) = store.transact(expected, |s| s.confirm(id, spec, source))?;
    Ok(receipt(snapshot, json!({"task":id})))
}

pub fn revise_task(store: &Store, expected: u64, input: Value) -> Result<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Revised {
        task: String,
        replacement: TaskInput,
    }
    let value: Revised = serde_json::from_value(input).map_err(|e| e.to_string())?;
    let (snapshot, _) = store.transact(expected, |s| s.revise(&value.task, value.replacement))?;
    let task = snapshot.state.tasks[&value.task].clone();
    Ok(receipt(snapshot, json!({"task":task})))
}

pub fn prepare_run(store: &Store, expected: u64, task: &str) -> Result<Value> {
    let machine = store.machine()?;
    let (snapshot, (run, operation)) = store.transact(expected, |s| {
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

pub fn attempt_operation(store: &Store, expected: u64, id: &str) -> Result<Value> {
    let (snapshot, _) = store.transact(expected, |s| {
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

pub fn bind_run(store: &Store, expected: u64, input: Value) -> Result<Value> {
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
    let value: Bound = serde_json::from_value(input).map_err(|e| e.to_string())?;
    if value.thread_id.trim().is_empty()
        || value.client_receipt_source.trim().is_empty()
        || !matches!(value.observed_status.as_str(), "idle" | "completed")
    {
        return Err("actual idle native client receipt required".into());
    }
    crate::store::ordinary(&value.cwd)?;
    let cwd = std::fs::canonicalize(&value.cwd).map_err(|e| e.to_string())?;
    let machine = store.machine()?;
    let (snapshot,_)=store.transact(expected,|s| {
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

pub fn pause(store: &Store, expected: u64, input: Value) -> Result<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Scope {
        all: bool,
        projects: Vec<String>,
    }
    let scope: Scope = serde_json::from_value(input).map_err(|e| e.to_string())?;
    let (snapshot, projects) = store.transact(expected, |s| s.pause(scope.all, &scope.projects))?;
    Ok(receipt(
        snapshot,
        json!({"pause_requested":projects,"note":"pausing is not stopped; inspect run terminal receipts"}),
    ))
}

pub fn resume(store: &Store, expected: u64, projects: Vec<String>) -> Result<Value> {
    let (snapshot, _) = store.transact(expected, |s| s.resume_projects(&projects))?;
    Ok(receipt(snapshot, json!({"resumed_projects":projects})))
}
