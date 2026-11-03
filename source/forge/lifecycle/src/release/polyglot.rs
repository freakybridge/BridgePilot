//! Snapshot-driven orchestration shared by the Rust and Python adapters.
use super::lifecycle::{Identity, Transition};
use super::model::{Language, LanguageSnapshot, NativeDependency, NativePackage};
use super::*;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Adapters {
    pub rust: Option<rust::RustConfig>,
    pub python: Option<python::Config>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SharedInput {
    pub path: String,
    pub packages: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CrossDependency {
    pub owner: String,
    pub target: String,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub schema_version: u32,
    pub release_policy: String,
    pub strategy: String,
    pub git_sync_setup: Option<setup::Confirmation>,
    pub adapters: Adapters,
    /// An explicit adapter contract for revisions predating schema 3. This
    /// avoids interpreting old Python files with current dependency mappings.
    pub bootstrap_adapters: Option<Adapters>,
    pub packages: Vec<Identity>,
    /// Explicit native product-version carriers in a mixed Rust workspace.
    /// These are validated in the native graph but never independently bumped.
    #[serde(default)]
    pub product_packages: Vec<Identity>,
    #[serde(default)]
    pub lifecycle: Vec<Transition>,
    #[serde(default)]
    pub shared_inputs: Vec<SharedInput>,
    #[serde(default)]
    pub product_inputs: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<CrossDependency>,
    #[serde(default)]
    pub target_versions: BTreeMap<String, String>,
    #[serde(default)]
    pub target_levels: BTreeMap<String, u8>,
}

const CONFIG: &str = ".codex/bridgeforge-version.json";

pub(super) fn parse(payload: &[u8]) -> Result<Option<Config>, String> {
    let value = crate::json::parse_unique_json(payload, "version sync config")?;
    if value["schema_version"].as_u64() == Some(4) {
        return super::schema4::parse(payload).map(|cfg| cfg.map(|cfg| cfg.native));
    }
    if value["schema_version"].as_u64() != Some(3) {
        return Ok(None);
    }
    setup::confirmation(&value)?;
    let mut cfg: Config = serde_json::from_value(value)
        .map_err(|e| format!("invalid schema 3 release config: {e}"))?;
    for p in cfg.packages.iter_mut().chain(
        cfg.lifecycle
            .iter_mut()
            .flat_map(|t| t.from.iter_mut().chain(t.to.iter_mut())),
    ) {
        if p.language == Language::Python {
            p.name = python_dependencies::normalize_name(&p.name)?;
        }
    }
    if cfg.schema_version != 3
        || cfg.release_policy != "explicit_release"
        || cfg.strategy != "independent"
    {
        return Err("schema 3 requires explicit_release and strategy=independent".into());
    }
    let mut ids = BTreeSet::new();
    let mut manifests = BTreeSet::new();
    for p in &cfg.packages {
        validate_identity(p)?;
        if !ids.insert(&p.id) || !manifests.insert(&p.manifest) {
            return Err("schema 3 has duplicate package id or manifest".into());
        }
    }
    for p in &cfg.product_packages {
        validate_identity(p)?;
        if p.language != Language::Rust
            || p.id == "product"
            || !ids.insert(&p.id)
            || !manifests.insert(&p.manifest)
        {
            return Err("product packages require unique Rust identities disjoint from independent packages".into());
        }
    }
    let mut events = BTreeSet::new();
    for t in &cfg.lifecycle {
        t.validate()?;
        if !events.insert(&t.id) {
            return Err("duplicate lifecycle event id".into());
        }
        for p in t.from.iter().chain(&t.to) {
            validate_identity(p)?;
        }
    }
    for p in &cfg.product_inputs {
        relative(p)?;
    }
    if cfg.product_inputs.iter().collect::<BTreeSet<_>>().len() != cfg.product_inputs.len() {
        return Err("overlapping shared/product ownership".into());
    }
    for mapping in &cfg.shared_inputs {
        relative(&mapping.path)?;
        if mapping.packages.is_empty() {
            return Err("shared input has no package owners".into());
        }
        if mapping
            .packages
            .iter()
            .any(|id| !cfg.packages.iter().any(|p| &p.id == id))
        {
            return Err("shared input references an unknown active package id".into());
        }
    }
    let patterns = cfg
        .product_inputs
        .iter()
        // Nested product declarations all select the same owner. Keep the
        // original config intact, but check overlap using only covering roots.
        .filter(|path| !cfg.product_inputs.iter().any(|parent| {
            parent != *path && owns(parent.as_str(), path.as_str())
        }))
        .chain(cfg.shared_inputs.iter().map(|m| &m.path))
        .collect::<Vec<_>>();
    for (i, a) in patterns.iter().enumerate() {
        if patterns[i + 1..].iter().any(|b| owns(a, b) || owns(b, a)) {
            return Err("overlapping shared/product ownership".into());
        }
    }
    for d in &cfg.dependencies {
        if !cfg.packages.iter().any(|p| p.id == d.owner)
            || !cfg.packages.iter().any(|p| p.id == d.target)
            || d.owner == d.target
            || !matches!(d.kind.as_str(), "build" | "runtime" | "dev")
        {
            return Err("invalid explicit package dependency".into());
        }
    }
    if cfg
        .target_versions
        .keys()
        .any(|id| !cfg.packages.iter().any(|p| &p.id == id))
    {
        return Err("target version has an unknown package id".into());
    }
    if cfg
        .target_levels
        .iter()
        .any(|(id, level)| !cfg.target_versions.contains_key(id) || !(1..=3).contains(level))
    {
        return Err("explicit target level must bind a target version and be 1..3".into());
    }
    let _ = &cfg.git_sync_setup;
    validate_version_ownership(&cfg)?;
    Ok(Some(cfg))
}

fn validate_identity(p: &Identity) -> Result<(), String> {
    if p.id.is_empty()
        || !p
            .id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.:".contains(&c))
        || p.name.is_empty()
    {
        return Err("package identity requires a stable ASCII id and native name".into());
    }
    if let Some(root) = &p.workspace {
        relative(root)?;
    }
    relative(&p.manifest)
}

fn relative(path: &str) -> Result<(), String> {
    let path = path.strip_suffix('/').unwrap_or(path);
    if path.is_empty()
        || path.contains('\\')
        || Path::new(path).is_absolute()
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || path.split('/').any(|p| {
            matches!(
                p,
                ".git" | ".runtime" | ".deps" | ".build" | ".venv" | "target"
            )
        })
    {
        return Err(format!("unsafe release input path: {path}"));
    }
    Ok(())
}

pub(super) fn load_config(root: &Path) -> Result<Option<Config>, String> {
    config::read_current(root, CONFIG)?
        .map(|p| parse(&p))
        .transpose()
        .map(Option::flatten)
}

#[derive(Clone, Debug, Default)]
pub(super) struct Snapshot {
    pub packages: BTreeMap<String, NativePackage>,
    pub dependencies: Vec<NativeDependency>,
    pub inputs: model::Inputs,
    pub config: Option<Config>,
}

pub(super) fn owns(prefix: &str, path: &str) -> bool {
    prefix == path || prefix.ends_with('/') && path.starts_with(prefix)
}

fn combine(target: &mut LanguageSnapshot, source: LanguageSnapshot) -> Result<(), String> {
    for (p, native) in source.packages {
        if target.packages.insert(p.clone(), native).is_some() {
            return Err(format!("duplicate adapter ownership: {p}"));
        }
    }
    for (p, bytes) in source.inputs {
        if let Some(old) = target.inputs.insert(p.clone(), bytes.clone()) {
            if old != bytes {
                return Err(format!("adapters disagree on input: {p}"));
            }
        }
    }
    target.dependencies.extend(source.dependencies);
    target.roots.extend(source.roots);
    Ok(())
}

pub(super) fn snapshot(
    cfg: &Config,
    paths: Vec<String>,
    historical: bool,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<Snapshot, String> {
    let payload = read(CONFIG)?;
    let _profile = crate::release_metrics::span(if historical {
        "snapshot.historical"
    } else {
        "snapshot.current"
    });
    let at = payload.as_deref().map(parse).transpose()?.flatten();
    let mut adapters = if historical {
        at.as_ref()
            .map(|c| c.adapters.clone())
            .or_else(|| cfg.bootstrap_adapters.clone())
            .unwrap_or_else(|| cfg.adapters.clone())
    } else {
        cfg.adapters.clone()
    };
    // Old Rust revisions retain their own scope and lock-validation contract.
    let old_rust = if historical && at.is_none() {
        payload.as_deref().map(config::parse).transpose()?.flatten()
    } else {
        None
    };
    if let Some(old) = &old_rust {
        adapters.rust = Some(old.clone());
    }
    let mut native = LanguageSnapshot::default();
    if let Some(mut rust_cfg) = adapters.rust {
        if historical && at.is_none() && old_rust.is_none() && cfg.bootstrap_adapters.is_none() {
            rust_cfg.workspaces.retain(|p| paths.contains(p));
        }
        if !rust_cfg.workspaces.is_empty() {
            let loaded = if historical && at.is_none() {
                let value = payload
                    .as_deref()
                    .map(|p| crate::json::parse_unique_json(p, "historical config"))
                    .transpose()?;
                let scope = if value
                    .as_ref()
                    .is_some_and(|v| v["schema_version"].as_u64() == Some(1))
                {
                    let value = value.as_ref().unwrap();
                    Some(if config::auto_manifests(value)? {
                        ["Cargo.toml", "package.json"]
                            .iter()
                            .filter(|p| paths.iter().any(|x| x == **p))
                            .map(|p| p.to_string())
                            .collect::<BTreeSet<_>>()
                    } else {
                        value["manifests"]
                            .as_array()
                            .ok_or("historical manifests missing")?
                            .iter()
                            .map(|p| {
                                p.as_str()
                                    .map(str::to_string)
                                    .ok_or("invalid historical manifest".to_string())
                            })
                            .collect::<Result<BTreeSet<_>, _>>()?
                    })
                } else {
                    None
                };
                let workspace = if let Some(scope) = scope {
                    rust::load_legacy_history(&rust_cfg, &paths, &scope, read)?
                } else {
                    rust::load(&rust_cfg, &paths, read)?
                };
                rust::from_workspace(workspace, read)?
            } else {
                rust::snapshot(&rust_cfg, &paths, read)?
            };
            combine(&mut native, loaded)?;
        }
    }
    if let Some(mut python_cfg) = adapters.python {
        if historical && at.is_none() && cfg.bootstrap_adapters.is_none() {
            python_cfg.projects.retain(|p| paths.contains(p));
            python_cfg
                .version_sources
                .retain(|p, _| python_cfg.projects.contains(p));
            python_cfg
                .import_roots
                .retain(|p, _| python_cfg.projects.contains(p));
            python_cfg
                .local_sources
                .retain(|p, _| python_cfg.projects.contains(p));
            if python_cfg
                .local_sources
                .values()
                .any(|mapping| mapping.values().any(|p| !python_cfg.projects.contains(p)))
            {
                return Err("historical Python sources require explicit bootstrap_adapters".into());
            }
            if python_cfg
                .locks
                .iter()
                .any(|spec| !paths.contains(&spec.path))
            {
                return Err(
                    "historical Python lock scope requires explicit bootstrap_adapters".into(),
                );
            }
        }
        if !python_cfg.projects.is_empty() {
            combine(&mut native, python::snapshot(&python_cfg, &paths, read)?)?;
        }
    }
    let candidates = cfg
        .packages
        .iter()
        .chain(
            cfg.lifecycle
                .iter()
                .flat_map(|t| t.from.iter().chain(&t.to)),
        )
        .chain(at.iter().flat_map(|c| c.packages.iter()))
        .collect::<Vec<_>>();
    let mut manifests = BTreeMap::new();
    let product_declarations = if historical {
        at.as_ref()
            .map(|c| c.product_packages.as_slice())
            .unwrap_or(&cfg.product_packages)
    } else {
        &cfg.product_packages
    };
    let mut product_manifests = BTreeSet::new();
    let mut result = Snapshot {
        config: if historical {
            at.clone()
        } else {
            Some(cfg.clone())
        },
        ..Snapshot::default()
    };
    for (manifest, package) in native.packages {
        let product_matches = product_declarations
            .iter()
            .filter(|p| lifecycle::matches(p, &package))
            .count();
        if product_matches != 0 {
            if product_matches != 1 || !paths.contains(&manifest) {
                return Err(format!(
                    "{manifest}: product package identity is ambiguous or outside Git inventory"
                ));
            }
            product_manifests.insert(manifest);
            continue;
        }
        let declared = if historical {
            at.as_ref().map(|c| &c.packages)
        } else {
            Some(&cfg.packages)
        };
        let mut ids = declared
            .into_iter()
            .flatten()
            .filter(|p| lifecycle::matches(p, &package))
            .map(|p| p.id.clone())
            .collect::<BTreeSet<_>>();
        if ids.is_empty() {
            ids = candidates
                .iter()
                .filter(|p| lifecycle::matches(p, &package))
                .map(|p| p.id.clone())
                .collect();
        }
        if ids.len() != 1 {
            return Err(format!(
                "{manifest}: package needs one unambiguous registered identity"
            ));
        }
        let id = ids.into_iter().next().unwrap();
        if !paths.contains(&manifest) {
            return Err(format!("{manifest}: package is outside Git inventory"));
        }
        manifests.insert(manifest, id.clone());
        if result.packages.insert(id, package).is_some() {
            return Err("two live packages share an identity".into());
        }
    }
    if !historical
        && cfg.packages.iter().any(|p| {
            !result
                .packages
                .get(&p.id)
                .is_some_and(|n| lifecycle::matches(p, n))
        })
    {
        return Err("registered package is missing from current adapter discovery".into());
    }
    if !historical
        && product_declarations
            .iter()
            .any(|p| !product_manifests.contains(&p.manifest))
    {
        return Err("product package is missing from current adapter discovery".into());
    }
    for dependency in native.dependencies {
        // The Rust adapter has already checked the complete graph and lock,
        // including product callers. Product carriers have no independent ledger.
        if product_manifests.contains(&dependency.owner) {
            continue;
        }
        if product_manifests.contains(&dependency.target) {
            return Err("an independent package cannot depend on a product-version carrier".into());
        }
        result.dependencies.push(NativeDependency {
            owner: manifests
                .get(&dependency.owner)
                .ok_or("dependency owner not registered")?
                .clone(),
            target: manifests
                .get(&dependency.target)
                .ok_or("dependency target not registered")?
                .clone(),
            kind: dependency.kind,
        });
    }
    let edges = result
        .config
        .as_ref()
        .map(|c| &c.dependencies)
        .unwrap_or(&cfg.dependencies);
    for edge in edges {
        if result.packages.contains_key(&edge.owner) && result.packages.contains_key(&edge.target) {
            result.dependencies.push(NativeDependency {
                owner: edge.owner.clone(),
                target: edge.target.clone(),
                kind: edge.kind.clone(),
            });
        } else if !historical {
            return Err("cross-language dependency refers to a missing package".into());
        }
    }
    result.inputs = native.inputs;
    result.inputs.insert(CONFIG.into(), payload);
    if !historical {
        validate_version_ownership(cfg)?;
    }
    Ok(result)
}

pub(super) fn historical(
    root: &Path,
    revision: &str,
    cfg: &Config,
    runner: &dyn ProcessRunner,
) -> Result<Snapshot, String> {
    let _profile = crate::release_metrics::span(format!("history.snapshot.{revision}"));
    let paths = history::paths(&history::git(
        root,
        runner,
        &["ls-tree", "-r", "--name-only", "-z", revision],
    )?);
    snapshot(cfg, paths, true, &mut |p| {
        revision_payload(root, revision, p, runner)
    })
}

pub(super) fn verify_historical(
    root: &Path,
    revision: &str,
    cfg: &Config,
    runner: &dyn ProcessRunner,
) -> Result<(), String> {
    historical(root, revision, cfg, runner).map(|_| ())
}

pub(super) fn validate_current(
    root: &Path,
    boundary: &str,
    cfg: &Config,
    runner: &dyn ProcessRunner,
) -> Result<(), String> {
    let current = snapshot(
        cfg,
        independent::inventory(root, runner)?,
        false,
        &mut |p| config::read_current(root, p),
    )?;
    let old = historical(root, boundary, cfg, runner)?;
    for (id, p) in &current.packages {
        if old.packages.get(id).is_some_and(|b| b.version != p.version) {
            return Err(format!(
                "{id}: native version differs from published baseline"
            ));
        }
    }
    Ok(())
}

pub(super) fn natural_owners(snapshot: &Snapshot, path: &str) -> Vec<String> {
    // An explicit external target cannot steal a file from its natural package.
    // Resolve natural roots with the existing longest-root rule, then union
    // exact target declarations and require shared_inputs for multiple owners.
    let explicit = snapshot
        .packages
        .iter()
        .filter(|(_, p)| p.language == Language::Rust && p.source_roots.iter().any(|r| r == path))
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    let best = snapshot
        .packages
        .iter()
        .filter_map(|(id, p)| {
            let size = p
                .source_roots
                .iter()
                .filter(|r| {
                    r.is_empty()
                        || (path == r.as_str()
                            && (p.language != Language::Rust || explicit.is_empty()))
                        || ((p.language != Language::Rust || r.ends_with('/'))
                            && path.starts_with(&format!("{}/", r.trim_end_matches('/'))))
                })
                .map(String::len)
                .max();
            if p.manifest == path {
                Some((id, usize::MAX))
            } else {
                size.map(|n| (id, n))
            }
        })
        .collect::<Vec<_>>();
    let max = best.iter().map(|(_, size)| *size).max();
    best.into_iter()
        .filter(|(_, size)| Some(*size) == max)
        .map(|(id, _)| id.clone())
        .chain(explicit)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Only snapshots rebuilt by the adapters supply this proof. A directory root
/// alone, a missing target or an opaque macro is not evidence of sharing.
/// Existing explicit roles always win and are never widened or replaced.
pub(super) fn proven_shared_inputs(snapshot: &Snapshot, cfg: &Config) -> Vec<SharedInput> {
    let scope = snapshot.config.as_ref().unwrap_or(cfg);
    let candidates = snapshot
        .packages
        .values()
        .filter(|p| p.language == Language::Rust)
        .flat_map(|p| p.source_roots.iter())
        .filter(|p| !p.is_empty() && !p.ends_with('/'))
        .collect::<BTreeSet<_>>();
    candidates
        .into_iter()
        .filter_map(|path| {
            if scope.product_inputs.iter().any(|p| owns(p, path))
                || scope.shared_inputs.iter().any(|p| owns(&p.path, path))
                || !snapshot.inputs.get(path).is_some_and(Option::is_some)
            {
                return None;
            }
            let packages = natural_owners(snapshot, path);
            (packages.len() > 1
                && packages
                    .iter()
                    .all(|id| snapshot.packages[id].language == Language::Rust))
            .then(|| SharedInput {
                path: path.clone(),
                packages,
            })
        })
        .collect()
}

pub(super) fn owner(snapshot: &Snapshot, path: &str, cfg: &Config) -> Result<Vec<String>, String> {
    let scope = snapshot.config.as_ref().unwrap_or(cfg);
    if scope.product_inputs.iter().any(|p| owns(p, path)) {
        return Ok(vec![]);
    }
    if let Some(mapping) = scope.shared_inputs.iter().find(|p| owns(&p.path, path)) {
        return Ok(mapping.packages.clone());
    }
    let ids = natural_owners(snapshot, path);
    if ids.len() == 1 {
        return Ok(ids);
    }
    if ids.len() > 1 {
        return Err(format!(
            "ambiguous package ownership: {path}; configure shared_inputs"
        ));
    }
    if snapshot.inputs.contains_key(path) {
        return Err(format!(
            "unattributed adapter input: {path}; configure shared_inputs or product_inputs"
        ));
    }
    Err(format!(
        "unattributed release input: {path}; configure ownership"
    ))
}

pub(super) fn bump_native(package: &NativePackage, impact: u8) -> Result<String, String> {
    match package.language {
        Language::Python => python::bump_version(&package.version, impact),
        Language::Rust => {
            let v: SemVer = package.version.parse()?;
            let n = match impact {
                3 => SemVer {
                    major: v.major.checked_add(1).ok_or("version overflow")?,
                    minor: 0,
                    patch: 0,
                },
                2 => SemVer {
                    major: v.major,
                    minor: v.minor.checked_add(1).ok_or("version overflow")?,
                    patch: 0,
                },
                _ => SemVer {
                    patch: v.patch.checked_add(1).ok_or("version overflow")?,
                    ..v
                },
            };
            Ok(n.to_string())
        }
    }
}

pub(super) fn add_writes(target: &mut model::Writes, source: model::Writes) -> Result<(), String> {
    for (path, bytes) in source {
        if let Some(old) = target.insert(path.clone(), bytes.clone()) {
            if old != bytes {
                return Err(format!("conflicting adapter writes: {path}"));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_target(package: &NativePackage, target: &str) -> Result<(), String> {
    match package.language {
        Language::Rust => {
            if target.parse::<SemVer>()? <= package.version.parse::<SemVer>()? {
                return Err("target version must increase".into());
            }
        }
        Language::Python => {
            let old = package
                .version
                .parse::<pep440_rs::Version>()
                .map_err(|e| e.to_string())?;
            let new = target
                .parse::<pep440_rs::Version>()
                .map_err(|e| e.to_string())?;
            if new <= old {
                return Err("target version must increase".into());
            }
        }
    }
    Ok(())
}

fn validate_version_ownership(cfg: &Config) -> Result<(), String> {
    if let Some(python) = &cfg.adapters.python {
        for (owner, source) in &python.version_sources {
            let path = source.path.to_ascii_lowercase();
            if matches!(path.as_str(), "version" | "changelog.md" | CONFIG)
                || path.starts_with(".codex/")
                || cfg
                    .packages
                    .iter()
                    .any(|p| p.manifest.to_ascii_lowercase() == path && p.manifest != *owner)
            {
                return Err(format!(
                    "{owner}: Python version source conflicts with another package or product output: {}",
                    source.path
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn native_semantics(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let doc = text.parse::<toml_edit::DocumentMut>().map_err(
        |_| "mechanical metadata proof requires TOML; declare compatible for legacy metadata",
    )?;
    fn convert(item: &toml_edit::Item, trail: &[String]) -> Result<Value, String> {
        if let Some(table) = item.as_table_like() {
            let mut values = serde_json::Map::new();
            for (k, v) in table.iter() {
                let keys = trail.iter().map(String::as_str).collect::<Vec<_>>();
                let dependency = match keys.as_slice() {
                    [section, _] => matches!(
                        *section,
                        "dependencies" | "dev-dependencies" | "build-dependencies"
                    ),
                    ["workspace", "dependencies", _] => true,
                    ["target", _, section, _] => matches!(
                        *section,
                        "dependencies" | "dev-dependencies" | "build-dependencies"
                    ),
                    ["tool", "poetry", section, _] => {
                        matches!(*section, "dependencies" | "dev-dependencies")
                    }
                    ["tool", "poetry", "group", _, "dependencies", _] => true,
                    _ => false,
                };
                let uv_source = trail.len() == 4 && trail[..3] == ["tool", "uv", "sources"];
                if k == "path" && (dependency || uv_source)
                    || k == "workspace" && trail == ["package"] && v.as_str().is_some()
                {
                    continue;
                }
                let mut next = trail.to_vec();
                next.push(k.into());
                values.insert(k.into(), convert(v, &next)?);
            }
            return Ok(Value::Object(values));
        }
        if let Some(array) = item.as_array_of_tables() {
            return array
                .iter()
                .map(|t| convert(&toml_edit::Item::Table(t.clone()), trail))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array);
        }
        fn scalar(v: &toml_edit::Value) -> Result<Value, String> {
            Ok(match v {
                toml_edit::Value::String(s) => json!(s.value()),
                toml_edit::Value::Integer(n) => json!(n.value()),
                toml_edit::Value::Boolean(b) => json!(b.value()),
                toml_edit::Value::Array(a) => {
                    Value::Array(a.iter().map(scalar).collect::<Result<Vec<_>, _>>()?)
                }
                toml_edit::Value::InlineTable(t) => {
                    let mut m = serde_json::Map::new();
                    for (k, v) in t.iter() {
                        m.insert(k.into(), scalar(v)?);
                    }
                    Value::Object(m)
                }
                _ => return Err("unsupported mechanical metadata value".into()),
            })
        }
        item.as_value().map(scalar).unwrap_or(Ok(Value::Null))
    }
    convert(doc.as_item(), &[])
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../../scripts/tests/unit/core_release_polyglot_scenarios.rs"]
mod scenario_tests;
#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../../scripts/tests/unit/core_release_polyglot.rs"]
mod tests;
