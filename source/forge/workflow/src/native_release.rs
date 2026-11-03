//! Single-call native release coordination. Git synchronization is an intermediate result.
use crate::{CommandOutcome, ProcessRequest, ProcessRunner, git_sync, persistence};
use bridgeforge_core::native_release::{Plan, raw_sha};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Default, Serialize)]
struct Calls {
    processes: u64,
    git: u64,
    fetch: u64,
    push: u64,
    cargo: u64,
    github_reads: u64,
    github_writes: u64,
    upload: u64,
    timeouts: u64,
}
struct Measured<'a> {
    inner: &'a dyn ProcessRunner,
    calls: std::cell::RefCell<Calls>,
}
impl ProcessRunner for Measured<'_> {
    fn run(&self, q: &ProcessRequest) -> std::io::Result<crate::ProcessOutput> {
        let program = Path::new(&q.program)
            .file_stem()
            .and_then(|p| p.to_str())
            .unwrap_or("");
        let args: Vec<_> = q.args.iter().filter_map(|s| s.to_str()).collect();
        {
            let mut calls = self.calls.borrow_mut();
            calls.processes += 1;
            if program == "git" {
                calls.git += 1;
                calls.fetch += u64::from(args.first() == Some(&"fetch"));
                calls.push += u64::from(args.first() == Some(&"push"));
            }
            if program == "cargo" {
                calls.cargo += 1;
            }
            if program == "gh" {
                if args.first() == Some(&"api") && args.windows(2).any(|p| p == ["--method", "GET"])
                {
                    calls.github_reads += 1;
                } else {
                    calls.github_writes += 1;
                }
                calls.upload += u64::from(args.starts_with(&["release", "upload"]));
            }
        }
        let out = self.inner.run(q);
        if out.as_ref().is_ok_and(|v| v.timed_out) {
            self.calls.borrow_mut().timeouts += 1;
        }
        out
    }
}

#[path = "native_release_github.rs"]
mod github;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Driver {
    build: Vec<String>,
    program: String,
    args: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema: u32,
    fingerprint: String,
    driver: Driver,
    plan: Plan,
    commit: Option<String>,
    status: String,
    result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    anchor: Option<Anchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_attempt: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Anchor {
    head: String,
    branch: String,
    remote: String,
    push_ref: String,
    url: String,
    config_sha256: String,
}

impl Anchor {
    fn same_target(&self, other: &Self) -> bool {
        self.branch == other.branch
            && self.remote == other.remote
            && self.push_ref == other.push_ref
            && self.url == other.url
            && self.config_sha256 == other.config_sha256
    }
}

#[derive(Debug)]
struct Stop {
    category: &'static str,
    message: String,
}
fn condition(message: impl Into<String>) -> Stop {
    Stop {
        category: "required_condition_failed",
        message: message.into(),
    }
}
fn conflict(message: impl Into<String>) -> Stop {
    Stop {
        category: "conflict",
        message: message.into(),
    }
}
fn decision(message: impl Into<String>) -> Stop {
    Stop {
        category: "decision_required",
        message: message.into(),
    }
}

fn driver(root: &Path) -> Result<Driver, Stop> {
    let text = fs::read_to_string(root.join("Cargo.toml")).map_err(|_| decision("native release entry is missing from Cargo.toml; ordinary sync will not be substituted"))?;
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| decision(e.to_string()))?;
    let table = doc
        .get("workspace")
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("bridgeforge"))
        .and_then(|v| v.get("native-release"))
        .and_then(toml_edit::Item::as_table)
        .ok_or_else(|| {
            decision("project has no native release entry; no Git writes or publication performed")
        })?;
    if table
        .iter()
        .any(|(key, _)| !["build", "program", "args"].contains(&key))
    {
        return Err(decision("unknown native release entry field"));
    }
    let array = |key: &str| -> Result<Vec<String>, Stop> {
        table
            .get(key)
            .and_then(toml_edit::Item::as_array)
            .ok_or_else(|| decision(format!("native release {key} array missing")))?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| decision("native release argv must contain strings"))
            })
            .collect()
    };
    let d = Driver {
        build: array("build")?,
        args: array("args")?,
        program: table
            .get("program")
            .and_then(toml_edit::Item::as_str)
            .ok_or_else(|| decision("native release program missing"))?
            .into(),
    };
    if d.build.is_empty() || !d.program.starts_with(".build/") {
        return Err(decision(
            "native release requires an explicit build command and .build program",
        ));
    }
    bridgeforge_core::managed_paths::safe_join(root, &d.program, "native release driver")
        .map_err(decision)?;
    Ok(d)
}

