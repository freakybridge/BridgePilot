use super::worker::PendingState;
use super::{
    Authorization, MemoryResult, MemorySyncError, atomic_write, atomic_write_json,
    is_link_or_reparse, native_memories_authorization, normalize_remote, sha256_hex, utc_now,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MARKER_NAME: &str = "state-migration.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReceipt {
    pub schema: u64,
    pub status: String,
    pub legacy_state: String,
    pub source_fingerprint: String,
    pub migrated: Vec<String>,
    pub skipped: Vec<String>,
    pub completed_utc: String,
}

pub fn legacy_state_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(".bridgeforge-codex/memory-sync")
}

pub fn status(codex_home: &Path, state_dir: &Path) -> MemoryResult<Value> {
    let legacy = legacy_state_dir(codex_home);
    let marker_path = state_dir.join(MARKER_NAME);
    let marker = read_marker(&marker_path)?;
    Ok(json!({
        "needed": legacy.is_dir() && marker.is_none(),
        "completed": marker.is_some(),
        "receipt": marker,
    }))
}

pub fn migrate_legacy_state(
    codex_home: &Path,
    state_dir: &Path,
    ledger_path: &Path,
) -> MemoryResult<Option<MigrationReceipt>> {
    let legacy = legacy_state_dir(codex_home);
    if !legacy.exists() {
        return Ok(None);
    }
    if !legacy.is_dir() || is_link_or_reparse(&legacy)? {
        return Err(MemorySyncError::new(format!(
            "legacy native memory state is unsafe: {}",
            legacy.display()
        )));
    }
    fs::create_dir_all(state_dir)?;
    if is_link_or_reparse(state_dir)? {
        return Err(MemorySyncError::new(format!(
            "native memory state is unsafe: {}",
            state_dir.display()
        )));
    }
    let _lock = acquire_lock(state_dir)?;
    let marker_path = state_dir.join(MARKER_NAME);
    if let Some(marker) = read_marker(&marker_path)? {
        return Ok(Some(marker));
    }

    let authorization = approved_authorization(ledger_path)?;
    let source_fingerprint = source_fingerprint(&legacy)?;
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| MemorySyncError::new(error.to_string()))?
        .as_nanos();
    let stage = state_dir.join(format!(".state-migration-{}-{token}", std::process::id()));
    fs::create_dir(&stage)?;
    let result = (|| {
        let mut migrated = Vec::new();
        let mut skipped = Vec::new();
        migrate_remote(&legacy, state_dir, &authorization, &mut migrated)?;
        migrate_baseline(&legacy, state_dir, &stage, &mut migrated, &mut skipped)?;
        migrate_active_conflict(&legacy, state_dir, &stage, &mut migrated, &mut skipped)?;
        migrate_pending(&legacy, state_dir, &mut migrated, &mut skipped)?;
        let receipt = MigrationReceipt {
            schema: 1,
            status: "completed".into(),
            legacy_state: legacy.display().to_string(),
            source_fingerprint,
            migrated,
            skipped,
            completed_utc: utc_now(),
        };
        atomic_write_json(&marker_path, &receipt)?;
        Ok(receipt)
    })();
    let cleanup = super::remove_directory_if_present(&stage);
    match (result, cleanup) {
        (Ok(receipt), Ok(())) => Ok(Some(receipt)),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), _) => Err(error),
    }
}

fn approved_authorization(ledger_path: &Path) -> MemoryResult<Authorization> {
    let authorization = native_memories_authorization(ledger_path)?.ok_or_else(|| {
        MemorySyncError::new("native memories automatic synchronization is not approved")
    })?;
    if authorization.decision != "approved" || authorization.remote.is_none() {
        return Err(MemorySyncError::new(
            "native memories automatic synchronization is not approved",
        ));
    }
    Ok(authorization)
}

