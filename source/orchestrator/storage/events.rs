use crate::{Result, digest, domain::*, store::Store};
pub use bridgepilot_tasks::tracking::*;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

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

pub fn latest_receipt_matches(conn: &Connection, event: &Event, latest: &str) -> Result<bool> {
    let prior:Option<String>=conn.query_row("SELECT payload_digest FROM task_events WHERE event_id=?1 AND task_id=?2 AND run_id IS ?3 AND spec_hash=?4 AND kind=?5 AND source=?6 AND summary=?7 AND blocker IS ?8",
        params![latest,event.task,event.run,event.spec_hash,event.kind,event.source,event.summary,event.blocker],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    Ok(prior.as_ref()
        == Some(&digest(
            &serde_json::to_vec(&event.payload).map_err(|e| e.to_string())?,
        )))
}

pub fn receipt_reference(receipt: &Value) -> Result<Value> {
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
