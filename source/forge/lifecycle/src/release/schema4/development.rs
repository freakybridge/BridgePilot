//! Package development closure. No Git/index/ref writes and no product version writes.
use super::*;
use crate::CommandOutcome;

pub(super) const RECORD: &str = ".codex/package-closures.json";
const JOURNAL: &str = ".runtime/bridgeforge-codex/develop-transaction.json";


/// A trusted application-layer renderer. It must return candidate bytes without
/// writing files; all returned outputs join this transaction and its rollback.
pub type Materializer =
    dyn Fn(&Path, &BTreeMap<String, Vec<u8>>) -> Result<BTreeMap<String, Vec<u8>>, String>;

fn materialize(
    root: &Path,
    writes: &mut model::Writes,
    renderer: Option<&Materializer>,
) -> Result<(), String> {
    if let Some(renderer) = renderer {
        for (path, bytes) in renderer(root, writes)? {
            safe_relative(root, &path)?;
            if path == RECORD
                || path == JOURNAL
                || matches!(path.as_str(), "VERSION" | "CHANGELOG.md" | CONFIG)
            {
                return Err(format!(
                    "development projection cannot own protected state: {path}"
                ));
            }
            if writes.get(&path).is_some_and(|before| before != &bytes) {
                return Err(format!(
                    "development projection conflicts with native output: {path}"
                ));
            }
            writes.insert(path, bytes);
        }
    }
    Ok(())
}

pub fn require_completed_transaction(root: &Path) -> Result<(), String> {
    if root.join(JOURNAL).exists() {
        Err(
            "unfinished development transaction; run develop --recover before Git synchronization"
                .into(),
        )
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckCommand {
    id: String,
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default = "default_timeout")]
    timeout_seconds: u64,
    #[serde(default)]
    artifacts: Vec<String>,
    #[serde(default)]
    inputs: Vec<String>,
    #[serde(default)]
    kind: String,
    #[serde(default, skip_serializing_if = "is_false")]
    reusable: bool,
    /// Checks of live external state or mandatory side effects cannot be staged.
    #[serde(default, skip_serializing_if = "is_false")]
    always_run: bool,
}
fn is_false(value: &bool) -> bool {
    !*value
}
fn default_timeout() -> u64 {
    300
}

/// Keep the registered command and its execution boundaries. A task may add
/// one Cargo test-name filter, but cannot replace flags, targets or runners.
fn task_arguments_allowed(registered: &CheckCommand, selected: &CheckCommand) -> bool {
    if registered.program != selected.program {
        return false;
    }
    if registered.args == selected.args {
        return true;
    }
    let cargo = Path::new(&registered.program)
        .file_stem()
        .and_then(|name| name.to_str())
        == Some("cargo");
    if !cargo || registered.args.first().map(String::as_str) != Some("test") {
        return false;
    }
    let before = registered
        .args
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(registered.args.len());
    let after = selected
        .args
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(selected.args.len());
    if after != before + 1 || registered.args[before..] != selected.args[after..] {
        return false;
    }
    let filter = &selected.args[before];
    !filter.is_empty()
        && !filter.starts_with('-')
        && selected.args[..before] == registered.args[..before]
}

