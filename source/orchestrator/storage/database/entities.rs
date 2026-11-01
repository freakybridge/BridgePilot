use super::{Result, integer, sql, unsigned};
use crate::domain::*;
use rusqlite::{Connection, OptionalExtension, Params, Row, params};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

pub(super) fn parse<T: DeserializeOwned>(text: &str) -> Result<T> {
    let value = bridgeforge_core::json::parse_unique_json(text.as_bytes(), "SQLite entity JSON")?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}
fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}
fn optional_json<T: Serialize>(value: &Option<T>) -> Result<Option<String>> {
    value.as_ref().map(json).transpose()
}
fn word<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_value(value)
        .map_err(|e| e.to_string())?
        .as_str()
        .map(str::to_owned)
        .ok_or("enum string required".into())
}
fn optional_word<T: Serialize>(value: &Option<T>) -> Result<Option<String>> {
    value.as_ref().map(word).transpose()
}
fn decode<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    parse(&text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::other(e)),
        )
    })
}
fn decode_optional<T: DeserializeOwned>(
    row: &Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<T>> {
    let text: Option<String> = row.get(index)?;
    text.map(|t| {
        parse(&t).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::other(e)),
            )
        })
    })
    .transpose()
}
fn enum_at<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    serde_json::from_value(serde_json::Value::String(text)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}
fn optional_enum<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<T>> {
    let text: Option<String> = row.get(index)?;
    text.map(|text| {
        serde_json::from_value(serde_json::Value::String(text)).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })
    })
    .transpose()
}
fn upsert<P: Params>(conn: &Connection, table: &str, columns: &[&str], values: P) -> Result<()> {
    let bind = (1..=columns.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let updates = columns
        .iter()
        .skip(1)
        .map(|c| format!("{c}=excluded.{c}"))
        .collect::<Vec<_>>()
        .join(",");
    sql(conn.execute(
        &format!(
            "INSERT INTO {table}({}) VALUES({bind}) ON CONFLICT({}) DO UPDATE SET {updates}",
            columns.join(","),
            columns[0]
        ),
        values,
    ))?;
    Ok(())
}
fn removed<T>(
    conn: &Connection,
    table: &str,
    before: &BTreeMap<String, T>,
    after: &BTreeMap<String, T>,
) -> Result<()> {
    for id in before.keys().filter(|id| !after.contains_key(*id)) {
        sql(conn.execute(&format!("DELETE FROM {table} WHERE id=?1"), [id]))?;
    }
    Ok(())
}
fn changed<T: Serialize>(before: &BTreeMap<String, T>, id: &str, value: &T) -> Result<bool> {
    Ok(before.get(id).map(json).transpose()?.as_ref() != Some(&json(value)?))
}

pub(super) fn load(conn: &Connection) -> Result<State> {
    let mut state = State::default();
    macro_rules! rows {
        ($field:ident,$query:literal,$build:expr) => {{
            let mut query = sql(conn.prepare($query))?;
            for row in sql(query.query_map([], $build))? {
                let (id, value) = row.map_err(|e| e.to_string())?;
                state.$field.insert(id, value);
            }
        }};
    }
    rows!(
        projects,
        "SELECT id,name,repository,paused FROM projects ORDER BY id",
        |r| {
            let id: String = r.get(0)?;
            Ok((
                id.clone(),
                Project {
                    id,
                    name: r.get(1)?,
                    repository: r.get(2)?,
                    paused: r.get(3)?,
                },
            ))
        }
    );
    rows!(
        bindings,
        "SELECT id,project,machine,root,desktop_project_id,host FROM bindings ORDER BY id",
        |r| {
            Ok((
                r.get(0)?,
                Binding {
                    project: r.get(1)?,
                    machine: r.get(2)?,
                    root: std::path::PathBuf::from(r.get::<_, String>(3)?),
                    desktop_project_id: r.get(4)?,
                    host: r.get(5)?,
                },
            ))
        }
    );
    rows!(
        tasks,
        "SELECT id,input_json,spec_hash,confirmation_json,state,resume_state,project,title FROM tasks ORDER BY id",
        |r| {
            let id: String = r.get(0)?;
            let input: TaskInput = decode(r, 1)?;
            if input.project != r.get::<_, String>(6)? || input.title != r.get::<_, String>(7)? {
                return Err(rusqlite::Error::InvalidQuery);
            }
            Ok((
                id.clone(),
                Task {
                    id,
                    input,
                    spec_hash: r.get(2)?,
                    confirmation: decode_optional(r, 3)?,
                    state: enum_at(r, 4)?,
                    resume_state: optional_enum(r, 5)?,
                },
            ))
        }
    );
    rows!(
        runs,
        "SELECT id,task,spec_hash,ordinal,machine,binding_json,state,thread_id,turn_id,server_pid,owner_pid,desktop_binding_source,model,effort,instruction_sources_json,rules_digest,result_json FROM runs ORDER BY id",
        |r| {
            let id: String = r.get(0)?;
            Ok((
                id.clone(),
                Run {
                    id,
                    task: r.get(1)?,
                    spec_hash: r.get(2)?,
                    ordinal: unsigned(r, 3)?,
                    machine: r.get(4)?,
                    binding: decode(r, 5)?,
                    state: enum_at(r, 6)?,
                    thread_id: r.get(7)?,
                    turn_id: r.get(8)?,
                    server_pid: r.get(9)?,
                    owner_pid: r.get(10)?,
                    desktop_binding_source: r.get(11)?,
                    model: r.get(12)?,
                    effort: r.get(13)?,
                    instruction_sources: decode(r, 14)?,
                    rules_digest: r.get(15)?,
                    result: decode_optional(r, 16)?,
                },
            ))
        }
    );
    rows!(
        operations,
        "SELECT id,run,kind,state,result_json FROM operations ORDER BY id",
        |r| {
            let id: String = r.get(0)?;
            Ok((
                id.clone(),
                Operation {
                    id,
                    run: r.get(1)?,
                    kind: r.get(2)?,
                    state: enum_at(r, 3)?,
                    result: decode_optional(r, 4)?,
                },
            ))
        }
    );
    rows!(
        wakeups,
        "SELECT id,machine,native_home,request_json,status,native_id,prompt,native_receipt_json,observed_source FROM wakeups ORDER BY id",
        |r| {
            let id: String = r.get(0)?;
            Ok((
                id.clone(),
                crate::wakeup::Wakeup {
                    id,
                    machine: r.get(1)?,
                    native_home: std::path::PathBuf::from(r.get::<_, String>(2)?),
                    request: decode(r, 3)?,
                    status: enum_at(r, 4)?,
                    native_id: r.get(5)?,
                    prompt: r.get(6)?,
                    native_receipt: decode_optional(r, 7)?,
                    observed_source: r.get(8)?,
                },
            ))
        }
    );
    rows!(
        tracking,
        "SELECT id,payload_json FROM tracking ORDER BY id",
        |r| { Ok((r.get(0)?, decode(r, 1)?)) }
    );
    state.custody=sql(conn.query_row("SELECT epoch,owner,handoff_json,checkpoints_json,verified_bindings_json,continuations_json,source FROM custody WHERE singleton=1",[],|r|{
        Ok(crate::handoff::Custody{epoch:unsigned(r,0)?,owner:r.get(1)?,handoff:decode_optional(r,2)?,checkpoints:decode(r,3)?,verified_bindings:decode(r,4)?,continuations:decode(r,5)?,source:r.get(6)?})
    }).optional())?;
    verify_dependencies(conn, &state)?;
    Ok(state)
}

fn verify_dependencies(conn: &Connection, state: &State) -> Result<()> {
    let mut q = sql(conn.prepare(
        "SELECT task_id,dependency,ordinal FROM task_dependencies ORDER BY task_id,ordinal",
    ))?;
    let mut dependencies: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in sql(q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            unsigned(r, 2)?,
        ))
    }))? {
        let (id, dep, ordinal) = row.map_err(|e| e.to_string())?;
        let list = dependencies.entry(id).or_default();
        if ordinal != u64::try_from(list.len()).map_err(|_| "dependency list size overflow")? {
            return Err("SQLite dependency order is inconsistent".into());
        }
        list.push(dep);
    }
    for (id, task) in &state.tasks {
        if dependencies.remove(id).unwrap_or_default() != task.input.dependencies {
            return Err("SQLite dependencies differ from task specification".into());
        }
    }
    if !dependencies.is_empty() {
        return Err("SQLite dependencies reference missing tasks".into());
    }
    Ok(())
}

