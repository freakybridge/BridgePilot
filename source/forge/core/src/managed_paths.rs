//! Managed asset path policy, shared by planning and isolated generated builds.
//! Other path policies remain with their owners; do not broaden this policy here.
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// Bounded metadata reads must not follow a leaf reparse point or allocate from
/// an untrusted file length. Missing is distinct from unreadable/invalid.
pub fn read_metadata(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, String> {
    reject_links(path)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!(
            "metadata is not a bounded regular file: {}",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit
        || crate::persistence::is_link_or_reparse(path).map_err(|e| e.to_string())?
    {
        return Err("metadata changed during capture".into());
    }
    Ok(Some(bytes))
}

pub fn safe_join(root: &Path, raw: &str, label: &str) -> Result<PathBuf, String> {
    if raw.is_empty() || raw.contains('\\') || raw.contains(['*', '?', '[']) {
        return Err(format!("{label} is unsafe: {raw}"));
    }
    let relative = Path::new(raw);
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("{label} is unsafe: {raw}"));
    }
    let target = root.join(relative);
    reject_links(&target).map_err(|error| format!("{label}: {error}"))?;
    Ok(target)
}

fn reject_links(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_)
                if crate::persistence::is_link_or_reparse(ancestor)
                    .map_err(|e| e.to_string())? =>
            {
                return Err(format!("path traverses a linked path: {}", path.display()));
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
