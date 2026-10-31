//! Exact project-local cleanup. Planning is read-only; execution returns partial
//! progress and never writes a receipt back into the removed artifact trees.
use crate::artifact_paths::{BUILD, CONFIG, DEPS, DOWNLOAD, RUNTIME};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    path: PathBuf,
    directory: bool,
    bytes: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
}

fn inspect(path: &Path) -> Result<Entry, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if crate::persistence::is_link_or_reparse(path).map_err(|e| e.to_string())?
        || (!metadata.is_file() && !metadata.is_dir())
    {
        return Err(format!(
            "linked or unsupported cleanup entry: {}",
            path.display()
        ));
    }
    Ok(Entry {
        path: path.into(),
        directory: metadata.is_dir(),
        bytes: if metadata.is_file() {
            metadata.len()
        } else {
            0
        },
        modified: metadata.modified().ok(),
        created: metadata.created().ok(),
    })
}

/// The deletion set contains literal filesystem paths, not manifest patterns.
/// Keep native path components intact (including brackets and Unicode names).
fn validate_cleanup_target(root: &Path, path: &Path) -> Result<(), String> {
    use std::path::Component;
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("cleanup target escapes project root: {}", path.display()))?;
    if relative.components().count() < 2
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || !(relative.starts_with(RUNTIME) || relative.starts_with(BUILD))
        || [CONFIG, DOWNLOAD]
            .iter()
            .any(|protected| crate::artifact_paths::within(&relative.to_string_lossy(), protected))
        || (cfg!(windows) && relative.to_string_lossy().contains(':'))
    {
        return Err(format!(
            "cleanup target is outside the deletion boundary: {}",
            path.display()
        ));
    }
    // Recheck every ancestor, including the project root, without resolving or
    // following links. Existing deletion handles still bind the actual objects.
    for ancestor in path.ancestors() {
        inspect(ancestor)?;
    }
    Ok(())
}

pub struct Plan {
    root: PathBuf,
    entries: Vec<Entry>,
    roots: Vec<Entry>,
    targets: Vec<String>,
    build: BuildPolicy,
    skipped: Vec<String>,
}

/// Exact current products from the owning producer or project declaration.
/// Every other build path is selected; `keep` always takes precedence.
#[derive(Clone, Debug, Default)]
pub struct BuildPolicy {
    // Retained for producer selection evidence compatibility. Deletion no longer
    // requires an opt-in remove role.
    pub remove: Vec<String>,
    pub keep: Vec<String>,
    pub directories: BTreeMap<String, bool>,
    pub selections: Vec<Selection>,
    pub witnesses: BTreeMap<String, Option<Vec<u8>>>,
}

