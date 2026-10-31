//! Python lock providers. They never resolve, install, execute a backend, or invent
//! artifact hashes: a refactor must supply the current source topology first.
//!
//! Supported wire formats are deliberately bounded. Hash algorithms follow Poetry
//! 2.1's Locker._get_content_hash and PDM's legacy PyProject.content_hash. Native
//! freshness checks belong to the factory fixtures, not release preview.
use super::model::{Language, LanguageSnapshot, NativePackage, Writes};
use serde::Deserialize;
use serde_json::{Map, Value as Json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use toml_edit::{DocumentMut, Item, Table, Value};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LockSpec {
    pub(super) path: String,
    /// uv, poetry, pdm, pylock, or requirements-export.
    pub(super) format: String,
    #[serde(default)]
    pub(super) members: Vec<String>,
    /// Root pyproject.toml for Poetry/PDM freshness metadata.
    #[serde(default)]
    pub(super) metadata: Option<String>,
    /// Producer version is recorded for diagnostics; format gates remain mandatory.
    #[serde(default)]
    pub(super) tool_version: Option<String>,
}

pub(super) fn additional_inputs(spec: &LockSpec) -> Result<Vec<String>, String> {
    canonical(&spec.path)?;
    let mut paths = vec![];
    if matches!(spec.format.as_str(), "poetry" | "pdm") {
        paths.push(metadata_path(spec)?);
    } else if spec.metadata.is_some() {
        return Err(format!(
            "{}: metadata is only supported for poetry/pdm",
            spec.path
        ));
    }
    Ok(paths)
}

pub(super) fn validate(specs: &[LockSpec], snapshot: &LanguageSnapshot) -> Result<(), String> {
    let mut paths = BTreeSet::new();
    for spec in specs {
        canonical(&spec.path)?;
        additional_inputs(spec)?;
        if !paths.insert(&spec.path) {
            return Err(format!("Duplicate Python lock path: {}", spec.path));
        }
        let members = members(spec, snapshot)?;
        if spec.format == "requirements-export" {
            validate_requirements(spec, snapshot, &members)?;
            continue;
        }
        let doc = document(snapshot, &spec.path)?;
        let entries = validate_document(spec, snapshot, &doc, &members)?;
        validate_edges(spec, snapshot, &doc, &entries, &members)?;
        validate_freshness(spec, snapshot, &doc)?;
    }
    Ok(())
}

pub(super) fn render(
    specs: &[LockSpec],
    snapshot: &LanguageSnapshot,
    versions: &BTreeMap<String, String>,
) -> Result<Writes, String> {
    let mut writes = Writes::new();
    let mut paths = BTreeSet::new();
    for spec in specs {
        canonical(&spec.path)?;
        additional_inputs(spec)?;
        if !paths.insert(&spec.path) {
            return Err(format!("Duplicate Python lock path: {}", spec.path));
        }
        let members = members(spec, snapshot)?;
        if spec.format == "requirements-export" {
            // Editable paths have no embedded package version. Registry pins are
            // immutable here; updating one would require the new artifact hashes.
            validate_requirements(spec, snapshot, &members)?;
            continue;
        }
        let mut doc = document(snapshot, &spec.path)?;
        let entries = validate_document(spec, snapshot, &doc, &members)?;
        validate_edges(spec, snapshot, &doc, &entries, &members)?;
        let key = package_key(spec)?;
        if let Some(packages) = doc.get_mut(key).and_then(Item::as_array_of_tables_mut) {
            for (index, entry) in packages.iter_mut().enumerate() {
                if let Some(manifest) = entries.get(&index) {
                    let package = &snapshot.packages[manifest];
                    if let Some(version) = versions.get(manifest) {
                        if spec.format != "pylock" {
                            entry["version"] = toml_edit::value(version);
                        }
                    }
                    update_entry_dependencies(
                        spec, entry, package, snapshot, versions, &members, &entries,
                    )?;
                } else {
                    // Registry packages can depend on local sources through a
                    // configured override. Resolve every edge by source identity.
                    update_entry_dependencies(
                        spec,
                        entry,
                        &NativePackage {
                            manifest: String::new(),
                            name: String::new(),
                            version: String::new(),
                            language: Language::Python,
                            workspace: String::new(),
                            source_roots: vec![],
                        },
                        snapshot,
                        versions,
                        &members,
                        &entries,
                    )?;
                }
            }
        }
        match spec.format.as_str() {
            "poetry" => {
                doc["metadata"]["content-hash"] = toml_edit::value(content_hash(spec, snapshot)?)
            }
            "pdm" => {
                doc["metadata"]["content_hash"] =
                    toml_edit::value(format!("sha256:{}", content_hash(spec, snapshot)?))
            }
            _ => (),
        }
        let rendered = doc.to_string().into_bytes();
        if bytes(snapshot, &spec.path)? != rendered {
            if writes.insert(spec.path.clone(), rendered).is_some() {
                return Err(format!("Conflicting Python lock writers: {}", spec.path));
            }
        }
    }
    Ok(writes)
}

fn bytes<'a>(snapshot: &'a LanguageSnapshot, path: &str) -> Result<&'a [u8], String> {
    snapshot
        .inputs
        .get(path)
        .and_then(Option::as_deref)
        .ok_or_else(|| format!("Required Python lock input missing: {path}"))
}

fn document(snapshot: &LanguageSnapshot, path: &str) -> Result<DocumentMut, String> {
    let raw = std::str::from_utf8(bytes(snapshot, path)?).map_err(|e| format!("{path}: {e}"))?;
    raw.parse()
        .map_err(|e| format!("Invalid Python TOML {path}: {e}"))
}

fn canonical(path: &str) -> Result<(), String> {
    let normalized = resolve("", path)?;
    if normalized.is_empty() || normalized != path {
        return Err(format!(
            "Python lock input must be canonical repository-relative path: {path}"
        ));
    }
    Ok(())
}

fn directory(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Lexical resolution is platform independent. Sources outside the repository
/// cannot be included in a guarded release snapshot.
fn resolve(base: &str, path: &str) -> Result<String, String> {
    if path.is_empty() || path.starts_with(['/', '\\']) || path.contains([':', '\\', '\0']) {
        return Err(format!("Unsupported Python local source path: {path}"));
    }
    let joined = if base.is_empty() {
        path.to_owned()
    } else {
        format!("{base}/{path}")
    };
    let mut parts = vec![];
    for part in joined.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!("Python local source escapes repository: {path}"));
                }
            }
            other => parts.push(other),
        }
    }
    Ok(parts.join("/"))
}

fn metadata_path(spec: &LockSpec) -> Result<String, String> {
    let result = spec.metadata.clone().unwrap_or_else(|| {
        let parent = directory(&spec.path);
        if parent.is_empty() {
            "pyproject.toml".into()
        } else {
            format!("{parent}/pyproject.toml")
        }
    });
    canonical(&result)?;
    if !result.ends_with("pyproject.toml") {
        return Err(format!(
            "{}: hash metadata must be pyproject.toml",
            spec.path
        ));
    }
    Ok(result)
}

fn normalize_name(raw: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for ch in raw.chars() {
        if matches!(ch, '-' | '_' | '.') {
            if !separator {
                result.push('-');
            }
            separator = true;
        } else {
            result.push(ch.to_ascii_lowercase());
            separator = false;
        }
    }
    result
}

fn members<'a>(
    spec: &LockSpec,
    snapshot: &'a LanguageSnapshot,
) -> Result<BTreeMap<String, &'a NativePackage>, String> {
    let mut result = BTreeMap::new();
    for manifest in &spec.members {
        canonical(manifest)?;
        let package = snapshot.packages.get(manifest).ok_or_else(|| {
            format!(
                "{}: lock member is not a current package: {manifest}",
                spec.path
            )
        })?;
        if package.language != Language::Python {
            return Err(format!(
                "{}: lock member is not Python: {manifest}",
                spec.path
            ));
        }
        if result.insert(manifest.clone(), package).is_some() {
            return Err(format!("{}: duplicate lock member: {manifest}", spec.path));
        }
    }
    for dependency in &snapshot.dependencies {
        if result.contains_key(&dependency.owner)
            && dependency.kind != "build"
            && snapshot
                .packages
                .get(&dependency.target)
                .is_some_and(|p| p.language == Language::Python)
            && !result.contains_key(&dependency.target)
        {
            return Err(format!(
                "{}: lock members omit required local dependency {} -> {} ({}); declare the complete local closure",
                spec.path, dependency.owner, dependency.target, dependency.kind
            ));
        }
    }
    if matches!(spec.format.as_str(), "poetry" | "pdm") {
        let root = metadata_path(spec)?;
        if snapshot
            .packages
            .get(&root)
            .is_some_and(|p| p.language == Language::Python)
            && !result.contains_key(&root)
        {
            return Err(format!(
                "{}: lock members omit the metadata root project {root}",
                spec.path
            ));
        }
    }
    Ok(result)
}

