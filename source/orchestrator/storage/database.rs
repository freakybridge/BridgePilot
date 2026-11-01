//! SQLite task archive. SQL rows are authoritative; State is an in-memory
//! compatibility model used by the existing authorization/state machine.
use crate::{
    Result,
    domain::State,
    store::{Head, Snapshot, ordinary},
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, params};
use std::{collections::BTreeMap, io::Cursor, path::Path, time::Duration};

mod entities;
mod history;
mod schema;

pub const FILE: &str = "bridgepilot.sqlite3";
pub const MAX_BYTES: usize = 256 * 1024 * 1024;
const APPLICATION_ID: i64 = 0x4250_4239;
const VERSION: i64 = 1;

pub(crate) fn file_bytes(path: &Path) -> Result<Vec<u8>> {
    ordinary(path)?;
    let m = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !m.is_file() || m.len() > MAX_BYTES as u64 {
        return Err("SQLite input is not a bounded ordinary file".into());
    }
    std::fs::read(path).map_err(|e| e.to_string())
}

pub(crate) fn integer(value: u64) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| "SQLite integer limit exceeded; value preserved without truncation".into())
}

pub(crate) fn unsigned(row: &Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

fn sql<T>(value: rusqlite::Result<T>) -> Result<T> {
    value.map_err(|e| match e.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            format!("archive writer lock is held or database busy: {e}")
        }
        _ => format!("SQLite archive: {e}"),
    })
}

