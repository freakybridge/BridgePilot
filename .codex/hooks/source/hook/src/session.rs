use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::StepResult;
use crate::util::{repo_root, run_command};

fn git(args: &[String], timeout: Duration) -> Option<std::process::Output> {
    let root = repo_root();
    let mut safe_args = vec![
        "--no-optional-locks".to_string(),
        "-c".to_string(),
        format!(
            "safe.directory={}",
            root.to_string_lossy().replace('\\', "/")
        ),
    ];
    safe_args.extend(args.iter().cloned());
    run_command("git", safe_args, &root, timeout).ok()
}

fn skeleton_version() -> String {
    "template-only-v1".into()
}

fn hooks_table(text: &str) -> Result<bool, String> {
    let header = Regex::new(r#"(?m)^\s*\[+\s*(?P<body>[^\]\r\n]+)\]+\s*(?:#.*)?$"#).unwrap();
    for line in text.lines() {
        if line.trim_start().starts_with('[') && !line.contains(']') {
            return Err("unclosed table header".into());
        }
    }
    for captures in header.captures_iter(text) {
        let body = captures
            .name("body")
            .unwrap()
            .as_str()
            .trim()
            .replace(['\'', '"', ' '], "");
        if body == "hooks" || body.starts_with("hooks.") {
            return Ok(true);
        }
    }
    let dotted = Regex::new(r"(?m)^\s*hooks(?:\.[A-Za-z0-9_-]+)+\s*=").unwrap();
    let inline = Regex::new(r"(?m)^\s*hooks\s*=\s*\{").unwrap();
    Ok(dotted.is_match(text) || inline.is_match(text))
}

pub fn config_health(strict: bool) -> StepResult {
    let root = repo_root();
    let mut failures: Vec<(&str, String)> = Vec::new();
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    // User-level configuration remains outside project skeleton retirement.
    for (label, path) in [("~/.codex/settings.json", home.join(".codex/settings.json"))] {
        if path.is_file()
            && fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .is_none()
        {
            failures.push((
                "settings-json-valid",
                format!("settings.json invalid JSON: {label}. FIX: repair the JSON syntax."),
            ));
        }
    }
    let mut source = Vec::new();
    let config = root.join(".codex/config.toml");
    if let Ok(text) = fs::read_to_string(config) {
        match hooks_table(&text) {
            Ok(true) => source.push(".codex/config.toml contains a hooks table".into()),
            Err(error) => source.push(format!(".codex/config.toml table header invalid ({error})")),
            _ => {}
        }
    }
    let hooks = root.join(".codex/hooks.json");
    if fs::read_to_string(hooks)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("hooks").cloned())
        .and_then(|value| value.as_object().cloned())
        .is_none()
    {
        source.push(".codex/hooks.json invalid JSON or has no hooks object".into());
    }
    if !source.is_empty() {
        failures.push(("single-hook-source", format!("Codex hook registration is not single-source: {}. FIX: merge project hooks into .codex/hooks.json and remove all other hook blocks.", source.join("; "))));
    }
    if failures.is_empty() {
        return StepResult::ok();
    }
    let mut stdout = format!(
        "[health-check] {} skeleton setting(s) need attention (check-only, nothing changed):\n",
        failures.len()
    );
    for (_, message) in &failures {
        stdout.push_str(&format!("  - {message}\n"));
    }
    let code = if strict
        && failures.iter().any(|(name, _)| {
            matches!(
                *name,
                "project-runtime" | "settings-json-valid" | "single-hook-source"
            )
        }) {
        2
    } else {
        0
    };
    StepResult {
        code,
        stdout,
        stderr: String::new(),
    }
}

pub struct GitState {
    pub branch: String,
    pub head: String,
    pub ahead: String,
    pub changes: String,
    pub known: bool,
}

pub fn parse_git_state(text: Option<&str>) -> GitState {
    let mut state = GitState {
        branch: "unknown".into(),
        head: "unknown".into(),
        ahead: "unknown".into(),
        changes: String::new(),
        known: false,
    };
    let Some(text) = text else {
        return state;
    };
    let mut changes = Vec::new();
    let mut upstream = false;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("# branch.head ") {
            state.branch = value.into();
        } else if let Some(value) = line.strip_prefix("# branch.oid ") {
            state.head = value.into();
        } else if line.starts_with("# branch.upstream ") {
            upstream = true;
        } else if let Some(value) = line.strip_prefix("# branch.ab ") {
            state.ahead = value.into();
        } else if !line.starts_with("# ") && !line.is_empty() {
            changes.push(line);
        }
    }
    state.known = !matches!(state.branch.as_str(), "" | "unknown")
        && !matches!(state.head.as_str(), "" | "unknown");
    if state.known && !upstream {
        state.ahead = "no-upstream".into();
    }
    state.changes = changes.join("\n");
    state
}

fn git_state() -> GitState {
    let output = git(
        &["status".into(), "--porcelain=v2".into(), "--branch".into()],
        Duration::from_secs(5),
    );
    let text = output
        .filter(|value| value.status.success())
        .map(|value| String::from_utf8_lossy(&value.stdout).to_string());
    parse_git_state(text.as_deref())
}

fn archive_count(root: &Path) -> Result<usize, String> {
    bridgeforge_checks::archive_scan::scan(root).map(|candidates| candidates.len())
}

pub fn show_state() -> StepResult {
    let root = repo_root();
    let state = git_state();
    let dirty = if state.known {
        state.changes.lines().count().to_string()
    } else {
        "unknown".into()
    };
    let mut lines = vec![format!(
        "[session-start] branch={} | dirty={dirty} | ahead/behind={} | skeleton=v{}",
        state.branch,
        state.ahead,
        skeleton_version()
    )];
    match archive_count(&root) {
        Ok(count) if count > 0 => {
            lines.push(format!(
                "[archive] delivery / bugs 有 {count} 个候选可归档 — 输入 $archive-scan 查看"
            ));
        }
        Err(error) => lines.push(format!(
            "[archive] scan failed; candidates unknown: {error}"
        )),
        _ => {}
    }
    StepResult::output(lines.join("\n"))
}

pub fn skill_sync() -> StepResult {
    let root = repo_root();
    let mut stale = Vec::new();
    for asset in bridgeforge_core::template_standard::assets() {
        if asset.rule.get("previous_sha256").is_none() {
            continue;
        }
        let Some(target) = asset.rule["target"].as_str() else { continue };
        let payload = bridgeforge_core::managed_paths::safe_join(&root, target, "project workflow")
            .and_then(|path| bridgeforge_core::managed_paths::read_metadata(&path, 32 * 1024 * 1024));
        if !matches!(payload, Ok(Some(bytes)) if asset.rule["current_sha256"] == bridgeforge_core::build_manifest::normalized_hash(&bytes)) {
            stale.push(target.to_owned());
        }
    }
    if stale.is_empty() {
        StepResult::ok()
    } else {
        StepResult::output(format!(
            "[skill-sync] 项目工作流缺失或内容漂移（{}）。请按项目 INSTALL.md 核对骨架。",
            stale.join("、")
        ))
    }
}

pub fn build_identity() -> Value {
    let mut hasher = Sha256::new();
    hasher.update(include_bytes!(env!("BRIDGEFORGE_WORKSPACE_LOCK")));
    json!({"schema":1,"name":"bridgeforge-hook","status":"ok","platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"lock_sha256":format!("{:x}",hasher.finalize()),
        "build_identity":bridgeforge_core::build_identity::identity()})
}
