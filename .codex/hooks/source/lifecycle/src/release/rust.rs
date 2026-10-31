//! Pure Rust release adapter. All filesystem access is supplied by the caller.
use super::SemVer;
#[cfg(test)]
use super::model::PackageChange;
use super::model::{Language, LanguageSnapshot, NativeDependency, NativePackage, Writes};
use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use toml_edit::{DocumentMut, Item, value};

#[path = "rust_discovery.rs"]
mod discovery;
#[path = "rust_sources.rs"]
mod sources;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RustConfig {
    pub workspaces: Vec<String>,
    #[serde(default = "patch")]
    pub dependency_bump: String,
    #[serde(default)]
    pub shared_inputs: Vec<SharedInput>,
    #[serde(default)]
    pub product_inputs: Vec<String>,
}

impl RustConfig {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.workspaces.is_empty() {
            return Err("rust.workspaces must list at least one Cargo.toml".into());
        }
        if self.dependency_bump != "patch" {
            return Err("rust.dependency_bump currently supports only patch".into());
        }
        let mut seen = BTreeSet::new();
        for path in &self.workspaces {
            if configured_path(path)? != *path
                || path.rsplit('/').next() != Some("Cargo.toml")
                || !seen.insert(path)
            {
                return Err(format!(
                    "invalid or duplicate Rust workspace manifest: {path}"
                ));
            }
        }
        for mapping in &self.shared_inputs {
            configured_path(&mapping.path)?;
            if mapping.packages.is_empty() {
                return Err("shared Rust input has no packages".into());
            }
            for path in &mapping.packages {
                configured_path(path)?;
            }
        }
        for path in &self.product_inputs {
            configured_path(path)?;
        }
        Ok(())
    }
}

fn patch() -> String {
    "patch".into()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SharedInput {
    pub path: String,
    pub packages: Vec<String>,
}

#[derive(Clone, Debug)]
struct Package {
    name: String,
    version: SemVer,
    workspace: String,
    inherited: bool,
    features: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug)]
struct Dependency {
    owner: String,
    target: String,
    kind: DependencyKind,
    alias: String,
    optional: bool,
    features: Vec<String>,
    default_features: bool,
    document: String,
    keys: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DependencyKind {
    Normal,
    Build,
    Dev,
}

#[derive(Clone, Debug)]
pub(super) struct Workspace {
    inventory: BTreeSet<String>,
    inputs: BTreeMap<String, Option<Vec<u8>>>,
    documents: BTreeMap<String, DocumentMut>,
    packages: BTreeMap<String, Package>,
    dependencies: Vec<Dependency>,
    roots: BTreeMap<String, BTreeSet<String>>,
    locks: BTreeSet<String>,
}

fn parse(path: &str, bytes: &[u8]) -> Result<DocumentMut, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("{path}: invalid UTF-8: {e}"))?;
    text.parse()
        .map_err(|e| format!("{path}: invalid TOML: {e}"))
}

// Paths are repository-relative, normalized lexically; the caller rejects links.
fn normalize(base: &str, raw: &str) -> Result<String, String> {
    if raw.is_empty() || raw.contains('\\') || raw.contains(':') || raw.starts_with('/') {
        return Err(format!(
            "Rust release requires a repository-relative slash path: {raw}"
        ));
    }
    let mut parts = base
        .split('/')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!("Rust path escapes repository: {raw}"));
                }
            }
            other => parts.push(other),
        }
    }
    Ok(parts.join("/"))
}

