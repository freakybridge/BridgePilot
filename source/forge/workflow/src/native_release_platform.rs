//! Platform selection is bound to native identity, never inferred from an unknown host.
use super::*;
use bridgeforge_core::native_release::{PublishedVersion, SourceIdentity, SourcePlan};

pub(super) fn match_remote(url: &str, repository: &str, origin: Option<&str>) -> Result<(), Stop> {
    bridgeforge_core::native_release::validate_repository(repository, origin).map_err(decision)?;
    match origin {
        Some(origin) => gitlab::match_remote(url, repository, origin),
        None => github::match_remote(url, repository),
    }
}

macro_rules! dispatch {
    ($name:ident($($arg:ident: $ty:ty),*) -> $result:ty, $origin:expr) => {
        pub(super) fn $name($($arg: $ty),*) -> $result {
            if $origin.is_some() { gitlab::$name($($arg),*) }
            else { github::$name($($arg),*) }
        }
    };
}
dispatch!(baseline(root: &Path, runner: &dyn ProcessRunner, identity: &SourceIdentity) -> Result<Option<PublishedVersion>, Stop>, identity.gitlab_origin);
dispatch!(source_available(root: &Path, runner: &dyn ProcessRunner, identity: &SourceIdentity, unchanged: bool) -> Result<(), Stop>, identity.gitlab_origin);
dispatch!(existing_plan(root: &Path, runner: &dyn ProcessRunner, selected: &SourcePlan) -> Result<Plan, Stop>, selected.identity.gitlab_origin);
dispatch!(obsolete_publication(root: &Path, runner: &dyn ProcessRunner, plan: &Plan, commit: &str) -> Result<bool, Stop>, plan.gitlab_origin);
dispatch!(precheck(root: &Path, runner: &dyn ProcessRunner, plan: &Plan, head: &str, dirty: bool) -> Result<(), Stop>, plan.gitlab_origin);
dispatch!(verify_existing(root: &Path, runner: &dyn ProcessRunner, plan: &Plan, commit: &str) -> Result<Value, Stop>, plan.gitlab_origin);
dispatch!(publish(root: &Path, runner: &dyn ProcessRunner, plan: &Plan, commit: &str) -> Result<Value, Stop>, plan.gitlab_origin);
