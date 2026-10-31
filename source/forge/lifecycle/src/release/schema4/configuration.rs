use super::*;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::release) struct Baseline {
    pub revision: String,
    pub migration_head: String,
    pub product_version: String,
    pub source_schema: u32,
    pub initial_configuration_hash: String,
    pub initial_configuration: Value,
    pub packages: BTreeMap<String, NativePackage>,
    pub dependencies: Vec<NativeDependency>,
    pub retired_ids: BTreeSet<String>,
    pub input_hashes: BTreeMap<String, Option<String>>,
    pub product_inputs: Vec<String>,
    pub shared_inputs: Vec<polyglot::SharedInput>,
    pub lifecycle: Vec<lifecycle::Transition>,
    pub lineage: Vec<LineageState>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::release) struct LineageState {
    pub revision: String,
    pub packages: BTreeMap<String, NativePackage>,
}

#[derive(Clone, Debug)]
pub(in crate::release) struct Config {
    pub native: polyglot::Config,
    pub strategy: String,
    pub current_model: bool,
    pub baseline: Baseline,
    pub build_inputs: Vec<String>,
    pub record_inputs: Vec<String>,
    pub required_checks: Vec<String>,
    pub check_scopes: BTreeMap<String, Vec<String>>,
    pub product_manifests: Vec<String>,
    pub python_environment: Option<PythonEnvironment>,
    pub development: BTreeMap<String, development::Policy>,
    pub package_adoptions: Vec<adoption::Adoption>,
    pub release_withdrawals: Vec<withdrawal::Declaration>,
}

pub(super) fn configuration_hash(value: &Value) -> Result<String, String> {
    let mut value = value.clone();
    value
        .as_object_mut()
        .ok_or("release configuration must be an object")?
        .remove("baseline");
    crate::manifest::canonical_sha(&value)
}

