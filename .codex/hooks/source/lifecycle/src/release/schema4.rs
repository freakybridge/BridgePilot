//! Schema 4 owns release decisions. Native parsers/renderers and Git transactions are shared.
use super::model::{Language, NativeDependency, NativePackage};
use super::*;
mod adoption;
mod analysis;
mod configuration;
mod content_identity;
mod environment;
mod inputs;
mod portable_environment;
pub use environment::{EnvironmentGuard, PythonEnvironment};
pub mod development;
mod development_inputs;
pub use development_inputs::BuildDirectoryGuard;
mod lineage;
mod migration;
mod plan;
pub(super) mod withdrawal;
pub(super) use configuration::{Baseline, Config, load, parse};
use inputs::{Capture, capture};
pub use migration::{Prepared as ReleaseUpgrade, prepare_project_upgrade};
pub use migration::{configuration_apply, configuration_plan, migration_apply, migration_plan};
pub use migration::{repair_baseline_apply, repair_baseline_plan};
pub use plan::{build, preview};
pub(super) const CONFIG: &str = ".codex/bridgeforge-version.json";

/// Prefetch only inputs already declared by the migration proof and registered
/// native manifests. Never load all repository blobs or guess source ownership.
/// A hint is not evidence: all existing reconstruction and hash checks still run.
pub(super) fn prefetch_history(
    cfg: &Config,
    head: &str,
    reader: &history_cache::Reader<'_>,
) -> Result<(), String> {
    let paths = cfg
        .baseline
        .input_hashes
        .keys()
        .cloned()
        .chain(cfg.native.packages.iter().map(|p| p.manifest.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let revisions = std::iter::once(cfg.baseline.revision.as_str())
        .chain(cfg.baseline.lineage.iter().map(|s| s.revision.as_str()))
        .chain(std::iter::once(head))
        .collect::<BTreeSet<_>>();
    reader.prefetch_hints(&revisions, &paths);
    Ok(())
}
pub(super) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn product_mapping_inputs(
    cfg: &Config,
    packages: &BTreeMap<String, NativePackage>,
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    product: SemVer,
) -> Result<BTreeMap<String, Option<Vec<u8>>>, String> {
    let inputs = rust::product_mapping_inputs(&cfg.product_manifests, inventory, read, product)?;
    if packages.values().any(|p| inputs.contains_key(&p.manifest)) {
        return Err(
            "product manifest workspaces must not include independently registered packages".into(),
        );
    }
    let mut inputs = inputs;
    if !cfg.native.product_packages.is_empty() {
        let rust = cfg
            .native
            .adapters
            .rust
            .as_ref()
            .ok_or("product packages require a Rust adapter")?;
        let explicit = rust::product_package_inputs(
            rust,
            &cfg.native.product_packages,
            inventory,
            read,
            product,
        )?;
        if cfg
            .product_manifests
            .iter()
            .any(|path| explicit.contains_key(path))
        {
            return Err(
                "whole-workspace product mappings overlap explicit product packages".into(),
            );
        }
        for (path, bytes) in explicit {
            if inputs.get(&path).is_some_and(|old| old != &bytes) {
                return Err(format!("product input groups disagree on {path}"));
            }
            inputs.insert(path, bytes);
        }
    }
    Ok(inputs)
}

pub(super) fn require_ignored_runtime(
    root: &Path,
    runner: &dyn ProcessRunner,
) -> Result<(), String> {
    let mut request = ProcessRequest::new("git", root);
    request.args = [
        "check-ignore",
        "--no-index",
        "--quiet",
        ".runtime/bridgeforge-codex/project-sync.lock",
    ]
    .iter()
    .map(Into::into)
    .collect();
    request.timeout = Duration::from_secs(30);
    let output = runner.run(&request).map_err(|e| e.to_string())?;
    if output.timed_out || output.code != 0 {
        return Err(
            "schema 4 requires .runtime/ to be ignored before migration or release analysis".into(),
        );
    }
    Ok(())
}

/// Inspection must not execute fsmonitor hooks or refresh the Git index.
pub(super) struct InspectionRunner<'a>(pub &'a dyn ProcessRunner);
impl ProcessRunner for InspectionRunner<'_> {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<crate::ProcessOutput> {
        let mut request = request.clone();
        if Path::new(&request.program)
            .file_stem()
            .and_then(|s| s.to_str())
            == Some("git")
        {
            request.args.splice(
                0..0,
                [
                    std::ffi::OsString::from("-c"),
                    std::ffi::OsString::from("core.fsmonitor=false"),
                    std::ffi::OsString::from("-c"),
                    std::ffi::OsString::from("diff.autoRefreshIndex=false"),
                ],
            );
            request.env.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
        }
        self.0.run(&request)
    }
}

