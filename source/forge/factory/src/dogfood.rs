//! Explicit factory-only mirrors. These targets never enter the public contract.
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contract {
    schema: u32,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asset {
    id: String,
    source: String,
    target: String,
    strategy: String,
}

pub(crate) fn render(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let contract: Contract =
        serde_json::from_str(include_str!("dogfood.json")).map_err(|e| e.to_string())?;
    if contract.schema != 1 || contract.assets.is_empty() {
        return Err("invalid factory mirror contract".into());
    }
    let mut ids = BTreeSet::new();
    let mut targets = BTreeSet::new();
    let mut outputs = BTreeMap::new();
    for asset in contract.assets {
        if asset.strategy != "whole"
            || !asset.id.starts_with("factory.skill.")
            || !asset
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
            || !ids.insert(asset.id)
            || !targets.insert(asset.target.to_ascii_lowercase())
            || asset.source.strip_prefix("source/forge/templates/skills/").is_none()
            || asset.target.strip_prefix(".codex/skills/")
                != asset.source.strip_prefix("source/forge/templates/skills/")
        {
            return Err("unsafe or duplicated factory mirror asset".into());
        }
        let source = crate::managed_paths::safe_join(root, &asset.source, "factory mirror source")?;
        crate::managed_paths::safe_join(root, &asset.target, "factory mirror target")?;
        let bytes = crate::managed_paths::read_metadata(&source, 1024 * 1024)?
            .ok_or("factory mirror source missing")?;
        outputs.insert(asset.target, bytes);
    }
    Ok(outputs)
}
