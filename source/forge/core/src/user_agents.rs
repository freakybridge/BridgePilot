//! User instructions are a single factory-owned asset. The bootstrap owns the
//! shared durable transaction; this module validates its contract and stages bytes.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub const MANIFEST: &str = crate::product_manifest::FILE;
pub const SOURCE: &str = "templates/user/AGENTS.md";

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn normalized(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(std::str::from_utf8(bytes)
        .map_err(|e| e.to_string())?
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .into_bytes())
}

fn ordinary_path(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                #[cfg(windows)]
                let link = {
                    use std::os::windows::fs::MetadataExt;
                    meta.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let link = meta.file_type().is_symlink();
                if link {
                    return Err(format!(
                        "reparse/symlink path is not allowed: {}",
                        ancestor.display()
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

pub fn render_manifest(root: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(crate::source_projection::template_path(root, SOURCE)).map_err(|e| e.to_string())?;
    let value = json!({
        "schema_version": 1,
        "asset_id": "codex.user-agents",
        "source": crate::source_projection::template_relative(root, SOURCE),
        "target": "~/.codex/AGENTS.md",
        "ownership_strategy": "backup_then_replace",
        "sha256": format!("sha256:{}", hash(&normalized(&bytes)?))
    });
    let mut output = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
    output.push(b'\n');
    Ok(output)
}

pub fn stage(root: &Path, profile: &Path, operation: &str) -> Result<Value, String> {
    if !root.is_absolute() || !profile.is_absolute() || !profile.is_dir() {
        return Err("existing absolute product root and user profile are required".into());
    }
    if operation.len() != 32
        || !operation
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("invalid operation identifier".into());
    }
    let home = profile.join(".codex");
    let target = home.join("AGENTS.md");
    let staged = home.join(format!(".AGENTS.bridgeforge-stage-{operation}"));
    let backup = home.join(format!(".AGENTS.bridgeforge-backup-{operation}.md"));
    for path in [
        root.join(MANIFEST),
        crate::source_projection::template_path(root, SOURCE),
        target.clone(),
        staged.clone(),
        backup.clone(),
    ] {
        ordinary_path(&path)?;
    }
    if home.exists() && !home.is_dir() || target.exists() && !target.is_file() {
        return Err("user instruction target must be an ordinary file".into());
    }
    if staged.exists() || backup.exists() {
        return Err("user instruction transaction paths already exist".into());
    }
    let declared = crate::product_manifest::section(root, "user_instructions")?;
    let expected: Value =
        serde_json::from_slice(&render_manifest(root)?).map_err(|e| e.to_string())?;
    if declared != expected {
        return Err("user instruction manifest identity or source hash differs".into());
    }
    let desired = normalized(&fs::read(crate::source_projection::template_path(root, SOURCE)).map_err(|e| e.to_string())?)?;
    if declared["sha256"] != format!("sha256:{}", hash(&desired)) {
        return Err("user instruction source changed during planning".into());
    }
    let current = if target.exists() {
        Some(fs::read(&target).map_err(|e| e.to_string())?)
    } else {
        None
    };
    // Text line endings are equivalent; preserve original bytes when already current.
    let needs_swap = current
        .as_ref()
        .is_none_or(|bytes| normalized(bytes).ok().as_deref() != Some(desired.as_slice()));
    if needs_swap {
        fs::create_dir_all(&home).map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|e| e.to_string())?;
        if let Err(error) = file.write_all(&desired).and_then(|()| file.sync_all()) {
            let _ = fs::remove_file(&staged);
            return Err(error.to_string());
        }
    }
    Ok(json!({
        "kind": "user-agents", "asset_id": "codex.user-agents",
        "target": target, "stage": staged, "backup": backup,
        "had_original": current.is_some(), "needs_swap": needs_swap,
        "original_hash": current.as_ref().map(|bytes| hash(bytes)),
        "desired_hash": if needs_swap { hash(&desired) } else { hash(current.as_ref().unwrap()) },
        "status": if needs_swap { "staged" } else { "current" },
        "override_present": home.join("AGENTS.override.md").exists()
    }))
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_user_agents.rs"]
mod tests;
