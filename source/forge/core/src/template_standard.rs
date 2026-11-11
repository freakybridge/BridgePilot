//! Generic distribution rules compiled from the factory Template. This contains
//! no project configuration, version registry, or closure ledger.
use serde_json::Value;

pub struct Asset {
    pub rule: Value,
    pub payload: Vec<u8>,
}

include!("template_assets.rs");

/// Relocations are generated from the owning product manifest, never a second registry.
pub fn skill_relocations() -> Vec<Value> {
    assets().into_iter().filter_map(|asset| asset.rule.get("relocation").cloned()).collect()
}
