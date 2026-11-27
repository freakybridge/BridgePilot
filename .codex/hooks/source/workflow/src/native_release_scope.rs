//! Bind the reviewed worktree to the release, including unstaged and untracked bytes.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize, PartialEq, Eq)]
pub(super) struct Change {
    status: String,
    index: String,
    content: String,
    #[cfg(unix)]
    mode: u32,
}
pub(super) type Changes = BTreeMap<String, Change>;

pub(super) fn changes(root: &Path, runner: &dyn ProcessRunner) -> Result<Changes, Stop> {
    let status = request(
        root,
        runner,
        &[
            "git",
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "--no-renames",
            "-z",
            "--untracked-files=all",
        ]
        .map(str::to_owned),
        60,
    )?;
    let mut files = Changes::new();
    for record in status.split_terminator('\0') {
        let path = record
            .get(3..)
            .ok_or_else(|| conflict("invalid Git status record"))?;
        let file = root.join(path);
        let content = match fs::symlink_metadata(&file) {
            Ok(m) if m.is_symlink() => fs::read_link(&file)
                .map_err(io_error)?
                .to_string_lossy()
                .into_owned(),
            Ok(m) if m.is_file() => {
                let mut digest = Sha256::new();
                std::io::copy(&mut fs::File::open(file).map_err(io_error)?, &mut digest)
                    .map_err(io_error)?;
                format!("{:x}", digest.finalize())
            }
            Ok(m) if m.is_dir() && file.join(".git").exists() => {
                if !git(
                    &file,
                    runner,
                    &["--no-optional-locks", "status", "--porcelain"],
                )?
                .is_empty()
                {
                    return Err(conflict(format!(
                        "nested repository has uncommitted changes: {path}"
                    )));
                }
                git(&file, runner, &["rev-parse", "HEAD"])?
            }
            Ok(m) if m.is_dir() => "directory".into(),
            Ok(_) => return Err(conflict(format!("unsupported Git working file: {path}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(io_error(e)),
        };
        let entry = files.entry(path.to_owned()).or_insert(Change {
            status: String::new(),
            index: String::new(),
            content,
            #[cfg(unix)]
            mode: fs::symlink_metadata(root.join(path))
                .map(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111)
                .unwrap_or(0),
        });
        entry.status.push_str(&record[..2]);
    }
    if !files.is_empty() {
        let entries = request(
            root,
            runner,
            &["git", "ls-files", "--stage", "-z"].map(str::to_owned),
            60,
        )?;
        for row in entries.split_terminator('\0') {
            if let Some((mode_blob_stage, path)) = row.split_once('\t') {
                if let Some(file) = files.get_mut(path) {
                    file.index = mode_blob_stage.into();
                }
            }
        }
    }
    Ok(files)
}

fn io_error(e: std::io::Error) -> Stop {
    condition(e.to_string())
}

pub(super) fn fingerprint(
    target: &Anchor,
    changes: &Changes,
    push_base: Option<&str>,
) -> Result<String, Stop> {
    let bytes =
        serde_json::to_vec(&(target, changes, push_base)).map_err(|e| condition(e.to_string()))?;
    Ok(bridgeforge_core::native_release::raw_sha(&bytes))
}

pub(super) fn selection(changes: &Changes, path: Option<&Path>) -> Result<BTreeSet<String>, Stop> {
    let Some(path) = path else {
        return Ok(changes.keys().cloned().collect());
    };
    let paths: Vec<String> = serde_json::from_slice(&fs::read(path).map_err(io_error)?)
        .map_err(|e| decision(format!("expected a JSON array of reviewed paths: {e}")))?;
    let mut selected = BTreeSet::new();
    for path in paths {
        if !changes.contains_key(&path) || !selected.insert(path.clone()) {
            return Err(conflict(format!(
                "path missing from review or duplicated: {path}"
            )));
        }
    }
    Ok(selected)
}

pub(super) struct Scratch(PathBuf);
impl Scratch {
    pub(super) fn new(root: &Path) -> Result<Self, Stop> {
        let base = bridgeforge_core::managed_paths::safe_join(
            root,
            ".build/native-release",
            "release build",
        )
        .map_err(condition)?;
        fs::create_dir_all(&base).map_err(io_error)?;
        let path = bridgeforge_core::persistence::temporary_sibling(&base.join("scope"), "build");
        fs::create_dir(&path).map_err(io_error)?;
        Ok(Self(path))
    }

    pub(super) fn tree(
        &self,
        root: &Path,
        runner: &dyn ProcessRunner,
        head: &str,
        paths: &BTreeSet<String>,
    ) -> Result<String, Stop> {
        let index = self
            .0
            .strip_prefix(root)
            .map_err(|e| condition(e.to_string()))?
            .join("index");
        git_input(root, runner, &["read-tree", head], &[], Some(&index))?;
        if !paths.is_empty() {
            git_input(
                root,
                runner,
                &["add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"],
                &path_input(paths.iter()),
                Some(&index),
            )?;
        }
        git_input(root, runner, &["write-tree"], &[], Some(&index))
    }

    pub(super) fn checkout(
        &self,
        root: &Path,
        runner: &dyn ProcessRunner,
        tree: &str,
    ) -> Result<PathBuf, Stop> {
        let dir = self.0.join("source");
        let dir_str = dir
            .strip_prefix(root)
            .map_err(|e| condition(e.to_string()))?
            .to_str()
            .ok_or_else(|| condition("non-UTF-8 build directory"))?;
        git(
            root,
            runner,
            &["clone", "--shared", "--no-checkout", "--", ".", dir_str],
        )?;
        git(&dir, runner, &["read-tree", "--reset", "-u", tree])?;
        let native = driver(&dir)?;
        if native.program == crate::project_release::PROGRAM {
            for path in [
                crate::project_release::PROGRAM,
                crate::project_release::RECEIPT,
            ] {
                let source =
                    bridgeforge_core::managed_paths::safe_join(root, path, "installed driver")
                        .map_err(condition)?;
                let dest =
                    bridgeforge_core::managed_paths::safe_join(&dir, path, "isolated driver")
                        .map_err(condition)?;
                fs::create_dir_all(dest.parent().unwrap()).map_err(io_error)?;
                fs::copy(source, dest).map_err(io_error)?;
            }
        }
        Ok(dir)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn path_input<'a>(paths: impl Iterator<Item = &'a String>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for path in paths {
        bytes.extend_from_slice(path.as_bytes());
        bytes.push(0);
    }
    bytes
}
fn git_input(
    root: &Path,
    runner: &dyn ProcessRunner,
    args: &[&str],
    input: &[u8],
    index: Option<&Path>,
) -> Result<String, Stop> {
    let mut q = ProcessRequest::new("git", root);
    q.args.push("--literal-pathspecs".into());
    q.args.extend(args.iter().map(Into::into));
    q.stdin = input.to_vec();
    if let Some(path) = index {
        q.env.insert("GIT_INDEX_FILE".into(), path.into());
    }
    let out = runner.run(&q).map_err(io_error)?;
    if out.code != 0 || out.timed_out {
        return Err(conflict(String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().into())
}

pub(super) fn commit(
    root: &Path,
    runner: &dyn ProcessRunner,
    changes: &Changes,
    paths: &BTreeSet<String>,
    message: &str,
) -> Result<(), Stop> {
    let new = path_input(
        paths
            .iter()
            .filter(|p| changes.get(*p).is_some_and(|s| s.status.contains("??"))),
    );
    if !new.is_empty() {
        git_input(
            root,
            runner,
            &["add", "-N", "--pathspec-from-file=-", "--pathspec-file-nul"],
            &new,
            None,
        )?;
    }
    git_input(
        root,
        runner,
        &[
            "commit",
            "--only",
            "-m",
            message,
            "--pathspec-from-file=-",
            "--pathspec-file-nul",
        ],
        &path_input(paths.iter()),
        None,
    )?;
    Ok(())
}

pub(super) fn align_index(
    root: &Path,
    runner: &dyn ProcessRunner,
    paths: &BTreeSet<String>,
) -> Result<(), Stop> {
    if !paths.is_empty() {
        git_input(
            root,
            runner,
            &[
                "reset",
                "-q",
                "HEAD",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ],
            &path_input(paths.iter()),
            None,
        )?;
    }
    Ok(())
}