fn package_key(spec: &LockSpec) -> Result<&'static str, String> {
    match spec.format.as_str() {
        "uv" | "poetry" | "pdm" => Ok("package"),
        "pylock" => Ok("packages"),
        other => Err(format!(
            "Unsupported Python lock provider {other}: {}",
            spec.path
        )),
    }
}

fn get_str<'a>(item: &'a Item, path: &[&str]) -> Option<&'a str> {
    let mut value = item;
    for key in path {
        value = value.get(key)?;
    }
    value.as_str()
}

fn check_format(spec: &LockSpec, doc: &DocumentMut) -> Result<(), String> {
    let valid = match spec.format.as_str() {
        "uv" => {
            doc.get("version").and_then(Item::as_integer) == Some(1)
                && doc
                    .get("revision")
                    .is_none_or(|v| v.as_integer().is_some_and(|v| (0..=5).contains(&v)))
        }
        "poetry" => matches!(
            get_str(doc.as_item(), &["metadata", "lock-version"]),
            Some("2.1")
        ),
        "pdm" => {
            matches!(
                get_str(doc.as_item(), &["metadata", "lock_version"]),
                Some("4.5.0" | "4.5.1")
            ) && doc
                .get("metadata")
                .and_then(|v| v.get("lock_inputs"))
                .is_none()
                && doc
                    .get("metadata")
                    .and_then(|v| v.get("lock-inputs"))
                    .is_none()
        }
        "pylock" => {
            doc.get("lock-version").and_then(Item::as_str) == Some("1.0")
                && doc
                    .get("created-by")
                    .and_then(Item::as_str)
                    .is_some_and(|v| !v.is_empty())
        }
        _ => false,
    };
    if !valid {
        return Err(format!(
            "{}: unsupported {} lock format/revision; regenerate with a supported producer",
            spec.path, spec.format
        ));
    }
    if spec
        .tool_version
        .as_ref()
        .is_some_and(|v| v.trim().is_empty())
    {
        return Err(format!("{}: empty tool_version", spec.path));
    }
    if spec.format == "pdm" {
        let input = has_canonical_pdm_inputs(doc);
        if input {
            return Err(format!(
                "{}: PDM canonical lock inputs require a newer provider",
                spec.path
            ));
        }
    }
    Ok(())
}

fn has_canonical_pdm_inputs(doc: &DocumentMut) -> bool {
    // Never refresh only the legacy hash when a newer producer also fingerprints
    // local source files / canonical lock inputs.
    ["lock-inputs", "lock_inputs"]
        .iter()
        .any(|key| doc.get(key).is_some())
}

fn local_directory(spec: &LockSpec, entry: &Table) -> Result<Option<String>, String> {
    let raw = match spec.format.as_str() {
        "uv" => {
            let source = entry.get("source");
            source
                .and_then(|s| {
                    s.get("editable")
                        .or_else(|| s.get("virtual"))
                        .or_else(|| s.get("directory"))
                })
                .and_then(Item::as_str)
        }
        "poetry" => {
            let source = entry.get("source");
            if source.and_then(|s| s.get("type")).and_then(Item::as_str) == Some("directory") {
                Some(
                    source
                        .and_then(|s| s.get("url"))
                        .and_then(Item::as_str)
                        .ok_or_else(|| format!("{}: directory source missing url", spec.path))?,
                )
            } else {
                None
            }
        }
        "pdm" => entry.get("path").and_then(Item::as_str),
        "pylock" => entry
            .get("directory")
            .and_then(|s| s.get("path"))
            .and_then(Item::as_str),
        _ => None,
    };
    raw.map(|raw| {
        let mut path = resolve(directory(&spec.path), raw)?;
        if spec.format == "pylock" {
            if let Some(subdir) = entry
                .get("directory")
                .and_then(|s| s.get("subdirectory"))
                .and_then(Item::as_str)
            {
                path = resolve(&path, subdir)?;
            }
        }
        Ok(path)
    })
    .transpose()
}

fn validate_document(
    spec: &LockSpec,
    snapshot: &LanguageSnapshot,
    doc: &DocumentMut,
    members: &BTreeMap<String, &NativePackage>,
) -> Result<BTreeMap<usize, String>, String> {
    check_format(spec, doc)?;
    let key = package_key(spec)?;
    let mut entries = BTreeMap::new();
    let mut covered = BTreeSet::new();
    let packages = doc.get(key).and_then(Item::as_array_of_tables);
    if packages.is_none()
        && !doc
            .get(key)
            .and_then(Item::as_array)
            .is_some_and(|a| a.is_empty())
        && !members.is_empty()
    {
        return Err(format!("{}: expected [[{key}]] entries", spec.path));
    }
    if let Some(packages) = packages {
        for (index, entry) in packages.iter().enumerate() {
            let name = entry
                .get("name")
                .and_then(Item::as_str)
                .ok_or_else(|| format!("{}: package missing name", spec.path))?;
            if spec.format == "uv" {
                let source = entry
                    .get("source")
                    .and_then(Item::as_table_like)
                    .ok_or_else(|| format!("{}: uv package missing source: {name}", spec.path))?;
                if source.len() != 1 {
                    return Err(format!(
                        "{}: uv package has ambiguous source: {name}",
                        spec.path
                    ));
                }
            }
            if let Some(local) = local_directory(spec, entry)? {
                let matches = members
                    .iter()
                    .filter(|(_, p)| directory(&p.manifest) == local)
                    .collect::<Vec<_>>();
                if matches.len() != 1 {
                    return Err(format!(
                        "{}: stale, undeclared, or ambiguous local source {local} ({name}); regenerate lock for current topology",
                        spec.path
                    ));
                }
                let (manifest, package) = matches[0];
                if normalize_name(name) != normalize_name(&package.name) {
                    return Err(format!(
                        "{}: local source {local} has stale name {name}; expected {}",
                        spec.path, package.name
                    ));
                }
                if !covered.insert(manifest.clone()) {
                    return Err(format!(
                        "{}: duplicate local lock identity {manifest}",
                        spec.path
                    ));
                }
                if spec.format == "pylock" {
                    if entry.contains_key("version") {
                        return Err(format!(
                            "{}: pylock directory source must omit unstable version: {name}",
                            spec.path
                        ));
                    }
                } else if entry.get("version").and_then(Item::as_str) != Some(&package.version) {
                    return Err(format!(
                        "{}: local version differs from manifest: {name}",
                        spec.path
                    ));
                }
                // Source directories have no reusable artifact hash. A mixed
                // directory/artifact record would silently validate stale bytes.
                if ["sdist", "wheels", "archive", "vcs"]
                    .iter()
                    .any(|k| entry.contains_key(k))
                    || entry
                        .get("files")
                        .and_then(Item::as_array)
                        .is_some_and(|a| !a.is_empty())
                {
                    return Err(format!(
                        "{}: local source contains immutable artifact records: {name}",
                        spec.path
                    ));
                }
                entries.insert(index, manifest.clone());
            } else {
                // A declared package referenced as a built artifact, VCS, URL or
                // registry record cannot be version-bumped by relabelling hashes.
                // Same-name third party entries are permitted when a separate
                // explicit directory record identifies the managed package.
                if members
                    .values()
                    .any(|p| normalize_name(&p.name) == normalize_name(name))
                    && entry.get("version").and_then(Item::as_str).is_none()
                {
                    return Err(format!(
                        "{}: non-directory local candidate lacks immutable version: {name}",
                        spec.path
                    ));
                }
            }
        }
    }
    for manifest in members.keys() {
        // Poetry and PDM do not lock the root project itself.
        let root =
            matches!(spec.format.as_str(), "poetry" | "pdm") && metadata_path(spec)? == *manifest;
        if !covered.contains(manifest) && !root {
            return Err(format!(
                "{}: local member {manifest} has no directory lock record; do not relabel registry/VCS/artifact hashes",
                spec.path
            ));
        }
    }
    let _ = snapshot;
    Ok(entries)
}