pub fn preflight(root: &Path, runner: &dyn ProcessRunner) -> CommandOutcome {
    match driver(root) {
        Err(e) => stopped("preflight", e),
        Ok(_) => {
            let mut out = git_sync::preflight(root, runner);
            if let Some(receipt) = out.receipt.as_mut() {
                receipt["operation"] = json!("native-release");
                receipt["release_requested"] = json!(true);
                receipt["publication_status"] = json!("not_verified");
            }
            out
        }
    }
}

fn request(
    root: &Path,
    runner: &dyn ProcessRunner,
    argv: &[String],
    timeout: u64,
) -> Result<String, Stop> {
    let mut req = ProcessRequest::new(&argv[0], root);
    req.args = argv[1..].iter().map(Into::into).collect();
    req.timeout = Duration::from_secs(timeout);
    let out = runner.run(&req).map_err(|e| condition(e.to_string()))?;
    if out.timed_out || out.code != 0 {
        return Err(condition(format!(
            "{} failed (exit {}, timeout {}): {}{}",
            argv[0],
            out.code,
            out.timed_out,
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        )));
    }
    String::from_utf8(out.stdout).map_err(|e| condition(e.to_string()))
}

fn git(root: &Path, runner: &dyn ProcessRunner, args: &[&str]) -> Result<String, Stop> {
    let mut argv = vec!["git".into()];
    argv.extend(args.iter().map(|s| (*s).into()));
    request(root, runner, &argv, 60).map(|s| s.trim().to_owned())
}

