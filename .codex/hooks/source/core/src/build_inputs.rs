use crate::manifest::{
    canonical_sha, generated_build_recipe, generated_source_sha, generated_sources,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct BuildInputs {
    original: PathBuf,
    pub snapshot: PathBuf,
    pub target: Option<PathBuf>,
    files: BTreeMap<String, Vec<u8>>,
    pub hashes: Value,
    projection: Option<String>,
}

fn read_files(root: &Path, projection: Option<&str>) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if let Some(mode) = projection {
        return crate::source_projection::projected(root, mode);
    }
    generated_sources(root)?
        .into_iter()
        .map(|relative| {
            let payload = fs::read(root.join(&relative)).map_err(|error| error.to_string())?;
            Ok((relative, payload))
        })
        .collect()
}

fn lock_sha(payload: &[u8]) -> String {
    let normalized = String::from_utf8_lossy(payload)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    format!("sha256:{:x}", Sha256::digest(normalized.as_bytes()))
}

impl BuildInputs {
    pub fn capture(original: &Path, snapshot: PathBuf, item: &Value) -> Result<Self, String> {
        Self::capture_mode(original, snapshot, item, None)
    }

    /// Caller retains the project artifact-cache lock until all build outputs
    /// have been captured. The slot's exact file inventory proves ownership.
    pub fn capture_persistent(
        original: &Path,
        slot: &Path,
        item: &Value,
        environment: &str,
    ) -> Result<Self, String> {
        Self::capture_mode(original, slot.join("source"), item, Some(environment))
    }

    fn capture_mode(
        original: &Path,
        snapshot: PathBuf,
        item: &Value,
        environment: Option<&str>,
    ) -> Result<Self, String> {
        let _profile = crate::release_metrics::span("build_inputs.capture");
        if item["manifest"] != "Cargo.toml" || item["lockfile"] != "Cargo.lock" {
            return Err("generated build requires workspace Cargo.toml and Cargo.lock".into());
        }
        let binary = item["build"]["binary_name"]
            .as_str()
            .ok_or("missing binary_name")?;
        if !matches!(
            binary,
            "bridgeforge" | "bridgeforge-hook" | "bridgeforge-factory"
        ) {
            return Err("unsupported generated binary_name".into());
        }
        let recipe = generated_build_recipe(binary);
        if item["build"] != recipe {
            return Err("generated build recipe does not match executed Cargo command".into());
        }
        let args = item["self_test"]["args"]
            .as_array()
            .ok_or("missing self-test args")?;
        if args.iter().any(|arg| !arg.is_string())
            || !item["self_test"]["expected_json"].is_object()
        {
            return Err("invalid generated self-test contract".into());
        }
        let projection = item
            .get("source_projection")
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or("source_projection must be a string")
            })
            .transpose()?;
        let files = read_files(original, projection.as_deref())?;
        if let Some(environment) = environment {
            sync_slot(&snapshot, &files, environment)?;
        }
        fs::create_dir_all(&snapshot).map_err(|error| error.to_string())?;
        for (relative, payload) in &files {
            let path = snapshot.join(relative);
            fs::create_dir_all(path.parent().ok_or("snapshot parent is missing")?)
                .map_err(|error| error.to_string())?;
            if fs::read(&path).ok().as_deref() != Some(payload) {
                fs::write(path, payload).map_err(|error| error.to_string())?;
            }
        }
        let sources = generated_sources(&snapshot)?;
        let hashes = json!({
            "source_tree_sha256": generated_source_sha(&snapshot, &sources)?,
            "lockfile_sha256": lock_sha(files.get("Cargo.lock").ok_or("missing Cargo.lock")?),
            "build_recipe_sha256": canonical_sha(&recipe)?,
            "self_test_sha256": canonical_sha(&item["self_test"])?,
        });
        for (key, actual) in hashes.as_object().ok_or("invalid input hashes")? {
            if &item[key] != actual {
                return Err(format!("generated build input hash mismatch: {key}"));
            }
        }
        let inputs = Self {
            original: original.to_path_buf(),
            target: environment.map(|_| snapshot.parent().unwrap().join("target")),
            snapshot,
            files,
            hashes,
            projection,
        };
        inputs.verify_unchanged()?;
        Ok(inputs)
    }

    pub fn verify_unchanged(&self) -> Result<(), String> {
        self.verify_original_unchanged()?;
        if self.target.is_some()
            && inventory(&self.snapshot)?
                != self
                    .files
                    .iter()
                    .map(|(p, b)| (p.clone(), raw_hash(b)))
                    .collect()
        {
            return Err("persistent Cargo source inventory changed during build".into());
        }
        if read_files(&self.snapshot, self.projection.as_deref())? != self.files {
            return Err("generated isolated build inputs changed during build".into());
        }
        Ok(())
    }

    /// A caller may share a captured workspace, never substitute a snapshot
    /// captured from another source root or projection.
    pub fn verify_source(&self, original: &Path, projection: Option<&str>) -> Result<(), String> {
        if self.original != original || self.projection.as_deref() != projection {
            return Err("captured build inputs belong to a different source or projection".into());
        }
        self.verify_unchanged()
    }

    pub fn verify_original_unchanged(&self) -> Result<(), String> {
        if read_files(&self.original, self.projection.as_deref())? != self.files {
            return Err("generated source inputs changed during build".into());
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Slot {
    schema: u32,
    owner: String,
    environment: String,
    files: BTreeMap<String, String>,
}

fn raw_hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn inventory(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut files = BTreeMap::new();
    if !root.exists() {
        return Ok(files);
    }
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if crate::persistence::is_link_or_reparse(entry.path()).map_err(|e| e.to_string())? {
            return Err("persistent Cargo workspace contains a linked path".into());
        }
        if entry.file_type().is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(
                relative,
                raw_hash(&fs::read(entry.path()).map_err(|e| e.to_string())?),
            );
        } else if !entry.file_type().is_dir() {
            return Err("unsupported persistent Cargo entry".into());
        }
    }
    Ok(files)
}

