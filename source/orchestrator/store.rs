use crate::{Result, digest, domain::State, new_id, valid_id};
use bridgeforge_core::{file_lock::FileLock, persistence};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const SCHEMA: u32 = 1;
const MAX_BYTES: u64 = 16 * 1024 * 1024;

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
    archive: PathBuf,
    runtime: PathBuf,
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

fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    ordinary(path)?;
    let metadata = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
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
        fs::create_dir_all(store.runtime.join("locks")).map_err(|e| e.to_string())?;
        let _lock = FileLock::acquire(&store.runtime.join("locks/writer.lock"))?;
        if store.archive.join("HEAD.json").exists() {
            store.read()?;
            return Ok(store);
        }
        let revisions = store.archive.join("revisions");
        if revisions.exists()
            && fs::read_dir(&revisions)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
        {
            return Err(
                "uncommitted archive files exist without HEAD; preserve for diagnosis".into(),
            );
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
        fs::create_dir_all(&revisions).map_err(|e| e.to_string())?;
        let index = store.workspace.join("doc/README.md");
        if !index.exists() {
            persistence::atomic_write(&index, b"---\ndelivery_layout: flat\n---\n\n# BridgePilot workspace\n\n- [Task archive](5_project_knowledgebase/bridgepilot/README.md)\n").map_err(|e| e.to_string())?;
        }
        let ignore = store.workspace.join(".gitignore");
        if !ignore.exists() {
            persistence::atomic_write(&ignore, b"/.runtime/\n/.deps/\n/.build/\n")
                .map_err(|e| e.to_string())?;
        }
        let readme = store.archive.join("README.md");
        if !readme.exists() {
            persistence::atomic_write(&readme, b"# BridgePilot task archive\n\nHEAD.json selects the committed immutable revision. Do not edit revision files. Use bp status and bp actions. Local logs and indexes are disposable.\n").map_err(|e| e.to_string())?;
        }
        let snapshot = Snapshot {
            schema: SCHEMA,
            archive_id: new_id("archive"),
            revision: 0,
            parent: None,
            state: State::default(),
        };
        let (head, bytes) = Self::encode(&snapshot)?;
        store.install_generation(&head, &bytes)?;
        persistence::atomic_write_json(&store.archive.join("HEAD.json"), &head)
            .map_err(|e| e.to_string())?;
        Ok(store)
    }

    fn encode(snapshot: &Snapshot) -> Result<(Head, Vec<u8>)> {
        snapshot.state.validate()?;
        let bytes = serde_json::to_vec(snapshot).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("archive revision exceeds size limit".into());
        }
        Ok((
            Head {
                schema: SCHEMA,
                archive_id: snapshot.archive_id.clone(),
                revision: snapshot.revision,
                digest: digest(&bytes),
            },
            bytes,
        ))
    }

    pub(crate) fn head(&self) -> Result<Head> {
        read_json(&self.archive.join("HEAD.json"))
    }

    pub(crate) fn writer_lock(&self) -> Result<FileLock> {
        ordinary(&self.runtime)?;
        FileLock::acquire(&self.runtime.join("locks/writer.lock"))
    }

    pub(crate) fn archive_files(&self) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
        self.read()?;
        let mut files = std::collections::BTreeMap::new();
        let mut cursor = self.head()?;
        files.insert(
            "HEAD.json".into(),
            serde_json::to_vec(&cursor).map_err(|e| e.to_string())?,
        );
        loop {
            let path = self.path(&cursor)?;
            let bytes = read_bytes(&path)?;
            let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let key = format!(
                "revisions/{}",
                path.file_name()
                    .unwrap()
                    .to_str()
                    .ok_or("invalid archive name")?
            );
            files.insert(key, bytes);
            match snapshot.parent {
                Some(parent) => cursor = parent,
                None => break,
            }
        }
        Ok(files)
    }

    pub(crate) fn derived(&self, state: State) -> Result<(Head, Vec<u8>)> {
        let before = self.read()?;
        let snapshot = Snapshot {
            schema: SCHEMA,
            archive_id: before.archive_id,
            revision: before.revision.checked_add(1).ok_or("revision overflow")?,
            parent: Some(self.head()?),
            state,
        };
        Self::encode(&snapshot)
    }

    // Caller holds the writer lock. Transport validates full history and exact
    // ancestry before invoking this; activation is always the last atomic write.
    pub(crate) fn install_transport(
        &self,
        files: &std::collections::BTreeMap<String, Vec<u8>>,
    ) -> Result<()> {
        let head: Head =
            serde_json::from_slice(files.get("HEAD.json").ok_or("missing transport HEAD")?)
                .map_err(|e| e.to_string())?;
        for (name, bytes) in files {
            if name == "HEAD.json" {
                continue;
            }
            let Some(file) = name.strip_prefix("revisions/") else {
                return Err("unknown archive transport path".into());
            };
            if file.contains(['/', '\\']) {
                return Err("invalid revision transport path".into());
            }
            let candidate: Snapshot = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
            let actual = Head {
                schema: SCHEMA,
                archive_id: candidate.archive_id.clone(),
                revision: candidate.revision,
                digest: digest(bytes),
            };
            let path = self.path(&actual)?;
            if path.file_name().and_then(|n| n.to_str()) != Some(file) {
                return Err("revision transport digest mismatch".into());
            }
            ordinary(&path)?;
            if path.exists() {
                if read_bytes(&path)? != *bytes {
                    return Err("immutable transport collision".into());
                }
            } else {
                persistence::atomic_write(&path, bytes).map_err(|e| e.to_string())?;
            }
        }
        persistence::atomic_write_json(&self.archive.join("HEAD.json"), &head)
            .map_err(|e| e.to_string())?;
        self.read()?;
        Ok(())
    }

    fn path(&self, head: &Head) -> Result<PathBuf> {
        if head.schema != SCHEMA
            || !valid_id(&head.archive_id)
            || head.digest.len() != 64
            || !head.digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("invalid archive HEAD identity".into());
        }
        Ok(self
            .archive
            .join("revisions")
            .join(format!("{}-{}.json", head.revision, head.digest)))
    }

    fn install_generation(&self, head: &Head, bytes: &[u8]) -> Result<()> {
        let path = self.path(head)?;
        ordinary(&path)?;
        if path.exists() {
            if read_bytes(&path)? != bytes {
                return Err("immutable revision collision".into());
            }
        } else {
            persistence::atomic_write(&path, bytes).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn read(&self) -> Result<Snapshot> {
        let head: Head = read_json(&self.archive.join("HEAD.json"))?;
        let mut cursor = head.clone();
        let mut latest = None;
        loop {
            let bytes = read_bytes(&self.path(&cursor)?)?;
            if digest(&bytes) != cursor.digest {
                return Err("revision digest mismatch; no automatic rollback".into());
            }
            let value = bridgeforge_core::json::parse_unique_json(&bytes, "BridgePilot revision")?;
            let snapshot: Snapshot = serde_json::from_value(value).map_err(|e| e.to_string())?;
            if snapshot.schema != SCHEMA
                || snapshot.archive_id != head.archive_id
                || snapshot.revision != cursor.revision
            {
                return Err("revision identity mismatch".into());
            }
            snapshot.state.validate()?;
            if latest.is_none() {
                latest = Some(snapshot.clone());
            }
            match snapshot.parent {
                Some(parent)
                    if snapshot.revision > 0
                        && parent.revision == snapshot.revision - 1
                        && parent.archive_id == head.archive_id =>
                {
                    cursor = parent
                }
                None if snapshot.revision == 0 => break,
                _ => return Err("broken archive history chain".into()),
            }
        }
        latest.ok_or_else(|| "missing archive".into())
    }

    pub fn transact<T>(
        &self,
        expected: u64,
        change: impl FnOnce(&mut State) -> Result<T>,
    ) -> Result<(Snapshot, T)> {
        self.transact_inner(expected, change, || Ok(()))
    }

    /// Retry only local CAS/lock contention; external actions stay outside.
    pub fn update<T>(&self, mut change: impl FnMut(&mut State) -> Result<T>) -> Result<T> {
        for _ in 0..20 {
            let revision = self.read()?.revision;
            match self.transact(revision, |s| change(s)) {
                Ok((_, result)) => return Ok(result),
                Err(e) if e.contains("revision changed") || e.contains("lock is held") => {
                    std::thread::sleep(std::time::Duration::from_millis(50))
                }
                Err(e) => return Err(e),
            }
        }
        Err("archive writer remained busy".into())
    }

    fn transact_inner<T>(
        &self,
        expected: u64,
        change: impl FnOnce(&mut State) -> Result<T>,
        before_head: impl FnOnce() -> Result<()>,
    ) -> Result<(Snapshot, T)> {
        ordinary(&self.runtime)?;
        let _lock = FileLock::acquire(&self.runtime.join("locks/writer.lock"))?;
        let previous: Head = read_json(&self.archive.join("HEAD.json"))?;
        let mut snapshot = self.read()?;
        if snapshot.state.custody.is_some() {
            crate::handoff::require_owner(&snapshot.state, &self.machine()?)?;
            if self
                .workspace
                .join(".runtime/bridgepilot/transport/pending.json")
                .exists()
            {
                return Err(
                    "archive transport outcome unresolved; reconcile before writing".into(),
                );
            }
        }
        if previous.revision != snapshot.revision || expected != snapshot.revision {
            return Err(format!(
                "archive revision changed; expected {expected}, current {}",
                snapshot.revision
            ));
        }
        let result = change(&mut snapshot.state)?;
        snapshot.revision = snapshot
            .revision
            .checked_add(1)
            .ok_or("revision overflow")?;
        snapshot.parent = Some(previous.clone());
        let (head, bytes) = Self::encode(&snapshot)?;
        self.install_generation(&head, &bytes)?;
        before_head()?;
        // Even non-cooperating HEAD changes are detected before committing.
        let now: Head = read_json(&self.archive.join("HEAD.json"))?;
        if now != previous {
            return Err("HEAD changed outside writer lock; preserve orphan revision".into());
        }
        persistence::atomic_write_json(&self.archive.join("HEAD.json"), &head)
            .map_err(|e| e.to_string())?;
        Ok((snapshot, result))
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
        let _lock = FileLock::acquire(&self.runtime.join("locks/writer.lock"))?;
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
        let snapshot = self.read()?;
        let path = self.runtime.join("index/tasks.json");
        ordinary(&path)?;
        persistence::atomic_write_json(&path, &serde_json::json!({"schema":1,"archive":snapshot.archive_id,"revision":snapshot.revision,"tasks":snapshot.state.tasks})).map_err(|e| e.to_string())?;
        Ok(path)
    }
}

#[cfg(test)]
#[path = "../../scripts/tests/orchestrator/store.rs"]
mod tests;
