use super::worker::{ReconcileLock, clear_pending_if_unchanged, mark_pending};
use super::{
    ConflictRecord, MemoryResult, MemorySyncError, SnapshotManifest, atomic_write_json,
    bootstrap_merge, build_snapshot, record_conflict, snapshot_files, snapshot_from_files,
    three_way_merge, utc_now, verify_snapshot,
};
use crate::{ProcessRequest, ProcessRunner};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SyncedState {
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
    content_sha256: String,
    revision: u64,
    commit: Option<String>,
    utc: String,
}

#[derive(Clone, Debug)]
struct RemoteSnapshot {
    manifest: Option<SnapshotManifest>,
    path: Option<PathBuf>,
    commit: Option<String>,
}

struct Git<'a> {
    runner: &'a dyn ProcessRunner,
}

impl Git<'_> {
    fn bytes(&self, cwd: &Path, args: &[&str], stdin: &[u8]) -> MemoryResult<Vec<u8>> {
        let mut request = ProcessRequest::new("git", cwd);
        request.args = args.iter().map(OsString::from).collect();
        request.stdin = stdin.to_vec();
        request.timeout = Duration::from_secs(180);
        let output = self
            .runner
            .run(&request)
            .map_err(|error| MemorySyncError::new(format!("cannot launch git: {error}")))?;
        if output.timed_out || output.code != 0 {
            return Err(MemorySyncError::new(format!(
                "git {} failed{}: {}",
                args.join(" "),
                if output.timed_out { " (timed out)" } else { "" },
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(output.stdout)
    }

    fn run(
        &self,
        cwd: &Path,
        args: &[&str],
        env: &[(&str, &str)],
        allow_failure: bool,
    ) -> MemoryResult<(i32, String, String)> {
        let mut request = ProcessRequest::new("git", cwd);
        request.args = args.iter().map(OsString::from).collect();
        request.timeout = Duration::from_secs(180);
        for (key, value) in env {
            request
                .env
                .insert(OsString::from(key), OsString::from(value));
        }
        let output = self
            .runner
            .run(&request)
            .map_err(|error| MemorySyncError::new(format!("cannot launch git: {error}")))?;
        if output.timed_out {
            return Err(MemorySyncError::new(format!(
                "git {} timed out",
                args.join(" ")
            )));
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if output.code != 0 && !allow_failure {
            return Err(MemorySyncError::new(format!(
                "git {} failed: {}",
                args.join(" "),
                if stderr.is_empty() { &stdout } else { &stderr }
            )));
        }
        Ok((output.code, stdout, stderr))
    }

    fn required(&self, cwd: &Path, args: &[&str]) -> MemoryResult<String> {
        self.run(cwd, args, &[], false).map(|(_, stdout, _)| stdout)
    }
}

fn verify_origin(git: &Git<'_>, cwd: &Path, remote: &str, push: bool) -> MemoryResult<()> {
    let approved = super::github_repository_identity(remote)?;
    let args: &[&str] = if push {
        &["remote", "get-url", "--push", "--all", "origin"]
    } else {
        &["remote", "get-url", "--all", "origin"]
    };
    let destinations = git.required(cwd, args)?;
    if destinations.is_empty() {
        return Err(MemorySyncError::new(
            "native memories Git destination is missing",
        ));
    }
    for destination in destinations.lines() {
        if !super::github_repository_identity(destination)?.eq_ignore_ascii_case(&approved) {
            return Err(MemorySyncError::new(
                "native memories Git URL rewrite changes the approved repository",
            ));
        }
    }
    Ok(())
}

fn unique_work_dir(state_dir: &Path) -> MemoryResult<PathBuf> {
    fs::create_dir_all(state_dir)?;
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| MemorySyncError::new(error.to_string()))?
        .as_nanos();
    let path = state_dir.join(format!(".reconcile-{}-{token}", std::process::id()));
    fs::create_dir(&path)?;
    Ok(path)
}

fn remove_tree(path: &Path) -> MemoryResult<()> {
    if !path.exists() {
        return Ok(());
    }
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(MemorySyncError::new(format!(
            "refusing to remove linked work path: {}",
            path.display()
        )));
    }
    fs::remove_dir_all(path)?;
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> MemoryResult<()> {
    if target.exists() {
        remove_tree(target)?;
    }
    fs::create_dir_all(target)?;
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| MemorySyncError::new(error.to_string()))?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| MemorySyncError::new(error.to_string()))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let destination = target.join(relative);
        if entry.file_type().is_symlink() {
            return Err(MemorySyncError::new(format!(
                "snapshot contains a linked path: {}",
                entry.path().display()
            )));
        }
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

fn read_remote(git: &Git<'_>, work: &Path, remote: &str) -> MemoryResult<RemoteSnapshot> {
    let bare = work.join("remote.git");
    fs::create_dir_all(&bare)?;
    git.required(work, &["init", "--bare", bare.to_string_lossy().as_ref()])?;
    git.required(&bare, &["remote", "add", "origin", remote])?;
    verify_origin(git, &bare, remote, false)?;
    let (code, _, stderr) = git.run(
        &bare,
        &[
            "fetch",
            "--prune",
            "origin",
            "+refs/heads/main:refs/remotes/origin/main",
        ],
        &[],
        true,
    )?;
    if code != 0 {
        let lower = stderr.to_lowercase();
        if lower.contains("couldn't find remote ref") || lower.contains("not found") {
            return Ok(RemoteSnapshot {
                manifest: None,
                path: None,
                commit: None,
            });
        }
        return Err(MemorySyncError::new(format!(
            "remote fetch failed: {stderr}"
        )));
    }
    let commit = git.required(&bare, &["rev-parse", "refs/remotes/origin/main"])?;
    let (code, manifest_text, _) = git.run(
        &bare,
        &["show", "refs/remotes/origin/main:snapshot-manifest.json"],
        &[],
        true,
    )?;
    if code != 0 {
        return Ok(RemoteSnapshot {
            manifest: None,
            path: None,
            commit: Some(commit),
        });
    }
    let manifest: SnapshotManifest = match serde_json::from_str(&manifest_text) {
        Ok(value) => value,
        Err(_) => {
            return Ok(RemoteSnapshot {
                manifest: None,
                path: None,
                commit: Some(commit),
            });
        }
    };
    let extracted = work.join("remote-snapshot");
    fs::create_dir_all(&extracted)?;
    fs::create_dir_all(extracted.join("memories"))?;
    // Plumbing reads the stored blob bytes directly: checkout applies attributes,
    // smudge filters and line-ending conversions even with core.autocrlf=false.
    for entry in git
        .bytes(
            &bare,
            &["ls-tree", "-r", "-z", &commit, "--", "memories"],
            &[],
        )?
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let entry = std::str::from_utf8(entry)
            .map_err(|error| MemorySyncError::new(format!("invalid snapshot tree: {error}")))?;
        let (metadata, path) = entry
            .split_once('\t')
            .ok_or_else(|| MemorySyncError::new("invalid snapshot tree entry"))?;
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 || !matches!(fields[0], "100644" | "100755") || fields[1] != "blob" {
            return Err(MemorySyncError::new(
                "snapshot tree contains a non-file entry",
            ));
        }
        let relative = path
            .strip_prefix("memories/")
            .ok_or_else(|| MemorySyncError::new("snapshot tree path is outside memories"))?;
        let target = extracted
            .join("memories")
            .join(super::safe_relative(relative)?);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(
            target,
            git.bytes(&bare, &["cat-file", "blob", fields[2]], &[])?,
        )?;
    }
    fs::write(
        extracted.join("snapshot-manifest.json"),
        manifest_text.as_bytes(),
    )?;
    if verify_snapshot(&extracted, &manifest).is_err() {
        return Ok(RemoteSnapshot {
            manifest: None,
            path: None,
            commit: Some(commit),
        });
    }
    let sync_files = snapshot_files(&extracted)?;
    let projected = work.join("remote-sync-snapshot");
    let projected_manifest = snapshot_from_files(&projected, &sync_files, manifest.revision)?;
    Ok(RemoteSnapshot {
        manifest: Some(projected_manifest),
        path: Some(projected),
        commit: Some(commit),
    })
}

fn push_snapshot(
    git: &Git<'_>,
    snapshot: &Path,
    work: &Path,
    remote: &str,
    expected: Option<&str>,
    local: (&Path, &SnapshotManifest),
) -> MemoryResult<String> {
    let publish = work.join("publish");
    fs::create_dir(&publish)?;
    git.required(&publish, &["init", "--bare"])?;
    let mut files = snapshot_files(snapshot)?
        .into_iter()
        .map(|(path, bytes)| (format!("memories/{path}"), bytes))
        .collect::<BTreeMap<_, _>>();
    files.insert(
        "snapshot-manifest.json".into(),
        fs::read(snapshot.join("snapshot-manifest.json"))?,
    );
    let mut index = Vec::new();
    for (path, bytes) in files {
        let hash = git.bytes(
            &publish,
            &["hash-object", "--no-filters", "-w", "--stdin"],
            &bytes,
        )?;
        let hash = std::str::from_utf8(&hash)
            .map_err(|error| MemorySyncError::new(error.to_string()))?
            .trim();
        if git.bytes(&publish, &["cat-file", "blob", hash], &[])? != bytes {
            return Err(MemorySyncError::new(
                "stored snapshot blob differs from captured bytes",
            ));
        }
        index.extend_from_slice(format!("100644 {hash}\t{path}\0").as_bytes());
    }
    git.bytes(&publish, &["update-index", "-z", "--index-info"], &index)?;
    let tree = git.required(&publish, &["write-tree"])?;
    git.required(&publish, &["remote", "add", "origin", remote])?;
    verify_origin(git, &publish, remote, false)?;
    let mut parent = Vec::new();
    if let Some(expected) = expected {
        git.required(&publish, &["fetch", "--no-tags", "origin", expected])?;
        let actual = git.required(&publish, &["rev-parse", "FETCH_HEAD"])?;
        if actual != expected {
            return Err(MemorySyncError::new(
                "remote HEAD changed before snapshot commit was created",
            ));
        }
        parent.extend(["-p", expected]);
    }
    let mut args = vec!["commit-tree", tree.as_str()];
    args.extend(parent);
    args.extend(["-m", "bridgeforge-codex memories snapshot"]);
    let env = [
        ("GIT_AUTHOR_NAME", "bridgeforge-codex Memory Sync"),
        ("GIT_AUTHOR_EMAIL", "bridgeforge-codex@invalid"),
        ("GIT_COMMITTER_NAME", "bridgeforge-codex Memory Sync"),
        ("GIT_COMMITTER_EMAIL", "bridgeforge-codex@invalid"),
    ];
    let (_, commit, _) = git.run(&publish, &args, &env, false)?;
    git.required(&publish, &["update-ref", "refs/heads/main", &commit])?;
    verify_origin(git, &publish, remote, true)?;
    super::verify_local_unchanged(local.0, Some(local.1))?;
    git.required(
        &publish,
        &["push", "origin", "refs/heads/main:refs/heads/main"],
    )?;
    Ok(commit)
}

fn restore_snapshot(
    snapshot: &Path,
    memories: &Path,
    expected: Option<&SnapshotManifest>,
) -> MemoryResult<()> {
    let manifest: SnapshotManifest =
        serde_json::from_slice(&fs::read(snapshot.join("snapshot-manifest.json"))?)?;
    let files = super::snapshot_files(snapshot)?;
    let parent = memories.parent().ok_or_else(|| {
        MemorySyncError::new(format!(
            "memories path has no parent: {}",
            memories.display()
        ))
    })?;
    super::ensure_real_directory(parent, true)?;
    let stage = super::temporary_sibling(memories, "incoming");
    fs::create_dir(&stage)?;
    let result = (|| {
        for (relative, payload) in files {
            let target = stage.join(super::safe_relative(&relative)?);
            fs::create_dir_all(target.parent().unwrap())?;
            fs::write(target, payload)?;
        }
        let projected = super::capture_manifest(&stage, manifest.revision, None)?;
        super::verify_manifest_directory(&stage, &projected)?;
        super::replace_memories_if_unchanged(&stage, memories, expected)
    })();
    if result.is_err() {
        remove_tree(&stage)?;
    }
    result
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_memory_remote.rs"]
mod tests;

fn baseline_path(state_dir: &Path) -> PathBuf {
    state_dir.join("last-synced-snapshot")
}

fn record_synced(
    state_dir: &Path,
    snapshot: &Path,
    manifest: &SnapshotManifest,
    commit: Option<String>,
) -> MemoryResult<()> {
    recover_baseline(state_dir)?;
    let baseline = baseline_path(state_dir);
    copy_tree(snapshot, &state_dir.join(".last-synced-snapshot-new"))?;
    if baseline.exists() {
        remove_tree(&state_dir.join(".last-synced-snapshot-old"))?;
        fs::rename(&baseline, state_dir.join(".last-synced-snapshot-old"))?;
    }
    fs::rename(state_dir.join(".last-synced-snapshot-new"), &baseline)?;
    let result = atomic_write_json(
        &state_dir.join("last-synced.json"),
        &SyncedState {
            schema_version: 2,
            content_sha256: manifest.content_sha256.clone(),
            revision: manifest.revision,
            commit,
            utc: utc_now(),
        },
    );
    if let Err(error) = result {
        recover_baseline(state_dir).map_err(|recovery| {
            MemorySyncError::new(format!("{error}; baseline recovery failed: {recovery}"))
        })?;
        return Err(error);
    }
    remove_tree(&state_dir.join(".last-synced-snapshot-old"))
}

fn recover_baseline(state_dir: &Path) -> MemoryResult<()> {
    let previous = state_dir.join(".last-synced-snapshot-old");
    if !previous.exists() {
        return Ok(());
    }
    let receipt = load_synced(state_dir).ok_or_else(|| {
        MemorySyncError::new(
            "interrupted baseline update has no trusted receipt; snapshots were preserved",
        )
    })?;
    let current = baseline_path(state_dir);
    let matches = |path: &Path| -> bool {
        super::read_manifest(path).ok().is_some_and(|manifest| {
            manifest.content_sha256 == receipt.content_sha256
                && manifest.revision == receipt.revision
        })
    };
    if matches(&current) {
        return remove_tree(&previous);
    }
    if !matches(&previous) {
        return Err(MemorySyncError::new(
            "interrupted baseline snapshots do not match the receipt; snapshots were preserved",
        ));
    }
    let interrupted = super::temporary_sibling(&current, "interrupted");
    if current.exists() {
        fs::rename(&current, &interrupted)?;
    }
    if let Err(error) = fs::rename(&previous, &current) {
        if interrupted.exists() {
            let _ = fs::rename(&interrupted, &current);
        }
        return Err(error.into());
    }
    Ok(())
}

fn clear_active_conflict(state_dir: &Path) -> MemoryResult<()> {
    match fs::remove_file(state_dir.join("active-conflict.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn load_synced(state_dir: &Path) -> Option<SyncedState> {
    serde_json::from_slice(&fs::read(state_dir.join("last-synced.json")).ok()?).ok()
}

pub fn validate_synced_baseline(state_dir: &Path) -> MemoryResult<()> {
    let path = state_dir.join("last-synced.json");
    if !path.exists() {
        return Ok(());
    }
    let state: SyncedState = serde_json::from_slice(&fs::read(&path)?)?;
    let manifest = super::read_manifest(&baseline_path(state_dir))?;
    if state.schema_version != 2
        || state.content_sha256 != manifest.content_sha256
        || state.revision != manifest.revision
    {
        return Err(MemorySyncError::new(
            "native memory baseline does not match its receipt",
        ));
    }
    Ok(())
}

fn baseline_files(
    state_dir: &Path,
    state: Option<&SyncedState>,
) -> Option<BTreeMap<String, Vec<u8>>> {
    let state = state?;
    let path = baseline_path(state_dir);
    let manifest: SnapshotManifest =
        serde_json::from_slice(&fs::read(path.join("snapshot-manifest.json")).ok()?).ok()?;
    if manifest.content_sha256 != state.content_sha256 {
        return None;
    }
    snapshot_files(&path).ok()
}

// Only the three local-only publication branches share this tail. Merge and
// restore retain their own source snapshot and post-push recovery semantics.
fn publish_local_snapshot(
    git: &Git<'_>,
    memories: &Path,
    state_dir: &Path,
    work: &Path,
    remote: &str,
    expected: Option<&str>,
    revision: u64,
    pending_before: Option<&[u8]>,
) -> MemoryResult<String> {
    let publish = work.join("publish-snapshot");
    let published = build_snapshot(memories, &publish, revision)?;
    let commit = push_snapshot(
        git,
        &publish,
        work,
        remote,
        expected,
        (memories, &published),
    )?;
    record_synced(state_dir, &publish, &published, Some(commit))?;
    clear_pending_if_unchanged(state_dir, pending_before)?;
    Ok("push".into())
}

fn reconcile_locked(
    memories: &Path,
    state_dir: &Path,
    remote: &str,
    runner: &dyn ProcessRunner,
    pending_before: Option<&[u8]>,
) -> MemoryResult<String> {
    recover_baseline(state_dir)?;
    let work = unique_work_dir(state_dir)?;
    let result = (|| {
        let git = Git { runner };
        let remote_snapshot = read_remote(&git, &work, remote)?;
        let state = load_synced(state_dir);
        if !memories.exists() {
            match (&remote_snapshot.manifest, &remote_snapshot.path) {
                (Some(manifest), Some(path)) => {
                    if !manifest.files.is_empty() {
                        restore_snapshot(path, memories, None)?;
                    }
                    record_synced(state_dir, path, manifest, remote_snapshot.commit.clone())?;
                    clear_pending_if_unchanged(state_dir, pending_before)?;
                    return Ok(if manifest.files.is_empty() {
                        "noop"
                    } else {
                        "restore"
                    }
                    .into());
                }
                (None, _) if remote_snapshot.commit.is_some() => {
                    return Err(MemorySyncError::new(
                        "remote snapshot is corrupt and no local memories exist to repair it",
                    ));
                }
                _ => {
                    clear_pending_if_unchanged(state_dir, pending_before)?;
                    return Ok("noop".into());
                }
            }
        }
        let local_path = work.join("local-snapshot");
        let local = build_snapshot(memories, &local_path, 0)?;
        let revision = state.as_ref().map(|item| item.revision).unwrap_or(0).max(
            remote_snapshot
                .manifest
                .as_ref()
                .map(|item| item.revision)
                .unwrap_or(0),
        ) + 1;
        let Some(remote_manifest) = remote_snapshot.manifest.as_ref() else {
            if remote_snapshot.commit.is_some() && local.files.is_empty() {
                return Err(MemorySyncError::new(
                    "remote snapshot is corrupt and local memories are empty",
                ));
            }
            return publish_local_snapshot(
                &git,
                memories,
                state_dir,
                &work,
                remote,
                remote_snapshot.commit.as_deref(),
                revision,
                pending_before,
            );
        };
        let remote_path = remote_snapshot.path.as_ref().unwrap();
        if super::same_manifest_files(&local, remote_manifest) {
            record_synced(
                state_dir,
                remote_path,
                remote_manifest,
                remote_snapshot.commit.clone(),
            )?;
            clear_pending_if_unchanged(state_dir, pending_before)?;
            return Ok("noop".into());
        }
        let synced_digest = state.as_ref().map(|item| item.content_sha256.as_str());
        if synced_digest.is_none() && remote_manifest.files.is_empty() {
            return publish_local_snapshot(
                &git,
                memories,
                state_dir,
                &work,
                remote,
                remote_snapshot.commit.as_deref(),
                revision,
                pending_before,
            );
        }
        if synced_digest.is_none() && local.files.is_empty() {
            restore_snapshot(remote_path, memories, Some(&local))?;
            record_synced(
                state_dir,
                remote_path,
                remote_manifest,
                remote_snapshot.commit.clone(),
            )?;
            clear_pending_if_unchanged(state_dir, pending_before)?;
            return Ok("restore".into());
        }
        let local_changed = synced_digest.is_none_or(|digest| local.content_sha256 != digest);
        let remote_changed =
            synced_digest.is_none_or(|digest| remote_manifest.content_sha256 != digest);
        if local_changed && !remote_changed {
            return publish_local_snapshot(
                &git,
                memories,
                state_dir,
                &work,
                remote,
                remote_snapshot.commit.as_deref(),
                revision,
                pending_before,
            );
        }
        if remote_changed && !local_changed {
            restore_snapshot(remote_path, memories, Some(&local))?;
            record_synced(
                state_dir,
                remote_path,
                remote_manifest,
                remote_snapshot.commit.clone(),
            )?;
            clear_pending_if_unchanged(state_dir, pending_before)?;
            return Ok("restore".into());
        }
        let local_files = snapshot_files(&local_path)?;
        let remote_files = snapshot_files(remote_path)?;
        let base = baseline_files(state_dir, state.as_ref());
        let merged = match base.as_ref() {
            Some(base) => three_way_merge(base, &local_files, &remote_files),
            None => bootstrap_merge(&local_files, &remote_files),
        };
        if !merged.conflicts.is_empty() || base.is_none() {
            record_conflict(
                state_dir,
                &local_path,
                remote_path,
                base.as_ref().map(|_| baseline_path(state_dir)).as_deref(),
                &merged.files,
                &merged.conflicts,
                remote_snapshot.commit.as_deref(),
                revision,
                if base.is_none() {
                    "bootstrap conflict: both sides changed without a trusted three-way baseline"
                } else {
                    "the same native memory path changed differently on both computers"
                },
            )?;
            return Ok("conflicted".into());
        }
        let merged_path = work.join("merged-snapshot");
        let merged_manifest = snapshot_from_files(&merged_path, &merged.files, revision)?;
        let (action, commit) = if super::same_manifest_files(&merged_manifest, remote_manifest) {
            ("restore", remote_snapshot.commit.clone())
        } else {
            let commit = push_snapshot(
                &git,
                &merged_path,
                &work,
                remote,
                remote_snapshot.commit.as_deref(),
                (memories, &local),
            )?;
            ("merge", Some(commit))
        };
        if !super::same_manifest_files(&merged_manifest, &local) {
            restore_snapshot(&merged_path, memories, Some(&local))?;
        }
        record_synced(state_dir, &merged_path, &merged_manifest, commit)?;
        clear_pending_if_unchanged(state_dir, pending_before)?;
        Ok(action.into())
    })();
    let result = result.and_then(|action| {
        if action != "conflicted" {
            clear_active_conflict(state_dir)?;
        }
        Ok(action)
    });
    if let Err(error) = remove_tree(&work) {
        let _ = mark_pending(state_dir, "work-cleanup-failed");
        if result.is_ok() {
            return Err(error);
        }
    }
    result
}

fn verify_remote(state_dir: &Path, remote: &str, runner: &dyn ProcessRunner) -> MemoryResult<()> {
    let cwd = state_dir
        .ancestors()
        .find(|path| path.is_dir())
        .ok_or_else(|| {
            MemorySyncError::new("native memories verification has no existing working directory")
        })?;
    super::MemoryRemoteClient::new(runner).verify_private_github_repository(cwd, remote)
}

pub fn reconcile(
    memories: &Path,
    state_dir: &Path,
    remote: &str,
    runner: &dyn ProcessRunner,
) -> MemoryResult<String> {
    verify_remote(state_dir, remote, runner)?;
    fs::create_dir_all(state_dir)?;
    let pending_before = fs::read(state_dir.join("pending.json")).ok();
    let Some(_lock) = ReconcileLock::try_acquire(state_dir)? else {
        mark_pending(state_dir, "deduplicated")?;
        return Ok("busy".into());
    };
    reconcile_locked(
        memories,
        state_dir,
        remote,
        runner,
        pending_before.as_deref(),
    )
}

fn verify_active_conflict(state_dir: &Path, conflict_id: &str) -> MemoryResult<()> {
    let active: serde_json::Value =
        serde_json::from_slice(&fs::read(state_dir.join("active-conflict.json"))?)?;
    if active["conflictId"].as_str() != Some(conflict_id) {
        return Err(MemorySyncError::new(
            "active native memory conflict identity changed",
        ));
    }
    Ok(())
}

fn apply_conflict_choices_locked(
    state_dir: &Path,
    conflict_id: &str,
    choices: &[(String, String)],
) -> MemoryResult<()> {
    let evidence = state_dir.join("conflicts").join(conflict_id);
    let record: ConflictRecord =
        serde_json::from_slice(&fs::read(evidence.join("conflict.json"))?)?;
    if record.conflict_id != conflict_id {
        return Err(MemorySyncError::new(
            "native memory conflict record identity changed",
        ));
    }
    let expected = record
        .conflict_paths
        .into_iter()
        .filter(|path| !super::is_local_only_memory_path(path))
        .collect::<BTreeSet<_>>();
    let mut selected = BTreeMap::<String, String>::new();
    for (path, side) in choices {
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err(MemorySyncError::new(format!(
                "native memory conflict path is unsafe: {path}"
            )));
        }
        if side != "local" && side != "remote" {
            return Err(MemorySyncError::new(format!(
                "native memory conflict choice must be local or remote: {path}"
            )));
        }
        if !expected.contains(path) || selected.insert(path.clone(), side.clone()).is_some() {
            return Err(MemorySyncError::new(format!(
                "native memory conflict choice is unknown or duplicated: {path}"
            )));
        }
    }
    if selected.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err(MemorySyncError::new(
            "every native memory conflict path must be selected exactly once",
        ));
    }
    for (path, side) in selected {
        let source = evidence.join(side).join("memories").join(&path);
        let target = evidence.join("merged/memories").join(&path);
        if source.is_file() {
            if super::is_link_or_reparse(&source)? {
                return Err(MemorySyncError::new(format!(
                    "native memory conflict source is linked or unsafe: {path}"
                )));
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source, target)?;
        } else if target.exists() {
            fs::remove_file(target)?;
        }
    }
    Ok(())
}

pub fn apply_conflict_choices(
    state_dir: &Path,
    conflict_id: &str,
    choices: &[(String, String)],
) -> MemoryResult<()> {
    let Some(_lock) = ReconcileLock::try_acquire(state_dir)? else {
        return Err(MemorySyncError::new(
            "native memory reconciliation is already running",
        ));
    };
    verify_active_conflict(state_dir, conflict_id)?;
    apply_conflict_choices_locked(state_dir, conflict_id, choices)
}

pub fn resolve_conflict(
    memories: &Path,
    state_dir: &Path,
    remote: &str,
    conflict_id: &str,
    runner: &dyn ProcessRunner,
) -> MemoryResult<String> {
    resolve_conflict_with_choices(memories, state_dir, remote, conflict_id, &[], runner)
}

pub fn resolve_conflict_with_choices(
    memories: &Path,
    state_dir: &Path,
    remote: &str,
    conflict_id: &str,
    choices: &[(String, String)],
    runner: &dyn ProcessRunner,
) -> MemoryResult<String> {
    verify_remote(state_dir, remote, runner)?;
    fs::create_dir_all(state_dir)?;
    let Some(_lock) = ReconcileLock::try_acquire(state_dir)? else {
        return Err(MemorySyncError::new(
            "native memory reconciliation is already running",
        ));
    };
    verify_active_conflict(state_dir, conflict_id)?;
    apply_conflict_choices_locked(state_dir, conflict_id, choices)?;
    let evidence = state_dir.join("conflicts").join(conflict_id);
    if !evidence.is_dir() || super::is_link_or_reparse(&evidence)? {
        return Err(MemorySyncError::new(
            "native memory conflict evidence is missing or unsafe",
        ));
    }
    let record: ConflictRecord =
        serde_json::from_slice(&fs::read(evidence.join("conflict.json"))?)?;
    if record.conflict_id != conflict_id {
        return Err(MemorySyncError::new(
            "native memory conflict record identity changed",
        ));
    }
    let resolved_memories = evidence.join("merged/memories");
    if !resolved_memories.is_dir() || super::is_link_or_reparse(&resolved_memories)? {
        return Err(MemorySyncError::new(
            "edit the conflict merged/memories tree before resolving",
        ));
    }
    let captured_local = super::read_manifest(&evidence.join("local"))?;
    super::verify_local_unchanged(memories, Some(&captured_local))?;
    let work = unique_work_dir(state_dir)?;
    let result = (|| {
        let git = Git { runner };
        let remote_snapshot = read_remote(&git, &work, remote)?;
        let expected_remote_commit = if remote_snapshot.commit.as_deref()
            == record.remote_commit.as_deref()
        {
            record.remote_commit.clone()
        } else {
            let remote_matches_captured_local = remote_snapshot
                .manifest
                .as_ref()
                .is_some_and(|manifest| super::same_manifest_files(manifest, &captured_local));
            if !remote_matches_captured_local {
                return Err(MemorySyncError::new(
                    "remote HEAD changed after conflict capture; reconcile again before resolving",
                ));
            }
            remote_snapshot.commit.clone()
        };
        let revision = remote_snapshot
            .manifest
            .as_ref()
            .map(|manifest| manifest.revision + 1)
            .unwrap_or(1);
        let resolved = work.join("resolved-snapshot");
        let manifest = build_snapshot(&resolved_memories, &resolved, revision)?;
        let commit = push_snapshot(
            &git,
            &resolved,
            &work,
            remote,
            expected_remote_commit.as_deref(),
            (memories, &captured_local),
        )?;
        restore_snapshot(&resolved, memories, Some(&captured_local))?;
        record_synced(state_dir, &resolved, &manifest, Some(commit))?;
        clear_active_conflict(state_dir)?;
        clear_pending_if_unchanged(state_dir, None)?;
        Ok("resolved".to_string())
    })();
    let cleanup = remove_tree(&work);
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), _) => Err(error),
    }
}

pub fn referenced_paths(manifest: &SnapshotManifest) -> BTreeSet<String> {
    manifest
        .files
        .iter()
        .map(|item| item.path.clone())
        .collect()
}
