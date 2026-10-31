pub use bridgeforge_core::build_manifest as manifest;
pub use bridgeforge_core::{
    CommandOutcome, EXIT_BLOCKED, EXIT_OK, ProcessOutput, ProcessRequest, ProcessRunner,
    ProjectContext, SystemProcessRunner,
};
pub use bridgeforge_core::{
    artifact_cache, build_inputs, build_metrics, file_lock, generated_assets, json,
    managed_markdown, managed_paths, persistence, release_metrics, runtime, source_projection,
    user_agents,
};
pub mod background;
pub mod migration;
pub mod ownership;
pub mod remote;
pub mod user_config;
pub mod worker;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

pub const REPOSITORY: &str = "bridgeforge-codex-memories";
pub const CONSENT_POLICY_VERSION: u64 = 1;
pub const CONSENT_SCOPE: &str = "~/.codex/memories/**";
pub const CONSENT_SYNC_MODE: &str = "bidirectional";
pub const SNAPSHOT_SCHEMA_VERSION: u64 = 1;
pub const LOCAL_ONLY_MEMORY_FILES: &[&str] = &["MEMORY.md", "memory_summary.md", "raw_memories.md"];

const EXCLUDED_NAMES: &[&str] = &[
    ".DS_Store",
    "Thumbs.db",
    "desktop.ini",
    "snapshot-manifest.json",
];
const EXCLUDED_SUFFIXES: &[&str] = &[".tmp", ".temp", ".lock", ".lck", ".swp", ".part"];

#[derive(Debug)]
pub struct MemorySyncError {
    message: String,
}

impl MemorySyncError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MemorySyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MemorySyncError {}

impl From<std::io::Error> for MemorySyncError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<serde_json::Error> for MemorySyncError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<crate::persistence::PersistenceError> for MemorySyncError {
    fn from(error: crate::persistence::PersistenceError) -> Self {
        Self::new(error.to_string())
    }
}

pub type MemoryResult<T> = Result<T, MemorySyncError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Authorization {
    pub decision: String,
    pub policy_version: u64,
    pub scope: String,
    pub sync_mode: String,
    pub auto_hook_maintenance: bool,
    pub repository: String,
    pub require_private: bool,
    pub remote: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryFileEntry {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub schema_version: u64,
    pub captured_at_utc: String,
    pub updated_at_utc: String,
    pub revision: u64,
    pub content_sha256: String,
    pub files: Vec<MemoryFileEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeResult {
    pub files: BTreeMap<String, Vec<u8>>,
    pub conflicts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictRecord {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    #[serde(rename = "conflictId")]
    pub conflict_id: String,
    #[serde(rename = "createdUtc")]
    pub created_utc: String,
    pub reason: String,
    #[serde(rename = "conflictPaths")]
    pub conflict_paths: Vec<String>,
    #[serde(rename = "remoteCommit")]
    pub remote_commit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncAction {
    Push,
    Noop,
    Merge,
    Restore,
}

impl SyncAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Noop => "noop",
            Self::Merge => "merge",
            Self::Restore => "restore",
        }
    }
}

pub fn utc_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

pub fn runtime_receipt_healthy(receipt: &Value) -> bool {
    let Some(verified) = receipt["verifiedUtc"].as_str() else {
        return false;
    };
    let Ok(verified) = DateTime::parse_from_rfc3339(verified) else {
        return false;
    };
    let age = Utc::now().signed_duration_since(verified.with_timezone(&Utc));
    receipt["schema"].as_u64() == Some(1)
        && receipt["handlerRevision"].as_str() == Some(user_config::HOOK_ID)
        && receipt["lastEvent"]
            .as_str()
            .is_some_and(|event| user_config::HOOK_EVENTS.contains(&event))
        && age.num_seconds() >= 0
        && age.num_seconds() <= 24 * 60 * 60
}

pub fn record_health(
    state_dir: &Path,
    status: &str,
    detail: Option<&str>,
    action: Option<&str>,
) -> MemoryResult<Value> {
    let alert_id = matches!(status, "failed" | "conflicted").then(|| {
        format!(
            "native-memory:{status}:{}",
            &sha256_hex(detail.unwrap_or(status).as_bytes())[..16]
        )
    });
    let receipt = json!({
        "schema": 1,
        "status": status,
        "detail": detail,
        "action": action,
        "updatedUtc": utc_now(),
        "alertId": alert_id,
    });
    atomic_write_json(&state_dir.join("health.json"), &receipt)?;
    if status == "healthy" {
        match fs::remove_file(state_dir.join("alert-state.json")) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(receipt)
}

pub fn read_health(state_dir: &Path) -> MemoryResult<Option<Value>> {
    let path = state_dir.join("health.json");
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}

pub fn emit_alert_once(state_dir: &Path, alert_id: Option<&str>) -> MemoryResult<Option<String>> {
    let Some(alert_id) = alert_id else {
        return Ok(None);
    };
    let Some(_lock) = worker::ReconcileLock::try_acquire(&state_dir.join("alerts"))? else {
        return Ok(None);
    };
    let path = state_dir.join("alert-state.json");
    let previous: Option<Value> = fs::read(&path)
        .ok()
        .and_then(|payload| serde_json::from_slice(&payload).ok());
    if previous.as_ref().is_some_and(|value| {
        value["lastEmitted"].as_str() == Some(alert_id)
            || value["lastAcknowledged"].as_str() == Some(alert_id)
    }) {
        return Ok(None);
    }
    let mut receipt = previous
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({"schema":1}));
    receipt["lastEmitted"] = json!(alert_id);
    receipt["updatedUtc"] = json!(utc_now());
    atomic_write_json(&path, &receipt)?;
    Ok(Some(alert_id.to_string()))
}

pub fn acknowledge_alert(state_dir: &Path, alert_id: &str) -> MemoryResult<()> {
    atomic_write_json(
        &state_dir.join("alert-state.json"),
        &json!({
            "schema": 1,
            "lastEmitted": alert_id,
            "lastAcknowledged": alert_id,
            "updatedUtc": utc_now()
        }),
    )
}

pub fn normalize_remote(value: &str) -> String {
    let normalized = value.trim().trim_end_matches('/');
    if normalized
        .get(normalized.len().saturating_sub(4)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".git"))
    {
        normalized[..normalized.len() - 4].to_string()
    } else {
        normalized.to_string()
    }
}

