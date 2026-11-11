//! Recoverable source edits proposed by a project-owned native adapter.
use super::*;
use bridgeforge_core::native_release::{SourceContext, SourceIdentity, SourcePlan};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transaction {
    schema: u32,
    anchor: Anchor,
    outside: String,
    plan: SourcePlan,
    finished: bool,
}

fn path(root: &Path) -> PathBuf { root.join(".runtime/bridgeforge-codex/native-release/source.json") }

pub(super) fn require_finished_recovery(root: &Path) -> Result<(), Stop> {
    if let Some(bytes) = state_bytes(&path(root))? {
        let tx: Transaction = serde_json::from_slice(&bytes).map_err(|e| conflict(e.to_string()))?;
        if tx.schema != 1 || !tx.finished {
            return Err(conflict("identity recovery cannot reanchor an unfinished source transaction"));
        }
    }
    Ok(())
}

fn paths(plan: &SourcePlan) -> Result<BTreeSet<String>, Stop> {
    let allowed: BTreeSet<_> = plan.identity.paths.iter().cloned().collect();
    if plan.schema != 1 || allowed.len() != plan.identity.paths.len() {
        return Err(conflict("invalid source preparation protocol"));
    }
    let mut changed = BTreeSet::new();
    for edit in &plan.edits {
        if !allowed.contains(&edit.path) || !changed.insert(edit.path.clone())
            || edit.path.starts_with('.') || edit.path.contains('\\') || edit.path.split('/').any(|p| p == ".." || p.is_empty()) {
            return Err(conflict("source adapter proposed an unowned or unsafe edit"));
        }
    }
    Ok(changed)
}

fn save_transaction(root: &Path, transaction: &Transaction, expected: &Option<Vec<u8>>) -> Result<(), Stop> {
    unchanged_state(&path(root), expected)?;
    let bytes = serde_json::to_vec_pretty(transaction).map_err(|e| condition(e.to_string()))?;
    persistence::atomic_write(&path(root), &bytes).map_err(|e| condition(e.to_string()))
}

fn apply(root: &Path, runner: &dyn ProcessRunner, transaction: &Transaction) -> Result<(), Stop> {
    let excluded = paths(&transaction.plan)?;
    if transaction.schema != 1 || anchor(root, runner)? != transaction.anchor
        || fingerprint_without(root, runner, &excluded)? != transaction.outside {
        return Err(conflict("source preparation checkpoint changed; transaction preserved"));
    }
    // Check every file before the first write. Partial writes can only finish the same candidate.
    for edit in &transaction.plan.edits {
        let target = bridgeforge_core::managed_paths::safe_join(root, &edit.path, "native source edit").map_err(conflict)?;
        let bytes = fs::read(&target).map_err(|e| condition(e.to_string()))?;
        if bytes != edit.before.as_bytes() && bytes != edit.after.as_bytes() {
            return Err(conflict("source version file changed externally; transaction preserved"));
        }
    }
    for edit in &transaction.plan.edits {
        let target = bridgeforge_core::managed_paths::safe_join(root, &edit.path, "native source edit").map_err(conflict)?;
        let bytes = fs::read(&target).map_err(|e| condition(e.to_string()))?;
        if bytes != edit.before.as_bytes() && bytes != edit.after.as_bytes() {
            return Err(conflict("source version file changed before write; external edit preserved"));
        }
        if bytes != edit.after.as_bytes() {
            persistence::atomic_write(&target, edit.after.as_bytes()).map_err(|e| condition(e.to_string()))?;
        }
    }
    for edit in &transaction.plan.edits {
        let target = bridgeforge_core::managed_paths::safe_join(root, &edit.path, "native source edit").map_err(conflict)?;
        if fs::read(target).map_err(|e| condition(e.to_string()))? != edit.after.as_bytes() {
            return Err(conflict("source version file changed after write; transaction preserved"));
        }
    }
    if anchor(root, runner)? != transaction.anchor || fingerprint_without(root, runner, &excluded)? != transaction.outside {
        return Err(conflict("source changed while applying version preparation"));
    }
    Ok(())
}

pub(super) fn recover(root: &Path, runner: &dyn ProcessRunner) -> Result<(), Stop> {
    if let Some(bytes) = state_bytes(&path(root))? {
        let mut tx: Transaction = serde_json::from_slice(&bytes).map_err(|e| conflict(e.to_string()))?;
        if !tx.finished {
            let excluded = paths(&tx.plan)?;
            let outside = fingerprint_without(root, runner, &excluded)?;
            let complete = tx.plan.edits.iter().all(|edit| {
                bridgeforge_core::managed_paths::safe_join(root, &edit.path, "native source edit").ok()
                    .and_then(|path| fs::read(path).ok()).is_some_and(|bytes| bytes == edit.after.as_bytes())
            });
            if tx.schema == 1 && anchor(root, runner)? == tx.anchor && complete && outside != tx.outside {
                // Fully applied, still before any commit: retain the candidate version and
                // reclassify corrected inputs. Never relax partial writes or fixed commits.
                let archived = root.join(format!(".runtime/bridgeforge-codex/native-release/source-{}.json", &raw_sha(&bytes)[7..]));
                if !archived.exists() { persistence::atomic_write(&archived, &bytes).map_err(|e| condition(e.to_string()))?; }
                tx.finished = true;
                save_transaction(root, &tx, &Some(bytes))?;
            } else { apply(root, runner, &tx)?; }
        }
    }
    Ok(())
}

