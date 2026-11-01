//! Explicit read-only audit of an already migrated private workspace.
//! Compile against locked, tested BP task/storage rlibs; never activates data.
use bridgepilot_storage::database;
use bridgepilot_tasks::digest;
use bridgepilot_storage::store::{Snapshot, Store};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let workspace = PathBuf::from(args.next().ok_or("workspace required")?);
    let backup = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err("unexpected audit arguments".into());
    }
    let store = Store::open(&workspace)?;
    let before = fs::read(store.database_path())?;
    let conn = store.read_connection()?;
    let snapshot = database::verify_all(&conn)?;
    let mut audited = 0;
    if let Some(backup) = backup {
        for entry in fs::read_dir(backup.join("revisions"))? {
            let path = entry?.path();
            if !path.is_file() || path.extension().is_none_or(|e| e != "json") {
                return Err("unexpected legacy evidence path".into());
            }
            let bytes = fs::read(&path)?;
            let old: Snapshot = serde_json::from_slice(&bytes)?;
            let expected = format!("{}-{}.json", old.revision, digest(&bytes));
            if path.file_name().and_then(|n| n.to_str()) != Some(expected.as_str()) {
                return Err("legacy evidence file digest mismatch".into());
            }
            let imported = database::read_at(&conn, old.revision)?;
            if serde_json::to_value(&old)? != serde_json::to_value(&imported)? {
                return Err(format!("legacy logical revision {} differs", old.revision).into());
            }
            let original: Vec<u8> = conn.query_row(
                "SELECT bytes FROM legacy_revisions WHERE revision=?1",
                [i64::try_from(old.revision)?],
                |r| r.get(0),
            )?;
            if original != bytes {
                return Err("legacy original bytes were rewritten".into());
            }
            audited += 1;
        }
    }
    let mut stmt = conn.prepare("SELECT id,task_no FROM task_numbers ORDER BY task_no")?;
    let numbers = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let local_rows: i64 = conn.query_row("SELECT count(*) FROM local_records", [], |r| r.get(0))?;
    let head: String = conn.query_row(
        "SELECT digest FROM revisions WHERE revision=?1",
        [i64::try_from(snapshot.revision)?],
        |r| r.get(0),
    )?;
    if before != fs::read(store.database_path())? {
        return Err("read-only audit changed database bytes".into());
    }
    println!(
        "{}",
        json!({"status":"read_only_archive_audit_passed","archive":snapshot.archive_id,"revision":snapshot.revision,
        "logical_head_digest":head,"state_sha256":digest(&serde_json::to_vec(&snapshot.state)?),"legacy_revisions_compared":audited,
        "projects":snapshot.state.projects.len(),"tasks":snapshot.state.tasks.len(),"runs":snapshot.state.runs.len(),"operations":snapshot.state.operations.len(),
        "task_numbers":numbers,"local_rows":local_rows,"custody":snapshot.state.custody,"wakeups":snapshot.state.wakeups,"database_unchanged":true})
    );
    Ok(())
}
