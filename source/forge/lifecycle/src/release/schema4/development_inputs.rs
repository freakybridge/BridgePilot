//! Explicit global build inputs include ignored files and directory membership.
use super::*;

#[derive(Clone, Debug, Default)]
pub struct BuildDirectoryGuard {
    directories: BTreeMap<String, BTreeSet<String>>,
    normalization: Option<content_identity::Rules>,
    portable: Vec<portable_environment::Guard>,
}

impl BuildDirectoryGuard {
    pub(super) fn new(
        directories: BTreeMap<String, BTreeSet<String>>,
        normalization: Option<content_identity::Rules>,
        portable: Vec<portable_environment::Guard>,
    ) -> Self {
        Self {
            directories,
            normalization,
            portable,
        }
    }

    /// Keep membership pinned through plan reuse and the final release write boundary.
    pub fn verify(&self, root: &Path) -> Result<(), String> {
        for guard in &self.portable {
            guard.verify(root)?;
        }
        if let Some(rules) = &self.normalization {
            rules.verify(root, &crate::SystemProcessRunner)?;
        }
        for (prefix, expected) in &self.directories {
            if members(root, prefix)? != *expected {
                return Err(format!(
                    "global build directory membership changed before release apply: {prefix}"
                ));
            }
        }
        Ok(())
    }
}

fn members(root: &Path, prefix: &str) -> Result<BTreeSet<String>, String> {
    let path = safe_relative(root, prefix.trim_end_matches('/'))?;
    for ancestor in path.ancestors().filter(|p| p.exists()) {
        if crate::persistence::is_link_or_reparse(ancestor).map_err(|e| e.to_string())? {
            return Err(format!("build input traverses a link: {prefix}"));
        }
    }
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(e) => return Err(e.to_string()),
        Ok(metadata) if !metadata.is_dir() => {
            return Err(format!("build directory is not a directory: {prefix}"));
        }
        _ => {}
    }
    let mut paths = BTreeSet::new();
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if crate::persistence::is_link_or_reparse(entry.path()).map_err(|e| e.to_string())?
            || (!entry.file_type().is_dir() && !entry.file_type().is_file())
        {
            return Err(format!(
                "build directory contains a link or special file: {prefix}"
            ));
        }
        let mut relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if entry.file_type().is_dir() {
            relative.push('/');
        }
        paths.insert(relative);
    }
    Ok(paths)
}

pub(super) fn capture(
    root: &Path,
    cfg: &Config,
    snapshot: &mut Capture,
    legacy_files: &BTreeMap<String, Option<String>>,
    identity: Option<&content_identity::Identity<'_>>,
) -> Result<(BTreeMap<String, Option<String>>, bool), String> {
    let mut inputs = BTreeMap::new();
    let mut legacy_complete = true;
    for path in &cfg.build_inputs {
        let files = if path.ends_with('/') {
            // Older digests never proved ignored membership or empty directories.
            legacy_complete = false;
            let paths = members(root, path)?;
            inputs.insert(
                path.clone(),
                Some(digest(
                    &serde_json::to_vec(&paths).map_err(|e| e.to_string())?,
                )),
            );
            snapshot
                .build_directories
                .entry(path.clone())
                .or_insert(paths.clone());
            paths
                .into_iter()
                .filter(|p| !p.ends_with('/'))
                .collect::<Vec<_>>()
        } else {
            legacy_complete &= legacy_files.contains_key(path);
            vec![path.clone()]
        };
        for file in files {
            let bytes = config::read_current(root, &file)?;
            inputs.insert(
                file.clone(),
                match identity {
                    Some(i) => i.hash(&file, bytes.as_deref())?,
                    None => bytes.as_deref().map(digest),
                },
            );
            snapshot.inputs.entry(root.join(&file)).or_insert(bytes);
        }
    }
    Ok((inputs, legacy_complete))
}

pub(super) fn verify(
    root: &Path,
    snapshot: &Capture,
    expected: &BTreeMap<PathBuf, Option<Vec<u8>>>,
) -> Result<(), String> {
    for guard in &snapshot.portable_guards {
        guard.verify(root)?;
    }
    if let Some(rules) = &snapshot.normalization {
        rules.verify(root, &crate::SystemProcessRunner)?;
    }
    for (prefix, before) in &snapshot.build_directories {
        let mut allowed = before.clone();
        allowed.retain(|p| {
            p.ends_with('/') || expected.get(&root.join(p)).is_some_and(Option::is_some)
        });
        // Only explicitly expected candidate writes and checked artifacts may add
        // members. Arbitrary files produced by checks are still input drift.
        for (path, _) in expected.iter().filter(|(_, b)| b.is_some()) {
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            if !relative.starts_with(prefix) {
                continue;
            }
            allowed.insert(relative.clone());
            let mut parent = Path::new(&relative).parent();
            while let Some(path) = parent {
                let path_prefix = format!("{}/", path.to_string_lossy().replace('\\', "/"));
                if !path_prefix.starts_with(prefix) {
                    break;
                }
                allowed.insert(path_prefix);
                parent = path.parent();
            }
        }
        if members(root, prefix)? != allowed {
            return Err(format!(
                "global build directory membership changed during validation: {prefix}"
            ));
        }
    }
    Ok(())
}
