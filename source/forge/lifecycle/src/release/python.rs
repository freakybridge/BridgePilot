//! Python distribution adapter. Discovery is explicit and never executes a backend.
use super::model::{Language, LanguageSnapshot, NativeDependency, NativePackage, Writes};
use super::{python_dependencies as deps, python_metadata as metadata};
use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub projects: Vec<String>,
    #[serde(default)]
    pub version_sources: BTreeMap<String, VersionSource>,
    /// Owner manifest -> normalized distribution name -> target manifest.
    #[serde(default)]
    pub local_sources: BTreeMap<String, BTreeMap<String, String>>,
    /// Explicit import paths, relative to the repository, for shared-layout ownership.
    #[serde(default)]
    pub import_roots: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub locks: Vec<super::python_lock::LockSpec>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VersionSource {
    pub path: String,
    /// text, toml (dotted key), or python (single literal assignment).
    pub kind: String,
    #[serde(default)]
    pub key: Option<String>,
}

pub(super) fn validate_version(raw: &str) -> Result<(), String> {
    pep440_rs::Version::from_str(raw)
        .map(|_| ())
        .map_err(|e| format!("Invalid PEP 440 version '{raw}': {e}"))
}

pub(super) fn validate_increase(before: &str, after: &str) -> Result<(), String> {
    let before = pep440_rs::Version::from_str(before)
        .map_err(|e| format!("Invalid Python previous version: {e}"))?;
    let after = pep440_rs::Version::from_str(after)
        .map_err(|e| format!("Invalid Python target version: {e}"))?;
    if after <= before {
        return Err(format!(
            "Python target version {after} must exceed previous version {before}"
        ));
    }
    Ok(())
}

pub(super) fn bump_version(raw: &str, level: u8) -> Result<String, String> {
    let parsed =
        pep440_rs::Version::from_str(raw).map_err(|e| format!("Invalid Python version: {e}"))?;
    if parsed.epoch() != 0
        || parsed.is_pre()
        || parsed.is_dev()
        || parsed.is_post()
        || parsed.is_local()
        || parsed.release().len() != 3
        || parsed.to_string() != raw
    {
        return Err(format!(
            "Python version {raw} requires an explicit target; automatic bump supports canonical stable three-component versions"
        ));
    }
    let mut version = parsed.release().to_vec();
    let index = match level {
        1 => 2,
        2 => 1,
        3 => 0,
        _ => return Err(format!("Invalid Python bump level: {level}")),
    };
    version[index] = version[index]
        .checked_add(1)
        .ok_or("Python version overflow")?;
    for part in &mut version[index + 1..] {
        *part = 0;
    }
    Ok(version
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join("."))
}

pub(super) fn normalize_path(base: &str, raw: &str) -> Result<String, String> {
    if raw.is_empty() || raw.starts_with(['/', '\\']) || raw.contains(':') || raw.contains('\0') {
        return Err(format!("Python path must stay repository-relative: {raw}"));
    }
    let raw = raw.replace('\\', "/");
    let mut parts = base
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .map(str::to_string)
        .collect::<Vec<_>>();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!("Python path escapes repository: {raw}"));
                }
            }
            other => {
                if matches!(
                    other.to_ascii_lowercase().as_str(),
                    ".git" | ".runtime" | ".deps" | ".build" | ".venv" | "target"
                ) || other.ends_with(['.', ' '])
                {
                    return Err(format!(
                        "Python release input uses a protected/generated or noncanonical directory: {raw}"
                    ));
                }
                parts.push(other.into());
            }
        }
    }
    Ok(parts.join("/"))
}

fn directory(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("")
}

