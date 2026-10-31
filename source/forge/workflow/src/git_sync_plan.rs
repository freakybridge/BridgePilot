//! Git write plans delegate factory materialization through an explicit adapter.
use crate::{ProcessRunner, release};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

pub(super) fn join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.contains('\\')
        || Path::new(relative)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("unsafe factory input path: {relative}"));
    }
    let path = root.join(relative);
    for ancestor in path.ancestors().filter(|p| p.exists()) {
        if crate::persistence::is_link_or_reparse(ancestor).map_err(|e| e.to_string())? {
            return Err(format!("factory input traverses a link: {relative}"));
        }
    }
    Ok(path)
}

pub(super) struct WritePlan {
    pub generated_reused: usize,
    pub generated_built: usize,
    pub writes: BTreeMap<PathBuf, Vec<u8>>,
    pub binaries: BTreeSet<PathBuf>,
    pub before: BTreeMap<PathBuf, Option<Vec<u8>>>,
    release_inputs: BTreeMap<PathBuf, Option<Vec<u8>>>,
    verify: Option<Box<dyn Fn(&Path) -> Result<(), String>>>,
}

impl WritePlan {
    pub fn prepare(
        root: &Path,
        writes: BTreeMap<PathBuf, Vec<u8>>,
        release_inputs: BTreeMap<PathBuf, Option<Vec<u8>>>,
        factory: bool,
        runner: &dyn ProcessRunner,
    ) -> Result<Self, String> {
        release::verify_release_inputs(&release_inputs)?;
        if factory {
            let p = runner.factory_transaction(root, writes, release_inputs.clone(), runner)?;
            return Ok(Self {
                generated_reused: p.generated_reused,
                generated_built: p.generated_built,
                writes: p.writes,
                binaries: p.binaries,
                before: p.before,
                release_inputs,
                verify: Some(p.verify),
            });
        }
        let before = writes
            .keys()
            .map(|p| Ok((p.clone(), read_optional(p)?)))
            .collect::<Result<_, String>>()?;
        Ok(Self {
            generated_reused: 0,
            generated_built: 0,
            writes,
            binaries: BTreeSet::new(),
            before,
            release_inputs,
            verify: None,
        })
    }

    pub fn verify_unchanged(&self, root: &Path) -> Result<(), String> {
        release::verify_release_inputs(&self.release_inputs)?;
        for (path, before) in &self.before {
            if &read_optional(path)? != before {
                return Err(format!(
                    "automatic target changed concurrently before apply: {}",
                    path.display()
                ));
            }
        }
        if let Some(verify) = &self.verify {
            verify(root)?;
        }
        Ok(())
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
