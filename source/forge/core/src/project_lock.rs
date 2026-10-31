use crate::managed_paths::safe_join;
use std::path::Path;
pub struct ProjectLock {
    _guard: crate::file_lock::FileLock,
}

impl ProjectLock {
    pub fn acquire(root: &Path) -> Result<Self, String> {
        let path = safe_join(
            root,
            ".runtime/bridgeforge-codex/project-sync.lock",
            "project lock",
        )?;
        let guard = crate::file_lock::FileLock::acquire(&path).map_err(|error| {
            format!("project-sync lock unavailable; another transaction may be active: {error}")
        })?;
        Ok(Self { _guard: guard })
    }
}