fn strings(value: &Value, key: &str) -> Result<Vec<String>, String> {
    value
        .get(key)
        .map(|v| {
            v.as_array()
                .ok_or_else(|| format!("{key} must be an array"))?
                .iter()
                .map(|p| {
                    p.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| format!("{key} must contain strings"))
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

pub(in crate::release) fn parse(payload: &[u8]) -> Result<Option<Config>, String> {
    let mut value = crate::json::parse_unique_json(payload, "schema 4 release config")?;
    if value["schema_version"].as_u64() != Some(4) {
        return Ok(None);
    }
    let strategy = value["strategy"]
        .as_str()
        .ok_or("schema 4 requires a strategy")?
        .to_string();
    if !matches!(strategy.as_str(), "synchronized" | "independent") {
        return Err("schema 4 strategy must be synchronized or independent".into());
    }
    let baseline: Baseline = serde_json::from_value(
        value
            .get("baseline")
            .cloned()
            .ok_or("schema 4 requires a verified migration baseline")?,
    )
    .map_err(|e| format!("invalid schema 4 baseline: {e}"))?;
    for revision in [&baseline.revision, &baseline.migration_head] {
        if !matches!(revision.len(), 40 | 64) || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("migration baseline requires immutable Git revisions".into());
        }
    }
    baseline.product_version.parse::<SemVer>()?;
    if !(1..=4).contains(&baseline.source_schema) {
        return Err("invalid migration source schema".into());
    }
    let build_inputs = strings(&value, "build_inputs")?;
    let record_inputs = strings(&value, "record_inputs")?;
    let required_checks = strings(&value, "required_checks")?;
    let check_scopes = value
        .get("check_scopes")
        .cloned()
        .map(serde_json::from_value::<BTreeMap<String, Vec<String>>>)
        .transpose()
        .map_err(|e| format!("invalid check_scopes: {e}"))?
        .unwrap_or_default();
    for (id, scope) in &check_scopes {
        if !required_checks.contains(id)
            || scope.is_empty()
            || scope.iter().any(|s| s.trim().is_empty())
            || scope.iter().collect::<BTreeSet<_>>().len() != scope.len()
        {
            return Err(
                "check_scopes requires declared checks and unique nonempty object scopes".into(),
            );
        }
    }
    let current_model = value["release_model"] == "product-packages";
    let release_withdrawals = withdrawal::declarations(&value)?;
    if !current_model && !release_withdrawals.is_empty() {
        return Err("release_withdrawals requires the product-packages model".into());
    }
    let package_adoptions = value
        .get("package_adoptions")
        .cloned()
        .map(serde_json::from_value::<Vec<adoption::Adoption>>)
        .transpose()
        .map_err(|e| format!("invalid package_adoptions: {e}"))?
        .unwrap_or_default();
    let development = value
        .get("development")
        .cloned()
        .map(serde_json::from_value::<BTreeMap<String, development::Policy>>)
        .transpose()
        .map_err(|e| format!("invalid development policy: {e}"))?
        .unwrap_or_default();
    for (id, policy) in &development {
        policy.validate(id)?;
    }
    if value.get("release_model").is_some() && !current_model {
        return Err("unsupported release_model".into());
    }
    if current_model && (strategy != "independent" || value.get("synchronized_manifests").is_some())
    {
        return Err(
            "product-packages requires independent package versions and product_manifests".into(),
        );
    }
    let product_manifests = strings(
        &value,
        if current_model {
            "product_manifests"
        } else {
            "synchronized_manifests"
        },
    )?;
    let python_environment = value
        .get("python_environment")
        .cloned()
        .map(serde_json::from_value::<PythonEnvironment>)
        .transpose()
        .map_err(|e| format!("invalid python_environment: {e}"))?;
    if let Some(environment) = &python_environment {
        environment.validate()?;
    }
    if !current_model && strategy == "independent" && !product_manifests.is_empty() {
        return Err("independent strategy cannot contain synchronized manifests".into());
    }
    for paths in [&build_inputs, &record_inputs, &product_manifests] {
        let mut unique = BTreeSet::new();
        for p in paths {
            safe_relative(Path::new("."), p.trim_end_matches('/'))?;
            if !unique.insert(p) {
                return Err("duplicate schema 4 input role".into());
            }
        }
    }
    for p in &record_inputs {
        if p.ends_with('/')
            || matches!(p.as_str(), "VERSION" | "CHANGELOG.md" | CONFIG)
            || !matches!(
                Path::new(p).extension().and_then(|e| e.to_str()),
                Some("json" | "jsonl")
            )
        {
            return Err(format!(
                "record_inputs must name exact automatic records, not source/version/lock inputs: {p}"
            ));
        }
    }
    for p in &build_inputs {
        if matches!(
            Path::new(p).file_name().and_then(|n| n.to_str()),
            Some("Cargo.toml" | "pyproject.toml" | "setup.cfg")
        ) || matches!(
            Path::new(p).extension().and_then(|e| e.to_str()),
            Some("rs" | "py" | "pyi" | "c" | "cc" | "cpp" | "h" | "hpp")
        ) {
            return Err(format!(
                "build_inputs cannot suppress package source or native manifest changes: {p}"
            ));
        }
    }
    if build_inputs.iter().any(|a| {
        record_inputs
            .iter()
            .any(|b| polyglot::owns(a, b) || polyglot::owns(b, a))
    }) {
        return Err("overlapping build/record input roles".into());
    }
    if required_checks.iter().any(|id| id.trim().is_empty())
        || required_checks.iter().collect::<BTreeSet<_>>().len() != required_checks.len()
    {
        return Err("required_checks must contain unique nonempty ids".into());
    }
    if value
        .get("target_versions")
        .is_some_and(|v| v.as_object().is_none_or(|v| !v.is_empty()))
        || value
            .get("target_levels")
            .is_some_and(|v| v.as_object().is_none_or(|v| !v.is_empty()))
    {
        return Err(
            "schema 4 explicit targets belong in one release analysis, not persistent config"
                .into(),
        );
    }
    let object = value
        .as_object_mut()
        .ok_or("release config must be an object")?;
    for key in [
        "baseline",
        "build_inputs",
        "record_inputs",
        "required_checks",
        "check_scopes",
        "synchronized_manifests",
        "product_manifests",
        "release_model",
        "python_environment",
        "development",
        "package_adoptions",
        "release_withdrawals",
    ] {
        object.remove(key);
    }
    object.insert("schema_version".into(), json!(3));
    object.insert("strategy".into(), json!("independent"));
    object.entry("adapters").or_insert(json!({}));
    object.entry("packages").or_insert(json!([]));
    let native = polyglot::parse(&serde_json::to_vec(&value).map_err(|e| e.to_string())?)?
        .ok_or("invalid schema 4 adapter configuration")?;
    if !current_model && !native.product_packages.is_empty() {
        return Err("explicit product packages require the product-packages release model".into());
    }
    if !native.product_packages.is_empty() && native.adapters.rust.is_none() {
        return Err("product packages require an explicit Rust adapter workspace".into());
    }
    adoption::validate_configuration(&package_adoptions, &native, current_model)?;
    if native.packages.iter().any(|p| p.id == "product") {
        return Err("product is reserved for the project version target".into());
    }
    for scope in check_scopes.values().flatten() {
        if scope != "product"
            && !native.packages.iter().any(|p| &p.id == scope)
            && !native
                .lifecycle
                .iter()
                .any(|t| t.from.iter().chain(&t.to).any(|p| &p.id == scope))
        {
            return Err(format!("check_scopes references unknown object: {scope}"));
        }
    }
    for p in &record_inputs {
        if native.packages.iter().any(|n| n.manifest == *p) {
            return Err("a package manifest cannot be an automatic record".into());
        }
    }
    if let Some(python) = &native.adapters.python {
        for source in python.version_sources.values() {
            if record_inputs.contains(&source.path)
                || build_inputs.iter().any(|p| polyglot::owns(p, &source.path))
            {
                return Err(
                    "a package version source cannot be an automatic record or global build input"
                        .into(),
                );
            }
        }
    }
    for p in &product_manifests {
        if native.packages.iter().any(|n| n.manifest == *p)
            || Path::new(p).file_name().and_then(|n| n.to_str()) != Some("Cargo.toml")
        {
            return Err("product_manifests requires unambiguous Cargo product mappings; register language packages via adapters".into());
        }
    }
    Ok(Some(Config {
        native,
        strategy,
        current_model,
        baseline,
        build_inputs,
        record_inputs,
        required_checks,
        check_scopes,
        product_manifests,
        python_environment,
        development,
        package_adoptions,
        release_withdrawals,
    }))
}

pub(in crate::release) fn load(root: &Path) -> Result<Option<Config>, String> {
    config::read_current(root, CONFIG)?
        .map(|b| parse(&b))
        .transpose()
        .map(Option::flatten)
}
