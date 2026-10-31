//! Language-neutral release orchestration for the opt-in Rust policy.
use super::rust::{RustConfig, Workspace};
use super::*;

pub(super) fn status(root: &Path, runner: &dyn ProcessRunner) -> Result<String, String> {
    history::git(
        root,
        runner,
        &[
            "--no-optional-locks",
            "-c",
            "core.quotepath=false",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ],
    )
}

pub(super) fn inventory(root: &Path, runner: &dyn ProcessRunner) -> Result<Vec<String>, String> {
    let mut paths = BTreeSet::new();
    for args in [
        vec!["ls-files", "-z"],
        vec!["ls-files", "--others", "--exclude-standard", "-z"],
    ] {
        paths.extend(history::paths(&history::git(root, runner, &args)?));
    }
    Ok(paths.into_iter().collect())
}

fn existing_config(cfg: &RustConfig, paths: &[String]) -> RustConfig {
    let mut existing = cfg.clone();
    existing.workspaces.retain(|path| paths.contains(path));
    for mapping in &mut existing.shared_inputs {
        mapping.packages.retain(|path| paths.contains(path));
    }
    existing
        .shared_inputs
        .retain(|mapping| !mapping.packages.is_empty());
    existing
}

pub(super) fn historical_workspace(
    root: &Path,
    revision: &str,
    cfg: &RustConfig,
    runner: &dyn ProcessRunner,
) -> Result<Workspace, String> {
    let paths = history::paths(&history::git(
        root,
        runner,
        &["ls-tree", "-r", "--name-only", "-z", revision],
    )?);
    // Schema 2 uses the roots and mappings actually registered at this revision.
    // This permits explicit registration of a genuinely new standalone workspace
    // without trying to read its manifest before it existed.
    if let Some(payload) =
        revision_payload(root, revision, ".codex/bridgeforge-version.json", runner)?
    {
        if let Some(historical) = config::parse(&payload)? {
            return super::rust::load(&historical, &paths, &mut |path| {
                revision_payload(root, revision, path, runner)
            });
        } else {
            let value = crate::json::parse_unique_json(&payload, "historical version config")?;
            let manifests = if config::auto_manifests(&value)? {
                ["Cargo.toml", "package.json"]
                    .into_iter()
                    .filter(|path| paths.iter().any(|p| p == path))
                    .map(str::to_string)
                    .collect::<BTreeSet<_>>()
            } else {
                let entries = value["manifests"]
                    .as_array()
                    .filter(|entries| !entries.is_empty())
                    .ok_or("release baseline manifests are missing or empty")?;
                let mut manifests = BTreeSet::new();
                for entry in entries {
                    let path = entry
                        .as_str()
                        .ok_or("invalid release baseline manifest path")?;
                    safe_relative(root, path)?;
                    if !paths.iter().any(|p| p == path) {
                        return Err(format!("historical native manifest missing: {path}"));
                    }
                    if !matches!(
                        Path::new(path).file_name().and_then(|s| s.to_str()),
                        Some("Cargo.toml" | "package.json")
                    ) {
                        return Err(format!("unsupported historical native manifest: {path}"));
                    }
                    manifests.insert(path.to_string());
                }
                manifests
            };
            let legacy = existing_config(cfg, &paths);
            return super::rust::load_legacy_history(&legacy, &paths, &manifests, &mut |path| {
                revision_payload(root, revision, path, runner)
            });
        }
    }
    // No config is the legacy automatic policy, but is not proof of a schema 1
    // lock exemption. Omit nonexistent new roots while keeping every lock strict.
    super::rust::load(&existing_config(cfg, &paths), &paths, &mut |path| {
        revision_payload(root, revision, path, runner)
    })
}

fn current_workspace(
    root: &Path,
    cfg: &RustConfig,
    runner: &dyn ProcessRunner,
) -> Result<Workspace, String> {
    super::rust::load(cfg, &inventory(root, runner)?, &mut |path| {
        config::read_current(root, path)
    })
}

pub(super) fn validate_current(
    root: &Path,
    boundary: &str,
    cfg: &RustConfig,
    runner: &dyn ProcessRunner,
) -> Result<(), String> {
    let current = current_workspace(root, cfg, runner)?;
    let baseline = historical_workspace(root, boundary, cfg, runner)?;
    current.validate_baseline(&baseline)
}
