//! BP-owned version selection. Returns edits; the Forge coordinator applies them transactionally.
use crate::Result;
use bridgeforge_core::{SemVer, ProcessRequest, ProcessRunner, SystemProcessRunner};
use bridgeforge_core::native_release::{SourceIdentity, SourceContext, SourcePlan, SourceEdit};
use std::{fs, path::Path, time::Duration};

pub fn identity(root: &Path) -> Result<SourceIdentity> {
    let version = fs::read_to_string(root.join("VERSION")).map_err(|e| e.to_string())?;
    version.trim().parse::<SemVer>()?;
    Ok(SourceIdentity { product: "BridgePilot".into(), repository: "freakybridge/BridgePilot".into(),
        version: version.trim().into(), tag_prefix: "bp-v".into(), paths: vec!["VERSION".into(), "CHANGELOG.md".into()] })
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let mut request = ProcessRequest::new("git", root);
    request.args = args.iter().map(Into::into).collect();
    request.timeout = Duration::from_secs(60);
    let result = SystemProcessRunner.run(&request).map_err(|e| e.to_string())?;
    if result.code != 0 || result.timed_out { return Err(format!("release source Git query failed: {}", String::from_utf8_lossy(&result.stderr))); }
    String::from_utf8(result.stdout).map_err(|e| e.to_string())
}

fn change_level(message: &str) -> Result<u8> {
    let subject = message.lines().next().unwrap_or("").trim();
    let Some((kind, _)) = subject.split_once(':') else {
        return Err(format!("release change classification requires a conventional commit: {subject}"));
    };
    let breaking = kind.ends_with('!') || message.lines().any(|line| line.starts_with("BREAKING CHANGE:") || line.starts_with("BREAKING-CHANGE:"));
    let kind = kind.trim_end_matches('!').split('(').next().unwrap();
    match kind {
        "feat" => Ok(if breaking { 3 } else { 2 }),
        "fix" | "perf" | "refactor" | "docs" | "test" | "build" | "ci" | "chore" | "style" | "revert" => Ok(if breaking { 3 } else { 1 }),
        _ => Err(format!("unclassified release change: {subject}")),
    }
}

fn increment(mut version: SemVer, level: u8) -> Result<SemVer> {
    let add = |value: u64| value.checked_add(1).ok_or_else(|| "version overflow".to_string());
    if level == 3 && version.major > 0 {
        version.major = add(version.major)?; version.minor = 0; version.patch = 0;
    } else if level >= 2 {
        version.minor = add(version.minor)?; version.patch = 0;
    } else { version.patch = add(version.patch)?; }
    Ok(version)
}

// Executable Skill/template Markdown is code input; ordinary prose is not.
// Unknown inputs remain relevant rather than silently hiding a new build input.
fn code_input(path: &str) -> bool {
    if path.starts_with("doc/") || matches!(path, "VERSION" | "CHANGELOG.md" | "README.md" | "INSTALL.md" | "AGENTS.md") { return false; }
    if path.starts_with("source/") || path.starts_with(".codex/") { return true; }
    if path.ends_with(".md") || path.ends_with(".rst") { return false; }
    true
}

fn changed_paths(root: &Path, base: &str) -> Result<Vec<String>> {
    let mut paths: Vec<String> = git(root, &["diff", "--no-renames", "--name-only", "-z", base, "--"])?
        .split('\0').filter(|p| !p.is_empty()).map(str::to_owned).collect();
    paths.extend(git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?
        .split('\0').filter(|p| !p.is_empty()).map(str::to_owned));
    paths.sort(); paths.dedup();
    Ok(paths)
}

fn published_asset_names() -> Vec<String> {
    let mut names = vec!["package.json".into()];
    names.extend(crate::install::assets().into_iter().map(|(_, path, _, _)|
        Path::new(path).file_name().unwrap().to_string_lossy().into_owned()));
    names
}

