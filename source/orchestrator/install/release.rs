//! BP release notes and tag for GitHub's source archives.
use crate::{Result, versions};
use bridgeforge_core::native_release::{Plan, ProductVersion};
use serde_json::Value;
use std::{fs, path::Path};

pub fn prepare(root: &Path, output: &Path) -> Result<Value> {
    let supplied_root = std::path::absolute(root).map_err(|e| e.to_string())?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    crate::store::ordinary(output)?;
    if !output.is_absolute() {
        return Err("BP release output must be an explicit project .build directory".into());
    }
    let relative = output
        .strip_prefix(&root)
        .or_else(|_| output.strip_prefix(&supplied_root))
        .map_err(|_| "BP release output must stay inside the project")?;
    let relative = relative
        .to_str()
        .ok_or("BP release path must be UTF-8")?
        .replace('\\', "/");
    if !relative.starts_with(".build/") {
        return Err("BP release output must stay inside .build".into());
    }
    bridgeforge_core::managed_paths::safe_join(&root, &relative, "BP release output")?;
    let version = fs::read_to_string(root.join("VERSION")).map_err(|e| e.to_string())?;
    if version.trim() != versions::BP_VERSION.trim() {
        return Err(
            "BP release driver differs from native VERSION; rebuild the current source".into(),
        );
    }
    let version = version.trim();
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?;
    let notes = version.parse::<ProductVersion>()?.notes(&changelog, "BP ")?;
    let components = versions::compiled()?;
    let notes = format!(
        "{notes}\n\n组件：BF {}（source-reference）、orchestrator {}（embedded）。\n\n从本页的 Source code 下载标签源码，按源码根目录 INSTALL.md 中的“从标签源码构建 BP”编译。",
        components.forge.version, components.orchestrator.version
    );
    if notes.len() > 100_000 {
        return Err("BP release notes exceed the publication limit".into());
    }
    let plan = Plan {
        schema: 1,
        product: "BridgePilot".into(),
        version: version.into(),
        repository: "freakybridge/BridgePilot".into(),
        gitlab_origin: None,
        tag: format!("bp-v{version}"),
        title: format!("BridgePilot {version}"),
        notes,
        assets: Vec::new(),
    };
    serde_json::to_value(plan).map_err(|e| e.to_string())
}
