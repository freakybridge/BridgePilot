//! Project-owned layout policy. Checks never move files or mutate the index.
use crate::project_structure::{Finding, StructureReport};
use crate::{ProcessRequest, ProcessRunner, SystemProcessRunner};
use bridgeforge_core::artifact_paths::{self, Kind};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Duration,
};
use toml_edit::DocumentMut;
use walkdir::WalkDir;

pub const CONFIG: &str = ".codex/artifact-layout.json";
const MANAGED: &str = ".codex/managed-skeleton.json";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: u32,
    #[serde(default)]
    pub source_roots: Vec<String>,
    #[serde(default = "tests_root")]
    pub tests_root: String,
    #[serde(default = "tools_root")]
    pub tools_root: String,
    #[serde(default)]
    pub python_artifacts: PythonArtifacts,
    #[serde(default)]
    pub forbidden_source_components: Vec<String>,
    #[serde(default)]
    pub cargo_registration: CargoRegistration,
    /// Exact output paths or directory roots, never ownership globs.
    #[serde(default)]
    pub outputs: Vec<OutputPath>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OutputPath {
    pub path: String,
    pub kind: Kind,
    #[serde(default)]
    pub directory: bool,
    /// Cleanup consumes these explicit roles; an absent role grants no deletion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<CleanupRole>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupRole {
    Keep,
    Cache,
    History,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema: 1,
            source_roots: Vec::new(),
            tests_root: tests_root(),
            tools_root: tools_root(),
            python_artifacts: PythonArtifacts::default(),
            forbidden_source_components: Vec::new(),
            cargo_registration: CargoRegistration::default(),
            outputs: Vec::new(),
        }
    }
}
fn effective_config(bytes: Option<&[u8]>) -> Result<Config, String> {
    bytes.map(parse).transpose().map(Option::unwrap_or_default)
}
fn tests_root() -> String {
    "scripts/tests/".into()
}
fn tools_root() -> String {
    "scripts/".into()
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PythonArtifacts {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_roots: Vec<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CargoRegistration {
    #[serde(default)]
    pub manifest_roots: Vec<String>,
    #[serde(default)]
    pub forbidden_target_roots: Vec<String>,
}
fn key(value: &str) -> String {
    let value = value.replace('\\', "/");
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}
fn within(path: &str, root: &str) -> bool {
    let path = key(path);
    let root = key(root).trim_end_matches('/').to_owned();
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|tail| tail.starts_with('/'))
}
// Real Git paths are literal names, not configuration globs. In particular,
// brackets in a tracked file name must not poison unrelated submissions.
fn repository_relative(value: &str) -> Result<String, String> {
    let value = value.replace('\\', "/");
    if value.is_empty()
        || value.contains('\0')
        || value.starts_with('/')
        || (cfg!(windows) && value.contains(':'))
        || value.split('/').any(|p| p == "." || p == "..")
    {
        return Err(format!("path must stay inside the repository: {value:?}"));
    }
    Ok(value.trim_end_matches('/').into())
}
fn safe_relative(value: &str) -> Result<String, String> {
    if value.trim().is_empty() || value.contains(['\0', '\r', '\n', ':', '*', '?', '[', ']']) {
        return Err(format!("unsafe repository-relative layout path: {value:?}"));
    }
    repository_relative(value)
}
pub fn parse(bytes: &[u8]) -> Result<Config, String> {
    let value = crate::json::parse_unique_json(bytes, CONFIG)?;
    let cfg: Config = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if cfg.schema != 1 {
        return Err("artifact-layout schema must be 1".into());
    }
    let mut paths: Vec<&str> = cfg.source_roots.iter().map(String::as_str).collect();
    paths.extend([cfg.tests_root.as_str(), cfg.tools_root.as_str()]);
    paths.extend(
        cfg.python_artifacts
            .allowed_roots
            .iter()
            .map(String::as_str),
    );
    paths.extend(
        cfg.cargo_registration
            .manifest_roots
            .iter()
            .map(String::as_str),
    );
    paths.extend(
        cfg.cargo_registration
            .forbidden_target_roots
            .iter()
            .map(String::as_str),
    );
    for path in paths {
        safe_relative(path)?;
    }
    for path in &cfg.python_artifacts.allowed_roots {
        if !within(path, artifact_paths::RUNTIME) {
            return Err("Python artifact roots must remain under .runtime/; legacy exemptions require migration".into());
        }
    }
    let mut outputs = BTreeSet::new();
    for output in &cfg.outputs {
        safe_relative(&output.path)?;
        if !output.kind.accepts(&output.path, output.directory) {
            return Err(format!(
                "declared {:?} output must respect {}: {}",
                output.kind,
                output.kind.root(),
                output.path
            ));
        }
        if output.cleanup.is_some()
            && (output.kind != Kind::Build
                || key(&output.path).trim_end_matches('/') == artifact_paths::BUILD)
        {
            return Err("cleanup output roles require exact paths below .build; the entire root cannot be selected".into());
        }
        if !outputs.insert(key(&output.path)) {
            return Err(format!("duplicate output path: {}", output.path));
        }
    }
    if !within(&cfg.tests_root, "scripts/tests") || !within(&cfg.tools_root, "scripts") {
        return Err(
            "project test/tool roots must respect scripts/tests and scripts public boundaries"
                .into(),
        );
    }
    if cfg.python_artifacts.enabled && cfg.python_artifacts.allowed_roots.is_empty() {
        return Err("enabled Python artifact checking requires allowed_roots".into());
    }
    for component in &cfg.forbidden_source_components {
        let safe = safe_relative(component)?;
        if safe.contains('/') {
            return Err("forbidden source components must be single directory names".into());
        }
    }
    for source in &cfg.source_roots {
        if within(source, &cfg.tests_root)
            || within(&cfg.tests_root, source)
            || within(source, &cfg.tools_root)
            || within(&cfg.tools_root, source)
        {
            return Err(format!(
                "production root overlaps test/tool roots: {source}"
            ));
        }
        for cache in &cfg.python_artifacts.allowed_roots {
            if within(cache, source) || within(source, cache) {
                return Err("cache roots overlap production roots".into());
            }
        }
    }
    Ok(cfg)
}
fn finding(code: &str, path: &str, message: impl Into<String>) -> Finding {
    Finding {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}
fn failed(error: String) -> StructureReport {
    StructureReport {
        errors: vec![finding("artifact-layout-incomplete", CONFIG, error)],
        advisories: vec![],
    }
}
fn process_artifact(path: &str) -> bool {
    let normalized = key(path);
    let p: Vec<_> = normalized.split('/').collect();
    let name = p.last().copied().unwrap_or("");
    p.iter()
        .any(|p| ["__pycache__", ".pytest_cache", "htmlcov"].contains(p))
        || name == "coverage.xml"
        || name == ".coverage"
        || name.starts_with(".coverage.")
        || name.ends_with(".pyc")
        || name.ends_with(".pyo")
}
fn known_kind(path: &str, is_dir: bool) -> Option<Kind> {
    let path = key(path);
    let parts: Vec<_> = path.split('/').collect();
    let name = parts.last().copied().unwrap_or("");
    let dirs = if is_dir {
        parts.as_slice()
    } else {
        &parts[..parts.len().saturating_sub(1)]
    };
    if dirs
        .iter()
        .any(|p| [".venv", "venv", "node_modules"].contains(p))
    {
        return Some(Kind::Dependency);
    }
    if dirs.iter().any(|p| ["target", "dist"].contains(p)) {
        return Some(Kind::Build);
    }
    if parts.contains(&"htmlcov")
        || name == "coverage.xml"
        || name == ".coverage"
        || name.starts_with(".coverage.")
        || parts.contains(&".pytest_cache")
    {
        return Some(Kind::Test);
    }
    if process_artifact(&path) {
        return Some(Kind::Runtime);
    }
    // Only named managed tools/receipts, not arbitrary business executables.
    if !is_dir
        && !within(&path, artifact_paths::DOWNLOAD)
        && ([
            "bridgeforge.exe",
            "bridgeforge-hook.exe",
            "bridgeforge-factory.exe",
        ]
        .contains(&name)
            || (parts.iter().any(|p| *p == "bin")
                && ["bridgeforge", "bridgeforge-hook", "bridgeforge-factory"].contains(&name))
            || [
                "build-receipt-cli.json",
                "build-receipt-hook.json",
                "build-receipt-factory.json",
            ]
            .contains(&name))
    {
        return Some(if within(&path, artifact_paths::DEPS) {
            Kind::Dependency
        } else {
            Kind::Build
        });
    }
    None
}

/// A root directory exclusion protects every descendant: Git cannot re-include
/// children without first re-including the parent. Inspect root semantics only;
/// ignored nested .gitignore files therefore cannot weaken these exclusions.
fn ignore_findings(bytes: Option<&[u8]>) -> Result<Vec<Finding>, String> {
    artifact_paths::ignore_problems(bytes).map(|items| {
        items
            .into_iter()
            .map(|message| finding("artifact-ignore-required", ".gitignore", message))
            .collect()
    })
}

fn managed_paths(bytes: Option<&[u8]>) -> Result<BTreeSet<String>, String> {
    let Some(bytes) = bytes else {
        return Ok(BTreeSet::new());
    };
    let value = crate::json::parse_unique_json(bytes, MANAGED)?;
    Ok(value["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["target"].as_str())
        .map(key)
        .collect())
}
pub fn path_findings(
    path: &str,
    is_dir: bool,
    cfg: Option<&Config>,
    managed: &BTreeSet<String>,
) -> Vec<Finding> {
    let path = key(path);
    let parts: Vec<_> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return vec![];
    }
    if ["test", "tests"].contains(&parts[0]) {
        return vec![finding(
            "legacy-test-root",
            &path,
            "root test/tests is forbidden; use scripts/tests/",
        )];
    }
    let defaults;
    let cfg = match cfg {
        Some(cfg) => cfg,
        None => {
            defaults = Config::default();
            &defaults
        }
    };
    let mut errors = vec![];
    if let Some(kind) = known_kind(&path, is_dir) {
        // Bytecode caches can also occur inside an installed dependency or a
        // generated build workspace. Those trees are never source inputs.
        let contained = kind == Kind::Runtime
            && [
                artifact_paths::DEPS,
                artifact_paths::BUILD,
                artifact_paths::TESTS,
            ]
            .iter()
            .any(|r| within(&path, r));
        if !contained && !kind.accepts(&path, is_dir) {
            errors.push(finding(
                if process_artifact(&path) {
                    "misplaced-python-artifact"
                } else {
                    "misplaced-artifact"
                },
                &path,
                format!("{kind:?} artifacts must stay under {}", kind.root()),
            ));
        }
    }
    if within(&path, artifact_paths::TESTS)
        && path != artifact_paths::TESTS
        && !Kind::Test.accepts(&path, is_dir)
    {
        errors.push(finding(
            "test-artifact-task-required",
            &path,
            "test output requires .runtime/tests/<task>/",
        ));
    }
    let directories = if is_dir {
        parts.as_slice()
    } else {
        &parts[..parts.len() - 1]
    };
    let managed_location =
        managed.contains(&path) || (is_dir && managed.iter().any(|asset| within(asset, &path)));
    if directories.contains(&"tests")
        && !within(&path, &cfg.tests_root)
        && !(is_dir && within(&cfg.tests_root, &path))
        && !managed_location
        && !artifact_paths::local(&path)
    {
        errors.push(finding(
            "misplaced-test-directory",
            &path,
            format!("standalone tests belong under {}", cfg.tests_root),
        ));
    }
    for source in &cfg.source_roots {
        if !within(&path, source) {
            continue;
        }
        let depth = key(source).trim_end_matches('/').split('/').count();
        if directories
            .iter()
            .skip(depth)
            .any(|p| cfg.forbidden_source_components.iter().any(|v| key(v) == *p))
        {
            errors.push(finding(
                "forbidden-source-directory",
                &path,
                "production roots cannot contain configured test/tool/cache directories",
            ));
        }
    }
    errors
}
fn manifest_path(path: &str, cfg: &Config) -> bool {
    Path::new(path)
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("Cargo.toml"))
        && cfg
            .cargo_registration
            .manifest_roots
            .iter()
            .any(|r| within(path, r))
}
fn resolve_reference(manifest: &str, reference: &str) -> Result<String, String> {
    let reference = reference.replace('\\', "/");
    if reference.starts_with('/') || reference.contains([':', '\0', '\r', '\n']) {
        return Err(format!(
            "Cargo layout target is absolute or unsafe: {reference}"
        ));
    }
    let mut p: Vec<String> = manifest.split('/').map(str::to_owned).collect();
    p.pop();
    for item in reference.split('/') {
        match item {
            "" | "." => (),
            ".." => {
                if p.pop().is_none() {
                    return Err(format!("Cargo target escapes repository: {reference}"));
                }
            }
            value => p.push(value.into()),
        }
    }
    Ok(p.join("/"))
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Registration {
    kind: String,
    raw: String,
    target: String,
}

use bridgeforge_core::artifact_paths::glob_regex;

fn expanded_registrations(
    path: &str,
    bytes: &[u8],
    tree: &BTreeSet<String>,
) -> Result<Vec<Registration>, String> {
    let mut output = Vec::new();
    for entry in registrations(path, bytes)? {
        if entry.kind == "workspace member" && entry.target.contains(['*', '?', '[']) {
            let expression = glob_regex(&entry.target)?;
            let mut directories = BTreeSet::new();
            for file in tree {
                let components: Vec<_> = file.split('/').collect();
                for count in 1..components.len() {
                    directories.insert(key(&components[..count].join("/")));
                }
            }
            output.extend(
                directories
                    .into_iter()
                    .filter(|dir| expression.is_match(dir))
                    .map(|target| Registration {
                        target,
                        ..entry.clone()
                    }),
            );
        } else {
            output.push(entry);
        }
    }
    Ok(output)
}
fn registrations(path: &str, bytes: &[u8]) -> Result<Vec<Registration>, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let document: DocumentMut = text
        .parse()
        .map_err(|e| format!("cannot inspect Cargo TOML: {e}"))?;
    let mut entries = Vec::new();
    let mut add = |kind: &str, raw: &str| -> Result<(), String> {
        entries.push(Registration {
            kind: kind.into(),
            raw: raw.into(),
            target: resolve_reference(path, raw)?,
        });
        Ok(())
    };
    if let Some(member_value) = document.get("workspace").and_then(|v| v.get("members")) {
        let members = member_value
            .as_array()
            .ok_or("workspace members must be an array")?;
        for member in members {
            add(
                "workspace member",
                member.as_str().ok_or("workspace members must be strings")?,
            )?;
        }
    }
    for kind in ["test", "bench", "example", "bin"] {
        if let Some(entries) = document.get(kind).and_then(|v| v.as_array_of_tables()) {
            for entry in entries {
                if let Some(value) = entry.get("path") {
                    add(
                        kind,
                        value.as_str().ok_or("Cargo target path must be a string")?,
                    )?;
                }
            }
        } else if let Some(entries) = document.get(kind).and_then(|v| v.as_array()) {
            for entry in entries {
                let entry = entry
                    .as_inline_table()
                    .ok_or("Cargo target entries must be tables")?;
                if let Some(value) = entry.get("path") {
                    add(
                        kind,
                        value.as_str().ok_or("Cargo target path must be a string")?,
                    )?;
                }
            }
        } else if document.get(kind).is_some() {
            return Err(format!("invalid Cargo {kind} table array"));
        }
    }
    Ok(entries)
}
fn check_manifest(
    path: &str,
    bytes: &[u8],
    cfg: &Config,
    baseline: Option<&[u8]>,
    changed: &BTreeSet<String>,
    expanded: bool,
    tree: &BTreeSet<String>,
    previous_cfg: Option<&Config>,
) -> Vec<Finding> {
    let entries = match expanded_registrations(path, bytes, tree) {
        Ok(v) => v,
        Err(e) => return vec![finding("invalid-cargo-layout", path, e)],
    };
    let previous = baseline
        .and_then(|b| expanded_registrations(path, b, tree).ok())
        .unwrap_or_default();
    entries
        .into_iter()
        .filter(|entry| {
            cfg.cargo_registration
                .forbidden_target_roots
                .iter()
                .any(|r| within(&entry.target, r))
                && (expanded
                    || !previous.contains(entry)
                    || !previous_cfg.is_some_and(|previous_cfg| {
                        manifest_path(path, previous_cfg)
                            && previous_cfg
                                .cargo_registration
                                .forbidden_target_roots
                                .iter()
                                .any(|r| within(&entry.target, r))
                    })
                    || changed
                        .iter()
                        .any(|p| within(p, &entry.target) || within(&entry.target, p)))
        })
        .map(|entry| {
            finding(
                "external-cargo-registration",
                path,
                format!(
                    "{} {:?} resolves to {}; production must not register tool/test targets",
                    entry.kind, entry.raw, entry.target
                ),
            )
        })
        .collect()
}
fn safe_file(root: &Path, relative: &str) -> Result<Option<Vec<u8>>, String> {
    let relative = repository_relative(relative)?;
    // Reuse the existing bounded reader, including Windows reparse checks.
    crate::managed_paths::read_metadata(&root.join(relative), 16 * 1024 * 1024)
}
pub fn worktree(root: &Path) -> StructureReport {
    let result = (|| -> Result<StructureReport, String> {
        let cfg = effective_config(safe_file(root, CONFIG)?.as_deref())?;
        let mut report = StructureReport::default();
        for path in bridgeforge_core::artifact_migration::legacy_test_outputs(root)?.keys() {
            report.errors.push(finding(
                "misplaced-test-artifact",
                &path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/"),
                "managed test receipts require .runtime/tests/<task>/; complete managed migration",
            ));
        }
        report
            .errors
            .extend(ignore_findings(safe_file(root, ".gitignore")?.as_deref())?);
        let tracked = if root.join(".git").exists() {
            git(root, &SystemProcessRunner, &["ls-files", "-z"], vec![])?
        } else {
            vec![]
        };
        for path in std::str::from_utf8(&tracked)
            .map_err(|e| e.to_string())?
            .split('\0')
            .filter(|p| !p.is_empty())
        {
            if artifact_paths::local(path) {
                report.errors.push(finding("tracked-local-artifact", path, "local artifact roots must not contain tracked files; preserve local data when removing tracking"));
            }
        }
        for output in &cfg.outputs {
            // Bounded metadata access verifies ancestors before the derived
            // tree is pruned; declared directories need a separate link check.
            let target = root.join(&output.path);
            for ancestor in target.ancestors().take_while(|p| *p != root) {
                if ancestor.exists()
                    && crate::persistence::is_link_or_reparse(ancestor)
                        .map_err(|e| e.to_string())?
                {
                    return Err(format!("declared output traverses link: {}", output.path));
                }
            }
            report.errors.extend(path_findings(
                &output.path,
                output.directory,
                Some(&cfg),
                &BTreeSet::new(),
            ));
        }
        let managed = managed_paths(safe_file(root, MANAGED)?.as_deref())?;
        let skipped = [".git", ".venv", "node_modules", "target"];
        let cfg = &cfg;
        let mut tree = BTreeSet::new();
        let mut manifests = Vec::new();
        let mut iterator = WalkDir::new(root).follow_links(false).into_iter();
        while let Some(entry) = iterator.next() {
            let entry = match entry {
                Ok(v) => v,
                Err(e) => {
                    report.errors.push(finding(
                        "artifact-layout-incomplete",
                        &e.path().unwrap_or(root).display().to_string(),
                        e.to_string(),
                    ));
                    continue;
                }
            };
            if entry.depth() == 0 {
                continue;
            }
            let path = entry
                .path()
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("non-UTF8 layout path")?
                .replace('\\', "/");
            let directory = entry.file_type().is_dir();
            if !directory {
                tree.insert(path.clone());
            }
            let linked = entry.file_type().is_symlink()
                || (directory
                    && crate::persistence::is_link_or_reparse(entry.path())
                        .map_err(|e| format!("cannot inspect layout directory {path}: {e}"))?);
            // Empty non-root test directories have no file-layout counterpart in
            // Git. Do not suppress explicit project rules, cache rules or roots.
            let mut findings = path_findings(&path, directory, Some(cfg), &managed);
            if directory
                && !linked
                && findings
                    .iter()
                    .any(|f| f.code == "misplaced-test-directory")
            {
                let mut children = std::fs::read_dir(entry.path())
                    .map_err(|e| format!("cannot read layout directory {path}: {e}"))?;
                if children
                    .next()
                    .transpose()
                    .map_err(|e| format!("cannot read layout directory {path}: {e}"))?
                    .is_none()
                {
                    findings.retain(|f| f.code != "misplaced-test-directory");
                }
            }
            // Check the directory itself before pruning its contents.
            report.errors.extend(findings);
            if linked {
                report.errors.push(finding(
                    "layout-excluded-link",
                    &path,
                    "link contents not inspected",
                ));
                if directory {
                    iterator.skip_current_dir();
                }
                continue;
            }
            if directory
                && (skipped.contains(&entry.file_name().to_string_lossy().as_ref())
                    || [".deps", ".build"].contains(&key(&path).as_str())
                    || (within(&path, ".runtime")
                        && entry.depth() >= 2
                        && key(&path) != artifact_paths::TESTS))
            {
                report.advisories.push(finding(
                    "layout-excluded-tree",
                    &path,
                    "dependency/build/runtime/allowed-cache contents not inspected",
                ));
                iterator.skip_current_dir();
            } else if entry.file_type().is_file() && manifest_path(&path, cfg) {
                let bytes = safe_file(root, &path)?
                    .ok_or("manifest disappeared during layout inspection")?;
                manifests.push((path, bytes));
            }
        }
        for (path, bytes) in manifests {
            report.errors.extend(check_manifest(
                &path,
                &bytes,
                cfg,
                None,
                &BTreeSet::new(),
                true,
                &tree,
                None,
            ));
        }
        Ok(report)
    })();
    result.unwrap_or_else(failed)
}

fn git(
    root: &Path,
    runner: &dyn ProcessRunner,
    args: &[&str],
    stdin: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let mut request = ProcessRequest::new("git", root);
    request.args = args.iter().map(Into::into).collect();
    request.stdin = stdin;
    request.timeout = Duration::from_secs(30);
    let output = runner.run(&request).map_err(|e| e.to_string())?;
    if output.timed_out || output.code != 0 {
        return Err(format!(
            "layout Git query failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}
#[derive(Clone)]
struct Entry {
    mode: String,
    oid: String,
}
fn index_entries(bytes: &[u8]) -> Result<BTreeMap<String, Entry>, String> {
    let mut entries = BTreeMap::new();
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    for entry in text.split('\0').filter(|v| !v.is_empty()) {
        let (metadata, path) = entry.split_once('\t').ok_or("invalid layout index entry")?;
        let fields: Vec<_> = metadata.split_whitespace().collect();
        if fields.len() != 3 || fields[2] != "0" {
            return Err("unmerged index cannot be checked for layout".into());
        }
        repository_relative(path)?;
        entries.insert(
            path.into(),
            Entry {
                mode: fields[0].into(),
                oid: fields[1].into(),
            },
        );
    }
    Ok(entries)
}
fn batch(
    root: &Path,
    runner: &dyn ProcessRunner,
    oids: &BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if oids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let stdin = oids
        .iter()
        .map(|oid| format!("{oid}\n"))
        .collect::<String>()
        .into_bytes();
    let bytes = git(root, runner, &["cat-file", "--batch"], stdin)?;
    let mut rest = bytes.as_slice();
    let mut output = BTreeMap::new();
    for oid in oids {
        let newline = rest
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("truncated layout blob header")?;
        let fields: Vec<_> = std::str::from_utf8(&rest[..newline])
            .map_err(|e| e.to_string())?
            .split_whitespace()
            .collect();
        if fields.len() != 3 || fields[0] != oid || fields[1] != "blob" {
            return Err("invalid layout blob identity".into());
        }
        let size: usize = fields[2].parse().map_err(|_| "invalid layout blob size")?;
        rest = &rest[newline + 1..];
        if rest.get(size) != Some(&b'\n') {
            return Err("truncated layout blob".into());
        }
        output.insert(oid.clone(), rest[..size].to_vec());
        rest = &rest[size + 1..];
    }
    if !rest.is_empty() {
        return Err("unexpected layout blob trailer".into());
    }
    Ok(output)
}
pub fn staged(root: &Path, runner: &dyn ProcessRunner) -> StructureReport {
    staged_inner(root, runner).unwrap_or_else(failed)
}
fn staged_inner(root: &Path, runner: &dyn ProcessRunner) -> Result<StructureReport, String> {
    // One diff carries both old/new object identities. No per-file Git subprocesses.
    let raw = git(
        root,
        runner,
        &[
            "diff",
            "--cached",
            "--raw",
            "--no-abbrev",
            "--no-renames",
            "--no-ext-diff",
            "-z",
        ],
        vec![],
    )?;
    let text = std::str::from_utf8(&raw).map_err(|e| e.to_string())?;
    let mut parts = text.split('\0').filter(|v| !v.is_empty());
    let mut changed = BTreeSet::new();
    let mut old = BTreeMap::new();
    let mut current = BTreeMap::new();
    while let Some(header) = parts.next() {
        let fields: Vec<_> = header.split_whitespace().collect();
        if fields.len() != 5 || !fields[0].starts_with(':') {
            return Err("malformed staged layout diff".into());
        }
        let path = parts.next().ok_or("missing staged layout path")?;
        repository_relative(path)?;
        changed.insert(path.to_owned());
        if fields[0] != ":000000" {
            old.insert(
                path.to_owned(),
                Entry {
                    mode: fields[0][1..].into(),
                    oid: fields[2].into(),
                },
            );
        }
        if fields[1] != "000000" {
            current.insert(
                path.to_owned(),
                Entry {
                    mode: fields[1].into(),
                    oid: fields[3].into(),
                },
            );
        }
    }
    if changed.is_empty() {
        return Ok(StructureReport::default());
    }
    let index = index_entries(&git(root, runner, &["ls-files", "--stage", "-z"], vec![])?)?;
    let tree: BTreeSet<_> = index.keys().chain(old.keys()).cloned().collect();
    let config_entry = index
        .iter()
        .find(|(path, _)| key(path) == key(CONFIG))
        .map(|(_, e)| e);
    let config_touched = changed.iter().any(|p| key(p) == key(CONFIG));
    let old_config_entry = old
        .iter()
        .find(|(path, _)| key(path) == key(CONFIG))
        .map(|(_, entry)| entry);
    let mut wanted = BTreeSet::new();
    let ignore_affected = config_touched
        || changed.iter().any(|p| key(p) == ".gitignore")
        || current.keys().any(|p| artifact_paths::local(p));
    let ignore_entry = index
        .iter()
        .find(|(p, _)| key(p) == ".gitignore")
        .map(|(_, e)| e);
    if ignore_affected && let Some(entry) = ignore_entry {
        if !entry.mode.starts_with("100") {
            return Err("project .gitignore must be a regular staged file".into());
        }
        wanted.insert(entry.oid.clone());
    }
    if let Some(entry) = config_entry {
        if !entry.mode.starts_with("100") {
            return Err("layout config must be a regular staged file".into());
        }
        wanted.insert(entry.oid.clone());
    }
    if let Some(entry) = old_config_entry.filter(|e| e.mode.starts_with("100")) {
        wanted.insert(entry.oid.clone());
    }
    let initial = batch(root, runner, &wanted)?;
    let cfg = effective_config(config_entry.map(|e| initial[&e.oid].as_slice()))?;
    let previous_cfg = if config_touched {
        // A malformed historical policy is not a reason to forbid its repair
        // or explicit deletion. An unknown baseline expands the new-rule audit.
        match old_config_entry {
            None => Some(Config::default()),
            Some(entry) if entry.mode.starts_with("100") => parse(&initial[&entry.oid]).ok(),
            Some(_) => None,
        }
    } else {
        Some(cfg.clone())
    };
    let config_changed = previous_cfg.as_ref() != Some(&cfg);
    let mut report = StructureReport::default();
    if ignore_affected {
        report.errors.extend(ignore_findings(
            ignore_entry.map(|e| initial[&e.oid].as_slice()),
        )?);
    }
    let mut selected = current.keys().cloned().collect::<BTreeSet<_>>();
    if config_changed || changed.iter().any(|p| key(p) == ".gitignore") {
        selected.extend(index.keys().cloned());
    }
    let managed_entry = index
        .iter()
        .find(|(path, _)| key(path) == key(MANAGED))
        .map(|(_, entry)| entry);
    let managed_touched = changed.iter().any(|p| key(p) == key(MANAGED));
    let old_managed_entry = old
        .iter()
        .find(|(path, _)| key(path) == key(MANAGED))
        .map(|(_, entry)| entry);
    let cargo_affected = !cfg.cargo_registration.manifest_roots.is_empty()
        && (config_changed
            || changed.iter().any(|p| {
                manifest_path(p, &cfg)
                    || cfg
                        .cargo_registration
                        .forbidden_target_roots
                        .iter()
                        .any(|r| within(p, r))
            }));
    // Defaults need managed identities only for paths that could use an asset
    // exception, or when identity changes themselves affect the staged tree.
    // Keep the existing configured-project path unchanged.
    let needs_managed = config_entry.is_some()
        || managed_touched
        || selected.iter().any(|path| {
            path_findings(path, false, Some(&cfg), &BTreeSet::new())
                .iter()
                .any(|finding| finding.code == "misplaced-test-directory")
        });
    let mut blobs = BTreeSet::new();
    {
        if needs_managed && managed_touched {
            if let Some(entry) = old_managed_entry.filter(|e| e.mode.starts_with("100")) {
                blobs.insert(entry.oid.clone());
            }
        }
        if let Some(entry) = managed_entry.filter(|_| needs_managed) {
            if !entry.mode.starts_with("100") {
                return Err("managed layout identities must be a regular staged file".into());
            }
            blobs.insert(entry.oid.clone());
        }
        for (path, entry) in &index {
            if cargo_affected && manifest_path(path, &cfg) {
                if !entry.mode.starts_with("100") {
                    return Err(format!("Cargo layout manifest must be regular: {path}"));
                }
                blobs.insert(entry.oid.clone());
                if let Some(previous) = old.get(path) {
                    blobs.insert(previous.oid.clone());
                }
            }
        }
    }
    let data = batch(root, runner, &blobs)?;
    let managed = managed_paths(
        managed_entry
            .filter(|_| needs_managed)
            .map(|e| data[&e.oid].as_slice()),
    )?;
    let previous_managed = if managed_touched {
        managed_paths(
            old_managed_entry
                .filter(|e| e.mode.starts_with("100"))
                .map(|e| data[&e.oid].as_slice()),
        )?
    } else {
        managed.clone()
    };
    selected.extend(
        index
            .keys()
            .filter(|path| previous_managed.contains(&key(path)) && !managed.contains(&key(path)))
            .cloned(),
    );
    for path in &selected {
        if !index.contains_key(path) {
            continue;
        }
        let mut findings = path_findings(path, false, Some(&cfg), &managed);
        if artifact_paths::local(path) {
            findings.push(finding(
                "tracked-local-artifact",
                path,
                "local artifacts must not be tracked; preserve local files when removing tracking",
            ));
        }
        if !current.contains_key(path)
            && let Some(previous_cfg) = &previous_cfg
        {
            let previous = path_findings(path, false, Some(previous_cfg), &previous_managed);
            findings.retain(|f| {
                !previous
                    .iter()
                    .any(|old| old.code == f.code && old.path == f.path)
            });
        }
        report.errors.extend(findings);
    }
    {
        let cfg = &cfg;
        for (path, entry) in &index {
            if !cargo_affected || !manifest_path(path, cfg) {
                continue;
            }
            // Unchanged historical manifest violations are filtered by reference identity.
            let before = if let Some(old) = old.get(path) {
                data.get(&old.oid).map(Vec::as_slice)
            } else if changed.contains(path) {
                None
            } else {
                data.get(&entry.oid).map(Vec::as_slice)
            };
            let findings = check_manifest(
                path,
                &data[&entry.oid],
                cfg,
                before,
                &changed,
                false,
                &tree,
                previous_cfg.as_ref(),
            );
            report.errors.extend(findings);
        }
    }
    Ok(report)
}
pub fn pre_commit(root: &Path) -> StructureReport {
    staged(root, &SystemProcessRunner)
}

/// Advisory-only post-edit check. Reads only policy, managed identities and the edited file.
pub fn edited(root: &Path, path: &str) -> StructureReport {
    let result = (|| -> Result<StructureReport, String> {
        let path = repository_relative(path)?;
        let cfg = effective_config(safe_file(root, CONFIG)?.as_deref())?;
        let managed = managed_paths(safe_file(root, MANAGED)?.as_deref())?;
        let mut report = StructureReport::default();
        if key(&path) == ".gitignore" || key(&path) == key(CONFIG) || artifact_paths::local(&path) {
            report
                .advisories
                .extend(ignore_findings(safe_file(root, ".gitignore")?.as_deref())?);
        }
        report
            .advisories
            .extend(path_findings(&path, false, Some(&cfg), &managed));
        if manifest_path(&path, &cfg) {
            if let Some(bytes) = safe_file(root, &path)? {
                // Editing remains local: globs are fully checked against the
                // staged tree at submission and the worktree at manual audit.
                report.advisories.extend(check_manifest(
                    &path,
                    &bytes,
                    &cfg,
                    None,
                    &BTreeSet::new(),
                    true,
                    &BTreeSet::new(),
                    None,
                ));
            }
        }
        Ok(report)
    })();
    match result {
        Ok(report) => report,
        Err(e) => StructureReport {
            errors: vec![],
            advisories: vec![finding("artifact-layout-incomplete", path, e)],
        },
    }
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_artifact_layout.rs"]
mod tests;
