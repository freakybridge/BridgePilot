//! Read-only project selection for controller skills. The snapshot is not an
//! authorization to mutate a project; its own tool must recheck live boundaries.
use crate::{
    Result,
    store::{Snapshot, Store, read_json},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub projects: Vec<String>,
}

pub fn query(store: &Store, selection: Option<Selection>) -> Result<Value> {
    // Store::machine creates a new identity when missing. A listing must never
    // create one or mask a missing installation/configuration.
    let identity: Value = read_json(
        &store
            .workspace
            .join(".runtime/config/bridgepilot-machine.json"),
    )?;
    let machine = identity["id"]
        .as_str()
        .filter(|id| crate::valid_id(id))
        .ok_or("existing local machine identity required for project-list")?;
    let snapshot = store.read()?;
    view(
        &snapshot,
        machine,
        selection.as_ref().map(|s| s.projects.as_slice()),
    )
}

fn view(snapshot: &Snapshot, machine: &str, selectors: Option<&[String]>) -> Result<Value> {
    let state = &snapshot.state;
    let mut ids = BTreeSet::new();
    if let Some(selectors) = selectors {
        if selectors.is_empty() {
            return Err("explicit project selection must not be empty".into());
        }
        for selector in selectors {
            let matches = state
                .projects
                .values()
                .filter(|p| &p.id == selector || &p.name == selector)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(format!("unknown or ambiguous project: {selector}"));
            }
            ids.insert(matches[0].id.as_str());
        }
    } else {
        ids.extend(state.projects.keys().map(String::as_str));
    }
    let mut selected = ids
        .into_iter()
        .map(|id| &state.projects[id])
        .collect::<Vec<_>>();
    selected.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    let projects = selected.into_iter().map(|project| {
        let tasks = state.tasks.values().filter(|t| t.input.project == project.id).collect::<Vec<_>>();
        let task_ids = tasks.iter().map(|t| t.id.as_str()).collect::<BTreeSet<_>>();
        let runs = state.runs.values().filter(|r| task_ids.contains(r.task.as_str())).collect::<Vec<_>>();
        let run_ids = runs.iter().map(|r| r.id.as_str()).collect::<BTreeSet<_>>();
        json!({
            "id":project.id,"name":project.name,"repository":project.repository,"paused":project.paused,
            "local_bindings":state.bindings.values().filter(|b| b.project == project.id && b.machine == machine).collect::<Vec<_>>(),
            "tasks":tasks.iter().map(|t| {
                let tracking = state.tracking.get(&t.id);
                json!({"id":t.id,"state":t.state,"mode":tracking.map(|m|m.mode),
                    "observation":tracking.and_then(|m|m.observation.as_ref()),
                    "observation_error":tracking.is_some_and(|m|m.observation_error.is_some()),
                    "latest_kind":tracking.and_then(|m|m.latest.as_ref()).filter(|p|p.spec_hash==t.spec_hash).map(|p|&p.kind),
                    "hold":tracking.and_then(|m|m.hold.as_ref())})
            }).collect::<Vec<_>>(),
            "runs":runs.iter().map(|r|json!({"id":r.id,"state":r.state,"machine":r.machine,"thread_id":r.thread_id})).collect::<Vec<_>>(),
            "operations":state.operations.values().filter(|o|run_ids.contains(o.run.as_str())).map(|o|json!({"id":o.id,"run":o.run,"kind":o.kind,"state":o.state})).collect::<Vec<_>>()
        })
    }).collect::<Vec<_>>();
    Ok(
        json!({"schema":1,"status":"projects_listed","read_only":true,"execution_started":false,
        "archive":snapshot.archive_id,"revision":snapshot.revision,"machine":machine,"custody":state.custody,"projects":projects}),
    )
}