/// Includes all tracked and untracked non-ignored inputs. Build/runtime output is ignored by project policy.
fn fingerprint(root: &Path, runner: &dyn ProcessRunner) -> Result<String, Stop> {
    use sha2::{Digest, Sha256};
    let files = request(
        root,
        runner,
        &[
            "git",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>(),
        60,
    )?;
    let paths: BTreeSet<_> = files.split('\0').filter(|s| !s.is_empty()).collect();
    let mut hash = Sha256::new();
    for name in paths {
        let path = bridgeforge_core::managed_paths::safe_join(root, name, "release source")
            .map_err(conflict)?;
        let Some(bytes) = bridgeforge_core::managed_paths::read_metadata(&path, 128 * 1024 * 1024)
            .map_err(conflict)?
        else {
            continue;
        };
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn state_bytes(path: &Path) -> Result<Option<Vec<u8>>, Stop> {
    bridgeforge_core::managed_paths::read_metadata(path, 4 * 1024 * 1024).map_err(conflict)
}

fn unchanged_state(path: &Path, expected: &Option<Vec<u8>>) -> Result<(), Stop> {
    if &state_bytes(path)? != expected {
        return Err(conflict(
            "release continuation changed concurrently; external state preserved",
        ));
    }
    Ok(())
}

fn save(path: &Path, state: &State, expected: &mut Option<Vec<u8>>) -> Result<(), Stop> {
    unchanged_state(path, expected)?;
    let mut bytes = serde_json::to_vec_pretty(state).map_err(|e| condition(e.to_string()))?;
    bytes.push(b'\n');
    persistence::atomic_write(path, &bytes).map_err(|e| condition(e.to_string()))?;
    if state_bytes(path)?.as_ref() != Some(&bytes) {
        return Err(conflict("release continuation changed during save"));
    }
    *expected = Some(bytes);
    Ok(())
}

fn anchor(root: &Path, runner: &dyn ProcessRunner) -> Result<Anchor, Stop> {
    let branch = git(root, runner, &["symbolic-ref", "--quiet", "HEAD"])?;
    let row = git(
        root,
        runner,
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(push:remotename)%00%(push)",
            &branch,
        ],
    )?;
    let fields: Vec<_> = row.split('\0').collect();
    if fields.len() != 3 || fields.iter().any(|v| v.is_empty() || v.contains('\n')) {
        return Err(decision(
            "actual Git push target is not uniquely configured",
        ));
    }
    let url = git(
        root,
        runner,
        &["remote", "get-url", "--push", "--all", fields[1]],
    )?;
    let config = git(root, runner, &["config", "--null", "--list"])?;
    Ok(Anchor {
        head: fields[0].into(),
        branch,
        remote: fields[1].into(),
        push_ref: fields[2].into(),
        url,
        config_sha256: raw_sha(config.as_bytes()),
    })
}

fn require_idle_git(root: &Path, runner: &dyn ProcessRunner) -> Result<(), Stop> {
    let markers = [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-apply",
        "rebase-merge",
        "sequencer",
        "index.lock",
        "HEAD.lock",
        "config.lock",
    ];
    let mut args = vec!["rev-parse"];
    for marker in &markers {
        args.extend(["--git-path", marker]);
    }
    let paths = git(root, runner, &args)?;
    if paths.lines().count() != markers.len() {
        return Err(conflict("Git operation paths could not be inspected"));
    }
    for path in paths.lines() {
        let path = PathBuf::from(path);
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        match fs::symlink_metadata(path) {
            Ok(_) => {
                return Err(conflict(
                    "unfinished Git operation; recovery scene preserved",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(conflict(e.to_string())),
        }
    }
    if !git(root, runner, &["ls-files", "--unmerged", "-z"])?.is_empty() {
        return Err(conflict("unmerged Git index; recovery scene preserved"));
    }
    Ok(())
}

fn archive_attempt(
    root: &Path,
    bytes: &[u8],
    guard: Option<&git_sync::CommitGuard>,
) -> Result<String, Stop> {
    let hash = raw_sha(bytes);
    let relative = format!(
        ".runtime/bridgeforge-codex/native-release/attempts/{}/state.json",
        &hash[7..]
    );
    let path =
        bridgeforge_core::managed_paths::safe_join(root, &relative, "release attempt archive")
            .map_err(conflict)?;
    persistence::ensure_real_directory(path.parent().unwrap(), true)
        .map_err(|e| conflict(e.to_string()))?;
    let preserve = |path: &Path, bytes: &[u8]| -> Result<(), Stop> {
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
        {
            Ok(mut file) => {
                file.write_all(bytes)
                    .map_err(|e| condition(e.to_string()))?;
                file.sync_all().map_err(|e| condition(e.to_string()))?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(conflict(e.to_string())),
        }
        if bridgeforge_core::managed_paths::read_metadata(path, 4 * 1024 * 1024)
            .map_err(conflict)?
            .as_deref()
            != Some(bytes)
        {
            return Err(conflict(
                "release attempt archive differs; archive and current state preserved",
            ));
        }
        Ok(())
    };
    preserve(&path, bytes)?;
    if let Some(guard) = guard {
        let name = format!("commit-guard-{}.json", &raw_sha(&guard.bytes)[7..]);
        preserve(&path.with_file_name(name), &guard.bytes)?;
    }
    Ok(relative)
}

fn require_repreparable(
    root: &Path,
    runner: &dyn ProcessRunner,
    old: &State,
    current: &Anchor,
) -> Result<(), Stop> {
    if old.commit.is_some()
        || !["prepared", "source-syncing", "blocked-before-commit"].contains(&old.status.as_str())
    {
        return Err(conflict(
            "unfinished release inputs changed after a fixed source commit; previous attempt preserved",
        ));
    }
    require_idle_git(root, runner)?;
    let guard = git_sync::inspect_commit_guard(root, runner).map_err(conflict)?;
    match &old.anchor {
        Some(before) if before == current => {}
        Some(_) => {
            return Err(conflict(
                "release Git checkpoint or push target changed; previous attempt preserved",
            ));
        }
        None if old.status == "prepared"
            && guard
                .as_ref()
                .is_some_and(|g| g.before_commit && g.parent == current.head) => {}
        None => {
            return Err(conflict(
                "legacy prepared release lacks verified before-commit evidence; previous attempt preserved",
            ));
        }
    }
    if guard.as_ref().is_some_and(|g| !g.before_commit) {
        return Err(conflict(
            "reviewed source commit already exists; reprepare refused",
        ));
    }
    github::match_remote(&current.url, &old.plan.repository)
}

fn verified_source_commit(
    root: &Path,
    runner: &dyn ProcessRunner,
    before: &Anchor,
    current: &Anchor,
    input: &str,
) -> Result<bool, Stop> {
    if !before.same_target(current)
        || !git(root, runner, &["status", "--porcelain"])?.is_empty()
        || fingerprint(root, runner)? != input
    {
        return Ok(false);
    }
    let parents = git(root, runner, &["rev-list", "--parents", "-n", "1", "HEAD"])?;
    let parents: Vec<_> = parents.split_whitespace().collect();
    if parents != [current.head.as_str(), before.head.as_str()] {
        return Ok(false);
    }
    if let Some(guard) = git_sync::inspect_commit_guard(root, runner).map_err(conflict)? {
        if guard.before_commit
            || guard.parent != before.head
            || git(root, runner, &["rev-parse", "HEAD^{tree}"])? != guard.tree
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn record_source_progress(
    root: &Path,
    runner: &dyn ProcessRunner,
    state: &mut State,
    input: &str,
) -> Result<(), Stop> {
    let current = anchor(root, runner)?;
    let before = state
        .anchor
        .as_ref()
        .ok_or_else(|| conflict("release source checkpoint missing"))?;
    if let Some(commit) = &state.commit {
        state.status = if commit == &current.head
            && before.same_target(&current)
            && git(root, runner, &["status", "--porcelain"])?.is_empty()
            && fingerprint(root, runner)? == input
        {
            "source-committed"
        } else {
            "source-uncertain"
        }
        .into();
        return Ok(());
    }
    if &current == before && fingerprint(root, runner)? == input {
        state.status = "blocked-before-commit".into();
    } else if verified_source_commit(root, runner, before, &current, input)? {
        state.commit = Some(current.head);
        state.status = "source-committed".into();
    } else {
        state.status = "source-uncertain".into();
    }
    Ok(())
}

fn stopped(stage: &str, e: Stop) -> CommandOutcome {
    CommandOutcome {
        code: 2,
        stdout: String::new(),
        stderr: format!("[native-release] {stage}: {}\n", e.message),
        receipt: Some(
            json!({"schema":1,"operation":"native-release","status":"incomplete","release_requested":true,
            "stage":stage,"pause_category":e.category,"reason":e.message,"resume":"git-sync release"}),
        ),
    }
}

pub fn run(
    root: &Path,
    runner: &dyn ProcessRunner,
    options: git_sync::GitSyncOptions,
) -> CommandOutcome {
    let measured = Measured {
        inner: runner,
        calls: std::cell::RefCell::new(Calls::default()),
    };
    let started = std::time::Instant::now();
    let mut outcome = run_inner(root, &measured, options);
    if let Some(receipt) = outcome.receipt.as_mut() {
        receipt["execution_counts"] = serde_json::to_value(&*measured.calls.borrow()).unwrap();
        receipt["count_scope"] = json!(
            "coordinator subprocess requests; compiler and gh internal subprocesses excluded"
        );
        receipt["elapsed_ms"] = json!(started.elapsed().as_millis());
    }
    outcome
}

fn run_inner(
    root: &Path,
    runner: &dyn ProcessRunner,
    mut options: git_sync::GitSyncOptions,
) -> CommandOutcome {
    let mut stage = "preflight";
    let mut archived_attempt = None;
    let result = (|| -> Result<Value, Stop> {
        driver(root)?;
        if options.skip_fetch || options.skip_push {
            return Err(decision("release cannot skip source integration or push"));
        }
        let preflight = git_sync::preflight(root, runner);
        if preflight.code != 0 {
            return Err(condition(preflight.stderr));
        }
        let _lock = bridgeforge_core::file_lock::FileLock::acquire(
            &root.join(".runtime/bridgeforge-codex/native-release/operation.lock"),
        )
        .map_err(conflict)?;
        let state_path = root.join(".runtime/bridgeforge-codex/native-release/current.json");
        let mut input = fingerprint(root, runner)?;
        let mut expected = state_bytes(&state_path)?;
        let old = expected
            .as_deref()
            .map(|bytes| {
                serde_json::from_slice::<State>(bytes)
                    .map_err(|e| conflict(format!("release continuation invalid: {e}")))
            })
            .transpose()?;
        if let Some(old) = &old {
            if old.schema != 1 {
                return Err(conflict("unknown native release continuation schema"));
            }
            if ![
                "prepared",
                "source-syncing",
                "blocked-before-commit",
                "source-committed",
                "source-uncertain",
                "pushed",
                "released",
            ]
            .contains(&old.status.as_str())
            {
                return Err(conflict("unknown native release continuation phase"));
            }
            if ["source-committed", "pushed", "released"].contains(&old.status.as_str())
                && old.commit.is_none()
            {
                return Err(conflict("release phase lacks its fixed source commit"));
            }
        }
        let observed = anchor(root, runner)?;
        require_idle_git(root, runner)?;
        if old.as_ref().is_some_and(|s| s.status == "source-uncertain") {
            return Err(conflict(
                "source synchronization outcome is uncertain; inspect Git progress before continuing",
            ));
        }
        let recovering = old
            .as_ref()
            .is_some_and(|s| s.status != "released" && s.fingerprint != input);
        if recovering {
            require_repreparable(root, runner, old.as_ref().unwrap(), &observed)?;
        }
        let resume = old.as_ref().is_some_and(|s| s.fingerprint == input);
        if !resume && let Some(bytes) = &expected {
            unchanged_state(&state_path, &expected)?;
            let guard = git_sync::inspect_commit_guard(root, runner).map_err(conflict)?;
            archived_attempt = Some(archive_attempt(root, bytes, guard.as_ref())?);
            unchanged_state(&state_path, &expected)?;
        }
        let mut state = if resume {
            old.unwrap()
        } else {
            stage = "source-integration";
            let integrated = git_sync::sync_remote(root, runner, options.clone());
            if integrated.code != 0 {
                return Err(condition(format!(
                    "source integration incomplete: {} {:?}",
                    integrated.stderr, integrated.receipt
                )));
            }
            input = fingerprint(root, runner)?;
            unchanged_state(&state_path, &expected)?;
            let prepared_from = anchor(root, runner)?;
            // Fetch may update the native entry too. Never execute the pre-fetch command against new source.
            let native = driver(root)?;
            stage = if recovering { "reprepare" } else { "prepare" };
            request(root, runner, &native.build, 1800)?;
            let output = format!(
                ".build/native-release/{}-{}",
                &input[..16],
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| condition(e.to_string()))?
                    .as_nanos()
            );
            let binary =
                bridgeforge_core::managed_paths::safe_join(root, &native.program, "release driver")
                    .map_err(conflict)?;
            let mut argv = vec![binary.to_string_lossy().into_owned()];
            argv.extend(native.args.clone());
            argv.extend([
                "--project-root".into(),
                root.to_string_lossy().into_owned(),
                "--output".into(),
                root.join(output).to_string_lossy().into_owned(),
            ]);
            let prepared = request(root, runner, &argv, 300)?;
            let plan: Plan = serde_json::from_str(&prepared)
                .map_err(|e| condition(format!("native release plan invalid: {e}")))?;
            plan.validate(root).map_err(condition)?;
            if fingerprint(root, runner)? != input {
                return Err(conflict(
                    "release source changed during preparation; prepared artifacts preserved",
                ));
            }
            if anchor(root, runner)? != prepared_from {
                return Err(conflict(
                    "Git checkpoint changed during preparation; previous attempt preserved",
                ));
            }
            let state = State {
                schema: 1,
                fingerprint: input.clone(),
                driver: native,
                plan,
                commit: None,
                status: "prepared".into(),
                result: None,
                anchor: Some(prepared_from),
                previous_attempt: archived_attempt.clone(),
            };
            save(&state_path, &state, &mut expected)?;
            state
        };
        // Driver identity is part of source fingerprint, but retain explicit diagnostics for hand-edited state.
        if serde_json::to_value(&state.driver).unwrap()
            != serde_json::to_value(driver(root)?).unwrap()
        {
            return Err(conflict("native release entry differs from continuation"));
        }
        state.plan.validate(root).map_err(conflict)?;
        let current = anchor(root, runner)?;
        if state
            .anchor
            .as_ref()
            .is_some_and(|before| !before.same_target(&current))
        {
            return Err(conflict(
                "release branch, push target or Git configuration changed",
            ));
        }
        github::match_remote(&current.url, &state.plan.repository)?;
        if let Some(commit) = &state.commit {
            if current.head != *commit || !git(root, runner, &["status", "--porcelain"])?.is_empty()
            {
                return Err(conflict(
                    "fixed release commit or working tree changed; continuation preserved",
                ));
            }
        } else if let Some(before) = &state.anchor {
            if before.head != current.head {
                if !verified_source_commit(root, runner, before, &current, &input)? {
                    return Err(conflict(
                        "source synchronization outcome is uncertain; previous attempt preserved",
                    ));
                }
                state.commit = Some(current.head.clone());
                state.status = "source-committed".into();
                save(&state_path, &state, &mut expected)?;
            }
        } else {
            // A legacy clean snapshot can only continue as a fixed source commit.
            // A dirty legacy snapshot needs the owning commit guard as evidence.
            let guard = git_sync::inspect_commit_guard(root, runner).map_err(conflict)?;
            if git(root, runner, &["status", "--porcelain"])?.is_empty() {
                state.commit = Some(current.head.clone());
                state.status = "source-committed".into();
            } else if !guard
                .as_ref()
                .is_some_and(|g| g.before_commit && g.parent == current.head)
            {
                return Err(conflict(
                    "legacy release lacks verified source progress; previous attempt preserved",
                ));
            }
            state.anchor = Some(current.clone());
            save(&state_path, &state, &mut expected)?;
        }
        if !["pushed", "released"].contains(&state.status.as_str()) {
            stage = "source-push";
            require_idle_git(root, runner)?;
            if options.message.is_none() && options.message_file.is_none() {
                options.message = Some(format!(
                    "chore: 发布 {} {}",
                    state.plan.product, state.plan.version
                ));
            }
            state.status = "source-syncing".into();
            save(&state_path, &state, &mut expected)?;
            options.skip_fetch = !resume;
            let synced = git_sync::sync(root, runner, options);
            if synced.code != 0 {
                record_source_progress(root, runner, &mut state, &input)?;
                save(&state_path, &state, &mut expected)?;
                return Err(condition(format!(
                    "source synchronization incomplete: {} {:?}",
                    synced.stderr, synced.receipt
                )));
            }
            let actual_commit = git(root, runner, &["rev-parse", "HEAD"])?;
            if state
                .commit
                .as_ref()
                .is_some_and(|fixed| fixed != &actual_commit)
            {
                state.status = "source-uncertain".into();
                save(&state_path, &state, &mut expected)?;
                return Err(conflict(
                    "fixed source commit changed during push recovery; publication stopped",
                ));
            }
            if fingerprint(root, runner)? != input {
                return Err(conflict(
                    "release source changed during Git synchronization; publication stopped",
                ));
            }
            let proof = synced
                .receipt
                .as_ref()
                .ok_or_else(|| conflict("source synchronization proof missing"))?;
            if proof["status"] != "synced"
                || proof["working_tree"] != "clean"
                || proof["ahead"] != 0
                || proof["behind"] != 0
            {
                return Err(conflict(
                    "source push parity was not verified; publication stopped",
                ));
            }
            state.commit = Some(actual_commit);
            if proof["commit"].as_str() != state.commit.as_deref() {
                return Err(conflict(
                    "source push commit differs from synchronization proof",
                ));
            }
            state.status = "pushed".into();
            save(&state_path, &state, &mut expected)?;
        }
        stage = "publication";
        let publishing = anchor(root, runner)?;
        if !current.same_target(&publishing)
            || Some(publishing.head.as_str()) != state.commit.as_deref()
        {
            return Err(conflict("Git publication remote changed during release"));
        }
        unchanged_state(&state_path, &expected)?;
        let result = github::publish(root, runner, &state.plan, state.commit.as_deref().unwrap())?;
        if fingerprint(root, runner)? != input {
            return Err(conflict(
                "source changed during publication; remote result must be inspected before continuing",
            ));
        }
        state.status = "released".into();
        state.result = Some(result.clone());
        save(&state_path, &state, &mut expected)?;
        let mut result = result;
        result["resumed"] = json!(resume);
        result["build_invocations"] = json!(usize::from(!resume));
        result["prepared_artifacts_reused"] = json!(resume);
        result["source_fingerprint"] = json!(input);
        result["continuation"] = json!(state_path);
        result["recovery_action"] = json!(if recovering {
            "reprepared"
        } else if resume {
            "reused"
        } else {
            "prepared"
        });
        result["previous_attempt"] = json!(state.previous_attempt);
        Ok(result)
    })();
    match result {
        Ok(value) => CommandOutcome::with_receipt(value),
        Err(e) => {
            let mut out = stopped(stage, e);
            if let Ok(Some(bytes)) = bridgeforge_core::managed_paths::read_metadata(
                &root.join(".runtime/bridgeforge-codex/native-release/current.json"),
                4 * 1024 * 1024,
            ) && let Ok(state) = serde_json::from_slice::<State>(&bytes)
                && let Some(receipt) = out.receipt.as_mut()
            {
                receipt["last_completed_stage"] = json!(state.status);
                receipt["source_commit"] = json!(state.commit);
                receipt["version"] = json!(state.plan.version);
                receipt["tag"] = json!(state.plan.tag);
                receipt["previous_attempt"] = json!(archived_attempt.or(state.previous_attempt));
            }
            out
        }
    }
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/native_release.rs"]
mod tests;
