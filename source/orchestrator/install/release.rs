//! BP native package adapter. Forge owns fixed-commit publication and transport.
use crate::{Result, install, versions};
use bridgeforge_core::native_release::{Asset, Plan, raw_sha};
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
    let output = bridgeforge_core::managed_paths::safe_join(&root, &relative, "BP release output")?;
    let version = fs::read_to_string(root.join("VERSION")).map_err(|e| e.to_string())?;
    if version.trim() != versions::BP_VERSION.trim() {
        return Err(
            "BP release driver differs from native VERSION; rebuild the current source".into(),
        );
    }
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let launcher = executable
        .parent()
        .ok_or("BP release executable parent missing")?
        .join(if cfg!(windows) {
            "bp-launcher.exe"
        } else {
            "bp-launcher"
        });
    install::bundle(&output, &executable, &launcher)?;
    let package: install::Package =
        serde_json::from_slice(&fs::read(output.join("package.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if package.schema != 2
        || package.assets.len() != 13
        || package.components.as_ref() != Some(&versions::compiled()?)
    {
        return Err("BP release requires the complete thirteen-asset product package".into());
    }
    let mut files = vec![output.join("package.json")];
    files.extend(package.assets.iter().map(|a| output.join(&a.source)));
    let assets: Result<Vec<Asset>> = files
        .into_iter()
        .map(|path| {
            crate::store::ordinary(&path)?;
            let bytes = fs::read(&path).map_err(|e| e.to_string())?;
            Ok(Asset {
                name: path
                    .file_name()
                    .and_then(|v| v.to_str())
                    .ok_or("non-UTF8 release artifact")?
                    .into(),
                path: path
                    .strip_prefix(&root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/"),
                sha256: raw_sha(&bytes),
                size: bytes.len() as u64,
            })
        })
        .collect();
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?;
    let heading = format!("## [BP {}]", package.version);
    let start = changelog
        .find(&heading)
        .ok_or("BP release notes are missing for the native version")?;
    let notes = &changelog[start..];
    let end = notes[heading.len()..]
        .find("\n## [BP ")
        .map(|i| i + heading.len())
        .unwrap_or(notes.len());
    let notes = notes[..end].trim();
    if notes.len() > 100_000 {
        return Err("BP release notes exceed the publication limit".into());
    }
    let components = package.components.as_ref().unwrap();
    let plan = Plan {
        schema: 1,
        product: "BridgePilot".into(),
        version: package.version.clone(),
        repository: "freakybridge/BridgePilot".into(),
        tag: format!("bp-v{}", package.version),
        title: format!("BridgePilot {} (Windows x64)", package.version),
        notes: format!(
            "{notes}\n\n组件：BF {}（source-reference）、orchestrator {}（embedded）。\n\n安装入口：对应标签源码中的 INSTALL.md；下载 package.json 和全部十三项 payload 文件，按安装说明校验后组装。仅包含产品文件，不包含个人档案、凭据、配置或 Memory。",
            components.forge.version, components.orchestrator.version
        ),
        assets: assets?,
    };
    plan.validate(&root)?;
    serde_json::to_value(plan).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "../../../scripts/tests/orchestrator/native_release.rs"]
mod tests;
