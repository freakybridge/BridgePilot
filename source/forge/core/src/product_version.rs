//! Forge product identity comes from its native CLI manifest, not BP's VERSION.
use std::{fs, path::Path};

pub fn explicit(manifest: &str) -> Result<String, String> {
    let manifest = manifest.parse::<toml_edit::DocumentMut>().map_err(|e| e.to_string())?;
    let version = manifest.get("package").and_then(|p| p.get("version"))
        .and_then(toml_edit::Item::as_str).ok_or("Forge CLI must own an explicit native version")?;
    version.parse::<crate::SemVer>()?;
    Ok(version.to_owned())
}

pub fn forge(root: &Path) -> Result<String, String> {
    let relative = crate::source_projection::physical_path(root, "source/cli/Cargo.toml");
    let path = root.join(relative);
    if path.is_file() {
        let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let manifest = text.parse::<toml_edit::DocumentMut>().map_err(|e| e.to_string())?;
        let native = manifest.get("package").and_then(|p| p.get("version")).ok_or("Forge CLI version missing")?;
        if native.as_str().is_some() { return explicit(&text); }
        if native.get("workspace").and_then(toml_edit::Item::as_bool) != Some(true) {
            return Err("Forge CLI version missing".into());
        }
        // Legacy official sources inherited the Forge version from the workspace.
        let workspace = fs::read_to_string(root.join("Cargo.toml")).map_err(|e| e.to_string())?
            .parse::<toml_edit::DocumentMut>().map_err(|e| e.to_string())?;
        let version = workspace.get("workspace").and_then(|w| w.get("package")).and_then(|p| p.get("version")).and_then(toml_edit::Item::as_str)
            .ok_or("legacy Forge workspace version missing")?;
        version.parse::<crate::SemVer>()?;
        return Ok(version.to_owned());
    }
    // Frozen legacy product fixtures/installations predate native source projection.
    if crate::source_projection::available(root) {
        return Err("Forge CLI manifest missing from product source".into());
    }
    let version = fs::read_to_string(root.join("VERSION")).map_err(|e| e.to_string())?;
    version.trim().parse::<crate::SemVer>()?;
    Ok(version.trim().to_owned())
}
