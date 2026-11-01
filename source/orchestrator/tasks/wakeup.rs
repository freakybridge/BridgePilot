use crate::{Result, domain::State, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, path::PathBuf};

pub fn native_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .unwrap_or_default(),
            )
            .join(".codex")
        })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WakeRequest {
    pub controller_thread_id: String,
    pub controller_project: String,
    pub projects: Vec<String>,
    pub interval_minutes: u32,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WakeStatus {
    Planned,
    Attempted,
    Active,
    Paused,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wakeup {
    pub id: String,
    pub machine: String,
    #[serde(default = "native_home")]
    pub native_home: PathBuf,
    pub request: WakeRequest,
    pub status: WakeStatus,
    pub native_id: Option<String>,
    pub prompt: String,
    pub native_receipt: Option<Value>,
    pub observed_source: Option<String>,
}
pub fn validate_request(s: &State, machine: &str, r: &WakeRequest) -> Result<()> {
    let mut seen = BTreeSet::new();
    if r.controller_thread_id.trim().is_empty()
        || r.source.trim().is_empty()
        || r.projects.is_empty()
        || !(1..=60).contains(&r.interval_minutes)
        || !s
            .bindings
            .contains_key(&format!("{machine}:{}", r.controller_project))
        || r.projects
            .iter()
            .any(|p| !seen.insert(p) || !s.bindings.contains_key(&format!("{machine}:{p}")))
    {
        return Err("heartbeat requires actual controller, known local projects, unique scope and interval 1..60 minutes".into());
    }
    Ok(())
}
pub fn validate(s: &State) -> Result<()> {
    let mut controllers = BTreeSet::new();
    let mut native_ids = BTreeSet::new();
    for (id, w) in &s.wakeups {
        if id != &w.id
            || !valid_id(id)
            || !valid_id(&w.machine)
            || !w.native_home.is_absolute()
            || w.prompt.trim().is_empty()
        {
            return Err("invalid native wakeup identity".into());
        }
        validate_request(s, &w.machine, &w.request)?;
        if !controllers.insert((&w.machine, &w.request.controller_thread_id)) {
            return Err("overlapping native controller heartbeats".into());
        }
        if let Some(native) = &w.native_id {
            if !valid_id(native) || !native_ids.insert(native) {
                return Err("duplicate native wakeup receipt".into());
            }
        }
        if matches!(w.status, WakeStatus::Active | WakeStatus::Paused)
            && (w.native_id.is_none()
                || w.native_receipt.is_none()
                || w.observed_source
                    .as_deref()
                    .is_none_or(|v| v.trim().is_empty()))
        {
            return Err("active heartbeat requires actual observed native receipt".into());
        }
    }
    Ok(())
}