fn acquire_lock(state_dir: &Path) -> MemoryResult<fs::File> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(lock) = super::worker::try_lock_file(&state_dir.join("state-migration.lock"))? {
            return Ok(lock);
        }
        if Instant::now() >= deadline {
            return Err(MemorySyncError::new(
                "legacy native memory state migration is already running",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read_marker(path: &Path) -> MemoryResult<Option<MigrationReceipt>> {
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_file() || is_link_or_reparse(path)? {
        return Err(MemorySyncError::new(
            "native memory state migration receipt is unsafe",
        ));
    }
    let receipt: MigrationReceipt = serde_json::from_slice(&fs::read(path)?)?;
    if receipt.schema != 1
        || receipt.status != "completed"
        || receipt.source_fingerprint.len() != 64
    {
        return Err(MemorySyncError::new(
            "native memory state migration receipt is invalid",
        ));
    }
    Ok(Some(receipt))
}

fn migrate_remote(
    legacy: &Path,
    state_dir: &Path,
    authorization: &Authorization,
    migrated: &mut Vec<String>,
) -> MemoryResult<()> {
    let source = legacy.join("remote.txt");
    if !source.is_file() || is_link_or_reparse(&source)? {
        return Err(MemorySyncError::new(
            "legacy native memory remote authorization is missing or unsafe",
        ));
    }
    let legacy_remote = normalize_remote(
        std::str::from_utf8(&fs::read(&source)?)
            .map_err(|error| MemorySyncError::new(format!("invalid remote encoding: {error}")))?,
    );
    if authorization.remote.as_deref() != Some(legacy_remote.as_str()) {
        return Err(MemorySyncError::new(
            "legacy native memory remote differs from the approved remote",
        ));
    }
    let target = state_dir.join("remote.txt");
    if target.exists() {
        if !target.is_file() || is_link_or_reparse(&target)? {
            return Err(MemorySyncError::new(
                "current native memory remote authorization is unsafe",
            ));
        }
        let current =
            normalize_remote(std::str::from_utf8(&fs::read(&target)?).map_err(|error| {
                MemorySyncError::new(format!("invalid remote encoding: {error}"))
            })?);
        if current != legacy_remote {
            return Err(MemorySyncError::new(
                "current and legacy native memory remotes differ",
            ));
        }
        return Ok(());
    }
    atomic_write(&target, format!("{legacy_remote}\n").as_bytes())?;
    migrated.push("remote".into());
    Ok(())
}

fn migrate_pending(
    legacy: &Path,
    state_dir: &Path,
    migrated: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> MemoryResult<()> {
    let source = legacy.join("pending.json");
    if !source.exists() {
        return Ok(());
    }
    if !source.is_file() || is_link_or_reparse(&source)? {
        return Err(MemorySyncError::new(
            "legacy native memory pending state is unsafe",
        ));
    }
    let legacy_pending: PendingState = match serde_json::from_slice(&fs::read(source)?) {
        Ok(value) => value,
        Err(_) => {
            skipped.push("pending-invalid".into());
            return Ok(());
        }
    };
    super::worker::merge_migrated_pending(state_dir, &legacy_pending)?;
    migrated.push("pending".into());
    Ok(())
}

fn migrate_baseline(
    legacy: &Path,
    state_dir: &Path,
    stage: &Path,
    migrated: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> MemoryResult<()> {
    let source_receipt = legacy.join("last-synced.json");
    let source_snapshot = legacy.join("last-synced-snapshot");
    if !source_receipt.exists() && !source_snapshot.exists() {
        return Ok(());
    }
    if !source_receipt.is_file() || !source_snapshot.is_dir() {
        skipped.push("baseline-incomplete".into());
        return Ok(());
    }
    if is_link_or_reparse(&source_receipt)? || is_link_or_reparse(&source_snapshot)? {
        return Err(MemorySyncError::new(
            "legacy native memory baseline package is unsafe",
        ));
    }
    let receipt: Value = serde_json::from_slice(&fs::read(&source_receipt)?)?;
    let manifest = super::read_manifest(&source_snapshot)?;
    if receipt["schemaVersion"].as_u64() != Some(2)
        || receipt["content_sha256"].as_str() != Some(manifest.content_sha256.as_str())
        || receipt["revision"].as_u64() != Some(manifest.revision)
    {
        skipped.push("baseline-invalid".into());
        return Ok(());
    }
    let target_receipt = state_dir.join("last-synced.json");
    let target_snapshot = state_dir.join("last-synced-snapshot");
    if target_receipt.exists() || target_snapshot.exists() {
        if !target_receipt.is_file() || !target_snapshot.is_dir() {
            return Err(MemorySyncError::new(
                "current native memory baseline package is incomplete",
            ));
        }
        let current: Value = serde_json::from_slice(&fs::read(&target_receipt)?)?;
        let current_manifest = super::read_manifest(&target_snapshot)?;
        if current["content_sha256"].as_str() != Some(current_manifest.content_sha256.as_str())
            || current["revision"].as_u64() != Some(current_manifest.revision)
        {
            return Err(MemorySyncError::new(
                "current native memory baseline package is invalid",
            ));
        }
        skipped.push("baseline-current-retained".into());
        return Ok(());
    }
    let staged_snapshot = stage.join("last-synced-snapshot");
    super::copy_directory(&source_snapshot, &staged_snapshot)?;
    super::read_manifest(&staged_snapshot)?;
    fs::rename(staged_snapshot, &target_snapshot)?;
    atomic_write(&target_receipt, &fs::read(source_receipt)?)?;
    migrated.push("baseline".into());
    Ok(())
}

fn migrate_active_conflict(
    legacy: &Path,
    state_dir: &Path,
    stage: &Path,
    migrated: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> MemoryResult<()> {
    let active_path = legacy.join("active-conflict.json");
    if !active_path.exists() {
        return Ok(());
    }
    if !active_path.is_file() || is_link_or_reparse(&active_path)? {
        return Err(MemorySyncError::new(
            "legacy native memory active conflict is unsafe",
        ));
    }
    if state_dir.join("active-conflict.json").exists() {
        skipped.push("active-conflict-current-retained".into());
        return Ok(());
    }
    let mut active: Value = serde_json::from_slice(&fs::read(active_path)?)?;
    let conflict_id = active["conflictId"]
        .as_str()
        .filter(|value| !value.is_empty() && value.chars().all(|item| item.is_ascii_hexdigit()))
        .ok_or_else(|| MemorySyncError::new("legacy active conflict identity is invalid"))?
        .to_string();
    if active["schemaVersion"].as_u64() != Some(1) {
        return Err(MemorySyncError::new(
            "legacy active conflict schema is invalid",
        ));
    }
    let source = legacy.join("conflicts").join(&conflict_id);
    validate_conflict(&source, &conflict_id)?;
    let target_parent = state_dir.join("conflicts");
    fs::create_dir_all(&target_parent)?;
    let target = target_parent.join(&conflict_id);
    if !target.exists() {
        let staged = stage.join("active-conflict-evidence");
        super::copy_directory(&source, &staged)?;
        validate_conflict(&staged, &conflict_id)?;
        fs::rename(staged, &target)?;
    } else {
        validate_conflict(&target, &conflict_id)?;
    }
    active["path"] = Value::String(target.display().to_string());
    atomic_write_json(&state_dir.join("active-conflict.json"), &active)?;
    migrated.push("active-conflict".into());
    Ok(())
}

fn validate_conflict(path: &Path, conflict_id: &str) -> MemoryResult<()> {
    if !path.is_dir() || is_link_or_reparse(path)? {
        return Err(MemorySyncError::new(
            "legacy native memory conflict evidence is missing or unsafe",
        ));
    }
    let record: super::ConflictRecord =
        serde_json::from_slice(&fs::read(path.join("conflict.json"))?)?;
    if record.conflict_id != conflict_id {
        return Err(MemorySyncError::new(
            "legacy native memory conflict evidence identity changed",
        ));
    }
    for snapshot in ["local", "remote", "merged"] {
        super::snapshot_files(&path.join(snapshot))?;
    }
    if path.join("base").exists() {
        super::snapshot_files(&path.join("base"))?;
    }
    Ok(())
}

fn source_fingerprint(legacy: &Path) -> MemoryResult<String> {
    let mut inputs = BTreeMap::new();
    for name in [
        "remote.txt",
        "pending.json",
        "last-synced.json",
        "last-synced-snapshot/snapshot-manifest.json",
        "active-conflict.json",
    ] {
        add_fingerprint_input(legacy, name, &mut inputs)?;
    }
    if let Some(active) = fs::read(legacy.join("active-conflict.json"))
        .ok()
        .and_then(|payload| serde_json::from_slice::<Value>(&payload).ok())
        && let Some(id) = active["conflictId"].as_str()
    {
        for name in [
            "conflict.json",
            "local/snapshot-manifest.json",
            "remote/snapshot-manifest.json",
            "base/snapshot-manifest.json",
            "merged/snapshot-manifest.json",
        ] {
            add_fingerprint_input(legacy, &format!("conflicts/{id}/{name}"), &mut inputs)?;
        }
    }
    Ok(sha256_hex(&serde_json::to_vec(&inputs)?))
}

fn add_fingerprint_input(
    root: &Path,
    relative: &str,
    inputs: &mut BTreeMap<String, String>,
) -> MemoryResult<()> {
    let path = root.join(relative);
    if !path.exists() {
        return Ok(());
    }
    if !path.is_file() || is_link_or_reparse(&path)? {
        return Err(MemorySyncError::new(format!(
            "legacy native memory migration input is unsafe: {}",
            path.display()
        )));
    }
    inputs.insert(relative.into(), sha256_hex(&fs::read(path)?));
    Ok(())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_memory_migration.rs"]
mod tests;
