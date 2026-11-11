//! Template-only verification. Never loads a project-side contract or registry.
use bridgeforge_core::{ProcessRequest, ProcessRunner, build_identity, build_manifest,
    build_receipt, managed_paths, template_standard};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

fn factory(root: &Path) -> Result<bool, String> {
    if !root.join("source/forge/factory/Cargo.toml").is_file() { return Ok(false); }
    verify_factory_identity(root, &build_identity::require()?)?;
    Ok(true)
}

/// Either official executable may verify the factory, against its own projection.
pub fn verify_factory_identity(root: &Path, identity: &Value) -> Result<(), String> {
    for public in [true, false] {
        let source = bridgeforge_core::source_projection::render(root, public)?;
        if identity["source_tree_sha256"] == bridgeforge_core::source_projection::tree_sha(&source)?
            && identity["lockfile_sha256"] == build_manifest::normalized_hash(source.get("Cargo.lock").ok_or("factory projection lock missing")?) {
            return Ok(());
        }
    }
    Err("factory source differs from compiled public or factory verifier".into())
}

/// Exact historical files only. Unknown content is preserved and blocks migration.
pub fn retired_workflow_inputs(root: &Path) -> Result<BTreeMap<std::path::PathBuf, Option<Vec<u8>>>, String> {
    let rows = template_standard::skill_relocations();
    let retired = crate::baseline::retired_assets_from_history(&json!({"schema":1,"format":"bridgeforge-asset-history-v1","relocations":rows,"retirements":[]}))?;
    let mut inputs = BTreeMap::new();
    for item in retired.as_array().ok_or("workflow retirements missing")? {
        let relative = item["target"].as_str().ok_or("workflow retirement target missing")?;
        let path = managed_paths::safe_join(root, relative, "workflow relocation")?;
        let bytes = managed_paths::read_metadata(&path, 32 * 1024 * 1024)?;
        if bytes.as_ref().is_some_and(|b|!item["known_hashes"].as_array().is_some_and(|known|
            known.iter().any(|h|h.as_str() == Some(build_manifest::normalized_hash(b).as_str())))) {
            return Err(format!("modified or unowned old project Skill is preserved: {relative}"));
        }
        inputs.insert(path, bytes);
    }
    Ok(inputs)
}

pub fn verify_workflow_retirements(root: &Path) -> Result<(), String> {
    if retired_workflow_inputs(root)?.values().any(Option::is_some) {
        return Err("old Forge project Skills remain in .agents/skills; managed migration required".into());
    }
    Ok(())
}

/// Remove only empty parents of explicitly relocated files; never traverse user content.
pub fn prune_workflow_directories(root: &Path) -> Vec<String> {
    let mut parents = std::collections::BTreeSet::new();
    for row in template_standard::skill_relocations() {
        if let Some(relative) = row["previous_target"].as_str() {
            let mut parent = Path::new(relative).parent();
            while let Some(path) = parent.filter(|p|p.starts_with(".agents")) {
                parents.insert(path.to_path_buf()); parent = path.parent();
            }
        }
    }
    let mut retained = Vec::new();
    for relative in parents.into_iter().rev() {
        let name = relative.to_string_lossy().replace('\\', "/");
        let result = managed_paths::safe_join(root, &name, "empty workflow directory").and_then(|p| {
            fs::remove_dir(p).map_err(|e|e.to_string())
        });
        if result.is_err() && root.join(&relative).exists() { retained.push(name); }
    }
    retained
}