pub fn remote_targets_managed_repository(value: &str) -> bool {
    github_repository_identity(value).is_ok()
}

pub fn authorization_payload(decision: &str, remote: Option<&str>) -> MemoryResult<Authorization> {
    let remote = match decision {
        "approved" => {
            let value = remote.ok_or_else(|| {
                MemorySyncError::new(
                    "approved native memories consent requires the managed repository remote",
                )
            })?;
            if !remote_targets_managed_repository(value) {
                return Err(MemorySyncError::new(
                    "approved native memories consent requires the managed repository remote",
                ));
            }
            Some(normalize_remote(value))
        }
        "declined" => {
            if remote.is_some() {
                return Err(MemorySyncError::new(
                    "declined native memories consent must not retain a remote authorization",
                ));
            }
            None
        }
        _ => {
            return Err(MemorySyncError::new(format!(
                "unsupported native memories consent: {decision}"
            )));
        }
    };
    Ok(Authorization {
        decision: decision.to_string(),
        policy_version: CONSENT_POLICY_VERSION,
        scope: CONSENT_SCOPE.to_string(),
        sync_mode: CONSENT_SYNC_MODE.to_string(),
        auto_hook_maintenance: true,
        repository: REPOSITORY.to_string(),
        require_private: true,
        remote,
    })
}

pub fn validate_authorization(value: &Value) -> MemoryResult<Authorization> {
    let object = value.as_object().ok_or_else(|| {
        MemorySyncError::new("managed ledger has invalid native memories consent")
    })?;
    const FIELDS: &[&str] = &[
        "decision",
        "policy_version",
        "scope",
        "sync_mode",
        "auto_hook_maintenance",
        "repository",
        "require_private",
        "remote",
    ];
    let actual: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = FIELDS.iter().copied().collect();
    if actual != expected {
        return Err(MemorySyncError::new(
            "managed ledger has invalid native memories consent",
        ));
    }
    let parsed: Authorization = serde_json::from_value(value.clone())
        .map_err(|_| MemorySyncError::new("managed ledger has invalid native memories consent"))?;
    let expected = authorization_payload(&parsed.decision, parsed.remote.as_deref())?;
    if parsed != expected {
        return Err(MemorySyncError::new(
            "managed ledger has invalid native memories authorization scope",
        ));
    }
    Ok(parsed)
}

pub fn managed_ledger(path: &Path) -> MemoryResult<Value> {
    if !path.is_file() || is_link_or_reparse(path)? {
        return Err(MemorySyncError::new(format!(
            "managed ledger is missing or unsafe: {}",
            path.display()
        )));
    }
    let data = ownership::load_json_object(&fs::read(path)?, &path.display().to_string())?;
    let object = data
        .as_object()
        .expect("load_json_object returns an object");
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(MemorySyncError::new(
            "managed ledger must use schema_version 1",
        ));
    }
    if object.get("platform").and_then(Value::as_str) != Some("codex")
        || !object.get("records").is_some_and(Value::is_object)
    {
        return Err(MemorySyncError::new(
            "managed ledger is not a Codex schema-v1 ledger",
        ));
    }
    let allowed: BTreeSet<&str> = ["schema_version", "platform", "records", "consents"]
        .into_iter()
        .collect();
    if object.keys().any(|key| !allowed.contains(key.as_str())) {
        return Err(MemorySyncError::new(
            "managed ledger contains unsupported top-level fields",
        ));
    }
    let name_pattern = regex::Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]*$").unwrap();
    let commit_pattern = regex::Regex::new(r"^[0-9a-f]{40}$").unwrap();
    let hash_pattern = regex::Regex::new(r"^sha256:[0-9a-f]{64}$").unwrap();
    for (name, record) in object["records"].as_object().unwrap() {
        let record = record.as_object().ok_or_else(|| {
            MemorySyncError::new(format!("managed ledger record is invalid: {name}"))
        })?;
        let valid = name_pattern.is_match(name)
            && record
                .get("source_commit")
                .and_then(Value::as_str)
                .is_some_and(|value| commit_pattern.is_match(value))
            && record
                .get("content_hash")
                .and_then(Value::as_str)
                .is_some_and(|value| hash_pattern.is_match(value))
            && record
                .get("installed_at")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty());
        if !valid {
            return Err(MemorySyncError::new(format!(
                "managed ledger record is invalid: {name}"
            )));
        }
    }
    if let Some(consents) = object.get("consents") {
        let consents = consents.as_object().ok_or_else(|| {
            MemorySyncError::new("managed ledger has invalid native memories consent")
        })?;
        if consents.len() != 1 || !consents.contains_key("native_memories") {
            return Err(MemorySyncError::new(
                "managed ledger has invalid native memories consent",
            ));
        }
        validate_authorization(&consents["native_memories"])?;
    }
    Ok(data)
}

