//! Private archive synchronization and explicit machine control transfer.
pub use bridgepilot_storage::{database, store};
pub use bridgepilot_tasks::{Result, digest, domain, new_id, valid_id};
pub mod handoff;
pub mod projects;
pub mod sync;
