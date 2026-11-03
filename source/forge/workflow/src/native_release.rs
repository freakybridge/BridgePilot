//! Single-call native release coordination. Git synchronization is an intermediate result.
use crate::{CommandOutcome, ProcessRequest, ProcessRunner, git_sync, persistence};
use bridgeforge_core::native_release::{Plan, raw_sha};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path, time::Duration};

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

fn save(path: &Path, state: &State) -> Result<(), Stop> {
    persistence::atomic_write_json(
        path,
        &serde_json::to_value(state).map_err(|e| condition(e.to_string()))?,
    )
    .map_err(|e| condition(e.to_string()))
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
        let old = bridgeforge_core::managed_paths::read_metadata(&state_path, 4 * 1024 * 1024)
            .map_err(conflict)?
            .map(|bytes| {
                serde_json::from_slice::<State>(&bytes)
                    .map_err(|e| conflict(format!("release continuation invalid: {e}")))
            })
            .transpose()?;
        if let Some(old) = &old {
            if old.schema != 1 {
                return Err(conflict("unknown native release continuation schema"));
            }
            if old.status != "released" && old.fingerprint != input {
                return Err(conflict(
                    "unfinished release inputs changed; previous artifacts and attempt preserved",
                ));
            }
        }
        let resume = old.as_ref().is_some_and(|s| s.fingerprint == input);
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
            // Fetch may update the native entry too. Never execute the pre-fetch command against new source.
            let native = driver(root)?;
            stage = "prepare";
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
            let state = State {
                schema: 1,
                fingerprint: input.clone(),
                driver: native,
                plan,
                commit: None,
                status: "prepared".into(),
                result: None,
            };
            save(&state_path, &state)?;
            state
        };
        // Driver identity is part of source fingerprint, but retain explicit diagnostics for hand-edited state.
        if serde_json::to_value(&state.driver).unwrap()
            != serde_json::to_value(driver(root)?).unwrap()
        {
            return Err(conflict("native release entry differs from continuation"));
        }
        state.plan.validate(root).map_err(conflict)?;
        let branch = git(root, runner, &["symbolic-ref", "--quiet", "HEAD"])?;
        let remote = git(
            root,
            runner,
            &["for-each-ref", "--format=%(push:remotename)", &branch],
        )?;
        if remote.is_empty() || remote.contains('\n') {
            return Err(decision(
                "actual Git push remote is not uniquely configured",
            ));
        }
        let remote_url = git(
            root,
            runner,
            &["remote", "get-url", "--push", "--all", &remote],
        )?;
        github::match_remote(&remote_url, &state.plan.repository)?;
        if let Some(commit) = &state.commit {
            if git(root, runner, &["rev-parse", "HEAD"])? != *commit
                || !git(root, runner, &["status", "--porcelain"])?.is_empty()
            {
                return Err(conflict(
                    "fixed release commit or working tree changed; continuation preserved",
                ));
            }
        } else {
            stage = "source-push";
            if options.message.is_none() && options.message_file.is_none() {
                options.message = Some(format!(
                    "chore: 发布 {} {}",
                    state.plan.product, state.plan.version
                ));
            }
            options.skip_fetch = !resume;
            let synced = git_sync::sync(root, runner, options);
            if synced.code != 0 {
                return Err(condition(format!(
                    "source synchronization incomplete: {} {:?}",
                    synced.stderr, synced.receipt
                )));
            }
            if fingerprint(root, runner)? != input {
                return Err(conflict(
                    "release source changed during Git synchronization; publication stopped",
                ));
            }
            state.commit = Some(git(root, runner, &["rev-parse", "HEAD"])?);
            state.status = "pushed".into();
            save(&state_path, &state)?;
        }
        stage = "publication";
        if git(
            root,
            runner,
            &["remote", "get-url", "--push", "--all", &remote],
        )? != remote_url
        {
            return Err(conflict("Git publication remote changed during release"));
        }
        let result = github::publish(root, runner, &state.plan, state.commit.as_deref().unwrap())?;
        if fingerprint(root, runner)? != input {
            return Err(conflict(
                "source changed during publication; remote result must be inspected before continuing",
            ));
        }
        state.status = "released".into();
        state.result = Some(result.clone());
        save(&state_path, &state)?;
        let mut result = result;
        result["resumed"] = json!(resume);
        result["build_invocations"] = json!(usize::from(!resume));
        result["prepared_artifacts_reused"] = json!(resume);
        result["source_fingerprint"] = json!(input);
        result["continuation"] = json!(state_path);
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
            }
            out
        }
    }
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/native_release.rs"]
mod tests;