fn sync_slot(
    snapshot: &Path,
    files: &BTreeMap<String, Vec<u8>>,
    environment: &str,
) -> Result<(), String> {
    sync_slot_with(snapshot, files, environment, &mut |path, bytes| {
        crate::persistence::atomic_write(path, bytes).map_err(|e| e.to_string())
    })
}

fn sync_slot_with(
    snapshot: &Path,
    files: &BTreeMap<String, Vec<u8>>,
    environment: &str,
    write: &mut dyn FnMut(&Path, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    let slot = snapshot.parent().ok_or("Cargo slot parent missing")?;
    crate::managed_paths::safe_join(slot, "source", "persistent Cargo workspace")?;
    let metadata = slot.join("inputs.json");
    let old = crate::managed_paths::read_metadata(&metadata, 16 * 1024 * 1024)?;
    let before = inventory(snapshot)?;
    let previous = old
        .as_deref()
        .map(|bytes| {
            serde_json::from_value::<Slot>(crate::json::parse_unique_json(
                bytes,
                "Cargo slot ownership",
            )?)
            .map_err(|e| e.to_string())
        })
        .transpose()?;
    if let Some(previous) = &previous {
        if previous.schema != 1
            || previous.owner != "bridgeforge-cargo-slot-v1"
            || previous.files != before
        {
            return Err(
                "persistent Cargo source ownership or bytes drifted; preserve the slot".into(),
            );
        }
    } else if !before.is_empty() || slot.join("target").exists() {
        return Err("persistent Cargo slot is unowned; preserve it".into());
    }
    if slot.exists() {
        for entry in fs::read_dir(slot).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !matches!(
                entry.file_name().to_str(),
                Some("source" | "target" | "inputs.json")
            ) || crate::persistence::is_link_or_reparse(&entry.path())
                .map_err(|e| e.to_string())?
            {
                return Err("unknown persistent Cargo slot entry; preserve it".into());
            }
        }
    }
    if slot.join("target").exists() {
        let target = slot.join("target");
        // Scan without following reparse points before retiring owned compiler
        // outputs. Source files are independently protected by the inventory.
        for entry in walkdir::WalkDir::new(&target).follow_links(false) {
            let entry = entry.map_err(|e| e.to_string())?;
            if crate::persistence::is_link_or_reparse(entry.path()).map_err(|e| e.to_string())? {
                return Err("Cargo target contains a linked path".into());
            }
        }
        if previous
            .as_ref()
            .is_some_and(|p| p.environment != environment)
        {
            fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
        }
    }
    let next = Slot {
        schema: 1,
        owner: "bridgeforge-cargo-slot-v1".into(),
        environment: environment.into(),
        files: files
            .iter()
            .map(|(p, b)| (p.clone(), raw_hash(b)))
            .collect(),
    };
    let bytes = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
    let mut changes = Vec::new();
    for relative in before
        .keys()
        .chain(files.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        if before.get(relative) == files.get(relative).map(|b| raw_hash(b)).as_ref() {
            continue;
        }
        let path = crate::managed_paths::safe_join(snapshot, relative, "Cargo source")?;
        let original = crate::managed_paths::read_metadata(&path, 64 * 1024 * 1024)?;
        changes.push((path, original, files.get(relative).cloned()));
    }
    if old.as_deref() != Some(bytes.as_slice()) {
        changes.push((metadata, old, Some(bytes)));
    }
    let mut applied = Vec::new();
    for (path, original, next) in changes {
        let result = (|| {
            if let Some(bytes) = &next {
                fs::create_dir_all(path.parent().ok_or("Cargo source parent missing")?)
                    .map_err(|e| e.to_string())?;
                write(&path, bytes)
            } else {
                fs::remove_file(&path).map_err(|e| e.to_string())
            }
        })();
        // Include the failing operation: atomic publication can succeed before
        // a later durability error. Restore only bytes still owned by this run.
        applied.push((path, original, next));
        if let Err(error) = result {
            let mut failures = Vec::new();
            for (path, original, expected) in applied.into_iter().rev() {
                let current = crate::managed_paths::read_metadata(&path, 64 * 1024 * 1024);
                match current {
                    Ok(current) if current == original => {}
                    Ok(current) if current == expected => {
                        let restored = match original {
                            Some(bytes) => crate::persistence::atomic_write(&path, &bytes)
                                .map_err(|e| e.to_string()),
                            None => fs::remove_file(&path).map_err(|e| e.to_string()),
                        };
                        if let Err(e) = restored {
                            failures.push(e);
                        }
                    }
                    _ => failures.push(format!("preserved concurrent change: {}", path.display())),
                }
            }
            return Err(format!(
                "Cargo source synchronization failed: {error}; rollback: {}",
                if failures.is_empty() {
                    "complete".into()
                } else {
                    failures.join("; ")
                }
            ));
        }
    }
    Ok(())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_build_inputs.rs"]
mod tests;
