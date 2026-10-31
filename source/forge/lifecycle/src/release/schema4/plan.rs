use super::analysis::Analysis;
use super::*;
use crate::CommandOutcome;

fn product(version: SemVer) -> NativePackage {
    NativePackage {
        manifest: "VERSION".into(),
        name: "product".into(),
        version: version.to_string(),
        language: Language::Rust,
        workspace: String::new(),
        source_roots: vec![],
    }
}

pub(super) fn render(
    root: &Path,
    cfg: &Config,
    capture: &Capture,
    versions: &BTreeMap<String, String>,
) -> Result<model::Writes, String> {
    let mut writes = BTreeMap::new();
    if let Some(rust_cfg) = &cfg.native.adapters.rust {
        let targets = versions
            .iter()
            .filter(|(path, _)| {
                capture
                    .current
                    .packages
                    .values()
                    .any(|p| p.language == Language::Rust && &p.manifest == *path)
            })
            .map(|(p, v)| (p.clone(), v.clone()))
            .collect();
        let rendered = rust::render_versions_with_product_packages(
            rust_cfg,
            &cfg.native.product_packages,
            &capture.inventory,
            &mut |p| config::read_current(root, p),
            &targets,
        )?;
        polyglot::add_writes(&mut writes, rendered)?;
    }
    if let Some(python_cfg) = &cfg.native.adapters.python {
        let targets = versions
            .iter()
            .filter(|(path, _)| {
                capture
                    .current
                    .packages
                    .values()
                    .any(|p| p.language == Language::Python && &p.manifest == *path)
            })
            .map(|(p, v)| (p.clone(), v.clone()))
            .collect();
        polyglot::add_writes(
            &mut writes,
            python::render_versions(
                python_cfg,
                &capture.inventory,
                &mut |p| config::read_current(root, p),
                &targets,
            )?,
        )?;
    }
    Ok(writes)
}

pub(super) fn choose(
    package: &NativePackage,
    impact: u8,
    target: Option<&String>,
    first: bool,
) -> Result<String, String> {
    let minimum = if first {
        package.version.clone()
    } else {
        match polyglot::bump_native(package, impact) {
            Ok(v) => v,
            Err(_) if package.language == Language::Python && target.is_some() => {
                let target = target.unwrap();
                polyglot::validate_target(package, target)?;
                return Ok(target.clone());
            }
            Err(e) => return Err(e),
        }
    };
    if let Some(target) = target {
        polyglot::validate_target(package, target)?;
        let enough = match package.language {
            Language::Rust => target.parse::<SemVer>()? >= minimum.parse::<SemVer>()?,
            Language::Python => {
                target
                    .parse::<pep440_rs::Version>()
                    .map_err(|e| e.to_string())?
                    >= minimum
                        .parse::<pep440_rs::Version>()
                        .map_err(|e| e.to_string())?
            }
        };
        if !enough {
            return Err(format!(
                "{}: explicit target {target} is below suggested {minimum}",
                package.name
            ));
        }
        Ok(target.clone())
    } else {
        Ok(minimum)
    }
}

pub(super) fn affected(cfg: &Config, capture: &Capture) -> BTreeSet<String> {
    let mut affected = capture
        .changes
        .iter()
        .filter(|c| c.role != "record")
        .flat_map(|c| c.packages.iter().cloned())
        .collect::<BTreeSet<_>>();
    affected.extend(lifecycle_affected(cfg, capture));
    affected
}

pub(super) fn lifecycle_affected(cfg: &Config, capture: &Capture) -> BTreeSet<String> {
    let mut affected = BTreeSet::new();
    for transition in cfg
        .native
        .lifecycle
        .iter()
        .filter(|t| t.base == capture.boundary)
    {
        let endpoints = transition
            .from
            .iter()
            .chain(&transition.to)
            .map(|p| p.id.clone())
            .collect::<BTreeSet<_>>();
        let net_identity_change = endpoints.iter().any(|id| {
            match (
                capture.baseline.packages.get(id),
                capture.current.packages.get(id),
            ) {
                (Some(before), Some(after)) => {
                    before.manifest != after.manifest
                        || before.name != after.name
                        || before.language != after.language
                }
                (None, None) => false,
                _ => true,
            }
        });
        if net_identity_change {
            affected.extend(endpoints);
        }
    }
    affected
}

