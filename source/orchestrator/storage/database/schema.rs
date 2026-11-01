use super::{Result, sql};
use rusqlite::{Connection, types::Value};

pub(super) const SHARED: &[&str] = &[
    "archive_meta",
    "revisions",
    "history_entities",
    "legacy_revisions",
    "projects",
    "bindings",
    "task_numbers",
    "tasks",
    "task_dependencies",
    "runs",
    "operations",
    "wakeups",
    "custody",
    "tracking",
    "task_events",
];

pub(super) fn create(conn: &Connection) -> Result<()> {
    sql(conn.execute_batch(r#"
CREATE TABLE archive_meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1),archive_id TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>=0),next_task_no INTEGER NOT NULL CHECK(next_task_no>0));
CREATE TABLE revisions(revision INTEGER PRIMARY KEY CHECK(revision>=0),schema INTEGER NOT NULL CHECK(schema=1),archive_id TEXT NOT NULL,digest TEXT NOT NULL CHECK(length(digest)=64),parent_schema INTEGER,parent_archive_id TEXT,parent_revision INTEGER,parent_digest TEXT);
CREATE TABLE history_entities(kind TEXT NOT NULL,id TEXT NOT NULL,revision INTEGER NOT NULL,payload_json TEXT,deleted INTEGER NOT NULL CHECK(deleted IN(0,1)),PRIMARY KEY(kind,id,revision),CHECK((deleted=1 AND payload_json IS NULL) OR (deleted=0 AND payload_json IS NOT NULL)));
CREATE INDEX history_by_revision ON history_entities(revision,kind,id);
CREATE TABLE legacy_revisions(revision INTEGER PRIMARY KEY,bytes BLOB NOT NULL,FOREIGN KEY(revision) REFERENCES revisions(revision) DEFERRABLE INITIALLY DEFERRED);
CREATE TABLE projects(id TEXT PRIMARY KEY,name TEXT NOT NULL,repository TEXT NOT NULL,paused INTEGER NOT NULL CHECK(paused IN(0,1)));
CREATE TABLE bindings(id TEXT PRIMARY KEY,project TEXT NOT NULL REFERENCES projects(id) DEFERRABLE INITIALLY DEFERRED,machine TEXT NOT NULL,root TEXT NOT NULL,desktop_project_id TEXT NOT NULL,host TEXT NOT NULL);
CREATE TABLE task_numbers(id TEXT PRIMARY KEY,task_no INTEGER NOT NULL UNIQUE CHECK(task_no>0),first_revision INTEGER NOT NULL CHECK(first_revision>=0));
CREATE TABLE tasks(id TEXT PRIMARY KEY,task_no INTEGER NOT NULL UNIQUE REFERENCES task_numbers(task_no) DEFERRABLE INITIALLY DEFERRED,project TEXT NOT NULL REFERENCES projects(id) DEFERRABLE INITIALLY DEFERRED,title TEXT NOT NULL,state TEXT NOT NULL,spec_hash TEXT NOT NULL,input_json TEXT NOT NULL,confirmation_json TEXT,resume_state TEXT);
CREATE TABLE task_dependencies(task_id TEXT NOT NULL REFERENCES tasks(id) DEFERRABLE INITIALLY DEFERRED,dependency TEXT NOT NULL REFERENCES tasks(id) DEFERRABLE INITIALLY DEFERRED,ordinal INTEGER NOT NULL CHECK(ordinal>=0),PRIMARY KEY(task_id,dependency),UNIQUE(task_id,ordinal));
CREATE TABLE runs(id TEXT PRIMARY KEY,task TEXT NOT NULL REFERENCES tasks(id) DEFERRABLE INITIALLY DEFERRED,spec_hash TEXT NOT NULL,ordinal INTEGER NOT NULL UNIQUE,machine TEXT NOT NULL,binding_json TEXT NOT NULL,state TEXT NOT NULL,thread_id TEXT,turn_id TEXT,server_pid INTEGER,owner_pid INTEGER,desktop_binding_source TEXT,model TEXT,effort TEXT,instruction_sources_json TEXT NOT NULL,rules_digest TEXT,result_json TEXT);
CREATE TABLE operations(id TEXT PRIMARY KEY,run TEXT NOT NULL REFERENCES runs(id) DEFERRABLE INITIALLY DEFERRED,kind TEXT NOT NULL,state TEXT NOT NULL,result_json TEXT);
CREATE TABLE wakeups(id TEXT PRIMARY KEY,machine TEXT NOT NULL,native_home TEXT NOT NULL,request_json TEXT NOT NULL,status TEXT NOT NULL,native_id TEXT,prompt TEXT NOT NULL,native_receipt_json TEXT,observed_source TEXT);
CREATE TABLE custody(singleton INTEGER PRIMARY KEY CHECK(singleton=1),epoch INTEGER NOT NULL,owner TEXT,handoff_json TEXT,checkpoints_json TEXT NOT NULL,verified_bindings_json TEXT NOT NULL,continuations_json TEXT NOT NULL,source TEXT NOT NULL);
CREATE TABLE tracking(id TEXT PRIMARY KEY REFERENCES tasks(id) DEFERRABLE INITIALLY DEFERRED,payload_json TEXT NOT NULL);
CREATE TABLE local_records(key TEXT PRIMARY KEY,value BLOB NOT NULL);
"#))?;
    Ok(())
}

pub(super) fn validate(conn: &Connection) -> Result<()> {
    let mut q =
        sql(conn
            .prepare("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)<>'sqlite_'"))?;
    let rows = sql(q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    }))?;
    let mut tables = std::collections::BTreeSet::new();
    for row in rows {
        let (kind, name, definition) = row.map_err(|e| e.to_string())?;
        if kind == "table" {
            if (!SHARED.contains(&name.as_str()) && name != "local_records")
                || definition
                    .as_deref()
                    .is_some_and(|s| s.to_ascii_uppercase().contains("VIRTUAL"))
            {
                return Err("SQLite archive contains an unowned or virtual table".into());
            }
            tables.insert(name);
        } else if kind != "index" {
            return Err("SQLite archive contains unsupported triggers or views".into());
        }
    }
    if SHARED.iter().any(|t| !tables.contains(*t)) || !tables.contains("local_records") {
        return Err("SQLite archive is missing required schema tables".into());
    }
    Ok(())
}

pub(super) fn copy_shared(src: &Connection, dest: &mut Connection) -> Result<()> {
    let tx = sql(dest.transaction())?;
    sql(tx.execute_batch("PRAGMA defer_foreign_keys=ON;"))?;
    for table in SHARED {
        let mut q = sql(src.prepare(&format!("SELECT * FROM {table}")))?;
        let count = q.column_count();
        let columns = (1..=count)
            .map(|n| format!("?{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut insert = sql(tx.prepare(&format!("INSERT INTO {table} VALUES({columns})")))?;
        let mut rows = sql(q.query([]))?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let mut values = Vec::with_capacity(count);
            for index in 0..count {
                values.push(sql(row.get::<_, Value>(index))?);
            }
            sql(insert.execute(rusqlite::params_from_iter(values)))?;
        }
    }
    sql(tx.commit())?;
    Ok(())
}