pub fn native_memories_authorization(path: &Path) -> MemoryResult<Option<Authorization>> {
    let ledger = managed_ledger(path)?;
    let Some(consents) = ledger.get("consents").and_then(Value::as_object) else {
        return Ok(None);
    };
    validate_authorization(&consents["native_memories"]).map(Some)
}

pub fn native_memories_consent(path: &Path) -> MemoryResult<Option<String>> {
    Ok(native_memories_authorization(path)?.map(|value| value.decision))
}

pub fn record_native_memories_consent(
    path: &Path,
    decision: &str,
    confirmed: bool,
    remote: Option<&str>,
) -> MemoryResult<bool> {
    if !confirmed {
        return Err(MemorySyncError::new(
            "consent changes require explicit confirmation",
        ));
    }
    let mut ledger = managed_ledger(path)?;
    let desired = serde_json::to_value(authorization_payload(decision, remote)?)?;
    let current = ledger
        .get("consents")
        .and_then(|value| value.get("native_memories"));
    if current == Some(&desired) {
        return Ok(false);
    }
    ledger
        .as_object_mut()
        .unwrap()
        .insert("consents".to_string(), json!({"native_memories": desired}));
    atomic_write_json(path, &ledger)?;
    Ok(true)
}

pub fn require_runtime_authorization(
    ledger_path: &Path,
    remote_file: &Path,
) -> MemoryResult<Authorization> {
    let authorization = native_memories_authorization(ledger_path)?.ok_or_else(|| {
        MemorySyncError::new("native memories automatic synchronization is not approved")
    })?;
    if authorization.decision != "approved" {
        return Err(MemorySyncError::new(
            "native memories automatic synchronization is not approved",
        ));
    }
    if !remote_file.is_file() || is_link_or_reparse(remote_file)? {
        return Err(MemorySyncError::new(
            "native memories remote authorization is missing or unsafe",
        ));
    }
    let configured = normalize_remote(
        std::str::from_utf8(&strip_utf8_bom(&fs::read(remote_file)?))
            .map_err(|error| MemorySyncError::new(format!("invalid remote encoding: {error}")))?,
    );
    if !remote_targets_managed_repository(&configured) {
        return Err(MemorySyncError::new(
            "native memories remote is outside the approved repository scope",
        ));
    }
    if authorization.remote.as_deref() != Some(configured.as_str()) {
        return Err(MemorySyncError::new(
            "native memories remote changed outside the approved scope",
        ));
    }
    Ok(authorization)
}

fn memory_files_including_local_only(source: &Path) -> MemoryResult<Vec<PathBuf>> {
    ensure_real_directory(source, false)?;
    let mut files = Vec::new();
    scan_directory(source, source, &mut files)?;
    files.sort();
    Ok(files)
}

pub fn is_local_only_memory_path(path: &str) -> bool {
    !path.contains('/')
        && !path.contains('\\')
        && LOCAL_ONLY_MEMORY_FILES
            .iter()
            .any(|name| path.eq_ignore_ascii_case(name))
}

pub fn memory_files(source: &Path) -> MemoryResult<Vec<PathBuf>> {
    memory_files_including_local_only(source).map(|files| {
        files
            .into_iter()
            .filter(|path| {
                path.strip_prefix(source)
                    .ok()
                    .and_then(|relative| path_to_posix(relative).ok())
                    .is_none_or(|relative| !is_local_only_memory_path(&relative))
            })
            .collect()
    })
}

fn capture_manifest_with_local_only(
    source: &Path,
    revision: u64,
    captured_at: Option<&str>,
    include_local_only: bool,
) -> MemoryResult<SnapshotManifest> {
    ensure_real_directory(source, false)?;
    let paths = if include_local_only {
        memory_files_including_local_only(source)?
    } else {
        memory_files(source)?
    };
    let mut files = Vec::new();
    let mut newest = UNIX_EPOCH;
    for path in paths {
        let relative = path.strip_prefix(source).map_err(|error| {
            MemorySyncError::new(format!("cannot relativize native memory: {error}"))
        })?;
        let payload = fs::read(&path)?;
        let modified = fs::metadata(&path)?.modified().unwrap_or(UNIX_EPOCH);
        newest = newest.max(modified);
        files.push(MemoryFileEntry {
            path: path_to_posix(relative)?,
            sha256: sha256_hex(&payload),
        });
    }
    let content = serde_json::to_vec(&files)?;
    let updated_at = DateTime::<Utc>::from(newest).to_rfc3339_opts(SecondsFormat::AutoSi, false);
    Ok(SnapshotManifest {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        captured_at_utc: captured_at.map(str::to_string).unwrap_or_else(utc_now),
        updated_at_utc: updated_at,
        revision,
        content_sha256: sha256_hex(&content),
        files,
    })
}