pub(super) fn interface_impacts(
    capture: &Capture,
    analysis: &Analysis,
    affected: &mut BTreeSet<String>,
    strict: bool,
) -> Result<bool, String> {
    let before = affected.len();
    let mut pending = analysis
        .interface_impacts
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    for (id, impact) in &analysis.interface_impacts {
        if !capture.current.packages.contains_key(id)
            || impact.reason.trim().is_empty()
            || impact.dependencies.is_empty()
            || impact.dependencies.iter().collect::<BTreeSet<_>>().len()
                != impact.dependencies.len()
        {
            return Err(format!(
                "{id}: public interface impact requires an active owner, reason and unique dependencies"
            ));
        }
        for dependency in &impact.dependencies {
            if dependency == id
                || !capture
                    .current
                    .dependencies
                    .iter()
                    .chain(&capture.baseline.dependencies)
                    .any(|edge| &edge.owner == id && &edge.target == dependency)
            {
                return Err(format!(
                    "{id}: public interface impact has no declared dependency on {dependency}"
                ));
            }
        }
    }
    while !pending.is_empty() {
        let resolved = pending
            .iter()
            .filter(|id| {
                analysis.interface_impacts[*id]
                    .dependencies
                    .iter()
                    .all(|d| affected.contains(d))
            })
            .cloned()
            .collect::<Vec<_>>();
        if resolved.is_empty() {
            if strict {
                return Err(
                    "public interface impact is not rooted in actual final package changes".into(),
                );
            }
            break;
        }
        for id in resolved {
            pending.remove(&id);
            affected.insert(id);
        }
    }
    Ok(affected.len() != before)
}

