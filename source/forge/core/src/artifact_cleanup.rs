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
    junction: Option<Junction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Junction {
    path: PathBuf,
    target: PathBuf,
    identity: (u32, u64),
    reparse_data: Vec<u8>,
}

fn inspect(path: &Path) -> Result<Entry, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let junction = if crate::persistence::is_link_or_reparse(path).map_err(|e| e.to_string())? {
        #[cfg(windows)]
        { Some(windows::junction(path)?) }
        #[cfg(not(windows))]
        { return Err(format!("unsupported cleanup link: {}", path.display())); }
    } else { None };
    if junction.is_none() && !metadata.is_file() && !metadata.is_dir() {
        return Err(format!(
            "linked or unsupported cleanup entry: {}",
            path.display()
        ));
    }
    Ok(Entry {
        path: path.into(),
        directory: metadata.is_dir() || junction.is_some(),
        bytes: if metadata.is_file() && junction.is_none() {
            metadata.len()
        } else {
            0
        },
        modified: metadata.modified().ok(),
        created: metadata.created().ok(),
        junction,
    })
}

/// Only the final component may be a recognized Junction. Parents remain
/// subject to the managed no-reparse policy, and no target is ever resolved.
fn product_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() || relative.contains(['\\', '*', '?', '['])
        || Path::new(relative).components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return Err(format!("unsafe cleanup product path: {relative}"));
    }
    let path = root.join(relative);
    let parent = Path::new(relative).parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(parent) = parent {
        crate::managed_paths::safe_join(root, &parent.to_string_lossy(), "cleanup product parent")?;
    } else {
        crate::managed_paths::safe_join(root, RUNTIME, "cleanup product root")?;
    }
    match fs::symlink_metadata(&path) {
        Ok(_) => { inspect(&path)?; }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.to_string()),
    }
    Ok(path)
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
    for (index, ancestor) in path.ancestors().enumerate() {
        if inspect(ancestor)?.junction.is_some() && index != 0 {
            return Err(format!("cleanup ancestor is a Junction: {}", ancestor.display()));
        }
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
            let normalized = crate::artifact_paths::normalized(relative);
            if !(crate::artifact_paths::within(relative, BUILD)
                || crate::artifact_paths::within(relative, RUNTIME))
                || [BUILD, RUNTIME].contains(&normalized.trim_end_matches('/'))
            {
                return Err("cleanup keep role must name an exact path below .build or .runtime".into());
            }
            product_path(root, relative)?;
        }
        for (relative, expected) in &self.witnesses {
            let path = crate::managed_paths::safe_join(root, relative, "cleanup policy")?;
            if crate::managed_paths::read_metadata(&path, 16 * 1024 * 1024)? != *expected {
                return Err(format!("cleanup policy changed: {relative}"));
            }
        }
        for (relative, directory) in &self.directories {
            let path = product_path(root, relative)?;
            match fs::symlink_metadata(&path) {
                Ok(_) => {
                    if inspect(&path)?.directory != *directory {
                        return Err(format!("cleanup output type changed: {relative}"));
                    }
                }
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
            if entry.junction.is_some() {
                return Err("cleanup ownership selection contains a Junction".into());
            }
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
    if entry.directory && entry.junction.is_none() {
        for child in fs::read_dir(path).map_err(|e| format!("cannot enumerate cleanup directory {}: {e}", path.display()))? {
            collect_build(
                root,
                &child.map_err(|e| format!("cannot read cleanup entry under {}: {e}", path.display()))?.path(),
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
pub struct JunctionReport {
    pub path: String,
    pub target: String,
    pub action: &'static str,
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
    pub planned_junctions: usize,
    pub junctions: Vec<JunctionReport>,
    pub deleted_files: usize,
    pub deleted_directories: usize,
    pub deleted_junctions: usize,
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
        // Match the same filesystem path spelling during selection and deletion.
        // In particular, a trailing '/.' must not turn a kept directory into
        // an unmatched string prefix and expose its children to deletion.
        for relative in build.keep.iter_mut().chain(build.remove.iter_mut()) {
            let raw = relative.replace('\\', "/");
            let mut parts = Vec::new();
            for component in Path::new(&raw).components() {
                match component {
                    std::path::Component::Normal(part) => {
                        let part = part.to_string_lossy();
                        if cfg!(windows) && (part.ends_with(['.', ' ']) || part.contains(':')) {
                            return Err(format!("ambiguous Windows cleanup product path: {relative}"));
                        }
                        parts.push(part.into_owned());
                    }
                    std::path::Component::CurDir => (),
                    _ => return Err(format!("unsafe cleanup product path: {relative}")),
                }
            }
            *relative = parts.join("/");
            let target = product_path(&root, relative)?;
            match fs::symlink_metadata(&target) {
                Ok(_) => {
                    // Resolve existing 8.3 aliases to the same long path names
                    // returned by directory enumeration. Never follow links:
                    // safe_join checks ancestors first, validate checks again.
                    let actual = match inspect(&target)?.junction {
                        Some(junction) => junction.path,
                        None => match target.canonicalize() {
                            Ok(path) => path,
                            #[cfg(windows)]
                            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => windows::metadata_product_path(&target)?,
                            Err(error) => return Err(format!("cannot identify cleanup product {}: {error}", target.display())),
                        },
                    };
                    *relative = actual
                        .strip_prefix(&root).map_err(|_| "cleanup product escapes project root")?
                        .to_string_lossy().replace('\\', "/");
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.to_string()),
            }
        }
        for relative in [CONFIG, DOWNLOAD] {
            build.keep.push(relative.into());
            build.directories.insert(relative.into(), true);
        }
        if let Ok(image) = std::env::current_exe().and_then(|p| p.canonicalize()) {
            if let Ok(relative) = image.strip_prefix(&root) {
                let relative = relative.to_string_lossy().replace('\\', "/");
                if crate::artifact_paths::within(&relative, BUILD)
                    || crate::artifact_paths::within(&relative, RUNTIME) {
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
        build.keep.retain(|relative| fs::symlink_metadata(root.join(relative)).is_ok());
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
            for child in fs::read_dir(&path).map_err(|e| format!("cannot enumerate cleanup directory {}: {e}", path.display()))? {
                collect_build(
                    &root,
                    &child.map_err(|e| format!("cannot read cleanup entry under {}: {e}", path.display()))?.path(),
                    &build,
                    &mut entries,
                    &mut targets,
                    &mut skipped,
                )?;
            }
        }
        #[cfg(windows)]
        {
            let own = windows::path_identity(&std::env::current_exe().map_err(|e| e.to_string())?)?;
            for entry in entries
                .iter()
                .filter(|e| !e.directory)
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
                self.build.keep.iter().filter(|p| ![CONFIG, DOWNLOAD].contains(&p.as_str())).cloned().collect(),
            ]
            .concat(),
            planned_files: self.entries.iter().filter(|e| !e.directory).count(),
            planned_directories: self.entries.iter().filter(|e| e.directory && e.junction.is_none()).count(),
            planned_junctions: self.entries.iter().filter(|e| e.junction.is_some()).count(),
            junctions: self.entries.iter().filter_map(|entry| entry.junction.as_ref().map(|junction| JunctionReport {
                path: entry.path.strip_prefix(&self.root).unwrap().to_string_lossy().replace('\\', "/"),
                target: junction.target.to_string_lossy().into_owned(),
                action: "remove_link_only",
            })).collect(),
            deleted_files: 0,
            deleted_directories: 0,
            deleted_junctions: 0,
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
        } else if report.deleted_files + report.deleted_directories + report.deleted_junctions == 0 {
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
            if entry.junction.is_some() && inspect(&entry.path)? != *entry {
                return Err(format!("cleanup directory or Junction drifted: {}", entry.path.display()));
            }
            if entry.junction.is_none() && fs::read_dir(&entry.path)
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
            if entry.junction.is_some() { report.deleted_junctions += 1; }
            else { report.deleted_directories += 1; }
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
        os::windows::{ffi::{OsStrExt, OsStringExt}, fs::OpenOptionsExt, io::AsRawHandle},
    };
    use windows_sys::Win32::{Storage::FileSystem::*, System::{IO::{DeviceIoControl, OVERLAPPED}, Ioctl::FSCTL_GET_REPARSE_POINT, SystemServices::IO_REPARSE_TAG_MOUNT_POINT}};

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(super) struct Identity(u32, u64);

    /// Resolve an explicitly kept, unreadable leaf by querying its readable
    /// parent. This handles 8.3 names without opening the protected leaf.
    pub(super) fn metadata_product_path(path: &Path) -> Result<PathBuf, String> {
        let input = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let mut data: WIN32_FIND_DATAW = unsafe { zeroed() };
        let handle = unsafe { FindFirstFileW(input.as_ptr(), &mut data) };
        if handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return Err(format!("cannot identify kept path {}: {}", path.display(), std::io::Error::last_os_error()));
        }
        unsafe { FindClose(handle); }
        if data.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(format!("kept path changed to a reparse point: {}", path.display()));
        }
        let length = data.cFileName.iter().position(|c| *c == 0).ok_or("invalid kept leaf name")?;
        let name = std::ffi::OsString::from_wide(&data.cFileName[..length]);
        if Path::new(&name).components().count() != 1 || !matches!(Path::new(&name).components().next(), Some(std::path::Component::Normal(_))) {
            return Err("invalid kept leaf component".into());
        }
        let parent = path.parent().ok_or("kept path has no parent")?;
        Ok(parent.canonicalize().map_err(|e| format!("cannot identify kept parent {}: {e}", parent.display()))?.join(name))
    }

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

    fn junction_from_handle(file: &File) -> Result<Junction, String> {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let required = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
        if info.dwFileAttributes & required != required {
            return Err("cleanup Junction must be a directory reparse point".into());
        }
        let mut data = vec![0u8; 16 * 1024];
        let mut length = 0;
        if unsafe { DeviceIoControl(file.as_raw_handle(), FSCTL_GET_REPARSE_POINT,
            std::ptr::null(), 0, data.as_mut_ptr().cast(), data.len() as u32,
            &mut length, std::ptr::null_mut()) } == 0 {
            return Err(format!("cannot inspect Junction: {}", std::io::Error::last_os_error()));
        }
        data.truncate(length as usize);
        let target = junction_target(&data)?;
        let mut path = vec![0u16; 32768];
        let count = unsafe { GetFinalPathNameByHandleW(file.as_raw_handle(), path.as_mut_ptr(), path.len() as u32, FILE_NAME_NORMALIZED | VOLUME_NAME_DOS) };
        if count == 0 || count as usize >= path.len() {
            return Err(format!("cannot identify Junction path: {}", std::io::Error::last_os_error()));
        }
        let path = PathBuf::from(std::ffi::OsString::from_wide(&path[..count as usize]));
        let id = (info.dwVolumeSerialNumber, (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow));
        Ok(Junction { path, target, identity: id, reparse_data: data })
    }

    pub(super) fn junction_target(data: &[u8]) -> Result<PathBuf, String> {
        if data.len() < 16 || u32::from_le_bytes(data[..4].try_into().unwrap()) != IO_REPARSE_TAG_MOUNT_POINT {
            return Err("unsupported cleanup reparse point; only directory Junctions are supported".into());
        }
        let payload_length = u16::from_le_bytes(data[4..6].try_into().unwrap()) as usize;
        let offset = u16::from_le_bytes(data[8..10].try_into().unwrap()) as usize;
        let target_length = u16::from_le_bytes(data[10..12].try_into().unwrap()) as usize;
        let start = 16 + offset;
        let end = start + target_length;
        if payload_length + 8 != data.len() || offset % 2 != 0 || target_length % 2 != 0 || end > data.len() {
            return Err("invalid Junction reparse payload".into());
        }
        let units = data[start..end].chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect::<Vec<_>>();
        // MOUNT_POINT also identifies volume mounts. Only drive-qualified
        // directory Junctions (\??\C:\...) belong to this cleanup operation.
        if units.len() < 7 || !units.starts_with(&[92, 63, 63, 92])
            || !matches!(units[4], 65..=90 | 97..=122) || units[5] != 58 || units[6] != 92 {
            return Err("volume mounts and non-local Junction targets are not cleanup entries".into());
        }
        let mut target = vec![92, 92, 63, 92];
        target.extend_from_slice(&units[4..]);
        Ok(PathBuf::from(std::ffi::OsString::from_wide(&target)))
    }

    pub(super) fn junction(path: &Path) -> Result<Junction, String> {
        // Read only the reparse object. No file in its target is opened.
        let file = OpenOptions::new().read(true).access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path).map_err(|e| format!("cannot open Junction {}: {e}", path.display()))?;
        junction_from_handle(&file).map_err(|e| format!("{}: {e}", path.display()))
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
            if let Some(expected) = &entry.junction {
                if junction_from_handle(&file)? != *expected {
                    return Err(format!("cleanup Junction identity, type or target changed: {}", entry.path.display()));
                }
            }
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
