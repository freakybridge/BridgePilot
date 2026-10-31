//! Retirement of the old generated runtime layout. Every deletion is a precise
//! file with receipt-bound bytes; callers incorporate these into their existing
//! snapshot, drift-check and rollback transaction.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub fn legacy_build_outputs(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    let mut files = crate::artifact_cache::legacy_files(root)?;
    let directory = crate::managed_paths::safe_join(
        root,
        ".runtime/bridgeforge-codex/git-sync-images",
        "legacy images",
    )?;
    if directory.exists() {
        for item in fs::read_dir(&directory).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            let name = item.file_name().to_string_lossy().into_owned();
            let parts = name
                .strip_suffix(".exe")
                .unwrap_or("")
                .split('-')
                .collect::<Vec<_>>();
            if parts.len() != 3
                || parts[0].len() != 64
                || !parts[0].bytes().all(|b| b.is_ascii_hexdigit())
                || !parts[1..]
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            {
                return Err("unknown legacy runtime image; preserve it".into());
            }
            let bytes = crate::managed_paths::read_metadata(&item.path(), 512 * 1024 * 1024)?
                .ok_or("legacy image disappeared")?;
            if format!("{:x}", Sha256::digest(&bytes)) != parts[0] {
                return Err("legacy runtime image bytes drifted".into());
            }
            files.insert(item.path(), bytes);
        }
    }
    Ok(files)
}

pub fn relocated_build_output(root: &Path, old: &Path) -> Result<PathBuf, String> {
    let relative = old
        .strip_prefix(root.join(".runtime/bridgeforge-codex"))
        .map_err(|e| e.to_string())?;
    crate::managed_paths::safe_join(
        root,
        &format!(
            ".build/bridgeforge/legacy/{}",
            relative.to_string_lossy().replace('\\', "/")
        ),
        "legacy build relocation",
    )
}

pub fn legacy_runtime(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    legacy_runtime_with_owned(root, &BTreeMap::new())
}

