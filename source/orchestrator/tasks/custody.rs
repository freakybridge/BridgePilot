use crate::{Result, digest, domain::*, valid_id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
fn commit_id(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
