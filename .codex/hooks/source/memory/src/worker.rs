use super::{MemoryResult, MemorySyncError, atomic_write_json, is_link_or_reparse, utc_now};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub const WORKER_START_GRACE: Duration = Duration::from_secs(30);
static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingState {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    #[serde(rename = "firstPendingUtc")]
    pub first_pending_utc: String,
    #[serde(rename = "updatedUtc")]
    pub updated_utc: String,
    pub trigger: String,
    pub triggers: Vec<String>,
}

impl PendingState {
    pub fn is_valid(&self) -> bool {
        self.schema_version == 2
            && DateTime::parse_from_rfc3339(&self.first_pending_utc).is_ok()
            && DateTime::parse_from_rfc3339(&self.updated_utc).is_ok()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerState {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub token: String,
    pub pid: u32,
    #[serde(rename = "launcherPid")]
    pub launcher_pid: u32,
    #[serde(rename = "startedUtc")]
    pub started_utc: String,
    #[serde(
        rename = "workerStartedUtc",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub worker_started_utc: Option<String>,
    #[serde(
        rename = "processIdentity",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub process_identity: Option<u64>,
}

impl WorkerState {
    pub fn is_valid(&self) -> bool {
        self.schema_version == 1
            && !self.token.is_empty()
            && DateTime::parse_from_rfc3339(&self.started_utc).is_ok()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerReservation {
    Acquired(WorkerState),
    Reused(WorkerState),
}

pub struct ReconcileLock {
    _file: fs::File,
}

impl ReconcileLock {
    pub fn try_acquire(state_dir: &Path) -> MemoryResult<Option<Self>> {
        try_acquire_reconcile_lock(state_dir)
    }
}

pub fn mark_pending(state_dir: &Path, trigger: &str) -> MemoryResult<PendingState> {
    fs::create_dir_all(state_dir)?;
    if is_link_or_reparse(state_dir)? {
        return Err(MemorySyncError::new(format!(
            "state directory is unsafe: {}",
            state_dir.display()
        )));
    }
    let _queue = queue_lock(state_dir)?;
    let path = state_dir.join("pending.json");
    let current = read_pending(state_dir)?;
    if current.is_none() && path.exists() {
        if !path.is_file() || is_link_or_reparse(&path)? {
            return Err(MemorySyncError::new("pending state is not a plain file"));
        }
        fs::rename(
            &path,
            state_dir.join(format!("pending.invalid-{}.json", unique_token())),
        )?;
    }
    let now = utc_now();
    let mut triggers = current
        .as_ref()
        .map(|value| value.triggers.clone())
        .unwrap_or_default();
    if !triggers.iter().any(|value| value == trigger) {
        triggers.push(trigger.to_string());
    }
    if triggers.len() > 16 {
        triggers.drain(0..triggers.len() - 16);
    }
    let pending = PendingState {
        schema_version: 2,
        first_pending_utc: current
            .map(|value| value.first_pending_utc)
            .unwrap_or_else(|| now.clone()),
        updated_utc: now,
        trigger: trigger.to_string(),
        triggers,
    };
    atomic_write_json(&path, &pending)?;
    Ok(pending)
}

pub fn read_pending(state_dir: &Path) -> MemoryResult<Option<PendingState>> {
    let path = state_dir.join("pending.json");
    if !path.is_file() || is_link_or_reparse(&path)? {
        return Ok(None);
    }
    let value = serde_json::from_slice::<PendingState>(&fs::read(path)?).ok();
    Ok(value.filter(PendingState::is_valid))
}

pub fn merge_migrated_pending(
    state_dir: &Path,
    legacy: &PendingState,
) -> MemoryResult<PendingState> {
    if legacy.schema_version != 2
        || DateTime::parse_from_rfc3339(&legacy.first_pending_utc).is_err()
        || DateTime::parse_from_rfc3339(&legacy.updated_utc).is_err()
    {
        return Err(MemorySyncError::new(
            "legacy native memory pending state is invalid",
        ));
    }
    fs::create_dir_all(state_dir)?;
    if is_link_or_reparse(state_dir)? {
        return Err(MemorySyncError::new(format!(
            "state directory is unsafe: {}",
            state_dir.display()
        )));
    }
    let _queue = queue_lock(state_dir)?;
    let path = state_dir.join("pending.json");
    let current = if path.exists() {
        read_pending(state_dir)?
            .ok_or_else(|| MemorySyncError::new("current native memory pending state is invalid"))?
            .into()
    } else {
        None
    };
    let mut triggers = current
        .as_ref()
        .map(|value: &PendingState| value.triggers.clone())
        .unwrap_or_default();
    for trigger in &legacy.triggers {
        if !triggers.contains(trigger) {
            triggers.push(trigger.clone());
        }
    }
    if !triggers.iter().any(|value| value == "state-migration") {
        triggers.push("state-migration".into());
    }
    if triggers.len() > 16 {
        triggers.drain(0..triggers.len() - 16);
    }
    let first_pending_utc = current
        .as_ref()
        .map(|value| value.first_pending_utc.as_str())
        .into_iter()
        .chain(std::iter::once(legacy.first_pending_utc.as_str()))
        .min_by_key(|value| DateTime::parse_from_rfc3339(value).ok())
        .unwrap_or(&legacy.first_pending_utc)
        .to_string();
    let pending = PendingState {
        schema_version: 2,
        first_pending_utc,
        updated_utc: utc_now(),
        trigger: "state-migration".into(),
        triggers,
    };
    atomic_write_json(&path, &pending)?;
    Ok(pending)
}

pub fn pending_age(state_dir: &Path) -> MemoryResult<Duration> {
    let Some(pending) = read_pending(state_dir)? else {
        return Ok(Duration::ZERO);
    };
    let Ok(started) = DateTime::parse_from_rfc3339(&pending.first_pending_utc) else {
        return Ok(Duration::ZERO);
    };
    let elapsed = Utc::now().signed_duration_since(started.with_timezone(&Utc));
    Ok(elapsed.to_std().unwrap_or(Duration::ZERO))
}

pub fn clear_pending_if_unchanged(
    state_dir: &Path,
    expected_payload: Option<&[u8]>,
) -> MemoryResult<bool> {
    let Some(expected) = expected_payload else {
        return Ok(false);
    };
    let _queue = queue_lock(state_dir)?;
    let path = state_dir.join("pending.json");
    if !path.is_file() {
        return Ok(false);
    }
    if fs::read(&path)? != expected {
        return Ok(false);
    }
    fs::remove_file(path)?;
    Ok(true)
}

fn queue_lock(state_dir: &Path) -> MemoryResult<ReconcileLock> {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(lock) = try_acquire_reconcile_lock(&state_dir.join("queue"))? {
            return Ok(lock);
        }
        if std::time::Instant::now() >= deadline {
            return Err(MemorySyncError::new("pending queue is busy"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn drain_pending<F>(state: &Path, token: &str, mut reconcile: F) -> MemoryResult<(String, bool)>
where
    F: FnMut() -> MemoryResult<String>,
{
    let result = (|| {
        let mut last = "noop".to_string();
        for _ in 0..16 {
            last = reconcile()?;
            if matches!(last.as_str(), "busy" | "conflicted") || read_pending(state)?.is_none() {
                break;
            }
        }
        Ok::<_, MemorySyncError>(last)
    })();
    release_worker(state, token)?;
    let action = result?;
    // A trigger can arrive after the last drain check but before releasing the reservation.
    // Recheck after release so it is either handled here or by the triggering launcher.
    let restart =
        !matches!(action.as_str(), "busy" | "conflicted") && read_pending(state)?.is_some();
    Ok((action, restart))
}

pub fn reserve_worker(state_dir: &Path) -> MemoryResult<WorkerReservation> {
    fs::create_dir_all(state_dir)?;
    let _queue = queue_lock(state_dir)?;
    let path = state_dir.join("worker.json");
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if !metadata.is_file() || is_link_or_reparse(&path)? {
            return Err(MemorySyncError::new(
                "worker reservation is not a plain file",
            ));
        }
        match read_worker_state(state_dir)? {
            Some(current) if worker_is_live(&current) => {
                return Ok(WorkerReservation::Reused(current));
            }
            Some(_) => fs::remove_file(&path)?,
            None => {
                // Preserve interrupted writes for diagnosis. The queue lock makes
                // recovery and replacement a single-launcher operation.
                fs::rename(
                    &path,
                    state_dir.join(format!("worker.invalid-{}.json", unique_token())),
                )?;
            }
        }
    }
    let state = WorkerState {
        schema_version: 1,
        token: unique_token(),
        pid: 0,
        launcher_pid: std::process::id(),
        started_utc: utc_now(),
        worker_started_utc: None,
        process_identity: None,
    };
    atomic_write_json(&path, &state)?;
    Ok(WorkerReservation::Acquired(state))
}

pub fn mark_worker_started(state_dir: &Path, token: &str, pid: u32) -> MemoryResult<bool> {
    let _queue = queue_lock(state_dir)?;
    let Some(mut current) = read_worker_state(state_dir)? else {
        return Ok(false);
    };
    if current.token != token {
        return Ok(false);
    }
    current.pid = pid;
    current.worker_started_utc = Some(utc_now());
    current.process_identity = process_identity(pid);
    atomic_write_json(&state_dir.join("worker.json"), &current)?;
    Ok(true)
}

pub fn release_worker(state_dir: &Path, token: &str) -> MemoryResult<bool> {
    let _queue = queue_lock(state_dir)?;
    let path = state_dir.join("worker.json");
    let Some(current) = read_worker_state(state_dir)? else {
        return Ok(false);
    };
    if current.token != token {
        return Ok(false);
    }
    fs::remove_file(path)?;
    Ok(true)
}

pub fn read_worker_state(state_dir: &Path) -> MemoryResult<Option<WorkerState>> {
    let path = state_dir.join("worker.json");
    if !path.is_file() || is_link_or_reparse(&path)? {
        return Ok(None);
    }
    let value = serde_json::from_slice::<WorkerState>(&fs::read(path)?).ok();
    Ok(value.filter(WorkerState::is_valid))
}

pub fn worker_is_live(value: &WorkerState) -> bool {
    if value.pid > 0 && process_alive(value.pid) {
        #[cfg(windows)]
        if let Some(actual) = process_identity(value.pid) {
            if let Some(expected) = value.process_identity {
                return actual == expected;
            }
            // Legacy reservations lack a birth-time receipt. A process created
            // outside the bounded launch interval cannot be their worker.
            if let Ok(started) = DateTime::parse_from_rfc3339(&value.started_utc) {
                let birth_millis = (actual / 10_000) as i64 - 11_644_473_600_000;
                let delta = birth_millis - started.timestamp_millis();
                return (-1000..=WORKER_START_GRACE.as_millis() as i64).contains(&delta);
            }
        }
        return true; // Query denial is not evidence that a live worker is stale.
    }
    if value.pid != 0 {
        return false;
    }
    DateTime::parse_from_rfc3339(&value.started_utc)
        .ok()
        .and_then(|started| {
            Utc::now()
                .signed_duration_since(started.with_timezone(&Utc))
                .to_std()
                .ok()
        })
        .is_some_and(|elapsed| elapsed < WORKER_START_GRACE)
}

#[cfg(windows)]
fn process_identity(pid: u32) -> Option<u64> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let _owned = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut created = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    if unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) } == 0 {
        return None;
    }
    Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

#[cfg(not(windows))]
fn process_identity(_pid: u32) -> Option<u64> {
    None
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_memory_worker.rs"]
mod tests;

pub fn try_acquire_reconcile_lock(state_dir: &Path) -> MemoryResult<Option<ReconcileLock>> {
    Ok(try_lock_file(&state_dir.join("reconcile.lock"))?.map(|file| ReconcileLock { _file: file }))
}

// Keep the file in place: unlinking a lock lets competing processes lock different
// file objects. The OS releases ownership when this handle closes or its process dies.
pub fn try_lock_file(path: &Path) -> MemoryResult<Option<fs::File>> {
    for ancestor in path.ancestors().filter(|p| p.exists()) {
        if is_link_or_reparse(ancestor)? {
            return Err(MemorySyncError::new(
                "lock path traverses a linked directory or file",
            ));
        }
    }
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| MemorySyncError::new("lock has no parent"))?,
    )?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        #[cfg(windows)]
        Err(error) if error.raw_os_error() == Some(32) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(error.into());
        }
    }
    #[cfg(not(any(windows, unix)))]
    return Err(MemorySyncError::new(
        "OS file locking is unsupported on this platform",
    ));
    #[cfg(any(windows, unix))]
    Ok(Some(file))
}

fn unique_token() -> String {
    let nonce = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    let seed = format!("{}:{}:{}", std::process::id(), utc_now(), nonce);
    let digest = Sha256::digest(seed.as_bytes());
    digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

#[cfg(windows)]
pub fn process_alive(pid: u32) -> bool {
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn GetExitCodeProcess(process: *mut std::ffi::c_void, code: *mut u32) -> i32;
        fn CloseHandle(object: *mut std::ffi::c_void) -> i32;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let mut code = 0;
    let result = unsafe { GetExitCodeProcess(handle, &mut code) };
    unsafe { CloseHandle(handle) };
    result == 0 || code == STILL_ACTIVE
}

#[cfg(unix)]
pub fn process_alive(pid: u32) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let result = unsafe { kill(pid as i32, 0) };
    if result == 0 {
        return true;
    }
    matches!(std::io::Error::last_os_error().raw_os_error(), Some(1))
}

#[cfg(not(any(unix, windows)))]
pub fn process_alive(pid: u32) -> bool {
    pid == std::process::id()
}