pub fn connect(path: &Path, write: bool) -> Result<Connection> {
    ordinary(path)?;
    for suffix in ["-journal", "-wal", "-shm"] {
        ordinary(&path.with_file_name(format!(
                "{}{}",
                path.file_name()
                    .ok_or("database name missing")?
                    .to_string_lossy(),
                suffix
            )))?;
    }
    let m = std::fs::metadata(path)
        .map_err(|e| format!("SQLite archive missing: {e}; explicit init or migration required"))?;
    if !m.is_file() || m.len() > MAX_BYTES as u64 {
        return Err("SQLite archive is not a bounded ordinary database".into());
    }
    let flags = if write {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let conn = sql(Connection::open_with_flags(
        path,
        flags | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ))?;
    configure(&conn, write)?;
    check_format(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection, write: bool) -> Result<()> {
    sql(conn.busy_timeout(Duration::from_millis(1500)))?;
    sql(conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF;"))?;
    if write {
        sql(conn.execute_batch("PRAGMA synchronous=FULL;"))?;
        let size: i64 = sql(conn.query_row("PRAGMA page_size", [], |r| r.get(0)))?;
        let page_size = usize::try_from(size).map_err(|_| "invalid SQLite page size")?;
        if page_size == 0 {
            return Err("invalid SQLite page size".into());
        }
        sql(conn.execute_batch(&format!("PRAGMA max_page_count={};", MAX_BYTES / page_size)))?;
    } else {
        sql(conn.execute_batch("PRAGMA query_only=ON;"))?;
    }
    Ok(())
}

pub fn create(snapshot: &Snapshot) -> Result<Connection> {
    let mut conn = sql(Connection::open_in_memory())?;
    configure(&conn, true)?;
    sql(conn.execute_batch(&format!(
        "PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version={VERSION};"
    )))?;
    schema::create(&conn)?;
    crate::tracking::create_schema(&conn)?;
    let tx = sql(conn.transaction())?;
    append_revision(&tx, snapshot, None)?;
    sql(tx.commit())?;
    Ok(conn)
}

pub fn check_format(conn: &Connection) -> Result<()> {
    let app: i64 = sql(conn.query_row("PRAGMA application_id", [], |r| r.get(0)))?;
    let version: i64 = sql(conn.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    if app != APPLICATION_ID || version != VERSION {
        return Err(format!(
            "unsupported SQLite archive format/application: {app}/{version}; compatible reader required"
        ));
    }
    schema::validate(conn)?;
    Ok(())
}

pub fn head(conn: &Connection) -> Result<Head> {
    let revision = sql(conn.query_row(
        "SELECT revision FROM archive_meta WHERE singleton=1",
        [],
        |r| unsigned(r, 0),
    ))?;
    let h = history::head_at(conn, revision)?;
    let archive: String = sql(conn.query_row(
        "SELECT archive_id FROM archive_meta WHERE singleton=1",
        [],
        |r| r.get(0),
    ))?;
    if archive != h.archive_id {
        return Err("SQLite metadata archive identity mismatch".into());
    }
    Ok(h)
}

pub fn read(conn: &Connection) -> Result<Snapshot> {
    check_format(conn)?;
    let h = head(conn)?;
    let snapshot = Snapshot {
        schema: h.schema,
        archive_id: h.archive_id.clone(),
        revision: h.revision,
        parent: history::parent_at(conn, h.revision)?,
        state: entities::load(conn)?,
    };
    verify_snapshot(conn, &snapshot, &h)?;
    entities::verify_numbers(conn)?;
    crate::tracking::validate_shared(conn, &snapshot.state)?;
    Ok(snapshot)
}

pub fn read_snapshot(conn: &Connection) -> Result<Snapshot> {
    read(conn)
}

pub fn read_at(conn: &Connection, revision: u64) -> Result<Snapshot> {
    let h = history::head_at(conn, revision)?;
    let snapshot = Snapshot {
        schema: h.schema,
        archive_id: h.archive_id.clone(),
        revision,
        parent: history::parent_at(conn, revision)?,
        state: history::state_at(conn, revision)?,
    };
    verify_snapshot(conn, &snapshot, &h)?;
    Ok(snapshot)
}

fn verify_snapshot(conn: &Connection, snapshot: &Snapshot, h: &Head) -> Result<()> {
    snapshot.state.validate()?;
    match &snapshot.parent {
        None if snapshot.revision == 0 => {}
        Some(parent)
            if snapshot.revision > 0
                && parent.revision == snapshot.revision - 1
                && parent.schema == snapshot.schema
                && parent.archive_id == snapshot.archive_id => {}
        _ => return Err("SQLite revision parent identity is invalid".into()),
    }
    let raw: Option<Vec<u8>> = sql(conn
        .query_row(
            "SELECT bytes FROM legacy_revisions WHERE revision=?1",
            [integer(snapshot.revision)?],
            |r| r.get(0),
        )
        .optional())?;
    let bytes = if let Some(raw) = raw {
        let parsed = bridgeforge_core::json::parse_unique_json(&raw, "legacy SQLite evidence")?;
        let legacy: Snapshot = serde_json::from_value(parsed).map_err(|e| e.to_string())?;
        if serde_json::to_vec(&legacy).map_err(|e| e.to_string())?
            != serde_json::to_vec(snapshot).map_err(|e| e.to_string())?
        {
            return Err("SQLite history differs from original legacy revision".into());
        }
        raw
    } else {
        serde_json::to_vec(snapshot).map_err(|e| e.to_string())?
    };
    if snapshot.schema != 1
        || !crate::valid_id(&snapshot.archive_id)
        || crate::digest(&bytes) != h.digest
    {
        return Err("SQLite archive logical revision digest mismatch".into());
    }
    Ok(())
}

/// The caller owns one SQLite transaction. All state rows, history, numbering,
/// and the caller's structured events are committed together.
pub fn append_revision(
    conn: &Connection,
    snapshot: &Snapshot,
    legacy: Option<&[u8]>,
) -> Result<Head> {
    snapshot.state.validate()?;
    let old: Option<u64> = sql(conn
        .query_row(
            "SELECT revision FROM archive_meta WHERE singleton=1",
            [],
            |r| unsigned(r, 0),
        )
        .optional())?;
    match old {
        Some(revision)
            if snapshot.revision == revision.checked_add(1).ok_or("revision overflow")?
                && snapshot.parent.as_ref() == Some(&history::head_at(conn, revision)?)
                && snapshot.parent.as_ref().is_some_and(|p| {
                    p.archive_id == snapshot.archive_id && p.schema == snapshot.schema
                }) => {}
        None if snapshot.revision == 0 && snapshot.parent.is_none() => {}
        _ => return Err("SQLite revision is not a direct child of the current archive".into()),
    }
    let bytes = legacy
        .map(ToOwned::to_owned)
        .unwrap_or(serde_json::to_vec(snapshot).map_err(|e| e.to_string())?);
    let h = Head {
        schema: snapshot.schema,
        archive_id: snapshot.archive_id.clone(),
        revision: snapshot.revision,
        digest: crate::digest(&bytes),
    };
    history::store_delta(conn, snapshot)?;
    entities::save(conn, &snapshot.state, snapshot.revision)?;
    history::insert(conn, &h, snapshot.parent.as_ref())?;
    if let Some(bytes) = legacy {
        sql(conn.execute(
            "INSERT INTO legacy_revisions(revision,bytes) VALUES(?1,?2)",
            params![integer(snapshot.revision)?, bytes],
        ))?;
    }
    sql(conn.execute("INSERT INTO archive_meta(singleton,archive_id,revision,next_task_no) VALUES(1,?1,?2,(SELECT COALESCE(MAX(task_no),0)+1 FROM task_numbers)) ON CONFLICT(singleton) DO UPDATE SET archive_id=excluded.archive_id,revision=excluded.revision,next_task_no=excluded.next_task_no", params![snapshot.archive_id,integer(snapshot.revision)?]))?;
    verify_snapshot(conn, snapshot, &h)?;
    crate::tracking::validate_shared(conn, &snapshot.state)?;
    Ok(h)
}

pub fn verify_all(conn: &Connection) -> Result<Snapshot> {
    check_format(conn)?;
    let check: String = sql(conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)))?;
    if check != "ok" {
        return Err(format!("SQLite integrity check failed: {check}"));
    }
    let mut fk = sql(conn.prepare("PRAGMA foreign_key_check"))?;
    if sql(fk.query([]))?
        .next()
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("SQLite archive foreign-key check failed".into());
    }
    let h = head(conn)?;
    for revision in 0..=h.revision {
        let snapshot = read_at(conn, revision)?;
        if revision > 0 && snapshot.parent.as_ref() != Some(&history::head_at(conn, revision - 1)?)
        {
            return Err("broken SQLite archive ancestry".into());
        }
    }
    entities::verify_numbers(conn)?;
    read(conn)
}

pub(crate) fn open_bytes(bytes: &[u8], write: bool) -> Result<Connection> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES || !bytes.starts_with(b"SQLite format 3\0") {
        return Err("invalid or oversized SQLite snapshot".into());
    }
    let mut conn = sql(Connection::open_in_memory())?;
    sql(conn.deserialize_read_exact(rusqlite::MAIN_DB, Cursor::new(bytes), bytes.len(), !write))?;
    configure(&conn, write)?;
    check_format(&conn)?;
    Ok(conn)
}

