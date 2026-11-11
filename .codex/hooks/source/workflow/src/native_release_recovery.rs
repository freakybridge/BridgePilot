//! Opt-in, addition-only identity recovery, including hash-only v1 continuations.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence {
    previous_attempt: String,
    before_config_sha256: String,
    after_config_sha256: String,
    removed_keys_to_reconstruct_before: Vec<String>,
    source_fingerprint: String,
}

// Only an exact, credential-free HTTP(S) origin for a different host is eligible.
// Paths, wildcards, userinfo and same-host port/scheme variations remain protected.
fn unrelated_provider(key: &str, value: &str, plan: &Plan) -> bool {
    let Some(origin) = key.strip_prefix("credential.").and_then(|s| s.strip_suffix(".provider")) else {
        return false;
    };
    let target = plan.gitlab_origin.as_deref().unwrap_or("https://github.com");
    fn host(url: &str) -> Option<&str> {
        url.split_once("://").map(|(_, s)| s.split(':').next().unwrap_or(""))
    }
    value == "generic"
        && bridgeforge_core::native_release::validate_gitlab_origin(origin).is_ok()
        && host(origin).zip(host(target)).is_some_and(|(a, b)| !a.eq_ignore_ascii_case(b))
}

pub(super) fn reconstruct(config: &str, before: &str, plan: &Plan) -> Result<Vec<String>, Stop> {
    // Keep byte order, duplicates, NUL terminators and the legacy trim convention.
    // A digest match proves that *all* other effective configuration is identical.
    let entries: Vec<_> = config.split_inclusive('\0').collect();
    let mut eligible = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some((key, value)) = entry.trim_end_matches('\0').split_once('\n') else { continue; };
        let identity = ["user.name", "user.email"].contains(&key);
        if (identity && !value.trim().is_empty() && !value.contains(['\r', '\n']))
            || unrelated_provider(key, value, plan) {
            eligible.push((index, key, identity));
        }
    }
    // Bound subset reconstruction; never turn missing legacy evidence into acceptance.
    if eligible.is_empty() || eligible.len() > 12 {
        return Err(conflict("identity recovery has insufficient or excessive configuration evidence"));
    }
    for mask in 1usize..(1 << eligible.len()) {
        if !eligible.iter().enumerate().any(|(bit, (_, _, identity))| *identity && mask & (1 << bit) != 0) {
            continue;
        }
        let selected: BTreeSet<_> = eligible.iter().enumerate()
            .filter(|(bit, _)| mask & (1 << bit) != 0).map(|(_, (index, _, _))| *index).collect();
        let reconstructed: String = entries.iter().enumerate().filter(|(index, _)| !selected.contains(index))
            .map(|(_, entry)| *entry).collect();
        if raw_sha(reconstructed.trim().as_bytes()) == before {
            return Ok(eligible.iter().enumerate().filter(|(bit, _)| mask & (1 << bit) != 0)
                .map(|(_, (_, key, _))| (*key).into()).collect());
        }
    }
    Err(conflict("identity recovery cannot reconstruct the exact previous Git configuration; critical or unproven drift preserved"))
}

pub(super) fn recover(
    root: &Path,
    runner: &dyn ProcessRunner,
    path: &Path,
    expected: &mut Option<Vec<u8>>,
    old: &mut Option<State>,
    current: &Anchor,
) -> Result<(), Stop> {
    let Some(prior) = old.as_ref() else { return Err(conflict("identity recovery requires an existing release continuation")); };
    let before = prior.anchor.as_ref().ok_or_else(|| conflict("identity recovery requires a recorded Git anchor"))?;
    if before.config_sha256 == current.config_sha256 { return Ok(()); }
    if prior.commit.is_some() || prior.result.is_some() || prior.status != "blocked-before-commit" {
        return Err(conflict("identity recovery requires a verified blocked-before-commit attempt; fixed or uncertain progress preserved"));
    }
    let mut target = before.clone();
    target.config_sha256 = current.config_sha256.clone();
    if &target != current {
        return Err(conflict("identity recovery Git HEAD, branch or push target changed"));
    }
    if serde_json::to_value(&prior.driver).unwrap() != serde_json::to_value(driver(root)?).unwrap() {
        return Err(conflict("identity recovery native driver changed"));
    }
    platform::match_remote(&current.url, &prior.plan.repository, prior.plan.gitlab_origin.as_deref())?;
    prior.plan.validate(root).map_err(conflict)?;
    source::require_finished_recovery(root)?;
    let guard = git_sync::inspect_commit_guard(root, runner).map_err(conflict)?
        .filter(|g| g.before_commit && g.parent == current.head)
        .ok_or_else(|| conflict("identity recovery lacks verified before-commit guard"))?;
    git(root, runner, &["diff", "--cached", "--quiet", "HEAD", "--"])
        .map_err(|_| conflict("identity recovery requires an unchanged, empty staging area"))?;
    // Resolve identity through Git as commit will, without changing config or invoking hooks.
    git(root, runner, &["var", "GIT_AUTHOR_IDENT"])?;
    git(root, runner, &["var", "GIT_COMMITTER_IDENT"])?;
    let config = git(root, runner, &["config", "--null", "--list"])?;
    if raw_sha(config.as_bytes()) != current.config_sha256 {
        return Err(conflict("Git configuration changed during identity recovery"));
    }
    let removed = reconstruct(&config, &before.config_sha256, &prior.plan)?;
    let input = fingerprint(root, runner)?;
    let bytes = expected.as_deref().ok_or_else(|| conflict("identity recovery state missing"))?;
    let archived = archive_attempt(root, bytes, Some(&guard))?;
    let mut recovered = prior.clone();
    recovered.identity_recovery = Some(Evidence {
        previous_attempt: archived.clone(),
        before_config_sha256: before.config_sha256.clone(),
        after_config_sha256: current.config_sha256.clone(),
        removed_keys_to_reconstruct_before: removed,
        source_fingerprint: input.clone(),
    });
    recovered.previous_attempt = Some(archived);
    recovered.anchor = Some(current.clone());
    // Archive-first, CAS-save-last: interruption leaves the old attempt usable. A changed
    // source still goes through the existing reprepare/adapter candidate-selection path.
    if anchor(root, runner)? != *current || fingerprint(root, runner)? != input
        || git_sync::inspect_commit_guard(root, runner).map_err(conflict)?.is_none_or(|g| g.bytes != guard.bytes) {
        return Err(conflict("identity recovery checkpoint changed concurrently; original attempt preserved"));
    }
    git(root, runner, &["diff", "--cached", "--quiet", "HEAD", "--"])
        .map_err(|_| conflict("staging area changed during identity recovery"))?;
    save(path, &recovered, expected)?;
    *old = Some(recovered);
    Ok(())
}