pub fn json_paths(root: &Path) -> Result<Vec<String>, String> {
    let directory = managed_paths::safe_join(root, ".codex", "Codex directory")?;
    if !directory.exists() { return Ok(Vec::new()); }
    let mut paths = Vec::new();
    let mut pending = vec![directory];
    while let Some(path) = pending.pop() {
        if bridgeforge_core::persistence::is_link_or_reparse(&path).map_err(|e| e.to_string())? {
            return Err(format!("linked Codex path is not allowed: {}", path.display()));
        }
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            for entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
                pending.push(entry.map_err(|e| e.to_string())?.path());
            }
        } else if metadata.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            paths.push(path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/"));
        } else if !metadata.is_file() {
            return Err(format!("Codex entry is not a regular file: {}", path.display()));
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn verify_runtime(root: &Path) -> Result<(), String> {
    let identity = if factory(root)? {
        // Installed public tools must match the public projection even when the
        // caller itself is the factory executable with a different fingerprint.
        let source = bridgeforge_core::source_projection::render(root, true)?;
        json!({"protocol":1,"source_tree_sha256":bridgeforge_core::source_projection::tree_sha(&source)?,
            "lockfile_sha256":build_manifest::normalized_hash(source.get("Cargo.lock").ok_or("public lock missing")?)})
    } else { build_identity::require()? };
    for (binary, id, receipt) in [
        ("bridgeforge", "codex.bridgeforge-cli", "build-receipt-cli.json"),
        ("bridgeforge-hook", "codex.hooks", "build-receipt-hook.json"),
    ] {
        let spec = json!({"id":id,
            "source_tree_sha256":identity["source_tree_sha256"],
            "lockfile_sha256":identity["lockfile_sha256"],
            "build_recipe_sha256":build_manifest::canonical_sha(&build_manifest::generated_build_recipe(binary))?,
            "self_test_sha256":build_manifest::canonical_sha(&json!({"args":["self-test","--json"],
                "expected_json":{"schema":1,"name":binary,"status":"ok","build_identity":identity}}))?});
        let prefix = if root.join("source/forge/factory/Cargo.toml").is_file() {
            bridgeforge_core::artifact_paths::FACTORY_BIN
        } else { bridgeforge_core::artifact_paths::PROJECT_BIN };
        let bytes = managed_paths::read_metadata(&managed_paths::safe_join(root,
            &format!("{prefix}/{binary}{}", std::env::consts::EXE_SUFFIX), "public runtime")?, 512 * 1024 * 1024)?
            .ok_or("public runtime missing")?;
        let receipt = managed_paths::read_metadata(&managed_paths::safe_join(root,
            &format!("{prefix}/{receipt}"), "public receipt")?, 1024 * 1024)?.ok_or("public receipt missing")?;
        build_receipt::verify_generated_payload(&spec, &bytes, &receipt)?;
    }
    Ok(())
}

pub fn verify(root: &Path, runtime: bool) -> Result<Value, String> {
    let paths = json_paths(root)?;
    let factory = factory(root)?;
    verify_workflow_retirements(root)?;
    // The factory owns its historical development records. This updater never
    // adopts the factory as a consumer or uses those records as online policy.
    if !factory && paths != [".codex/hooks.json"] { return Err(format!("Codex JSON must be exactly hooks.json; found: {}", paths.join(", "))); }
    let mut verified = Vec::new();
    for asset in assets_for(root)? {
        if asset.rule["strategy"] == "seed" { continue; }
        let target = asset.rule["target"].as_str().ok_or("template target missing")?;
        let payload = managed_paths::read_metadata(&managed_paths::safe_join(root, target, "template target")?, 32 * 1024 * 1024)?
            .ok_or_else(|| format!("template asset missing: {target}"))?;
        verified.push(crate::baseline::verify_asset_payload_mode(&asset.rule, &payload, true, Some(root))?);
    }
    if runtime { verify_runtime(root)?; }
    Ok(json!({"schema":1,"state":"clean","model":if factory {"factory-template-v1"} else {"template-only-v1"},"json_paths":paths,"verified":verified,
        "runtime_verified":runtime,"build_identity":build_identity::identity()}))
}

fn git(root: &Path, runner: &dyn ProcessRunner, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut request = ProcessRequest::new("git", root);
    request.args = args.iter().map(Into::into).collect();
    request.env.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
    let result = runner.run(&request).map_err(|e| e.to_string())?;
    if result.code != 0 || result.timed_out { return Err("cannot read staged template assets".into()); }
    Ok(result.stdout)
}

pub fn verify_index(root: &Path, runner: &dyn ProcessRunner) -> Result<Value, String> {
    let listing = git(root, runner, &["ls-files", "--stage", "-z"])?;
    let mut entries = BTreeMap::new();
    for row in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let text = std::str::from_utf8(row).map_err(|e| e.to_string())?;
        let (header, path) = text.split_once('\t').ok_or("invalid Git index framing")?;
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 || fields[2] != "0" { return Err("unmerged Git index".into()); }
        entries.insert(path.to_owned(), (fields[0].to_owned(), fields[1].to_owned()));
    }
    let paths = entries.keys().filter(|p| p.starts_with(".codex/") && p.to_ascii_lowercase().ends_with(".json")).cloned().collect::<Vec<_>>();
    let factory = factory(root)?;
    if !factory && paths != [".codex/hooks.json"] { return Err(format!("staged Codex JSON must be exactly hooks.json; found: {}", paths.join(", "))); }
    for relocation in template_standard::skill_relocations() {
        if relocation["previous_target"].as_str().is_some_and(|p|entries.contains_key(p)) {
            return Err("old Forge Skill remains in the staged tree".into());
        }
    }
    for asset in assets_for(root)? {
        if asset.rule["strategy"] == "seed" { continue; }
        let target = asset.rule["target"].as_str().ok_or("template target missing")?;
        let (mode, oid) = entries.get(target).ok_or_else(|| format!("staged template asset missing: {target}"))?;
        if !mode.starts_with("100") { return Err(format!("staged template asset is not a regular file: {target}")); }
        let payload = git(root, runner, &["cat-file", "blob", oid])?;
        crate::baseline::verify_asset_payload_mode(&asset.rule, &payload, true, None)?;
    }
    Ok(json!({"schema":1,"state":"clean","model":if factory {"factory-template-v1"} else {"template-only-v1"},"index":true,"json_paths":paths}))
}

