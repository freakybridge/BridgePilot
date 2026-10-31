use crate::{ProcessOutput, ProcessRequest, ProcessRunner};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};
struct Git<'a> {
    root: &'a Path,
    runner: &'a dyn ProcessRunner,
}

impl<'a> Git<'a> {
    fn run_raw(&self, args: &[&str], timeout: Duration) -> Result<(i32, String, String), String> {
        let mut request = ProcessRequest::new("git", self.root);
        request.args = args.iter().map(OsString::from).collect();
        request.timeout = timeout;
        let output = self
            .runner
            .run(&request)
            .map_err(|error| format!("cannot launch git: {error}"))?;
        if output.timed_out {
            return Err(format!("git {} timed out", args.join(" ")));
        }
        Ok((
            output.code,
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }

    fn required(&self, args: &[&str], timeout: Duration) -> Result<String, String> {
        let (code, stdout, stderr) = self.run_raw(args, timeout)?;
        if code == 0 {
            Ok(stdout)
        } else {
            Err(format!(
                "git {} failed: {}",
                args.join(" "),
                if stderr.is_empty() { stdout } else { stderr }
            ))
        }
    }
}

pub struct SyncLock {
    _guard: crate::file_lock::FileLock,
}

pub fn lock_factory_runtime(root: &Path, runner: &dyn ProcessRunner) -> Result<SyncLock, String> {
    SyncLock::acquire(&Git { root, runner })
}

impl SyncLock {
    fn acquire(git: &Git<'_>) -> Result<Self, String> {
        let raw = git.required(
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            Duration::from_secs(30),
        )?;
        if raw.is_empty() || !Path::new(&raw).is_absolute() {
            return Err("Git common directory is not an absolute path".into());
        }
        let path = PathBuf::from(raw).join("bridgeforge-git-sync.lock");
        let guard = crate::file_lock::FileLock::acquire(&path).map_err(|error| {
            format!(
                "another bridgeforge git-sync is already running or lock is unavailable: {error}"
            )
        })?;
        Ok(Self { _guard: guard })
    }
}

fn reject_external_filters(git: &Git<'_>) -> Result<(), String> {
    fn query(git: &Git<'_>, args: &[&str], stdin: Vec<u8>) -> Result<ProcessOutput, String> {
        let mut request = ProcessRequest::new("git", git.root);
        request.args = args.iter().map(Into::into).collect();
        request.stdin = stdin;
        request.timeout = Duration::from_secs(30);
        let output = git
            .runner
            .run(&request)
            .map_err(|error| error.to_string())?;
        if output.timed_out {
            return Err("preflight Git filter inspection timed out".into());
        }
        Ok(output)
    }
    let output = query(
        git,
        &[
            "config",
            "--null",
            "--name-only",
            "--get-regexp",
            r"^filter\..*\.(clean|process)$",
        ],
        Vec::new(),
    )?;
    if output.code != 0 && !(output.code == 1 && output.stdout.is_empty()) {
        return Err(format!(
            "preflight Git filter inspection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let keys = std::str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    let mut drivers = std::collections::BTreeSet::new();
    for key in keys.split('\0').filter(|s| !s.is_empty()) {
        let driver = key
            .strip_prefix("filter.")
            .and_then(|s| {
                s.strip_suffix(".clean")
                    .or_else(|| s.strip_suffix(".process"))
            })
            .ok_or("unmodelled Git filter key")?;
        drivers.insert(driver);
    }
    let tracked = query(git, &["ls-files", "--stage", "-z"], Vec::new())?;
    if tracked.code != 0 {
        return Err("cannot inspect tracked paths for Git filters".into());
    }
    if tracked.stdout.is_empty() {
        return Ok(());
    }
    let mut entries = tracked.stdout.split(|b| *b == 0).collect::<Vec<_>>();
    if entries.pop() != Some(&b""[..]) {
        return Err("invalid tracked path framing".into());
    }
    let mut paths = Vec::new();
    let mut expected = Vec::new();
    for entry in entries {
        if entry.starts_with(b"160000 ") {
            return Err("aggregate preflight unavailable: submodule helper isolation requires project-specific review".into());
        }
        let tab = entry
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("invalid tracked stage entry")?;
        let path = &entry[tab + 1..];
        if path.is_empty() {
            return Err("empty tracked path".into());
        }
        expected.push(path);
        paths.extend_from_slice(path);
        paths.push(0);
    }
    if drivers.is_empty() {
        return Ok(());
    }
    let attributes = query(git, &["check-attr", "-z", "--stdin", "filter"], paths)?;
    if attributes.code != 0 {
        return Err("cannot inspect active Git filter attributes".into());
    }
    let mut fields = attributes.stdout.split(|b| *b == 0).collect::<Vec<_>>();
    if fields.pop() != Some(&b""[..]) || fields.len() != expected.len() * 3 {
        return Err("invalid Git filter attribute framing".into());
    }
    for (item, path) in fields.chunks_exact(3).zip(expected) {
        if item[0] != path || item[1] != b"filter" {
            return Err("unexpected Git filter attribute".into());
        }
        let value = std::str::from_utf8(item[2]).map_err(|e| e.to_string())?;
        if drivers.contains(value) || value == "set" {
            return Err("aggregate preflight unavailable: active Git clean/process filters may execute project helpers; ordinary git-sync behavior is unchanged and requires project-specific review".into());
        }
    }
    Ok(())
}

pub fn verify_readonly_git(root: &Path, runner: &dyn ProcessRunner) -> Result<(), String> {
    reject_external_filters(&Git { root, runner })
}