fn finish(
    root: &Path,
    message: &str,
    cfg: &Config,
    mut capture: Capture,
    analysis: Analysis,
    ledger: development::Ledger,
    runner: &dyn ProcessRunner,
) -> Result<Option<FileReleasePlan>, String> {
    if !analysis.validation.is_empty() {
        return Err(
            "release cannot execute development validation commands; complete develop separately"
                .into(),
        );
    }
    let inspection = InspectionRunner(runner);
    let runner: &dyn ProcessRunner = &inspection;
    let meaningful = capture
        .changes
        .iter()
        .filter(|c| c.role != "record")
        .cloned()
        .collect::<Vec<_>>();
    if meaningful.is_empty() {
        if !analysis.targets.is_empty() {
            return Err("version targets require actual release changes".into());
        }
        return Ok(None);
    }
    for id in analysis.targets.keys() {
        if id != "product" && !capture.current.packages.contains_key(id) {
            return Err(format!(
                "explicit target references unknown active package: {id}"
            ));
        }
    }
    if !analysis.packages.is_empty()
        || analysis.targets.keys().any(|id| id != "product")
        || !analysis.interface_impacts.is_empty()
    {
        return Err(
            "release only analyzes product; close package analysis with develop first".into(),
        );
    }
    let published = development::Ledger::parse(
        revision_payload(root, &capture.boundary, development::RECORD, runner)?.as_deref(),
    )?;
    ledger.extends(&published)?;
    let cumulative = &ledger.entries[published.entries.len()..];
    let mut affected = affected(cfg, &capture);
    affected.extend(cumulative.iter().map(|e| e.id.clone()));
    let program = analysis
        .product
        .as_ref()
        .ok_or("product analysis is required for a release")?;
    let old_version: SemVer = capture.product_version.parse()?;
    let program_level =
        program.version_level("product", &capture.fingerprint, old_version.major == 0)?;
    if cumulative.iter().any(|e| e.compatibility == "breaking")
        && program.compatibility != "breaking"
    {
        return Err("product: cumulative breaking package changes require explicit breaking analysis and bound acceptance".into());
    }
    let maximum = minimum_product_level(cfg, &capture, cumulative, old_version.major == 0);
    if program_level < maximum {
        return Err(
            "product analysis must cover cumulative package impact since last release".into(),
        );
    }
    if cfg
        .native
        .lifecycle
        .iter()
        .any(|t| t.base == capture.boundary && t.compatibility == "breaking")
        && program.compatibility != "breaking"
    {
        return Err("product: breaking lifecycle transition requires breaking analysis and explicit user acceptance".into());
    }
    let version_base = capture
        .release_history
        .as_ref()
        .map(|h| h.product_high_water.max(old_version))
        .unwrap_or(old_version);
    let suggested: SemVer =
        polyglot::bump_native(&product(version_base), program_level)?.parse()?;
    let new_version: SemVer = choose(
        &product(version_base),
        program_level,
        analysis.targets.get("product"),
        false,
    )?
    .parse()?;
    let versions = BTreeMap::<String, String>::new();
    let mut writes = BTreeMap::new();
    let mut evidence_scope = affected.clone();
    evidence_scope.extend(capture.release_packages.iter().cloned());
    evidence_scope.insert("product".into());
    analysis::validate_checks(root, cfg, &mut capture, &analysis, &evidence_scope)?;
    if !cfg.product_manifests.is_empty() {
        polyglot::add_writes(
            &mut writes,
            rust::render_product_mappings(
                &cfg.product_manifests,
                &capture.inventory,
                &mut |p| config::read_current(root, p),
                old_version,
                new_version,
            )?,
        )?;
    }
    if !cfg.native.product_packages.is_empty() {
        polyglot::add_writes(
            &mut writes,
            rust::render_product_packages(
                cfg.native
                    .adapters
                    .rust
                    .as_ref()
                    .ok_or("product packages require a Rust adapter")?,
                &cfg.native.product_packages,
                &capture.inventory,
                &mut |p| config::read_current(root, p),
                old_version,
                new_version,
            )?,
        )?;
    }
    let kind = if root.join("bridgeforge-manifest.json").is_file() {
        ReleaseKind::Factory
    } else {
        ReleaseKind::Business
    };
    let paths = meaningful
        .iter()
        .map(|c| c.path.clone())
        .collect::<Vec<_>>();
    let mut info = parse_commit_message(message)?;
    info.breaking = program.compatibility == "breaking";
    info.kind = if program_level == 2 {
        "feat"
    } else if program_level == 3 {
        "feat"
    } else {
        "chore"
    }
    .into();
    let mut log = String::from_utf8(render_changelog(root, new_version, &[info], &kind, &paths)?)
        .map_err(|e| e.to_string())?;
    let mut changes = Vec::new();
    let mut events = Vec::new();
    let mut all_packages = Vec::new();
    for (id, p) in &capture.current.packages {
        let after = versions.get(&p.manifest).unwrap_or(&p.version);
        let before = capture
            .baseline
            .packages
            .get(id)
            .map(|p| p.version.as_str());
        let reasons = cumulative
            .iter()
            .filter(|e| &e.id == id)
            .map(|e| e.reason.as_str())
            .collect::<Vec<_>>();
        let reason = if reasons.is_empty() {
            "no_package_version_change".into()
        } else {
            reasons.join("; ")
        };
        all_packages.push(json!({"id":id,"language":p.language,"manifest":p.manifest,"version_before":before,"version_after":after,"reason":reason}));
        if before != Some(after.as_str()) {
            changes.push(PackageChange {
                manifest: p.manifest.clone(),
                name: p.name.clone(),
                version_before: before.unwrap_or("unreleased").into(),
                version_after: after.clone(),
                reason,
                dependencies: vec![],
            });
        }
    }
    for id in capture
        .baseline
        .packages
        .keys()
        .chain(capture.current.packages.keys())
        .collect::<BTreeSet<_>>()
    {
        let before = capture.baseline.packages.get(id);
        let after = capture.current.packages.get(id);
        let operation = if before.is_none() {
            "add"
        } else if after.is_none() {
            "retire"
        } else if before.unwrap().name != after.unwrap().name {
            "rename"
        } else if before.unwrap().manifest != after.unwrap().manifest {
            "move"
        } else {
            "modify"
        };
        if operation != "modify" || affected.contains(id) {
            let p = after.or(before).unwrap();
            events.push(PackageEvent {
                id: id.clone(),
                language: format!("{:?}", p.language).to_lowercase(),
                operation: operation.into(),
                manifest_before: before.map(|p| p.manifest.clone()),
                manifest_after: after.map(|p| p.manifest.clone()),
                name_before: before.map(|p| p.name.clone()),
                name_after: after.map(|p| p.name.clone()),
                version_before: before.map(|p| p.version.clone()),
                version_after: after
                    .map(|p| versions.get(&p.manifest).unwrap_or(&p.version).clone()),
                related: cfg
                    .native
                    .lifecycle
                    .iter()
                    .filter(|t| {
                        t.base == capture.boundary
                            && t.from.iter().chain(&t.to).any(|p| p.id == *id)
                    })
                    .flat_map(|t| t.from.iter().chain(&t.to).map(|p| p.id.clone()))
                    .filter(|p| p != id.as_str())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            });
        }
    }
    let heading = format!("## [{new_version}]");
    let start = log
        .find(&heading)
        .ok_or("release changelog heading missing")?;
    let end = log[start + heading.len()..]
        .find("\n## [")
        .map(|i| start + heading.len() + i)
        .unwrap_or(log.len());
    let mut section = format!("\n### Release analysis\n\n- Product: {}\n", program.reason);
    for p in &all_packages {
        section.push_str(&format!(
            "- `{}`: {} -> {} ({})\n",
            p["id"].as_str().unwrap(),
            p["version_before"].as_str().unwrap_or("unreleased"),
            p["version_after"].as_str().unwrap(),
            p["reason"].as_str().unwrap()
        ));
    }
    for e in &events {
        if e.operation == "retire" {
            section.push_str(&format!("- `{}`: retired\n", e.id));
        }
    }
    log.insert_str(end, &section);
    writes.insert("VERSION".into(), format!("{new_version}\n").into_bytes());
    writes.insert("CHANGELOG.md".into(), log.into_bytes());
    for path in writes.keys() {
        capture
            .inputs
            .entry(root.join(path))
            .or_insert(config::read_current(root, path)?);
    }
    verify_release_inputs(&capture.inputs)?;
    development_inputs::verify(root, &capture, &capture.inputs)?;
    if let Some(environment) = &capture.environment_guard {
        environment.verify(root)?;
    }
    if independent::status(root, runner)? != capture.status
        || independent::inventory(root, runner)? != capture.inventory
        || history::git(root, runner, &["rev-parse", "HEAD"])?.trim() != capture.head
    {
        return Err("repository changed before release plan completion".into());
    }
    let details = json!({"schema_version":4,"fingerprint":capture.fingerprint,"validation_fingerprint":capture.validation_fingerprint,"python_environment_fingerprint":capture.environment_guard.as_ref().map(|e|e.fingerprint.clone()),"strategy":cfg.strategy,"suggested_version":suggested.to_string(),"version_after":new_version.to_string(),"product_reason":program.reason,"package_version_baseline":capture.boundary,"packages":all_packages,"required_checks":cfg.required_checks,"check_scopes":cfg.check_scopes,"compatibility_validation":"agent_evidence_verified_not_automatically_proven"});
    Ok(Some(FileReleasePlan {
        inputs: capture.inputs,
        build_directories: BuildDirectoryGuard::new(
            capture.build_directories,
            capture.normalization,
            capture.portable_guards,
        ),
        old_version,
        new_version,
        kind,
        writes: writes.into_iter().map(|(p, b)| (root.join(p), b)).collect(),
        package_changes: changes,
        package_events: events,
        lifecycle_events: cfg
            .native
            .lifecycle
            .iter()
            .filter(|t| t.base == capture.boundary)
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?,
        release_inventory: Some(capture.inventory),
        release_status: Some(capture.status),
        details: Some(details),
        environment: capture.environment_guard,
    }))
}

