//! Platform selection is bound to native identity, never inferred from an unknown host.
use super::*;

pub(super) fn match_remote(url: &str, repository: &str, origin: Option<&str>) -> Result<(), Stop> {
    bridgeforge_core::native_release::validate_repository(repository, origin).map_err(decision)?;
    match origin {
        Some(origin) => gitlab::match_remote(url, repository, origin),
        None => github::match_remote(url, repository),
    }
}

pub(super) fn publish(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
) -> Result<(String, bool), Stop> {
    match plan.gitlab_origin.as_deref() {
        Some(origin) => gitlab::publish(root, runner, plan, commit, origin).map(|url| (url, false)),
        None => github::publish(root, runner, plan, commit),
    }
}
