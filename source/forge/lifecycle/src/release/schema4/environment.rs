//! Explicit Python validation environment identity. No Python/backend execution.
use super::*;
use std::io::Read;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PythonEnvironment {
    pub interpreter: String,
    pub runtime_roots: Vec<String>,
    pub dependency_roots: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct EnvironmentGuard {
    pub configuration: PythonEnvironment,
    pub fingerprint: String,
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|e| format!("cannot read validation environment {}: {e}", path.display()))?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let count = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("sha256:{:x}", hash.finalize()))
}

fn resolve(root: &Path, path: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("Python validation environment paths must be explicit".into());
    }
    let path = Path::new(path);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    path.canonicalize().map_err(|e| {
        format!(
            "Python validation environment path is unavailable: {}: {e}",
            path.display()
        )
    })
}

impl PythonEnvironment {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.interpreter.trim().is_empty()
            || self.runtime_roots.is_empty()
            || self.dependency_roots.is_empty()
            || self
                .runtime_roots
                .iter()
                .chain(&self.dependency_roots)
                .any(|p| p.trim().is_empty())
        {
            return Err(
                "python_environment requires interpreter, runtime_roots and dependency_roots"
                    .into(),
            );
        }
        Ok(())
    }
    pub(super) fn capture(&self, root: &Path) -> Result<EnvironmentGuard, String> {
        self.capture_with_portable(root).map(|(guard, _)| guard)
    }

    pub(super) fn capture_with_portable(
        &self,
        root: &Path,
    ) -> Result<(EnvironmentGuard, String), String> {
        self.validate()?;
        let interpreter = resolve(root, &self.interpreter)?;
        if !interpreter.is_file() {
            return Err("Python interpreter identity must be a file".into());
        }
        let mut facts = BTreeMap::from([(
            interpreter.to_string_lossy().into_owned(),
            hash_file(&interpreter)?,
        )]);
        let mut portable = BTreeMap::from([("interpreter".to_string(), hash_file(&interpreter)?)]);
        for (index, configured) in self
            .runtime_roots
            .iter()
            .chain(&self.dependency_roots)
            .enumerate()
        {
            let directory = resolve(root, configured)?;
            if !directory.is_dir() {
                return Err("Python runtime/dependency roots must be directories".into());
            }
            // Empty directories remain part of the declared environment identity.
            facts.insert(
                format!("directory:{}", directory.display()),
                "directory".into(),
            );
            portable.insert(format!("root:{index}"), "directory".into());
            for entry in walkdir::WalkDir::new(&directory)
                .follow_links(false)
                .into_iter()
            {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().is_symlink()
                    || crate::persistence::is_link_or_reparse(entry.path())
                        .map_err(|e| e.to_string())?
                {
                    return Err(format!(
                        "Python environment link requires an explicit real root: {}",
                        entry.path().display()
                    ));
                }
                if entry.file_type().is_file() {
                    let hash = hash_file(entry.path())?;
                    facts.insert(entry.path().to_string_lossy().into_owned(), hash.clone());
                    portable.insert(
                        format!(
                            "root:{index}/{}",
                            entry
                                .path()
                                .strip_prefix(&directory)
                                .map_err(|e| e.to_string())?
                                .to_string_lossy()
                                .replace('\\', "/")
                        ),
                        hash,
                    );
                } else if entry.file_type().is_dir() {
                    portable.insert(
                        format!(
                            "root:{index}/{}",
                            entry
                                .path()
                                .strip_prefix(&directory)
                                .map_err(|e| e.to_string())?
                                .to_string_lossy()
                                .replace('\\', "/")
                        ),
                        "directory".into(),
                    );
                }
            }
        }
        Ok((
            EnvironmentGuard {
                configuration: self.clone(),
                fingerprint: crate::manifest::canonical_sha(&json!(facts))?,
            },
            crate::manifest::canonical_sha(&json!(portable))?,
        ))
    }
}

impl EnvironmentGuard {
    pub fn verify(&self, root: &Path) -> Result<(), String> {
        if self.configuration.capture(root)?.fingerprint != self.fingerprint {
            return Err("Python validation environment changed; prior evidence is stale".into());
        }
        Ok(())
    }
}
