//! Personal project orchestration. Forge remains independent of this crate.
pub mod cli;
pub mod codex;
pub mod delivery;
pub mod domain;
pub mod grouping;
pub mod handoff;
pub mod install;
mod owned;
mod repository;
pub mod review;
pub mod scheduler;
pub mod store;
pub mod sync;
pub mod wakeup;

pub type Result<T> = std::result::Result<T, String>;

pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub fn new_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let seed = format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    format!("{prefix}-{}", &digest(seed.as_bytes())[..24])
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
