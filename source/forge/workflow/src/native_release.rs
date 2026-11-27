//! Single-call native release coordination. Git synchronization is an intermediate result.
use crate::{CommandOutcome, ProcessRequest, ProcessRunner, git_sync};
use bridgeforge_core::native_release::Plan;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
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
    gitlab_reads: u64,
    gitlab_writes: u64,
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
            if program == "curl"
                && args
                    .windows(2)
                    .any(|p| p[0] == "--url" && p[1].contains("/api/v4/projects/"))
            {
                if args.windows(2).any(|p| p == ["--request", "GET"]) {
                    calls.gitlab_reads += 1;
                } else {
                    calls.gitlab_writes += 1;
                }
                calls.upload += u64::from(args.contains(&"--upload-file"));
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
#[path = "native_release_gitlab.rs"]
mod gitlab;
#[path = "native_release_platform.rs"]
mod platform;
#[path = "native_release_scope.rs"]
mod scope;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Driver {
    self_version: bool,
    build: Vec<String>,
    program: String,
    args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prepare_timeout_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Anchor {
    head: String,
    branch: String,
    remote: String,
    push_ref: String,
    url: String,
}

impl Anchor {
    fn same_target(&self, other: &Self) -> bool {
        self.branch == other.branch
            && self.remote == other.remote
            && self.push_ref == other.push_ref
            && self.url == other.url
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
    if table.iter().any(|(key, _)| {
        ![
            "build",
            "program",
            "args",
            "source-args",
            "version-source",
            "prepare-timeout-seconds",
        ]
        .contains(&key)
    }) {
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
        self_version: match table.get("version-source").map(|v| v.as_str()) {
            None => false,
            Some(Some("bp-product")) => true,
            _ => return Err(decision("unknown native release version-source")),
        },
        build: array("build")?,
        args: array("args")?,
        prepare_timeout_seconds: table
            .get("prepare-timeout-seconds")
            .map(|v| {
                v.as_integer()
                    .and_then(|n| u64::try_from(n).ok())
                    .filter(|n| (1..=21600).contains(n))
                    .ok_or_else(|| {
                        decision("native prepare timeout must be between 1 and 21600 seconds")
                    })
            })
            .transpose()?,
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
    let result = (|| -> Result<_, Stop> {
        let native = driver(root)?;
        require_idle_git(root, runner)?;
        let target = anchor(root, runner)?;
        let (_, version) =
            crate::project_release::product_version(root, native.self_version, false)
                .map_err(condition)?;
        let changes = scope::changes(root, runner)?;
        let base = git(root, runner, &["rev-parse", "--verify", "@{push}"]).ok();
        let review = scope::fingerprint(&target, &changes, base.as_deref())?;
        let range = base
            .as_ref()
            .map(|oid| format!("{oid}..HEAD"))
            .unwrap_or_else(|| "HEAD".into());
        let outgoing = git(root, runner, &["log", "--oneline", &range])?;
        Ok((target, version, changes, review, base, outgoing))
    })();
    match result {
        Ok((target, version, changes, review, base, outgoing)) => CommandOutcome::with_receipt(
            json!({"schema":1,"operation":"native-release","status":"ready",
            "release_requested":true,"version":version.to_string(),"branch":target.branch,"head":target.head,"push_remote":target.remote,"push_target":target.push_ref,
            "changed_paths":changes.keys().collect::<Vec<_>>(),"review_id":review,"cached_push_head":base,
            "outgoing_from_cached_push_ref":outgoing,"publication_verified":false}),
        ),
        Err(error) => stopped("preflight", error),
    }
}

/// Local-only installation evidence. No fetch, build, credential or hosting API call.
pub fn installation_target(
    root: &Path,
    repository: &str,
    gitlab_origin: Option<&str>,
) -> Result<Value, String> {
    let target = anchor(root, &crate::SystemProcessRunner).map_err(|e| e.message)?;
    platform::match_remote(&target.url, repository, gitlab_origin).map_err(|e| e.message)?;
    serde_json::to_value(target).map_err(|e| e.to_string())
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

fn anchor(root: &Path, runner: &dyn ProcessRunner) -> Result<Anchor, Stop> {
    let branch = git(root, runner, &["symbolic-ref", "--quiet", "HEAD"])?;
    let row = git(
        root,
        runner,
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(push:remotename)%00%(push)%00%(push:remoteref)%00%(upstream)%00%(upstream:remoteref)",
            &branch,
        ],
    )?;
    let fields: Vec<_> = row.split('\0').collect();
    if fields.len() != 6 || fields[..3].iter().any(|v| v.is_empty() || v.contains('\n')) {
        return Err(decision(
            "actual Git push target is not uniquely configured",
        ));
    }
    let url = git(
        root,
        runner,
        &["remote", "get-url", "--push", "--all", fields[1]],
    )?;
    if url.is_empty() || url.lines().count() != 1 {
        return Err(decision("Git push remote must have one destination URL"));
    }
    let destination = if !fields[3].is_empty() {
        fields[3]
    } else if fields[2] == fields[4] {
        fields[5]
    } else {
        ""
    };
    let push_ref = if destination.starts_with("refs/heads/") {
        destination.to_owned()
    } else {
        let specs = git(
            root,
            runner,
            &[
                "config",
                "--get-all",
                &format!("remote.{}.fetch", fields[1]),
            ],
        )?;
        let mut matches = specs
            .lines()
            .filter_map(|s| {
                let (src, dst) = s.trim_start_matches('+').split_once(':')?;
                if dst == fields[2] {
                    return Some(src.to_owned());
                }
                let (start, end) = dst.split_once('*')?;
                Some(src.replace('*', fields[2].strip_prefix(start)?.strip_suffix(end)?))
            })
            .filter(|s| s.starts_with("refs/heads/"));
        let branch = matches
            .next()
            .ok_or_else(|| decision("cannot resolve push branch"))?;
        if matches.any(|s| s != branch) {
            return Err(decision("ambiguous push branch"));
        }
        branch
    };
    Ok(Anchor {
        head: fields[0].into(),
        branch,
        remote: fields[1].into(),
        push_ref,
        url,
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

fn transient(message: &str) -> bool {
    let text = message.to_ascii_lowercase();
    [
        "timeout true",
        "timed out",
        "connection reset",
        "could not resolve host",
        "connection refused",
        "failed to connect",
        "unexpected eof",
        "http 502",
        "http 503",
        "http 504",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn retry_read<T>(mut read: impl FnMut() -> Result<T, Stop>) -> Result<T, Stop> {
    match read() {
        Err(e) if transient(&e.message) => read(),
        result => result,
    }
}

fn remote_head(
    root: &Path,
    runner: &dyn ProcessRunner,
    target: &Anchor,
) -> Result<Option<String>, Stop> {
    let row = retry_read(|| {
        git(
            root,
            runner,
            &["ls-remote", "--refs", "--", &target.url, &target.push_ref],
        )
    })?;
    if row.is_empty() {
        return Ok(None);
    }
    let (oid, name) = row
        .split_once('\t')
        .ok_or_else(|| conflict("invalid remote branch response"))?;
    if name != target.push_ref
        || !matches!(oid.len(), 40 | 64)
        || !oid.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(conflict("unexpected remote branch identity"));
    }
    Ok(Some(oid.into()))
}

fn align_branch(
    root: &Path,
    runner: &dyn ProcessRunner,
    target: &Anchor,
    remote: Option<&str>,
) -> Result<Anchor, Stop> {
    let Some(oid) = remote else {
        return Ok(target.clone());
    };
    if oid == target.head {
        return Ok(target.clone());
    }
    retry_read(|| {
        git(
            root,
            runner,
            &["fetch", "--no-tags", "--", &target.url, &target.push_ref],
        )
    })?;
    if git(root, runner, &["rev-parse", "FETCH_HEAD"])? != oid || anchor(root, runner)? != *target {
        return Err(conflict("branch moved during release fetch"));
    }
    let base = git(root, runner, &["merge-base", &target.head, oid])?;
    if base == oid {
        return Ok(target.clone());
    }
    if base != target.head {
        return Err(conflict("release branch diverged from its push target"));
    }
    git(root, runner, &["merge", "--ff-only", oid])?;
    let now = anchor(root, runner)?;
    if !target.same_target(&now) || now.head != oid {
        return Err(conflict("fast-forward target changed"));
    }
    Ok(now)
}

fn stopped(stage: &str, e: Stop) -> CommandOutcome {
    CommandOutcome {
        code: 2,
        stdout: String::new(),
        stderr: format!("[native-release] {stage}: {}\n", e.message),
        receipt: Some(
            json!({"schema":1,"operation":"native-release","status":"incomplete","release_requested":true,
            "stage":stage,"pause_category":e.category,"reason":e.message}),
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
            "coordinator subprocess requests; native-driver, compiler and hosting-client internal subprocesses excluded"
        );
        receipt["elapsed_ms"] = json!(started.elapsed().as_millis());
    }
    outcome
}

fn run_inner(
    root: &Path,
    runner: &dyn ProcessRunner,
    options: git_sync::GitSyncOptions,
) -> CommandOutcome {
    let mut stage = "preflight";
    let mut source_commit = None;
    let mut pushed = false;
    let result = (|| -> Result<Value, Stop> {
        require_idle_git(root, runner)?;
        let _lock = bridgeforge_core::file_lock::FileLock::acquire(
            &root.join(".runtime/bridgeforge-codex/native-release/operation.lock"),
        )
        .map_err(conflict)?;
        let target = anchor(root, runner)?;
        let reviewed = scope::changes(root, runner)?;
        let cached = git(root, runner, &["rev-parse", "--verify", "@{push}"]).ok();
        if let Some(id) = &options.review_id {
            if *id != scope::fingerprint(&target, &reviewed, cached.as_deref())? {
                return Err(conflict("reviewed release scope changed"));
            }
        }
        let mut paths = scope::selection(&reviewed, options.release_paths.as_deref())?;
        let remote = remote_head(root, runner, &target)?;
        let target = align_branch(root, runner, &target, remote.as_deref())?;
        if let Some(base) = cached.as_deref() {
            let endpoint = remote.as_deref().ok_or_else(|| {
                decision("previously tracked remote branch is absent; review branch destination")
            })?;
            if endpoint != target.head
                && endpoint != base
                && git(root, runner, &["merge-base", base, endpoint])? != base
            {
                return Err(conflict(
                    "remote history was replaced; refresh and review outgoing commits",
                ));
            }
        }
        if scope::changes(root, runner)? != reviewed {
            return Err(conflict("working changes differ after target check"));
        }
        let scratch = scope::Scratch::new(root)?;
        let initial_tree = scratch.tree(root, runner, &target.head, &paths)?;
        let isolated = if paths.len() < reviewed.len() {
            Some(scratch.checkout(root, runner, &initial_tree)?)
        } else {
            None
        };
        let build_root = isolated.as_deref().unwrap_or(root);
        let native = driver(build_root)?;
        stage = "product-version";
        let (version_path, version) =
            crate::project_release::product_version(build_root, native.self_version, false)
                .map_err(condition)?;
        let version = version.to_string();
        let file = build_root.join(&version_path);
        let before = fs::read_to_string(&file).map_err(|e| condition(e.to_string()))?;
        if scope::changes(root, runner)? != reviewed || anchor(root, runner)? != target {
            return Err(conflict(
                "reviewed source or push target changed before commit",
            ));
        }
        if before.trim() != version {
            if reviewed.contains_key(&version_path) && !paths.contains(&version_path) {
                return Err(conflict(
                    "version qualification overlaps an excluded change",
                ));
            }
            let path = root.join(&version_path);
            if fs::read(&path).map_err(|e| condition(e.to_string()))? != before.as_bytes() {
                return Err(conflict("release version changed concurrently"));
            }
            let qualified = before.replacen(before.trim(), &version, 1);
            bridgeforge_core::persistence::atomic_write(&path, qualified.as_bytes())
                .map_err(|e| condition(e.to_string()))?;
            paths.insert(version_path.clone());
        }
        stage = "source-commit";
        let tree = if before.trim() == version {
            initial_tree
        } else {
            scratch.tree(
                root,
                runner,
                &initial_tree,
                &std::collections::BTreeSet::from([version_path.clone()]),
            )?
        };
        let mut remaining = reviewed;
        remaining.retain(|path, _| !paths.contains(path));
        if git(root, runner, &["show", &format!("{tree}:{version_path}")])? != version {
            return Err(conflict(
                "staged product version changed during release preparation",
            ));
        }
        let creating = tree != git(root, runner, &["rev-parse", "HEAD^{tree}"])?;
        if creating {
            let message = match (options.message_file, options.message) {
                (Some(path), _) => {
                    fs::read_to_string(path).map_err(|e| condition(e.to_string()))?
                }
                (_, Some(message)) => message,
                _ => "Publish project version".into(),
            };
            let current = scope::changes(root, runner)?;
            scope::commit(root, runner, &current, &paths, &message)?;
        } else {
            scope::align_index(root, runner, &paths)?;
        }
        let commit = git(root, runner, &["rev-parse", "HEAD"])?;
        if (creating && git(root, runner, &["rev-parse", "HEAD^"])? != target.head)
            || (!creating && commit != target.head)
        {
            return Err(conflict("branch moved while committing release content"));
        }
        if git(root, runner, &["rev-parse", "HEAD^{tree}"])? != tree {
            return Err(conflict("committed content differs from the staged tree"));
        }
        source_commit = Some(commit.clone());
        if scope::changes(root, runner)? != remaining {
            return Err(conflict(
                "release files or excluded work changed during commit",
            ));
        }
        if isolated.is_some() {
            git(
                build_root,
                runner,
                &["checkout", "--detach", "--force", &commit],
            )?;
        }
        stage = "prepare";
        request(build_root, runner, &native.build, 1800)?;
        let output = build_root.join(".build/native-release").join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| condition(e.to_string()))?
                .as_nanos()
        ));
        let binary = bridgeforge_core::managed_paths::safe_join(
            build_root,
            &native.program,
            "release driver",
        )
        .map_err(condition)?;
        let mut argv = vec![binary.to_string_lossy().into_owned()];
        argv.extend(native.args);
        argv.extend([
            "--project-root".into(),
            build_root.to_string_lossy().into_owned(),
            "--output".into(),
            output.to_string_lossy().into_owned(),
        ]);
        let prepared = request(
            build_root,
            runner,
            &argv,
            native.prepare_timeout_seconds.unwrap_or(300),
        )?;
        let plan: Plan = serde_json::from_str(&prepared).map_err(|e| condition(e.to_string()))?;
        if plan.version != version {
            return Err(conflict(
                "package version differs from the committed product version",
            ));
        }
        if !bridgeforge_core::native_release::identifier(&plan.tag) {
            return Err(condition("invalid tag in project release output"));
        }
        platform::match_remote(&target.url, &plan.repository, plan.gitlab_origin.as_deref())?;
        stage = "source-push";
        let now = anchor(root, runner)?;
        if !target.same_target(&now)
            || now.head != commit
            || scope::changes(root, runner)? != remaining
            || (isolated.is_some() && !scope::changes(build_root, runner)?.is_empty())
        {
            return Err(conflict("source or push target changed before push"));
        }
        let dest = &target.push_ref;
        let pushed_head = remote_head(root, runner, &target)?;
        if pushed_head.as_deref() != Some(&commit) {
            if pushed_head != remote {
                return Err(conflict("remote branch moved during release preparation"));
            }
            if pushed_head.is_some() && plan.gitlab_origin.is_none() {
                github::check_existing(build_root, runner, &plan, &commit)?;
            }
            for attempt in 0..2 {
                if anchor(root, runner)? != now || scope::changes(root, runner)? != remaining {
                    return Err(conflict("release range or destination changed before push"));
                }
                let write = git(
                    root,
                    runner,
                    &[
                        "push",
                        "--no-follow-tags",
                        "--",
                        &target.remote,
                        &format!("{commit}:{dest}"),
                    ],
                );
                let observed = remote_head(root, runner, &target).map_err(|e| {
                    condition(format!(
                        "push outcome unknown; remote lookup failed: {}",
                        e.message
                    ))
                })?;
                if observed.as_deref() == Some(&commit) {
                    break;
                }
                if attempt == 0
                    && observed == pushed_head
                    && write.as_ref().is_err_and(|e| transient(&e.message))
                {
                    continue;
                }
                write?;
                return Err(conflict(
                    "remote branch does not contain the release commit at its tip",
                ));
            }
        }
        pushed = true;
        if anchor(root, runner)? != now {
            return Err(conflict("remote push commit or local branch differs"));
        }
        stage = "publication";
        let (url, verified) = platform::publish(build_root, runner, &plan, &commit)?;
        Ok(
            json!({"schema":1,"operation":"native-release","status":"released","publication_verified":verified,
            "version":plan.version,"tag":plan.tag,"commit":commit,"synchronization_commit":commit,
            "release_url":url,"source_push_verified":true}),
        )
    })();
    match result {
        Ok(receipt) => CommandOutcome::with_receipt(receipt),
        Err(error) => {
            let mut out = stopped(stage, error);
            if let Some(receipt) = &mut out.receipt {
                receipt["source_commit"] = json!(source_commit);
                receipt["source_push_verified"] = json!(pushed);
                receipt["publication_verified"] = json!(false);
            }
            out
        }
    }
}
