//! Explicit, resumable one-way conversion of a verified legacy JSON chain.
//! Original files are moved without rewriting into a protected offline backup.
use crate::{
    Result, database, digest,
    store::{Head, RETIRED_HEAD, Snapshot, Store, ordinary, read_bytes, read_json},
    valid_id,
};
use bridgeforge_core::persistence;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

type Legacy = Vec<(Snapshot, Head, Vec<u8>)>;
const CANDIDATE: &str = "bridgepilot.migration.sqlite3";
const JOURNAL: &str = "migration.activation";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Activation {
    schema: u32,
    head: Head,
    source: String,
    backup: String,
}

fn legacy_at(archive: &Path) -> Result<Legacy> {
    ordinary(archive)?;
    let head: Head = read_json(&archive.join("HEAD.json"))?;
    let mut cursor = head.clone();
    let mut history = Vec::new();
    let mut total = 0_usize;
    let mut names = BTreeSet::new();
    loop {
        if cursor.schema != 1
            || !valid_id(&cursor.archive_id)
            || cursor.archive_id != head.archive_id
            || cursor.digest.len() != 64
            || !cursor.digest.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("invalid legacy archive head identity".into());
        }
        let name = format!("{}-{}.json", cursor.revision, cursor.digest);
        if !names.insert(name.clone()) {
            return Err("legacy archive history cycle".into());
        }
        let bytes = read_bytes(&archive.join("revisions").join(&name))?;
        total = total
            .checked_add(bytes.len())
            .ok_or("legacy history size overflow")?;
        if total > database::MAX_BYTES / 2 {
            return Err("legacy history exceeds bounded migration size".into());
        }
        if digest(&bytes) != cursor.digest {
            return Err("legacy archive revision digest mismatch".into());
        }
        let value = bridgeforge_core::json::parse_unique_json(&bytes, "legacy task revision")?;
        let snapshot: Snapshot = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if snapshot.schema != cursor.schema
            || snapshot.archive_id != cursor.archive_id
            || snapshot.revision != cursor.revision
        {
            return Err("legacy revision identity mismatch".into());
        }
        snapshot.state.validate()?;
        let parent = snapshot.parent.clone();
        history.push((snapshot, cursor.clone(), bytes));
        match parent {
            Some(p)
                if cursor.revision > 0
                    && p.revision == cursor.revision - 1
                    && p.archive_id == head.archive_id =>
            {
                cursor = p
            }
            None if cursor.revision == 0 => break,
            _ => return Err("broken legacy archive history chain".into()),
        }
        if history.len() > 100_000 {
            return Err("legacy history exceeds migration bound".into());
        }
    }
    let revisions = archive.join("revisions");
    ordinary(&revisions)?;
    for entry in fs::read_dir(&revisions).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        ordinary(&entry.path())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file()
            || !names.remove(&entry.file_name().to_string_lossy().into_owned())
        {
            return Err(
                "unreferenced or unowned legacy revision preserved; migration blocked".into(),
            );
        }
    }
    if !names.is_empty() {
        return Err("legacy revision files disappeared during migration preflight".into());
    }
    history.reverse();
    Ok(history)
}

pub fn read_legacy(store: &Store) -> Result<Snapshot> {
    if store.archive.join(CANDIDATE).exists() {
        return Err(
            "archive migration is interrupted; explicit migration recovery required".into(),
        );
    }
    legacy_at(&store.archive)?
        .pop()
        .map(|r| r.0)
        .ok_or("legacy archive missing".into())
}

pub fn preview(store: &Store) -> Result<Value> {
    if store.is_sqlite() {
        let conn = store.read_connection()?;
        let snapshot = database::verify_all(&conn)?;
        return Ok(
            json!({"schema":1,"status":"sqlite_archive_current","archive":snapshot.archive_id,"revision":snapshot.revision,"writes":false}),
        );
    }
    let history = legacy_at(&store.archive)?;
    let (last, head, _) = history.last().ok_or("legacy archive is empty")?;
    let orphaned = store.archive.join(CANDIDATE).exists();
    Ok(
        json!({"schema":1,"status":"legacy_migration_preview","archive":last.archive_id,"revision":last.revision,
        "legacy_head":head,"revisions":history.len(),"projects":last.state.projects.len(),"tasks":last.state.tasks.len(),
        "runs":last.state.runs.len(),"operations":last.state.operations.len(),"database":store.database_path(),
        "recovery_required":orphaned,"writes":false,"execution_started":false}),
    )
}

fn permitted(store: &Store, snapshot: &Snapshot) -> Result<()> {
    if snapshot.state.runs.values().any(|r| r.state.active()) {
        return Err(
            "finish or explicitly reconcile active/unknown execution before migration".into(),
        );
    }
    if store.runtime.join("transport/pending.json").exists() {
        return Err("legacy transport outcome unresolved; reconcile before migration".into());
    }
    if snapshot.state.custody.is_some() {
        crate::handoff::require_owner(&snapshot.state, &store.machine()?)?;
    }
    Ok(())
}