fn validate_task_selection(
    id: &str,
    policy: &Policy,
    selected: &[CheckCommand],
) -> Result<(), String> {
    // User-authorized task selection does not alter the project's stability,
    // environment or delivery requirements. Reuse their complete validation.
    let task = Policy {
        checks: selected.to_vec(),
        stability: policy.stability.clone(),
        environment: policy.environment.clone(),
        release_artifacts: policy.release_artifacts.clone(),
    };
    task.validate(id)?;
    for check in selected {
        let registered = policy
            .checks
            .iter()
            .find(|entry| entry.id == check.id)
            .ok_or_else(|| format!("{id}: task check is not registered: {}", check.id))?;
        if !task_arguments_allowed(registered, check)
            || check.inputs != registered.inputs
            || check.artifacts != registered.artifacts
            || check.kind != registered.kind
            || check.always_run != registered.always_run
            || check.timeout_seconds > registered.timeout_seconds
            || (check.reusable && !registered.reusable)
        {
            return Err(format!(
                "{id}: task selection changed a registered command boundary: {}",
                check.id
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Stability {
    contracts: Vec<String>,
    checks: Vec<CheckCommand>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::release) struct Policy {
    checks: Vec<CheckCommand>,
    #[serde(default)]
    stability: Option<Stability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    environment: Option<portable_environment::Contract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    release_artifacts: Vec<String>,
}
impl Policy {
    pub(super) fn covers_durable_input(&self, path: &str) -> bool {
        // checks.inputs binds execution, not durable product content. Explicit
        // retained documentation does not certify or reuse a check result.
        self.checks
            .iter()
            .flat_map(|c| &c.artifacts)
            .chain(
                self.stability
                    .iter()
                    .flat_map(|s| &s.checks)
                    .flat_map(|c| c.inputs.iter().chain(&c.artifacts)),
            )
            .chain(self.stability.iter().flat_map(|s| &s.contracts))
            .chain(&self.release_artifacts)
            .any(|input| {
                path.eq_ignore_ascii_case(input.trim_end_matches('/'))
                    || path.to_ascii_lowercase().starts_with(&format!(
                        "{}/",
                        input.trim_end_matches('/').to_ascii_lowercase()
                    ))
            })
    }
    pub(super) fn validate(&self, id: &str) -> Result<(), String> {
        if id.trim().is_empty() || self.checks.is_empty() {
            return Err(format!(
                "{id}: development requires a nonempty mandatory check list"
            ));
        }
        let mut ids = BTreeSet::new();
        for check in self
            .checks
            .iter()
            .chain(self.stability.iter().flat_map(|s| &s.checks))
        {
            if check.id.trim().is_empty()
                || !ids.insert(&check.id)
                || check.program.trim().is_empty()
                || !(1..=14400).contains(&check.timeout_seconds)
            {
                return Err(format!(
                    "{id}: invalid/duplicate development check or timeout"
                ));
            }
            for path in check.artifacts.iter().chain(&check.inputs) {
                safe_relative(Path::new("."), path)?;
            }
        }
        if let Some(stability) = &self.stability {
            for path in &stability.contracts {
                safe_relative(Path::new("."), path)?;
            }
        }
        if let Some(environment) = &self.environment {
            environment.validate(
                self.checks
                    .iter()
                    .chain(self.stability.iter().flat_map(|s| &s.checks))
                    .map(|check| check.program.as_str()),
            )?;
            for check in self
                .checks
                .iter()
                .chain(self.stability.iter().flat_map(|s| &s.checks))
            {
                environment.validate_arguments(&check.args)?;
            }
        } else if !self.release_artifacts.is_empty() {
            return Err(
                "release_artifacts requires an explicit portable environment contract".into(),
            );
        }
        if self
            .checks
            .iter()
            .chain(self.stability.iter().flat_map(|s| &s.checks))
            .any(|check| {
                check.reusable
                    && (self.environment.is_none()
                        || check
                            .inputs
                            .iter()
                            .any(|path| check.artifacts.contains(path)))
            })
        {
            return Err(format!(
                "{id}: reusable checks require an explicit environment and disjoint inputs/outputs"
            ));
        }
        for path in &self.release_artifacts {
            safe_relative(Path::new("."), path)?;
            if !self
                .checks
                .iter()
                .chain(self.stability.iter().flat_map(|s| &s.checks))
                .any(|check| check.artifacts.contains(path))
            {
                return Err(format!(
                    "{id}: release artifact is not produced by a declared check: {path}"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub id: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub content: String,
    pub evidence: String,
    pub level: u8,
    pub compatibility: String,
    pub acceptance: Option<Value>,
    pub reason: String,
    pub stability: String,
    pub checks: Vec<Value>,
    pub artifacts: BTreeMap<String, String>,
    pub previous: String,
    pub hash: String,
    // 6.x and 7.0.x records did not identify their evidence algorithm. Omit
    // this field for those records so their append-only chain stays byte-valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_schema: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_components: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_details: Option<BTreeMap<String, String>>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ledger {
    pub entries: Vec<Entry>,
}
impl Ledger {
    pub(super) fn parse(bytes: Option<&[u8]>) -> Result<Self, String> {
        let ledger: Self = match bytes {
            None => Self::default(),
            Some(bytes) => {
                serde_json::from_value(crate::json::parse_unique_json(bytes, "package closures")?)
                    .map_err(|e| format!("invalid package closure ledger: {e}"))?
            }
        };
        let mut previous = String::new();
        let mut versions = BTreeMap::new();
        for entry in &ledger.entries {
            if entry
                .evidence_schema
                .is_some_and(|schema| !matches!(schema, 2 | 3 | 4 | 5 | 6))
            {
                return Err(format!(
                    "{}: unsupported closure evidence schema {:?}; upgrade the tool before release",
                    entry.id, entry.evidence_schema
                ));
            }
            match (entry.evidence_schema, &entry.evidence_components) {
                (Some(6), Some(parts))
                    if parts.keys().map(String::as_str).collect::<BTreeSet<_>>()
                        == BTreeSet::from(["content", "dependencies", "build"])
                        && parts.get("content") == Some(&entry.content)
                        && hash(parts)? == entry.evidence => {}
                (Some(3 | 4 | 5), Some(parts))
                    if parts
                        .keys()
                        .map(String::as_str)
                        .filter(|k| {
                            !matches!(entry.evidence_schema, Some(4 | 5)) || *k != "normalization"
                        })
                        .filter(|k| entry.evidence_schema != Some(5) || *k != "details")
                        .collect::<BTreeSet<_>>()
                        == BTreeSet::from([
                            "adapters",
                            "build",
                            "check_inputs",
                            "content",
                            "contracts",
                            "dependencies",
                            "environment",
                            "policy",
                        ])
                        && (!matches!(entry.evidence_schema, Some(4 | 5))
                            || parts.contains_key("normalization"))
                        && (entry.evidence_schema != Some(5)
                            || entry.evidence_details.as_ref().is_some_and(|d| {
                                hash(d).ok().as_ref() == parts.get("details")
                            }))
                        && parts.get("content") == Some(&entry.content)
                        && hash(parts)? == entry.evidence => {}
                (None | Some(2), None) => {}
                _ => return Err(format!("{}: invalid closure evidence components", entry.id)),
            }
            if entry.evidence_schema != Some(5) && entry.evidence_details.is_some() {
                return Err(format!(
                    "{}: unexpected portable evidence details",
                    entry.id
                ));
            }
            let mut unhashed = entry.clone();
            unhashed.hash.clear();
            if entry.previous != previous
                || entry.hash != hash(&unhashed)?
                || entry.reason.trim().is_empty()
                || entry.level > 3
                || entry.checks.is_empty()
                || entry.checks.iter().any(|c| c["status"] != "passed")
            {
                return Err(format!(
                    "{}: invalid closure chain or validation evidence",
                    entry.id
                ));
            }
            if let Some(version) = versions.get(&entry.id) {
                if version != &entry.before {
                    return Err(format!("{}: discontinuous closure versions", entry.id));
                }
            }
            if let (Some(before), Some(after)) = (&entry.before, &entry.after) {
                let before = before
                    .parse::<pep440_rs::Version>()
                    .map_err(|e| e.to_string())?;
                let after = after
                    .parse::<pep440_rs::Version>()
                    .map_err(|e| e.to_string())?;
                if after < before || (after == before && entry.level != 0) {
                    return Err(format!(
                        "{}: closure version must increase for actual changes",
                        entry.id
                    ));
                }
            }
            if entry.compatibility == "breaking"
                && entry.acceptance.as_ref().is_none_or(|a| {
                    a["source"].as_str().is_none_or(|s| s.trim().is_empty())
                        || a["reason"].as_str().is_none_or(|s| s.trim().is_empty())
                        || !a["scope"]
                            .as_array()
                            .is_some_and(|s| s.iter().any(|id| id == &entry.id))
                })
            {
                return Err(format!("{}: breaking closure acceptance missing", entry.id));
            }
            versions.insert(entry.id.clone(), entry.after.clone());
            previous = entry.hash.clone();
        }
        Ok(ledger)
    }
    pub(super) fn load(root: &Path) -> Result<Self, String> {
        Self::parse(config::read_current(root, RECORD)?.as_deref())
    }
    pub(super) fn latest(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().rev().find(|e| e.id == id)
    }
    fn append(&mut self, mut entry: Entry) -> Result<(), String> {
        entry.previous = self
            .entries
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_default();
        entry.hash = hash(&entry)?;
        self.entries.push(entry);
        Ok(())
    }
    pub(super) fn extends(&self, old: &Self) -> Result<(), String> {
        if self.entries.len() < old.entries.len()
            || self
                .entries
                .iter()
                .zip(&old.entries)
                .any(|(a, b)| a.hash != b.hash)
        {
            return Err("committed package closure history must be preserved verbatim".into());
        }
        Ok(())
    }
}
fn hash(value: &impl Serialize) -> Result<String, String> {
    Ok(digest(
        &serde_json::to_vec(value).map_err(|e| e.to_string())?,
    ))
}
fn uses_build(cfg: &Config, capture: &Capture, id: &str, path: &str) -> bool {
    if cfg.build_inputs.iter().any(|p| polyglot::owns(p, path)) {
        return true;
    }
    let Some(package) = capture.current.packages.get(id) else {
        return false;
    };
    if package.language == Language::Rust {
        return Path::new(&package.workspace)
            .with_file_name("Cargo.lock")
            .to_string_lossy()
            .replace('\\', "/")
            == path;
    }
    cfg.native.adapters.python.as_ref().is_some_and(|a| {
        a.locks.iter().any(|l| {
            l.path == path && (l.members.is_empty() || l.members.contains(&package.manifest))
        })
    })
}
fn is_build(cfg: &Config, path: &str) -> bool {
    Path::new(path).file_name().and_then(|s| s.to_str()) == Some("Cargo.lock")
        || cfg.build_inputs.iter().any(|p| polyglot::owns(p, path))
        || cfg
            .native
            .adapters
            .python
            .as_ref()
            .is_some_and(|a| a.locks.iter().any(|l| l.path == path))
}
pub(super) fn version_recorded(root: &Path, id: &str, version: &str) -> Result<bool, String> {
    Ok(Ledger::load(root)?
        .latest(id)
        .is_some_and(|e| e.after.as_deref() == Some(version)))
}

// Content determines version changes; dependency/build coverage detects work
// requiring validation without forcing an unrelated second version bump.
fn dependency_scope(capture: &Capture, id: &str) -> BTreeSet<String> {
    let mut scope = BTreeSet::from([id.to_string()]);
    loop {
        let before = scope.len();
        for edge in &capture.current.dependencies {
            if scope.contains(&edge.owner) {
                scope.insert(edge.target.clone());
            }
        }
        if before == scope.len() {
            return scope;
        }
    }
}

fn parse_task_packages(value: &str) -> Result<BTreeSet<String>, String> {
    let mut packages = BTreeSet::new();
    for id in value.split(',').map(str::trim) {
        if id.is_empty() || !packages.insert(id.to_string()) {
            return Err("task packages must be nonempty, unique registered package ids".into());
        }
    }
    Ok(packages)
}

/// Reject malformed task selection before any project discovery or forwarding.
pub fn task_packages_argument(args: &[String]) -> Result<Option<String>, String> {
    if args.iter().any(|arg| arg.starts_with("--packages=")) {
        return Err("use --packages followed by one comma-separated list".into());
    }
    let positions = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| arg.as_str() == "--packages")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if positions.is_empty() {
        return Ok(None);
    }
    if positions.len() != 1 || args.iter().any(|arg| arg == "--recover") {
        return Err(
            "--packages requires one comma-separated list and cannot be combined with --recover"
                .into(),
        );
    }
    let value = args
        .get(positions[0] + 1)
        .filter(|value| !value.starts_with("--"))
        .ok_or("--packages requires one comma-separated list")?;
    parse_task_packages(value)?;
    Ok(Some(value.clone()))
}

// A task cannot opt out of its consumers, dependencies or identity transitions.
// Upstream dependencies do not pull in unrelated sibling consumers.
pub(super) fn task_scope(
    cfg: &Config,
    capture: &Capture,
    requested: &BTreeSet<String>,
) -> Result<BTreeSet<String>, String> {
    let known = |id: &str| {
        capture.current.packages.contains_key(id) || capture.baseline.packages.contains_key(id)
    };
    if requested.is_empty() || requested.iter().any(|id| !known(id)) {
        return Err("task scope contains an empty or unknown registered package".into());
    }
    let mut scope = requested.clone();
    let mut impact = requested.clone();
    loop {
        let before = (scope.len(), impact.len());
        for transition in cfg
            .native
            .lifecycle
            .iter()
            .filter(|t| t.base == capture.boundary)
        {
            let ids = transition.from.iter().chain(&transition.to).map(|p| &p.id);
            if ids.clone().any(|id| scope.contains(id)) {
                let endpoints = ids.cloned().collect::<BTreeSet<_>>();
                scope.extend(endpoints.iter().cloned());
                impact.extend(endpoints);
            }
        }
        for edge in capture
            .current
            .dependencies
            .iter()
            .chain(&capture.baseline.dependencies)
        {
            if impact.contains(&edge.target) && known(&edge.owner) {
                scope.insert(edge.owner.clone());
                impact.insert(edge.owner.clone());
            }
            if scope.contains(&edge.owner) && known(&edge.target) {
                scope.insert(edge.target.clone());
            }
        }
        if (scope.len(), impact.len()) == before {
            return Ok(scope);
        }
    }
}

fn product_workspaces(
    root: &Path,
    cfg: &Config,
    capture: &Capture,
) -> Result<BTreeSet<String>, String> {
    match &cfg.native.adapters.rust {
        Some(adapter) => rust::product_workspaces(
            adapter,
            &cfg.native.product_packages,
            &capture.inventory,
            &mut |path| config::read_current(root, path),
        ),
        None => Ok(BTreeSet::new()),
    }
}

/// Evidence follows actual Cargo dependency closures. Product carrier versions
/// are excluded by graph reachability, not by rewriting lock version strings.
fn rust_build_evidence(
    root: &Path,
    capture: &Capture,
    scope: &BTreeSet<String>,
    products: &BTreeSet<String>,
    overlay: &model::Writes,
) -> Result<BTreeMap<String, Option<String>>, String> {
    rust_build_evidence_from(capture, scope, products, &mut |path| {
        overlay
            .get(path)
            .cloned()
            .map(Some)
            .map(Ok)
            .unwrap_or_else(|| config::read_current(root, path))
    })
}

fn rust_build_evidence_from(
    capture: &Capture,
    scope: &BTreeSet<String>,
    products: &BTreeSet<String>,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<BTreeMap<String, Option<String>>, String> {
    rust_build_evidence_mode(capture, scope, products, read, false)
}

// Only impact selection uses semantic TOML. Historical certificate hashes keep
// their original serializer, so enabling precise scope cannot rebind old proof.
fn toml_semantics(bytes: &[u8]) -> Result<Value, String> {
    fn value(v: &toml_edit::Value) -> Value {
        match v {
            toml_edit::Value::String(v) => json!(v.value()),
            toml_edit::Value::Integer(v) => json!(v.value()),
            toml_edit::Value::Float(v) => json!({"float":v.value().to_string()}),
            toml_edit::Value::Boolean(v) => json!(v.value()),
            toml_edit::Value::Datetime(v) => json!({"datetime":v.value().to_string()}),
            toml_edit::Value::Array(v) => Value::Array(v.iter().map(value).collect()),
            toml_edit::Value::InlineTable(v) => json!(
                v.iter()
                    .map(|(k, v)| (k, value(v)))
                    .collect::<BTreeMap<_, _>>()
            ),
        }
    }
    fn item(i: &toml_edit::Item) -> Value {
        match i {
            toml_edit::Item::None => Value::Null,
            toml_edit::Item::Value(v) => value(v),
            toml_edit::Item::Table(t) => table(t),
            toml_edit::Item::ArrayOfTables(a) => Value::Array(a.iter().map(table).collect()),
        }
    }
    fn table(t: &toml_edit::Table) -> Value {
        json!(
            t.iter()
                .map(|(k, v)| (k, item(v)))
                .collect::<BTreeMap<_, _>>()
        )
    }
    let document = std::str::from_utf8(bytes)
        .map_err(|e| e.to_string())?
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.to_string())?;
    Ok(table(document.as_table()))
}

fn rust_build_evidence_mode(
    capture: &Capture,
    scope: &BTreeSet<String>,
    products: &BTreeSet<String>,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    semantic: bool,
) -> Result<BTreeMap<String, Option<String>>, String> {
    let mut workspaces = BTreeMap::<String, BTreeSet<String>>::new();
    for id in scope {
        if let Some(package) = capture
            .current
            .packages
            .get(id)
            .filter(|p| p.language == Language::Rust)
        {
            workspaces
                .entry(package.workspace.clone())
                .or_default()
                .insert(package.name.clone());
        }
    }
    let mut evidence = BTreeMap::new();
    for (workspace, names) in workspaces {
        let bytes = read(&workspace)?
            .ok_or_else(|| format!("{workspace}: Rust workspace evidence is missing"))?;
        let mut document = std::str::from_utf8(&bytes)
            .map_err(|e| e.to_string())?
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| format!("{workspace}: {e}"))?;
        if products.contains(&workspace) {
            if let Some(package) = document
                .get_mut("workspace")
                .and_then(|w| w.get_mut("package"))
                .and_then(toml_edit::Item::as_table_like_mut)
            {
                package.remove("version");
            }
        }
        evidence.insert(
            workspace.clone(),
            Some(digest(document.to_string().as_bytes())),
        );
        let lock = Path::new(&workspace)
            .with_file_name("Cargo.lock")
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = read(&lock)?;
        let normalized = bytes
            .as_deref()
            .map(|bytes| crate::source_projection::lock_dependency_closure(bytes, &names))
            .transpose()?;
        evidence.insert(
            lock,
            normalized
                .as_deref()
                .map(|bytes| {
                    if semantic {
                        hash(&toml_semantics(bytes)?)
                    } else {
                        Ok(digest(bytes))
                    }
                })
                .transpose()?,
        );
    }
    Ok(evidence)
}

/// Version coverage deliberately excludes test policy and host identity.
#[derive(Serialize)]
struct Fingerprints {
    content: String,
    evidence: String,
    components: BTreeMap<String, String>,
    #[serde(skip)]
    raw: Option<Box<Fingerprints>>,
}

impl Fingerprints {
    fn content_matches(&self, entry: &Entry) -> bool {
        entry.content == self.content
            || (!matches!(entry.evidence_schema, Some(4 | 5 | 6))
                && self
                    .raw
                    .as_ref()
                    .is_some_and(|raw| raw.content == entry.content))
    }

    fn matches(&self, entry: &Entry) -> bool {
        let current = if entry.content == self.content {
            self
        } else if !matches!(entry.evidence_schema, Some(4 | 5 | 6)) {
            self.raw.as_deref().unwrap_or(self)
        } else {
            self
        };
        current.content == entry.content
            && entry.evidence_components.as_ref().is_some_and(|old| {
                current
                    .components
                    .iter()
                    .all(|(key, value)| old.get(key) == Some(value))
            })
    }

    fn mismatch(&self, entry: Option<&Entry>) -> String {
        let Some(entry) = entry else {
            return "missing version coverage record [record-missing]; inspect existing managed validation/closure before deciding whether checks are needed".into();
        };
        if !self.content_matches(entry) {
            return "package content changed [content-changed]".into();
        }
        let current = if entry.content == self.content {
            self
        } else {
            self.raw.as_deref().unwrap_or(self)
        };
        let mut missing = Vec::new();
        let mut changed = Vec::new();
        for (key, value) in &current.components {
            match entry
                .evidence_components
                .as_ref()
                .and_then(|old| old.get(key))
            {
                None => missing.push(key.as_str()),
                Some(old) if old != value => changed.push(key.as_str()),
                _ => {}
            }
        }
        format!(
            "package dependency/build coverage mismatch [coverage-mismatch]; missing components: [{}]; changed components: [{}]; inspect record compatibility separately from changed inputs",
            missing.join(", "),
            changed.join(", ")
        )
    }
}

fn fingerprints(
    root: &Path,
    cfg: &Config,
    capture: &mut Capture,
    runner: &dyn ProcessRunner,
) -> Result<BTreeMap<String, Fingerprints>, String> {
    fingerprints_scoped(root, cfg, capture, runner, None)
}

fn fingerprints_scoped(
    root: &Path,
    cfg: &Config,
    capture: &mut Capture,
    runner: &dyn ProcessRunner,
    scope: Option<&BTreeSet<String>>,
) -> Result<BTreeMap<String, Fingerprints>, String> {
    let _profile = crate::release_metrics::span("evidence.fingerprints");
    if scope.is_some_and(BTreeSet::is_empty) {
        return Ok(BTreeMap::new());
    }
    let raw = fingerprints_with_identity(root, cfg, capture, None, scope)?;
    let paths = capture.inputs.keys().filter_map(|p| {
        p.strip_prefix(root)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    });
    let rules = content_identity::Rules::capture(root, paths, runner)?;
    if capture
        .validation_normalization
        .as_ref()
        .is_some_and(|earlier| !rules.covers(earlier))
    {
        return Err(
            "Git normalization rules changed during input capture; inspect inputs again".into(),
        );
    }
    let identity = content_identity::Identity::new(root, rules, runner);
    identity.prepare(&capture.inputs)?;
    let mut normalized = fingerprints_with_identity(root, cfg, capture, Some(&identity), scope)?;
    for (id, entry) in raw {
        let current = normalized
            .get_mut(&id)
            .ok_or("normalization package inventory changed")?;
        current.raw = Some(Box::new(entry));
    }
    if let Some(previous) = &capture.normalization {
        previous.verify(root, runner)?;
    } else {
        capture.fingerprint = hash(
            &json!({"capture":capture.fingerprint,"normalization":identity.rules.planning_digest()?}),
        )?;
    }
    capture.normalization = Some(identity.rules);
    verify_release_inputs(&capture.inputs)?;
    development_inputs::verify(root, capture, &capture.inputs)?;
    Ok(normalized)
}

fn fingerprints_with_identity(
    root: &Path,
    cfg: &Config,
    capture: &mut Capture,
    identity: Option<&content_identity::Identity<'_>>,
    scope: Option<&BTreeSet<String>>,
) -> Result<BTreeMap<String, Fingerprints>, String> {
    let file_hash = |path: &str, bytes: Option<&[u8]>| match identity {
        Some(identity) => identity.hash(path, bytes),
        None => Ok(bytes.map(digest)),
    };
    let ids = capture
        .current
        .packages
        .keys()
        .chain(capture.baseline.packages.keys())
        .filter(|id| scope.is_none_or(|scope| scope.contains(*id)))
        .cloned()
        .collect::<BTreeSet<_>>();
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let content_scope = ids
        .iter()
        .flat_map(|id| dependency_scope(capture, id))
        .chain(ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mut files = BTreeMap::<String, Option<String>>::new();
    for path in &capture.inventory {
        if path == RECORD
            || cfg.record_inputs.contains(path)
            || matches!(path.as_str(), "VERSION" | "CHANGELOG.md")
        {
            continue;
        }
        if scope.is_some()
            && !content_scope
                .iter()
                .any(|id| uses_build(cfg, capture, id, path))
            && !polyglot::owner(&capture.current, path, &cfg.native)
                .unwrap_or_default()
                .iter()
                .any(|id| content_scope.contains(id))
        {
            continue;
        }
        let bytes = match capture.inputs.get(&root.join(path)) {
            Some(bytes) => bytes.clone(),
            None => config::read_current(root, path)?,
        };
        files.insert(path.clone(), file_hash(path, bytes.as_deref())?);
        capture.inputs.entry(root.join(path)).or_insert(bytes);
    }
    for (path, bytes) in &capture.current.inputs {
        files.insert(path.clone(), file_hash(path, bytes.as_deref())?);
        capture
            .inputs
            .entry(root.join(path))
            .or_insert(bytes.clone());
    }
    let (declared_build, _) = development_inputs::capture(root, cfg, capture, &files, identity)?;
    let mut contents = BTreeMap::new();
    for id in &content_scope {
        let mut owned = BTreeMap::new();
        for (path, digest) in &files {
            if digest.is_none() || is_build(cfg, path) {
                continue;
            }
            if polyglot::owner(&capture.current, path, &cfg.native)
                .unwrap_or_default()
                .contains(id)
            {
                owned.insert(path, json!({"hash":digest,"mode":capture.modes.get(path).map(String::as_str).unwrap_or("100644")}));
            }
        }
        contents.insert(
            id.clone(),
            hash(&json!({"package":capture.current.packages.get(id),"files":owned}))?,
        );
    }
    let build = files
        .iter()
        .filter(|(path, _)| {
            Path::new(path).file_name().and_then(|s| s.to_str()) == Some("Cargo.lock")
                || cfg
                    .build_inputs
                    .iter()
                    .any(|rule| polyglot::owns(rule, path))
                || cfg
                    .native
                    .adapters
                    .python
                    .as_ref()
                    .is_some_and(|adapter| adapter.locks.iter().any(|lock| &lock.path == *path))
        })
        .collect::<BTreeMap<_, _>>();
    let products = product_workspaces(root, cfg, capture)?;
    let mut result = BTreeMap::new();
    for id in ids {
        let dependencies = dependency_scope(capture, &id);
        let dependency_inputs = dependencies
            .iter()
            .filter_map(|id| contents.get(id).map(|value| (id, value)))
            .collect::<BTreeMap<_, _>>();
        let rust_build =
            rust_build_evidence_from(capture, &dependencies, &products, &mut |path| {
                let bytes = config::read_current(root, path)?;
                match identity {
                    Some(identity) => bytes.map(|bytes| identity.bytes(path, &bytes)).transpose(),
                    None => Ok(bytes),
                }
            })?;
        let scoped_build = build
            .iter()
            .filter(|(path, _)| {
                (!rust_build.contains_key(path.as_str())
                    || cfg
                        .build_inputs
                        .iter()
                        .any(|rule| polyglot::owns(rule, path)))
                    && dependencies
                        .iter()
                        .any(|id| uses_build(cfg, capture, id, path))
            })
            .collect::<BTreeMap<_, _>>();
        let components = BTreeMap::from([
            ("content".into(), contents[&id].clone()),
            ("dependencies".into(), hash(&dependency_inputs)?),
            (
                "build".into(),
                hash(
                    &json!({"build":scoped_build,"rust_build":rust_build,"declared":declared_build}),
                )?,
            ),
        ]);
        result.insert(
            id.clone(),
            Fingerprints {
                content: contents[&id].clone(),
                evidence: hash(&components)?,
                components,
                raw: None,
            },
        );
    }
    Ok(result)
}

// Legacy aggregate records remain immutable historical data. Without separable
// dependency/build components, only an unchanged published baseline can cover them.
fn published_unchanged(
    root: &Path,
    cfg: &Config,
    capture: &Capture,
    ledger: &Ledger,
    hashes: &BTreeMap<String, Fingerprints>,
    runner: &dyn ProcessRunner,
) -> Result<BTreeSet<String>, String> {
    let published =
        Ledger::parse(revision_payload(root, &capture.boundary, RECORD, runner)?.as_deref())?;
    ledger.extends(&published)?;
    let mut covered = BTreeSet::new();
    for (id, current) in hashes {
        let (Some(entry), Some(old), Some(package), Some(baseline)) = (
            ledger.latest(id),
            published.latest(id),
            capture.current.packages.get(id),
            capture.baseline.packages.get(id),
        ) else {
            continue;
        };
        if current.matches(entry)
            || entry.evidence_components.is_some()
            || !matches!(entry.evidence_schema, None | Some(2))
            || entry.hash != old.hash
            || !current.content_matches(entry)
            || serde_json::to_value(package).ok() != serde_json::to_value(baseline).ok()
            || !cfg.build_inputs.is_empty()
            || capture
                .current
                .inputs
                .keys()
                .any(|path| !capture.inventory.contains(path))
        {
            continue;
        }
        let dependencies = dependency_scope(capture, id);
        let products = product_workspaces(root, cfg, capture)?;
        let rules = capture
            .normalization
            .clone()
            .ok_or("version normalization rules missing")?;
        let identity = content_identity::Identity::new(root, rules, runner);
        let current_build =
            rust_build_evidence_from(capture, &dependencies, &products, &mut |path| {
                config::read_current(root, path)?
                    .map(|bytes| identity.bytes(path, &bytes))
                    .transpose()
            })?;
        let published_build =
            rust_build_evidence_from(capture, &dependencies, &products, &mut |path| {
                revision_payload(root, &capture.boundary, path, runner)?
                    .map(|bytes| identity.bytes(path, &bytes))
                    .transpose()
            })?;
        if current_build != published_build {
            continue;
        }
        let changed = capture
            .changes
            .iter()
            .filter(|change| change.role != "record")
            // These exact native paths already have equal published/current
            // build identities above. In particular, an unrelated lock member
            // must not override the proven Rust dependency closure comparison.
            .filter(|change| change.mode_changed || !current_build.contains_key(&change.path))
            .any(|change| {
                (change.mode_changed && current_build.contains_key(&change.path))
                    || change
                        .packages
                        .iter()
                        .any(|owner| dependencies.contains(owner))
                    || dependencies
                        .iter()
                        .any(|owner| uses_build(cfg, capture, owner, &change.path))
            });
        if !changed {
            covered.insert(id.clone());
        }
    }
    Ok(covered)
}

fn pending(
    ledger: &Ledger,
    hashes: &BTreeMap<String, Fingerprints>,
    published: &BTreeSet<String>,
    validation_scope: &BTreeSet<String>,
) -> BTreeSet<String> {
    hashes
        .iter()
        .filter(|(id, current)| match ledger.latest(id) {
            Some(entry) => !current.matches(entry) && !published.contains(*id),
            None => validation_scope.contains(*id),
        })
        .map(|(id, _)| id.clone())
        .collect()
}

fn package_roles(
    hashes: &BTreeMap<String, Fingerprints>,
    affected: &BTreeSet<String>,
    requested: Option<&BTreeSet<String>>,
) -> Value {
    json!(
        hashes
            .keys()
            .map(|id| {
                let role = if requested.is_some_and(|ids| ids.contains(id)) {
                    "requested"
                } else if affected.contains(id) {
                    "affected-dependency-or-consumer"
                } else {
                    "context-only"
                };
                (id.clone(), role)
            })
            .collect::<BTreeMap<_, _>>()
    )
}

fn shared_configuration_affected(
    cfg: &Config,
    capture: &Capture,
    old: &Value,
    new: &Value,
) -> Result<BTreeSet<String>, String> {
    let mappings = |value: &Value| -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        let entries: Vec<polyglot::SharedInput> = match value.get("shared_inputs") {
            Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
            None => Vec::new(),
        };
        Ok(entries
            .into_iter()
            .map(|entry| (entry.path, entry.packages.into_iter().collect()))
            .collect())
    };
    let old = mappings(old)?;
    let new = mappings(new)?;
    let paths = old.keys().chain(new.keys()).collect::<BTreeSet<_>>();
    let mut affected = BTreeSet::new();
    for path in paths
        .into_iter()
        .filter(|path| old.get(*path) != new.get(*path))
    {
        affected.extend(old.get(path).into_iter().flatten().cloned());
        affected.extend(new.get(path).into_iter().flatten().cloned());
        // Adding or removing an override can also take inputs away from, or
        // return them to, a natural owner absent from both explicit owner lists.
        // Use each revision's roles and the existing adapter ownership rule.
        for snapshot in [&capture.baseline, &capture.current] {
            let roles = snapshot.config.as_ref().unwrap_or(&cfg.native);
            let candidates = std::iter::once(path.as_str())
                .chain(capture.inventory.iter().map(String::as_str))
                .chain(snapshot.inputs.keys().map(String::as_str))
                .chain(snapshot.packages.values().flat_map(|package| {
                    std::iter::once(package.manifest.as_str())
                        .chain(package.source_roots.iter().map(String::as_str))
                }));
            for candidate in candidates.filter(|candidate| polyglot::owns(path, candidate)) {
                if !roles
                    .product_inputs
                    .iter()
                    .any(|p| polyglot::owns(p, candidate))
                    && !roles
                        .shared_inputs
                        .iter()
                        .any(|p| polyglot::owns(&p.path, candidate))
                {
                    affected.extend(polyglot::natural_owners(snapshot, candidate));
                }
            }
        }
    }
    Ok(affected)
}

fn release_scope(
    root: &Path,
    cfg: &Config,
    capture: &Capture,
    ledger: &Ledger,
    published: &Ledger,
    runner: &dyn ProcessRunner,
) -> Result<BTreeSet<String>, String> {
    let mut scope = plan::affected(cfg, capture);
    // These files are explicitly delivered by a release, including a product-
    // only release with no new library source diff. Ignored file loss must not
    // disappear merely because Git reports no package change.
    scope.extend(
        cfg.development
            .iter()
            .filter(|(id, policy)| {
                !policy.release_artifacts.is_empty() && capture.current.packages.contains_key(*id)
            })
            .map(|(id, _)| id.clone()),
    );
    scope.extend(
        ledger.entries[published.entries.len()..]
            .iter()
            .map(|e| e.id.clone()),
    );
    let products = product_workspaces(root, cfg, capture)?;
    let old_config = if capture.changes.iter().any(|c| c.path == CONFIG) {
        if capture.boundary == cfg.baseline.revision {
            Some(cfg.baseline.initial_configuration.clone())
        } else {
            revision_payload(root, &capture.boundary, CONFIG, runner)?
                .map(|bytes| crate::json::parse_unique_json(&bytes, "published configuration"))
                .transpose()?
        }
    } else {
        None
    };
    let new_config = if old_config.is_some() {
        Some(crate::json::parse_unique_json(
            &config::read_current(root, CONFIG)?.ok_or("configuration missing")?,
            "configuration",
        )?)
    } else {
        None
    };
    if let (Some(old), Some(new)) = (&old_config, &new_config) {
        scope.extend(shared_configuration_affected(cfg, capture, old, new)?);
    }
    let dependencies = capture
        .current
        .packages
        .keys()
        .map(|id| (id.clone(), dependency_scope(capture, id)))
        .collect::<BTreeMap<_, _>>();
    for change in &capture.changes {
        if change.role == "record" {
            continue;
        }
        for id in capture.current.packages.keys() {
            if scope.contains(id) {
                continue;
            }
            let configuration_changed = if change.path == CONFIG {
                match (&old_config, &new_config) {
                    (Some(old), Some(new)) => {
                        old["development"][id]["release_artifacts"]
                            != new["development"][id]["release_artifacts"]
                            || [
                                "adapters",
                                "build_inputs",
                                "record_inputs",
                                "product_manifests",
                                "product_packages",
                            ]
                            .iter()
                            .any(|key| old[key] != new[key])
                            || serde_json::to_value(capture.baseline.packages.get(id)).ok()
                                != serde_json::to_value(capture.current.packages.get(id)).ok()
                    }
                    _ => true,
                }
            } else {
                false
            };
            let mut build_changed = dependencies[id]
                .iter()
                .any(|dep| uses_build(cfg, capture, dep, &change.path));
            if build_changed
                && Path::new(&change.path)
                    .file_name()
                    .is_some_and(|n| n == "Cargo.lock")
                && !cfg
                    .build_inputs
                    .iter()
                    .any(|p| polyglot::owns(p, &change.path))
                && capture.current.packages[id].language == Language::Rust
            {
                let before = revision_payload(root, &capture.boundary, &change.path, runner)?;
                let now = rust_build_evidence_mode(
                    capture,
                    &dependencies[id],
                    &products,
                    &mut |path| config::read_current(root, path),
                    true,
                )?;
                let old = rust_build_evidence_mode(
                    capture,
                    &dependencies[id],
                    &products,
                    &mut |path| {
                        if path == change.path {
                            Ok(before.clone())
                        } else {
                            config::read_current(root, path)
                        }
                    },
                    true,
                )?;
                build_changed = old != now;
            }
            let cargo_config =
                matches!(change.path.as_str(), ".cargo/config" | ".cargo/config.toml")
                    && capture.current.packages[id].language == Language::Rust;
            if configuration_changed || build_changed || cargo_config {
                scope.insert(id.clone());
            }
        }
    }
    loop {
        let before = scope.len();
        for edge in capture
            .current
            .dependencies
            .iter()
            .chain(&capture.baseline.dependencies)
        {
            if scope.contains(&edge.target) {
                scope.insert(edge.owner.clone());
            }
        }
        if before == scope.len() {
            return Ok(scope);
        }
    }
}

pub(super) fn release_gate(
    root: &Path,
    cfg: &Config,
    capture: &mut Capture,
    runner: &dyn ProcessRunner,
) -> Result<Ledger, String> {
    if root.join(JOURNAL).exists() {
        return Err(
            "unfinished development transaction; run develop --recover before release".into(),
        );
    }
    let ledger = Ledger::load(root)?;
    let published_ledger =
        Ledger::parse(revision_payload(root, &capture.boundary, RECORD, runner)?.as_deref())?;
    ledger.extends(&published_ledger)?;
    if let Some(history) = &capture.release_history {
        for entry in ledger.entries.iter().skip(published_ledger.entries.len()) {
            if entry.before == entry.after {
                continue;
            }
            if let (Some(floor), Some(version)) =
                (history.package_high_water.get(&entry.id), &entry.after)
            {
                let package = capture
                    .current
                    .packages
                    .get(&entry.id)
                    .or_else(|| capture.baseline.packages.get(&entry.id))
                    .unwrap_or(floor);
                withdrawal::version_base(package, Some(floor))?;
                if !withdrawal::greater(package, version, &floor.version)? {
                    return Err(format!(
                        "{}: new closure must exceed withdrawn published version {}",
                        entry.id, floor.version
                    ));
                }
            }
        }
    }
    if capture.changes.iter().all(|c| c.role == "record") {
        return Ok(ledger);
    }
    let scope = release_scope(root, cfg, capture, &ledger, &published_ledger, runner)?;
    // Explicit environments guard this release transaction, without making
    // another host's test environment a durable version qualification.
    let mut environments = BTreeMap::new();
    for id in &scope {
        if let Some(environment) = cfg
            .development
            .get(id)
            .and_then(|policy| policy.environment.as_ref())
        {
            let facts = environment.snapshot(root, capture)?;
            environments.insert(id.clone(), facts.clone());
            capture
                .portable_guards
                .push(portable_environment::Guard::new(
                    environment,
                    facts,
                    capture,
                ));
        }
    }
    if !environments.is_empty() {
        capture.fingerprint =
            hash(&json!({"capture":capture.fingerprint,"release_environments":environments}))?;
        capture.validation_fingerprint = hash(
            &json!({"validation":capture.validation_fingerprint,"release_environments":environments}),
        )?;
    }
    if scope.is_empty() {
        // Product-only releases still write files. Reuse the rules captured for
        // their validation identity without scanning unrelated package contents.
        let initial = capture
            .validation_normalization
            .clone()
            .ok_or("release validation rules missing")?;
        let (declared_build, _) =
            development_inputs::capture(root, cfg, capture, &BTreeMap::new(), None)?;
        let rules = if declared_build.is_empty() {
            initial
        } else {
            let paths = capture.inputs.keys().filter_map(|p| {
                p.strip_prefix(root)
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
            });
            let rules = content_identity::Rules::capture(root, paths, runner)?;
            if !rules.covers(&initial) {
                return Err("Git normalization rules changed during input capture".into());
            }
            // Product-level check receipts must also expire if a declared global
            // input changed before a fresh preview, including ignored inputs.
            capture.validation_fingerprint = hash(
                &json!({"validation":capture.validation_fingerprint,"declared_build":declared_build}),
            )?;
            rules
        };
        capture.fingerprint = hash(
            &json!({"capture":capture.fingerprint,"normalization":rules.planning_digest()?,"declared_build":declared_build}),
        )?;
        capture.normalization = Some(rules);
        verify_release_inputs(&capture.inputs)?;
        development_inputs::verify(root, capture, &capture.inputs)?;
        return Ok(ledger);
    }
    let hashes = fingerprints_scoped(root, cfg, capture, runner, Some(&scope))?;
    let published = published_unchanged(root, cfg, capture, &ledger, &hashes, runner)?;
    capture.release_packages = scope.clone();
    let pending = hashes
        .iter()
        .filter(|(id, current)| match ledger.latest(id) {
            Some(entry) => !current.matches(entry) && !published.contains(*id),
            None => scope.contains(*id),
        })
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    if !pending.is_empty() {
        let reasons = pending
            .iter()
            .map(|id| {
                let reason = hashes[id].mismatch(ledger.latest(id));
                format!("{id}: {reason}")
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "package development closure required: {reasons}; release stopped without running checks; reconcile existing managed evidence first, report only unresolved affected packages (release does not run develop)"
        ));
    }
    for id in scope {
        if let Some(entry) = ledger.latest(&id) {
            for path in cfg
                .development
                .get(&id)
                .iter()
                .flat_map(|policy| &policy.release_artifacts)
            {
                let expected = entry.artifacts.get(path).ok_or_else(|| {
                    format!("{id}: declared release artifact has no verified coverage: {path} [artifact-unrecorded]; reconcile this deliverable's managed evidence; release does not run develop")
                })?;
                let bytes = config::read_current(root, path)?.ok_or_else(|| {
                    format!("{id}: development artifact missing: {path} [artifact-missing]; restore the exact recorded deliverable or separately validate its replacement; release does not run develop")
                })?;
                if digest(&bytes) != *expected {
                    return Err(format!(
                        "{id}: development artifact changed: {path} [artifact-changed]; this deliverable needs matching evidence; release does not run develop"
                    ));
                }
                capture.inputs.insert(root.join(path), Some(bytes));
            }
        }
    }
    Ok(ledger)
}

struct Prepared {
    cfg: Config,
    capture: Capture,
    ledger: Ledger,
    analysis: analysis::Analysis,
    affected: BTreeSet<String>,
    versions: BTreeMap<String, String>,
    fallback: BTreeMap<String, String>,
    stability: BTreeMap<String, String>,
    levels: BTreeMap<String, u8>,
    direct_changes: BTreeSet<String>,
    writes: model::Writes,
    receipt: Value,
    task_scope: Option<BTreeSet<String>>,
    check_keys: BTreeMap<String, String>,
    analysis_path: Option<PathBuf>,
}
fn unstable(package: &NativePackage) -> bool {
    unstable_version(package, &package.version)
}
fn unstable_version(package: &NativePackage, version: &str) -> bool {
    match package.language {
        Language::Rust => version.parse::<SemVer>().is_ok_and(|v| v.major == 0),
        Language::Python => version
            .parse::<pep440_rs::Version>()
            .is_ok_and(|v| v.release().first() == Some(&0)),
    }
}

fn requirement_owners(
    root: &Path,
    cfg: &Config,
    capture: &Capture,
    writes: &model::Writes,
) -> Result<BTreeSet<String>, String> {
    let mut owners = BTreeSet::new();
    if let Some(rust) = &cfg.native.adapters.rust {
        owners.extend(rust::requirement_changes(
            rust,
            &capture.inventory,
            &mut |p| config::read_current(root, p),
            writes,
        )?);
    }
    for package in capture
        .current
        .packages
        .values()
        .filter(|p| p.language == Language::Python)
    {
        if let Some(after) = writes.get(&package.manifest) {
            let before = config::read_current(root, &package.manifest)?
                .ok_or("Python manifest disappeared")?;
            let before = super::super::python_metadata::parse(&before, &package.manifest)?;
            let after = super::super::python_metadata::parse(after, &package.manifest)?;
            if before
                .dependencies
                .iter()
                .map(|d| &d.requirement)
                .collect::<Vec<_>>()
                != after
                    .dependencies
                    .iter()
                    .map(|d| &d.requirement)
                    .collect::<Vec<_>>()
            {
                owners.insert(package.manifest.clone());
            }
        }
    }
    // Product followers can need a dependency requirement rewrite during a
    // library closure; their own version remains owned by product release.
    owners.retain(|manifest| {
        !cfg.native
            .product_packages
            .iter()
            .any(|p| &p.manifest == manifest)
    });
    Ok(owners)
}
fn stability_gap(
    root: &Path,
    cfg: &Config,
    ledger: &Ledger,
    capture: &Capture,
    id: &str,
    analysis: &analysis::ObjectAnalysis,
) -> Result<Option<String>, String> {
    if analysis.compatibility != "compatible" {
        return Ok(Some("current change is incompatible".into()));
    }
    if ledger.latest(id).is_none() && !capture.baseline.packages.contains_key(id) {
        return Ok(Some(
            "no previous effective development or published baseline for compatibility comparison"
                .into(),
        ));
    }
    let Some(policy) = cfg.development.get(id).and_then(|p| p.stability.as_ref()) else {
        return Ok(Some("stability contract is not configured".into()));
    };
    if policy.contracts.is_empty()
        || policy.checks.is_empty()
        || !policy.checks.iter().any(|c| c.kind == "core")
        || !policy.checks.iter().any(|c| c.kind == "integration")
    {
        return Ok(Some(
            "stability requires contract baselines plus core and real-caller integration checks"
                .into(),
        ));
    }
    for path in &policy.contracts {
        if config::read_current(root, path)?.is_none_or(|b| b.is_empty()) {
            return Ok(Some(format!("contract baseline missing or empty: {path}")));
        }
    }
    Ok(None)
}

fn prepare(
    root: &Path,
    request: &Request,
    runner: &dyn ProcessRunner,
    renderer: Option<&Materializer>,
    requested: Option<&BTreeSet<String>>,
) -> Result<Result<Prepared, Value>, String> {
    require_ignored_runtime(root, runner)?;
    if root.join(JOURNAL).exists() {
        return Err("unfinished development transaction; run develop --recover".into());
    }
    let cfg = load(root)?.ok_or("develop requires schema 4; migrate configuration first")?;
    let mut capture = capture(root, &cfg, runner)?;
    let ledger = Ledger::load(root)?;
    let products = product_workspaces(root, &cfg, &capture)?;
    let task_scope = requested
        .map(|ids| task_scope(&cfg, &capture, ids))
        .transpose()?;
    let hashes = if task_scope.is_some() {
        fingerprints_scoped(root, &cfg, &mut capture, runner, task_scope.as_ref())?
    } else {
        fingerprints(root, &cfg, &mut capture, runner)?
    };
    // This is a short-lived transaction guard, not part of durable version
    // coverage. Retain the exact inputs/environment of the checks we will run.
    let mut task_environments = BTreeMap::new();
    for (id, policy) in &cfg.development {
        if task_scope.as_ref().is_some_and(|scope| !scope.contains(id)) {
            continue;
        }
        for path in policy
            .checks
            .iter()
            .chain(policy.stability.iter().flat_map(|p| &p.checks))
            .flat_map(|check| &check.inputs)
            .chain(policy.stability.iter().flat_map(|p| &p.contracts))
        {
            capture
                .inputs
                .entry(root.join(path))
                .or_insert(config::read_current(root, path)?);
        }
        if let Some(environment) = &policy.environment {
            let facts = environment.snapshot(root, &capture)?;
            task_environments.insert(id.clone(), facts.clone());
            capture
                .portable_guards
                .push(portable_environment::Guard::new(
                    environment,
                    facts,
                    &capture,
                ));
        }
    }
    // Declared ignored inputs are discovered above and must bind the preview too.
    capture.fingerprint = hash(
        &json!({"capture":capture.fingerprint,"packages":hashes,"task_environments":task_environments,
        "inputs":capture.inputs.iter().map(|(p,b)|(p.to_string_lossy().into_owned(),b.as_deref().map(digest))).collect::<BTreeMap<_,_>>()}),
    )?;
    if let Some(scope) = &task_scope {
        capture.fingerprint = hash(
            &json!({"capture":capture.fingerprint,"task_packages":requested,"effective_task_packages":scope}),
        )?;
    }
    let published = published_unchanged(root, &cfg, &capture, &ledger, &hashes, runner)?;
    let published_ledger =
        Ledger::parse(revision_payload(root, &capture.boundary, RECORD, runner)?.as_deref())?;
    ledger.extends(&published_ledger)?;
    let validation_scope = release_scope(root, &cfg, &capture, &ledger, &published_ledger, runner)?;
    let mut affected = pending(&ledger, &hashes, &published, &validation_scope);
    // Lost or modified generated artifacts require revalidation, never a second bump.
    for (id, _) in &hashes {
        if let Some(entry) = ledger.latest(id) {
            if cfg
                .development
                .get(id)
                .iter()
                .flat_map(|policy| &policy.release_artifacts)
                .any(|p| {
                    let Some(h) = entry.artifacts.get(p) else {
                        return true;
                    };
                    config::read_current(root, p)
                        .ok()
                        .flatten()
                        .as_deref()
                        .map(digest)
                        .as_ref()
                        != Some(h)
                })
            {
                affected.insert(id.clone());
            }
        }
    }
    let mut receipt = json!({"schema":1,"status":"development-analysis-required","fingerprint":capture.fingerprint,
        "affected_packages":affected,"packages":capture.current.packages,"writes":[],"product_version":capture.product_version});
    receipt["capabilities"] = json!([
        "package-development-v1",
        "package-stability-v1",
        "task-selected-development-checks-v1",
        "version-coverage-v1",
        "task-scoped-development-v1",
        "staged-development-validation-v1",
        "direct-development-apply-v1"
    ]);
    receipt["reused_published_packages"] = json!(published);
    receipt["readiness_scope"] = json!(if task_scope.is_some() {
        "task"
    } else {
        "repository"
    });
    receipt["task_packages"] = json!(requested);
    receipt["effective_task_packages"] = json!(task_scope);
    receipt["outside_task_evidence_checked"] = json!(task_scope.is_none());
    receipt["pending_reasons"] = json!(affected.iter().map(|id| {
        let reason = if ledger.latest(id).is_some_and(|entry| hashes[id].matches(entry))
            || published.contains(id) {
            "development artifact missing or changed; restore or revalidate the declared artifact".into()
        } else {
            hashes[id].mismatch(ledger.latest(id))
        };
        (id.clone(), reason)
    }).collect::<BTreeMap<_, _>>());
    receipt["package_roles"] = package_roles(&hashes, &affected, requested);
    if affected.is_empty() {
        receipt["status"] = json!(if task_scope.is_some() {
            "task-development-current"
        } else {
            "nothing-to-close"
        });
        return Ok(Err(receipt));
    }
    receipt["python_environment_required"] = json!(
        capture.environment_guard.is_none()
            && affected.iter().any(|id| capture
                .current
                .packages
                .get(id)
                .or_else(|| capture.baseline.packages.get(id))
                .is_some_and(|p| p.language == Language::Python))
    );
    let Some(analysis) = analysis::read(root, request, &mut capture)? else {
        return Ok(Err(receipt));
    };
    if !analysis.checks.is_empty() {
        return Err(
            "develop does not accept release evidence claims; select task validation commands"
                .into(),
        );
    }
    if receipt["python_environment_required"] == true {
        return Err("Python development checks require explicit python_environment interpreter/runtime/dependency roots".into());
    }
    if analysis.product.is_some() || analysis.targets.contains_key("product") {
        return Err("develop cannot analyze or upgrade product VERSION".into());
    }
    // An older raw-byte certificate may have been made from a CRLF checkout.
    // Without an exact match or a real source diff, do not turn the missing
    // representation proof into a fabricated package version change.
    for id in &affected {
        if let Some(entry) = ledger.latest(id) {
            let same_published_package = capture
                .current
                .packages
                .get(id)
                .zip(capture.baseline.packages.get(id))
                .is_some_and(|(now, old)| {
                    now.manifest == old.manifest
                        && now.name == old.name
                        && now.source_roots == old.source_roots
                        && now.version == old.version
                        && entry.after.as_deref() == Some(now.version.as_str())
                });
            if same_published_package
                && !matches!(entry.evidence_schema, Some(4 | 5 | 6))
                && !hashes[id].content_matches(entry)
                && !capture
                    .git_content_changes
                    .iter()
                    .chain(&capture.git_mode_changes)
                    .any(|path| {
                        !is_build(&cfg, path)
                            && path != RECORD
                            && !cfg.record_inputs.contains(path)
                            && polyglot::owner(&capture.current, path, &cfg.native)
                                .unwrap_or_default()
                                .contains(id)
                    })
            {
                return Err(format!(
                    "{id}: legacy content representation cannot be proven; no Git source change justifies a version bump; resolve the historical evidence before managed develop"
                ));
            }
        }
    }
    let lifecycle_affected = plan::lifecycle_affected(&cfg, &capture);
    let unchanged_content = |id: &String| match ledger.latest(id) {
        Some(entry) => hashes[id].content_matches(entry),
        None => {
            !lifecycle_affected.contains(id)
                && capture.baseline.packages.contains_key(id)
                && serde_json::to_value(capture.baseline.packages.get(id)).ok()
                    == serde_json::to_value(capture.current.packages.get(id)).ok()
                && !capture.changes.iter().any(|c| {
                    c.role != "record" && c.packages.contains(id) && !is_build(&cfg, &c.path)
                })
        }
    };
    let mut direct_changes = affected
        .iter()
        .filter(|id| !unchanged_content(id))
        .cloned()
        .collect::<BTreeSet<_>>();
    plan::interface_impacts(&capture, &analysis, &mut direct_changes, true)?;
    affected.extend(direct_changes.iter().cloned());
    let mut force_bump = analysis
        .interface_impacts
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut mechanical = BTreeSet::new();
    let mut versions = BTreeMap::new();
    let mut fallback = BTreeMap::new();
    let mut stability = BTreeMap::new();
    let mut levels = BTreeMap::new();
    let mut writes = BTreeMap::new();
    for round in 0..=capture.current.packages.len() + 1 {
        if task_scope
            .as_ref()
            .is_some_and(|scope| !affected.is_subset(scope))
        {
            return Err("development impact expanded outside the captured task scope; inspect the required packages again".into());
        }
        for id in &affected {
            let a = analysis.packages.get(id).ok_or_else(|| {
                format!(
                    "{id}: development analysis missing (including effective dependency changes)"
                )
            })?;
            let policy = cfg
                .development
                .get(id)
                .ok_or_else(|| format!("{id}: mandatory development checks must be configured"))?;
            policy.validate(id)?;
            let package = capture
                .current
                .packages
                .get(id)
                .or_else(|| capture.baseline.packages.get(id))
                .ok_or("unknown development object")?;
            let impact = a.version_level(id, &capture.fingerprint, unstable(package))?;
            if cfg.native.lifecycle.iter().any(|t| {
                t.base == capture.boundary
                    && t.compatibility == "breaking"
                    && t.from.iter().chain(&t.to).any(|p| &p.id == id)
            }) && a.compatibility != "breaking"
            {
                return Err(format!(
                    "{id}: breaking lifecycle transition requires breaking analysis and explicit user acceptance"
                ));
            }
            levels.insert(id.clone(), impact);
            if !capture.current.packages.contains_key(id) {
                continue;
            }
            let revalidation =
                !force_bump.contains(id) && (mechanical.contains(id) || unchanged_content(id));
            if revalidation {
                levels.insert(id.clone(), 0);
            }
            let first = !capture.baseline.packages.contains_key(id) && ledger.latest(id).is_none();
            if unstable(package)
                && analysis
                    .targets
                    .get(id)
                    .is_some_and(|v| !unstable_version(package, v))
            {
                return Err(format!(
                    "{id}: stable graduation is automatic; explicit stable targets are not accepted"
                ));
            }
            let target = if revalidation {
                package.version.clone()
            } else {
                let published = capture
                    .release_history
                    .as_ref()
                    .and_then(|h| h.package_high_water.get(id));
                let base = withdrawal::version_base(package, published)?;
                plan::choose(
                    &base,
                    impact,
                    analysis.targets.get(id),
                    first && published.is_none(),
                )?
            };
            if revalidation && analysis.targets.contains_key(id) {
                return Err(format!("{id}: unchanged content cannot be upgraded"));
            }
            fallback.insert(package.manifest.clone(), target.clone());
            let gap = stability_gap(root, &cfg, &ledger, &capture, id, a)?;
            let promote = unstable(package) && !revalidation && gap.is_none();
            if unstable(package) && !promote && !unstable_version(package, &target) {
                return Err(format!(
                    "{id}: entry to 1.0.0 is automatic and requires the stability contract"
                ));
            }
            let final_target = if promote { "1.0.0".into() } else { target };
            if !revalidation {
                if let Some(published) = capture
                    .release_history
                    .as_ref()
                    .and_then(|h| h.package_high_water.get(id))
                {
                    if !withdrawal::greater(package, &final_target, &published.version)? {
                        return Err(format!(
                            "{id}: development target {final_target} must exceed withdrawn published version {}",
                            published.version
                        ));
                    }
                }
            }
            versions.insert(package.manifest.clone(), final_target);
            stability.insert(
                id.clone(),
                if promote {
                    "eligible; final-version stability checks required".into()
                } else {
                    gap.unwrap_or_else(|| "already stable or no version change".into())
                },
            );
        }
        writes = plan::render(root, &cfg, &capture, &versions)?;
        let owners = requirement_owners(root, &cfg, &capture, &writes)?;
        let before = (affected.len(), force_bump.len());
        for manifest in owners {
            let id = capture
                .current
                .packages
                .iter()
                .find(|(_, p)| p.manifest == manifest)
                .map(|(id, _)| id)
                .ok_or("dependency owner is unregistered")?;
            force_bump.insert(id.clone());
            affected.insert(id.clone());
        }
        // Native adapters may materialize an unchanged inherited version. Verify
        // and record that final manifest without inventing a business bump.
        for (id, package) in &capture.current.packages {
            if writes.contains_key(&package.manifest) && !affected.contains(id) {
                mechanical.insert(id.clone());
                affected.insert(id.clone());
            }
        }
        // Revalidate only consumers whose actual lock dependency closure changed.
        if writes.keys().any(|path| {
            Path::new(path).file_name().and_then(|s| s.to_str()) == Some("Cargo.lock")
                || cfg
                    .native
                    .adapters
                    .python
                    .as_ref()
                    .is_some_and(|a| a.locks.iter().any(|l| &l.path == path))
        }) {
            for id in ledger
                .entries
                .iter()
                .map(|e| &e.id)
                .collect::<BTreeSet<_>>()
            {
                let scope = dependency_scope(&capture, id);
                let before =
                    rust_build_evidence(root, &capture, &scope, &products, &BTreeMap::new())?;
                let after = rust_build_evidence(root, &capture, &scope, &products, &writes)?;
                let explicit_or_python = writes.keys().any(|path| {
                    scope
                        .iter()
                        .any(|owner| uses_build(&cfg, &capture, owner, path))
                        && (!before.contains_key(path)
                            || cfg.build_inputs.iter().any(|p| polyglot::owns(p, path)))
                });
                if before != after || explicit_or_python {
                    affected.insert(id.clone());
                }
            }
        }
        if (affected.len(), force_bump.len()) == before {
            break;
        }
        if round == capture.current.packages.len() + 1 {
            return Err("development dependency closure did not converge".into());
        }
    }
    if analysis
        .packages
        .keys()
        .chain(analysis.targets.keys())
        .chain(analysis.validation.keys())
        .any(|id| !affected.contains(id))
    {
        return Err("development analysis includes unchanged or unknown packages".into());
    }
    if task_scope
        .as_ref()
        .is_some_and(|scope| !affected.is_subset(scope))
    {
        return Err("development impact expanded outside the captured task scope; inspect the required packages again".into());
    }
    for id in &affected {
        let selected = analysis.validation.get(id).ok_or_else(|| {
            format!("{id}: final-candidate task validation selection is required")
        })?;
        validate_task_selection(id, &cfg.development[id], selected)?;
    }
    validate_cache_artifacts(root, &cfg, &affected, &analysis.validation, runner, false)?;
    materialize(root, &mut writes, renderer)?;
    for path in writes.keys() {
        capture
            .inputs
            .entry(root.join(path))
            .or_insert(config::read_current(root, path)?);
    }
    verify_release_inputs(&capture.inputs)?;
    receipt["status"] = json!("development-planned");
    receipt["validation_mode"] = json!("task-selected-final-candidate");
    receipt["validation_checks"] = json!(
        analysis
            .validation
            .iter()
            .map(|(id, checks)| (id, checks.iter().map(|check| &check.id).collect::<Vec<_>>()))
            .collect::<BTreeMap<_, _>>()
    );
    development_inputs::verify(root, &capture, &capture.inputs)?;
    receipt["affected_packages"] = json!(affected);
    receipt["package_roles"] = package_roles(&hashes, &affected, requested);
    receipt["versions"] = json!(versions);
    receipt["stability"] = json!(stability);
    receipt["writes"] = json!(
        writes
            .keys()
            .chain(std::iter::once(&RECORD.to_string()))
            .collect::<Vec<_>>()
    );
    Ok(Ok(Prepared {
        cfg,
        capture,
        ledger,
        analysis,
        affected,
        versions,
        fallback,
        stability,
        levels,
        direct_changes,
        writes,
        receipt,
        task_scope,
        check_keys: hashes
            .iter()
            .map(|(id, value)| (id.clone(), value.evidence.clone()))
            .collect(),
        analysis_path: request.analysis_file.as_ref().map(|path| {
            if path.is_absolute() {
                path.clone()
            } else {
                root.join(path)
            }
        }),
    }))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    root: String,
    head: String,
    before: model::Inputs,
    after: model::Writes,
}

fn validate_cache_artifacts(
    root: &Path,
    cfg: &Config,
    affected: &BTreeSet<String>,
    validation: &BTreeMap<String, Vec<CheckCommand>>,
    runner: &dyn ProcessRunner,
    staged: bool,
) -> Result<(), String> {
    let checks = affected
        .iter()
        .flat_map(|id| {
            validation[id].iter().chain(
                cfg.development[id]
                    .stability
                    .iter()
                    .flat_map(|policy| &policy.checks),
            )
        })
        .collect::<Vec<_>>();
    if !staged && !checks.iter().any(|check| check.reusable) {
        return Ok(());
    }
    let artifacts = checks
        .into_iter()
        .flat_map(|check| &check.artifacts)
        .collect::<BTreeSet<_>>();
    let inspection = InspectionRunner(runner);
    for path in artifacts {
        // A cached check may observe Git status. Only ignored outputs can change
        // without invalidating that identity; never silently relax the Git guard.
        let mut command = ProcessRequest::new("git", root);
        command.args = ["check-ignore", "--quiet", "--", path.as_str()]
            .iter()
            .map(Into::into)
            .collect();
        command.timeout = Duration::from_secs(30);
        let output = inspection.run(&command).map_err(|e| e.to_string())?;
        if output.timed_out || !matches!(output.code, 0 | 1) {
            return Err("cannot establish ignored output identity for reusable checks".into());
        }
        if output.code != 0 {
            return Err(format!(
                "reusable development checks require ignored artifacts throughout the transaction; disable reuse for tracked or unignored output: {path}"
            ));
        }
    }
    Ok(())
}
fn atomic(root: &Path, path: &str, bytes: &[u8]) -> Result<(), String> {
    let path = safe_relative(root, path)?;
    crate::persistence::atomic_write(&path, bytes).map_err(|e| e.to_string())
}
fn rollback(root: &Path, journal: &Journal) -> Result<(), String> {
    let mut conflicts = Vec::new();
    for (path, after) in &journal.after {
        let before = journal
            .before
            .get(path)
            .ok_or("invalid development journal")?;
        let current = config::read_current(root, path)?;
        if &current == before {
            continue;
        }
        if current.as_ref() != Some(after) {
            conflicts.push(path.clone());
            continue;
        }
        match before {
            Some(bytes) => atomic(root, path, bytes)?,
            None => {
                fs::remove_file(safe_relative(root, path)?).map_err(|e| e.to_string())?;
            }
        }
    }
    if !conflicts.is_empty() {
        return Err(format!(
            "rollback preserved external edits; resolve journal conflicts: {}",
            conflicts.join(", ")
        ));
    }
    fs::remove_file(root.join(JOURNAL)).map_err(|e| e.to_string())
}
fn save_journal(root: &Path, journal: &Journal) -> Result<(), String> {
    atomic(
        root,
        JOURNAL,
        &serde_json::to_vec_pretty(journal).map_err(|e| e.to_string())?,
    )
}
pub fn recover(root: &Path) -> CommandOutcome {
    let result: Result<Value, String> = (|| {
        let runner = crate::SystemProcessRunner;
        require_ignored_runtime(root, &runner)?;
        let _sync_lock = crate::git_sync::lock_factory_runtime(root, &runner)?;
        let _lock = crate::project_sync::ProjectLock::acquire(root)?;
        let bytes =
            config::read_current(root, JOURNAL)?.ok_or("no unfinished development transaction")?;
        let journal: Journal = serde_json::from_value(crate::json::parse_unique_json(
            &bytes,
            "development journal",
        )?)
        .map_err(|e| e.to_string())?;
        if journal.root
            != root
                .canonicalize()
                .map_err(|e| e.to_string())?
                .to_string_lossy()
            || journal.head
                != history::git(root, &InspectionRunner(&runner), &["rev-parse", "HEAD"])?.trim()
        {
            return Err("development journal belongs to another checkout or HEAD; preserve it for manual diagnosis".into());
        }
        for path in journal.before.keys().chain(journal.after.keys()) {
            safe_relative(root, path)?;
            if matches!(path.as_str(), "VERSION" | "CHANGELOG.md" | CONFIG)
                || path.starts_with(".git/")
            {
                return Err("invalid development journal write boundary".into());
            }
        }
        if journal
            .after
            .get(RECORD)
            .is_some_and(|b| config::read_current(root, RECORD).ok().flatten().as_ref() == Some(b))
        {
            fs::remove_file(root.join(JOURNAL)).map_err(|e| e.to_string())?;
        } else {
            rollback(root, &journal)?;
        }
        Ok(json!({"status":"development-recovered","git_written":false}))
    })();
    outcome(result)
}

fn run_check(
    root: &Path,
    check: &CheckCommand,
    environment: Option<&portable_environment::Contract>,
    filtered: bool,
    runner: &dyn ProcessRunner,
    logs: Option<&mut (Vec<u8>, Vec<u8>)>,
) -> Result<(Value, BTreeMap<String, String>), String> {
    for path in &check.inputs {
        if config::read_current(root, path)?.is_none() {
            return Err(format!(
                "{}: required validation input missing: {path}",
                check.id
            ));
        }
    }
    let mut command = ProcessRequest::new(&check.program, root);
    command.args = check.args.iter().map(Into::into).collect();
    command.timeout = Duration::from_secs(check.timeout_seconds);
    if let Some(environment) = environment {
        environment.configure_process(root, &mut command)?;
    }
    let output = runner
        .run(&command)
        .map_err(|e| format!("{}: cannot run check: {e}", check.id))?;
    if output.code != 0 || output.timed_out {
        return Err(format!(
            "{}: development check failed (exit {}, timeout {}): {}",
            check.id,
            output.code,
            output.timed_out,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    verify_cargo_execution(check, &output.stdout, filtered)?;
    let mut artifacts = BTreeMap::new();
    for path in &check.artifacts {
        let bytes = config::read_current(root, path)?
            .ok_or_else(|| format!("{}: required final artifact missing: {path}", check.id))?;
        artifacts.insert(path.clone(), digest(&bytes));
    }
    if let Some(logs) = logs {
        *logs = (output.stdout.clone(), output.stderr.clone());
    }
    Ok((
        json!({"id":check.id,"status":"passed","command":check,"stdout_sha256":digest(&output.stdout),"stderr_sha256":digest(&output.stderr)}),
        artifacts,
    ))
}

fn verify_cargo_execution(
    check: &CheckCommand,
    stdout: &[u8],
    filtered: bool,
) -> Result<(), String> {
    if Path::new(&check.program)
        .file_stem()
        .and_then(|name| name.to_str())
        != Some("cargo")
        || check.args.first().map(String::as_str) != Some("test")
    {
        return Ok(());
    }
    if check
        .args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--list" | "--help" | "-h" | "--no-run"))
    {
        return Err(format!(
            "{}: cargo test did not execute validation tests",
            check.id
        ));
    }
    let text = String::from_utf8_lossy(stdout);
    let counts = text
        .lines()
        .filter_map(|line| {
            let summary = line.trim().strip_prefix("test result: ")?;
            let words = summary.split_whitespace().collect::<Vec<_>>();
            if words.first() != Some(&"ok.")
                || words.get(2) != Some(&"passed;")
                || words.get(4) != Some(&"failed;")
            {
                return None;
            }
            Some(
                words
                    .get(1)?
                    .parse::<u64>()
                    .ok()?
                    .saturating_add(words.get(3)?.parse::<u64>().ok()?),
            )
        })
        .collect::<Vec<_>>();
    if !counts.is_empty() && counts.iter().all(|count| *count == 0) {
        return Err(format!(
            "{}: cargo test executed zero tests; choose a meaningful validation filter",
            check.id
        ));
    }
    if filtered && counts.is_empty() {
        return Err(format!(
            "{}: cannot prove filtered cargo test executed tests; use a registered check with a supported test summary",
            check.id
        ));
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CachedCheck {
    schema: u32,
    key: String,
    proof: Value,
    artifacts: BTreeMap<String, String>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    checksum: String,
}

fn cache_input_snapshot(
    root: &Path,
    p: &Prepared,
    check: &CheckCommand,
) -> Result<BTreeMap<String, Option<String>>, String> {
    let mut inputs = BTreeMap::new();
    for path in p.capture.inputs.keys() {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if check.artifacts.contains(&relative) {
            continue;
        }
        if (relative == RECORD || p.analysis_path.as_ref() == Some(path))
            && !check.inputs.contains(&relative)
        {
            continue;
        }
        inputs.insert(
            relative.clone(),
            config::read_current(root, &relative)?
                .as_deref()
                .map(digest),
        );
    }
    Ok(inputs)
}

/// Task proofs follow this package's declared inputs and dependency closure.
/// Transaction guards still cover the complete worktree during execution.
fn task_input_snapshot(
    root: &Path,
    p: &Prepared,
    id: &str,
    check: &CheckCommand,
) -> Result<BTreeMap<String, Option<String>>, String> {
    let scope = dependency_scope(&p.capture, id);
    let policy = &p.cfg.development[id];
    let mut inputs = cache_input_snapshot(root, p, check)?;
    inputs.retain(|path, _| {
        check.inputs.contains(path)
            || policy
                .stability
                .as_ref()
                .is_some_and(|s| s.contracts.contains(path))
            || scope
                .iter()
                .any(|id| uses_build(&p.cfg, &p.capture, id, path))
            || polyglot::owner(&p.capture.current, path, &p.cfg.native)
                .unwrap_or_default()
                .iter()
                .any(|id| scope.contains(id))
    });
    Ok(inputs)
}

fn run_candidate_check(
    root: &Path,
    p: &Prepared,
    id: &str,
    check: &CheckCommand,
    runner: &dyn ProcessRunner,
    validation_only: bool,
) -> Result<(Value, BTreeMap<String, String>), String> {
    let environment = p.cfg.development[id].environment.as_ref();
    let filtered = p.cfg.development[id]
        .checks
        .iter()
        .find(|registered| registered.id == check.id)
        .is_some_and(|registered| registered.args != check.args);
    if check.always_run || (!check.reusable && environment.is_none()) {
        return run_check(root, check, environment, filtered, runner, None);
    }
    // Task evidence is produced by --validate, not by accepting chat claims or
    // importing arbitrary logs. Persistent retry caching remains opt-in.
    let staged = !check.reusable;
    let stage_path = format!(
        ".runtime/bridgeforge-codex/develop-validation/{}.json",
        hash(&json!({"package":id,"check":check.id}))?.trim_start_matches("sha256:")
    );
    if staged && !validation_only && config::read_current(root, &stage_path)?.is_none() {
        return run_check(root, check, environment, filtered, runner, None);
    }
    if check.reusable && environment.is_none() {
        return Err("reusable check environment is missing".into());
    }
    let host = inputs::environment(root, &p.cfg, p.capture.environment_guard.as_ref(), runner)?;
    if host != p.capture.environment {
        return Err("validation environment changed before reusable check".into());
    }
    let facts = environment
        .map(|e| e.snapshot(root, &p.capture))
        .transpose()?;
    let inputs = if staged {
        task_input_snapshot(root, p, id, check)?
    } else {
        cache_input_snapshot(root, p, check)?
    };
    let inventory = independent::inventory(root, &InspectionRunner(runner))?;
    let git = check_git_identity(root, runner)?;
    let key_scope = if staged {
        json!({"inputs":inputs,"modes":p.capture.modes.iter().filter(|(path,_)|inputs.contains_key(*path)).collect::<BTreeMap<_,_>>(),
            "policy":p.cfg.development[id],"git":check_git_identity_without_status(root, runner)?})
    } else {
        json!({"candidate":p.writes.iter().map(|(path,bytes)|(path,digest(bytes))).collect::<BTreeMap<_,_>>(),
            "inputs":inputs,"inventory":inventory,"git":git,"plan":p.capture.fingerprint})
    };
    let key = hash(
        &json!({"schema":1,"root":root.canonicalize().map_err(|e|e.to_string())?.to_string_lossy(),
        "engine":env!("CARGO_PKG_VERSION"),"engine_binary":running_tool_identity()?,"package":id,"evidence":p.check_keys.get(id),"command":check,"filtered":filtered,
        "scope":key_scope,"environment":facts,"host_environment":host,"staged":staged}),
    )?;
    let path = if staged {
        stage_path
    } else {
        format!(
            ".runtime/bridgeforge-codex/develop-check-cache/{}.json",
            key.trim_start_matches("sha256:")
        )
    };
    let saved = config::read_current(root, &path)?;
    // Explicit validation refreshes staged evidence; failure must invalidate
    // an older success for the same task/check.
    if validation_only && staged && saved.is_some() {
        fs::remove_file(safe_relative(root, &path)?).map_err(|e| e.to_string())?;
    }
    if let Some(bytes) = saved.filter(|_| !(validation_only && staged)) {
        if let Ok(value) = crate::json::parse_unique_json(&bytes, "development check cache") {
            if let Ok(cached) = serde_json::from_value::<CachedCheck>(value) {
                let checksum = hash(&json!({"proof":cached.proof,"artifacts":cached.artifacts,
                    "stdout":digest(&cached.stdout),"stderr":digest(&cached.stderr)}))?;
                if cached.schema == 1
                    && cached.key == key
                    && checksum == cached.checksum
                    && cached.proof["id"] == check.id
                    && cached.proof["status"] == "passed"
                    && cached.proof["command"]
                        == serde_json::to_value(check).map_err(|e| e.to_string())?
                    && cached.proof["stdout_sha256"] == digest(&cached.stdout)
                    && cached.proof["stderr_sha256"] == digest(&cached.stderr)
                    && verify_cargo_execution(check, &cached.stdout, filtered).is_ok()
                    && cached.artifacts.keys().collect::<BTreeSet<_>>()
                        == check.artifacts.iter().collect::<BTreeSet<_>>()
                    && cached.artifacts.iter().all(|(path, expected)| {
                        config::read_current(root, path)
                            .ok()
                            .flatten()
                            .as_deref()
                            .map(digest)
                            .as_ref()
                            == Some(expected)
                    })
                {
                    let mut proof = cached.proof;
                    proof["cache_reused"] = json!(true);
                    proof["cache_key"] = json!(key);
                    proof["evidence_source"] = json!(if staged {
                        "development-validation"
                    } else {
                        "check-cache"
                    });
                    return Ok((proof, cached.artifacts));
                }
            }
        }
    }
    let mut logs = (Vec::new(), Vec::new());
    let (mut proof, artifacts) =
        run_check(root, check, environment, filtered, runner, Some(&mut logs))?;
    if (if staged {
        task_input_snapshot(root, p, id, check)?
    } else {
        cache_input_snapshot(root, p, check)?
    }) != inputs
        || independent::inventory(root, &InspectionRunner(runner))? != inventory
        || check_git_identity(root, runner)? != git
        || inputs::environment(root, &p.cfg, p.capture.environment_guard.as_ref(), runner)? != host
        || environment
            .map(|e| e.snapshot(root, &p.capture))
            .transpose()?
            != facts
    {
        return Err(format!(
            "{}: reusable check changed its validation inputs; no successful cache was saved",
            check.id
        ));
    }
    proof["cache_reused"] = json!(false);
    proof["cache_key"] = json!(key);
    if staged && !validation_only {
        return Ok((proof, artifacts));
    }
    let checksum = hash(
        &json!({"proof":proof,"artifacts":artifacts,"stdout":digest(&logs.0),"stderr":digest(&logs.1)}),
    )?;
    let cached = CachedCheck {
        schema: 1,
        key,
        proof: proof.clone(),
        artifacts: artifacts.clone(),
        stdout: logs.0,
        stderr: logs.1,
        checksum,
    };
    let destination = safe_relative(root, &path)?;
    fs::create_dir_all(destination.parent().ok_or("check cache has no parent")?)
        .map_err(|e| e.to_string())?;
    atomic(
        root,
        &path,
        &serde_json::to_vec(&cached).map_err(|e| e.to_string())?,
    )?;
    Ok((proof, artifacts))
}

fn running_tool_identity() -> Result<String, String> {
    static IDENTITY: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    IDENTITY
        .get_or_init(|| {
            use std::io::Read;
            let executable = std::env::current_exe().map_err(|e| e.to_string())?;
            let mut file = fs::File::open(executable).map_err(|e| e.to_string())?;
            let mut digest = Sha256::new();
            let mut buffer = [0u8; 65536];
            loop {
                let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                digest.update(&buffer[..count]);
            }
            Ok(format!("sha256:{:x}", digest.finalize()))
        })
        .clone()
}

fn check_git_identity(root: &Path, runner: &dyn ProcessRunner) -> Result<String, String> {
    git_identity(root, runner, true)
}
fn check_git_identity_without_status(
    root: &Path,
    runner: &dyn ProcessRunner,
) -> Result<String, String> {
    git_identity(root, runner, false)
}
fn git_identity(
    root: &Path,
    runner: &dyn ProcessRunner,
    include_status: bool,
) -> Result<String, String> {
    let runner = InspectionRunner(runner);
    let mut state = BTreeMap::new();
    for (role, args) in [
        ("head", vec!["rev-parse", "HEAD"]),
        ("branch", vec!["rev-parse", "--symbolic-full-name", "HEAD"]),
        ("index", vec!["ls-files", "--stage", "-z"]),
        (
            "configuration",
            vec!["config", "--null", "--list", "--show-origin"],
        ),
        (
            "refs",
            vec!["for-each-ref", "--format=%(refname)%00%(objectname)"],
        ),
    ] {
        state.insert(role, digest(history::git(root, &runner, &args)?.as_bytes()));
    }
    if include_status {
        state.insert(
            "status",
            digest(independent::status(root, &runner)?.as_bytes()),
        );
    }
    hash(&state)
}

fn execute(
    root: &Path,
    mut p: Prepared,
    runner: &dyn ProcessRunner,
    renderer: Option<&Materializer>,
    validation_only: bool,
) -> Result<Value, String> {
    let mut journal = Journal {
        root: root
            .canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into(),
        head: p.capture.head.clone(),
        before: BTreeMap::new(),
        after: p.writes.clone(),
    };
    for path in journal.after.keys() {
        journal
            .before
            .insert(path.clone(), config::read_current(root, path)?);
    }
    verify_release_inputs(&p.capture.inputs)?;
    development_inputs::verify(root, &p.capture, &p.capture.inputs)?;
    let portable_before = p
        .affected
        .iter()
        .filter_map(|id| {
            p.cfg
                .development
                .get(id)
                .and_then(|p| p.environment.as_ref())
                .map(|contract| (id, contract))
        })
        .map(|(id, contract)| Ok((id.clone(), contract.snapshot(root, &p.capture)?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    save_journal(root, &journal)?;
    let result: Result<Value, String> = (|| {
        let mut proofs = BTreeMap::<String, Vec<Value>>::new();
        let mut artifacts = BTreeMap::<String, BTreeMap<String, String>>::new();
        let mut cached_git_guard = None;
        // Each failed stability contract downgrades only that package. Re-render and
        // rerun checks for the final dependency graph; successful packages may graduate.
        for round in 0..=p.affected.len() {
            for (path, bytes) in &p.writes {
                atomic(root, path, bytes)?;
            }
            if validation_only
                || root
                    .join(".runtime/bridgeforge-codex/develop-validation")
                    .is_dir()
                || p.affected.iter().any(|id| {
                    let policy = &p.cfg.development[id];
                    p.analysis.validation[id]
                        .iter()
                        .chain(policy.stability.iter().flat_map(|s| &s.checks))
                        .any(|check| check.reusable)
                })
            {
                cached_git_guard = Some(check_git_identity(root, runner)?);
            }
            proofs.clear();
            artifacts.clear();
            let mut failures = BTreeMap::new();
            for id in &p.affected {
                let policy = &p.cfg.development[id];
                for check in &p.analysis.validation[id] {
                    let (proof, files) =
                        run_candidate_check(root, &p, id, check, runner, validation_only)?;
                    proofs.entry(id.clone()).or_default().push(proof);
                    artifacts.entry(id.clone()).or_default().extend(files);
                }
                if let Some(package) = p.capture.current.packages.get(id) {
                    if p.versions.get(&package.manifest) != p.fallback.get(&package.manifest) {
                        for check in &policy
                            .stability
                            .as_ref()
                            .ok_or("stability policy missing")?
                            .checks
                        {
                            match run_candidate_check(root, &p, id, check, runner, validation_only)
                            {
                                Ok((proof, files)) => {
                                    proofs.entry(id.clone()).or_default().push(proof);
                                    artifacts.entry(id.clone()).or_default().extend(files);
                                }
                                Err(error) => {
                                    failures.insert(id.clone(), error);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            if failures.is_empty() {
                for id in &p.affected {
                    if let Some(package) = p.capture.current.packages.get(id) {
                        if p.versions.get(&package.manifest) != p.fallback.get(&package.manifest) {
                            p.stability.insert(
                                id.clone(),
                                "stable contract and core/integration checks passed on final 1.0.0"
                                    .into(),
                            );
                        }
                    }
                }
                break;
            }
            if round == p.affected.len() {
                return Err("stability fallback did not converge".into());
            }
            rollback(root, &journal)?;
            for (id, error) in failures {
                let manifest = &p.capture.current.packages[&id].manifest;
                p.versions
                    .insert(manifest.clone(), p.fallback[manifest].clone());
                p.stability
                    .insert(id, format!("remains in development: {error}"));
            }
            p.writes = plan::render(root, &p.cfg, &p.capture, &p.versions)?;
            // A candidate graduation may have required a caller bump. If the final
            // fallback no longer changes that requirement, retain the caller version.
            for _ in 0..=p.affected.len() {
                let owners = requirement_owners(root, &p.cfg, &p.capture, &p.writes)?;
                let mut changed = false;
                for id in &p.affected {
                    if let Some(package) = p.capture.current.packages.get(id) {
                        if !p.direct_changes.contains(id)
                            && !owners.contains(&package.manifest)
                            && p.versions.get(&package.manifest) != Some(&package.version)
                        {
                            p.versions
                                .insert(package.manifest.clone(), package.version.clone());
                            p.fallback
                                .insert(package.manifest.clone(), package.version.clone());
                            p.levels.insert(id.clone(), 0);
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
                p.writes = plan::render(root, &p.cfg, &p.capture, &p.versions)?;
            }
            materialize(root, &mut p.writes, renderer)?;
            journal = Journal {
                root: journal.root.clone(),
                head: journal.head.clone(),
                before: BTreeMap::new(),
                after: p.writes.clone(),
            };
            for path in journal.after.keys() {
                journal
                    .before
                    .insert(path.clone(), config::read_current(root, path)?);
            }
            save_journal(root, &journal)?;
        }
        for id in &p.affected {
            for path in &p.cfg.development[id].release_artifacts {
                if !artifacts
                    .get(id)
                    .is_some_and(|files| files.contains_key(path))
                {
                    return Err(format!(
                        "{id}: declared release artifact was not checked on the final candidate: {path}"
                    ));
                }
            }
        }
        let mut expected = p.capture.inputs.clone();
        for (path, bytes) in &p.writes {
            expected.insert(root.join(path), Some(bytes.clone()));
        }
        for files in artifacts.values() {
            for (path, hash) in files {
                if p.capture.inventory.contains(path)
                    && !p.cfg.build_inputs.iter().any(|p| polyglot::owns(p, path))
                {
                    return Err(format!(
                        "tracked artifact must be declared as a build input: {path}"
                    ));
                }
                let bytes = config::read_current(root, path)?
                    .ok_or_else(|| format!("final artifact disappeared: {path}"))?;
                if digest(&bytes) != *hash {
                    return Err(format!("final artifact changed after its check: {path}"));
                }
                expected.insert(root.join(path), Some(bytes));
            }
        }
        verify_release_inputs(&expected)?;
        development_inputs::verify(root, &p.capture, &expected)?;
        if history::git(root, &InspectionRunner(runner), &["rev-parse", "HEAD"])?.trim()
            != p.capture.head
        {
            return Err("HEAD changed during development validation".into());
        }
        if inputs::modes(root, &InspectionRunner(runner))? != p.capture.modes {
            return Err("file modes changed during development validation".into());
        }
        if let Some(environment) = &p.capture.environment_guard {
            environment.verify(root)?;
        }
        if inputs::environment(root, &p.cfg, p.capture.environment_guard.as_ref(), runner)?
            != p.capture.environment
        {
            return Err("validation environment changed during development checks".into());
        }
        let inventory = independent::inventory(root, &InspectionRunner(runner))?;
        if inventory != p.capture.inventory {
            return Err("project file inventory changed during development checks".into());
        }
        for (id, expected) in &portable_before {
            if p.cfg.development[id]
                .environment
                .as_ref()
                .unwrap()
                .snapshot(root, &p.capture)?
                != *expected
            {
                return Err(format!(
                    "{id}: portable environment changed during development checks"
                ));
            }
        }
        p.capture.current = polyglot::snapshot(
            &p.cfg.native,
            p.capture.inventory.clone(),
            false,
            &mut |path| config::read_current(root, path),
        )?;
        p.capture.inputs = expected.clone();
        let hashes = if p.task_scope.is_some() {
            fingerprints_scoped(root, &p.cfg, &mut p.capture, runner, p.task_scope.as_ref())?
        } else {
            fingerprints(root, &p.cfg, &mut p.capture, runner)?
        };
        let mut changes = Vec::new();
        let reused_checks = proofs
            .values()
            .flatten()
            .filter(|proof| proof["cache_reused"] == true)
            .count();
        let final_checks = proofs.values().map(Vec::len).sum::<usize>();
        for files in artifacts.values() {
            for (path, expected) in files {
                let bytes = config::read_current(root, path)?
                    .ok_or_else(|| format!("final artifact disappeared: {path}"))?;
                if digest(&bytes) != *expected {
                    return Err(format!("final artifact changed after its check: {path}"));
                }
                p.capture.inputs.insert(root.join(path), Some(bytes));
            }
        }
        if validation_only {
            if let Some(before) = &cached_git_guard {
                if &check_git_identity(root, runner)? != before {
                    return Err("Git identity changed during development validation".into());
                }
            }
            // Validation is a write operation, but never closes a package or
            // leaves candidate manifest/projection writes behind. Artifacts and
            // ignored task evidence remain for the following closure.
            rollback(root, &journal)?;
            return Ok(json!({"schema":1,"status":"development-validated",
                "validation_mode":"task-selected-final-candidate","version_written":false,
                "ledger_written":false,"git_written":false,
                "readiness_scope":p.receipt["readiness_scope"],"task_packages":p.receipt["task_packages"],
                "effective_task_packages":p.task_scope,"versions":p.versions,
                "checks":{"scope":"final-candidate","executed":final_checks-reused_checks,"reused":reused_checks}}));
        }
        for id in &p.affected {
            let before = p
                .ledger
                .latest(id)
                .and_then(|e| e.after.clone())
                .or_else(|| {
                    p.capture
                        .baseline
                        .packages
                        .get(id)
                        .map(|p| p.version.clone())
                });
            let after = p
                .capture
                .current
                .packages
                .get(id)
                .map(|p| p.version.clone());
            let a = &p.analysis.packages[id];
            let current = hashes.get(id).ok_or("missing package fingerprint")?;
            let stability = p
                .stability
                .get(id)
                .cloned()
                .unwrap_or_else(|| "retired".into());
            changes.push(json!({"id":id,"version_before":before,"version_after":after,"reason":a.reason,"stability":stability}));
            p.ledger.append(Entry{id:id.clone(),before,after,content:current.content.clone(),evidence:current.evidence.clone(),evidence_schema:Some(6),evidence_components:Some(current.components.clone()),evidence_details:None,
                level:p.levels[id],compatibility:a.compatibility.clone(),acceptance:a.acceptance.as_ref().map(|v|json!({"fingerprint":v.fingerprint,"scope":v.scope,"reason":v.reason,"source":v.source})),reason:a.reason.clone(),stability,
                checks:proofs.remove(id).unwrap_or_default(),artifacts:artifacts.remove(id).unwrap_or_default(),previous:String::new(),hash:String::new()})?;
        }
        let bytes = serde_json::to_vec_pretty(&p.ledger).map_err(|e| e.to_string())?;
        Ledger::parse(Some(&bytes))?;
        journal
            .before
            .insert(RECORD.into(), config::read_current(root, RECORD)?);
        journal.after.insert(RECORD.into(), bytes.clone());
        save_journal(root, &journal)?;
        verify_release_inputs(&p.capture.inputs)?;
        development_inputs::verify(root, &p.capture, &expected)?;
        if let Some(before) = &cached_git_guard {
            if &check_git_identity(root, runner)? != before {
                return Err("Git identity changed during reusable development checks".into());
            }
        }
        atomic(root, RECORD, &bytes)?;
        fs::remove_file(root.join(JOURNAL)).map_err(|e| e.to_string())?;
        Ok(
            json!({"schema":1,"status":"development-closed","validation_mode":"task-selected-final-candidate","product_version":p.capture.product_version,"package_changes":changes,"git_written":false,
                "readiness_scope":p.receipt["readiness_scope"],"task_packages":p.receipt["task_packages"],"effective_task_packages":p.task_scope,
                "outside_task_evidence_checked":p.receipt["outside_task_evidence_checked"],
                "checks":{"scope":"final-candidate","executed":final_checks-reused_checks,"reused":reused_checks}}),
        )
    })();
    match result {
        Ok(value) => Ok(value),
        Err(error) => match rollback(root, &journal) {
            Ok(()) => Err(format!("{error}; native version writes rolled back")),
            Err(recovery) => Err(format!("{error}; {recovery}")),
        },
    }
}
fn outcome(result: Result<Value, String>) -> CommandOutcome {
    match result {
        Ok(value) => CommandOutcome::with_receipt(value),
        Err(error) => CommandOutcome::blocked(format!("[package-development] {error}\n")),
    }
}
pub fn preview(root: &Path, request: &Request, runner: &dyn ProcessRunner) -> CommandOutcome {
    preview_with_materializer(root, request, runner, None)
}

pub fn preview_with_materializer(
    root: &Path,
    request: &Request,
    runner: &dyn ProcessRunner,
    renderer: Option<&Materializer>,
) -> CommandOutcome {
    preview_scoped(root, request, runner, renderer, None)
}

pub fn preview_for_packages(
    root: &Path,
    request: &Request,
    packages: &str,
    runner: &dyn ProcessRunner,
    renderer: Option<&Materializer>,
) -> CommandOutcome {
    match parse_task_packages(packages) {
        Ok(packages) => preview_scoped(root, request, runner, renderer, Some(&packages)),
        Err(error) => outcome(Err(error)),
    }
}

fn preview_scoped(
    root: &Path,
    request: &Request,
    runner: &dyn ProcessRunner,
    renderer: Option<&Materializer>,
    packages: Option<&BTreeSet<String>>,
) -> CommandOutcome {
    outcome(
        prepare(root, request, runner, renderer, packages).map(|p| match p {
            Ok(p) => p.receipt,
            Err(receipt) => receipt,
        }),
    )
}
pub fn apply(
    root: &Path,
    request: &Request,
    expected: &str,
    runner: &dyn ProcessRunner,
) -> CommandOutcome {
    apply_with_materializer(root, request, expected, runner, None)
}

pub fn apply_with_materializer(
    root: &Path,
    request: &Request,
    expected: &str,
    runner: &dyn ProcessRunner,
    renderer: Option<&Materializer>,
) -> CommandOutcome {
    apply_scoped(root, request, expected, runner, renderer, None)
}

pub fn apply_for_packages(
    root: &Path,
    request: &Request,
    expected: &str,
    packages: &str,
    runner: &dyn ProcessRunner,
    renderer: Option<&Materializer>,
) -> CommandOutcome {
    match parse_task_packages(packages) {
        Ok(packages) => apply_scoped(root, request, expected, runner, renderer, Some(&packages)),
        Err(error) => outcome(Err(error)),
    }
}

fn apply_scoped(
    root: &Path,
    request: &Request,
    expected: &str,
    runner: &dyn ProcessRunner,
    renderer: Option<&Materializer>,
    packages: Option<&BTreeSet<String>>,
) -> CommandOutcome {
    mutate_scoped(root, request, expected, runner, renderer, packages, false)
}

/// Execute task validation on the final candidate, restoring native version
/// writes afterwards. This is deliberately distinct from zero-write preview.
pub fn validate_for_packages(
    root: &Path,
    request: &Request,
    packages: Option<&str>,
    runner: &dyn ProcessRunner,
    renderer: Option<&Materializer>,
) -> CommandOutcome {
    match packages.map(parse_task_packages).transpose() {
        Ok(packages) => mutate_scoped(root, request, "", runner, renderer, packages.as_ref(), true),
        Err(error) => outcome(Err(error)),
    }
}

fn mutate_scoped(
    root: &Path,
    request: &Request,
    expected: &str,
    runner: &dyn ProcessRunner,
    renderer: Option<&Materializer>,
    packages: Option<&BTreeSet<String>>,
    validation_only: bool,
) -> CommandOutcome {
    outcome((|| {
        require_ignored_runtime(root, runner)?;
        let _sync_lock = crate::git_sync::lock_factory_runtime(root, runner)?;
        let _lock = crate::project_sync::ProjectLock::acquire(root)?;
        match prepare(root, request, runner, renderer, packages)? {
            Err(receipt)
                if receipt["status"] == "nothing-to-close"
                    || receipt["status"] == "task-development-current" =>
            {
                Ok(receipt)
            }
            Err(_) => Err("development analysis is required".into()),
            Ok(p) => {
                if !expected.is_empty() && expected != p.capture.fingerprint {
                    return Err("development plan changed; preview again before applying".into());
                }
                if validation_only {
                    validate_cache_artifacts(
                        root,
                        &p.cfg,
                        &p.affected,
                        &p.analysis.validation,
                        runner,
                        true,
                    )?;
                }
                execute(root, p, runner, renderer, validation_only)
            }
        }
    })())
}
