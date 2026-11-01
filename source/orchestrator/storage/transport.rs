//! Local transport outbox transactions; never published in shared snapshots.
use crate::{Result, database, store::Store};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn local_record(store: &Store, key: &str) -> Result<Option<Vec<u8>>> {
    store
        .read_connection()?
        .query_row("SELECT value FROM local_records WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())
}
pub fn save_intent<T: Serialize>(
    store: &Store,
    pending: &T,
    candidate: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let mut conn = store.write_connection()?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(pending).map_err(|e| e.to_string())?;
    let snapshot = files
        .get(database::FILE)
        .ok_or("new transport intent requires a SQLite snapshot")?;
    tx.execute("INSERT INTO local_records(key,value) VALUES('transport.pending',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[bytes]).map_err(|e|e.to_string())?;
    tx.execute("INSERT INTO local_records(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![format!("transport.snapshot.{}",candidate),snapshot]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub fn finish_intent<T: Serialize + serde::de::DeserializeOwned + PartialEq>(
    store: &Store,
    pending: &T,
    readback: Value,
) -> Result<()> {
    let mut conn = store.write_connection()?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let actual: Vec<u8> = tx
        .query_row(
            "SELECT value FROM local_records WHERE key='transport.pending'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if serde_json::from_slice::<T>(&actual).map_err(|e| e.to_string())? != *pending {
        return Err("pending intent changed during confirmation".into());
    }
    let receipt =
        serde_json::to_vec(&json!({"status":"confirmed","operation":pending,"readback":readback}))
            .map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO local_records(key,value) VALUES('transport.last_receipt',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[receipt]).map_err(|e|e.to_string())?;
    tx.execute("DELETE FROM local_records WHERE key='transport.pending' OR key LIKE 'transport.snapshot.%'",[]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

/// Preserve an abandoned candidate before preparing its fencing publication.
pub fn abandon_intent<T: Serialize>(store: &Store, candidate: &str, pending: &T) -> Result<()> {
    store
        .write_connection()?
        .execute(
            "INSERT OR REPLACE INTO local_records(key,value) VALUES(?1,?2)",
            params![
                format!("transport.abandoned.{candidate}"),
                serde_json::to_vec(pending).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