pub fn capture_manifest(
    source: &Path,
    revision: u64,
    captured_at: Option<&str>,
) -> MemoryResult<SnapshotManifest> {
    capture_manifest_with_local_only(source, revision, captured_at, false)
}

pub fn build_snapshot(
    source: &Path,
    destination: &Path,
    revision: u64,
) -> MemoryResult<SnapshotManifest> {
    let parent = destination.parent().ok_or_else(|| {
        MemorySyncError::new(format!(
            "snapshot destination has no parent: {}",
            destination.display()
        ))
    })?;
    ensure_real_directory(parent, true)?;
    let mut last_error: Option<MemorySyncError> = None;
    for _ in 0..3 {
        let stage = temporary_sibling(destination, "snapshot");
        let result = (|| {
            let manifest = capture_manifest(source, revision, None)?;
            fs::create_dir(&stage)?;
            fs::create_dir(stage.join("memories"))?;
            for item in &manifest.files {
                let relative = safe_relative(&item.path)?;
                let target = stage.join("memories").join(&relative);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(source.join(&relative), target)?;
            }
            atomic_write_json(&stage.join("snapshot-manifest.json"), &manifest)?;
            verify_snapshot(&stage, &manifest)?;
            replace_directory(&stage, destination)?;
            Ok(manifest)
        })();
        match result {
            Ok(manifest) => return Ok(manifest),
            Err(error) => {
                let _ = remove_directory_if_present(&stage);
                last_error = Some(error);
            }
        }
    }
    Err(MemorySyncError::new(format!(
        "native memories changed while snapshotting: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "unknown error".to_string())
    )))
}

pub fn verify_snapshot(snapshot: &Path, manifest: &SnapshotManifest) -> MemoryResult<()> {
    verify_manifest_directory(&snapshot.join("memories"), manifest)
}

fn comparable_manifest_files(
    manifest: &SnapshotManifest,
    include_local_only: bool,
) -> Vec<&MemoryFileEntry> {
    let mut files: Vec<_> = manifest
        .files
        .iter()
        .filter(|item| include_local_only || !is_local_only_memory_path(&item.path))
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

fn same_manifest_files_exact(left: &SnapshotManifest, right: &SnapshotManifest) -> bool {
    let left_files = comparable_manifest_files(left, true);
    let right_files = comparable_manifest_files(right, true);
    left_files == right_files
}

fn same_manifest_files(left: &SnapshotManifest, right: &SnapshotManifest) -> bool {
    let left_files = comparable_manifest_files(left, false);
    let right_files = comparable_manifest_files(right, false);
    left_files == right_files
}

fn verify_manifest_directory(source: &Path, manifest: &SnapshotManifest) -> MemoryResult<()> {
    if manifest.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(MemorySyncError::new(
            "remote snapshot manifest schema is invalid",
        ));
    }
    let actual = capture_manifest_with_local_only(source, manifest.revision, None, true)?;
    // The digest authenticates the stored ordering; filesystem ordering is not
    // part of file identity and can differ across snapshot producers/platforms.
    let declared_digest = sha256_hex(&serde_json::to_vec(&manifest.files)?);
    if !same_manifest_files_exact(&actual, manifest) || declared_digest != manifest.content_sha256 {
        return Err(MemorySyncError::new(
            "remote snapshot content does not match its SHA-256 manifest",
        ));
    }
    Ok(())
}

pub fn snapshot_files(snapshot: &Path) -> MemoryResult<BTreeMap<String, Vec<u8>>> {
    let manifest_path = snapshot.join("snapshot-manifest.json");
    let manifest: SnapshotManifest =
        serde_json::from_slice(&fs::read(&manifest_path)?).map_err(|error| {
            MemorySyncError::new(format!(
                "cannot read snapshot manifest: {}: {error}",
                manifest_path.display()
            ))
        })?;
    verify_snapshot(snapshot, &manifest)?;
    manifest
        .files
        .into_iter()
        .filter(|item| !is_local_only_memory_path(&item.path))
        .map(|item| {
            let relative = safe_relative(&item.path)?;
            Ok((
                item.path,
                fs::read(snapshot.join("memories").join(relative))?,
            ))
        })
        .collect()
}

pub fn snapshot_from_files(
    destination: &Path,
    files: &BTreeMap<String, Vec<u8>>,
    revision: u64,
) -> MemoryResult<SnapshotManifest> {
    let source = temporary_sibling(destination, "source");
    fs::create_dir_all(&source)?;
    let result = (|| {
        for (relative, payload) in files {
            let relative = safe_relative(relative)?;
            let target = source.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            atomic_write(&target, payload)?;
        }
        build_snapshot(&source, destination, revision)
    })();
    let _ = remove_directory_if_present(&source);
    result
}

