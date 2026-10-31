use super::*;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Acceptance {
    pub fingerprint: String,
    pub scope: Vec<String>,
    pub reason: String,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObjectAnalysis {
    pub level: String,
    pub reason: String,
    pub compatibility: String,
    pub acceptance: Option<Acceptance>,
}
impl ObjectAnalysis {
    pub(super) fn version_level(
        &self,
        id: &str,
        fingerprint: &str,
        unstable: bool,
    ) -> Result<u8, String> {
        let level = match self.level.as_str() {
            "patch" => 1,
            "minor" => 2,
            "major" => 3,
            _ => return Err(format!("{id}: level must be patch, minor or major")),
        };
        if self.reason.trim().is_empty() {
            return Err(format!("{id}: release reason is required"));
        }
        match self.compatibility.as_str() {
            "compatible" => {}
            "breaking" => {
                if level != if unstable { 2 } else { 3 } {
                    return Err(format!(
                        "{id}: breaking changes require {}",
                        if unstable {
                            "minor during 0.x"
                        } else {
                            "major"
                        }
                    ));
                }
                let accepted = self.acceptance.as_ref().ok_or_else(|| {
                    format!("{id}: incompatible changes require explicit user acceptance")
                })?;
                if accepted.fingerprint != fingerprint
                    || !accepted.scope.iter().any(|s| s == id)
                    || accepted.reason.trim().is_empty()
                    || accepted.source.trim().is_empty()
                {
                    return Err(format!(
                        "{id}: incompatible acceptance is missing, stale or outside scope"
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "{id}: compatibility is unknown; release is blocked"
                ));
            }
        }
        Ok(level)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Check {
    pub id: String,
    pub status: String,
    pub fingerprint: String,
    pub scope: Vec<String>,
    pub reason: String,
    pub artifact: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InterfaceImpact {
    pub dependencies: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Analysis {
    pub schema: u32,
    pub fingerprint: String,
    pub product: Option<ObjectAnalysis>,
    #[serde(default)]
    pub packages: BTreeMap<String, ObjectAnalysis>,
    #[serde(default)]
    pub targets: BTreeMap<String, String>,
    // Accepted as inert input so an old analysis cannot use it to create impact.
    #[serde(rename = "pure_version_reason")]
    pub _retired_pure_version_reason: Option<String>,
    #[serde(default)]
    pub checks: Vec<Check>,
    /// Necessary checks selected for this task and executed on its final
    /// candidate; project stability checks remain an independent requirement.
    #[serde(default)]
    pub validation: BTreeMap<String, Vec<development::CheckCommand>>,
    #[serde(default)]
    pub interface_impacts: BTreeMap<String, InterfaceImpact>,
}

pub(super) fn read(
    root: &Path,
    request: &Request,
    capture: &mut Capture,
) -> Result<Option<Analysis>, String> {
    let _profile = crate::release_metrics::span("analysis.read_validate");
    let Some(path) = &request.analysis_file else {
        return Ok(None);
    };
    let path = if path.is_absolute() {
        path.clone()
    } else {
        root.join(path)
    };
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "release analysis must belong to this project")?
        .to_string_lossy()
        .replace('\\', "/");
    if !relative.starts_with(".runtime/") {
        return Err("release analysis must be stored in ignored .runtime inputs".into());
    }
    if capture.inventory.contains(&relative) {
        return Err("release analysis must be ignored and untracked".into());
    }
    let bytes = config::read_current(root, &relative)?.ok_or("release analysis file is missing")?;
    let analysis = validate(&bytes, capture)?;
    capture.inputs.insert(path, Some(bytes));
    Ok(Some(analysis))
}

/// A preview produces no transaction and may inspect an external analysis file
/// without writing into the target project. Mutation paths must keep using read.
pub(super) fn read_preview(
    root: &Path,
    request: &Request,
    capture: &mut Capture,
) -> Result<Option<Analysis>, String> {
    let Some(path) = request
        .analysis_file
        .as_ref()
        .filter(|p| p.is_absolute() && !p.starts_with(root))
    else {
        return read(root, request, capture);
    };
    for ancestor in path.ancestors().filter(|p| p.exists()) {
        if crate::persistence::is_link_or_reparse(ancestor).map_err(|e| e.to_string())? {
            return Err("external preview analysis must not traverse a link".into());
        }
    }
    if !fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("external preview analysis must be a regular file".into());
    }
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    if path.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
        return Err("project-local analysis must use its ignored .runtime path".into());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let analysis = validate(&bytes, capture)?;
    capture.inputs.insert(path, Some(bytes));
    Ok(Some(analysis))
}

fn validate(bytes: &[u8], capture: &Capture) -> Result<Analysis, String> {
    let value = crate::json::parse_unique_json(bytes, "schema 4 release analysis")?;
    let analysis: Analysis =
        serde_json::from_value(value).map_err(|e| format!("invalid release analysis: {e}"))?;
    if analysis.schema != 1 || analysis.fingerprint != capture.fingerprint {
        return Err(
            "release analysis is stale or has an unsupported schema; inspect final inputs again"
                .into(),
        );
    }
    Ok(analysis)
}

/// Explicit scopes restrict the requirement; an unscoped check covers all
/// affected objects. Never extend a scoped requirement to unrelated objects.
pub(super) fn required_check_scopes(
    cfg: &Config,
    affected: &BTreeSet<String>,
) -> BTreeMap<String, BTreeSet<String>> {
    cfg.required_checks
        .iter()
        .filter_map(|id| {
            let scope = cfg
                .check_scopes
                .get(id)
                .map(|scope| {
                    affected
                        .iter()
                        .filter(|object| scope.contains(object))
                        .cloned()
                        .collect()
                })
                .unwrap_or_else(|| affected.clone());
            if scope.is_empty() {
                None
            } else {
                Some((id.clone(), scope))
            }
        })
        .collect()
}

pub(super) fn validate_checks(
    root: &Path,
    cfg: &Config,
    capture: &mut Capture,
    analysis: &Analysis,
    affected: &BTreeSet<String>,
) -> Result<(), String> {
    if cfg.native.adapters.python.is_some()
        && !analysis.checks.is_empty()
        && capture.environment_guard.is_none()
    {
        return Err("Python validation evidence requires explicit python_environment interpreter/runtime/dependency roots".into());
    }
    let mut seen = BTreeSet::new();
    for check in &analysis.checks {
        if !seen.insert(check.id.clone()) || check.id.trim().is_empty() {
            return Err("duplicate/empty release evidence id".into());
        }
        if check.fingerprint != capture.validation_fingerprint {
            return Err(format!("{}: validation evidence is stale", check.id));
        }
        if check.scope.is_empty() || check.reason.trim().is_empty() {
            return Err(format!(
                "{}: validation scope and reason are required",
                check.id
            ));
        }
        if check.scope.iter().any(|id| {
            id != "product"
                && !capture.current.packages.contains_key(id)
                && !capture.baseline.packages.contains_key(id)
        }) {
            return Err(format!(
                "{}: validation scope references unknown object",
                check.id
            ));
        }
        match check.status.as_str() {
            "passed" => {}
            "historical-unrelated" => {
                if check.scope.iter().any(|id| affected.contains(id)) {
                    return Err(format!(
                        "{}: historical failure overlaps affected release scope",
                        check.id
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "{}: new, failed or unclassified validation blocks release",
                    check.id
                ));
            }
        }
        if !check.artifact.starts_with(".runtime/") {
            return Err("validation artifacts must be project-local runtime records".into());
        }
        if capture.inventory.contains(&check.artifact) {
            return Err("validation artifacts must be ignored and untracked".into());
        }
        let bytes = config::read_current(root, &check.artifact)?
            .ok_or_else(|| format!("{}: validation artifact missing", check.id))?;
        if digest(&bytes) != check.sha256 {
            return Err(format!("{}: validation artifact changed", check.id));
        }
        let proof = crate::json::parse_unique_json(&bytes, "release validation artifact")?;
        if proof["id"] != check.id
            || proof["fingerprint"] != check.fingerprint
            || proof["status"] != check.status
            || proof["scope"] != json!(check.scope)
            || proof["reason"] != check.reason
        {
            return Err(format!(
                "{}: validation artifact does not bind this claim",
                check.id
            ));
        }
        capture
            .inputs
            .insert(root.join(&check.artifact), Some(bytes));
    }
    for (id, applicable) in required_check_scopes(cfg, affected) {
        let check = analysis
            .checks
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| format!("required release evidence missing: {id}"))?;
        if check.status != "passed" || !applicable.iter().all(|id| check.scope.contains(id)) {
            return Err(format!(
                "{id}: required evidence does not cover affected release scope"
            ));
        }
    }
    Ok(())
}

pub(super) fn requirements(cfg: &Config, capture: &Capture, ledger: &development::Ledger) -> Value {
    let mut affected = super::plan::affected(cfg, capture);
    affected.extend(capture.release_packages.iter().cloned());
    let mut evidence_scope = affected.clone();
    evidence_scope.insert("product".into());
    // Called only after release_gate succeeded. These are references to the
    // existing ledger, not new validation certificates or test-cache claims.
    let coverage = capture
        .release_packages
        .iter()
        .filter_map(|id| {
            ledger.latest(id).map(|entry| {
                (
                    id.clone(),
                    json!({
                        "version":entry.after,"record_hash":entry.hash,
                        "evidence_schema":entry.evidence_schema
                    }),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    json!({"schema":1,"status":"analysis-required","fingerprint":capture.fingerprint,"validation_fingerprint":capture.validation_fingerprint,"version_strategy":cfg.strategy,
        "version_before":capture.product_version,
        "changes":capture.changes,"affected_packages":affected,"packages":capture.current.packages.iter().map(|(id,p)|json!({"id":id,"language":p.language,"manifest":p.manifest,"version":p.version})).collect::<Vec<_>>(),
        "required_checks":cfg.required_checks,"check_scopes":cfg.check_scopes,"applicable_required_checks":required_check_scopes(cfg, &evidence_scope),"version_coverage":coverage,"development_validation":"not-executed-by-release","python_environment_required":cfg.native.adapters.python.is_some() && cfg.python_environment.is_none(),"product_manifests":cfg.product_manifests,"release_model":"product-packages","writes":[]})
}