pub(super) fn finish(root: &Path, runner: &dyn ProcessRunner) -> Result<(), Stop> {
    let bytes = state_bytes(&path(root))?;
    if let Some(bytes) = &bytes {
        let mut tx: Transaction = serde_json::from_slice(bytes).map_err(|e| conflict(e.to_string()))?;
        if !tx.finished {
            apply(root, runner, &tx)?;
            tx.finished = true;
            save_transaction(root, &tx, &Some(bytes.clone()))?;
        }
    }
    Ok(())
}

pub(super) fn can_replace_obsolete_attempt(root: &Path, runner: &dyn ProcessRunner, old: &State, current: &Anchor) -> Result<bool, Stop> {
    if old.status != "pushed" || old.result.is_some() || old.reuse_release.is_some()
        || driver(root)?.source_args.is_empty() { return Ok(false); }
    let (Some(commit), Some(before)) = (&old.commit, &old.anchor) else { return Ok(false); };
    if !before.same_target(current) { return Ok(false); }
    platform::match_remote(&current.url, &old.plan.repository, old.plan.gitlab_origin.as_deref())?;
    if git(root, runner, &["merge-base", commit, &current.head])? != *commit { return Ok(false); }
    platform::obsolete_publication(root, runner, &old.plan, commit)
}

pub(super) fn prepare(root: &Path, runner: &dyn ProcessRunner, driver: &Driver, options: &git_sync::GitSyncOptions) -> Result<(bool, SourcePlan), Stop> {
    let previous = state_bytes(&path(root))?;
    if let Some(bytes) = &previous {
        let tx: Transaction = serde_json::from_slice(bytes).map_err(|e| conflict(e.to_string()))?;
        if !tx.finished { apply(root, runner, &tx)?; return Ok((true, tx.plan)); }
    }
    let before = fingerprint(root, runner)?;
    let checkpoint = anchor(root, runner)?;
    let binary = bridgeforge_core::managed_paths::safe_join(root, &driver.program, "source adapter").map_err(conflict)?;
    let mut argv = vec![binary.to_string_lossy().into_owned()];
    argv.extend(driver.source_args.clone());
    argv.extend(["--project-root".into(), root.to_string_lossy().into_owned()]);
    let identity: SourceIdentity = serde_json::from_str(&request(root, runner, &argv, 120)?).map_err(|e| condition(e.to_string()))?;
    if identity.product.is_empty() || identity.tag_prefix.is_empty() || !bridgeforge_core::native_release::identifier(&format!("{}{}", identity.tag_prefix, identity.version)) {
        return Err(decision("invalid source adapter identity"));
    }
    identity.version.parse::<bridgeforge_core::SemVer>().map_err(decision)?;
    platform::match_remote(&checkpoint.url, &identity.repository, identity.gitlab_origin.as_deref())?;
    let baseline = platform::baseline(root, runner, &identity)?;
    let message = match &options.message_file {
        Some(path) => Some(fs::read_to_string(path).map_err(|e| condition(e.to_string()))?),
        None => options.message.clone(),
    };
    let context = SourceContext { schema: 1, identity: identity.clone(), baseline, message };
    let context_path = root.join(".runtime/bridgeforge-codex/native-release/source-input.json");
    persistence::atomic_write(&context_path, &serde_json::to_vec(&context).map_err(|e| condition(e.to_string()))?).map_err(|e| condition(e.to_string()))?;
    argv.extend(["--input".into(), context_path.to_string_lossy().into_owned()]);
    let plan: SourcePlan = serde_json::from_str(&request(root, runner, &argv, 120)?).map_err(|e| condition(e.to_string()))?;
    if plan.identity.product != identity.product || plan.identity.repository != identity.repository
        || plan.identity.gitlab_origin != identity.gitlab_origin
        || plan.identity.tag_prefix != identity.tag_prefix || plan.identity.paths != identity.paths {
        return Err(conflict("source adapter changed its declared ownership or product"));
    }
    plan.identity.version.parse::<bridgeforge_core::SemVer>().map_err(decision)?;
    let excluded = paths(&plan)?;
    if fingerprint(root, runner)? != before || anchor(root, runner)? != checkpoint {
        return Err(conflict("read-only source adapter changed source or Git checkpoint"));
    }
    if let Some(published) = &plan.reuse_release {
        if !plan.edits.is_empty() || plan.identity.version != published.version
            || context.baseline.as_ref().is_none_or(|baseline| baseline.version != published.version || baseline.commit != published.commit)
            || plan.release_assets.is_empty() {
            return Err(conflict("invalid existing publication selection"));
        }
        return Ok((false, plan));
    }
    let unchanged = plan.edits.is_empty() && git(root, runner, &["status", "--porcelain"])?.is_empty();
    platform::source_available(root, runner, &plan.identity, unchanged)?;
    if plan.edits.is_empty() { return Ok((false, plan)); }
    let selected = plan.clone();
    let tx = Transaction { schema: 1, anchor: checkpoint,
        outside: fingerprint_without(root, runner, &excluded)?, plan, finished: false };
    save_transaction(root, &tx, &previous)?;
    apply(root, runner, &tx)?;
    Ok((true, selected))
}
