//! Bind an installed home or product checkout to a separately owned user archive.
use crate::{Result, store::Store};
use serde_json::{Value, json};
use std::path::Path;

pub fn configure(root: &Path, workspace: &Path) -> Result<Value> {
    bridgepilot_storage::store::ordinary(root)?;
    if !root.is_absolute()
        || (!root.join("bridgeforge-manifest.json").is_file()
            && crate::install::inspect(root).is_err())
    {
        return Err("explicit product repository root required".into());
    }
    let workspace = Store::open(workspace)?;
    workspace.read()?;
    let path = root.join(".runtime/config/bridgepilot.json");
    bridgepilot_storage::store::ordinary(&path)?;
    bridgeforge_core::persistence::atomic_write_json(
        &path,
        &json!({"schema":1,"workspace":workspace.workspace}),
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({"schema":1,"status":"configured","path":path}))
}