pub fn plan(root: &Path, context: &SourceContext) -> Result<SourcePlan> {
    let mut identity = identity(root)?;
    if context.schema != 1 || context.identity != identity { return Err("release source identity changed".into()); }
    let before_version = fs::read_to_string(root.join("VERSION")).map_err(|e| e.to_string())?;
    let before_notes = fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?;
    let current = identity.version.parse::<SemVer>()?;
    let mut selected = current;
    let mut messages = Vec::<String>::new();
    let mut reuse_release = None;
    if let Some(baseline) = &context.baseline {
        if baseline.commit.len() != 40 || !baseline.commit.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("invalid published commit".into()); }
        let base = baseline.version.parse::<SemVer>()?;
        if current < base { return Err("native VERSION is older than the published baseline".into()); }
        if git(root, &["merge-base", &baseline.commit, "HEAD"])?.trim() != baseline.commit {
            return Err("published baseline is not an ancestor; explicit release decision required".into());
        }
        let code_paths: Vec<_> = changed_paths(root, &baseline.commit)?.into_iter().filter(|p| code_input(p)).collect();
        if code_paths.is_empty() && current == base { reuse_release = Some(baseline.clone()); }
        let range = format!("{}..HEAD", baseline.commit);
        if !code_paths.is_empty() {
            let mut args = vec!["log", "--format=%B%x00", range.as_str(), "--"];
            args.extend(code_paths.iter().map(String::as_str));
            messages.extend(git(root, &args)?.split('\0').map(str::trim).filter(|m| !m.is_empty()).map(str::to_owned));
        }
        let dirty_code = changed_paths(root, "HEAD")?.iter().any(|p| code_paths.contains(p));
        if dirty_code {
            let message = context.message.as_deref().filter(|m| !m.trim().is_empty())
                .ok_or("uncommitted release changes require a classified commit message")?;
            messages.push(message.trim().into());
        }
        if !messages.is_empty() {
            let level = messages.iter().map(|m| change_level(m)).collect::<Result<Vec<_>>>()?.into_iter().max().unwrap();
            let required = increment(base, level)?;
            if current > base && current < required { return Err("prepared VERSION is below the required cumulative change level".into()); }
            selected = current.max(required);
        }
    }
    identity.version = selected.to_string();
    if reuse_release.is_some() {
        return Ok(SourcePlan { schema: 1, identity, edits: vec![], reuse_release,
            release_assets: published_asset_names() });
    }
    let heading = format!("## [BP {selected}]");
    let mut after_notes = before_notes.clone();
    let unreleased = before_notes.lines().any(|line| line.trim() == "## [Unreleased]");
    let existing = before_notes.lines().any(|line| line == heading || line.starts_with(&format!("{heading} - ")));
    if !existing {
        if unreleased {
            after_notes = before_notes.replacen("## [Unreleased]", &heading, 1);
        } else if selected != current {
            let notes = messages.iter().map(|m| format!("- {}", m.lines().next().unwrap_or(""))).collect::<Vec<_>>().join("\n");
            let at = before_notes.find("\n## ").ok_or("CHANGELOG has no insertion point")?;
            after_notes.insert_str(at, &format!("\n{heading}\n\n{notes}\n"));
        } else { return Err("native release notes are missing for the selected version".into()); }
    } else if unreleased {
        return Err("both selected release notes and Unreleased exist; resolve notes ownership".into());
    }
    let mut edits = Vec::new();
    if selected != current { edits.push(SourceEdit { path: "VERSION".into(), before: before_version, after: format!("{selected}\n") }); }
    if after_notes != before_notes { edits.push(SourceEdit { path: "CHANGELOG.md".into(), before: before_notes, after: after_notes }); }
    Ok(SourcePlan { schema: 1, identity, edits, reuse_release: None, release_assets: vec![] })
}

#[cfg(test)]
#[path = "../../../scripts/tests/orchestrator/release_source.rs"]
mod tests;