pub fn apply(store: &Store, expected: u64, source: &str) -> Result<Value> {
    apply_inner(store, expected, source, |_| Ok(()))
}

fn apply_inner(
    store: &Store,
    expected: u64,
    source: &str,
    mut boundary: impl FnMut(&str) -> Result<()>,
) -> Result<Value> {
    if source.trim().is_empty() {
        return Err("migration requires actual user authorization source".into());
    }
    let _lock = store.writer_lock()?;
    if store.is_sqlite() {
        let snapshot = store.read()?;
        if snapshot.revision != expected {
            return Err("archive revision changed since migration request".into());
        }
        return Ok(
            json!({"schema":1,"status":"sqlite_archive_current","archive":snapshot.archive_id,"revision":snapshot.revision,"changed":false}),
        );
    }
    let candidate = store.archive.join(CANDIDATE);
    ordinary(&candidate)?;
    if candidate.exists() {
        return Err(
            "migration candidate already exists; explicit recover instead of rebuilding it".into(),
        );
    }
    let history = legacy_at(&store.archive)?;
    let (snapshot, head, _) = history.last().ok_or("legacy archive empty")?;
    if snapshot.revision != expected {
        return Err(format!(
            "archive revision changed; expected {expected}, current {}",
            snapshot.revision
        ));
    }
    permitted(store, snapshot)?;
    let conn = database::import_history(&history)?;
    let journal = Activation {
        schema: 1,
        head: head.clone(),
        source: source.into(),
        backup: format!(
            "legacy-json-backup/{}-{}",
            head.revision,
            &head.digest[..16]
        ),
    };
    let payload = serde_json::to_vec(&journal).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO local_records(key,value) VALUES(?1,?2)",
        params![JOURNAL, payload],
    )
    .map_err(|e| e.to_string())?;
    let candidate_bytes = conn
        .serialize(rusqlite::MAIN_DB)
        .map_err(|e| e.to_string())?
        .to_vec();
    persistence::atomic_write(&candidate, &candidate_bytes).map_err(|e| e.to_string())?;
    boundary("candidate_saved")?;
    finish(store, &candidate_bytes, &journal, &mut boundary)
}

