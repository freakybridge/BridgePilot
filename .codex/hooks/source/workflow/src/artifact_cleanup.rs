//! The old cleanup policy depended on retired project sidecars. Until an owning
//! native producer proves current outputs, fail before inspecting or deleting.
use crate::{CommandOutcome, ProcessRunner};
use serde_json::Value;
use std::path::Path;

pub fn command(args: &[String], runner: &dyn ProcessRunner) -> CommandOutcome {
    command_with_contract(args, runner, &|_, value| Ok(value.clone()))
}
pub fn command_with_contract(
    _args: &[String], _runner: &dyn ProcessRunner,
    _contract: &dyn Fn(&Path, &Value) -> Result<Value, String>,
) -> CommandOutcome {
    CommandOutcome::blocked("[cleanup] BLOCKED: configuration-based cleanup is retired; no files inspected or deleted; use the project's verified native cleanup procedure\n")
}
