//! Exact retirement of previously distributed source files, never directory ownership.
use super::*;

pub(super) fn plan(
    root: &Path,
    contract: &Value,
    safe: &mut Vec<SyncAction>,
    deletes: &mut Vec<PathBuf>,
    gaps: &mut Vec<String>,
    reads: &mut BTreeMap<String, Option<Vec<u8>>>,
    entries: &mut Vec<Value>,
) -> Result<(), String> {
    for asset in contract["retired_assets"].as_array().into_iter().flatten() {
        let target = asset["target"]
            .as_str()
            .ok_or("retired source target missing")?;
        let path = safe_join(root, target, "retired source target")?;
        let before = transaction_file_state(&path)?;
        reads.insert(target.into(), before.clone());
        let Some(bytes) = before else {
            continue;
        };
        let hash = sha_git(&bytes);
        let known = asset["known_hashes"].as_array().is_some_and(|hashes| {
            hashes
                .iter()
                .any(|expected| expected.as_str() == Some(hash.as_str()))
        });
        entries.push(
            json!({"id":asset["id"],"target":target,"kind":"retired-runtime-source",
            "before_sha256":hash,"disposition":if known {"delete"} else {"required-preserve"}}),
        );
        if !known {
            gaps.push(format!(
                "retired runtime source has unknown content; preserved: {target}"
            ));
            continue;
        }
        safe.push(SyncAction {
            id: format!(
                "retired-source:{}",
                asset["id"].as_str().unwrap_or_default()
            ),
            target: target.into(),
            operation: "delete".into(),
            risk: false,
            before_sha256: Some(hash),
            after_sha256: "sha256:deleted".into(),
        });
        deletes.push(path);
    }
    Ok(())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/source_retirement.rs"]
mod tests;
