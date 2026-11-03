//! Task execution, native protocol, supervision and verified delivery.
pub use bridgepilot_storage::{database, store};
pub use bridgepilot_tasks::{Result, digest, domain, new_id, valid_id};
pub mod codex;
pub mod commands;
pub mod delivery;
pub mod grouping;
mod owned;
pub mod repository;
pub mod review;
pub mod scheduler;
pub mod status;
pub mod tracking;
pub mod wakeup;

/// Native readback needed by explicit machine handoff.
pub mod handoff_probe {
    pub use crate::codex::{pid_exists, read_idle_thread};
    pub use crate::owned::{image_name, same_process};
}

static PROGRAM_VERSION: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// Set the composed executable identity before opening native RPC connections.
pub fn set_program_version(version: &'static str) -> Result<()> {
    let actual = PROGRAM_VERSION.get_or_init(|| version);
    if *actual != version {
        return Err("BP executable version already initialized differently".into());
    }
    Ok(())
}

pub(crate) fn program_version() -> &'static str {
    PROGRAM_VERSION
        .get()
        .copied()
        .unwrap_or(include_str!("../../../VERSION").trim())
}
