//! Resolve legacy desktop identity through actual project roots, then use the
//! native metadata API. Never write Codex's database or settings files.
use crate::{
    Result,
    codex::Rpc,
    domain::{OperationState, Run, RunState, State},
    store::{Snapshot, Store, ordinary},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

fn context(state: &State, run: &Run, machine: &str) -> Result<()> {
    let current = state.runs.get(&run.id).ok_or("missing grouping run")?;
    let task = &state.tasks[&run.task];
    let binding = state
        .bindings
        .get(&format!("{machine}:{}", task.input.project))
        .ok_or("missing current project binding")?;
    let owner_receipt = state
        .operations
        .values()
        .find(|o| o.run == run.id && o.kind == "resume_execution")
        .and_then(|o| o.result.as_ref());
    let owner_birth = owner_receipt.and_then(|v| v["owner_identity"].as_str());
    let own_claim = current.state == RunState::BindingRequired
        && current.owner_pid == Some(std::process::id())
        && owner_receipt.is_some_and(|v| v["owner_pid"] == std::process::id())
        && owner_birth.is_some()
        && crate::owned::current_identity()?.as_deref() == owner_birth;
    if run.machine != machine
        || current.thread_id != run.thread_id
        || run.thread_id.is_none()
        || current.spec_hash != run.spec_hash
        || current.spec_hash != task.spec_hash
        || binding.root != run.binding.root
        || binding.desktop_project_id != run.binding.desktop_project_id
        || state.projects[&task.input.project].paused
        || !(matches!(
            current.state,
            RunState::Prepared | RunState::Completed | RunState::Interrupted | RunState::Failed
        ) || own_claim)
        || state.runs.values().any(|r| {
            r.id != run.id
                && r.state.active()
                && (state.tasks[&r.task].input.project == task.input.project
                    || r.thread_id == run.thread_id)
        })
    {
        return Err(
            "native grouping requires an idle local bound run without another execution owner"
                .into(),
        );
    }
    Ok(())
}
fn project_root_matches(project: &Value, root: &Path) -> bool {
    project["roots"].as_array().is_some_and(|roots| {
        roots.len() == 1
            && roots[0]["path"]
                .as_str()
                .and_then(|p| fs::canonicalize(p).ok())
                .as_deref()
                == Some(root)
    })
}
fn read_thread(rpc: &mut Rpc, thread: &str, root: &Path) -> Result<Value> {
    let response = rpc.call(
        "thread/read",
        json!({"threadId":thread,"includeTurns":false}),
    )?;
    if response["thread"]["id"] != thread
        || response["thread"]["cwd"]
            .as_str()
            .and_then(|p| fs::canonicalize(p).ok())
            .as_deref()
            != Some(root)
    {
        return Err("native grouping readback changed the thread or working directory".into());
    }
    Ok(response)
}
fn assignment_intent(
    store: &Store,
    snapshot: &Snapshot,
    run_id: &str,
    machine: &str,
    source: &str,
    root: &Path,
) -> Result<(String, bool)> {
    let run = snapshot
        .state
        .runs
        .get(run_id)
        .ok_or("missing grouping run")?;
    let thread = run.thread_id.as_deref().ok_or("bound thread required")?;
    // Only retry the local intent, before any native RPC. Each attempt checks
    // current ownership/spec/binding/pause and reuses an existing operation.
    store.update(|s| {
        context(s, run, machine)?;
        if let Some(op) = s.operations.values().find(|o| o.run == run_id && o.kind == "assign_execution_project") {
            return Ok((op.id.clone(), false));
        }
        let op = s.operation(run_id, "assign_execution_project")?;
        let o = s.operations.get_mut(&op).unwrap();
        o.state = OperationState::Attempted;
        o.result = Some(json!({"source":source,"thread_id":thread,"desktop_project_id":run.binding.desktop_project_id,"cwd":root}));
        Ok((op, true))
    })
}

pub fn ensure(store: &Store, run_id: &str, executable: &Path, source: &str) -> Result<Value> {
    ensure_limited(
        store,
        run_id,
        executable,
        source,
        Instant::now() + Duration::from_secs(30),
    )
}
pub(crate) fn ensure_limited(
    store: &Store,
    run_id: &str,
    executable: &Path,
    source: &str,
    deadline: Instant,
) -> Result<Value> {
    if source.trim().is_empty() {
        return Err("actual grouping request source required".into());
    }
    let machine = store.machine()?;
    let snapshot = store.read()?;
    let run = snapshot
        .state
        .runs
        .get(run_id)
        .ok_or("unknown run")?
        .clone();
    context(&snapshot.state, &run, &machine)?;
    let root = fs::canonicalize(&run.binding.root).map_err(|e| e.to_string())?;
    ordinary(&root)?;
    let thread = run.thread_id.as_deref().unwrap();
    let (operation, fresh) = assignment_intent(store, &snapshot, run_id, &machine, source, &root)?;
    let result = (|| -> Result<Value> {
        let mut rpc = Rpc::start_limited(executable, &root, Some(deadline))?;
        let mut cursor = Value::Null;
        let mut seen = std::collections::BTreeSet::new();
        let mut matches = Vec::new();
        loop {
            let response = rpc.call("project/list", json!({"limit":100,"cursor":cursor}))?;
            let projects = response["data"]
                .as_array()
                .ok_or("native project inventory missing")?;
            matches.extend(
                projects
                    .iter()
                    .filter(|p| project_root_matches(p, &root))
                    .cloned(),
            );
            let next = response["nextCursor"].as_str();
            match next {
                None => break,
                Some(next) if seen.len() < 32 && seen.insert(next.to_owned()) => {
                    cursor = json!(next)
                }
                _ => return Err("native project inventory pagination is incomplete".into()),
            }
        }
        if matches.len() != 1 {
            return Err(
                "native project root must resolve to one unambiguous single-root project".into(),
            );
        }
        let project = matches.remove(0);
        let server_id = project["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("native project identity missing")?;
        let current_project = rpc.call("project/read", json!({"projectId":server_id}))?;
        if current_project["project"]["id"] != server_id
            || !project_root_matches(&current_project["project"], &root)
        {
            return Err("native project roots changed during grouping".into());
        }
        let before = read_thread(&mut rpc, thread, &root)?;
        if before["thread"]["projectId"] != server_id {
            if !fresh {
                return Err("prior native assignment is unresolved or drifted; readback does not permit repeating the write".into());
            }
            // Repeat local ownership checks immediately before the native write.
            context(&store.read()?.state, &run, &machine)?;
            let changed = rpc.call(
                "thread/metadata/update",
                json!({"threadId":thread,"projectId":server_id}),
            )?;
            if changed["thread"]["id"] != thread || changed["thread"]["projectId"] != server_id {
                return Err("native project assignment acknowledgement does not match".into());
            }
        }
        let observed = read_thread(&mut rpc, thread, &root)?;
        let project_after = rpc.call("project/read", json!({"projectId":server_id}))?;
        if observed["thread"]["projectId"] != server_id
            || project_after["project"]["id"] != server_id
            || !project_root_matches(&project_after["project"], &root)
        {
            return Err("native project assignment was not preserved on readback".into());
        }
        let mut member = None;
        let mut cursor = Value::Null;
        let mut pages = std::collections::BTreeSet::new();
        loop {
            let page=rpc.call("thread/list",json!({"projectId":server_id,"useStateDbOnly":true,"sourceKinds":["cli","vscode","appServer"],"limit":100,"cursor":cursor}))?;
            let rows = page["data"]
                .as_array()
                .ok_or("native project member inventory missing")?;
            if let Some(row) = rows.iter().find(|r| r["id"] == thread) {
                member = Some(row.clone());
                break;
            }
            match page["nextCursor"].as_str() {
                None => break,
                Some(next) if pages.len() < 32 && pages.insert(next.to_owned()) => {
                    cursor = json!(next)
                }
                _ => return Err("native member inventory pagination is incomplete".into()),
            }
        }
        let member =
            member.ok_or("bound chat is absent from the actual native project member list")?;
        if member["projectId"] != server_id
            || member["cwd"]
                .as_str()
                .and_then(|p| fs::canonicalize(p).ok())
                .as_deref()
                != Some(root.as_path())
        {
            return Err("native project member identity does not match the bound chat".into());
        }
        let proof = json!({"thread_id":thread,"project_id":run.binding.desktop_project_id,"cwd":root,"source":format!("native-metadata-readback:{operation}; {source}")});
        Ok(
            json!({"schema":1,"status":"native_grouping_verified","run":run_id,"operation":operation,"desktop_project_id":run.binding.desktop_project_id,"native_project_id":server_id,"thread_id":thread,"cwd":root,"proof":proof,"project":project_after["project"],"thread_observation":observed["thread"],"membership_observation":member,"task_turn_started":false}),
        )
    })();
    if let Err(error) = &result {
        let _ = store.update(|s| {
            let o = s
                .operations
                .get_mut(&operation)
                .ok_or("missing native assignment")?;
            // A previously observed receipt is retained; add the latest failed
            // observation rather than erasing its evidence.
            let mut diagnostic = o.result.take().unwrap_or_else(|| json!({}));
            diagnostic["latest_error"] = json!(error);
            o.result = Some(diagnostic);
            o.state = OperationState::Unknown;
            Ok(())
        });
        return result;
    }
    let receipt = result.unwrap();
    store.update(|s| {
        context(s, &run, &machine)?;
        let o = s
            .operations
            .get_mut(&operation)
            .ok_or("missing native assignment")?;
        o.state = OperationState::Observed;
        o.result = Some(receipt.clone());
        Ok(())
    })?;
    Ok(receipt)
}

pub(crate) fn observed_proof(state: &State, run_id: &str) -> Option<Value> {
    state
        .operations
        .values()
        .find(|o| {
            o.run == run_id
                && o.kind == "assign_execution_project"
                && o.state == OperationState::Observed
        })
        .and_then(|o| o.result.as_ref())
        .map(|v| v["proof"].clone())
        .filter(|v| v.is_object())
}

#[cfg(test)]
mod intent_tests {
    use super::*;
    use crate::{domain::*, new_id};
    use std::{
        path::PathBuf,
        sync::{Arc, Barrier},
        thread,
    };

    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn regression_concurrent_intents_recheck_identity_without_replaying_external_writes() {
        let root = std::path::absolute(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.runtime/tests/bp-native-intent"),
        )
        .unwrap()
        .join(new_id("case"));
        let _cleanup = Cleanup(root.clone());
        let store = Store::init(&root).unwrap();
        let machine = store.machine().unwrap();
        for id in ["p", "q"] {
            fs::create_dir_all(root.join(id)).unwrap();
        }
        store
            .update(|s| {
                for id in ["p", "q"] {
                    s.projects.insert(
                        id.into(),
                        Project {
                            id: id.into(),
                            name: id.into(),
                            repository: format!("test:{id}"),
                            paused: false,
                        },
                    );
                    s.bindings.insert(
                        format!("{machine}:{id}"),
                        Binding {
                            project: id.into(),
                            machine: machine.clone(),
                            root: fs::canonicalize(root.join(id)).unwrap(),
                            desktop_project_id: format!("desktop-{id}"),
                            host: "local".into(),
                        },
                    );
                    let task = s.add_task(TaskInput {
                        project: id.into(),
                        title: "intent regression".into(),
                        goal: "commit local intent".into(),
                        non_goals: vec!["no external call".into()],
                        acceptance: vec!["both intents retained".into()],
                        dependencies: vec![],
                        complexity: Complexity::Simple,
                        authorization: Authorization {
                            human_source: "test:controlled".into(),
                            allow_write: false,
                            allow_publish: false,
                            allow_execution_chat: true,
                        },
                    })?;
                    let run = s.prepare_run(&task, &machine)?;
                    let r = s.runs.get_mut(&run).unwrap();
                    r.thread_id = Some(format!("thread-{id}"));
                    r.desktop_binding_source = Some("test:controlled".into());
                }
                Ok(())
            })
            .unwrap();
        let observed = store.read().unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for run in observed.state.runs.values() {
            let (store, snapshot, machine, barrier, id, root) = (
                store.clone(),
                observed.clone(),
                machine.clone(),
                barrier.clone(),
                run.id.clone(),
                run.binding.root.clone(),
            );
            workers.push(thread::spawn(move || {
                barrier.wait();
                assignment_intent(
                    &store,
                    &snapshot,
                    &id,
                    &machine,
                    "test:same-revision",
                    &root,
                )
            }));
        }
        barrier.wait();
        let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        assert!(
            results.iter().all(Result::is_ok),
            "concurrent local intents failed: {results:?}"
        );
        let current = store.read().unwrap();
        assert_eq!(
            current
                .state
                .operations
                .values()
                .filter(|o| o.kind == "assign_execution_project")
                .count(),
            2
        );
        let run = observed
            .state
            .runs
            .values()
            .find(|r| observed.state.tasks[&r.task].input.project == "p")
            .unwrap();
        store
            .update(|s| {
                s.projects.get_mut("p").unwrap().paused = true;
                Ok(())
            })
            .unwrap();
        let before = store.read().unwrap().revision;
        assert!(
            assignment_intent(
                &store,
                &observed,
                &run.id,
                &machine,
                "test:stale-paused",
                &run.binding.root
            )
            .is_err()
        );
        assert_eq!(before, store.read().unwrap().revision);
    }
}