fn validate_config(cfg: &Config) -> Result<(), String> {
    let mut projects = BTreeSet::new();
    let mut directories = BTreeSet::new();
    for path in &cfg.projects {
        let normalized = normalize_path("", path)?;
        if &normalized != path
            || !(path == "pyproject.toml"
                || path == "setup.cfg"
                || path.ends_with("/pyproject.toml")
                || path.ends_with("/setup.cfg"))
        {
            return Err(format!(
                "Python projects must use canonical explicit pyproject.toml/setup.cfg paths: {path}"
            ));
        }
        if !projects.insert(path.clone()) || !directories.insert(directory(path).to_string()) {
            return Err(format!(
                "Duplicate Python project/distribution directory: {path}"
            ));
        }
    }
    for path in cfg
        .version_sources
        .keys()
        .chain(cfg.local_sources.keys())
        .chain(cfg.import_roots.keys())
    {
        if !projects.contains(path) {
            return Err(format!(
                "Python configuration references an undeclared project: {path}"
            ));
        }
    }
    let mut version_owners = BTreeMap::new();
    let mut source_formats = BTreeMap::new();
    for (manifest, source) in &cfg.version_sources {
        let canonical = source.path.to_lowercase();
        if let Some((path, kind)) = source_formats.insert(canonical, (&source.path, &source.kind)) {
            if path != &source.path {
                return Err(format!(
                    "Python version source paths cannot differ only by case: {path} and {}",
                    source.path
                ));
            }
            if kind != &source.kind {
                return Err(format!(
                    "Python version source cannot use multiple parser kinds for the same file: {} ({kind} and {})",
                    source.path, source.kind
                ));
            }
        }
        let identity = (&source.path, &source.kind, &source.key);
        if let Some(previous) = version_owners.insert(identity, manifest) {
            return Err(format!(
                "Independent Python packages {previous} and {manifest} cannot share the same version fact source: {}",
                source.path
            ));
        }
    }
    for (owner, sources) in &cfg.local_sources {
        let mut names = BTreeSet::new();
        for (name, target) in sources {
            if !names.insert(deps::normalize_name(name)?) {
                return Err(format!(
                    "Duplicate normalized Python local source in {owner}: {name}"
                ));
            }
            if !projects.contains(target) {
                return Err(format!(
                    "Python local source references undeclared target {target} from {owner}"
                ));
            }
        }
    }
    Ok(())
}