pub fn legacy_test_outputs(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    let directory = crate::managed_paths::safe_join(
        root,
        crate::artifact_paths::LEGACY_TEST_RECEIPTS,
        "legacy test receipts",
    )?;
    let mut files = BTreeMap::new();
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.is_dir() => (),
        Ok(_) => return Err("legacy test output root is not a directory; preserve it".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(error) => return Err(error.to_string()),
    }
    let sample_name = regex::Regex::new(r"^payload_samples/\d{4}-\d{2}-\d{2}T\d{6}\.json$")
        .map_err(|e| e.to_string())?;
    let mut entries = walkdir::WalkDir::new(&directory)
        .follow_links(false)
        .into_iter();
    while let Some(item) = entries.next() {
        let item = item.map_err(|e| e.to_string())?;
        if crate::persistence::is_link_or_reparse(item.path()).map_err(|e| e.to_string())? {
            return Err("legacy test output contains a link".into());
        }
        let relative = item
            .path()
            .strip_prefix(&directory)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if item.file_type().is_dir() {
            if relative == "receipts.jsonl" || sample_name.is_match(&relative) {
                return Err("legacy test receipt is not a regular file; preserve it".into());
            }
            if !matches!(relative.as_str(), "" | "payload_samples") {
                // Project report directories are opaque: do not inspect their
                // contents or infer ownership from names inside them.
                entries.skip_current_dir();
            }
            continue;
        }
        if !item.file_type().is_file() {
            return Err("legacy test output is not a regular file; preserve it".into());
        }
        if relative != "receipts.jsonl" && !sample_name.is_match(&relative) {
            // Only the legacy record and fixed event-sample names are managed.
            // Other reports stay in place without parsing or reading their bytes.
            continue;
        }
        let bytes = crate::managed_paths::read_metadata(item.path(), 64 * 1024 * 1024)?
            .ok_or("legacy test output disappeared")?;
        if relative == "receipts.jsonl" {
            for line in std::str::from_utf8(&bytes)
                .map_err(|e| e.to_string())?
                .lines()
                .filter(|s| !s.is_empty())
            {
                let value = crate::json::parse_unique_json(line.as_bytes(), "legacy test receipt")?;
                if !value["cmd"].is_string()
                    || !value["kind"].is_string()
                    || !value["ts"].is_string()
                    || !value["sha1"]
                        .as_str()
                        .is_some_and(|s| s.len() == 12 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                {
                    return Err("unknown legacy test receipt format; preserve it".into());
                }
            }
        } else if sample_name.is_match(&relative) {
            crate::json::parse_unique_json(&bytes, "legacy test payload sample")?;
        }
        files.insert(item.path().to_path_buf(), bytes);
    }
    Ok(files)
}

pub fn relocated_test_output(root: &Path, old: &Path) -> Result<PathBuf, String> {
    let relative = old
        .strip_prefix(root.join(crate::artifact_paths::LEGACY_TEST_RECEIPTS))
        .map_err(|e| e.to_string())?;
    crate::managed_paths::safe_join(
        root,
        &format!(
            ".runtime/tests/legacy-test-receipts/{}",
            relative.to_string_lossy().replace('\\', "/")
        ),
        "legacy test relocation",
    )
}

/// Additional project-owned files must be proven by their owning subsystem.
pub fn legacy_runtime_with_owned(
    root: &Path,
    additional: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    let directory = root.join(".codex/bin");
    for ancestor in directory.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_)
                if crate::persistence::is_link_or_reparse(ancestor)
                    .map_err(|e| e.to_string())? =>
            {
                return Err("legacy runtime traverses a link; preserve it".into());
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.is_dir() => (),
        Ok(_) => return Err("legacy runtime is not a directory; preserve it".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e.to_string()),
    }
    let profiles: Vec<Value> = serde_json::from_str(include_str!("legacy_artifact_receipts.json"))
        .map_err(|e| e.to_string())?;
    let mut owned = additional.clone();
    let mut recognized = BTreeSet::new();
    for (path, expected) in additional {
        if path.parent() != Some(directory.as_path())
            || crate::managed_paths::read_metadata(path, 512 * 1024 * 1024)?.as_ref()
                != Some(expected)
        {
            return Err("project-owned legacy runtime changed".into());
        }
        recognized.insert(
            path.file_name()
                .ok_or("legacy file name missing")?
                .to_string_lossy()
                .into_owned(),
        );
    }
    for (name, id, receipt) in [
        (
            "bridgeforge",
            "codex.bridgeforge-cli",
            "build-receipt-cli.json",
        ),
        ("bridgeforge-hook", "codex.hooks", "build-receipt-hook.json"),
        (
            "bridgeforge-factory",
            "codex.factory-cli",
            "build-receipt-factory.json",
        ),
    ] {
        let binary_name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        let binary_path = directory.join(&binary_name);
        let receipt_path = directory.join(receipt);
        let binary = crate::managed_paths::read_metadata(&binary_path, 512 * 1024 * 1024)?;
        let document = crate::managed_paths::read_metadata(&receipt_path, 128 * 1024)?;
        if binary.is_none() && document.is_none() {
            continue;
        }
        let (binary, document) = binary
            .zip(document)
            .ok_or_else(|| format!("legacy runtime pair is incomplete: {name}; preserve it"))?;
        let record = crate::json::parse_unique_json(&document, "legacy runtime receipt")?;
        let inputs = [
            "source_tree_sha256",
            "lockfile_sha256",
            "build_recipe_sha256",
            "self_test_sha256",
        ];
        if record["schema_version"] != 2
            || record["generated_asset_id"] != id
            || record["platform"] != crate::build_receipt::platform_key()?
            || record["binary_sha256"] != format!("sha256:{:x}", Sha256::digest(&binary))
            || !profiles
                .iter()
                .any(|p| p["id"] == id && inputs.iter().all(|field| p[field] == record[field]))
        {
            return Err(format!(
                "legacy runtime ownership is unknown: {name}; preserve it"
            ));
        }
        recognized.extend([binary_name, receipt.into()]);
        owned.insert(binary_path, binary);
        owned.insert(receipt_path, document);
    }
    let mut preserved = false;
    for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file()
            || crate::persistence::is_link_or_reparse(&entry.path()).map_err(|e| e.to_string())?
        {
            return Err(format!(
                "unsafe legacy runtime entry preserved: {}",
                entry.path().display()
            ));
        }
        let name = entry.file_name();
        if name != ".gitignore" && !recognized.contains(&name.to_string_lossy().to_string()) {
            // Unowned regular files are opaque project data, not retirements.
            preserved = true;
        }
    }
    // Keep the directory's ignore policy whenever project files remain. Their
    // contents and receipts are outside the public runtime ownership contract.
    if !preserved
        && let Some(bytes) =
            crate::managed_paths::read_metadata(&directory.join(".gitignore"), 4096)?
    {
        if String::from_utf8_lossy(&bytes).replace("\r\n", "\n") != "*\n!.gitignore\n" {
            return Err("legacy binary ignore file has external changes; preserve it".into());
        }
        owned.insert(directory.join(".gitignore"), bytes);
    }
    Ok(owned)
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_artifact_migration.rs"]
mod tests;
