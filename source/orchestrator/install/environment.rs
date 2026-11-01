//! Explicit, bounded prerequisite preparation. No authentication or elevation.
use crate::Result;
use bridgeforge_core::{ProcessRequest, ProcessRunner, SystemProcessRunner, persistence};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

const TOOLS: [(&str, &str, (u64, u64)); 2] =
    [("git", "Git.Git", (2, 28)), ("gh", "GitHub.cli", (2, 0))];

fn version(text: &str, program: &str) -> Option<(u64, u64)> {
    let prefix = if program == "git" {
        "git version "
    } else {
        "gh version "
    };
    let raw = text.strip_prefix(prefix)?.split_whitespace().next()?;
    let mut parts = raw.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}
fn request(program: &str, cwd: &Path, args: &[&str], timeout: u64) -> ProcessRequest {
    let mut q = ProcessRequest::new(program, cwd);
    q.args = args.iter().map(Into::into).collect();
    q.timeout = Duration::from_secs(timeout);
    q
}

pub fn check_with(runner: &dyn ProcessRunner, cwd: &Path) -> Result<Value> {
    crate::store::ordinary(cwd)?;
    if !cwd.is_absolute() || !cwd.is_dir() {
        return Err("absolute existing context required".into());
    }
    let mut tools = Vec::new();
    for (name, package, minimum) in TOOLS {
        let output = runner.run(&request(name, cwd, &["--version"], 15));
        let (ready, observed) = match output {
            Ok(o) if o.code == 0 && !o.timed_out && o.stdout.len() < 16384 => {
                let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
                (
                    version(&text, name).is_some_and(|v| v >= minimum),
                    Some(text),
                )
            }
            _ => (false, None),
        };
        tools.push(json!({"program":name,"package":package,"ready":ready,"version":observed,"minimum":format!("{}.{}",minimum.0,minimum.1)}));
    }
    let ready = tools.iter().all(|t| t["ready"] == true);
    Ok(
        json!({"schema":1,"status":if ready{"environment_ready"}else{"environment_preparation_required"},"ready":ready,"tools":tools,"authentication_checked":false,"writes":false}),
    )
}
pub fn check(cwd: &Path) -> Result<Value> {
    check_with(&SystemProcessRunner, cwd)
}

/// The caller authorizes this specific scope. Missing user-scope installers do
/// not silently become machine installs. Reboots/UAC/login are never scripted.
pub fn prepare_with(
    runner: &dyn ProcessRunner,
    home: &Path,
    scope: &str,
    source: &str,
) -> Result<Value> {
    if !matches!(scope, "user" | "machine") || source.trim().is_empty() {
        return Err(
            "explicit user/machine installation scope and authorization source required".into(),
        );
    }
    crate::install::inspect(home)?;
    let before = check_with(runner, home)?;
    if before["ready"] == true {
        return Ok(before);
    }
    let folder = home.join(".runtime/bridgepilot-environment");
    crate::store::ordinary(&folder)?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let _lock = bridgeforge_core::file_lock::FileLock::acquire(&folder.join("writer.lock"))?;
    let receipt = folder.join("last-install.json");
    let mut prior_attempts = Vec::new();
    if receipt.exists() {
        let prior: Value = crate::store::read_json(&receipt)?;
        if prior["state"] == "attempted" {
            return Ok(
                json!({"schema":1,"status":"environment_install_unknown","ready":false,"previous":prior,"instructions":"Read actual installed versions and the original installer state. Do not resend an attempted installation or change scope to bypass it."}),
            );
        }
        prior_attempts = prior["attempts"].as_array().cloned().unwrap_or_default();
    }
    let mut attempts = Vec::new();
    // Read again after obtaining the installation lock.
    let current = check_with(runner, home)?;
    for tool in current["tools"]
        .as_array()
        .ok_or("invalid environment observation")?
    {
        if tool["ready"] == true {
            continue;
        }
        let id = tool["package"].as_str().ok_or("package identity missing")?;
        if prior_attempts
            .iter()
            .any(|a| a["package"] == id && a["exit"] == 0 && a["timed_out"] == false)
        {
            return Ok(
                json!({"schema":1,"status":"environment_path_refresh_required","ready":false,"package":id,"instructions":"This package already returned installation success. Refresh the process PATH and check its actual installation before another install; do not resend it."}),
            );
        }
        let q = request(
            "winget",
            home,
            &[
                "install",
                "--exact",
                "--id",
                id,
                "--source",
                "winget",
                "--scope",
                scope,
                "--silent",
                "--disable-interactivity",
                "--accept-source-agreements",
                "--accept-package-agreements",
            ],
            600,
        );
        persistence::atomic_write_json(&receipt,&json!({"schema":1,"state":"attempted","package":id,"scope":scope,"source":source,"attempts":attempts})).map_err(|e|e.to_string())?;
        let result = runner.run(&q);
        let (ok, unknown, record) = match result {
            Ok(o) => (
                o.code == 0 && !o.timed_out,
                o.timed_out,
                json!({"package":id,"scope":scope,"exit":o.code,"timed_out":o.timed_out,"stdout":String::from_utf8_lossy(&o.stdout),"stderr":String::from_utf8_lossy(&o.stderr)}),
            ),
            Err(e) => (
                false,
                e.kind() != std::io::ErrorKind::NotFound,
                json!({"package":id,"scope":scope,"error":e.to_string()}),
            ),
        };
        attempts.push(record);
        persistence::atomic_write_json(&receipt, &json!({"schema":1,"state":if unknown{"attempted"}else if ok{"completed"}else{"failed"},"source":source,"attempts":attempts})).map_err(|e|e.to_string())?;
        if !ok {
            return Ok(
                json!({"schema":1,"status":"environment_install_attention_required","ready":false,"attempts":attempts,"instructions":"Inspect the actual installer result. Do not retry unknown installation or elevate automatically. A missing user-scope package needs explicit machine-scope authorization; login remains a user action."}),
            );
        }
    }
    let after = check_with(runner, home)?;
    Ok(
        json!({"schema":1,"status":if after["ready"]==true{"environment_ready"}else{"environment_path_refresh_required"},"ready":after["ready"],"observation":after,"attempts":attempts,"instructions":"If PATH is not visible yet, refresh the current process from actual installed paths or reopen Codex and recheck. Do not reinstall a successfully installed package merely because this process has stale PATH."}),
    )
}
pub fn prepare(home: &Path, scope: &str, source: &str) -> Result<Value> {
    prepare_with(&SystemProcessRunner, home, scope, source)
}