pub fn choose_action(local: &str, remote: Option<&str>, synced: Option<&str>) -> SyncAction {
    let Some(remote) = remote else {
        return SyncAction::Push;
    };
    if local == remote {
        return SyncAction::Noop;
    }
    let local_changed = synced.is_none_or(|value| local != value);
    let remote_changed = synced.is_none_or(|value| remote != value);
    if local_changed && remote_changed {
        SyncAction::Merge
    } else if local_changed {
        SyncAction::Push
    } else {
        SyncAction::Restore
    }
}

pub fn three_way_merge(
    base: &BTreeMap<String, Vec<u8>>,
    local: &BTreeMap<String, Vec<u8>>,
    remote: &BTreeMap<String, Vec<u8>>,
) -> MergeResult {
    let paths: BTreeSet<&String> = base
        .keys()
        .chain(local.keys())
        .chain(remote.keys())
        .collect();
    let mut files = BTreeMap::new();
    let mut conflicts = Vec::new();
    for path in paths {
        let before = base.get(path);
        let ours = local.get(path);
        let theirs = remote.get(path);
        let selected = if ours == theirs {
            Some(ours)
        } else if ours == before {
            Some(theirs)
        } else if theirs == before {
            Some(ours)
        } else {
            None
        };
        match selected {
            Some(Some(payload)) => {
                files.insert(path.clone(), payload.clone());
            }
            Some(None) => {}
            None => conflicts.push(path.clone()),
        }
    }
    MergeResult { files, conflicts }
}

pub fn bootstrap_merge(
    local: &BTreeMap<String, Vec<u8>>,
    remote: &BTreeMap<String, Vec<u8>>,
) -> MergeResult {
    let paths: BTreeSet<&String> = local.keys().chain(remote.keys()).collect();
    let mut files = BTreeMap::new();
    let mut conflicts = Vec::new();
    for path in paths {
        match (local.get(path), remote.get(path)) {
            (Some(ours), Some(theirs)) if ours == theirs => {
                files.insert(path.clone(), ours.clone());
            }
            _ => conflicts.push(path.clone()),
        }
    }
    MergeResult { files, conflicts }
}

#[allow(clippy::too_many_arguments)]
pub fn record_conflict(
    state_dir: &Path,
    local_snapshot: &Path,
    remote_snapshot: &Path,
    base_snapshot: Option<&Path>,
    merged_files: &BTreeMap<String, Vec<u8>>,
    conflict_paths: &[String],
    remote_commit: Option<&str>,
    revision: u64,
    reason: &str,
) -> MemoryResult<ConflictRecord> {
    ensure_real_directory(state_dir, true)?;
    let local_manifest = read_manifest(local_snapshot)?;
    let remote_manifest = read_manifest(remote_snapshot)?;
    let base_hash = base_snapshot
        .map(read_manifest)
        .transpose()?
        .map(|item| item.content_sha256);
    let mut sorted_paths = conflict_paths.to_vec();
    sorted_paths.sort();
    let mut identity = BTreeMap::new();
    identity.insert("base", base_hash.map(Value::String).unwrap_or(Value::Null));
    identity.insert("local", Value::String(local_manifest.content_sha256));
    identity.insert(
        "paths",
        Value::Array(sorted_paths.iter().cloned().map(Value::String).collect()),
    );
    identity.insert("remote", Value::String(remote_manifest.content_sha256));
    identity.insert(
        "remoteCommit",
        remote_commit
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
    );
    let conflict_id = sha256_hex(&serde_json::to_vec(&identity)?)[..16].to_string();
    let conflicts = state_dir.join("conflicts");
    ensure_real_directory(&conflicts, true)?;
    let root = conflicts.join(&conflict_id);
    let record = ConflictRecord {
        schema_version: 1,
        conflict_id: conflict_id.clone(),
        created_utc: utc_now(),
        reason: reason.to_string(),
        conflict_paths: sorted_paths,
        remote_commit: remote_commit.map(str::to_string),
    };
    if !root.exists() {
        let stage = temporary_sibling(&root, "conflict");
        fs::create_dir(&stage)?;
        copy_directory(local_snapshot, &stage.join("local"))?;
        copy_directory(remote_snapshot, &stage.join("remote"))?;
        if let Some(base) = base_snapshot {
            copy_directory(base, &stage.join("base"))?;
        }
        snapshot_from_files(&stage.join("merged"), merged_files, revision)?;
        atomic_write_json(&stage.join("conflict.json"), &record)?;
        fs::rename(&stage, &root)?;
    } else if !root.is_dir() || is_link_or_reparse(&root)? {
        return Err(MemorySyncError::new(format!(
            "native memory conflict evidence is unsafe: {}",
            root.display()
        )));
    }
    let active = json!({
        "schemaVersion": 1,
        "conflictId": record.conflict_id,
        "createdUtc": record.created_utc,
        "path": root,
        "reason": record.reason,
        "conflictPaths": record.conflict_paths,
        "remoteCommit": record.remote_commit,
    });
    atomic_write_json(&state_dir.join("active-conflict.json"), &active)?;
    Ok(record)
}

