pub use bridgeforge_core::{
    CommandOutcome, EXIT_BLOCKED, EXIT_OK, ProcessOutput, ProcessRequest, ProcessRunner,
    ProjectContext, SystemProcessRunner,
};
pub use bridgeforge_core::{
    artifact_cache, build_inputs, build_metrics, build_receipt, file_lock, generated_assets, json,
    managed_markdown, managed_paths, persistence, release_metrics, runtime, source_projection,
    user_agents,
};
pub use bridgeforge_lifecycle::release;
pub use bridgeforge_skeleton::{baseline, project_hooks};
pub use bridgeforge_workflow::{git_sync, project_sync};
pub mod commands;
mod dogfood;
pub mod factory_runtime;
pub mod factory_version;
pub mod git_sync_plan;
pub mod manifest;
pub mod runner;

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/factory/public_artifact_cache.rs"]
mod public_artifact_cache_tests;
