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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Binding, Project, State};
    use std::{fs, path::PathBuf};
    fn fixture() -> Snapshot {
        let mut state = State::default();
        for (id, name) in [("a", "Zulu"), ("b", "Alpha")] {
            state.projects.insert(
                id.into(),
                Project {
                    id: id.into(),
                    name: name.into(),
                    repository: format!("owner/{id}"),
                    paused: false,
                },
            );
        }
        for machine in ["here", "elsewhere"] {
            state.bindings.insert(
                format!("{machine}:b"),
                Binding {
                    project: "b".into(),
                    machine: machine.into(),
                    root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                    desktop_project_id: machine.into(),
                    host: "local".into(),
                },
            );
        }
        Snapshot {
            schema: 2,
            archive_id: "fixture".into(),
            revision: 7,
            parent: None,
            state,
        }
    }
    #[test]
    fn defaults_select_all_and_bind_only_the_current_machine() {
        let snapshot = fixture();
        let result = view(&snapshot, "here", None).unwrap();
        assert_eq!(result["read_only"], true);
        assert_eq!(result["revision"], 7);
        assert_eq!(result["projects"][0]["id"], "b");
        assert_eq!(result["projects"].as_array().unwrap().len(), 2);
        assert_eq!(
            result["projects"][0]["local_bindings"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            result["projects"][0]["local_bindings"][0]["machine"],
            "here"
        );
    }
    #[test]
    fn selection_is_exact_deduplicated_and_never_broadens_on_errors() {
        let mut snapshot = fixture();
        let result = view(&snapshot, "here", Some(&["b".into(), "Alpha".into()])).unwrap();
        assert_eq!(result["projects"].as_array().unwrap().len(), 1);
        assert!(view(&snapshot, "here", Some(&[])).is_err());
        assert!(view(&snapshot, "here", Some(&["missing".into()])).is_err());
        snapshot.state.projects.get_mut("a").unwrap().name = "Alpha".into();
        assert!(view(&snapshot, "here", Some(&["Alpha".into()])).is_err());
        assert!(serde_json::from_value::<Selection>(json!({"projects":["b"],"all":true})).is_err());
    }
    #[test]
    fn query_does_not_create_missing_identity_or_change_the_archive() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/project-list")
            .join(crate::new_id("query"));
        let store = Store::init(&root).unwrap();
        let identity = root.join(".runtime/config/bridgepilot-machine.json");
        assert!(!identity.exists());
        assert!(query(&store, None).is_err());
        assert!(!identity.exists());
        store.machine().unwrap();
        let db = root.join("doc/5_project_knowledgebase/bridgepilot/bridgepilot.sqlite3");
        let before = fs::read(&db).unwrap();
        let machine_before = fs::read(&identity).unwrap();
        assert_eq!(query(&store, None).unwrap()["projects"], json!([]));
        assert_eq!(fs::read(&db).unwrap(), before);
        assert_eq!(fs::read(&identity).unwrap(), machine_before);
    }
}
