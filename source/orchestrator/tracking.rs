//! Durable task provenance and native execution events. This is not a second
//! scheduler or an authority for development/publication certification.
use crate::{
    Result, digest,
    domain::*,
    store::{Snapshot, Store},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
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

pub(crate) fn observation_failed(
    store: &Store,
    expected: u64,
    id: &str,
    error: &str,
    source: &str,
) -> Result<()> {
    let before = store.read()?;
    if before.revision != expected {
        return Err("archive changed before recording observation failure".into());
    }
    let task = before.state.tasks.get(id).ok_or("unknown observed task")?;
    let error = error.chars().take(600).collect::<String>();
    let failure = ObservationError {
        attempted_at: now(),
        revision: expected,
        error: error.clone(),
        source: source.into(),
    };
    let event = Event::native(
        task,
        None,
        &format!("observation-error:{expected}"),
        "observation_failed",
        "关联对话观察受限，保留最后一次实际读回".into(),
        Some(error),
        source.into(),
        serde_json::to_value(&failure).map_err(|e| e.to_string())?,
    );
    store.transact_with_sql(expected, |s, tx| {
        let meta = s
            .tracking
            .get_mut(id)
            .ok_or("observed task metadata missing")?;
        if meta.mode != ExecutionMode::Observed {
            return Err("cannot record an observer failure for owned execution".into());
        }
        meta.observation_error = Some(failure);
        append(s, tx, &event)?;
        Ok(())
    })?;
    Ok(())
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS task_events (
        event_id TEXT PRIMARY KEY, task_id TEXT NOT NULL REFERENCES tasks(id) DEFERRABLE INITIALLY DEFERRED,
        run_id TEXT, spec_hash TEXT NOT NULL, kind TEXT NOT NULL, summary TEXT NOT NULL,
        blocker TEXT, source TEXT NOT NULL, observed_at INTEGER NOT NULL,
        payload_json TEXT NOT NULL, payload_digest TEXT NOT NULL,
        parent_digest TEXT NOT NULL, digest TEXT NOT NULL UNIQUE);
        CREATE INDEX IF NOT EXISTS task_events_task ON task_events(task_id,observed_at,event_id);")
        .map_err(|e| e.to_string())
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

fn event_digest(e: &Event, observed: u64, payload_hash: &str, parent: &str) -> Result<String> {
    Ok(digest(
        &serde_json::to_vec(&(
            &e.id,
            &e.task,
            &e.run,
            &e.spec_hash,
            &e.kind,
            &e.summary,
            &e.blocker,
            &e.source,
            observed,
            payload_hash,
            parent,
        ))
        .map_err(|e| e.to_string())?,
    ))
}

fn same_event(conn: &Connection, e: &Event, payload_hash: &str) -> Result<bool> {
    let existing: Option<(String,String,Option<String>,String,String,String,Option<String>,String)> = conn.query_row(
        "SELECT payload_digest,task_id,run_id,spec_hash,kind,summary,blocker,source FROM task_events WHERE event_id=?1",
        [&e.id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?)))
        .optional().map_err(|e| e.to_string())?;
    let Some(row) = existing else {
        return Ok(false);
    };
    if row
        != (
            payload_hash.to_owned(),
            e.task.clone(),
            e.run.clone(),
            e.spec_hash.clone(),
            e.kind.clone(),
            e.summary.clone(),
            e.blocker.clone(),
            e.source.clone(),
        )
    {
        return Err("native event identity was reused with different content".into());
    }
    Ok(true)
}

pub(crate) fn latest_receipt_matches(
    conn: &Connection,
    event: &Event,
    latest: &str,
) -> Result<bool> {
    let prior:Option<String>=conn.query_row("SELECT payload_digest FROM task_events WHERE event_id=?1 AND task_id=?2 AND run_id IS ?3 AND spec_hash=?4 AND kind=?5 AND source=?6 AND summary=?7 AND blocker IS ?8",
        params![latest,event.task,event.run,event.spec_hash,event.kind,event.source,event.summary,event.blocker],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    Ok(prior.as_ref()
        == Some(&digest(
            &serde_json::to_vec(&event.payload).map_err(|e| e.to_string())?,
        )))
}

pub(crate) fn receipt_reference(receipt: &Value) -> Result<Value> {
    let bytes = serde_json::to_vec(receipt).map_err(|e| e.to_string())?;
    Ok(json!({"receipt_sha256":digest(&bytes),"receipt_bytes":bytes.len()}))
}

pub fn append(s: &mut State, tx: &Transaction<'_>, event: &Event) -> Result<bool> {
    let payload = serde_json::to_string(&event.payload).map_err(|e| e.to_string())?;
    if payload.len() > 2 * 1024 * 1024
        || event.summary.chars().count() > 800
        || event.source.trim().is_empty()
    {
        return Err("execution event is oversized or lacks actual provenance".into());
    }
    let payload_hash = digest(payload.as_bytes());
    if same_event(tx, event, &payload_hash)? {
        return Ok(false);
    }
    let task = s
        .tasks
        .get(&event.task)
        .ok_or("event task does not exist")?;
    if task.spec_hash != event.spec_hash {
        return Err("stale event belongs to another task revision".into());
    }
    if let Some(run) = &event.run {
        let r = s.runs.get(run).ok_or("event run does not exist")?;
        if r.task != event.task || r.spec_hash != event.spec_hash {
            return Err("event run/task binding mismatch".into());
        }
    }
    let metadata = s
        .tracking
        .entry(event.task.clone())
        .or_insert_with(|| TaskTracking {
            source_key: format!("legacy-task:{}", task.id),
            source_uri: task.input.authorization.human_source.clone(),
            ..Default::default()
        });
    let observed = now();
    let observed_sql =
        i64::try_from(observed).map_err(|_| "event timestamp exceeds SQLite integer range")?;
    let parent = metadata.event_head.clone();
    let event_hash = event_digest(event, observed, &payload_hash, &parent)?;
    tx.execute("INSERT INTO task_events(event_id,task_id,run_id,spec_hash,kind,summary,blocker,source,observed_at,payload_json,payload_digest,parent_digest,digest)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![event.id,event.task,event.run,event.spec_hash,event.kind,event.summary,
        event.blocker,event.source,observed_sql,payload,payload_hash,parent,event_hash]).map_err(|e|e.to_string())?;
    metadata.event_head = event_hash;
    metadata.latest = Some(Progress {
        event_id: event.id.clone(),
        spec_hash: event.spec_hash.clone(),
        kind: event.kind.clone(),
        summary: event.summary.clone(),
        blocker: event.blocker.clone(),
        source: event.source.clone(),
        observed_at: observed,
    });
    Ok(true)
}

pub fn record(store: &Store, event: Event) -> Result<()> {
    for _ in 0..20 {
        let conn = store.read_connection()?;
        let payload = serde_json::to_vec(&event.payload).map_err(|e| e.to_string())?;
        if same_event(&conn, &event, &digest(&payload))? {
            return Ok(());
        }
        drop(conn);
        let expected = store.read()?.revision;
        match store.transact_with_sql(expected, |s, tx| append(s, tx, &event)) {
            Ok(_) => return Ok(()),
            Err(e)
                if e.contains("revision changed")
                    || e.contains("lock is held")
                    || e.contains("database is locked") =>
            {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => return Err(e),
        }
    }
    Err("task event writer remained busy".into())
}

/// Commit a verified executor transition and its receipt together. The caller
/// supplies the existing domain checks; this cannot certify a model assertion.
pub fn transition<T>(
    store: &Store,
    event: &Event,
    mut change: impl FnMut(&mut State) -> Result<T>,
) -> Result<T> {
    for _ in 0..20 {
        let expected = store.read()?.revision;
        match store.transact_with_sql(expected, |s, tx| {
            let result = change(s)?;
            append(s, tx, event)?;
            Ok(result)
        }) {
            Ok((_, value)) => return Ok(value),
            Err(e)
                if e.contains("revision changed")
                    || e.contains("lock is held")
                    || e.contains("database is locked") =>
            {
                std::thread::sleep(std::time::Duration::from_millis(50))
            }
            Err(e) => return Err(e),
        }
    }
    Err("task transition writer remained busy".into())
}

pub fn validate_shared(conn: &Connection, state: &State) -> Result<()> {
    let mut stmt=conn.prepare("SELECT event_id,task_id,run_id,spec_hash,kind,summary,blocker,source,observed_at,payload_json,payload_digest,parent_digest,digest FROM task_events")
        .map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                Event {
                    id: r.get(0)?,
                    task: r.get(1)?,
                    run: r.get(2)?,
                    spec_hash: r.get(3)?,
                    kind: r.get(4)?,
                    summary: r.get(5)?,
                    blocker: r.get(6)?,
                    source: r.get(7)?,
                    payload: Value::Null,
                },
                read_u64(r, 8)?,
                r.get::<_, String>(9)?,
                r.get::<_, String>(10)?,
                r.get::<_, String>(11)?,
                r.get::<_, String>(12)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut events = BTreeMap::new();
    for row in rows {
        let (mut event, observed, payload, payload_hash, parent, event_hash) =
            row.map_err(|e| e.to_string())?;
        if digest(payload.as_bytes()) != payload_hash || !state.tasks.contains_key(&event.task) {
            return Err("task event payload digest or task identity mismatch".into());
        }
        event.payload =
            bridgeforge_core::json::parse_unique_json(payload.as_bytes(), "task event payload")?;
        if event_digest(&event, observed, &payload_hash, &parent)? != event_hash
            || events.contains_key(&event_hash)
        {
            return Err("task event hash chain is invalid".into());
        }
        events.insert(event_hash, (event, observed, parent));
    }
    let mut visited = BTreeSet::new();
    for (task, meta) in &state.tracking {
        let mut cursor = meta.event_head.as_str();
        let mut latest = true;
        while !cursor.is_empty() {
            if !visited.insert(cursor.to_owned()) {
                return Err("task event chain is cyclic or shared between tasks".into());
            }
            let (e, observed, parent) =
                events.get(cursor).ok_or("task event chain is incomplete")?;
            if &e.task != task {
                return Err("task event chain crosses task identities".into());
            }
            if latest {
                let p = meta.latest.as_ref().ok_or("task event progress missing")?;
                if p.event_id != e.id
                    || p.spec_hash != e.spec_hash
                    || p.kind != e.kind
                    || p.summary != e.summary
                    || p.blocker != e.blocker
                    || p.source != e.source
                    || p.observed_at != *observed
                {
                    return Err("task progress differs from its verified event".into());
                }
            }
            latest = false;
            cursor = parent;
        }
    }
    if visited.len() != events.len() {
        return Err("unreferenced task execution events".into());
    }
    Ok(())
}

fn read_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub task: TaskInput,
    pub source_key: String,
    pub source_uri: String,
    #[serde(default)]
    pub controller_thread_id: Option<String>,
    #[serde(default)]
    pub execution_thread_id: Option<String>,
    #[serde(default)]
    pub mode: ExecutionMode,
}

pub fn register(store: &Store, expected: u64, input: Value) -> Result<(Snapshot, String)> {
    let registration: Registration = if input.get("task").is_some() {
        serde_json::from_value(input).map_err(|e| e.to_string())?
    } else {
        let task: TaskInput = serde_json::from_value(input).map_err(|e| e.to_string())?;
        Registration {
            source_key: format!(
                "request:{}",
                digest(&serde_json::to_vec(&task).map_err(|e| e.to_string())?)
            ),
            source_uri: task.authorization.human_source.clone(),
            task,
            controller_thread_id: None,
            execution_thread_id: None,
            mode: ExecutionMode::Owned,
        }
    };
    store.transact_with_sql(expected,|s,tx| {
        if let Some((id,_))=s.tracking.iter().find(|(_,t)|t.source_key==registration.source_key) {
            let spec=digest(&serde_json::to_vec(&registration.task).map_err(|e|e.to_string())?);
            if s.tasks[id].spec_hash!=spec {return Err("task source already registered with a different specification; revise it explicitly".into());}
            return Ok(id.clone());
        }
        let id=s.add_task(registration.task)?;
        if s.tracking.contains_key(&id) {return Err("matching task already has another provenance; reuse its existing task identity".into());}
        s.tracking.insert(id.clone(),TaskTracking {source_key:registration.source_key.clone(),source_uri:registration.source_uri.clone(),
            mode:registration.mode,controller_thread_id:registration.controller_thread_id,execution_thread_id:registration.execution_thread_id,..Default::default()});
        let e=Event::native(&s.tasks[&id],None,&registration.source_key,"registered","任务已登记，等待执行条件满足".into(),None,
            registration.source_uri,json!({"source_key":registration.source_key}));
        append(s,tx,&e)?;Ok(id)
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldRequest {
    pub task: String,
    pub scope: HoldScope,
    pub reason: String,
    pub human_source: String,
    pub release: bool,
}

pub fn hold(store: &Store, expected: u64, input: HoldRequest) -> Result<Snapshot> {
    store.transact_with_sql(expected,|s,tx| {
        let task=s.tasks.get(&input.task).ok_or("unknown task")?.clone();
        if input.reason.trim().is_empty()||input.human_source.trim().is_empty(){return Err("explicit task hold decision is required".into());}
        if task.state.delivered()||task.state==TaskState::Cancelled {return Err("cannot hold a delivered or cancelled task".into());}
        if input.scope==HoldScope::Publication && s.runs.values().any(|r|r.task==input.task&&r.state.active()){
            return Err("pause and reconcile the active task before changing its publication decision".into());
        }
        if input.scope==HoldScope::Task && !input.release && s.tracking.get(&input.task).is_some_and(|m|m.mode==ExecutionMode::Observed
            && m.observation.as_ref().is_none_or(|o|matches!(o.status.as_str(),"inProgress"|"unknown"))){
            return Err("observed thread must be explicitly stopped and read back before certifying task pause".into());
        }
        if input.scope==HoldScope::Task {
            if input.release {
                if s.runs.values().any(|r|r.task==input.task&&r.state.active()){return Err("reconcile active/unknown execution before resuming a task".into());}
                let t=s.tasks.get_mut(&input.task).unwrap();
                if t.state==TaskState::Paused {t.state=t.resume_state.take().unwrap_or(TaskState::Ready);}
            } else {
                let t=s.tasks.get_mut(&input.task).unwrap();
                if !matches!(t.state,TaskState::Paused|TaskState::Pausing) {t.resume_state=Some(t.state);}
                t.state=TaskState::Paused;
                for r in s.runs.values_mut().filter(|r|r.task==input.task&&r.state.active()) {
                    if r.state==RunState::Unknown {t.state=TaskState::Unknown;}
                    else if r.state==RunState::Prepared&&r.owner_pid.is_none()&&r.server_pid.is_none(){
                        if s.operations.values().any(|op|op.run==r.id&&matches!(op.state,OperationState::Attempted|OperationState::Unknown)){
                            r.state=RunState::Unknown;t.state=TaskState::Unknown;
                        }else{
                            for op in s.operations.values_mut().filter(|op|op.run==r.id&&op.state==OperationState::Intent){
                                op.state=OperationState::Observed;op.result=Some(json!({"cancelled_before_attempt":true}));
                            }
                            r.state=RunState::Interrupted;r.result=Some(json!({"stopped_before_execution":true}));t.resume_state=Some(TaskState::Ready);
                        }
                    }else{r.state=RunState::Pausing;t.state=TaskState::Pausing;}
                }
            }
        }
        let m=s.tracking.entry(input.task.clone()).or_insert_with(||TaskTracking {source_key:format!("legacy-task:{}",task.id),
            source_uri:task.input.authorization.human_source.clone(),..Default::default()});
        if m.hold.as_ref().is_some_and(|h|h.scope==HoldScope::Publication){m.publication_hold=m.hold.take();}
        let decision=if input.scope==HoldScope::Publication{&mut m.publication_hold}else{&mut m.hold};
        if input.release&&decision.is_none(){return Err("no hold exists for the requested decision scope".into());}
        *decision=if input.release {None}else{Some(Hold {scope:input.scope,reason:input.reason.clone(),human_source:input.human_source.clone()})};
        let key=digest(&serde_json::to_vec(&(&input.task,expected,input.scope,&input.reason,&input.human_source,input.release)).map_err(|e|e.to_string())?);
        let e=Event::native(&task,None,&key,if input.release{"hold_cleared"}else{"held"},
            if input.release {"已记录明确恢复决定".into()}else{"已记录明确暂停／暂缓决定".into()},
            (!input.release).then(||input.reason.clone()),input.human_source,json!({"scope":input.scope,"release":input.release,"reason":input.reason}));
        append(s,tx,&e)?;Ok(())
    }).map(|(s,())|s)
}