fn validate_freshness(
    spec: &LockSpec,
    snapshot: &LanguageSnapshot,
    doc: &DocumentMut,
) -> Result<(), String> {
    let expected = match spec.format.as_str() {
        "poetry" => Some(("content-hash", content_hash(spec, snapshot)?)),
        "pdm" => Some((
            "content_hash",
            format!("sha256:{}", content_hash(spec, snapshot)?),
        )),
        _ => None,
    };
    if let Some((key, expected)) = expected {
        if get_str(doc.as_item(), &["metadata", key]) != Some(&expected) {
            return Err(format!(
                "{}: stale {key}; regenerate lock before release",
                spec.path
            ));
        }
    }
    Ok(())
}

fn validate_edges(
    spec: &LockSpec,
    snapshot: &LanguageSnapshot,
    doc: &DocumentMut,
    entries: &BTreeMap<usize, String>,
    members: &BTreeMap<String, &NativePackage>,
) -> Result<(), String> {
    // Exercise all edge identities without changing their values. This catches a
    // retired source in metadata even when no version bump is required.
    if let Some(packages) = doc
        .get(package_key(spec)?)
        .and_then(Item::as_array_of_tables)
    {
        for (index, package) in packages.iter().enumerate() {
            let owner = entries.get(&index).map_or("", String::as_str);
            let mut copy = package.clone();
            let synthetic = NativePackage {
                manifest: owner.into(),
                name: String::new(),
                version: String::new(),
                language: Language::Python,
                workspace: String::new(),
                source_roots: vec![],
            };
            update_entry_dependencies(
                spec,
                &mut copy,
                &synthetic,
                snapshot,
                &BTreeMap::new(),
                members,
                entries,
            )?;
            if spec.format == "uv" {
                validate_uv_edges(spec, package, packages, members, owner, snapshot)?;
            } else if !owner.is_empty() {
                validate_local_edges(spec, package, packages, members, owner, snapshot)?;
            }
        }
    }
    Ok(())
}

/// Root Poetry/PDM requirements are guarded by their content hash and the
/// required target directory records. Dependencies of locked local projects need
/// their own graph evidence: the root hash alone does not fingerprint them.
fn validate_local_edges(
    spec: &LockSpec,
    package: &Table,
    packages: &toml_edit::ArrayOfTables,
    members: &BTreeMap<String, &NativePackage>,
    owner: &str,
    snapshot: &LanguageSnapshot,
) -> Result<(), String> {
    let mut observed = BTreeSet::new();
    let owner_package = members[owner];
    match spec.format.as_str() {
        "poetry" => {
            if let Some(dependencies) = package.get("dependencies").and_then(Item::as_table_like) {
                for (name, constraint) in dependencies.iter() {
                    observed.extend(poetry_locked_targets(
                        spec,
                        name,
                        constraint,
                        owner_package,
                        packages,
                        members,
                    )?);
                }
            }
        }
        "pdm" => {
            if let Some(dependencies) = package.get("dependencies").and_then(Item::as_array) {
                for requirement in dependencies.iter() {
                    let raw = requirement
                        .as_str()
                        .ok_or_else(|| format!("{}: invalid PDM locked requirement", spec.path))?;
                    let requirement = super::python_dependencies::parse(raw)?;
                    if let Some(target) = pdm_locked_target(
                        spec,
                        &requirement.name,
                        &requirement.specifier,
                        requirement.direct,
                        owner_package,
                        packages,
                        members,
                    )? {
                        if requirement.direct {
                            validate_pdm_source(
                                spec,
                                &requirement.specifier,
                                owner_package,
                                members[&target],
                            )?;
                        } else if !super::python_dependencies::accepts(
                            &requirement.specifier,
                            &members[&target].version,
                        )? {
                            return Err(format!("{}: stale PDM local constraint {raw}", spec.path));
                        }
                        observed.insert(target.clone());
                    }
                }
            }
        }
        "pylock" => {
            let dependencies = package
                .get("dependencies")
                .map(dependency_tables)
                .transpose()?
                .unwrap_or_default();
            for edge in dependencies {
                let name = edge
                    .get("name")
                    .and_then(Item::as_str)
                    .ok_or_else(|| format!("{}: pylock dependency lacks name", spec.path))?;
                let candidates = packages
                    .iter()
                    .filter(|candidate| {
                        candidate
                            .get("name")
                            .and_then(Item::as_str)
                            .is_some_and(|n| normalize_name(n) == normalize_name(name))
                            && edge.as_table_like().is_some_and(|edge| {
                                edge.iter().all(|(key, value)| {
                                    key == "name"
                                        || candidate
                                            .get(key)
                                            .and_then(|candidate| json_item(candidate).ok())
                                            == json_item(value).ok()
                                })
                            })
                    })
                    .collect::<Vec<_>>();
                if candidates.len() != 1 {
                    return Err(format!(
                        "{}: ambiguous/stale pylock dependency {name}",
                        spec.path
                    ));
                }
                if let Some(path) = local_directory(spec, candidates[0])? {
                    let target = members
                        .iter()
                        .find(|(_, p)| directory(&p.manifest) == path)
                        .map(|(manifest, _)| manifest)
                        .ok_or_else(|| {
                            format!("{}: undeclared pylock dependency {name}", spec.path)
                        })?;
                    if edge.get("version").is_some() {
                        return Err(format!(
                            "{}: pylock directory dependency must not pin a version: {name}",
                            spec.path
                        ));
                    }
                    observed.insert(target.clone());
                }
            }
        }
        _ => (),
    }
    for dependency in snapshot.dependencies.iter().filter(|dep| {
        dep.owner == owner && dep.kind != "build" && members.contains_key(&dep.target)
    }) {
        if !observed.contains(&dependency.target) {
            return Err(format!(
                "{}: {} lock missing current local dependency {} -> {} ({}); regenerate lock with required groups/extras",
                spec.path, spec.format, owner, dependency.target, dependency.kind
            ));
        }
    }
    validate_local_conditions(spec, package, members, owner, snapshot)?;
    for target in observed {
        if !snapshot
            .dependencies
            .iter()
            .any(|dep| dep.owner == owner && dep.target == target && dep.kind != "build")
        {
            return Err(format!(
                "{}: stale local dependency edge {owner} -> {target}",
                spec.path
            ));
        }
    }
    Ok(())
}

fn named_locked_target(
    spec: &LockSpec,
    name: &str,
    constraint: &str,
    packages: &toml_edit::ArrayOfTables,
    members: &BTreeMap<String, &NativePackage>,
) -> Result<Option<String>, String> {
    let mut candidates = packages
        .iter()
        .filter(|entry| {
            entry
                .get("name")
                .and_then(Item::as_str)
                .is_some_and(|n| normalize_name(n) == normalize_name(name))
        })
        .collect::<Vec<_>>();
    if candidates.len() > 1 {
        let had_local = candidates
            .iter()
            .any(|entry| local_directory(spec, entry).ok().flatten().is_some());
        let mut selected = vec![];
        for entry in &candidates {
            if entry
                .get("version")
                .and_then(Item::as_str)
                .map(|version| super::python_dependencies::accepts(constraint, version))
                .transpose()?
                .unwrap_or(false)
            {
                selected.push(*entry);
            }
        }
        candidates = selected;
        if candidates.is_empty() && had_local {
            return Err(format!(
                "{}: stale local lock dependency {name} has no matching version/source",
                spec.path
            ));
        }
    }
    if candidates.len() > 1 {
        if candidates
            .iter()
            .any(|entry| local_directory(spec, entry).ok().flatten().is_some())
        {
            return Err(format!(
                "{}: ambiguous registry/local lock dependency {name}; source-qualified identity required",
                spec.path
            ));
        }
        return Ok(None);
    }
    let Some(entry) = candidates.first() else {
        return Ok(None);
    };
    if let Some(path) = local_directory(spec, entry)? {
        let target = members
            .iter()
            .find(|(_, p)| {
                directory(&p.manifest) == path && normalize_name(&p.name) == normalize_name(name)
            })
            .map(|(manifest, _)| manifest.clone())
            .ok_or_else(|| format!("{}: undeclared local lock dependency {name}", spec.path))?;
        Ok(Some(target))
    } else {
        Ok(None)
    }
}

