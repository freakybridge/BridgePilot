use super::*;

/// History is replayed only to audit identities and native version continuity.
/// Severity always comes from the final difference, never accumulated commits.
pub(super) fn validate(
    root: &Path,
    cfg: &Config,
    baseline: &polyglot::Snapshot,
    current: &polyglot::Snapshot,
    head: &str,
    boundary: &str,
    runner: &dyn ProcessRunner,
) -> Result<BTreeMap<String, NativePackage>, String> {
    let adopted = adoption::verify(root, cfg, head, runner)?;
    let mut observed_adoptions = revision_payload(root, boundary, CONFIG, runner)?
        .map(|bytes| parse(&bytes))
        .transpose()?
        .flatten()
        .map(|prior| prior.package_adoptions)
        .unwrap_or_default();
    let prior = baseline
        .config
        .as_ref()
        .map(|c| c.lifecycle.as_slice())
        .unwrap_or(&[]);
    if cfg.native.lifecycle.len() < prior.len() || cfg.native.lifecycle[..prior.len()] != *prior {
        return Err("published lifecycle history must be preserved verbatim".into());
    }
    let transitions = cfg
        .native
        .lifecycle
        .iter()
        .filter(|t| t.base == boundary)
        .cloned()
        .collect::<Vec<_>>();
    if cfg
        .native
        .lifecycle
        .iter()
        .any(|t| t.base != boundary && !prior.iter().any(|p| p.id == t.id))
    {
        return Err("new lifecycle event must bind the current release baseline".into());
    }
    let mut consumed = BTreeSet::new();
    let mut previous = baseline.packages.clone();
    let mut retired = cfg.baseline.retired_ids.clone();
    for transition in prior {
        for p in &transition.from {
            if !transition.to.iter().any(|n| n.id == p.id) {
                retired.insert(p.id.clone());
            }
        }
    }
    let mut versions = previous
        .iter()
        .map(|(id, p)| (id.clone(), p.version.clone()))
        .collect::<BTreeMap<_, _>>();
    for (id, package) in &adopted {
        versions
            .entry(id.clone())
            .or_insert_with(|| package.version.clone());
    }
    let mut steps = Vec::new();
    let mut closure_history = development::Ledger::parse(
        revision_payload(root, boundary, development::RECORD, runner)?.as_deref(),
    )?;
    let start = if boundary == cfg.baseline.revision {
        for state in &cfg.baseline.lineage {
            let ledger = development::Ledger::parse(
                revision_payload(root, &state.revision, development::RECORD, runner)?.as_deref(),
            )?;
            steps.push((state.packages.clone(), ledger));
        }
        cfg.baseline.migration_head.as_str()
    } else {
        boundary
    };
    let commits = history::git(
        root,
        runner,
        &[
            "rev-list",
            "--reverse",
            "--first-parent",
            &format!("{start}..{head}"),
        ],
    )?;
    for commit in commits.lines().filter(|s| !s.is_empty()) {
        // Every post-migration revision must carry the new contract. The old
        // parser is never re-entered by daily publishing.
        let bytes = revision_payload(root, commit, CONFIG, runner)?
            .ok_or("post-migration revision has no release configuration")?;
        let committed = parse(&bytes)?.ok_or("post-migration revision must use schema 4")?;
        adoption::preserve(&observed_adoptions, &committed.package_adoptions)?;
        observed_adoptions = committed.package_adoptions.clone();
        if cfg.current_model && !committed.current_model {
            return Err("post-cutover revision must use product-packages".into());
        }
        if serde_json::to_value(&committed.baseline).map_err(|e| e.to_string())?
            != serde_json::to_value(&cfg.baseline).map_err(|e| e.to_string())?
        {
            if boundary != cfg.baseline.revision {
                return Err("committed schema 4 migration baseline cannot be changed".into());
            }
            migration::verify_correction(root, &committed, cfg, runner)?;
        }
        let ledger = development::Ledger::parse(
            revision_payload(root, commit, development::RECORD, runner)?.as_deref(),
        )?;
        steps.push((
            polyglot::historical(root, commit, &cfg.native, runner)?.packages,
            ledger,
        ));
    }
    adoption::preserve(&observed_adoptions, &cfg.package_adoptions)?;
    steps.push((current.packages.clone(), development::Ledger::load(root)?));
    for (next, ledger) in steps {
        ledger.extends(&closure_history)?;
        // Identity transitions retain the original lifecycle validator. Version
        // continuity is checked separately against the append-only closure chain.
        let mut identities = next.clone();
        for (id, p) in &mut identities {
            if let Some(old) = previous.get(id) {
                p.version = old.version.clone();
            }
        }
        lifecycle::validate_step(&previous, &identities, &transitions, &mut consumed)?;
        for id in previous.keys().filter(|id| !next.contains_key(*id)) {
            retired.insert(id.clone());
        }
        for (id, p) in &next {
            if retired.contains(id) {
                return Err(format!("{id}: retired identity cannot be reused"));
            }
            if let Some(version) = versions.get(id) {
                if version != &p.version {
                    let entries = ledger.entries[closure_history.entries.len()..]
                        .iter()
                        .filter(|e| &e.id == id)
                        .collect::<Vec<_>>();
                    if entries.first().and_then(|e| e.before.as_ref()) != Some(version)
                        || entries.last().and_then(|e| e.after.as_ref()) != Some(&p.version)
                    {
                        return Err(format!(
                            "{id}: native version changed outside managed development closure"
                        ));
                    }
                }
            } else {
                versions.insert(id.clone(), p.version.clone());
            }
            versions.insert(id.clone(), p.version.clone());
        }
        previous = next;
        closure_history = ledger;
    }
    if transitions.iter().any(|t| !consumed.contains(&t.id)) {
        return Err("lifecycle declaration has no matching observed transition".into());
    }
    Ok(adopted)
}