pub fn inspect_bytes(bytes: &[u8]) -> Result<Snapshot> {
    verify_all(&open_bytes(bytes, false)?)
}
pub fn inspect_shared_bytes(bytes: &[u8]) -> Result<Snapshot> {
    let conn = open_bytes(bytes, false)?;
    let local = sql(conn.query_row("SELECT COUNT(*) FROM local_records", [], |r| unsigned(r, 0)))?;
    if local != 0 {
        return Err("shared SQLite snapshot contains local-only recovery records".into());
    }
    verify_all(&conn)
}
pub fn head_bytes(bytes: &[u8]) -> Result<Head> {
    head(&open_bytes(bytes, false)?)
}

pub fn require_ancestor_bytes(ancestor: &Head, bytes: &[u8]) -> Result<()> {
    let conn = open_bytes(bytes, false)?;
    let h = head(&conn)?;
    if ancestor.archive_id != h.archive_id
        || ancestor.revision > h.revision
        || history::head_at(&conn, ancestor.revision)? != *ancestor
    {
        return Err("archive history diverged or would roll back; no automatic merge".into());
    }
    verify_all(&conn)?;
    Ok(())
}

pub fn derive_bytes(bytes: &[u8], state: State) -> Result<(Head, Vec<u8>)> {
    let mut conn = open_bytes(bytes, true)?;
    let before = read(&conn)?;
    let snapshot = Snapshot {
        schema: 1,
        archive_id: before.archive_id,
        revision: before.revision.checked_add(1).ok_or("revision overflow")?,
        parent: Some(head(&conn)?),
        state,
    };
    let tx = sql(conn.transaction())?;
    let h = append_revision(&tx, &snapshot, None)?;
    sql(tx.commit())?;
    Ok((h, snapshot_bytes(&conn)?))
}