fn with_release_history<T>(
    root: &Path,
    runner: &dyn ProcessRunner,
    operation: impl FnOnce(&Config, &history_cache::Reader<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let cfg = load(root)?.ok_or("release requires current schema 4 configuration; use --setup-plan for first-use setup; legacy migrations are retired")?;
    if !cfg.current_model {
        return Err("release requires the current product-packages configuration".into());
    }
    let inspection = InspectionRunner(runner);
    let reader = history_cache::Reader::new(root, &inspection);
    let result = operation(&cfg, &reader)?;
    reader.verify_unchanged()?;
    Ok(result)
}

fn minimum_product_level(
    cfg: &Config,
    capture: &Capture,
    entries: &[development::Entry],
    unstable: bool,
) -> u8 {
    let breaking_level = if unstable { 2 } else { 3 };
    let packages = entries
        .iter()
        .map(|entry| {
            if entry.compatibility == "breaking" {
                breaking_level
            } else {
                entry.level
            }
        })
        .max()
        .unwrap_or(1)
        .max(1);
    if cfg
        .native
        .lifecycle
        .iter()
        .any(|t| t.base == capture.boundary && t.compatibility == "breaking")
    {
        packages.max(breaking_level)
    } else {
        packages
    }
}

pub(super) fn product_requirements(
    root: &Path,
    cfg: &Config,
    capture: &Capture,
    ledger: &development::Ledger,
    runner: &dyn ProcessRunner,
) -> Result<Value, String> {
    let published = development::Ledger::parse(
        revision_payload(root, &capture.boundary, development::RECORD, runner)?.as_deref(),
    )?;
    ledger.extends(&published)?;
    let entries = &ledger.entries[published.entries.len()..];
    let version: SemVer = capture.product_version.parse()?;
    let level = minimum_product_level(cfg, capture, entries, version.major == 0);
    let packages = entries.iter().filter(|e| e.compatibility == "breaking").map(|e| {
        json!({"id":e.id,"before":e.before,"after":e.after,"reason":e.reason,"acceptance":e.acceptance})
    }).collect::<Vec<_>>();
    let transitions = cfg
        .native
        .lifecycle
        .iter()
        .filter(|t| t.base == capture.boundary && t.compatibility == "breaking")
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let breaking = !packages.is_empty() || !transitions.is_empty();
    Ok(json!({
        "minimum_level":match level { 3 => "major", 2 => "minor", _ => "patch" },
        "minimum_version":polyglot::bump_native(&product(capture.release_history.as_ref().map(|h| h.product_high_water.max(version)).unwrap_or(version)), level)?,
        "published_version_high_water":capture.release_history.as_ref().map(|h| h.product_high_water.to_string()),
        "breaking_packages":packages,"breaking_transitions":transitions,
        "acceptance_required":breaking,"acceptance_scope":if breaking {vec!["product"]} else {vec![]},
        "acceptance_fingerprint":if breaking {Some(&capture.fingerprint)} else {None},
        "scope":"recorded-package-and-lifecycle-impact",
        "product_analysis_required":true
    }))
}

pub fn build(
    root: &Path,
    message: &str,
    request: &Request,
    runner: &dyn ProcessRunner,
) -> Result<Option<FileReleasePlan>, String> {
    let _profile = crate::release_metrics::span("release.plan.build");
    with_release_history(root, runner, |cfg, runner| {
        let mut capture = inputs::capture_release(root, &cfg, runner)?;
        let ledger = development::release_gate(root, &cfg, &mut capture, runner)?;
        let analysis = analysis::read(root, request, &mut capture)?;
        if capture.changes.iter().all(|c| c.role == "record")
            && analysis.as_ref().is_none_or(|a| a.targets.is_empty())
        {
            return Ok(None);
        }
        let analysis=analysis.ok_or("schema 4 requires final-difference analysis; run --release-preview and supply --release-analysis-file")?;
        finish(root, message, &cfg, capture, analysis, ledger, runner)
    })
}

pub fn preview(
    root: &Path,
    message: &str,
    request: &Request,
    runner: &dyn ProcessRunner,
) -> CommandOutcome {
    let _profile = crate::release_metrics::span("release.plan.preview");
    let result = with_release_history(root, runner, |cfg, runner| {
        let mut capture = inputs::capture_release(root, &cfg, runner)?;
        let ledger = development::release_gate(root, &cfg, &mut capture, runner)?;
        let analysis = analysis::read_preview(root, request, &mut capture)?;
        if capture.changes.iter().all(|c| c.role == "record")
            && analysis.as_ref().is_none_or(|a| a.targets.is_empty())
        {
            let mut value = analysis::requirements(&cfg, &capture, &ledger);
            value["product_constraints"] =
                product_requirements(root, &cfg, &capture, &ledger, runner)?;
            value["status"] = json!("nothing-to-release");
            value["package_changes"] = json!([]);
            return Ok(value);
        }
        let Some(analysis) = analysis else {
            let mut value = analysis::requirements(&cfg, &capture, &ledger);
            value["product_constraints"] =
                product_requirements(root, &cfg, &capture, &ledger, runner)?;
            return Ok(value);
        };
        let p = finish(root, message, &cfg, capture, analysis, ledger, runner)?
            .ok_or("release plan unexpectedly empty")?;
        Ok::<_, String>(
            json!({"schema":1,"status":"release-planned","version_before":p.old_version.to_string(),"version_after":p.new_version.to_string(),"version_strategy":cfg.strategy,"package_changes":p.package_changes,"package_events":p.package_events,"lifecycle_events":p.lifecycle_events,"details":p.details,"kind":p.kind,"writes":p.writes.keys().map(|p|p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\',"/")).collect::<Vec<_>>()}),
        )
    });
    match result {
        Ok(v) => CommandOutcome::with_receipt(v),
        Err(e) => CommandOutcome::blocked(format!("[schema4-release] {e}\n")),
    }
}