pub struct MemoryRemoteClient<'a> {
    runner: &'a dyn ProcessRunner,
}

impl<'a> MemoryRemoteClient<'a> {
    pub fn new(runner: &'a dyn ProcessRunner) -> Self {
        Self { runner }
    }

    pub fn run_git_read_only(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> MemoryResult<String> {
        let mut request = ProcessRequest::new("git", cwd);
        request.args = args.iter().map(OsString::from).collect();
        request.timeout = timeout;
        let output = self
            .runner
            .run(&request)
            .map_err(|error| MemorySyncError::new(format!("cannot launch git: {error}")))?;
        if output.timed_out {
            return Err(MemorySyncError::new("git command timed out"));
        }
        if output.code != 0 {
            return Err(MemorySyncError::new(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn run_github_read_only(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> MemoryResult<String> {
        let mut request = ProcessRequest::new("gh", cwd);
        request.args = args.iter().map(OsString::from).collect();
        request.timeout = timeout;
        let output = self
            .runner
            .run(&request)
            .map_err(|error| MemorySyncError::new(format!("cannot launch GitHub CLI: {error}")))?;
        if output.timed_out || output.code != 0 {
            return Err(MemorySyncError::new(format!(
                "GitHub verification failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn verify_private_github_repository(
        &self,
        cwd: &Path,
        repository: &str,
    ) -> MemoryResult<()> {
        let identity = github_repository_identity(repository)?;
        let effective = self.run_git_read_only(
            cwd,
            &["ls-remote", "--get-url", repository],
            Duration::from_secs(10),
        )?;
        if !github_repository_identity(&effective)?.eq_ignore_ascii_case(&identity) {
            return Err(MemorySyncError::new(
                "native memories Git URL rewrite changes the approved repository",
            ));
        }
        // A full URL pins gh to github.com even when GH_HOST selects another host.
        let github_url = format!("https://github.com/{identity}");
        let args = [
            "repo",
            "view",
            &github_url,
            "--json",
            "visibility",
            "--jq",
            ".visibility",
        ];
        let visibility = self
            .run_github_read_only(cwd, &args, Duration::from_secs(45))
            .or_else(|_| self.github_with_git_credential(cwd, &identity, &args))?;
        if visibility.trim().eq_ignore_ascii_case("private") {
            Ok(())
        } else {
            Err(MemorySyncError::new(
                "native memories repository must be private",
            ))
        }
    }

    fn github_with_git_credential(
        &self,
        cwd: &Path,
        identity: &str,
        args: &[&str],
    ) -> MemoryResult<String> {
        // Secrets travel through stdin/stdout and the child environment only. Never include
        // helper/API diagnostics in errors: helpers may echo credentials on failure.
        let failure = || {
            MemorySyncError::new("GitHub private verification failed with existing Git credentials")
        };
        let mut fill = ProcessRequest::new("git", cwd);
        fill.args = ["-c", "credential.interactive=false", "credential", "fill"]
            .map(OsString::from)
            .to_vec();
        fill.stdin =
            format!("protocol=https\nhost=github.com\npath={identity}.git\n\n").into_bytes();
        fill.timeout = Duration::from_secs(15);
        for (key, value) in [
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GCM_INTERACTIVE", "never"),
            ("GCM_GUI_PROMPT", "false"),
        ] {
            fill.env.insert(key.into(), value.into());
        }
        fill.env_remove = [
            "GIT_TRACE",
            "GIT_TRACE_CURL",
            "GIT_CURL_VERBOSE",
            "GCM_TRACE",
        ]
        .map(OsString::from)
        .to_vec();
        let credentials = self.runner.run(&fill).map_err(|_| failure())?;
        if credentials.timed_out || credentials.code != 0 {
            return Err(failure());
        }
        let text = String::from_utf8(credentials.stdout).map_err(|_| failure())?;
        let mut fields = BTreeMap::new();
        for line in text.lines().filter(|line| !line.is_empty()) {
            let (key, value) = line.split_once('=').ok_or_else(failure)?;
            if fields.insert(key, value).is_some() {
                return Err(failure());
            }
        }
        if fields.get("protocol") != Some(&"https") || fields.get("host") != Some(&"github.com") {
            return Err(failure());
        }
        let token = fields
            .get("password")
            .filter(|value| !value.is_empty() && !value.contains(['\r', '\n', '\0']))
            .ok_or_else(failure)?;
        let mut request = ProcessRequest::new("gh", cwd);
        request.args = args.iter().map(OsString::from).collect();
        request.timeout = Duration::from_secs(45);
        request.env.insert("GH_TOKEN".into(), (*token).into());
        request.env.insert("GH_PROMPT_DISABLED".into(), "1".into());
        request.env.insert("GH_HOST".into(), "github.com".into());
        request.env_remove = ["GH_DEBUG", "DEBUG", "GITHUB_TOKEN"]
            .map(OsString::from)
            .to_vec();
        let output = self.runner.run(&request).map_err(|_| failure())?;
        if output.timed_out || output.code != 0 {
            return Err(failure());
        }
        // Return only the allowlisted result; never let unexpected API output escape.
        match String::from_utf8(output.stdout)
            .map_err(|_| failure())?
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "private" => Ok("private".into()),
            "public" | "internal" => Err(MemorySyncError::new(
                "native memories repository must be private",
            )),
            _ => Err(failure()),
        }
    }
}

pub fn atomic_write(path: &Path, payload: &[u8]) -> MemoryResult<()> {
    crate::persistence::atomic_write(path, payload).map_err(MemorySyncError::from)
}

pub fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> MemoryResult<()> {
    crate::persistence::atomic_write_json(path, value).map_err(MemorySyncError::from)
}

pub fn is_link_or_reparse(path: &Path) -> MemoryResult<bool> {
    crate::persistence::is_link_or_reparse(path).map_err(MemorySyncError::from)
}

fn ensure_real_directory(path: &Path, create: bool) -> MemoryResult<PathBuf> {
    crate::persistence::ensure_real_directory(path, create).map_err(MemorySyncError::from)
}

fn scan_directory(source: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> MemoryResult<()> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| {
            MemorySyncError::new(format!(
                "cannot scan native memories: {}: {error}",
                directory.display()
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(source).unwrap();
        if excluded(relative) {
            continue;
        }
        if is_link_or_reparse(&path)? {
            return Err(MemorySyncError::new(format!(
                "native memories contain a link or junction: {}",
                path_to_posix(relative)?
            )));
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            scan_directory(source, &path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn excluded(relative: &Path) -> bool {
    if relative.components().any(|part| {
        matches!(part, Component::Normal(value) if matches!(value.to_str(), Some("__pycache__" | ".git" | ".bridgeforge" | ".bridgeforge-codex")))
    }) {
        return true;
    }
    let Some(name) = relative.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    EXCLUDED_NAMES.contains(&name)
        || name.starts_with(".~")
        || EXCLUDED_SUFFIXES
            .iter()
            .any(|suffix| name.to_lowercase().ends_with(suffix))
}

fn safe_relative(value: &str) -> MemoryResult<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(MemorySyncError::new(format!(
            "unsafe native memory path: {value}"
        )));
    }
    Ok(path.to_path_buf())
}

fn path_to_posix(path: &Path) -> MemoryResult<String> {
    path.components()
        .map(|part| match part {
            Component::Normal(value) => value
                .to_str()
                .map(str::to_string)
                .ok_or_else(|| MemorySyncError::new("native memory path is not UTF-8")),
            _ => Err(MemorySyncError::new("unsafe native memory path")),
        })
        .collect::<MemoryResult<Vec<_>>>()
        .map(|parts| parts.join("/"))
}

fn sha256_hex(payload: &[u8]) -> String {
    let digest = Sha256::digest(payload);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn github_repository_identity(remote: &str) -> MemoryResult<String> {
    let normalized = normalize_remote(remote);
    let lower = normalized.to_ascii_lowercase();
    let prefix = [
        "https://github.com/",
        "https://github.com:443/",
        "ssh://git@github.com/",
        "ssh://git@github.com:22/",
        "git@github.com:",
    ]
    .into_iter()
    .find(|prefix| lower.starts_with(prefix))
    .ok_or_else(|| {
        MemorySyncError::new("native memories remote must use HTTPS or SSH on exactly github.com")
    })?;
    let parts: Vec<&str> = normalized[prefix.len()..].split('/').collect();
    if parts.len() != 2
        || parts[0].is_empty()
        || parts[0].starts_with('-')
        || parts[0].ends_with('-')
        || !parts[0]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || !parts[1].eq_ignore_ascii_case(REPOSITORY)
    {
        return Err(MemorySyncError::new(
            "native memories remote is outside the approved repository scope",
        ));
    }
    Ok(format!("{}/{REPOSITORY}", parts[0]))
}

fn temporary_sibling(path: &Path, purpose: &str) -> PathBuf {
    crate::persistence::temporary_sibling(path, purpose)
}

fn strip_utf8_bom(payload: &[u8]) -> Vec<u8> {
    payload
        .strip_prefix(&[0xEF, 0xBB, 0xBF])
        .unwrap_or(payload)
        .to_vec()
}

fn read_manifest(snapshot: &Path) -> MemoryResult<SnapshotManifest> {
    let manifest: SnapshotManifest =
        serde_json::from_slice(&fs::read(snapshot.join("snapshot-manifest.json"))?)?;
    verify_snapshot(snapshot, &manifest)?;
    Ok(manifest)
}

fn copy_directory(source: &Path, destination: &Path) -> MemoryResult<()> {
    ensure_real_directory(source, false)?;
    fs::create_dir(destination)?;
    for entry in walkdir::WalkDir::new(source)
        .min_depth(1)
        .follow_links(false)
    {
        let entry = entry.map_err(|error| MemorySyncError::new(error.to_string()))?;
        let relative = entry.path().strip_prefix(source).unwrap();
        if entry.file_type().is_symlink() {
            return Err(MemorySyncError::new(format!(
                "refusing to copy linked path: {}",
                entry.path().display()
            )));
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn remove_directory_if_present(path: &Path) -> MemoryResult<()> {
    if path.exists() {
        if is_link_or_reparse(path)? {
            return Err(MemorySyncError::new(format!(
                "refusing to remove linked directory: {}",
                path.display()
            )));
        }
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn verify_local_unchanged(
    destination: &Path,
    expected: Option<&SnapshotManifest>,
) -> MemoryResult<()> {
    let unchanged = match expected {
        Some(expected) if destination.exists() => {
            let actual = capture_manifest(destination, expected.revision, None)?;
            same_manifest_files(&actual, expected)
        }
        None => !destination.exists(),
        _ => false,
    };
    if !unchanged {
        return Err(MemorySyncError::new(
            "local native memories changed after capture; reconcile again before replacing or publishing",
        ));
    }
    Ok(())
}

fn capture_local_only_files(source: &Path) -> MemoryResult<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    if !source.exists() {
        return Ok(files);
    }
    ensure_real_directory(source, false)?;
    for name in LOCAL_ONLY_MEMORY_FILES {
        let path = source.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() || is_link_or_reparse(&path)? {
            return Err(MemorySyncError::new(format!(
                "local-only native memory path is not a regular file: {}",
                path.display()
            )));
        }
        files.insert((*name).to_string(), fs::read(path)?);
    }
    Ok(files)
}

fn verify_local_only_unchanged(
    source: &Path,
    expected: &BTreeMap<String, Vec<u8>>,
) -> MemoryResult<()> {
    if capture_local_only_files(source)? != *expected {
        return Err(MemorySyncError::new(
            "local-only native memory files changed during replacement; reconcile again",
        ));
    }
    Ok(())
}

fn replace_memories_if_unchanged(
    stage: &Path,
    destination: &Path,
    expected: Option<&SnapshotManifest>,
) -> MemoryResult<()> {
    verify_local_unchanged(destination, expected)?;
    let local_only = capture_local_only_files(destination)?;
    for (name, payload) in &local_only {
        fs::write(stage.join(name), payload)?;
    }
    verify_local_only_unchanged(destination, &local_only)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| MemorySyncError::new(error.to_string()))?
        .as_nanos();
    let old =
        temporary_sibling(destination, "before-sync").with_extension(format!("{timestamp}.tmp"));
    if expected.is_some() {
        fs::rename(destination, &old)?;
        // The native writer does not share our lock. Recheck the tree we actually
        // moved, not merely the path checked before rename.
        let moved_unchanged = verify_local_unchanged(&old, expected)
            .and_then(|()| verify_local_only_unchanged(&old, &local_only));
        if let Err(error) = moved_unchanged {
            if !destination.exists() {
                if let Err(restore) = fs::rename(&old, destination) {
                    return Err(MemorySyncError::new(format!(
                        "{error}; rollback failed: {restore}; original tree retained at {}",
                        old.display()
                    )));
                }
            }
            return Err(MemorySyncError::new(format!(
                "{error}; changed tree retained at {}",
                if old.exists() { &old } else { destination }.display()
            )));
        }
    }
    // Never overwrite a directory another writer recreated while ours was moved.
    let install = if destination.exists() {
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "local memories reappeared during replacement",
        ))
    } else {
        fs::rename(stage, destination)
    };
    if let Err(error) = install {
        if old.exists() && !destination.exists() {
            if let Err(restore) = fs::rename(&old, destination) {
                return Err(MemorySyncError::new(format!(
                    "{error}; rollback failed: {restore}; original tree retained at {}",
                    old.display()
                )));
            }
        }
        return Err(MemorySyncError::new(format!(
            "{error}; original tree retained at {}",
            if old.exists() { &old } else { destination }.display()
        )));
    }
    // Retain this unique sibling: a native writer may still hold an open handle
    // and finish writing into the old tree after replacement. No automatic cleanup.
    if old.exists() {
        verify_local_unchanged(&old, expected).map_err(|error| {
            MemorySyncError::new(format!(
                "{error}; original tree retained at {}",
                old.display()
            ))
        })?;
        verify_local_only_unchanged(&old, &local_only).map_err(|error| {
            MemorySyncError::new(format!(
                "{error}; original tree retained at {}",
                old.display()
            ))
        })?;
    }
    Ok(())
}

fn replace_directory(stage: &Path, destination: &Path) -> MemoryResult<()> {
    if destination.exists() && is_link_or_reparse(destination)? {
        return Err(MemorySyncError::new(format!(
            "snapshot destination is a link: {}",
            destination.display()
        )));
    }
    let old = temporary_sibling(destination, "old");
    if destination.exists() {
        fs::rename(destination, &old)?;
    }
    if let Err(error) = fs::rename(stage, destination) {
        if old.exists() {
            let _ = fs::rename(&old, destination);
        }
        return Err(error.into());
    }
    remove_directory_if_present(&old)?;
    Ok(())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/memory/persistence_contract.rs"]
mod persistence_contract_tests;
