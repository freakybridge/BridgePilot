use crate::{Result, database, domain::State, new_id, valid_id};
use bridgeforge_core::{file_lock::FileLock, persistence};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const RETIRED_HEAD: &[u8] =
    b"BridgePilot SQLite archive. Legacy JSON reader and initializer are retired.\n";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Head {
    pub schema: u32,
    pub archive_id: String,
    pub revision: u64,
    pub digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema: u32,
    pub archive_id: String,
    pub revision: u64,
    pub parent: Option<Head>,
    pub state: State,
}

#[derive(Clone, Debug)]
pub struct Store {
    pub workspace: PathBuf,
    pub(crate) archive: PathBuf,
    pub(crate) runtime: PathBuf,
}

pub fn ordinary(path: &Path) -> Result<()> {
    for p in path.ancestors() {
        match fs::symlink_metadata(p) {
            Ok(_) => {
                if persistence::is_link_or_reparse(p).map_err(|e| e.to_string())? {
                    return Err(format!("linked path rejected: {}", p.display()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", p.display())),
        }
    }
    Ok(())
}

pub(crate) fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    ordinary(path)?;
    let m = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !m.is_file() || m.len() > MAX_BYTES {
        return Err("archive entry is not a bounded ordinary file".into());
    }
    fs::read(path).map_err(|e| e.to_string())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = read_bytes(path)?;
    let value = bridgeforge_core::json::parse_unique_json(&bytes, "BridgePilot JSON")?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        if !root.is_absolute() {
            return Err("workspace must be an explicit absolute path".into());
        }
        ordinary(root)?;
        let workspace = fs::canonicalize(root).map_err(|e| e.to_string())?;
        if !workspace.is_dir() {
            return Err("workspace is not a directory".into());
        }
        if workspace.join("bridgeforge-manifest.json").exists() {
            return Err("user archive must be separate from the product repository".into());
        }
        Ok(Self {
            archive: workspace.join("doc/5_project_knowledgebase/bridgepilot"),
            runtime: workspace.join(".runtime/bridgepilot"),
            workspace,
        })
    }

    pub fn init(root: &Path) -> Result<Self> {
        if !root.is_absolute() {
            return Err("workspace must be an explicit absolute path".into());
        }
        ordinary(root)?;
        if root.join("bridgeforge-manifest.json").exists() {
            return Err("cannot initialize user state in the product repository".into());
        }
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let store = Self::open(root)?;
        ordinary(&store.archive)?;
        ordinary(&store.runtime)?;
        let candidate = store.archive.join("bridgepilot.initial.sqlite3");
        ordinary(&candidate)?;
        if store.database_path().exists() {
            if candidate.exists() {
                return store.finish_initial(&candidate);
            }
            store.read()?;
            return Ok(store);
        }
        if candidate.exists() {
            return store.finish_initial(&candidate);
        }
        if store.archive.join("HEAD.json").exists()
            || store.archive.join("revisions").exists()
            || store.archive.join("bridgepilot.migration.sqlite3").exists()
        {
            return Err("existing legacy, retired, or interrupted archive requires explicit migration/recovery; refusing empty init".into());
        }
        for layer in [
            "0_architecture",
            "1_delivery",
            "2_bugs",
            "3_reference",
            "4_archive",
            "5_project_knowledgebase",
        ] {
            fs::create_dir_all(store.workspace.join("doc").join(layer))
                .map_err(|e| e.to_string())?;
        }
        fs::create_dir_all(&store.archive).map_err(|e| e.to_string())?;
        fs::create_dir_all(store.runtime.join("locks")).map_err(|e| e.to_string())?;
        let _lock = store.writer_lock()?;
        if store.database_path().exists() {
            store.read()?;
            return Ok(store);
        }
        store.write_scaffold()?;
        let snapshot = Snapshot {
            schema: 1,
            archive_id: new_id("archive"),
            revision: 0,
            parent: None,
            state: State::default(),
        };
        let conn = database::create(&snapshot)?;
        let bytes = database::snapshot_bytes(&conn)?;
        persistence::atomic_write(&candidate, &bytes).map_err(|e| e.to_string())?;
        store.protect_legacy()?;
        persistence::atomic_write(&store.database_path(), &bytes).map_err(|e| e.to_string())?;
        fs::remove_file(candidate).map_err(|e| e.to_string())?;
        Ok(store)
    }

    fn finish_initial(self, candidate: &Path) -> Result<Self> {
        let _lock = self.writer_lock()?;
        if !candidate.exists() {
            self.read()?;
            return Ok(self);
        }
        let bytes = database::file_bytes(candidate)?;
        let snapshot = database::inspect_bytes(&bytes)?;
        if snapshot.revision != 0
            || !snapshot.state.projects.is_empty()
            || !snapshot.state.tasks.is_empty()
        {
            return Err("initialization candidate contains unexpected state".into());
        }
        if self.database_path().exists() {
            if self.head()? != database::head_bytes(&bytes)? {
                return Err(
                    "online SQLite state differs from unfinished initializer; preserve both".into(),
                );
            }
            self.read()?;
            fs::remove_file(candidate).map_err(|e| e.to_string())?;
            return Ok(self);
        }
        self.protect_legacy()?;
        persistence::atomic_write(&self.database_path(), &bytes).map_err(|e| e.to_string())?;
        fs::remove_file(candidate).map_err(|e| e.to_string())?;
        Ok(self)
    }

    fn write_scaffold(&self) -> Result<()> {
        let index = self.workspace.join("doc/README.md");
        if !index.exists() {
            persistence::atomic_write(&index,b"---\ndelivery_layout: flat\n---\n\n# BridgePilot workspace\n\n- [Task archive](5_project_knowledgebase/bridgepilot/README.md)\n").map_err(|e|e.to_string())?;
        }
        let ignore = self.workspace.join(".gitignore");
        if !ignore.exists() {
            persistence::atomic_write(&ignore, b"/.runtime/\n/.deps/\n/.build/\n")
                .map_err(|e| e.to_string())?;
        }
        let readme = self.archive.join("README.md");
        if !readme.exists() {
            persistence::atomic_write(&readme,b"# BridgePilot task archive\n\nbridgepilot.sqlite3 is the only online task state. Use BP commands; do not edit the database or retired HEAD guard.\n").map_err(|e|e.to_string())?;
        }
        Ok(())
    }

    pub fn database_path(&self) -> PathBuf {
        self.archive.join(database::FILE)
    }
    pub fn is_sqlite(&self) -> bool {
        self.database_path().is_file()
    }

    pub(crate) fn protect_legacy(&self) -> Result<()> {
        let path = self.archive.join("HEAD.json");
        ordinary(&path)?;
        if path.exists() {
            if fs::read(&path).map_err(|e| e.to_string())? != RETIRED_HEAD {
                return Err("legacy HEAD must be explicitly migrated before activation".into());
            }
        } else {
            persistence::atomic_write(&path, RETIRED_HEAD).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn require_sqlite(&self) -> Result<()> {
        ordinary(&self.database_path())?;
        if !self.database_path().is_file() {
            return Err(
                "SQLite task archive is not active; explicit legacy migration required".into(),
            );
        }
        if read_bytes(&self.archive.join("HEAD.json"))? != RETIRED_HEAD {
            return Err("SQLite archive legacy-write guard is missing or modified; explicit recovery required".into());
        }
        Ok(())
    }

    pub fn read_connection(&self) -> Result<Connection> {
        self.require_sqlite()?;
        database::connect(&self.database_path(), false)
    }
    pub fn write_connection(&self) -> Result<Connection> {
        self.require_sqlite()?;
        if self.archive.join("bridgepilot.migration.sqlite3").exists() {
            return Err("migration activation is unfinished; recover before writing".into());
        }
        if self.archive.join("bridgepilot.initial.sqlite3").exists() {
            return Err(
                "initialization activation is unfinished; run init recovery before writing".into(),
            );
        }
        database::connect(&self.database_path(), true)
    }

    pub fn read(&self) -> Result<Snapshot> {
        if !self.database_path().exists() {
            return crate::migration::read_legacy(self);
        }
        let conn = self.read_connection()?;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let snapshot = database::read(&tx)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(snapshot)
    }

    pub fn head(&self) -> Result<Head> {
        if !self.database_path().exists() {
            return read_json(&self.archive.join("HEAD.json"));
        }
        database::head(&self.read_connection()?)
    }

    pub fn writer_lock(&self) -> Result<FileLock> {
        ordinary(&self.runtime)?;
        let path = self.runtime.join("locks/writer.lock");
        ordinary(&path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        FileLock::acquire(&path)
    }

    pub fn archive_files(&self) -> Result<BTreeMap<String, Vec<u8>>> {
        let conn = self.read_connection()?;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let bytes = database::snapshot_bytes(&tx)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(BTreeMap::from([(database::FILE.into(), bytes)]))
    }

    pub fn derived(&self, state: State) -> Result<(Head, Vec<u8>)> {
        let files = self.archive_files()?;
        database::derive_bytes(files.get(database::FILE).ok_or("snapshot missing")?, state)
    }

    /// Caller owns writer_lock. Exported snapshots cannot overwrite local outbox.
    pub fn install_transport(&self, files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
        self.install_transport_inner(files, database::MAX_BYTES)
    }

    fn install_transport_inner(
        &self,
        files: &BTreeMap<String, Vec<u8>>,
        candidate_limit: usize,
    ) -> Result<()> {
        if files.len() != 1 {
            return Err("SQLite transport must contain exactly one owned snapshot".into());
        }
        let bytes = files
            .get(database::FILE)
            .ok_or("SQLite transport snapshot missing")?;
        database::inspect_shared_bytes(bytes)?;
        let local = if self.database_path().exists() {
            database::local_records(&self.read_connection()?)?
        } else {
            BTreeMap::new()
        };
        let restored = database::restore_local_limited(bytes, &local, candidate_limit)?;
        database::inspect_bytes(&restored)?;
        if self.archive.join("HEAD.json").exists()
            && read_bytes(&self.archive.join("HEAD.json"))? != RETIRED_HEAD
        {
            return Err(
                "legacy local task archive requires explicit migration before SQLite import".into(),
            );
        }
        fs::create_dir_all(&self.archive).map_err(|e| e.to_string())?;
        self.protect_legacy()?;
        persistence::atomic_write(&self.database_path(), &restored).map_err(|e| e.to_string())?;
        self.read()?;
        Ok(())
    }

    pub fn transact<T>(
        &self,
        expected: u64,
        change: impl FnOnce(&mut State) -> Result<T>,
    ) -> Result<(Snapshot, T)> {
        self.transact_with_sql(expected, |s, _| change(s))
    }

    pub fn transact_with_sql<T>(
        &self,
        expected: u64,
        change: impl FnOnce(&mut State, &Transaction<'_>) -> Result<T>,
    ) -> Result<(Snapshot, T)> {
        self.transact_sql_inner(expected, change, || Ok(()))
    }

    fn transact_sql_inner<T>(
        &self,
        expected: u64,
        change: impl FnOnce(&mut State, &Transaction<'_>) -> Result<T>,
        before_commit: impl FnOnce() -> Result<()>,
    ) -> Result<(Snapshot, T)> {
        self.require_sqlite()?;
        let _lock = self.writer_lock()?;
        let mut conn = self.write_connection()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("archive writer lock is held or SQLite unavailable: {e}"))?;
        let mut snapshot = database::read(&tx)?;
        if snapshot.state.custody.is_some() {
            crate::handoff::require_owner(&snapshot.state, &self.machine()?)?;
            let pending: Option<i64> = tx
                .query_row(
                    "SELECT 1 FROM local_records WHERE key='transport.pending'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if pending.is_some()
                || self
                    .workspace
                    .join(".runtime/bridgepilot/transport/pending.json")
                    .exists()
            {
                return Err(
                    "archive transport outcome unresolved; reconcile before writing".into(),
                );
            }
        }
        if snapshot.revision != expected {
            return Err(format!(
                "archive revision changed; expected {expected}, current {}",
                snapshot.revision
            ));
        }
        let previous = database::head(&tx)?;
        let result = change(&mut snapshot.state, &tx)?;
        snapshot.revision = snapshot
            .revision
            .checked_add(1)
            .ok_or("revision overflow")?;
        snapshot.parent = Some(previous);
        database::append_revision(&tx, &snapshot, None)?;
        before_commit()?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok((snapshot, result))
    }

    #[cfg(test)]
    fn transact_inner<T>(
        &self,
        expected: u64,
        change: impl FnOnce(&mut State) -> Result<T>,
        before_commit: impl FnOnce() -> Result<()>,
    ) -> Result<(Snapshot, T)> {
        self.transact_sql_inner(expected, |s, _| change(s), before_commit)
    }

    pub fn update<T>(&self, mut change: impl FnMut(&mut State) -> Result<T>) -> Result<T> {
        for _ in 0..20 {
            let revision = self.read()?.revision;
            match self.transact(revision, |s| change(s)) {
                Ok((_, result)) => return Ok(result),
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
        Err("archive writer remained busy".into())
    }

    pub fn machine(&self) -> Result<String> {
        #[derive(Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Machine {
            id: String,
        }
        let path = self
            .workspace
            .join(".runtime/config/bridgepilot-machine.json");
        ordinary(&path)?;
        if path.exists() {
            let value: Machine = read_json(&path)?;
            if !valid_id(&value.id) {
                return Err("invalid local machine identity".into());
            }
            return Ok(value.id);
        }
        let _lock = self.writer_lock()?;
        if path.exists() {
            return Ok(read_json::<Machine>(&path)?.id);
        }
        let value = Machine {
            id: new_id("machine"),
        };
        persistence::atomic_write_json(&path, &value).map_err(|e| e.to_string())?;
        Ok(value.id)
    }

    pub fn rebuild_index(&self) -> Result<PathBuf> {
        let conn = self.write_connection()?;
        database::verify_all(&conn)?;
        conn.execute_batch("REINDEX;").map_err(|e| e.to_string())?;
        Ok(self.database_path())
    }
}

#[cfg(test)]
#[path = "../../../scripts/tests/orchestrator/store.rs"]
mod tests;

/// Read archive state and permanent task numbers from the same SQLite snapshot.
pub fn status_snapshot(store: &Store) -> Result<(Snapshot, BTreeMap<String, u64>)> {
    let mut conn = store.read_connection()?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let snapshot = database::read_snapshot(&tx)?;
    let mut statement = tx
        .prepare("SELECT id,task_no FROM task_numbers")
        .map_err(|e| e.to_string())?;
    let numbers = statement
        .query_map([], |row| {
            let value: i64 = row.get(1)?;
            Ok((
                row.get::<_, String>(0)?,
                u64::try_from(value)
                    .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(1, value))?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()
        .map_err(|e| e.to_string())?;
    Ok((snapshot, numbers))
}