/// Rebuild the explicit shared table set in a new database. Local recovery rows
/// never enter the destination, including unused SQLite pages.
pub fn snapshot_bytes(conn: &Connection) -> Result<Vec<u8>> {
    let _read = if conn.is_autocommit() {
        Some(sql(conn.unchecked_transaction())?)
    } else {
        None
    };
    verify_all(conn)?;
    let mut dest = sql(Connection::open_in_memory())?;
    configure(&dest, true)?;
    sql(dest.execute_batch(&format!(
        "PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version={VERSION};"
    )))?;
    schema::create(&dest)?;
    crate::tracking::create_schema(&dest)?;
    schema::copy_shared(conn, &mut dest)?;
    verify_all(&dest)?;
    let bytes = sql(dest.serialize(rusqlite::MAIN_DB))?.to_vec();
    if bytes.len() > MAX_BYTES {
        return Err("SQLite snapshot exceeds transport bound".into());
    }
    Ok(bytes)
}

pub fn local_records(conn: &Connection) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut q = sql(conn.prepare("SELECT key,value FROM local_records ORDER BY key"))?;
    let rows = sql(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?))))?;
    rows.map(|r| r.map_err(|e| e.to_string())).collect()
}

pub fn restore_local(bytes: &[u8], local: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    restore_local_limited(bytes, local, MAX_BYTES)
}

pub(crate) fn restore_local_limited(
    bytes: &[u8],
    local: &BTreeMap<String, Vec<u8>>,
    limit: usize,
) -> Result<Vec<u8>> {
    if limit > MAX_BYTES || bytes.len() > limit {
        return Err("SQLite restore candidate exceeds archive bound".into());
    }
    let conn = open_bytes(bytes, true)?;
    for (key, value) in local {
        sql(conn.execute(
            "INSERT INTO local_records(key,value) VALUES(?1,?2)",
            params![key, value],
        ))?;
    }
    verify_all(&conn)?;
    let restored = sql(conn.serialize(rusqlite::MAIN_DB))?.to_vec();
    if restored.len() > limit {
        return Err(
            "SQLite restore candidate exceeds archive bound after preserving local records".into(),
        );
    }
    // Validate the exact serialized image which will be atomically installed,
    // rather than relying solely on the in-memory connection before serialize.
    inspect_bytes(&restored)?;
    Ok(restored)
}

pub(crate) fn import_history(history: &[(Snapshot, Head, Vec<u8>)]) -> Result<Connection> {
    let mut conn = sql(Connection::open_in_memory())?;
    configure(&conn, true)?;
    sql(conn.execute_batch(&format!(
        "PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version={VERSION};"
    )))?;
    schema::create(&conn)?;
    crate::tracking::create_schema(&conn)?;
    let tx = sql(conn.transaction())?;
    for (snapshot, expected, raw) in history {
        if append_revision(&tx, snapshot, Some(raw))? != *expected {
            return Err("legacy import digest changed".into());
        }
    }
    sql(tx.commit())?;
    verify_all(&conn)?;
    Ok(conn)
}