fn input(
    snapshot: &mut LanguageSnapshot,
    path: &str,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<Vec<u8>, String> {
    if let Some(value) = snapshot.inputs.get(path) {
        return value
            .clone()
            .ok_or_else(|| format!("Missing Python release input: {path}"));
    }
    let bytes = read(path)?;
    snapshot.inputs.insert(path.to_string(), bytes.clone());
    bytes.ok_or_else(|| format!("Missing Python release input: {path}"))
}

fn version_source(source: &VersionSource, bytes: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| format!("Python version source is not UTF-8: {}", source.path))?;
    match source.kind.as_str() {
        "text" => {
            if source.key.is_some() {
                return Err("text Python version source cannot have a key".into());
            }
            let value = text.trim();
            validate_version(value)?;
            Ok(value.into())
        }
        "toml" => {
            let key = source
                .key
                .as_ref()
                .ok_or("toml Python version source requires key")?;
            let doc = metadata::document(bytes, &source.path)?;
            let mut item = doc.as_item();
            for part in key.split('.') {
                item = item
                    .get(part)
                    .ok_or_else(|| format!("Missing Python version key {key}: {}", source.path))?;
            }
            let value = item
                .as_str()
                .ok_or("Python TOML version must be a string")?;
            validate_version(value)?;
            Ok(value.into())
        }
        "python" => {
            let key = source
                .key
                .as_ref()
                .ok_or("python version source requires assignment key")?;
            let static_assignment=Regex::new(r#"^[A-Za-z_][A-Za-z0-9_]*[ \t]*=[ \t]*(?:"[^"\\\r\n]*"|'[^'\\\r\n]*')[ \t]*(?:\#.*)?\r?$"#).unwrap();
            if text.lines().any(|line| {
                !line.trim().is_empty()
                    && !line.trim_start().starts_with('#')
                    && !static_assignment.is_match(line)
            }) {
                return Err(format!(
                    "Python version source must be a dedicated static file of literal string assignments; executable statements are unsupported: {}",
                    source.path
                ));
            }
            if !Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$")
                .unwrap()
                .is_match(key)
            {
                return Err("Unsafe Python version assignment key".into());
            }
            let re=Regex::new(&format!(r#"(?m)^({}[ \t]*=[ \t]*)(?:"([^"\r\n]*)"|'([^'\r\n]*)')([ \t]*(?:\#[^\r\n]*)?\r?)$"#,regex::escape(key))).unwrap();
            let matches = re.captures_iter(text).collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(format!(
                    "Python version source requires exactly one literal assignment {key}: {}",
                    source.path
                ));
            }
            // Reject any other assignment to this name, including computed ones.
            let assignment =
                Regex::new(&format!(r"(?m)^\s*{}\s*(?::[^=\n]+)?=", regex::escape(key))).unwrap();
            if assignment.find_iter(text).count() != 1 {
                return Err(format!(
                    "Ambiguous Python version assignment {key}: {}",
                    source.path
                ));
            }
            let value = matches[0]
                .get(2)
                .or_else(|| matches[0].get(3))
                .unwrap()
                .as_str();
            validate_version(value)?;
            Ok(value.into())
        }
        other => Err(format!(
            "Unsupported safe Python version source kind: {other}"
        )),
    }
}

fn render_source(source: &VersionSource, bytes: &[u8], version: &str) -> Result<Vec<u8>, String> {
    version_source(source, bytes)?;
    let text = std::str::from_utf8(bytes).map_err(|_| "Invalid Python version source UTF-8")?;
    match source.kind.as_str() {
        "text" => {
            let value = text.trim();
            let offset = text.find(value).ok_or("Missing version text")?;
            let mut result = text.to_string();
            result.replace_range(offset..offset + value.len(), version);
            Ok(result.into_bytes())
        }
        "toml" => {
            let mut doc = metadata::document(bytes, &source.path)?;
            let mut item = doc.as_item_mut();
            for part in source.key.as_ref().unwrap().split('.') {
                item = item
                    .get_mut(part)
                    .ok_or("Python version source key disappeared")?;
            }
            *item = toml_edit::value(version);
            Ok(doc.to_string().into_bytes())
        }
        "python" => {
            let key = source.key.as_ref().unwrap();
            let re=Regex::new(&format!(r#"(?m)^({}[ \t]*=[ \t]*)(?:"([^"\r\n]*)"|'([^'\r\n]*)')([ \t]*(?:\#[^\r\n]*)?\r?)$"#,regex::escape(key))).unwrap();
            let captures = re
                .captures(text)
                .ok_or("Python version assignment disappeared")?;
            let value = captures.get(2).or_else(|| captures.get(3)).unwrap();
            let mut result = text.to_string();
            result.replace_range(value.start()..value.end(), version);
            Ok(result.into_bytes())
        }
        _ => Err("Unsupported Python version source".into()),
    }
}

pub(super) fn snapshot(
    cfg: &Config,
    paths: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<LanguageSnapshot, String> {
    validate_config(cfg)?;
    let mut snapshot = LanguageSnapshot::default();
    let mut parsed = BTreeMap::new();
    let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in &cfg.projects {
        let bytes = input(&mut snapshot, path, read)?;
        if path.ends_with("setup.cfg") {
            let shim = normalize_path(directory(path), "setup.py")?;
            if paths.contains(&shim) {
                let shim_bytes = input(&mut snapshot, &shim, read)?;
                let shim_text = std::str::from_utf8(&shim_bytes)
                    .map_err(|_| format!("setup.py is not UTF-8: {shim}"))?;
                let statements = shim_text
                    .lines()
                    .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
                    .map(str::trim_end)
                    .collect::<Vec<_>>();
                let from =
                    Regex::new(r"^from[ \t]+setuptools[ \t]+import[ \t]+setup[ \t]*$").unwrap();
                let call = Regex::new(r"^setup[ \t]*\([ \t]*\)[ \t]*$").unwrap();
                let import = Regex::new(r"^import[ \t]+setuptools[ \t]*$").unwrap();
                let qualified = Regex::new(r"^setuptools\.setup[ \t]*\([ \t]*\)[ \t]*$").unwrap();
                if statements.len() != 2
                    || !(from.is_match(statements[0]) && call.is_match(statements[1])
                        || import.is_match(statements[0]) && qualified.is_match(statements[1]))
                {
                    return Err(format!(
                        "Dynamic setup.py is unsupported for static setup.cfg release; only an argument-free setuptools shim is accepted: {shim}"
                    ));
                }
            }
        }
        let info = metadata::parse(&bytes, path)?;
        let version = if let Some(source) = cfg.version_sources.get(path) {
            if info.version.is_some() {
                return Err(format!(
                    "Python project {path} has both a static version and an external version source; select one fact source"
                ));
            }
            let source_path = normalize_path("", &source.path)?;
            if source_path != source.path {
                return Err(format!(
                    "Python version source must use canonical path: {}",
                    source.path
                ));
            }
            if !paths.contains(&source.path) {
                return Err(format!(
                    "Python version source is outside the exact Git inventory (including path casing): {}",
                    source.path
                ));
            }
            let bytes = input(&mut snapshot, &source.path, read)?;
            version_source(source, &bytes)?
        } else {
            info.version.clone().ok_or_else(||format!("Python project {path} needs a static version or explicit safe version_sources entry; dynamic setup.py/SCM execution is unsupported"))?
        };
        validate_version(&version)?;
        let mut roots = if let Some(roots) = cfg.import_roots.get(path) {
            roots.clone()
        } else {
            info.roots
                .iter()
                .map(|root| normalize_path(directory(path), root))
                .collect::<Result<Vec<_>, _>>()?
        };
        if cfg.import_roots.contains_key(path) {
            for root in &roots {
                if normalize_path("", root)? != *root {
                    return Err(format!("Python import root must be canonical: {root}"));
                }
            }
        }
        // A distribution also owns its project tests/configuration, not only
        // the import subtree. Explicit roots add shared-layout source locations.
        roots.push(directory(path).to_string());
        if let Some(source) = cfg.version_sources.get(path) {
            roots.push(source.path.clone());
        }
        roots.sort();
        roots.dedup();
        names
            .entry(info.name.clone())
            .or_default()
            .push(path.clone());
        snapshot.roots.insert(directory(path).to_string());
        snapshot.packages.insert(
            path.clone(),
            NativePackage {
                manifest: path.clone(),
                name: info.name.clone(),
                version,
                language: Language::Python,
                workspace: directory(path).into(),
                source_roots: roots,
            },
        );
        parsed.insert(path.clone(), info);
    }
    for (owner, info) in &parsed {
        if let Some(sources) = cfg.local_sources.get(owner) {
            for name in sources.keys() {
                let normalized = deps::normalize_name(name)?;
                if !info
                    .dependencies
                    .iter()
                    .any(|dependency| dependency.name == normalized)
                {
                    return Err(format!(
                        "Python local_sources entry {name} in {owner} has no declared dependency"
                    ));
                }
            }
        }
        for dependency in &info.dependencies {
            if let Some(target) = resolve_target(cfg, &snapshot, &names, owner, dependency)? {
                let requirement = match dependency.slot {
                    metadata::Slot::Poetry(_) => dependency.requirement.clone(),
                    _ => {
                        let requirement = deps::parse(&dependency.requirement)?;
                        if requirement.direct {
                            String::new()
                        } else {
                            requirement.specifier
                        }
                    }
                };
                if !deps::accepts(&requirement, &snapshot.packages[&target].version)? {
                    return Err(format!(
                        "Python local dependency {} in {owner} excludes current target version {}",
                        dependency.name, snapshot.packages[&target].version
                    ));
                }
                snapshot.dependencies.push(NativeDependency {
                    owner: owner.clone(),
                    target,
                    kind: dependency.kind.clone(),
                });
            }
        }
    }
    snapshot
        .dependencies
        .sort_by(|a, b| (&a.owner, &a.target, &a.kind).cmp(&(&b.owner, &b.target, &b.kind)));
    snapshot.dependencies.dedup();
    for spec in &cfg.locks {
        input(&mut snapshot, &spec.path, read)?;
        for extra in super::python_lock::additional_inputs(spec)? {
            input(&mut snapshot, &extra, read)?;
        }
    }
    super::python_lock::validate(&cfg.locks, &snapshot)?;
    Ok(snapshot)
}

fn resolve_target(
    cfg: &Config,
    snapshot: &LanguageSnapshot,
    names: &BTreeMap<String, Vec<String>>,
    owner: &str,
    dependency: &metadata::Dependency,
) -> Result<Option<String>, String> {
    let explicit = cfg
        .local_sources
        .get(owner)
        .map(|sources| {
            sources
                .iter()
                .filter_map(|(name, target)| {
                    deps::normalize_name(name)
                        .ok()
                        .filter(|name| name == &dependency.name)
                        .map(|_| target.clone())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut candidates = explicit;
    if let Some(path) = &dependency.local_path {
        let root = normalize_path(directory(owner), path)?;
        let matches = snapshot
            .packages
            .keys()
            .filter(|manifest| directory(manifest) == root || manifest.as_str() == root)
            .cloned()
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "Python path dependency {} in {owner} does not resolve to one declared project: {path}",
                dependency.name
            ));
        }
        candidates.extend(matches);
    }
    if dependency.workspace {
        let matches = names.get(&dependency.name).cloned().unwrap_or_default();
        if matches.len() != 1 {
            return Err(format!(
                "Python workspace dependency {} in {owner} does not resolve uniquely",
                dependency.name
            ));
        }
        candidates.extend(matches);
    }
    // PEP 508 file URLs are explicit local sources; arbitrary URL/VCS artifacts
    // remain external even when their distribution name matches a local project.
    if matches!(
        dependency.slot,
        metadata::Slot::Array(..) | metadata::Slot::Ini(..)
    ) {
        let requirement = deps::parse(&dependency.requirement)?;
        if requirement.direct {
            let local = requirement
                .specifier
                .strip_prefix("file:///${PROJECT_ROOT}/")
                .or_else(|| requirement.specifier.strip_prefix("{root:uri}/"))
                .or_else(|| requirement.specifier.strip_prefix("file:"));
            if let Some(raw) = local {
                if raw.starts_with('/') || raw.contains(['%', '$', '{', '}', '?', '#']) {
                    return Err(format!(
                        "Absolute/encoded Python file URLs need an explicit supported repository source: {}",
                        dependency.requirement
                    ));
                }
                let root = normalize_path(directory(owner), raw)?;
                let matches = snapshot
                    .packages
                    .keys()
                    .filter(|manifest| directory(manifest) == root || manifest.as_str() == root)
                    .cloned()
                    .collect::<Vec<_>>();
                if matches.len() != 1 {
                    return Err(format!(
                        "Python file source is not one declared project: {}",
                        dependency.requirement
                    ));
                }
                candidates.extend(matches);
            } else if !candidates.is_empty() {
                return Err(format!(
                    "External artifact/VCS requirement cannot be remapped as a local Python dependency: {}",
                    dependency.requirement
                ));
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    if dependency.external
        && !candidates.is_empty()
        && matches!(dependency.slot, metadata::Slot::Poetry(..))
    {
        return Err(format!(
            "External Poetry artifact/VCS source cannot be remapped to a local distribution: {} in {owner}",
            dependency.name
        ));
    }
    if candidates.len() > 1 {
        return Err(format!(
            "Conflicting Python local sources for {} in {owner}",
            dependency.name
        ));
    }
    let Some(target) = candidates.pop() else {
        return Ok(None);
    };
    let package = snapshot
        .packages
        .get(&target)
        .ok_or_else(|| format!("Missing Python local target {target}"))?;
    if package.name != dependency.name {
        return Err(format!(
            "Python dependency name {} does not match local distribution {}: {owner}",
            dependency.name, package.name
        ));
    }
    Ok(Some(target))
}

pub(super) fn render_versions(
    cfg: &Config,
    paths: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    versions: &BTreeMap<String, String>,
) -> Result<Writes, String> {
    let mut snapshot = snapshot(cfg, paths, read)?;
    let mut writes = Writes::new();
    let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (manifest, package) in &snapshot.packages {
        names
            .entry(package.name.clone())
            .or_default()
            .push(manifest.clone());
    }
    for (manifest, package) in &snapshot.packages {
        if let Some(target) = versions.get(manifest) {
            validate_version(target)?;
            if target != &package.version {
                validate_increase(&package.version, target)?;
            }
        }
        let bytes = snapshot.inputs[manifest]
            .as_ref()
            .ok_or("Python manifest disappeared")?;
        let info = metadata::parse(bytes, manifest)?;
        let mut replacements = Vec::new();
        for dependency in &info.dependencies {
            let Some(target) = resolve_target(cfg, &snapshot, &names, manifest, dependency)? else {
                continue;
            };
            let Some(version) = versions.get(&target) else {
                continue;
            };
            let before = &snapshot.packages[&target].version;
            let updated = match dependency.slot {
                metadata::Slot::Poetry(_) => {
                    deps::update_constraint(&dependency.requirement, before, version)?
                }
                _ => deps::update_requirement(&dependency.requirement, before, version)?,
            };
            if updated != dependency.requirement {
                replacements.push((dependency.slot.clone(), updated));
            }
        }
        let version = versions.get(manifest).filter(|v| *v != &package.version);
        let native_version = if cfg.version_sources.contains_key(manifest) {
            None
        } else {
            version.map(String::as_str)
        };
        if native_version.is_some() || !replacements.is_empty() {
            let rendered = metadata::render(bytes, manifest, &info, native_version, &replacements)?;
            if &rendered != bytes {
                writes.insert(manifest.clone(), rendered);
            }
        }
    }
    let mut source_versions = BTreeMap::new();
    for (manifest, source) in &cfg.version_sources {
        let Some(version) = versions
            .get(manifest)
            .filter(|version| *version != &snapshot.packages[manifest].version)
        else {
            continue;
        };
        let identity = (source.path.clone(), source.kind.clone(), source.key.clone());
        if let Some(previous) = source_versions.insert(identity, version.clone()) {
            if previous != *version {
                return Err(format!(
                    "Conflicting Python version source targets: {}",
                    source.path
                ));
            }
        }
        let bytes = writes
            .get(&source.path)
            .or_else(|| snapshot.inputs.get(&source.path).and_then(Option::as_ref))
            .ok_or("Python version source disappeared")?;
        let rendered = render_source(source, bytes, version)?;
        writes.insert(source.path.clone(), rendered);
    }
    for (path, bytes) in &writes {
        snapshot.inputs.insert(path.clone(), Some(bytes.clone()));
    }
    for (path, bytes) in super::python_lock::render(&cfg.locks, &snapshot, versions)? {
        if let Some(existing) = writes.insert(path.clone(), bytes.clone()) {
            if existing != bytes {
                return Err(format!("Conflicting Python metadata/lock writes: {path}"));
            }
        }
    }
    Ok(writes)
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../../scripts/tests/unit/core_release_python.rs"]
mod tests;