impl BuildPolicy {
    fn validate(&self, root: &Path) -> Result<(), String> {
        for relative in self.remove.iter().chain(&self.keep) {
            if !crate::artifact_paths::within(relative, BUILD)
                || crate::artifact_paths::normalized(relative).trim_end_matches('/') == BUILD
            {
                return Err("cleanup build role must name an exact path below .build".into());
            }
            crate::managed_paths::safe_join(root, relative, "cleanup build role")?;
        }
        for (relative, expected) in &self.witnesses {
            let path = crate::managed_paths::safe_join(root, relative, "cleanup policy")?;
            if crate::managed_paths::read_metadata(&path, 16 * 1024 * 1024)? != *expected {
                return Err(format!("cleanup policy changed: {relative}"));
            }
        }
        for (relative, directory) in &self.directories {
            let path = crate::managed_paths::safe_join(root, relative, "cleanup output type")?;
            match fs::symlink_metadata(&path) {
                Ok(metadata)
                    if metadata.is_dir() == *directory
                        && (metadata.is_file() || metadata.is_dir()) =>
                {
                    ()
                }
                Ok(_) => return Err(format!("cleanup output type changed: {relative}")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(format!("{}: {error}", path.display())),
            }
        }
        for selection in &self.selections {
            selection.verify(root)?;
        }
        Ok(())
    }
}

/// Ownership evidence captured by the producer at selection time, not recaptured
/// from arbitrary contents later. Empty member names identify the selected root.
#[derive(Clone, Debug)]
pub struct Selection {
    pub path: PathBuf,
    pub members: BTreeMap<String, Option<String>>,
}

impl Selection {
    fn verify(&self, root: &Path) -> Result<(), String> {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let relative = self.path.strip_prefix(root).map_err(|e| e.to_string())?;
        let path = crate::managed_paths::safe_join(
            root,
            &relative.to_string_lossy().replace('\\', "/"),
            "cleanup selection",
        )?;
        let mut actual = BTreeMap::new();
        for item in walkdir::WalkDir::new(&path).follow_links(false) {
            let item = item.map_err(|e| e.to_string())?;
            let entry = inspect(item.path())?;
            let name = item
                .path()
                .strip_prefix(&path)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let digest = if entry.directory {
                None
            } else {
                let mut file = fs::File::open(item.path()).map_err(|e| e.to_string())?;
                let mut digest = Sha256::new();
                let mut buffer = [0; 65536];
                loop {
                    let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
                    if count == 0 {
                        break;
                    }
                    digest.update(&buffer[..count]);
                }
                Some(format!("sha256:{:x}", digest.finalize()))
            };
            actual.insert(name, digest);
        }
        if actual != self.members {
            return Err(format!(
                "cleanup ownership evidence changed: {}",
                path.display()
            ));
        }
        Ok(())
    }
}

fn collect_build(
    root: &Path,
    path: &Path,
    policy: &BuildPolicy,
    entries: &mut Vec<Entry>,
    targets: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> Result<(), String> {
    let entry = inspect(path)?;
    let relative = path
        .strip_prefix(root)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    if policy
        .keep
        .iter()
        .any(|keep| crate::artifact_paths::within(&relative, keep))
    {
        return Ok(());
    }
    let protects_descendant = policy
        .keep
        .iter()
        .any(|keep| crate::artifact_paths::within(keep, &relative));
    if !protects_descendant {
        targets.push(relative.clone());
        entries.push(entry.clone());
    }
    if entry.directory {
        for child in fs::read_dir(path).map_err(|e| e.to_string())? {
            collect_build(
                root,
                &child.map_err(|e| e.to_string())?.path(),
                policy,
                entries,
                targets,
                skipped,
            )?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct Report {
    pub schema: u32,
    pub status: String,
    pub read_only: bool,
    pub project_root: String,
    pub targets: Vec<String>,
    pub preserved: Vec<String>,
    pub planned_files: usize,
    pub planned_directories: usize,
    pub deleted_files: usize,
    pub deleted_directories: usize,
    pub logical_bytes_removed: u64,
    pub remaining_entries: Option<usize>,
    pub failures: Vec<String>,
    pub skipped_build_paths: Vec<String>,
}

impl Plan {
    pub fn capture(root: &Path) -> Result<Self, String> {
        Self::capture_with_build(root, BuildPolicy::default())
    }

    pub fn capture_with_build(root: &Path, build: BuildPolicy) -> Result<Self, String> {
        // Validate the supplied path before canonicalization can hide a junction.
        crate::managed_paths::safe_join(root, RUNTIME, "cleanup root")?;
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let mut build = build;
        if let Ok(image) = std::env::current_exe().and_then(|p| p.canonicalize()) {
            if let Ok(relative) = image.strip_prefix(&root) {
                let relative = relative.to_string_lossy().replace('\\', "/");
                if crate::artifact_paths::within(&relative, BUILD) {
                    build.keep.push(relative);
                }
            }
        }
        build.keep.sort();
        build.keep.dedup();
        if root.parent().is_none() {
            return Err("cleanup requires a project directory, not a volume root".into());
        }
        let mut entries = Vec::new();
        let mut roots = Vec::new();
        let mut targets = Vec::new();
        let mut skipped = Vec::new();
        build.validate(&root)?;
        // A missing declared product does not justify retaining empty parents.
        // Recapture still detects newly appearing entries before deletion.
        build.keep.retain(|relative| root.join(relative).exists());
        for relative in [RUNTIME, BUILD, DEPS] {
            let path = crate::managed_paths::safe_join(&root, relative, "cleanup boundary")?;
            match fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.to_string()),
                Ok(_) => (),
            }
            let entry = inspect(&path)?;
            if !entry.directory {
                return Err(format!(
                    "artifact root is not a directory: {}",
                    path.display()
                ));
            }
            if relative == DEPS {
                continue;
            }
            roots.push(entry);
            if relative == BUILD {
                for child in fs::read_dir(&path).map_err(|e| e.to_string())? {
                    collect_build(
                        &root,
                        &child.map_err(|e| e.to_string())?.path(),
                        &build,
                        &mut entries,
                        &mut targets,
                        &mut skipped,
                    )?;
                }
                continue;
            }
            for child in fs::read_dir(path).map_err(|e| e.to_string())? {
                let child = child.map_err(|e| e.to_string())?;
                let path = child.path();
                let relative = path
                    .strip_prefix(&root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let entry = inspect(&path)?;
                if [CONFIG, DOWNLOAD]
                    .iter()
                    .any(|p| crate::artifact_paths::normalized(&relative) == *p)
                {
                    if !entry.directory {
                        return Err(format!(
                            "protected runtime path is not a directory: {relative}"
                        ));
                    }
                    continue;
                }
                targets.push(relative);
                if entry.directory {
                    for item in walkdir::WalkDir::new(&path).follow_links(false) {
                        entries.push(inspect(item.map_err(|e| e.to_string())?.path())?);
                    }
                } else {
                    entries.push(entry);
                }
            }
        }
        #[cfg(windows)]
        {
            let own = windows::path_identity(&std::env::current_exe().map_err(|e| e.to_string())?)?;
            for entry in entries
                .iter()
                .filter(|e| !e.directory && e.path.starts_with(root.join(BUILD)))
            {
                if windows::path_identity(&entry.path)? == own {
                    build.keep.push(
                        entry
                            .path
                            .strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
            build.keep.sort();
            build.keep.dedup();
            let protected = |relative: &str| {
                build.keep.iter().any(|keep| {
                    crate::artifact_paths::within(relative, keep)
                        || crate::artifact_paths::within(keep, relative)
                })
            };
            entries.retain(|entry| {
                !protected(
                    &entry
                        .path
                        .strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                )
            });
            targets.retain(|target| !protected(target));
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        targets.sort();
        // Report only maximal selected roots, not every file in their subtrees.
        let mut covered = std::collections::BTreeSet::new();
        targets.retain(|path| {
            let key = crate::artifact_paths::normalized(path);
            if Path::new(&key)
                .ancestors()
                .skip(1)
                .any(|parent| covered.contains(&parent.to_string_lossy().into_owned()))
            {
                return false;
            }
            covered.insert(key);
            true
        });
        skipped.sort();
        Ok(Self {
            root,
            entries,
            roots,
            targets,
            build,
            skipped,
        })
    }

    pub fn preview(&self) -> Report {
        Report {
            schema: 1,
            status: "preview".into(),
            read_only: true,
            project_root: self.root.to_string_lossy().into_owned(),
            targets: self.targets.clone(),
            preserved: [
                vec![
                    format!("{CONFIG}/"),
                    format!("{DOWNLOAD}/"),
                    format!("{DEPS}/"),
                ],
                self.build.keep.clone(),
            ]
            .concat(),
            planned_files: self.entries.iter().filter(|e| !e.directory).count(),
            planned_directories: self.entries.iter().filter(|e| e.directory).count(),
            deleted_files: 0,
            deleted_directories: 0,
            logical_bytes_removed: 0,
            remaining_entries: Some(self.entries.len()),
            failures: Vec::new(),
            skipped_build_paths: self.skipped.clone(),
        }
    }

    fn verify(&self) -> Result<(), String> {
        let current = Self::capture_with_build(&self.root, self.build.clone())?;
        if current.entries != self.entries
            || current.roots != self.roots
            || current.targets != self.targets
        {
            return Err("cleanup input paths or contents changed during preflight".into());
        }
        Ok(())
    }

    pub fn execute(self) -> Report {
        self.execute_observed(&mut || Ok(()))
    }

    fn execute_observed(self, after_delete: &mut dyn FnMut() -> Result<(), String>) -> Report {
        let mut report = self.preview();
        report.read_only = false;
        let result = self.execute_inner(&mut report, after_delete);
        if let Err(error) = result {
            report.failures.push(error);
        }
        match self.remaining() {
            Ok(remaining) => {
                report.remaining_entries = Some(remaining.entries.len());
                if !remaining.entries.is_empty() && report.failures.is_empty() {
                    report.failures.push(
                        "artifact contents remain after cleanup; preserved for next invocation"
                            .into(),
                    );
                }
            }
            Err(error) => {
                report.remaining_entries = None;
                report
                    .failures
                    .push(format!("cannot verify final cleanup state: {error}"));
            }
        }
        report.status = if report.failures.is_empty() {
            "completed"
        } else if report.deleted_files + report.deleted_directories == 0 {
            "blocked"
        } else {
            "partial"
        }
        .into();
        report
    }

    fn remaining(&self) -> Result<Self, String> {
        let mut policy = self.build.clone();
        // Deleted members no longer satisfy the pre-deletion proof. Remaining
        // paths are counted only, never authorized for an additional deletion.
        policy.selections.clear();
        Self::capture_with_build(&self.root, policy)
    }

    #[cfg(not(windows))]
    fn execute_inner(
        &self,
        _: &mut Report,
        _: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        Err("cleanup execution currently requires the verified Windows handle-based backend".into())
    }

    #[cfg(windows)]
    fn execute_inner(
        &self,
        report: &mut Report,
        after_delete: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        use std::collections::BTreeMap;
        self.verify()?;
        if self.entries.is_empty() {
            return Ok(());
        }
        let own_image = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if self.entries.iter().any(|entry| entry.path == own_image) {
            return Err("cleanup executor conflicts with a cache/history path; use the current project tool and correct its build output roles".into());
        }
        for (pid, image) in crate::process_activity::images()? {
            if pid == std::process::id() {
                continue;
            }
            let image = image.canonicalize().unwrap_or(image);
            if self.entries.iter().any(|e| !e.directory && e.path == image) {
                return Err(format!(
                    "active process {pid} uses cleanup target {}",
                    image.display()
                ));
            }
        }
        // Occupancy is checked against the deletion set, not a process's cwd,
        // command line or protected dependency image. Hold all target handles
        // before deleting anything; incompatible writers/readers and locks fail
        // here even when the owning process is installed outside the project.
        let _root_guards = self
            .roots
            .iter()
            .map(|e| windows::Guard::open(e))
            .collect::<Result<Vec<_>, _>>()?;
        // Cache coordination files are deletion targets too. Acquire and lock
        // them once with the complete target set, before the first deletion.
        let mut files = self
            .entries
            .iter()
            .filter(|e| !e.directory)
            .collect::<Vec<_>>();
        files.sort_by_key(|e| {
            (
                e.path == own_image,
                e.path
                    .file_name()
                    .is_some_and(|s| s.to_string_lossy().ends_with(".lock")),
                e.path.file_name().is_some_and(|s| s == "entry.json"),
            )
        });
        let mut identities = BTreeMap::new();
        let mut counts = BTreeMap::new();
        for entry in &files {
            let id = windows::path_identity(&entry.path)?;
            *counts.entry(id).or_insert(0usize) += 1;
            identities.insert(entry.path.clone(), id);
        }
        let mut guards = BTreeMap::new();
        let mut anchors = BTreeMap::new();
        let mut opened = std::collections::BTreeSet::new();
        for entry in self.entries.iter().filter(|entry| entry.directory) {
            guards.insert(entry.path.clone(), windows::Guard::open(entry)?);
        }
        for entry in &files {
            let id = identities[&entry.path];
            if !opened.insert(id) {
                continue;
            }
            if counts[&id] > 1 {
                // Hold a read-only anchor across successive hard-link removals.
                // It denies writes even between closing one DELETE handle and
                // opening the next; each name is rebound to the original file ID.
                anchors.insert(id, windows::read_anchor(&entry.path, id)?);
            }
            let guard = windows::Guard::open(entry)?;
            guard.require_identity(id)?;
            guards.insert(entry.path.clone(), guard);
        }
        self.verify()?;
        for entry in &files {
            if let Some(guard) = guards.get(&entry.path) {
                guard.lock()?;
            }
        }
        for entry in files {
            validate_cleanup_target(&self.root, &entry.path)?;
            if inspect(&entry.path)? != *entry {
                return Err(format!("cleanup file drifted: {}", entry.path.display()));
            }
            let guard = match guards.remove(&entry.path) {
                Some(guard) => guard,
                None => {
                    let guard = windows::Guard::open(entry)?;
                    guard.require_identity(identities[&entry.path])?;
                    guard.lock()?;
                    guard
                }
            };
            guard.remove()?;
            report.deleted_files += 1;
            report.logical_bytes_removed += entry.bytes;
            after_delete()?;
            require_absent(&entry.path)?;
        }
        let mut directories = self
            .entries
            .iter()
            .filter(|e| e.directory)
            .collect::<Vec<_>>();
        directories.sort_by_key(|e| std::cmp::Reverse(e.path.components().count()));
        for entry in directories {
            validate_cleanup_target(&self.root, &entry.path)?;
            if fs::read_dir(&entry.path)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
            {
                return Err(format!(
                    "new contents appeared during cleanup; preserved: {}",
                    entry.path.display()
                ));
            }
            guards
                .remove(&entry.path)
                .ok_or("cleanup directory handle missing")?
                .remove()?;
            report.deleted_directories += 1;
            after_delete()?;
            require_absent(&entry.path)?;
        }
        let remaining = self.remaining()?;
        if !remaining.entries.is_empty() {
            return Err(
                "new artifact contents appeared during cleanup; preserved for next invocation"
                    .into(),
            );
        }
        Ok(())
    }
}

#[cfg(windows)]
fn require_absent(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "cannot verify deleted path {}: {error}",
            path.display()
        )),
        Ok(_) => Err(format!(
            "cleanup path was recreated and preserved: {}",
            path.display()
        )),
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        fs::{File, OpenOptions},
        mem::{size_of, zeroed},
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    };
    use windows_sys::Win32::{Storage::FileSystem::*, System::IO::OVERLAPPED};

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(super) struct Identity(u32, u64);

    fn identity(file: &File) -> Result<Identity, String> {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(Identity(
            info.dwVolumeSerialNumber,
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        ))
    }

    pub(super) fn path_identity(path: &Path) -> Result<Identity, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        identity(&file)
    }

    pub(super) fn read_anchor(path: &Path, expected: Identity) -> Result<File, String> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|e| e.to_string())?;
        if identity(&file)? != expected {
            return Err("cleanup hard-link identity changed".into());
        }
        Ok(file)
    }

    pub(super) struct Guard {
        file: File,
        path: PathBuf,
    }
    impl Guard {
        pub(super) fn require_identity(&self, expected: Identity) -> Result<(), String> {
            if identity(&self.file)? != expected {
                return Err(format!(
                    "cleanup hard-link identity changed: {}",
                    self.path.display()
                ));
            }
            Ok(())
        }
        pub(super) fn open(entry: &Entry) -> Result<Self, String> {
            let mut options = OpenOptions::new();
            options
                .read(true)
                .access_mode(0x80000000 | DELETE | FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
            let file = options.open(&entry.path).map_err(|e| {
                format!(
                    "cleanup target busy or inaccessible: {}: {e}",
                    entry.path.display()
                )
            })?;
            Ok(Self {
                file,
                path: entry.path.clone(),
            })
        }
        pub(super) fn lock(&self) -> Result<(), String> {
            let mut overlap: OVERLAPPED = unsafe { zeroed() };
            if unsafe {
                LockFileEx(
                    self.file.as_raw_handle(),
                    LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                    0,
                    1,
                    0,
                    &mut overlap,
                )
            } == 0
            {
                return Err(format!(
                    "active file lock: {}: {}",
                    self.path.display(),
                    std::io::Error::last_os_error()
                ));
            }
            Ok(())
        }
        pub(super) fn remove(self) -> Result<(), String> {
            // Delete only the opened object and retain the image-section check.
            // A mapped executor inside the deletion set is rejected in preflight.
            // Ignore READONLY for this deletion only. Never clear attributes:
            // failed deletions and surviving hard links retain their exact state.
            // DELETE access, sharing checks and file locks remain mandatory.
            let info = FILE_DISPOSITION_INFO_EX {
                Flags: FILE_DISPOSITION_FLAG_DELETE
                    | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
                    | FILE_DISPOSITION_FLAG_FORCE_IMAGE_SECTION_CHECK
                    | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
            };
            if unsafe {
                SetFileInformationByHandle(
                    self.file.as_raw_handle(),
                    FileDispositionInfoEx,
                    (&info as *const FILE_DISPOSITION_INFO_EX).cast(),
                    size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
                )
            } == 0
            {
                return Err(format!(
                    "cleanup deletion failed: {}: {}",
                    self.path.display(),
                    std::io::Error::last_os_error()
                ));
            }
            drop(self.file);
            Ok(())
        }
    }
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_artifact_cleanup.rs"]
mod tests;
