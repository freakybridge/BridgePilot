//! Shared reliable file writes and the path checks they require.

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct PersistenceError {
    message: String,
}

impl PersistenceError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PersistenceError {}

impl From<std::io::Error> for PersistenceError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

pub type PersistenceResult<T> = Result<T, PersistenceError>;

pub fn atomic_write(path: &Path, payload: &[u8]) -> PersistenceResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| PersistenceError::new(format!("path has no parent: {}", path.display())))?;
    ensure_real_directory(parent, true)?;
    if path.exists() && is_link_or_reparse(path)? {
        return Err(PersistenceError::new(format!(
            "refusing to replace linked file: {}",
            path.display()
        )));
    }
    let temp = temporary_sibling(path, "write");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    let result = (|| {
        file.write_all(payload)?;
        file.sync_all()?;
        drop(file);
        atomic_replace_file(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> PersistenceResult<()> {
    let canonical = sort_json_value(&serde_json::to_value(value)?);
    let mut payload = serde_json::to_vec_pretty(&canonical)?;
    payload.push(b'\n');
    atomic_write(path, &payload)
}

pub fn is_link_or_reparse(path: &Path) -> PersistenceResult<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        Ok(metadata.file_attributes() & 0x400 != 0)
    }
    #[cfg(not(windows))]
    Ok(false)
}

pub fn ensure_real_directory(path: &Path, create: bool) -> PersistenceResult<PathBuf> {
    if create {
        fs::create_dir_all(path)?;
    }
    if !path.is_dir() || is_link_or_reparse(path)? {
        return Err(PersistenceError::new(format!(
            "directory must exist and must not be a link: {}",
            path.display()
        )));
    }
    let canonical = fs::canonicalize(path)?;
    for ancestor in path.ancestors() {
        if ancestor.exists() && is_link_or_reparse(ancestor)? {
            return Err(PersistenceError::new(format!(
                "path traverses a link: {}",
                ancestor.display()
            )));
        }
    }
    Ok(canonical)
}

fn sort_json_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted: BTreeMap<&String, &Value> = object.iter().collect();
            let mut result = serde_json::Map::new();
            for (key, value) in sorted {
                result.insert(key.clone(), sort_json_value(value));
            }
            Value::Object(result)
        }
        Value::Array(values) => Value::Array(values.iter().map(sort_json_value).collect()),
        _ => value.clone(),
    }
}

pub fn temporary_sibling(path: &Path, purpose: &str) -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("memory-sync");
    path.with_file_name(format!(
        ".{name}.{purpose}.{}.{}.tmp",
        std::process::id(),
        counter
    ))
}

#[cfg(not(windows))]
pub fn atomic_replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
pub fn atomic_replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    // Rust filesystem APIs support extended Windows paths, but raw Win32 calls
    // need the same absolute verbatim spelling. The destination may not exist yet.
    let source = fs::canonicalize(source)?;
    let parent = destination.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination has no parent",
        )
    })?;
    let name = destination.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination has no filename",
        )
    })?;
    let destination = fs::canonicalize(parent)?.join(name);
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