fn poetry_locked_targets(
    spec: &LockSpec,
    name: &str,
    constraint: &Item,
    owner: &NativePackage,
    packages: &toml_edit::ArrayOfTables,
    members: &BTreeMap<String, &NativePackage>,
) -> Result<BTreeSet<String>, String> {
    if let Some(variants) = constraint.as_array() {
        let mut targets = BTreeSet::new();
        for variant in variants.iter() {
            targets.extend(poetry_locked_targets(
                spec,
                name,
                &Item::Value(variant.clone()),
                owner,
                packages,
                members,
            )?);
        }
        return Ok(targets);
    }
    let table = constraint.as_table_like();
    if table.is_some_and(|table| {
        ["git", "url", "source"]
            .iter()
            .any(|key| table.get(key).is_some())
    }) {
        return Ok(BTreeSet::new());
    }
    let target = if let Some(path) = table
        .and_then(|table| table.get("path"))
        .and_then(Item::as_str)
    {
        let path = resolve(directory(&owner.manifest), path)?;
        Some(
            members
                .iter()
                .find(|(_, p)| {
                    directory(&p.manifest) == path
                        && normalize_name(&p.name) == normalize_name(name)
                })
                .map(|(manifest, _)| manifest.clone())
                .ok_or_else(|| {
                    format!(
                        "{}: stale/undeclared Poetry local dependency {name} @ {path}",
                        spec.path
                    )
                })?,
        )
    } else {
        let version = constraint
            .as_str()
            .or_else(|| {
                table
                    .and_then(|table| table.get("version"))
                    .and_then(Item::as_str)
            })
            .unwrap_or("*");
        named_locked_target(spec, name, version, packages, members)?
    };
    let mut targets = BTreeSet::new();
    if let Some(target) = target {
        validate_poetry_path(spec, constraint, owner, members[&target])?;
        validate_poetry_constraint(constraint, &members[&target].version)?;
        targets.insert(target);
    }
    Ok(targets)
}

fn pdm_locked_target(
    spec: &LockSpec,
    name: &str,
    requirement: &str,
    direct: bool,
    owner: &NativePackage,
    packages: &toml_edit::ArrayOfTables,
    members: &BTreeMap<String, &NativePackage>,
) -> Result<Option<String>, String> {
    if direct {
        let local = requirement.starts_with("file:") || requirement.starts_with("{root:uri}");
        if !local {
            return Ok(None);
        }
        let matching = members
            .iter()
            .filter(|(_, p)| normalize_name(&p.name) == normalize_name(name))
            .collect::<Vec<_>>();
        let mut targets = vec![];
        for (manifest, package) in matching {
            if validate_pdm_source(spec, requirement, owner, package).is_ok() {
                targets.push(manifest.clone());
            }
        }
        if targets.len() != 1 {
            return Err(format!(
                "{}: stale/ambiguous PDM local dependency source {name} @ {requirement}",
                spec.path
            ));
        }
        Ok(targets.pop())
    } else {
        named_locked_target(spec, name, requirement, packages, members)
    }
}

type Conditions = (Vec<String>, String);

fn validate_local_conditions(
    spec: &LockSpec,
    package: &Table,
    members: &BTreeMap<String, &NativePackage>,
    owner: &str,
    snapshot: &LanguageSnapshot,
) -> Result<(), String> {
    let input = bytes(snapshot, owner)?;
    let metadata = super::python_metadata::parse(input, owner)?;
    let source = if owner.ends_with("pyproject.toml") {
        Some(document(snapshot, owner)?)
    } else {
        None
    };
    let mut expected_conditions: BTreeMap<String, BTreeSet<Conditions>> = BTreeMap::new();
    let mut locked_conditions: BTreeMap<String, BTreeSet<Conditions>> = BTreeMap::new();
    for dependency in metadata
        .dependencies
        .iter()
        .filter(|dep| dep.kind != "build")
    {
        let Some(target) = source_target(spec, &dependency.name, None, owner, snapshot, members)?
        else {
            continue;
        };
        if matches!(spec.format.as_str(), "poetry" | "pdm")
            && (dependency.kind.starts_with("group:") || dependency.kind == "development")
        {
            return Err(format!(
                "{}: locked local subproject metadata cannot prove development-group edge {} -> {}; use that project's own lock scope",
                spec.path, owner, target
            ));
        }
        let mut expected = match &dependency.slot {
            super::python_metadata::Slot::Poetry(keys) => {
                let mut item = source
                    .as_ref()
                    .ok_or("Missing Poetry source document")?
                    .as_item();
                for key in keys {
                    item = item.get(key).ok_or("Missing Poetry dependency source")?;
                }
                poetry_conditions(item)?
            }
            _ => super::python_dependencies::requirement_identity(&dependency.requirement)?,
        };
        let observed = match spec.format.as_str() {
            "poetry" => {
                if let Some(extra) = dependency.kind.strip_prefix("extra:") {
                    let group = package
                        .get("extras")
                        .and_then(|extras| extras.as_table_like())
                        .and_then(|extras| {
                            extras
                                .iter()
                                .find(|(key, _)| normalize_name(key) == normalize_name(extra))
                                .map(|(_, value)| value)
                        });
                    if !group.and_then(Item::as_array).is_some_and(|group| {
                        group
                            .iter()
                            .filter_map(Value::as_str)
                            .any(|raw| normalize_name(requirement_name(raw)) == dependency.name)
                    }) {
                        return Err(format!(
                            "{}: Poetry lock missing extra {extra} -> {}",
                            spec.path, dependency.name
                        ));
                    }
                }
                let constraint = package
                    .get("dependencies")
                    .and_then(Item::as_table_like)
                    .and_then(|deps| {
                        deps.iter()
                            .find(|(name, _)| normalize_name(name) == dependency.name)
                            .map(|(_, value)| value)
                    })
                    .ok_or_else(|| format!("{}: missing Poetry local conditions", spec.path))?;
                if let Some(values) = constraint.as_array() {
                    values
                        .iter()
                        .map(|value| poetry_conditions(&Item::Value(value.clone())))
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    vec![poetry_conditions(constraint)?]
                }
            }
            "pdm" => {
                if let Some(extra) = dependency.kind.strip_prefix("extra:") {
                    let marker = if expected.1.is_empty() {
                        format!("extra == '{extra}'")
                    } else {
                        format!("({}) and extra == '{extra}'", expected.1)
                    };
                    expected.1 = super::python_dependencies::requirement_identity(&format!(
                        "{}; {marker}",
                        dependency.name
                    ))?
                    .1;
                }
                package
                    .get("dependencies")
                    .and_then(Item::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|raw| normalize_name(requirement_name(raw)) == dependency.name)
                    .map(super::python_dependencies::requirement_identity)
                    .collect::<Result<Vec<_>, _>>()?
            }
            "pylock" => {
                if !expected.0.is_empty()
                    || !expected.1.is_empty()
                    || dependency.kind.starts_with("extra:")
                    || dependency.kind.starts_with("group:")
                    || dependency.kind == "development"
                {
                    return Err(format!(
                        "{}: pylock informational edges cannot prove conditional/extras/group local dependency {} -> {}; use a producer-specific lock provider",
                        spec.path, owner, target
                    ));
                }
                continue;
            }
            _ => continue,
        };
        expected_conditions
            .entry(target.clone())
            .or_default()
            .insert(expected);
        locked_conditions
            .entry(target.clone())
            .or_default()
            .extend(observed);
    }
    for (target, expected) in expected_conditions {
        if locked_conditions.get(&target) != Some(&expected) {
            return Err(format!(
                "{}: {} local dependency marker/extras differ from manifest: {} -> {}; regenerate lock",
                spec.path, spec.format, owner, target
            ));
        }
    }
    Ok(())
}

