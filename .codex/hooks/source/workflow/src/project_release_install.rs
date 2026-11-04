//! Installation-side deployment of a real release driver; release never repairs it.
use crate::project_release::{self, OWNER, PROGRAM, RECEIPT, Recipe};
use bridgeforge_core::{managed_paths, native_release::raw_sha};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub struct Installation {
    pub before: BTreeMap<PathBuf, Option<Vec<u8>>>,
    pub writes: BTreeMap<PathBuf, Vec<u8>>,
    pub recipe: Option<Recipe>,
    pub gaps: Vec<String>,
    pub report: Value,
}
fn read(root: &Path, relative: &str) -> Result<Option<Vec<u8>>, String> {
    crate::project_sync::transaction_file_state(&managed_paths::safe_join(
        root,
        relative,
        "release installation",
    )?)
}

pub fn inspect(root: &Path, requested: Option<&Recipe>) -> Result<Installation, String> {
    let mut before = BTreeMap::new();
    for relative in ["Cargo.toml", PROGRAM, RECEIPT] {
        before.insert(root.join(relative), read(root, relative)?);
    }
    let existing = project_release::recipe(root)?;
    if let (Some(old), Some(new)) = (&existing, requested) {
        if old != new {
            return Err("project release recipe already exists; preserve project decisions rather than silently replacing them".into());
        }
    }
    let recipe = existing.or_else(|| requested.cloned());
    let mut writes = BTreeMap::new();
    let mut gaps = vec![];
    let mut target = Value::Null;
    let binary = before[&root.join(PROGRAM)].as_deref();
    let receipt = before[&root.join(RECEIPT)].as_deref();
    if binary.is_some() || receipt.is_some() {
        let owned = receipt
            .and_then(|b| crate::json::parse_unique_json(b, "release installation receipt").ok())
            .is_some_and(|v| {
                v["schema"] == 1
                    && v["owner"] == OWNER
                    && binary.is_none_or(|b| v["binary_sha256"] == raw_sha(b))
            });
        if !owned {
            return Err("unowned or modified release driver preserved; resolve ownership before installation".into());
        }
    }
    if let Some(r) = &recipe {
        match crate::native_release::installation_target(
            root,
            &r.repository,
            r.gitlab_origin.as_deref(),
        ) {
            Ok(value) => target = value,
            Err(error) => gaps.push(error),
        }
        let manifest = project_release::manifest(root, r)?;
        if before[&root.join("Cargo.toml")].as_deref() != Some(manifest.as_slice()) {
            writes.insert(root.join("Cargo.toml"), manifest);
        }
        for rel in [&r.version_file, &r.changelog] {
            before.insert(root.join(rel), read(root, rel)?);
        }
        // Bind project build entry scripts without running a business build.
        for argv in &r.build {
            for arg in argv {
                if !arg.starts_with('-') && arg.replace('\\', "/").starts_with("scripts/") {
                    let rel = arg.replace('\\', "/");
                    let bytes = read(root, &rel)?
                        .ok_or_else(|| format!("native build entry missing: {rel}"))?;
                    before.insert(root.join(rel), Some(bytes));
                }
            }
        }
    } else {
        let custom = before[&root.join("Cargo.toml")]
            .as_ref()
            .is_some_and(|b| String::from_utf8_lossy(b).contains("native-release"));
        gaps.push(if custom {"existing custom release adapter is preserved; its installation needs an explicit verified project recipe"}
            else {"native release installation needs a project-owned recipe; supply --release-spec during installation, not during release"}.into());
    }
    let report = json!({"status":if gaps.is_empty(){"planned"}else{"needs_configuration"},"program":PROGRAM,
        "recipe_source":"Cargo.toml:workspace.metadata.bridgeforge.project-release","shared_driver":true,
        "platform":recipe.as_ref().map(|r|if r.gitlab_origin.is_some(){"gitlab"}else{"github"}),"target":target,"gaps":gaps});
    Ok(Installation {
        before,
        writes,
        recipe,
        gaps,
        report,
    })
}

pub fn payloads(recipe: &Recipe, cli: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let identity = bridgeforge_core::build_identity::require()?;
    let proof = json!({"schema":1,"owner":OWNER,"binary_sha256":raw_sha(cli),"build_identity":identity,
        "recipe_sha256":raw_sha(&serde_json::to_vec(recipe).map_err(|e|e.to_string())?)});
    Ok(BTreeMap::from([
        (PROGRAM.into(), cli.to_vec()),
        (
            RECEIPT.into(),
            serde_json::to_vec_pretty(&proof).map_err(|e| e.to_string())?,
        ),
    ]))
}