/// The analysis is deliberately outside project configuration: it describes one final state.
#[derive(Clone, Debug, Default)]
pub struct Request {
    pub analysis_file: Option<PathBuf>,
}

pub(super) fn baseline_snapshot(
    root: &Path,
    cfg: &Config,
    revision: &str,
    runner: &dyn ProcessRunner,
) -> Result<polyglot::Snapshot, String> {
    migration::verify_baseline(root, cfg, revision, runner)?;
    if revision != cfg.baseline.revision {
        let payload = revision_payload(root, revision, CONFIG, runner)?
            .ok_or("schema 4 release baseline config is missing")?;
        parse(&payload)?.ok_or("release baseline after migration must use schema 4")?;
        return polyglot::historical(root, revision, &cfg.native, runner);
    }
    for (path, expected) in &cfg.baseline.input_hashes {
        let actual = revision_payload(root, revision, path, runner)?.map(|b| digest(&b));
        if &actual != expected {
            return Err(format!(
                "migration baseline input no longer matches immutable history: {path}"
            ));
        }
    }
    let mut scope = cfg.native.clone();
    scope.product_inputs = cfg.baseline.product_inputs.clone();
    scope.shared_inputs = cfg.baseline.shared_inputs.clone();
    scope.lifecycle = cfg.baseline.lifecycle.clone();
    Ok(polyglot::Snapshot {
        packages: cfg.baseline.packages.clone(),
        dependencies: cfg.baseline.dependencies.clone(),
        config: Some(scope),
        ..Default::default()
    })
}

pub(super) fn validate_current(
    root: &Path,
    boundary: &str,
    cfg: &Config,
    runner: &dyn ProcessRunner,
) -> Result<(), String> {
    let capture = capture(root, cfg, runner)?;
    if capture.boundary != boundary {
        return Err("schema 4 release baseline changed".into());
    }
    Ok(())
}

pub(super) fn verify_historical(
    root: &Path,
    revision: &str,
    cfg: &Config,
    runner: &dyn ProcessRunner,
) -> Result<(), String> {
    let snapshot = polyglot::historical(root, revision, &cfg.native, runner)?;
    let version = revision_payload(root, revision, "VERSION", runner)?
        .ok_or("schema 4 baseline VERSION missing")?;
    let version = std::str::from_utf8(&version)
        .map_err(|e| e.to_string())?
        .trim()
        .parse::<SemVer>()?;
    if cfg.strategy == "synchronized" {
        for p in snapshot.packages.values() {
            if p.version != version.to_string() {
                return Err(format!(
                    "synchronized release baseline version mismatch: {}",
                    p.manifest
                ));
            }
        }
    }
    if !cfg.product_manifests.is_empty() {
        let inventory = history::paths(&history::git(
            root,
            runner,
            &["ls-tree", "-r", "--name-only", "-z", revision],
        )?);
        product_mapping_inputs(
            cfg,
            &snapshot.packages,
            &inventory,
            &mut |p| revision_payload(root, revision, p, runner),
            version,
        )?;
    }
    // Explicit carrier declarations are revision-owned. A later split or move
    // must not impose today's carrier paths on an older published snapshot.
    if let Some(native) = snapshot.config.as_ref()
        && !native.product_packages.is_empty()
    {
        let inventory = history::paths(&history::git(
            root,
            runner,
            &["ls-tree", "-r", "--name-only", "-z", revision],
        )?);
        rust::product_package_inputs(
            native
                .adapters
                .rust
                .as_ref()
                .ok_or("historical product packages require a Rust adapter")?,
            &native.product_packages,
            &inventory,
            &mut |p| revision_payload(root, revision, p, runner),
            version,
        )?;
    }
    Ok(())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../../scripts/tests/unit/core_release_schema4.rs"]
mod tests;

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../../scripts/tests/support/schema4_agent.rs"]
pub mod fixture;