fn poetry_conditions(item: &Item) -> Result<Conditions, String> {
    let Some(table) = item.as_table_like() else {
        return Ok((vec![], String::new()));
    };
    if table.get("python").is_some() || table.get("platform").is_some() {
        return Err("Poetry python/platform dependency restrictions require an explicitly supported marker conversion".into());
    }
    let mut extras = table
        .get("extras")
        .and_then(Item::as_array)
        .into_iter()
        .flatten()
        .map(|extra| {
            extra
                .as_str()
                .map(normalize_name)
                .ok_or_else(|| "Invalid Poetry dependency extras".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    extras.sort();
    extras.dedup();
    let marker = table.get("markers").and_then(Item::as_str).unwrap_or("");
    let marker = if marker.is_empty() {
        String::new()
    } else {
        super::python_dependencies::requirement_identity(&format!("placeholder; {marker}"))?.1
    };
    Ok((extras, marker))
}

fn dependency_tables(item: &Item) -> Result<Vec<Item>, String> {
    if let Some(tables) = item.as_array_of_tables() {
        Ok(tables
            .iter()
            .map(|table| Item::Table(table.clone()))
            .collect())
    } else if let Some(array) = item.as_array() {
        array
            .iter()
            .map(|value| {
                if value.is_inline_table() {
                    Ok(Item::Value(value.clone()))
                } else {
                    Err("Python lock dependency must be a table".into())
                }
            })
            .collect()
    } else {
        Err("Python lock dependencies must be an array of tables".into())
    }
}

fn validate_poetry_constraint(item: &Item, version: &str) -> Result<(), String> {
    if let Some(raw) = item.as_str() {
        if !super::python_dependencies::accepts(raw, version)? {
            return Err(format!(
                "Stale Poetry local dependency constraint {raw} excludes {version}"
            ));
        }
    } else if let Some(table) = item.as_table_like() {
        if let Some(raw) = table.get("version").and_then(Item::as_str) {
            if !super::python_dependencies::accepts(raw, version)? {
                return Err(format!(
                    "Stale Poetry local dependency constraint {raw} excludes {version}"
                ));
            }
        } else if table.get("path").is_none() {
            return Err("Poetry local constraint has no supported version/path identity".into());
        }
    } else if let Some(array) = item.as_array() {
        for value in array.iter() {
            validate_poetry_constraint(&Item::Value(value.clone()), version)?;
        }
    } else {
        return Err("Unsupported Poetry local dependency constraint".into());
    }
    Ok(())
}

fn validate_pdm_source(
    spec: &LockSpec,
    raw: &str,
    owner: &NativePackage,
    target: &NativePackage,
) -> Result<(), String> {
    let root = if owner.manifest.is_empty() {
        directory(&metadata_path(spec)?).to_string()
    } else {
        directory(&owner.manifest).to_string()
    };
    let path = if let Some(path) = raw
        .strip_prefix("file:///${PROJECT_ROOT}/")
        .or_else(|| raw.strip_prefix("{root:uri}/"))
    {
        resolve(&root, path)?
    } else if let Some(path) = raw.strip_prefix("file:./") {
        resolve(directory(&owner.manifest), path)?
    } else {
        return Err(format!(
            "{}: unsupported/immutable PDM local dependency source {raw}",
            spec.path
        ));
    };
    if path != directory(&target.manifest) {
        return Err(format!(
            "{}: stale PDM local dependency source {raw}",
            spec.path
        ));
    }
    Ok(())
}

fn validate_uv_edges(
    spec: &LockSpec,
    package: &Table,
    packages: &toml_edit::ArrayOfTables,
    members: &BTreeMap<String, &NativePackage>,
    owner: &str,
    snapshot: &LanguageSnapshot,
) -> Result<(), String> {
    let mut arrays = vec![];
    if let Some(array) = package.get("dependencies").and_then(Item::as_array) {
        arrays.push(("runtime".to_string(), array));
    }
    for section in ["optional-dependencies", "dev-dependencies"] {
        if let Some(groups) = package.get(section).and_then(Item::as_table_like) {
            for (group, value) in groups.iter() {
                if let Some(array) = value.as_array() {
                    let prefix = if section == "optional-dependencies" {
                        "extra"
                    } else {
                        "group"
                    };
                    arrays.push((format!("{prefix}:{}", normalize_name(group)), array));
                }
            }
        }
    }
    let mut targets = BTreeSet::new();
    let mut observed_conditions = BTreeSet::new();
    for (kind, edges) in arrays {
        for edge in edges.iter() {
            let edge = edge
                .as_inline_table()
                .ok_or_else(|| format!("{}: invalid uv edge", spec.path))?;
            let name = edge
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{}: uv edge missing name", spec.path))?;
            let matches = packages
                .iter()
                .filter(|candidate| {
                    candidate
                        .get("name")
                        .and_then(Item::as_str)
                        .is_some_and(|candidate| normalize_name(candidate) == normalize_name(name))
                        && edge.get("version").is_none_or(|version| {
                            candidate.get("version").and_then(Item::as_str) == version.as_str()
                        })
                        && edge.get("source").is_none_or(|source| {
                            candidate
                                .get("source")
                                .and_then(Item::as_value)
                                .is_some_and(|candidate| {
                                    json_item(&Item::Value(candidate.clone())).ok()
                                        == json_item(&Item::Value(source.clone())).ok()
                                })
                        })
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(format!(
                    "{}: uv dependency {name} resolves to {} lock entries; regenerate current topology",
                    spec.path,
                    matches.len()
                ));
            }
            if let Some(path) = local_directory(spec, matches[0])? {
                if let Some((manifest, _)) =
                    members.iter().find(|(_, p)| directory(&p.manifest) == path)
                {
                    targets.insert(manifest.clone());
                    let mut extras = edge
                        .get("extra")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|value| {
                            value.as_str().map(normalize_name).ok_or_else(|| {
                                format!("{}: invalid uv dependency extras", spec.path)
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    extras.sort();
                    extras.dedup();
                    let marker = edge.get("marker").and_then(Value::as_str).unwrap_or("");
                    let marker = if marker.is_empty() {
                        String::new()
                    } else {
                        super::python_dependencies::requirement_identity(&format!(
                            "placeholder; {marker}"
                        ))?
                        .1
                    };
                    observed_conditions.insert((kind.clone(), manifest.clone(), (extras, marker)));
                }
            }
        }
    }
    for dependency in snapshot.dependencies.iter().filter(|dependency| {
        dependency.owner == owner
            && dependency.kind != "build"
            && members.contains_key(&dependency.target)
    }) {
        if !targets.contains(&dependency.target) {
            return Err(format!(
                "{}: uv lock missing current local dependency {} -> {}; regenerate current topology",
                spec.path, owner, dependency.target
            ));
        }
    }
    if !owner.is_empty() {
        for target in targets {
            if !snapshot.dependencies.iter().any(|dependency| {
                dependency.owner == owner
                    && dependency.target == target
                    && dependency.kind != "build"
            }) {
                return Err(format!(
                    "{}: stale uv local dependency edge {owner} -> {target}",
                    spec.path
                ));
            }
        }
        let metadata = super::python_metadata::parse(bytes(snapshot, owner)?, owner)?;
        let source = if owner.ends_with("pyproject.toml") {
            Some(document(snapshot, owner)?)
        } else {
            None
        };
        let mut expected_conditions = BTreeSet::new();
        for dependency in metadata
            .dependencies
            .iter()
            .filter(|dependency| dependency.kind != "build")
        {
            if let Some(target) =
                source_target(spec, &dependency.name, None, owner, snapshot, members)?
            {
                let conditions =
                    if let super::python_metadata::Slot::Poetry(keys) = &dependency.slot {
                        let mut item = source
                            .as_ref()
                            .ok_or("Missing Poetry dependency source")?
                            .as_item();
                        for key in keys {
                            item = item.get(key).ok_or("Missing Poetry dependency field")?;
                        }
                        poetry_conditions(item)?
                    } else {
                        super::python_dependencies::requirement_identity(&dependency.requirement)?
                    };
                let kind = if dependency.kind == "development" {
                    "group:dev".to_string()
                } else if let Some((prefix, group)) = dependency.kind.split_once(':') {
                    format!("{prefix}:{}", normalize_name(group))
                } else {
                    dependency.kind.clone()
                };
                let mut effective = conditions.clone();
                if kind == "runtime"
                    && !observed_conditions.contains(&(
                        kind.clone(),
                        target.clone(),
                        conditions.clone(),
                    ))
                {
                    for (observed_kind, observed_target, observed) in &observed_conditions {
                        if observed_kind == &kind
                            && observed_target == target
                            && observed.1 == conditions.1
                            && observed.0.iter().all(|extra| conditions.0.contains(extra))
                            && uv_proves_empty_extra_simplification(
                                spec,
                                package,
                                packages,
                                snapshot,
                                members,
                                owner,
                                target,
                                &conditions,
                                &observed.0,
                            )?
                        {
                            effective = observed.clone();
                            break;
                        }
                    }
                }
                expected_conditions.insert((kind, target.clone(), effective));
            }
        }
        if expected_conditions != observed_conditions {
            return Err(format!(
                "{}: uv local dependency kinds/markers/extras differ from manifest for {owner}; unsupported resolution conditions require native lock validation",
                spec.path
            ));
        }
    }
    Ok(())
}

/// uv can omit a requested extra from a resolved edge when that extra has no
/// dependencies. Keep declaration metadata and the resolved graph separate: this
/// exception requires matching source/marker/full extras in requires-dist, an
/// actually empty target manifest extra, and the target lock's provides-extras.
fn uv_proves_empty_extra_simplification(
    spec: &LockSpec,
    owner_entry: &Table,
    packages: &toml_edit::ArrayOfTables,
    snapshot: &LanguageSnapshot,
    members: &BTreeMap<String, &NativePackage>,
    owner: &str,
    target: &str,
    expected: &Conditions,
    resolved_extras: &[String],
) -> Result<bool, String> {
    let omitted = expected
        .0
        .iter()
        .filter(|extra| !resolved_extras.contains(extra))
        .collect::<Vec<_>>();
    if omitted.is_empty() {
        return Ok(false);
    }
    let manifest = document(snapshot, target)?;
    let optional = manifest
        .get("project")
        .and_then(|project| project.get("optional-dependencies"))
        .and_then(Item::as_table_like);
    for extra in &omitted {
        if !optional
            .and_then(|optional| {
                optional
                    .iter()
                    .find(|(name, _)| normalize_name(name) == extra.as_str())
                    .map(|(_, value)| value)
            })
            .and_then(Item::as_array)
            .is_some_and(|requirements| requirements.is_empty())
        {
            return Ok(false);
        }
    }
    let target_entry = packages.iter().find(|entry| {
        local_directory(spec, entry)
            .ok()
            .flatten()
            .is_some_and(|path| path == directory(target))
    });
    let provided = target_entry
        .and_then(|entry| entry.get("metadata"))
        .and_then(|metadata| metadata.get("provides-extras"))
        .and_then(Item::as_array);
    if !omitted.iter().all(|extra| {
        provided.is_some_and(|provided| {
            provided
                .iter()
                .filter_map(Value::as_str)
                .any(|name| normalize_name(name) == extra.as_str())
        })
    }) {
        return Ok(false);
    }
    let declarations = owner_entry
        .get("metadata")
        .and_then(|metadata| metadata.get("requires-dist"))
        .and_then(Item::as_array);
    for declaration in declarations.into_iter().flatten() {
        let Some(declaration) = declaration.as_inline_table() else {
            return Err(format!(
                "{}: invalid uv requires-dist declaration",
                spec.path
            ));
        };
        let Some(name) = declaration.get("name").and_then(Value::as_str) else {
            continue;
        };
        if normalize_name(name) != normalize_name(&members[target].name) {
            continue;
        }
        let mut source = toml_edit::InlineTable::new();
        for key in [
            "editable",
            "virtual",
            "directory",
            "path",
            "registry",
            "index",
            "git",
            "url",
        ] {
            if let Some(value) = declaration.get(key) {
                source.insert(key, value.clone());
            }
        }
        if source.is_empty()
            || !source.iter().all(|(key, _)| {
                target_entry
                    .and_then(|entry| entry.get("source"))
                    .and_then(|source| source.get(key))
                    .is_some()
            })
            || source_target(
                spec,
                name,
                Some(&Value::InlineTable(source)),
                owner,
                snapshot,
                members,
            )?
            .map(String::as_str)
                != Some(target)
        {
            continue;
        }
        let mut extras = declaration
            .get("extras")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|value| {
                value
                    .as_str()
                    .map(normalize_name)
                    .ok_or_else(|| format!("{}: invalid uv declaration extras", spec.path))
            })
            .collect::<Result<Vec<_>, _>>()?;
        extras.sort();
        extras.dedup();
        let marker = declaration
            .get("marker")
            .and_then(Value::as_str)
            .unwrap_or("");
        let marker = if marker.is_empty() {
            String::new()
        } else {
            super::python_dependencies::requirement_identity(&format!("placeholder; {marker}"))?.1
        };
        if extras == expected.0 && marker == expected.1 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn source_target<'a>(
    spec: &LockSpec,
    name: &str,
    source: Option<&Value>,
    owner: &str,
    snapshot: &LanguageSnapshot,
    members: &'a BTreeMap<String, &NativePackage>,
) -> Result<Option<&'a String>, String> {
    if let Some(source) = source {
        let table = source
            .as_inline_table()
            .ok_or_else(|| format!("{}: invalid dependency source", spec.path))?;
        if let Some(raw) = table
            .get("editable")
            .or_else(|| table.get("virtual"))
            .or_else(|| table.get("directory"))
            .or_else(|| table.get("path"))
            .and_then(Value::as_str)
        {
            let path = resolve(directory(&spec.path), raw)?;
            let matches = members
                .iter()
                .filter(|(_, p)| {
                    directory(&p.manifest) == path
                        && normalize_name(&p.name) == normalize_name(name)
                })
                .map(|(m, _)| m)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(format!(
                    "{}: dependency references stale/undeclared local source {name} @ {raw}",
                    spec.path
                ));
            }
            return Ok(matches.first().copied());
        }
        // An explicit registry/VCS/URL identity is never a local directory.
        return Ok(None);
    }
    let targets = snapshot
        .dependencies
        .iter()
        .filter(|dep| {
            dep.owner == owner
                && members
                    .get(&dep.target)
                    .is_some_and(|p| normalize_name(&p.name) == normalize_name(name))
        })
        .map(|dep| &dep.target)
        .collect::<BTreeSet<_>>();
    if targets.len() > 1 {
        return Err(format!("{}: ambiguous local dependency {name}", spec.path));
    }
    if let Some(target) = targets.first() {
        return Ok(members.get_key_value(*target).map(|(key, _)| key));
    }
    Ok(None)
}

fn update_entry_dependencies(
    spec: &LockSpec,
    entry: &mut Table,
    owner: &NativePackage,
    snapshot: &LanguageSnapshot,
    versions: &BTreeMap<String, String>,
    members: &BTreeMap<String, &NativePackage>,
    _entries: &BTreeMap<usize, String>,
) -> Result<(), String> {
    match spec.format.as_str() {
        "uv" => {
            if let Some(value) = entry.get_mut("dependencies").and_then(Item::as_value_mut) {
                update_uv_values(
                    spec,
                    value,
                    &owner.manifest,
                    snapshot,
                    versions,
                    members,
                    false,
                )?;
            }
            for section in ["optional-dependencies", "dev-dependencies"] {
                if let Some(table) = entry.get_mut(section).and_then(Item::as_table_like_mut) {
                    for (_, item) in table.iter_mut() {
                        if let Some(value) = item.as_value_mut() {
                            update_uv_values(
                                spec,
                                value,
                                &owner.manifest,
                                snapshot,
                                versions,
                                members,
                                false,
                            )?;
                        }
                    }
                }
            }
            if let Some(metadata) = entry.get_mut("metadata").and_then(Item::as_table_like_mut) {
                if let Some(value) = metadata
                    .get_mut("requires-dist")
                    .and_then(Item::as_value_mut)
                {
                    update_uv_values(
                        spec,
                        value,
                        &owner.manifest,
                        snapshot,
                        versions,
                        members,
                        true,
                    )?;
                }
                if let Some(groups) = metadata
                    .get_mut("requires-dev")
                    .and_then(Item::as_table_like_mut)
                {
                    for (_, item) in groups.iter_mut() {
                        if let Some(value) = item.as_value_mut() {
                            update_uv_values(
                                spec,
                                value,
                                &owner.manifest,
                                snapshot,
                                versions,
                                members,
                                true,
                            )?;
                        }
                    }
                }
            }
        }
        "poetry" => {
            if let Some(deps) = entry
                .get_mut("dependencies")
                .and_then(Item::as_table_like_mut)
            {
                for (name, dependency) in deps.iter_mut() {
                    let target =
                        source_target(spec, name.get(), None, &owner.manifest, snapshot, members)?;
                    if let Some(target) = target {
                        validate_poetry_path(spec, dependency, owner, members[target])?;
                        update_poetry_constraint(
                            dependency,
                            &members[target].version,
                            versions.get(target),
                        )?;
                    }
                }
            }
            if let Some(extras) = entry.get_mut("extras").and_then(Item::as_table_like_mut) {
                for (_, value) in extras.iter_mut() {
                    if let Some(values) = value.as_array_mut() {
                        for value in values.iter_mut() {
                            let raw = value
                                .as_str()
                                .ok_or_else(|| format!("{}: invalid Poetry extra", spec.path))?;
                            let name = requirement_name(raw);
                            if let Some(target) =
                                source_target(spec, name, None, &owner.manifest, snapshot, members)?
                            {
                                if let Some(after) = versions.get(target) {
                                    *value = Value::from(
                                        super::python_dependencies::update_requirement(
                                            raw,
                                            &members[target].version,
                                            after,
                                        )?,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        "pdm" => {
            if let Some(deps) = entry.get_mut("dependencies").and_then(Item::as_array_mut) {
                for dependency in deps.iter_mut() {
                    let raw = dependency
                        .as_str()
                        .ok_or_else(|| format!("{}: invalid PDM dependency", spec.path))?;
                    let name = requirement_name(raw);
                    if let Some(target) =
                        source_target(spec, name, None, &owner.manifest, snapshot, members)?
                    {
                        if let Some(after) = versions.get(target) {
                            *dependency =
                                Value::from(super::python_dependencies::update_requirement(
                                    raw,
                                    &members[target].version,
                                    after,
                                )?);
                        }
                    }
                }
            }
        }
        "pylock" => {
            // Directory records have no stable version and their informational
            // dependency links must also resolve by name/source, never old version.
            if let Some(deps) = entry.get_mut("dependencies").and_then(Item::as_array_mut) {
                for dep in deps.iter_mut() {
                    if let Some(dep) = dep.as_inline_table_mut() {
                        let name = dep
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        if source_target(spec, &name, None, &owner.manifest, snapshot, members)?
                            .is_some()
                            && dep.contains_key("version")
                        {
                            return Err(format!(
                                "{}: pylock source-tree dependency must not pin a version: {name}",
                                spec.path
                            ));
                        }
                    }
                }
            }
        }
        _ => (),
    }
    Ok(())
}

fn validate_poetry_path(
    spec: &LockSpec,
    dependency: &Item,
    owner: &NativePackage,
    target: &NativePackage,
) -> Result<(), String> {
    if let Some(array) = dependency.as_array() {
        for value in array.iter() {
            validate_poetry_path(spec, &Item::Value(value.clone()), owner, target)?;
        }
    } else if let Some(table) = dependency.as_table_like() {
        if let Some(raw) = table.get("path").and_then(Item::as_str) {
            let base = if owner.manifest.is_empty() {
                directory(&spec.path)
            } else {
                directory(&owner.manifest)
            };
            if resolve(base, raw)? != directory(&target.manifest) {
                return Err(format!("{}: stale Poetry dependency path {raw}", spec.path));
            }
        }
    }
    Ok(())
}

fn update_uv_values(
    spec: &LockSpec,
    value: &mut Value,
    owner: &str,
    snapshot: &LanguageSnapshot,
    versions: &BTreeMap<String, String>,
    members: &BTreeMap<String, &NativePackage>,
    metadata: bool,
) -> Result<(), String> {
    let array = value
        .as_array_mut()
        .ok_or_else(|| format!("{}: expected dependency array", spec.path))?;
    for value in array.iter_mut() {
        let table = value
            .as_inline_table_mut()
            .ok_or_else(|| format!("{}: expected dependency identity table", spec.path))?;
        let name = table
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{}: dependency missing name", spec.path))?
            .to_owned();
        // requires-dist stores editable/path fields directly instead of source={...}.
        let direct_source = if metadata {
            let mut source = toml_edit::InlineTable::new();
            for key in [
                "editable",
                "virtual",
                "directory",
                "path",
                "registry",
                "index",
                "git",
                "url",
            ] {
                if let Some(value) = table.get(key) {
                    source.insert(key, value.clone());
                }
            }
            if source.is_empty() {
                None
            } else {
                Some(Value::InlineTable(source))
            }
        } else {
            table.get("source").cloned()
        };
        if let Some(target) = source_target(
            spec,
            &name,
            direct_source.as_ref(),
            owner,
            snapshot,
            members,
        )? {
            let before = &members[target].version;
            if !metadata
                && table
                    .get("version")
                    .and_then(Value::as_str)
                    .is_some_and(|version| version != before)
            {
                return Err(format!(
                    "{}: stale dependency version for {name}",
                    spec.path
                ));
            }
            if metadata {
                if let Some(specifier) = table.get("specifier").and_then(Value::as_str) {
                    if !super::python_dependencies::accepts(specifier, before)? {
                        return Err(format!(
                            "{}: stale local metadata requirement for {name}",
                            spec.path
                        ));
                    }
                }
            }
            if let Some(after) = versions.get(target) {
                if after != before && table.contains_key("hashes") {
                    return Err(format!(
                        "{}: local dependency metadata has immutable artifact hashes: {name}",
                        spec.path
                    ));
                }
                if let Some(version) = table.get_mut("version") {
                    *version = Value::from(after.as_str());
                }
                if let Some(specifier) = table.get_mut("specifier") {
                    let raw = specifier
                        .as_str()
                        .ok_or_else(|| format!("{}: invalid dependency specifier", spec.path))?;
                    *specifier = Value::from(super::python_dependencies::update_constraint(
                        raw, before, after,
                    )?);
                }
            }
        }
    }
    Ok(())
}

fn update_poetry_constraint(
    item: &mut Item,
    before: &str,
    after: Option<&String>,
) -> Result<(), String> {
    let Some(after) = after else {
        return Ok(());
    };
    if let Some(raw) = item.as_str() {
        *item = toml_edit::value(super::python_dependencies::update_constraint(
            raw, before, after,
        )?);
    } else if let Some(table) = item.as_table_like_mut() {
        if let Some(version) = table.get_mut("version") {
            let raw = version
                .as_str()
                .ok_or_else(|| "Invalid Poetry dependency version".to_owned())?;
            *version = toml_edit::value(super::python_dependencies::update_constraint(
                raw, before, after,
            )?);
        }
    } else if let Some(array) = item.as_array_mut() {
        for value in array.iter_mut() {
            let mut item = Item::Value(value.clone());
            update_poetry_constraint(&mut item, before, Some(after))?;
            *value = item
                .into_value()
                .map_err(|_| "Invalid Poetry constraint".to_owned())?;
        }
    } else {
        return Err("Unsupported Poetry dependency constraint".into());
    }
    Ok(())
}

fn requirement_name(raw: &str) -> &str {
    raw.trim()
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')))
        .next()
        .unwrap_or("")
}

fn validate_requirements(
    spec: &LockSpec,
    snapshot: &LanguageSnapshot,
    members: &BTreeMap<String, &NativePackage>,
) -> Result<(), String> {
    if spec.metadata.is_some() {
        return Err(format!(
            "{}: requirements export does not accept hash metadata",
            spec.path
        ));
    }
    let raw = std::str::from_utf8(bytes(snapshot, &spec.path)?)
        .map_err(|e| format!("{}: {e}", spec.path))?;
    let mut covered = BTreeSet::new();
    for line in raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        if let Some(path) = line
            .strip_prefix("-e ")
            .or_else(|| line.strip_prefix("--editable "))
        {
            let path = path.split(" #").next().unwrap_or(path).trim();
            let resolved = resolve(directory(&spec.path), path)?;
            let matching = members
                .iter()
                .filter(|(_, p)| directory(&p.manifest) == resolved)
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err(format!(
                    "{}: undeclared requirements editable source {path}",
                    spec.path
                ));
            }
            covered.insert(matching[0].0.clone());
        } else if line.starts_with(['-', '\\']) {
            // Continuations/hash lines are preserved; recursive inputs and index
            // directives would create unguarded release inputs.
            if !line.starts_with("--hash=") && line != "\\" {
                return Err(format!(
                    "{}: unsupported requirements export directive: {line}",
                    spec.path
                ));
            }
        } else {
            let name = normalize_name(requirement_name(line));
            if members.values().any(|p| normalize_name(&p.name) == name) {
                return Err(format!(
                    "{}: managed local package must use an editable directory, not immutable exported pin: {name}",
                    spec.path
                ));
            }
        }
    }
    for manifest in members.keys() {
        if !covered.contains(manifest) {
            return Err(format!(
                "{}: requirements export missing editable local member {manifest}",
                spec.path
            ));
        }
    }
    Ok(())
}

fn content_hash(spec: &LockSpec, snapshot: &LanguageSnapshot) -> Result<String, String> {
    let path = metadata_path(spec)?;
    let doc = document(snapshot, &path)?;
    // Hash only the producer's relevant inputs. Unrelated build/lint settings
    // (including TOML floats/datetimes) are not resolution metadata.
    let project = selected_toml(
        doc.get("project"),
        &["requires-python", "dependencies", "optional-dependencies"],
    )?;
    let provider = doc.get("tool").and_then(|tool| tool.get(&spec.format));
    let relevant_keys: &[&str] = if spec.format == "poetry" {
        &[
            "dependencies",
            "source",
            "extras",
            "dev-dependencies",
            "group",
        ]
    } else {
        &["source", "dev-dependencies", "resolution", "workspace"]
    };
    let provider = selected_toml(provider, relevant_keys)?;
    let groups = if spec.format == "pdm" {
        doc.get("dependency-groups")
            .map(json_item)
            .transpose()?
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    let data = serde_json::json!({"project": project, "tool": {(spec.format.clone()): provider}, "dependency-groups": groups});
    let empty = Json::Object(Map::new());
    let project = data.get("project").unwrap_or(&empty);
    let tool = data.get("tool").unwrap_or(&empty);
    let relevant = if spec.format == "poetry" {
        let poetry = tool.get("poetry").unwrap_or(&empty);
        let project_keys = ["requires-python", "dependencies", "optional-dependencies"];
        let project_values = select(project, &project_keys);
        let mut poetry_values = Map::new();
        for key in [
            "dependencies",
            "source",
            "extras",
            "dev-dependencies",
            "group",
        ] {
            if let Some(value) = poetry.get(key) {
                poetry_values.insert(key.into(), value.clone());
            } else if key != "group" && project_values.is_empty() {
                poetry_values.insert(key.into(), Json::Null);
            }
        }
        if project_values.is_empty() {
            Json::Object(poetry_values)
        } else {
            serde_json::json!({"project": project_values, "tool": {"poetry": poetry_values}})
        }
    } else {
        let pdm = tool.get("pdm").unwrap_or(&empty);
        if pdm.get("workspace").is_some()
            || pdm
                .get("resolution")
                .and_then(|r| r.get("lock_inputs"))
                .and_then(Json::as_bool)
                == Some(true)
        {
            return Err(format!(
                "{}: PDM workspace/canonical lock input hash is not the supported legacy 4.5.x algorithm",
                spec.path
            ));
        }
        let mut dev = Map::new();
        for (name, value) in data
            .get("dependency-groups")
            .and_then(Json::as_object)
            .into_iter()
            .flatten()
        {
            let name = normalize_name(name);
            if dev.insert(name.clone(), value.clone()).is_some() {
                return Err(format!(
                    "{path}: duplicate normalized dependency group {name}"
                ));
            }
        }
        for (name, value) in pdm
            .get("dev-dependencies")
            .and_then(Json::as_object)
            .into_iter()
            .flatten()
        {
            let values = value
                .as_array()
                .ok_or_else(|| format!("{path}: invalid dev-dependencies"))?;
            let target = dev
                .entry(normalize_name(name))
                .or_insert_with(|| Json::Array(vec![]));
            target
                .as_array_mut()
                .ok_or_else(|| format!("{path}: invalid dependency group"))?
                .extend(values.iter().cloned());
        }
        serde_json::json!({
            "sources": pdm.get("source").cloned().unwrap_or_else(|| serde_json::json!([])),
            "dependencies": project.get("dependencies").cloned().unwrap_or_else(|| serde_json::json!([])),
            "dev-dependencies": dev,
            "optional-dependencies": project.get("optional-dependencies").cloned().unwrap_or_else(|| serde_json::json!({})),
            "requires-python": project.get("requires-python").cloned().unwrap_or_else(|| serde_json::json!("")),
            "resolution": pdm.get("resolution").cloned().unwrap_or_else(|| serde_json::json!({})),
        })
    };
    Ok(format!(
        "{:x}",
        Sha256::digest(python_json(&relevant)?.as_bytes())
    ))
}

fn selected_toml(item: Option<&Item>, keys: &[&str]) -> Result<Json, String> {
    let mut result = Map::new();
    for key in keys {
        if let Some(value) = item.and_then(|item| item.get(key)) {
            result.insert((*key).into(), json_item(value)?);
        }
    }
    Ok(Json::Object(result))
}

fn select(value: &Json, keys: &[&str]) -> Map<String, Json> {
    keys.iter()
        .filter_map(|key| value.get(*key).map(|v| ((*key).into(), v.clone())))
        .collect()
}

fn json_item(item: &Item) -> Result<Json, String> {
    if let Some(table) = item.as_table_like() {
        return table
            .iter()
            .map(|(key, value)| Ok((key.into(), json_item(value)?)))
            .collect::<Result<Map<_, _>, String>>()
            .map(Json::Object);
    }
    if let Some(tables) = item.as_array_of_tables() {
        return tables
            .iter()
            .map(|t| json_item(&Item::Table(t.clone())))
            .collect::<Result<Vec<_>, _>>()
            .map(Json::Array);
    }
    match item.as_value() {
        Some(Value::String(v)) => Ok(Json::String(v.value().clone())),
        Some(Value::Integer(v)) => Ok(Json::from(*v.value())),
        Some(Value::Boolean(v)) => Ok(Json::Bool(*v.value())),
        Some(Value::Array(values)) => values
            .iter()
            .map(|v| json_item(&Item::Value(v.clone())))
            .collect::<Result<Vec<_>, _>>()
            .map(Json::Array),
        Some(Value::Float(_)) | Some(Value::Datetime(_)) => {
            Err("Python lock hash input contains unsupported float/datetime".into())
        }
        Some(Value::InlineTable(_)) => unreachable!("inline tables handled above"),
        None => Ok(Json::Null),
    }
}

/// Match json.dumps(sort_keys=True) exactly, including separators and ensure_ascii.
fn python_json(value: &Json) -> Result<String, String> {
    Ok(match value {
        Json::Object(values) => {
            let sorted = values.iter().collect::<BTreeMap<_, _>>();
            let items = sorted
                .into_iter()
                .map(|(key, value)| {
                    Ok(format!(
                        "{}: {}",
                        python_json(&Json::String(key.clone()))?,
                        python_json(value)?
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            format!("{{{}}}", items.join(", "))
        }
        Json::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(python_json)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ),
        Json::String(value) => {
            let encoded = serde_json::to_string(value).map_err(|e| e.to_string())?;
            let mut result = String::new();
            for ch in encoded.chars() {
                if ch.is_ascii() && ch != '\u{7f}' {
                    result.push(ch);
                } else {
                    for unit in ch.encode_utf16(&mut [0; 2]).iter() {
                        result.push_str(&format!("\\u{unit:04x}"));
                    }
                }
            }
            result
        }
        other => other.to_string(),
    })
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../../scripts/tests/unit/core_release_python_lock.rs"]
mod tests;
