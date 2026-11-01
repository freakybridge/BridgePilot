use super::{Result, entities, integer, sql, unsigned};
use crate::{
    domain::State,
    store::{Head, Snapshot},
};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeMap;

pub(super) fn head_at(conn: &Connection, revision: u64) -> Result<Head> {
    sql(conn.query_row(
        "SELECT schema,archive_id,revision,digest FROM revisions WHERE revision=?1",
        [integer(revision)?],
        |r| {
            Ok(Head {
                schema: r.get(0)?,
                archive_id: r.get(1)?,
                revision: unsigned(r, 2)?,
                digest: r.get(3)?,
            })
        },
    ))
}

pub(super) fn parent_at(conn: &Connection, revision: u64) -> Result<Option<Head>> {
    sql(conn.query_row("SELECT parent_schema,parent_archive_id,parent_revision,parent_digest FROM revisions WHERE revision=?1",[integer(revision)?],|r|{
        let schema: Option<u32> = r.get(0)?;
        Ok(match schema {Some(schema)=>Some(Head{schema,archive_id:r.get(1)?,revision:unsigned(r,2)?,digest:r.get(3)?}),None=>None})
    }))
}

pub(super) fn insert(conn: &Connection, h: &Head, parent: Option<&Head>) -> Result<()> {
    sql(conn.execute(
        "INSERT INTO revisions VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            integer(h.revision)?,
            h.schema,
            h.archive_id,
            h.digest,
            parent.map(|p| p.schema),
            parent.map(|p| &p.archive_id),
            parent.map(|p| integer(p.revision)).transpose()?,
            parent.map(|p| &p.digest)
        ],
    ))?;
    Ok(())
}

pub(super) fn values(state: &State) -> Result<BTreeMap<(String, String), String>> {
    let mut values = BTreeMap::new();
    macro_rules! collect {
        ($field:ident) => {
            for (id, value) in &state.$field {
                values.insert(
                    (stringify!($field).into(), id.clone()),
                    serde_json::to_string(value).map_err(|e| e.to_string())?,
                );
            }
        };
    }
    collect!(projects);
    collect!(bindings);
    collect!(tasks);
    collect!(runs);
    collect!(operations);
    collect!(wakeups);
    collect!(tracking);
    if let Some(value) = &state.custody {
        values.insert(
            ("custody".into(), "1".into()),
            serde_json::to_string(value).map_err(|e| e.to_string())?,
        );
    }
    Ok(values)
}

pub(super) fn store_delta(conn: &Connection, snapshot: &Snapshot) -> Result<()> {
    let before_revision: Option<u64> = sql(conn
        .query_row(
            "SELECT revision FROM archive_meta WHERE singleton=1",
            [],
            |r| unsigned(r, 0),
        )
        .optional())?;
    let before = if let Some(revision) = before_revision {
        values(&state_at(conn, revision)?)?
    } else {
        BTreeMap::new()
    };
    let after = values(&snapshot.state)?;
    for ((kind, id), value) in &after {
        if before.get(&(kind.clone(), id.clone())) != Some(value) {
            sql(conn.execute("INSERT INTO history_entities(kind,id,revision,payload_json,deleted) VALUES(?1,?2,?3,?4,0)",params![kind,id,integer(snapshot.revision)?,value]))?;
        }
    }
    for (kind, id) in before.keys() {
        if !after.contains_key(&(kind.clone(), id.clone())) {
            sql(conn.execute("INSERT INTO history_entities(kind,id,revision,payload_json,deleted) VALUES(?1,?2,?3,NULL,1)",params![kind,id,integer(snapshot.revision)?]))?;
        }
    }
    Ok(())
}

pub(super) fn state_at(conn: &Connection, revision: u64) -> Result<State> {
    let mut state = State::default();
    let mut q=sql(conn.prepare("SELECT h.kind,h.id,h.payload_json FROM history_entities h WHERE h.revision<=?1 AND h.revision=(SELECT MAX(x.revision) FROM history_entities x WHERE x.kind=h.kind AND x.id=h.id AND x.revision<=?1) AND h.deleted=0 ORDER BY h.kind,h.id"))?;
    let rows = sql(q.query_map([integer(revision)?], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    }))?;
    for row in rows {
        let (kind, id, value) = row.map_err(|e| e.to_string())?;
        macro_rules! put {
            ($field:ident) => {{
                state.$field.insert(id, entities::parse(&value)?);
            }};
        }
        match kind.as_str() {
            "projects" => {
                put!(projects)
            }
            "bindings" => {
                put!(bindings)
            }
            "tasks" => {
                put!(tasks)
            }
            "runs" => {
                put!(runs)
            }
            "operations" => {
                put!(operations)
            }
            "wakeups" => {
                put!(wakeups)
            }
            "tracking" => {
                put!(tracking)
            }
            "custody" if id == "1" => {
                state.custody = Some(entities::parse(&value)?);
            }
            _ => return Err("SQLite history contains unknown entity kind".into()),
        }
    }
    Ok(state)
}
