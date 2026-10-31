//! Advisory, zero-write context for the agent before a managed synchronization.
use super::*;
use crate::ProcessOutput;

struct ReadOnly<'a>(&'a dyn ProcessRunner);

pub(super) fn reject_external_filters(git: &Git<'_>) -> Result<(), String> {
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

impl ProcessRunner for ReadOnly<'_> {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        if Path::new(&request.program)
            .file_stem()
            .and_then(|p| p.to_str())
            != Some("git")
        {
            return Err(std::io::Error::other(
                "non-Git subprocess is not supported by read-only preflight",
            ));
        }
        let mut request = request.clone();
        // A configured fsmonitor hook is executable project code, even for status.
        request.args.splice(
            0..0,
            [
                OsString::from("-c"),
                OsString::from("core.fsmonitor=false"),
                OsString::from("-c"),
                OsString::from("diff.autoRefreshIndex=false"),
            ],
        );
        // Even status must not refresh the index during a read-only inspection.
        request.env.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
        self.0.run(&request)
    }
}

pub(super) fn inspect(root: &Path, runner: &dyn ProcessRunner) -> CommandOutcome {
    let read_only = ReadOnly(runner);
    let mut setup = crate::release::setup::status_with(root, &read_only, false);
    if setup.code != 0
        || !matches!(
            setup.receipt.as_ref().and_then(|r| r["status"].as_str()),
            Some("configured" | "configuration-required")
        )
    {
        setup.code = crate::EXIT_BLOCKED;
        if setup.receipt.is_none() {
            setup.receipt =
                Some(json!({"schema":1,"status":"blocked","reason":setup.stderr.trim_end()}));
        }
        return setup;
    }
    let git = Git {
        root,
        runner: &read_only,
    };
    let result = (|| -> Result<serde_json::Value, String> {
        reject_external_filters(&git)?;
        let branch = git.required(
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
            Duration::from_secs(30),
        )?;
        let head = git.required(&["rev-parse", "HEAD"], Duration::from_secs(30))?;
        let upstream = git.required(
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
            Duration::from_secs(30),
        )?;
        let push_target = git.required(
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{push}",
            ],
            Duration::from_secs(30),
        )?;
        let status = git.status()?;
        let dirty = !status.is_empty();
        let (unstaged, staged) = if dirty {
            (
                git.required(
                    &["diff", "--no-ext-diff", "--no-textconv", "--stat"],
                    Duration::from_secs(45),
                )?,
                git.required(
                    &[
                        "diff",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--cached",
                        "--stat",
                    ],
                    Duration::from_secs(45),
                )?,
            )
        } else {
            (String::new(), String::new())
        };
        Ok(json!({
            "schema":1,"status":"ready","setup":setup.receipt,
            "branch":branch,"head":head,"upstream":upstream,"push_target":push_target,
            "working_tree":if dirty {"dirty"} else {"clean"},
            "status_porcelain":status,"diff_stat":{"unstaged":unstaged,"staged":staged},
            "diff_review_required":dirty,"remote_state":"not_fetched",
            "version":fs::read_to_string(root.join("VERSION")).ok().map(|s|s.trim().to_string())
        }))
    })();
    match result {
        Ok(receipt) => CommandOutcome::with_receipt(receipt),
        Err(error) => CommandOutcome {
            code: crate::EXIT_BLOCKED,
            stderr: format!("[git-sync-preflight] {error}\n"),
            receipt: Some(json!({"schema":1,"status":"blocked","reason":error})),
            ..CommandOutcome::default()
        },
    }
}