pub(super) fn save(conn: &Connection, state: &State, revision: u64) -> Result<()> {
    let before = load(conn)?;
    removed(conn, "tracking", &before.tracking, &state.tracking)?;
    removed(conn, "operations", &before.operations, &state.operations)?;
    removed(conn, "runs", &before.runs, &state.runs)?;
    for id in before
        .tasks
        .keys()
        .filter(|id| !state.tasks.contains_key(*id))
    {
        sql(conn.execute("DELETE FROM task_dependencies WHERE task_id=?1", [id]))?;
    }
    removed(conn, "tasks", &before.tasks, &state.tasks)?;
    removed(conn, "bindings", &before.bindings, &state.bindings)?;
    removed(conn, "projects", &before.projects, &state.projects)?;
    removed(conn, "wakeups", &before.wakeups, &state.wakeups)?;
    for (id, p) in &state.projects {
        if changed(&before.projects, id, p)? {
            upsert(
                conn,
                "projects",
                &["id", "name", "repository", "paused"],
                params![id, p.name, p.repository, p.paused],
            )?;
        }
    }
    for (id, b) in &state.bindings {
        if changed(&before.bindings, id, b)? {
            upsert(
                conn,
                "bindings",
                &[
                    "id",
                    "project",
                    "machine",
                    "root",
                    "desktop_project_id",
                    "host",
                ],
                params![
                    id,
                    b.project,
                    b.machine,
                    b.root.to_string_lossy(),
                    b.desktop_project_id,
                    b.host
                ],
            )?;
        }
    }
    save_tasks(conn, &before, state, revision)?;
    save_runs(conn, &before, state)?;
    save_other(conn, &before, state)?;
    Ok(())
}