fn candidate_journal(bytes: &[u8]) -> Result<Activation> {
    // Local records are read from a disposable in-memory database, never from a
    // writable copy of the user's original SQLite archive.
    let conn = database::open_bytes(bytes, true)?;
    database::verify_all(&conn)?;
    let payload: Vec<u8> = conn
        .query_row(
            "SELECT value FROM local_records WHERE key=?1",
            [JOURNAL],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let journal: Activation = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
    let expected = format!(
        "legacy-json-backup/{}-{}",
        journal.head.revision,
        &journal.head.digest[..journal.head.digest.len().min(16)]
    );
    if journal.schema != 1
        || journal.source.trim().is_empty()
        || journal.backup != expected
        || journal.head != database::head(&conn)?
    {
        return Err("migration activation journal does not match its candidate".into());
    }
    Ok(journal)
}

pub fn recover(store: &Store, source: &str) -> Result<Value> {
    if source.trim().is_empty() {
        return Err("explicit migration recovery source required".into());
    }
    let _lock = store.writer_lock()?;
    let candidate = store.archive.join(CANDIDATE);
    ordinary(&candidate)?;
    if !candidate.exists() {
        if store.is_sqlite() {
            let snapshot = store.read()?;
            return Ok(
                json!({"schema":1,"status":"sqlite_archive_current","archive":snapshot.archive_id,"revision":snapshot.revision,"recovered":false}),
            );
        }
        return Err("no migration candidate to recover".into());
    }
    let bytes = database::file_bytes(&candidate)?;
    let journal = candidate_journal(&bytes)?;
    if store.is_sqlite() {
        let head = store.head()?;
        if head != journal.head {
            return Err(
                "new SQLite history has advanced; preserve candidate and current archive".into(),
            );
        }
        store.read()?;
        fs::remove_file(candidate).map_err(|e| e.to_string())?;
        return Ok(
            json!({"schema":1,"status":"migration_recovered","archive":head.archive_id,"revision":head.revision,"recovered":true}),
        );
    }
    finish(store, &bytes, &journal, &mut |_| Ok(()))
}

fn finish(
    store: &Store,
    bytes: &[u8],
    journal: &Activation,
    boundary: &mut impl FnMut(&str) -> Result<()>,
) -> Result<Value> {
    let backup = store.archive.join(&journal.backup);
    ordinary(&backup)?;
    fs::create_dir_all(&backup).map_err(|e| e.to_string())?;
    let head = store.archive.join("HEAD.json");
    let saved_head = backup.join("HEAD.json");
    ordinary(&head)?;
    ordinary(&saved_head)?;
    if !saved_head.exists() {
        if read_json::<Head>(&head)? != journal.head {
            return Err("legacy HEAD changed after migration candidate was prepared".into());
        }
        fs::rename(&head, &saved_head).map_err(|e| e.to_string())?;
    } else if read_json::<Head>(&saved_head)? != journal.head {
        return Err("legacy backup HEAD conflicts with migration candidate".into());
    }
    boundary("head_backed_up")?;
    // Install the old-reader/init guard while revisions still exists. Otherwise
    // moving both old entry points would briefly look like an empty archive to
    // an already-published legacy initializer after a crash.
    if head.exists() && read_bytes(&head)? != RETIRED_HEAD {
        return Err("legacy HEAD was externally recreated during migration".into());
    }
    store.protect_legacy()?;
    boundary("guard_installed")?;
    let revisions = store.archive.join("revisions");
    let saved_revisions = backup.join("revisions");
    ordinary(&revisions)?;
    ordinary(&saved_revisions)?;
    if !saved_revisions.exists() {
        fs::rename(&revisions, &saved_revisions).map_err(|e| e.to_string())?;
    } else if revisions.exists() {
        return Err("both legacy revision locations exist; preserve external edits".into());
    }
    let saved = legacy_at(&backup)?;
    if saved.last().map(|r| &r.1) != Some(&journal.head) {
        return Err("legacy offline backup identity differs from candidate".into());
    }
    boundary("legacy_backed_up")?;
    if read_bytes(&head)? != RETIRED_HEAD {
        return Err("legacy guard changed during backup; preserve all files".into());
    }
    if store.database_path().exists() {
        return Err("unexpected online database appeared during migration".into());
    }
    persistence::atomic_write(&store.database_path(), bytes).map_err(|e| e.to_string())?;
    boundary("sqlite_activated")?;
    let snapshot = store.read()?;
    if store.head()? != journal.head {
        return Err("activated SQLite head differs from verified legacy history".into());
    }
    fs::remove_file(store.archive.join(CANDIDATE)).map_err(|e| e.to_string())?;
    Ok(
        json!({"schema":1,"status":"archive_migrated","backend":"sqlite","archive":snapshot.archive_id,"revision":snapshot.revision,
        "database":store.database_path(),"backup":backup,"legacy_preserved":true,"execution_started":false,"source":journal.source}),
    )
}

/// Legacy transport remains read-only so the first SQLite publication can prove
/// its ancestry against the existing private remote without rewriting history.
pub fn legacy_files(files: &BTreeMap<String, Vec<u8>>) -> Result<(Snapshot, Head)> {
    let head_bytes = files
        .get("HEAD.json")
        .ok_or("legacy transport HEAD missing")?;
    let head: Head = serde_json::from_value(bridgeforge_core::json::parse_unique_json(
        head_bytes,
        "legacy transport HEAD",
    )?)
    .map_err(|e| e.to_string())?;
    let mut cursor = head.clone();
    let mut latest = None;
    let mut count = 1;
    loop {
        if cursor.schema != 1
            || !valid_id(&cursor.archive_id)
            || cursor.archive_id != head.archive_id
            || cursor.digest.len() != 64
            || !cursor.digest.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("invalid legacy transport identity".into());
        }
        let name = format!("revisions/{}-{}.json", cursor.revision, cursor.digest);
        let bytes = files.get(&name).ok_or("legacy transport history missing")?;
        if bytes.len() > 16 * 1024 * 1024 || digest(bytes) != cursor.digest {
            return Err("legacy transport digest mismatch".into());
        }
        let snapshot: Snapshot = serde_json::from_value(bridgeforge_core::json::parse_unique_json(
            bytes,
            "legacy transport revision",
        )?)
        .map_err(|e| e.to_string())?;
        if snapshot.schema != 1
            || snapshot.archive_id != head.archive_id
            || snapshot.revision != cursor.revision
        {
            return Err("legacy transport revision identity mismatch".into());
        }
        snapshot.state.validate()?;
        if latest.is_none() {
            latest = Some(snapshot.clone());
        }
        count += 1;
        match snapshot.parent {
            Some(parent) if cursor.revision > 0 && parent.revision == cursor.revision - 1 => {
                cursor = parent
            }
            None if cursor.revision == 0 => break,
            _ => return Err("legacy transport ancestry broken".into()),
        }
    }
    if count != files.len() {
        return Err("unowned or unreferenced legacy transport files".into());
    }
    Ok((latest.ok_or("empty legacy transport")?, head))
}

#[cfg(test)]
#[path = "../../../scripts/tests/orchestrator/migration.rs"]
mod tests;