fn directory(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn manifest_at(dir: &str) -> String {
    if dir.is_empty() {
        "Cargo.toml".into()
    } else {
        format!("{dir}/Cargo.toml")
    }
}

fn configured_path(raw: &str) -> Result<String, String> {
    let trailing = raw.ends_with('/');
    let normalized = normalize("", raw)?;
    if normalized.is_empty() {
        return Err("Rust input path cannot be repository root".into());
    }
    if raw.trim_end_matches('/') != normalized {
        return Err(format!("Rust config path must be normalized: {raw}"));
    }
    Ok(if trailing {
        format!("{normalized}/")
    } else {
        normalized
    })
}

fn matches_input(pattern: &str, path: &str) -> bool {
    if pattern.ends_with('/') {
        path.starts_with(pattern)
    } else {
        path == pattern
    }
}

fn strings(item: Option<&Item>, context: &str) -> Result<Vec<String>, String> {
    let Some(item) = item else {
        return Ok(Vec::new());
    };
    let array = item
        .as_array()
        .ok_or_else(|| format!("{context} must be an array"))?;
    array
        .iter()
        .map(|v| {
            v.as_str()
                .map(String::from)
                .ok_or_else(|| format!("{context} must contain strings"))
        })
        .collect()
}

fn member_pattern(raw: &str) -> Result<Regex, String> {
    if raw.contains('[') || raw.contains('{') || raw.contains('\\') || raw.contains("**") {
        return Err(format!("unsupported Cargo member glob: {raw}"));
    }
    let mut pattern = String::from("^");
    for c in raw.chars() {
        match c {
            '*' => pattern.push_str("[^/]*"),
            '?' => pattern.push_str("[^/]"),
            _ => pattern.push_str(&regex::escape(&c.to_string())),
        }
    }
    pattern.push('$');
    Regex::new(&pattern).map_err(|e| e.to_string())
}

fn read_document(
    workspace: &mut Workspace,
    path: &str,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<(), String> {
    if workspace.documents.contains_key(path) {
        return Ok(());
    }
    let bytes = read(path)?.ok_or_else(|| format!("Rust manifest missing: {path}"))?;
    let document = parse(path, &bytes)?;
    workspace.inputs.insert(path.into(), Some(bytes));
    workspace.documents.insert(path.into(), document);
    Ok(())
}

fn validate_workspace_root(
    workspace: &mut Workspace,
    root: &str,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<(), String> {
    let document = &workspace.documents[root];
    if let Some(pointer) = document.get("package").and_then(|p| p.get("workspace")) {
        let pointer = pointer
            .as_str()
            .ok_or_else(|| format!("{root}: package.workspace must be a path string"))?;
        let actual = manifest_at(&normalize(directory(root), pointer)?);
        if actual != root {
            return Err(format!(
                "{root}: package.workspace identifies {actual}; rust.workspaces must register the actual workspace root"
            ));
        }
        if document.get("workspace").is_none() {
            return Err(format!(
                "{root}: package.workspace does not identify a manifest with [workspace]"
            ));
        }
    }
    // An explicit [workspace] makes a nested package independent of ancestors.
    if document.get("workspace").is_some() {
        return Ok(());
    }
    let root_dir = directory(root);
    if root_dir.is_empty() {
        return Ok(());
    }
    let mut ancestor_dir = directory(root_dir).to_string();
    loop {
        let ancestor = manifest_at(&ancestor_dir);
        if !workspace.inputs.contains_key(&ancestor) {
            let bytes = read(&ancestor)?;
            if let Some(bytes) = &bytes {
                workspace
                    .documents
                    .insert(ancestor.clone(), parse(&ancestor, bytes)?);
            }
            workspace.inputs.insert(ancestor.clone(), bytes);
        }
        if let Some(document) = workspace.documents.get(&ancestor) {
            if let Some(table) = document.get("workspace") {
                let exclusions = strings(
                    table.get("exclude"),
                    &format!("{ancestor}: workspace.exclude"),
                )?;
                let excluded = exclusions
                    .iter()
                    .map(|raw| member_pattern(&normalize(&ancestor_dir, raw)?))
                    .collect::<Result<Vec<_>, String>>()?
                    .iter()
                    .any(|pattern| pattern.is_match(root_dir));
                if excluded {
                    return Ok(());
                }
                // Cargo searches ancestor workspaces even for automatically
                // included path members. An unlisted, unexcluded nested package
                // is rejected by Cargo too, rather than becoming a new root.
                return Err(format!(
                    "{root}: ancestor workspace {ancestor} owns this package; rust.workspaces must register the actual workspace root (or explicitly exclude an independent package)"
                ));
            }
        }
        if ancestor_dir.is_empty() {
            break;
        }
        ancestor_dir = directory(&ancestor_dir).to_string();
    }
    Ok(())
}

pub(super) fn load(
    config: &RustConfig,
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<Workspace, String> {
    load_with_lock_scope(config, inventory, None, read)
}

/// Describe the current Cargo graph without imposing schema 2's historical
/// identity restrictions. The language-neutral core owns lifecycle validation;
/// this adapter still requires a complete, internally consistent current graph.
pub(super) fn snapshot(
    config: &RustConfig,
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<LanguageSnapshot, String> {
    let workspace = load(config, inventory, read)?;
    workspace.validate_lock_edges()?;
    from_workspace(workspace, read)
}

/// Convert an already validated workspace without strengthening its historical
/// lock contract. In particular, schema 1 histories use the limited lock scope
/// established by `load_legacy_history`, rather than today's ownership rules.
pub(super) fn from_workspace(
    mut workspace: Workspace,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<LanguageSnapshot, String> {
    let mut source_roots = BTreeMap::new();
    for manifest in workspace.packages.keys() {
        let mut roots = cargo_source_roots(
            manifest,
            &workspace.documents[manifest],
            &workspace.inventory,
        )?;
        let entries = roots.iter().skip(1).cloned().collect::<Vec<_>>();
        if !entries.is_empty() {
            roots.extend(sources::reachable(
                &entries,
                &workspace.inventory,
                &mut workspace.inputs,
                read,
            )?);
            let directory_root = roots.remove(0);
            roots = std::iter::once(directory_root)
                .chain(roots.into_iter().collect::<BTreeSet<_>>())
                .collect();
        }
        source_roots.insert(manifest.clone(), roots);
    }
    Ok(LanguageSnapshot {
        packages: workspace
            .packages
            .iter()
            .map(|(manifest, package)| {
                (
                    manifest.clone(),
                    NativePackage {
                        manifest: manifest.clone(),
                        name: package.name.clone(),
                        version: package.version.to_string(),
                        language: Language::Rust,
                        workspace: package.workspace.clone(),
                        source_roots: source_roots[manifest].clone(),
                    },
                )
            })
            .collect(),
        dependencies: workspace
            .dependencies
            .iter()
            .map(|dependency| NativeDependency {
                owner: dependency.owner.clone(),
                target: dependency.target.clone(),
                kind: match dependency.kind {
                    DependencyKind::Normal => "normal",
                    DependencyKind::Build => "build",
                    DependencyKind::Dev => "dev",
                }
                .into(),
            })
            .collect(),
        inputs: workspace.inputs,
        roots: workspace.roots.into_keys().collect(),
    })
}

/// Cargo permits explicit targets outside the package directory. Retain exact
/// paths, not their parent directories: a sibling is not ownership evidence.
/// This operates on the same parsed manifests/inventory for disk and Git history.
fn cargo_source_roots(
    manifest: &str,
    doc: &DocumentMut,
    _inventory: &BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let dir = directory(manifest);
    let root = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let mut external = BTreeSet::new();
    let mut add = |raw: &str| -> Result<(), String> {
        let path = normalize(dir, raw)?;
        // Package-local sources already belong to the directory root. Native
        // version/lock snapshots may intentionally contain metadata only.
        if path.starts_with(&root) {
            return Ok(());
        }
        // A historical manifest can retain a removed target. Its declaration
        // still proves exact ownership, but supplies no bytes to hash or scan.
        external.insert(path);
        Ok(())
    };
    if let Some(path) = doc.get("lib").and_then(|t| t.get("path")) {
        add(path.as_str().ok_or("lib.path must be a string")?)?;
    }
    for kind in ["bin", "test", "bench", "example"] {
        if let Some(targets) = doc.get(kind) {
            if let Some(tables) = targets.as_array_of_tables() {
                for target in tables {
                    if let Some(path) = target.get("path") {
                        add(path.as_str().ok_or("Cargo target path must be a string")?)?;
                    }
                }
            } else if let Some(tables) = targets.as_array() {
                for target in tables {
                    let table = target
                        .as_inline_table()
                        .ok_or("Cargo target must be a table")?;
                    if let Some(path) = table.get("path") {
                        add(path.as_str().ok_or("Cargo target path must be a string")?)?;
                    }
                }
            } else {
                return Err("Cargo targets must be tables".into());
            }
        }
    }
    if let Some(build) = doc.get("package").and_then(|p| p.get("build")) {
        if let Some(path) = build.as_str() {
            add(path)?;
        } else if build.as_bool().is_none() {
            return Err("package.build must be a string or boolean".into());
        }
    }
    Ok(std::iter::once(root).chain(external).collect())
}

/// Render exact final versions chosen by the common release planner. No package
/// rename, removal, split or merge is guessed here: manifests and locks must
/// already describe the intended current package graph.
pub(super) fn render_versions(
    config: &RustConfig,
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    versions: &BTreeMap<String, String>,
) -> Result<Writes, String> {
    let workspace = load(config, inventory, read)?;
    workspace.validate_lock_edges()?;
    let versions = versions
        .iter()
        .map(|(manifest, raw)| {
            if !workspace.packages.contains_key(manifest) {
                return Err(format!(
                    "{manifest}: final Rust version references an unknown package"
                ));
            }
            SemVer::from_str(raw)
                .map(|version| (manifest.clone(), version))
                .map_err(|error| format!("{manifest}: invalid final Rust version: {error}"))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let writes = workspace.render_versions(&versions)?;
    // Re-parse the exact proposed bytes to prove that the rewritten constraints,
    // inherited versions and local lock identities agree with the final graph.
    let final_workspace = load(config, inventory, &mut |path| {
        if let Some(bytes) = writes.get(path) {
            Ok(Some(bytes.clone()))
        } else {
            read(path)
        }
    })?;
    final_workspace.validate_lock_edges()?;
    Ok(writes)
}

/// Independent development may update local requirements of product callers,
/// but must preserve their inherited product version declaration.
pub(super) fn render_versions_with_product_packages(
    config: &RustConfig,
    products: &[super::lifecycle::Identity],
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    versions: &BTreeMap<String, String>,
) -> Result<Writes, String> {
    if products.is_empty() {
        return render_versions(config, inventory, read, versions);
    }
    let workspace = load(config, inventory, read)?;
    workspace.validate_lock_edges()?;
    let selected = validate_product_packages(&workspace, products, None)?;
    if versions.keys().any(|path| selected.contains(path)) {
        return Err("development cannot write a product-package version".into());
    }
    let native = versions
        .iter()
        .map(|(path, version)| {
            if !workspace.packages.contains_key(path) {
                return Err(format!(
                    "{path}: final Rust version references an unknown package"
                ));
            }
            Ok((path.clone(), version.parse::<SemVer>()?))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let mut writes = workspace.render_versions(&native)?;
    restore_product_inheritance(&workspace, &selected, &mut writes)?;
    validate_rendered_workspace(config, inventory, read, &writes)?;
    Ok(writes)
}

fn validate_rendered_workspace(
    config: &RustConfig,
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    writes: &Writes,
) -> Result<(), String> {
    let final_workspace = load(config, inventory, &mut |path| {
        writes
            .get(path)
            .map(|bytes| Ok(Some(bytes.clone())))
            .unwrap_or_else(|| read(path))
    })?;
    final_workspace.validate_lock_edges()
}

fn validate_product_packages(
    workspace: &Workspace,
    products: &[super::lifecycle::Identity],
    expected: Option<SemVer>,
) -> Result<BTreeSet<String>, String> {
    let mut selected = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for identity in products {
        let package = workspace.packages.get(&identity.manifest).ok_or_else(|| {
            format!(
                "{}: explicit product package was not discovered",
                identity.manifest
            )
        })?;
        if identity.language != Language::Rust
            || identity.name != package.name
            || identity
                .workspace
                .as_ref()
                .is_some_and(|root| root != &package.workspace)
            || !selected.insert(identity.manifest.clone())
            || !ids.insert(identity.id.clone())
        {
            return Err("explicit product packages need unique matching Rust identities".into());
        }
        if expected.is_some_and(|version| version != package.version) {
            return Err(format!(
                "{}: product package version differs from VERSION",
                identity.manifest
            ));
        }
    }
    let product_roots = selected
        .iter()
        .map(|path| workspace.packages[path].workspace.clone())
        .collect::<BTreeSet<_>>();
    for (path, package) in &workspace.packages {
        if package.inherited
            && product_roots.contains(&package.workspace)
            && !selected.contains(path)
        {
            return Err(format!(
                "{path}: an independent member of a mixed product workspace must own its version"
            ));
        }
    }
    Ok(selected)
}

fn restore_product_inheritance(
    workspace: &Workspace,
    products: &BTreeSet<String>,
    writes: &mut Writes,
) -> Result<(), String> {
    for path in products {
        let Some(bytes) = writes.get(path) else {
            continue;
        };
        if workspace.packages[path].inherited {
            let mut document = parse(path, bytes)?;
            document["package"]["version"] =
                workspace.documents[path]["package"]["version"].clone();
            let bytes = document.to_string().into_bytes();
            if workspace.inputs.get(path).and_then(Option::as_ref) == Some(&bytes) {
                writes.remove(path);
            } else {
                writes.insert(path.clone(), bytes);
            }
        }
    }
    Ok(())
}

/// Resolve carrier workspaces with Cargo discovery, including explicit
/// package.workspace overrides. Directory ancestry is not an ownership proof.
pub(super) fn product_workspaces(
    config: &RustConfig,
    products: &[super::lifecycle::Identity],
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<BTreeSet<String>, String> {
    if products.is_empty() {
        return Ok(BTreeSet::new());
    }
    let workspace = load(config, inventory, read)?;
    let selected = validate_product_packages(&workspace, products, None)?;
    Ok(selected
        .iter()
        .map(|path| workspace.packages[path].workspace.clone())
        .collect())
}

pub(super) fn product_package_inputs(
    config: &RustConfig,
    products: &[super::lifecycle::Identity],
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    product: SemVer,
) -> Result<BTreeMap<String, Option<Vec<u8>>>, String> {
    let workspace = load(config, inventory, read)?;
    workspace.validate_lock_edges()?;
    validate_product_packages(&workspace, products, Some(product))?;
    Ok(workspace.inputs)
}

pub(super) fn render_product_packages(
    config: &RustConfig,
    products: &[super::lifecycle::Identity],
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    product: SemVer,
    target: SemVer,
) -> Result<Writes, String> {
    if products.is_empty() {
        return Ok(Writes::new());
    }
    let workspace = load(config, inventory, read)?;
    workspace.validate_lock_edges()?;
    let selected = validate_product_packages(&workspace, products, Some(product))?;
    let versions = selected.iter().map(|path| (path.clone(), target)).collect();
    let mut writes = workspace.render_versions(&versions)?;
    let inherited_roots = selected
        .iter()
        .filter(|path| workspace.packages[*path].inherited)
        .map(|path| workspace.packages[path].workspace.clone())
        .collect::<BTreeSet<_>>();
    for root in inherited_roots {
        let original = workspace.inputs[&root]
            .as_ref()
            .ok_or("product workspace input missing")?;
        let mut document = parse(&root, writes.get(&root).unwrap_or(original))?;
        document["workspace"]["package"]["version"] = value(target.to_string());
        writes.insert(root, document.to_string().into_bytes());
    }
    let inherited = workspace
        .packages
        .iter()
        .filter(|(_, package)| package.inherited)
        .map(|(path, _)| path.clone())
        .collect();
    restore_product_inheritance(&workspace, &inherited, &mut writes)?;
    validate_rendered_workspace(config, inventory, read, &writes)?;
    // Product release may not silently rewrite a separately managed caller.
    for (path, package) in &workspace.packages {
        if !selected.contains(path) && writes.contains_key(path) {
            return Err(format!(
                "{path}: product release would modify an independent package ({})",
                package.name
            ));
        }
    }
    Ok(writes)
}

/// Return only owners whose *effective declared requirement* changed in proposed
/// output. A target version changing within an accepted range is not propagation.
pub(super) fn requirement_changes(
    config: &RustConfig,
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    writes: &Writes,
) -> Result<BTreeSet<String>, String> {
    let workspace = load(config, inventory, read)?;
    let mut affected = BTreeSet::new();
    for dependency in &workspace.dependencies {
        let Some(bytes) = writes.get(&dependency.document) else {
            continue;
        };
        let proposed = std::str::from_utf8(bytes)
            .map_err(|e| e.to_string())?
            .parse::<DocumentMut>()
            .map_err(|e| e.to_string())?;
        let mut before = workspace.documents[&dependency.document].as_item();
        let mut after = proposed.as_item();
        for key in &dependency.keys {
            before = before
                .get(key)
                .ok_or("original dependency requirement disappeared")?;
            after = after
                .get(key)
                .ok_or("proposed dependency requirement disappeared")?;
        }
        if before.get("version").and_then(Item::as_str)
            != after.get("version").and_then(Item::as_str)
        {
            affected.insert(dependency.owner.clone());
        }
    }
    Ok(affected)
}

/// Unified products keep Cargo's workspace inheritance rather than turning all
/// followers into unrelated literal versions. The final graph is revalidated.
pub(super) fn render_synchronized_versions(
    config: &RustConfig,
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    versions: &BTreeMap<String, String>,
) -> Result<Writes, String> {
    if versions.is_empty() {
        return Ok(Writes::new());
    }
    let target = versions.values().next().unwrap();
    if versions.values().any(|v| v != target) {
        return Err("synchronized Rust targets must agree".into());
    }
    let mut writes = render_versions(config, inventory, read, versions)?;
    let snapshot = snapshot(config, inventory, read)?;
    for root in &config.workspaces {
        let Some(bytes) = read(root)? else {
            return Err("synchronized workspace missing".into());
        };
        let mut doc = parse(root, writes.get(root).unwrap_or(&bytes))?;
        if doc
            .get("workspace")
            .and_then(|w| w.get("package"))
            .and_then(|p| p.get("version"))
            .is_some()
        {
            doc["workspace"]["package"]["version"] = value(target);
            writes.insert(root.clone(), doc.to_string().into_bytes());
        }
    }
    for path in snapshot.packages.keys() {
        let bytes = read(path)?.ok_or("synchronized package disappeared")?;
        let old = parse(path, &bytes)?;
        let Some(version) = old.get("package").and_then(|p| p.get("version")) else {
            continue;
        };
        if version.get("workspace").and_then(Item::as_bool) == Some(true) {
            let mut doc = parse(path, writes.get(path).unwrap_or(&bytes))?;
            doc["package"]["version"] = version.clone();
            writes.insert(path.clone(), doc.to_string().into_bytes());
        }
    }
    let final_snapshot = self::snapshot(config, inventory, &mut |p| {
        if let Some(b) = writes.get(p) {
            Ok(Some(b.clone()))
        } else {
            read(p)
        }
    })?;
    if final_snapshot
        .packages
        .values()
        .any(|p| &p.version != target)
    {
        return Err("synchronized Rust workspace versions disagree after rendering".into());
    }
    Ok(writes)
}

// Product mappings can contain independent copies of the same workspace (the
// factory's template and dogfood). Validate each connected workspace group with
// the same native graph/lock rules, without merging duplicate lock identities.
fn product_groups(
    roots: &[String],
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    product: SemVer,
) -> Result<(Workspace, Vec<Vec<String>>), String> {
    let cfg = RustConfig {
        workspaces: roots.to_vec(),
        dependency_bump: "patch".into(),
        shared_inputs: vec![],
        product_inputs: vec![],
    };
    let all = load_with_lock_scope(&cfg, inventory, Some(&BTreeSet::new()), read)?;
    for (path, package) in &all.packages {
        if package.version != product {
            return Err(format!(
                "{path}: synchronized product mapping version differs from VERSION"
            ));
        }
    }
    let mut groups = BTreeSet::new();
    for root in roots {
        let mut connected = BTreeSet::from([root.clone()]);
        loop {
            let packages = all
                .packages
                .iter()
                .filter(|(_, p)| connected.contains(&p.workspace))
                .map(|(p, _)| p.clone())
                .collect::<BTreeSet<_>>();
            let extra = all
                .dependencies
                .iter()
                .filter(|d| packages.contains(&d.owner))
                .map(|d| all.packages[&d.target].workspace.clone())
                .collect::<BTreeSet<_>>();
            let count = connected.len();
            connected.extend(extra);
            if connected.len() == count {
                break;
            }
        }
        let mut group = all.clone();
        group.roots.retain(|root, _| connected.contains(root));
        group
            .packages
            .retain(|_, p| connected.contains(&p.workspace));
        group.dependencies.retain(|d| {
            group.packages.contains_key(&d.owner) && group.packages.contains_key(&d.target)
        });
        group.validate_locks(None)?;
        group.validate_lock_edges()?;
        groups.insert(connected.into_iter().collect::<Vec<_>>());
    }
    Ok((all, groups.into_iter().collect()))
}

pub(super) fn product_mapping_inputs(
    roots: &[String],
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    product: SemVer,
) -> Result<BTreeMap<String, Option<Vec<u8>>>, String> {
    if roots.is_empty() {
        return Ok(BTreeMap::new());
    }
    Ok(product_groups(roots, inventory, read, product)?.0.inputs)
}

pub(super) fn render_product_mappings(
    roots: &[String],
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    product: SemVer,
    target: SemVer,
) -> Result<Writes, String> {
    if roots.is_empty() {
        return Ok(Writes::new());
    }
    let (all, groups) = product_groups(roots, inventory, read, product)?;
    let mut writes = Writes::new();
    for group in groups {
        let cfg = RustConfig {
            workspaces: group.clone(),
            dependency_bump: "patch".into(),
            shared_inputs: vec![],
            product_inputs: vec![],
        };
        let versions = all
            .packages
            .iter()
            .filter(|(_, p)| group.contains(&p.workspace))
            .map(|(p, _)| (p.clone(), target.to_string()))
            .collect();
        for (path, bytes) in render_synchronized_versions(&cfg, inventory, read, &versions)? {
            if writes.get(&path).is_some_and(|old| old != &bytes) {
                return Err(format!("product mapping groups disagree on {path}"));
            }
            writes.insert(path, bytes);
        }
    }
    Ok(writes)
}

/// A schema 1 snapshot predates independent Rust lock ownership. Keep full
/// discovery and identity checks; validate locks for its managed roots/members.
pub(super) fn load_legacy_history(
    config: &RustConfig,
    inventory: &[String],
    manifests: &BTreeSet<String>,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<Workspace, String> {
    load_with_lock_scope(config, inventory, Some(manifests), read)
}

fn load_with_lock_scope(
    config: &RustConfig,
    inventory: &[String],
    legacy_manifests: Option<&BTreeSet<String>>,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<Workspace, String> {
    config.validate()?;
    if config.workspaces.is_empty() {
        return Err("rust.workspaces must list at least one Cargo.toml".into());
    }
    if config.dependency_bump != "patch" {
        return Err("rust.dependency_bump currently supports only patch".into());
    }
    let mut result = Workspace {
        inventory: inventory.iter().cloned().collect(),
        inputs: BTreeMap::new(),
        documents: BTreeMap::new(),
        packages: BTreeMap::new(),
        dependencies: Vec::new(),
        roots: BTreeMap::new(),
        locks: BTreeSet::new(),
    };
    let mut seen = BTreeSet::new();
    for root in &config.workspaces {
        if configured_path(root)? != *root
            || root.rsplit('/').next() != Some("Cargo.toml")
            || !seen.insert(root.clone())
        {
            return Err(format!(
                "invalid or duplicate Rust workspace manifest: {root}"
            ));
        }
        read_document(&mut result, root, read)?;
        validate_workspace_root(&mut result, root, read)?;
        let document = &result.documents[root];
        if document.get("patch").is_some() || document.get("replace").is_some() {
            return Err(format!(
                "{root}: patch/replace require an explicit future Rust adapter policy"
            ));
        }
        let root_dir = directory(root);
        let members = strings(
            document.get("workspace").and_then(|w| w.get("members")),
            &format!("{root}: workspace.members"),
        )?;
        let excludes = strings(
            document.get("workspace").and_then(|w| w.get("exclude")),
            &format!("{root}: workspace.exclude"),
        )?;
        let excludes = excludes
            .iter()
            .map(|x| member_pattern(&normalize(root_dir, x)?))
            .collect::<Result<Vec<_>, String>>()?;
        let mut paths = BTreeSet::new();
        if document.get("package").is_some() {
            paths.insert(root.clone());
        }
        for member in members {
            let normalized = normalize(root_dir, &member)?;
            let pattern = member_pattern(&normalized)?;
            let found = inventory
                .iter()
                .filter(|path| {
                    path.rsplit('/').next() == Some("Cargo.toml")
                        && pattern.is_match(directory(path))
                        && !excludes.iter().any(|e| e.is_match(directory(path)))
                })
                .cloned()
                .collect::<Vec<_>>();
            if found.is_empty() {
                return Err(format!(
                    "{root}: Cargo member has no matching manifest: {member}"
                ));
            }
            paths.extend(found);
        }
        if paths.is_empty() {
            return Err(format!("{root}: Rust workspace has no packages"));
        }
        discovery::expand(&mut result, root, &mut paths, &excludes, inventory, read)?;
        result.roots.insert(root.clone(), paths.clone());
        for path in paths {
            if result.packages.contains_key(&path) {
                return Err(format!(
                    "Rust package is registered in multiple workspaces: {path}"
                ));
            }
            read_document(&mut result, &path, read)?;
            let doc = &result.documents[&path];
            if doc.get("patch").is_some() || doc.get("replace").is_some() {
                return Err(format!("{path}: patch/replace are unsupported"));
            }
            let package = doc
                .get("package")
                .ok_or_else(|| format!("{path}: missing package"))?;
            if path != *root && doc.get("workspace").is_some() {
                return Err(format!(
                    "{path}: nested workspace cannot be a member of {root}; register its actual workspace root separately"
                ));
            }
            if let Some(pointer) = package.get("workspace") {
                let pointer = pointer
                    .as_str()
                    .ok_or_else(|| format!("{path}: package.workspace must be a path string"))?;
                let actual = manifest_at(&normalize(directory(&path), pointer)?);
                if actual != *root {
                    return Err(format!(
                        "{path}: package.workspace identifies {actual}, not configured member root {root}"
                    ));
                }
            }
            let name = package
                .get("name")
                .and_then(Item::as_str)
                .ok_or_else(|| format!("{path}: missing package.name"))?
                .to_owned();
            let version = package
                .get("version")
                .ok_or_else(|| format!("{path}: missing package.version"))?;
            let inherited = version.get("workspace").and_then(Item::as_bool) == Some(true);
            let raw = if inherited {
                result.documents[root]
                    .get("workspace")
                    .and_then(|w| w.get("package"))
                    .and_then(|p| p.get("version"))
                    .and_then(Item::as_str)
            } else {
                version.as_str()
            }
            .ok_or_else(|| format!("{path}: unsupported or missing package version"))?;
            let version = SemVer::from_str(raw)?;
            let mut features = BTreeMap::new();
            if let Some(table) = doc.get("features") {
                let table = table
                    .as_table_like()
                    .ok_or_else(|| format!("{path}: features must be a table"))?;
                for (name, values) in table.iter() {
                    features.insert(
                        name.to_string(),
                        strings(Some(values), &format!("{path}: features.{name}"))?,
                    );
                }
            }
            result.packages.insert(
                path,
                Package {
                    name,
                    version,
                    workspace: root.clone(),
                    inherited,
                    features,
                },
            );
        }
        let lock = if root_dir.is_empty() {
            "Cargo.lock".to_string()
        } else {
            format!("{root_dir}/Cargo.lock")
        };
        let bytes = read(&lock)?;
        if let Some(bytes) = &bytes {
            result.documents.insert(lock.clone(), parse(&lock, bytes)?);
        }
        result.inputs.insert(lock.clone(), bytes);
        result.locks.insert(lock);
    }
    for input in &config.shared_inputs {
        configured_path(&input.path)?;
        if input.packages.is_empty() {
            return Err(format!("shared Rust input has no packages: {}", input.path));
        }
        let mut packages = BTreeSet::new();
        for path in &input.packages {
            if !result.packages.contains_key(path) || !packages.insert(path) {
                return Err(format!(
                    "shared Rust input references unknown or duplicate package: {path}"
                ));
            }
        }
    }
    for path in &config.product_inputs {
        configured_path(path)?;
    }
    let mappings = config
        .shared_inputs
        .iter()
        .map(|x| &x.path)
        .chain(config.product_inputs.iter())
        .collect::<Vec<_>>();
    for (index, path) in mappings.iter().enumerate() {
        for other in &mappings[index + 1..] {
            if path == other
                || matches_input(path, other.trim_end_matches('/'))
                || matches_input(other, path.trim_end_matches('/'))
            {
                return Err(format!(
                    "overlapping Rust input mappings: {path} and {other}"
                ));
            }
        }
    }
    result.collect_dependencies()?;
    result.validate_locks(legacy_manifests)?;
    Ok(result)
}

impl Workspace {
    #[cfg(test)]
    pub(super) fn inputs(&self) -> &BTreeMap<String, Option<Vec<u8>>> {
        &self.inputs
    }

    pub(super) fn validate_baseline(&self, old: &Workspace) -> Result<(), String> {
        self.validate_transition(old, old)
    }

    /// Only genuinely new manifests may join a release. The origin remains the
    /// release boundary even when registration follows the file's first commit.
    pub(super) fn validate_transition(
        &self,
        old: &Workspace,
        origin: &Workspace,
    ) -> Result<(), String> {
        for root in old.roots.keys() {
            if !self.roots.contains_key(root) {
                return Err(format!(
                    "{root}: Rust workspace removal or boundary change is unsupported; restore the registered workspace"
                ));
            }
        }
        for root in self.roots.keys().filter(|p| !old.roots.contains_key(*p)) {
            if !self.inventory.contains(root) {
                return Err(format!(
                    "{root}: new Rust workspace is outside the current Git inventory; track the manifest before release"
                ));
            }
            if origin.inventory.contains(root) {
                return Err(format!(
                    "{root}: existing workspace cannot be enrolled as a new workspace; explicit migration is not supported"
                ));
            }
        }
        for (path, before) in &old.packages {
            let package = self.packages.get(path).ok_or_else(|| format!(
                "{path}: Rust package removal/rename is unsupported; restore the package identity"
            ))?;
            if package.name != before.name
                || package.version != before.version
                || package.workspace != before.workspace
            {
                return Err(format!(
                    "{path}: package identity or effective version changed outside the managed Rust release"
                ));
            }
        }
        for path in self
            .packages
            .keys()
            .filter(|p| !old.packages.contains_key(*p))
        {
            if !self.inventory.contains(path) {
                return Err(format!(
                    "{path}: new Rust package is outside the current Git inventory; track the manifest before release"
                ));
            }
            if origin.inventory.contains(path) {
                return Err(format!(
                    "{path}: existing manifest cannot be enrolled as a new package; workspace boundary migration is not supported"
                ));
            }
            if self
                .packages
                .values()
                .filter(|p| p.name == self.packages[path].name)
                .count()
                != 1
            {
                return Err(format!(
                    "{path}: new Rust package name conflicts with another registered identity"
                ));
            }
        }
        Ok(())
    }

    fn collect_dependencies(&mut self) -> Result<(), String> {
        for (manifest, package) in &self.packages {
            let doc = &self.documents[manifest];
            let mut sections = Vec::new();
            for kind in ["dependencies", "build-dependencies", "dev-dependencies"] {
                if let Some(table) = doc.get(kind) {
                    sections.push((vec![kind.to_owned()], table));
                }
            }
            if let Some(targets) = doc.get("target").and_then(Item::as_table_like) {
                for (target, item) in targets.iter() {
                    for kind in ["dependencies", "build-dependencies", "dev-dependencies"] {
                        if let Some(table) = item.get(kind) {
                            sections
                                .push((vec!["target".into(), target.into(), kind.into()], table));
                        }
                    }
                }
            }
            for (section, table) in sections {
                // Keep the member's declaration kind even when the actual
                // requirement is inherited from workspace.dependencies.
                let kind = match section.last().map(String::as_str) {
                    Some("dependencies") => DependencyKind::Normal,
                    Some("build-dependencies") => DependencyKind::Build,
                    Some("dev-dependencies") => DependencyKind::Dev,
                    _ => return Err(format!("{manifest}: unsupported dependency section")),
                };
                let table = table
                    .as_table_like()
                    .ok_or_else(|| format!("{manifest}: invalid dependency table"))?;
                for (name, declaration) in table.iter() {
                    let inherited =
                        declaration.get("workspace").and_then(Item::as_bool) == Some(true);
                    let (document, mut keys, dependency) = if inherited {
                        let root = &self.documents[&package.workspace];
                        let dependency = root
                            .get("workspace")
                            .and_then(|w| w.get("dependencies"))
                            .and_then(|d| d.get(name))
                            .ok_or_else(|| {
                                format!("{manifest}: inherited dependency {name} is missing")
                            })?;
                        (
                            package.workspace.clone(),
                            vec!["workspace".into(), "dependencies".into()],
                            dependency,
                        )
                    } else {
                        (manifest.clone(), section.clone(), declaration)
                    };
                    keys.push(name.into());
                    let Some(path) = dependency.get("path").and_then(Item::as_str) else {
                        continue;
                    };
                    let bool_field =
                        |item: &Item, key: &str, default: bool| -> Result<bool, String> {
                            item.get(key).map_or(Ok(default), |v| {
                                v.as_bool().ok_or_else(|| {
                                    format!("{manifest}: dependency {name}.{key} must be boolean")
                                })
                            })
                        };
                    if inherited && bool_field(dependency, "optional", false)? {
                        return Err(format!(
                            "{document}: workspace dependency {name} cannot be optional"
                        ));
                    }
                    let optional = bool_field(declaration, "optional", false)?;
                    if optional && kind == DependencyKind::Dev {
                        return Err(format!(
                            "{manifest}: dev dependency {name} cannot be optional"
                        ));
                    }
                    let mut default_features = bool_field(dependency, "default-features", true)?;
                    if inherited && declaration.get("default-features").is_some() {
                        let member_default = bool_field(declaration, "default-features", true)?;
                        if default_features && !member_default {
                            return Err(format!(
                                "{manifest}: cannot disable inherited default-features for {name}; declare false in workspace.dependencies"
                            ));
                        }
                        default_features |= member_default;
                    }
                    let mut features = strings(
                        dependency.get("features"),
                        &format!("{document}: dependency {name}.features"),
                    )?;
                    if inherited {
                        features.extend(strings(
                            declaration.get("features"),
                            &format!("{manifest}: dependency {name}.features"),
                        )?);
                    }
                    features.sort();
                    features.dedup();
                    if dependency.get("git").is_some() {
                        return Err(format!(
                            "{document}: local dependency {name} also declares git"
                        ));
                    }
                    let target = manifest_at(&normalize(directory(&document), path)?);
                    let target_package = self.packages.get(&target).ok_or_else(|| format!("{manifest}: local dependency {name} is not in registered workspaces: {target}"))?;
                    let expected = dependency
                        .get("package")
                        .and_then(Item::as_str)
                        .unwrap_or(name);
                    if expected != target_package.name {
                        return Err(format!(
                            "{document}: dependency {name} package does not match {target}"
                        ));
                    }
                    if let Some(requirement) = dependency.get("version") {
                        let requirement = requirement
                            .as_str()
                            .ok_or_else(|| format!("{document}: invalid version for {name}"))?;
                        if !accepts(requirement, target_package.version)? {
                            return Err(format!(
                                "{document}: dependency {name} requirement {requirement} does not accept {}",
                                target_package.version
                            ));
                        }
                    }
                    self.dependencies.push(Dependency {
                        owner: manifest.clone(),
                        target,
                        kind,
                        alias: name.to_string(),
                        optional,
                        features,
                        default_features,
                        document,
                        keys,
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_locks(&self, legacy_manifests: Option<&BTreeSet<String>>) -> Result<(), String> {
        for (root, members) in &self.roots {
            if legacy_manifests.is_some_and(|manifests| {
                !manifests.contains(root) && members.is_disjoint(manifests)
            }) {
                continue;
            }
            let lock = if directory(root).is_empty() {
                "Cargo.lock".to_owned()
            } else {
                format!("{}/Cargo.lock", directory(root))
            };
            let Some(document) = self.documents.get(&lock) else {
                continue;
            };
            if !matches!(
                document.get("version").and_then(Item::as_integer),
                Some(3 | 4)
            ) {
                return Err(format!(
                    "{lock}: Rust release supports only Cargo.lock format version 3 or 4"
                ));
            }
            let entries = document
                .get("package")
                .and_then(Item::as_array_of_tables)
                .ok_or_else(|| format!("{lock}: missing package entries"))?;
            let closure = self.required_lock_closure(members);
            for manifest in &closure {
                let package = &self.packages[manifest];
                let matches = entries
                    .iter()
                    .filter(|entry| {
                        entry.get("source").is_none()
                            && entry.get("name").and_then(Item::as_str)
                                == Some(package.name.as_str())
                    })
                    .collect::<Vec<_>>();
                if matches.len() != 1
                    || matches[0].get("version").and_then(Item::as_str)
                        != Some(package.version.to_string().as_str())
                {
                    return Err(format!(
                        "{lock}: local package {} has missing, ambiguous, or stale version",
                        package.name
                    ));
                }
            }
            for entry in entries {
                if entry.get("source").is_some() {
                    continue;
                }
                let name = entry
                    .get("name")
                    .and_then(Item::as_str)
                    .ok_or_else(|| format!("{lock}: local lock entry has no name"))?;
                let version = entry
                    .get("version")
                    .and_then(Item::as_str)
                    .ok_or_else(|| format!("{lock}: local lock entry has no version"))?;
                let matches = self
                    .packages
                    .values()
                    .filter(|p| p.name == name && p.version.to_string() == version)
                    .count();
                if matches != 1 {
                    return Err(format!(
                        "{lock}: local lock package {name} {version} has no unique registered identity"
                    ));
                }
            }
        }
        Ok(())
    }

    /// The common lifecycle path starts from the post-refactor lock. Validate
    /// its references as well as identities so a renamed/retired local package
    /// cannot survive only in an old dependency identifier. Schema 1/2 retain
    /// their original historical contract through `load` above.
    fn validate_lock_edges(&self) -> Result<(), String> {
        for (root, members) in &self.roots {
            let lock = if directory(root).is_empty() {
                "Cargo.lock".to_owned()
            } else {
                format!("{}/Cargo.lock", directory(root))
            };
            let Some(document) = self.documents.get(&lock) else {
                continue;
            };
            let entries = document
                .get("package")
                .and_then(Item::as_array_of_tables)
                .ok_or_else(|| format!("{lock}: missing package entries"))?;
            for entry in entries {
                let local_owner = if entry.get("source").is_none() {
                    self.packages.iter().find(|(_, package)| {
                        entry.get("name").and_then(Item::as_str) == Some(package.name.as_str())
                            && entry.get("version").and_then(Item::as_str)
                                == Some(package.version.to_string().as_str())
                    })
                } else {
                    None
                };
                let dependencies = match entry.get("dependencies") {
                    Some(item) => item
                        .as_array()
                        .ok_or_else(|| format!("{lock}: dependencies must be an array"))?,
                    None => continue,
                };
                for dependency in dependencies {
                    let raw = dependency
                        .as_str()
                        .ok_or_else(|| format!("{lock}: invalid dependency identifier"))?;
                    let parts = raw.split_whitespace().collect::<Vec<_>>();
                    if parts.is_empty()
                        || parts.len() > 3
                        || (parts.len() == 3
                            && !(parts[2].starts_with('(') && parts[2].ends_with(')')))
                    {
                        return Err(format!("{lock}: unsupported dependency identifier {raw}"));
                    }
                    let candidates = entries
                        .iter()
                        .filter(|candidate| {
                            candidate.get("name").and_then(Item::as_str) == Some(parts[0])
                                && (parts.len() == 1
                                    || candidate.get("version").and_then(Item::as_str)
                                        == Some(parts[1]))
                                && (parts.len() < 3
                                    || candidate.get("source").and_then(Item::as_str)
                                        == Some(&parts[2][1..parts[2].len() - 1]))
                        })
                        .collect::<Vec<_>>();
                    // Cargo disambiguates registry/git identities with source
                    // qualifiers while the source-less path identity remains
                    // unqualified, even when both share a name and version.
                    let locals = candidates
                        .iter()
                        .filter(|candidate| candidate.get("source").is_none())
                        .count();
                    let matches = if parts.len() < 3 && locals == 1 {
                        1
                    } else {
                        candidates.len()
                    };
                    if matches != 1 {
                        return Err(format!(
                            "{lock}: dependency identifier {raw} has missing or ambiguous target"
                        ));
                    }
                    // A target can remain legitimately locked through another
                    // owner after this owner removed its dependency. Merely
                    // resolving the old identifier would miss that stale edge.
                    // Compare with all declared kinds/conditions/optional edges;
                    // platform and feature selection must not erase declarations.
                    if let Some((owner_manifest, owner)) = local_owner {
                        if parts.len() < 3 && locals == 1 {
                            let target_entry = candidates
                                .iter()
                                .find(|candidate| candidate.get("source").is_none())
                                .unwrap();
                            let (target_manifest, target) = self
                                .packages
                                .iter()
                                .find(|(_, package)| {
                                    target_entry.get("name").and_then(Item::as_str)
                                        == Some(package.name.as_str())
                                        && target_entry.get("version").and_then(Item::as_str)
                                            == Some(package.version.to_string().as_str())
                                })
                                .ok_or_else(|| {
                                    format!(
                                        "{lock}: local dependency target has no registered identity"
                                    )
                                })?;
                            if !self.dependencies.iter().any(|dependency| {
                                dependency.owner == *owner_manifest
                                    && dependency.target == *target_manifest
                            }) {
                                return Err(format!(
                                    "{lock}: stale local dependency edge {} -> {} is absent from current declarations",
                                    owner.name, target.name
                                ));
                            }
                        }
                    }
                }
            }
            let (_, required_edges) = self.required_lock_graph(members);
            for (owner_manifest, target_manifest) in required_edges {
                let owner = &self.packages[&owner_manifest];
                let target = &self.packages[&target_manifest];
                let entry = entries
                    .iter()
                    .find(|entry| {
                        entry.get("source").is_none()
                            && entry.get("name").and_then(Item::as_str) == Some(owner.name.as_str())
                            && entry.get("version").and_then(Item::as_str)
                                == Some(owner.version.to_string().as_str())
                    })
                    .ok_or_else(|| format!("{lock}: missing local owner {}", owner.name))?;
                let present = entry
                    .get("dependencies")
                    .and_then(Item::as_array)
                    .is_some_and(|items| {
                        items.iter().filter_map(|item| item.as_str()).any(|raw| {
                            let parts = raw.split_whitespace().collect::<Vec<_>>();
                            !parts.is_empty()
                                && parts[0] == target.name
                                && (parts.len() == 1
                                    || (parts.len() == 2 && parts[1] == target.version.to_string()))
                        })
                    });
                if !present {
                    return Err(format!(
                        "{lock}: missing local dependency edge {} -> {}",
                        owner.name, target.name
                    ));
                }
            }
        }
        Ok(())
    }

    /// Resolve only the local feature implications needed to prove lock entries
    /// mandatory. Registry feature resolution remains Cargo's responsibility.
    /// Workspace members are locked with all features; external path packages
    /// receive only defaults and features enabled by their incoming dependencies.
    fn required_lock_closure(&self, members: &BTreeSet<String>) -> BTreeSet<String> {
        self.required_lock_graph(members).0
    }

    fn required_lock_graph(
        &self,
        members: &BTreeSet<String>,
    ) -> (BTreeSet<String>, BTreeSet<(String, String)>) {
        let mut closure = members.clone();
        let mut edges = BTreeSet::new();
        let mut features = members
            .iter()
            .map(|manifest| {
                (
                    manifest.clone(),
                    self.packages[manifest]
                        .features
                        .keys()
                        .cloned()
                        .collect::<BTreeSet<_>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        loop {
            let mut changed = false;
            for owner in closure.clone() {
                let empty = BTreeSet::new();
                let (optional, requests) =
                    self.local_feature_requests(&owner, features.get(&owner).unwrap_or(&empty));
                for dependency in self.dependencies.iter().filter(|d| d.owner == owner) {
                    if dependency.kind == DependencyKind::Dev && !members.contains(&owner) {
                        continue;
                    }
                    if dependency.optional
                        && !members.contains(&owner)
                        && !optional.contains(&dependency.alias)
                    {
                        continue;
                    }
                    edges.insert((dependency.owner.clone(), dependency.target.clone()));
                    changed |= closure.insert(dependency.target.clone());
                    let enabled = features.entry(dependency.target.clone()).or_default();
                    for feature in &dependency.features {
                        changed |= enabled.insert(feature.clone());
                    }
                    if dependency.default_features {
                        changed |= enabled.insert("default".into());
                    }
                    if let Some(requests) = requests.get(&dependency.alias) {
                        for feature in requests {
                            changed |= enabled.insert(feature.clone());
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        (closure, edges)
    }

    fn local_feature_requests(
        &self,
        owner: &str,
        requested: &BTreeSet<String>,
    ) -> (BTreeSet<String>, BTreeMap<String, BTreeSet<String>>) {
        let package = &self.packages[owner];
        let mut enabled = requested.clone();
        let mut optional = BTreeSet::new();
        let mut dependency_features = BTreeMap::<String, BTreeSet<String>>::new();
        loop {
            let mut changed = false;
            for feature in enabled.clone() {
                if let Some(expressions) = package.features.get(&feature) {
                    for expression in expressions {
                        if let Some(alias) = expression.strip_prefix("dep:") {
                            changed |= optional.insert(alias.to_string());
                        } else if let Some((alias, target_feature)) = expression.split_once('/') {
                            let weak = alias.ends_with('?');
                            let alias = alias.trim_end_matches('?');
                            if !weak {
                                changed |= optional.insert(alias.to_string());
                            }
                            let required = self
                                .dependencies
                                .iter()
                                .any(|d| d.owner == owner && d.alias == alias && !d.optional);
                            if !weak || required || optional.contains(alias) {
                                changed |= dependency_features
                                    .entry(alias.to_string())
                                    .or_default()
                                    .insert(target_feature.to_string());
                            }
                        } else {
                            changed |= enabled.insert(expression.clone());
                        }
                    }
                } else if self
                    .dependencies
                    .iter()
                    .any(|d| d.owner == owner && d.alias == feature && d.optional)
                {
                    // Cargo's implicit feature named after an optional dependency.
                    changed |= optional.insert(feature);
                }
            }
            if !changed {
                break;
            }
        }
        (optional, dependency_features)
    }

    fn render_versions(&self, versions: &BTreeMap<String, SemVer>) -> Result<Writes, String> {
        let mut documents = self.documents.clone();
        let mut touched = BTreeSet::new();
        // Literal versions split inherited workspace versions without bumping
        // unaffected packages. Future releases cannot silently reunify them.
        for (manifest, package) in &self.packages {
            if package.inherited
                || versions
                    .get(manifest)
                    .is_some_and(|next| *next != package.version)
            {
                documents.get_mut(manifest).unwrap()["package"]["version"] = value(
                    versions
                        .get(manifest)
                        .unwrap_or(&package.version)
                        .to_string(),
                );
                touched.insert(manifest.clone());
            }
        }
        for dependency in &self.dependencies {
            let Some(next) = versions.get(&dependency.target) else {
                continue;
            };
            let document = documents.get_mut(&dependency.document).unwrap();
            let mut item = document.as_item_mut();
            for key in &dependency.keys {
                item = item
                    .get_mut(key)
                    .ok_or_else(|| format!("{}: dependency path changed", dependency.document))?;
            }
            if let Some(requirement) = item.get("version").and_then(Item::as_str) {
                if !accepts(requirement, *next)? {
                    let updated = updated_requirement(requirement, *next)?;
                    if let Some(inline) = item.as_inline_table_mut() {
                        inline.insert("version", updated.into());
                    } else if let Some(table) = item.as_table_mut() {
                        table.insert("version", value(updated));
                    } else {
                        return Err("local dependency must be a table".into());
                    }
                    touched.insert(dependency.document.clone());
                }
            }
        }
        self.render_locks(versions, &mut documents, &mut touched)?;
        let mut writes = BTreeMap::new();
        for path in touched {
            let bytes = documents[&path].to_string().into_bytes();
            if self.inputs.get(&path).and_then(Option::as_ref) != Some(&bytes) {
                writes.insert(path, bytes);
            }
        }
        Ok(writes)
    }

    fn render_locks(
        &self,
        versions: &BTreeMap<String, SemVer>,
        documents: &mut BTreeMap<String, DocumentMut>,
        touched: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        for lock in &self.locks {
            let Some(document) = documents.get_mut(lock) else {
                continue;
            };
            let entries = document
                .get_mut("package")
                .and_then(Item::as_array_of_tables_mut)
                .ok_or_else(|| format!("{lock}: invalid package entries"))?;
            // Identity comes from a source-less lock entry, never a name-only
            // replacement; registry packages with identical versions survive.
            let mut replacements = BTreeMap::new();
            for entry in entries.iter_mut() {
                if entry.get("source").is_some() {
                    continue;
                }
                let name = entry
                    .get("name")
                    .and_then(Item::as_str)
                    .unwrap_or("")
                    .to_string();
                let old = entry
                    .get("version")
                    .and_then(Item::as_str)
                    .unwrap_or("")
                    .to_string();
                let manifest = self
                    .packages
                    .iter()
                    .find(|(_, p)| p.name == name && p.version.to_string() == old)
                    .map(|(m, _)| m);
                if let Some(next) = manifest.and_then(|m| versions.get(m)) {
                    if next.to_string() == old {
                        continue;
                    }
                    replacements.insert((name, old), next.to_string());
                    entry.insert("version", value(next.to_string()));
                    touched.insert(lock.clone());
                }
            }
            for entry in entries.iter_mut() {
                if let Some(dependencies) =
                    entry.get_mut("dependencies").and_then(Item::as_array_mut)
                {
                    for dependency in dependencies.iter_mut() {
                        let raw = dependency
                            .as_str()
                            .ok_or_else(|| format!("{lock}: invalid dependency identifier"))?;
                        let parts = raw.split_whitespace().collect::<Vec<_>>();
                        // Source-qualified registry/git dependencies are external.
                        if parts.len() == 2 {
                            if let Some(next) =
                                replacements.get(&(parts[0].into(), parts[1].into()))
                            {
                                *dependency = format!("{} {next}", parts[0]).into();
                                touched.insert(lock.clone());
                            }
                        } else if parts.len() > 2 && !parts[2].starts_with('(') {
                            return Err(format!("{lock}: unsupported dependency identifier {raw}"));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// Canonical semantic comparison without depending on serialization of Item
// (which retains trivia and formatting rather than TOML values).
fn comparator(raw: &str, version: SemVer) -> Result<bool, String> {
    let raw = raw.trim();
    if raw == "*" {
        return Ok(true);
    }
    let (operator, number) = [">=", "<=", ">", "<", "=", "^", "~"]
        .into_iter()
        .find_map(|op| raw.strip_prefix(op).map(|n| (op, n.trim())))
        .unwrap_or(("^", raw));
    let parts = number.split('.').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 3 {
        return Err(format!("unsupported Cargo version requirement: {raw}"));
    }
    let mut numbers = Vec::new();
    let mut wildcard = false;
    for part in &parts {
        if matches!(*part, "*" | "x" | "X") {
            wildcard = true;
            break;
        }
        if part.is_empty()
            || !part.bytes().all(|b| b.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return Err(format!("unsupported Cargo version requirement: {raw}"));
        }
        numbers.push(
            part.parse::<u64>()
                .map_err(|_| format!("unsupported Cargo version requirement: {raw}"))?,
        );
    }
    if wildcard
        && parts[numbers.len()..]
            .iter()
            .any(|p| !matches!(*p, "*" | "x" | "X"))
    {
        return Err(format!("unsupported Cargo version requirement: {raw}"));
    }
    if numbers.is_empty() {
        if operator != "^" && operator != "=" {
            return Err(format!("unsupported Cargo version requirement: {raw}"));
        }
        return Ok(true);
    }
    if wildcard && !matches!(operator, "^" | "=") {
        return Err(format!("unsupported Cargo version requirement: {raw}"));
    }
    let lower = SemVer {
        major: numbers[0],
        minor: *numbers.get(1).unwrap_or(&0),
        patch: *numbers.get(2).unwrap_or(&0),
    };
    let upper =
        if wildcard || operator == "~" || (matches!(operator, ">" | "<=") && numbers.len() < 3) {
            if numbers.len() == 1 {
                SemVer {
                    major: lower.major.checked_add(1).ok_or("version bound overflow")?,
                    minor: 0,
                    patch: 0,
                }
            } else {
                SemVer {
                    minor: lower.minor.checked_add(1).ok_or("version bound overflow")?,
                    patch: 0,
                    ..lower
                }
            }
        } else if lower.major > 0 || numbers.len() == 1 {
            SemVer {
                major: lower.major.checked_add(1).ok_or("version bound overflow")?,
                minor: 0,
                patch: 0,
            }
        } else if lower.minor > 0 || numbers.len() == 2 {
            SemVer {
                minor: lower.minor.checked_add(1).ok_or("version bound overflow")?,
                patch: 0,
                ..lower
            }
        } else {
            SemVer {
                patch: lower.patch.checked_add(1).ok_or("version bound overflow")?,
                ..lower
            }
        };
    Ok(match operator {
        ">=" => version >= lower,
        ">" => {
            if numbers.len() == 3 {
                version > lower
            } else {
                version >= upper
            }
        }
        "<" => version < lower,
        "<=" => {
            if numbers.len() == 3 {
                version <= lower
            } else {
                version < upper
            }
        }
        "=" if !wildcard => {
            version.major == lower.major
                && (numbers.len() < 2 || version.minor == lower.minor)
                && (numbers.len() < 3 || version.patch == lower.patch)
        }
        _ => version >= lower && version < upper,
    })
}

fn accepts(raw: &str, version: SemVer) -> Result<bool, String> {
    let mut accepted = true;
    for component in raw.split(',') {
        accepted &= comparator(component, version)?;
    }
    Ok(accepted)
}

fn updated_requirement(raw: &str, version: SemVer) -> Result<String, String> {
    if raw.contains(',') || raw.trim().starts_with(['>', '<']) {
        return Err(format!(
            "cannot safely widen internal Cargo requirement {raw}; configure an explicit supported requirement"
        ));
    }
    let prefix = if raw.trim().starts_with('=') {
        "="
    } else if raw.trim().starts_with('~') {
        "~"
    } else if raw.trim().starts_with('^') {
        "^"
    } else {
        ""
    };
    Ok(format!("{prefix}{version}"))
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../../scripts/tests/unit/core_release_rust.rs"]
mod tests;