fn task_no(conn: &Connection, id: &str, revision: u64) -> Result<u64> {
    if let Some(value) = sql(conn
        .query_row("SELECT task_no FROM task_numbers WHERE id=?1", [id], |r| {
            unsigned(r, 0)
        })
        .optional())?
    {
        return Ok(value);
    }
    let previous = sql(conn.query_row(
        "SELECT COALESCE(MAX(task_no),0) FROM task_numbers",
        [],
        |r| unsigned(r, 0),
    ))?;
    let value = previous.checked_add(1).ok_or("task number overflow")?;
    sql(conn.execute(
        "INSERT INTO task_numbers(id,task_no,first_revision) VALUES(?1,?2,?3)",
        params![id, integer(value)?, integer(revision)?],
    ))?;
    sql(conn.execute(
        "UPDATE archive_meta SET next_task_no=?1 WHERE singleton=1",
        [integer(
            value.checked_add(1).ok_or("task number overflow")?,
        )?],
    ))?;
    Ok(value)
}

fn save_tasks(conn: &Connection, before: &State, state: &State, revision: u64) -> Result<()> {
    for (id, t) in &state.tasks {
        if changed(&before.tasks, id, t)? {
            let number = task_no(conn, id, revision)?;
            upsert(
                conn,
                "tasks",
                &[
                    "id",
                    "task_no",
                    "project",
                    "title",
                    "state",
                    "spec_hash",
                    "input_json",
                    "confirmation_json",
                    "resume_state",
                ],
                params![
                    id,
                    integer(number)?,
                    t.input.project,
                    t.input.title,
                    word(&t.state)?,
                    t.spec_hash,
                    json(&t.input)?,
                    optional_json(&t.confirmation)?,
                    optional_word(&t.resume_state)?
                ],
            )?;
            sql(conn.execute("DELETE FROM task_dependencies WHERE task_id=?1", [id]))?;
            for (ordinal, dependency) in t.input.dependencies.iter().enumerate() {
                sql(conn.execute(
                    "INSERT INTO task_dependencies(task_id,dependency,ordinal) VALUES(?1,?2,?3)",
                    params![
                        id,
                        dependency,
                        i64::try_from(ordinal).map_err(|_| "dependency ordinal overflow")?
                    ],
                ))?;
            }
        }
    }
    Ok(())
}

