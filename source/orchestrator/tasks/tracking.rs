use crate::{Result, digest, domain::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    #[default]
    Owned,
    Observed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldScope {
    Task,
    Publication,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hold {
    pub scope: HoldScope,
    pub reason: String,
    pub human_source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub event_id: String,
    pub spec_hash: String,
    pub kind: String,
    pub summary: String,
    pub blocker: Option<String>,
    pub source: String,
    pub observed_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub status: String,
    pub observed_at: u64,
    pub source: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskTracking {
    pub source_key: String,
    pub source_uri: String,
    pub mode: ExecutionMode,
    pub controller_thread_id: Option<String>,
    pub execution_thread_id: Option<String>,
    pub hold: Option<Hold>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_hold: Option<Hold>,
    pub latest: Option<Progress>,
    pub event_head: String,
    pub observation: Option<Observation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_error: Option<ObservationError>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationError {
    pub attempted_at: u64,
    pub revision: u64,
    pub error: String,
    pub source: String,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn validate_state(s: &State) -> Result<()> {
    let mut keys = BTreeSet::new();
    for (id, t) in &s.tracking {
        if !s.tasks.contains_key(id)
            || t.source_key.trim().is_empty()
            || t.source_uri.trim().is_empty()
            || !keys.insert(&t.source_key)
            || (!t.event_head.is_empty() && !hash(&t.event_head))
            || (t.mode == ExecutionMode::Observed
                && t.execution_thread_id.as_deref().is_none_or(str::is_empty))
        {
            return Err("invalid task provenance or duplicate task source".into());
        }
        for h in t.hold.iter().chain(t.publication_hold.iter()) {
            if h.reason.trim().is_empty() || h.human_source.trim().is_empty() {
                return Err("task hold requires an explicit reason and human source".into());
            }
        }
        if t.publication_hold
            .as_ref()
            .is_some_and(|h| h.scope != HoldScope::Publication)
        {
            return Err("publication hold has another decision scope".into());
        }
        if let Some(o) = &t.observation {
            if t.mode != ExecutionMode::Observed
                || Some(&o.thread_id) != t.execution_thread_id.as_ref()
                || !matches!(
                    o.status.as_str(),
                    "inProgress" | "completed" | "interrupted" | "failed" | "unknown" | "idle"
                )
                || o.source.trim().is_empty()
            {
                return Err("invalid native task observation".into());
            }
        }
        if t.latest.is_some() != !t.event_head.is_empty() {
            return Err("task event head is incomplete".into());
        }
    }
    Ok(())
}

fn hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}

#[derive(Clone, Debug)]
pub struct Event {
    pub id: String,
    pub task: String,
    pub run: Option<String>,
    pub spec_hash: String,
    pub kind: String,
    pub summary: String,
    pub blocker: Option<String>,
    pub source: String,
    pub payload: Value,
}

impl Event {
    pub fn native(
        task: &Task,
        run: Option<&str>,
        native_id: &str,
        kind: &str,
        summary: String,
        blocker: Option<String>,
        source: String,
        payload: Value,
    ) -> Self {
        let key = serde_json::to_vec(&(
            task.id.as_str(),
            task.spec_hash.as_str(),
            run,
            native_id,
            kind,
        ))
        .expect("event key");
        Self {
            id: digest(&key),
            task: task.id.clone(),
            run: run.map(str::to_owned),
            spec_hash: task.spec_hash.clone(),
            kind: kind.into(),
            summary,
            blocker,
            source,
            payload,
        }
    }
}
