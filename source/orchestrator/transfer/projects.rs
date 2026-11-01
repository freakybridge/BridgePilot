//! Project registration and verified rebinding during explicit machine handoff.
use crate::{Result, domain::*, store::Store};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
fn receipt(snapshot: crate::store::Snapshot, result: Value) -> Value {
    json!({"schema":1,"status":"committed","archive":snapshot.archive_id,"revision":snapshot.revision,"result":result})
}
pub fn add(store: &Store, expected: u64, input: Value) -> Result<Value> {
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
    let value: ProjectInput = serde_json::from_value(input).map_err(|e| e.to_string())?;
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
            bridgepilot_execution::repository::verify_rename(
                &bridgeforge_core::SystemProcessRunner,
                &store.workspace,
                &p.repository,
                &value.repository,
            )
        })
        .transpose()?;
    let (snapshot, _) = store.transact(expected, |s| {
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
        crate::handoff::verify_binding(&bridgeforge_core::SystemProcessRunner, s, &binding)?;
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