fn assets_for(root: &Path) -> Result<Vec<template_standard::Asset>, String> {
    let mut assets = template_standard::assets();
    if !factory(root)? { return Ok(assets); }
    assets.retain(|asset| factory_managed_asset(&asset.rule));
    for asset in &mut assets {
        if !matches!(asset.rule["id"].as_str(), Some("codex.hooks-config" | "codex.precommit")) { continue; }
        asset.payload = factory_entrypoint_payload(&asset.payload)?;
        if asset.rule["merge_policy"] == "codex-hooks" {
            let payload: Value = serde_json::from_slice(&asset.payload).map_err(|e| e.to_string())?;
            for required in asset.rule["merge_validation"]["required_handlers"].as_array_mut().ok_or("hook rules missing")? {
                let handlers = crate::baseline::hook_handlers(&payload)?;
                required["sha256"] = json!(build_manifest::canonical_sha(&handlers[required["id"].as_str().ok_or("hook id missing")?]["handler"])?);
            }
        } else {
            let text = std::str::from_utf8(&asset.payload).map_err(|e| e.to_string())?;
            let begin = asset.rule["region"]["begin"].as_str().ok_or("region begin missing")?;
            let end = asset.rule["region"]["end"].as_str().ok_or("region end missing")?;
            let start = text.find(begin).ok_or("region begin missing")?;
            let stop = text[start..].find(end).ok_or("region end missing")? + start + end.len();
            let stop = if text[stop..].starts_with('\n') { stop+1 } else { stop };
            asset.rule["region"]["current_sha256"] = json!(build_manifest::normalized_hash(text[start..stop].as_bytes()));
        }
    }
    Ok(assets)
}

/// The owning factory maintains its own document index. Consumer Markdown
/// blocks and empty-directory seeds are not factory entrypoint mirrors.
pub fn factory_managed_asset(rule: &Value) -> bool {
    rule["id"] != "codex.doc.readme" && rule["strategy"] != "seed"
}

pub fn factory_entrypoint_payload(payload: &[u8]) -> Result<Vec<u8>, String> {
    Ok(String::from_utf8(payload.to_vec()).map_err(|e| e.to_string())?
        .replace(bridgeforge_core::artifact_paths::PROJECT_BIN, bridgeforge_core::artifact_paths::FACTORY_BIN)
        .replace(".deps\\\\bridgeforge\\\\bin", ".build\\\\bridgeforge\\\\bin").into_bytes())
}