fn save_runs(conn: &Connection, before: &State, state: &State) -> Result<()> {
    for (id, r) in &state.runs {
        if changed(&before.runs, id, r)? {
            upsert(
                conn,
                "runs",
                &[
                    "id",
                    "task",
                    "spec_hash",
                    "ordinal",
                    "machine",
                    "binding_json",
                    "state",
                    "thread_id",
                    "turn_id",
                    "server_pid",
                    "owner_pid",
                    "desktop_binding_source",
                    "model",
                    "effort",
                    "instruction_sources_json",
                    "rules_digest",
                    "result_json",
                ],
                params![
                    id,
                    r.task,
                    r.spec_hash,
                    integer(r.ordinal)?,
                    r.machine,
                    json(&r.binding)?,
                    word(&r.state)?,
                    r.thread_id,
                    r.turn_id,
                    r.server_pid,
                    r.owner_pid,
                    r.desktop_binding_source,
                    r.model,
                    r.effort,
                    json(&r.instruction_sources)?,
                    r.rules_digest,
                    optional_json(&r.result)?
                ],
            )?;
        }
    }
    for (id, o) in &state.operations {
        if changed(&before.operations, id, o)? {
            upsert(
                conn,
                "operations",
                &["id", "run", "kind", "state", "result_json"],
                params![
                    id,
                    o.run,
                    o.kind,
                    word(&o.state)?,
                    optional_json(&o.result)?
                ],
            )?;
        }
    }
    Ok(())
}

fn save_other(conn: &Connection, before: &State, state: &State) -> Result<()> {
    for (id, w) in &state.wakeups {
        if changed(&before.wakeups, id, w)? {
            upsert(
                conn,
                "wakeups",
                &[
                    "id",
                    "machine",
                    "native_home",
                    "request_json",
                    "status",
                    "native_id",
                    "prompt",
                    "native_receipt_json",
                    "observed_source",
                ],
                params![
                    id,
                    w.machine,
                    w.native_home.to_string_lossy(),
                    json(&w.request)?,
                    word(&w.status)?,
                    w.native_id,
                    w.prompt,
                    optional_json(&w.native_receipt)?,
                    w.observed_source
                ],
            )?;
        }
    }
    for (id, t) in &state.tracking {
        if changed(&before.tracking, id, t)? {
            upsert(
                conn,
                "tracking",
                &["id", "payload_json"],
                params![id, json(t)?],
            )?;
        }
    }
    if json(&before.custody)? != json(&state.custody)? {
        if let Some(c) = &state.custody {
            upsert(
                conn,
                "custody",
                &[
                    "singleton",
                    "epoch",
                    "owner",
                    "handoff_json",
                    "checkpoints_json",
                    "verified_bindings_json",
                    "continuations_json",
                    "source",
                ],
                params![
                    1,
                    integer(c.epoch)?,
                    c.owner,
                    optional_json(&c.handoff)?,
                    json(&c.checkpoints)?,
                    json(&c.verified_bindings)?,
                    json(&c.continuations)?,
                    c.source
                ],
            )?;
        } else {
            sql(conn.execute("DELETE FROM custody", []))?;
        }
    }
    Ok(())
}

pub(super) fn verify_numbers(conn: &Connection) -> Result<()> {
    let mut q = sql(conn
        .prepare("SELECT id,task_no,first_revision FROM task_numbers ORDER BY first_revision,id"))?;
    let mut previous = 0_u64;
    for row in sql(q.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, unsigned(r, 1)?, unsigned(r, 2)?))
    }))? {
        let (id, number, first) = row.map_err(|e| e.to_string())?;
        let actual: Option<i64> = sql(conn.query_row(
            "SELECT MIN(revision) FROM history_entities WHERE kind='tasks' AND id=?1 AND deleted=0",
            [&id],
            |r| r.get(0),
        ))?;
        if number != previous + 1 || actual != Some(integer(first)?) {
            return Err("SQLite stable task numbering is inconsistent".into());
        }
        previous = number;
    }
    let invalid=sql(conn.query_row("SELECT COUNT(*) FROM tasks t LEFT JOIN task_numbers n ON n.id=t.id AND n.task_no=t.task_no WHERE n.id IS NULL",[],|r|unsigned(r,0)))?;
    if invalid != 0 {
        return Err("SQLite current task number identity mismatch".into());
    }
    let history_count = sql(conn.query_row(
        "SELECT COUNT(DISTINCT id) FROM history_entities WHERE kind='tasks' AND deleted=0",
        [],
        |r| unsigned(r, 0),
    ))?;
    let next = sql(conn.query_row(
        "SELECT next_task_no FROM archive_meta WHERE singleton=1",
        [],
        |r| unsigned(r, 0),
    ))?;
    if previous != history_count || next != previous + 1 {
        return Err("SQLite task number sequence does not cover its history".into());
    }
    Ok(())
}
