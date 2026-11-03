use bridgeforge_core::artifact_paths;
pub use bridgeforge_core::build_manifest::{
    canonical_sha, generated_build_recipe, generated_source_sha, generated_sources,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

fn sha_raw(payload: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(payload))
}

fn sha_git(payload: &[u8]) -> String {
    if payload.contains(&0) {
        sha_raw(payload)
    } else {
        sha_raw(
            String::from_utf8_lossy(payload)
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .as_bytes(),
        )
    }
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains('\\')
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("unsafe manifest path: {relative}"));
    }
    Ok(root.join(path))
}

fn marker_block(payload: &[u8], begin: &str, end: &str) -> Result<Vec<u8>, String> {
    let text =
        String::from_utf8(payload.to_vec()).map_err(|_| "managed marker source is not UTF-8")?;
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let starts = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_end_matches(['\r', '\n']) == begin)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let stops = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_end_matches(['\r', '\n']) == end)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if starts.len() != 1 || stops.len() != 1 || starts[0] >= stops[0] {
        return Err(format!(
            "managed markers must appear exactly once: {begin} / {end}"
        ));
    }
    Ok(lines[starts[0]..=stops[0]].concat().into_bytes())
}

fn hook_asset_id(relative: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for character in relative.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    format!("codex.hooks.{slug}")
}

fn hook_sources(root: &Path) -> Result<Vec<String>, String> {
    generated_sources(&crate::source_projection::template_path(root, "templates/hooks"))
}

fn source_tree_sha(root: &Path, sources: &[String]) -> Result<String, String> {
    generated_source_sha(&crate::source_projection::template_path(root, "templates/hooks"), sources)
}

fn generated_asset(
    binary: &str,
    receipt: &str,
    sources_sha: &str,
    lock_sha: &str,
) -> Result<Value, String> {
    let build = generated_build_recipe(binary);
    let expected_name = binary;
    let self_test = json!({
        "args": ["self-test", "--json"],
        "expected_json": {"schema": 1, "name": expected_name, "status": "ok",
            "build_identity":{"protocol":1,"source_tree_sha256":sources_sha,"lockfile_sha256":lock_sha}},
    });
    let file = binary;
    Ok(json!({
        "id": if binary == "bridgeforge-hook" { "codex.hooks" } else { "codex.bridgeforge-cli" },
        "source_root": "templates/hooks",
        "target_source_root": ".codex/hooks",
        "manifest": "Cargo.toml",
        "lockfile": "Cargo.lock",
        "binary_targets": {
            "windows-x86_64": format!("{}/{file}.exe", artifact_paths::PROJECT_BIN),
            "linux-x86_64": format!("{}/{file}", artifact_paths::PROJECT_BIN),
            "macos-x86_64": format!("{}/{file}", artifact_paths::PROJECT_BIN),
        },
        "receipt_target": receipt,
        "build": build,
        "self_test": self_test,
        "source_tree_sha256": sources_sha,
        "lockfile_sha256": lock_sha,
        "build_recipe_sha256": canonical_sha(&build)?,
        "self_test_sha256": canonical_sha(&self_test)?,
    }))
}

/// Add the private factory executable only to a local build contract. The
/// distributed managed contract deliberately continues to expose two binaries.
pub fn runtime_contract(root: &Path, public: &Value) -> Result<Value, String> {
    let mut contract = public.clone();
    if !crate::source_projection::available(root) {
        return Ok(contract);
    }
    factory_binary_targets(&mut contract)?;
    let files = crate::source_projection::render(root, false)?;
    let package = std::str::from_utf8(
        files
            .get("source/factory/Cargo.toml")
            .ok_or("factory package manifest missing from source projection")?,
    )
    .map_err(|e| e.to_string())?
    .parse::<toml_edit::DocumentMut>()
    .map_err(|e| e.to_string())?;
    let version = package["package"]["version"]
        .as_str()
        .ok_or("factory package must own its native version")?;
    let product = bridgeforge_core::product_version::forge(root)?;
    let build = generated_build_recipe("bridgeforge-factory");
    let self_test = json!({"args":["self-test","--json"],"expected_json":{
        "schema":1,"name":"bridgeforge-factory","status":"ok","version":version,"product_version":product}});
    let asset = json!({
        "id":"codex.factory-cli",
        "source_root":".","target_source_root":".","source_projection":crate::source_projection::FACTORY,
        "manifest":"Cargo.toml","lockfile":"Cargo.lock",
        "binary_targets":{
            "windows-x86_64":format!("{}/bridgeforge-factory.exe", artifact_paths::FACTORY_BIN),
            "linux-x86_64":format!("{}/bridgeforge-factory", artifact_paths::FACTORY_BIN),
            "macos-x86_64":format!("{}/bridgeforge-factory", artifact_paths::FACTORY_BIN)},
        "receipt_target":format!("{}/build-receipt-factory.json", artifact_paths::FACTORY_BIN),
        "source_tree_sha256":crate::source_projection::tree_sha(&files)?,
        "lockfile_sha256":sha_git(files.get("Cargo.lock").ok_or("factory lockfile missing")?),
        "build_recipe_sha256":canonical_sha(&build)?,"build":build,
        "self_test_sha256":canonical_sha(&self_test)?,"self_test":self_test,
    });
    let assets = contract["generated_assets"]
        .as_array_mut()
        .ok_or("runtime generated assets missing")?;
    if assets.iter().any(|entry| entry["id"] == asset["id"]) {
        return Err("public contract must not contain the private factory executable".into());
    }
    assets.push(asset);
    Ok(contract)
}

pub fn render_managed_contract(root: &Path) -> Result<Vec<u8>, String> {
    render_managed_contract_with_overlay(root, &BTreeMap::new())
}

fn render_managed_contract_with_overlay(
    root: &Path,
    overlay: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, String> {
    render_contract(root, overlay, false)
}

pub fn render_factory_contract(
    root: &Path,
    overlay: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, String> {
    render_contract(root, overlay, crate::source_projection::available(root))
}

fn factory_binary_targets(contract: &mut Value) -> Result<(), String> {
    for asset in contract["generated_assets"]
        .as_array_mut()
        .ok_or("generated assets missing")?
    {
        for value in asset["binary_targets"]
            .as_object_mut()
            .ok_or("binary targets missing")?
            .values_mut()
        {
            let name = Path::new(value.as_str().ok_or("binary path missing")?)
                .file_name()
                .ok_or("binary name missing")?
                .to_string_lossy();
            *value = json!(format!("{}/{name}", artifact_paths::FACTORY_BIN));
        }
        let name = Path::new(
            asset["receipt_target"]
                .as_str()
                .ok_or("receipt target missing")?,
        )
        .file_name()
        .ok_or("receipt name missing")?
        .to_string_lossy();
        asset["receipt_target"] = json!(format!("{}/{name}", artifact_paths::FACTORY_BIN));
    }
    Ok(())
}

fn factory_payload(id: &str, payload: Vec<u8>) -> Result<Vec<u8>, String> {
    if matches!(id, "codex.hooks-config" | "codex.precommit") {
        bridgeforge_skeleton::template::factory_entrypoint_payload(&payload)
    } else {
        Ok(payload)
    }
}

fn render_contract(
    root: &Path,
    overlay: &BTreeMap<String, Vec<u8>>,
    factory: bool,
) -> Result<Vec<u8>, String> {
    let contract = bridgeforge_core::product_manifest::project(root)?;
    render_contract_value(root, overlay, factory, contract)
}

fn render_contract_value(
    root: &Path,
    overlay: &BTreeMap<String, Vec<u8>>,
    factory: bool,
    mut contract: Value,
) -> Result<Vec<u8>, String> {
    if contract["schema_version"].as_u64() != Some(4)
        || contract["baseline_model"].as_str() != Some("current-only")
    {
        return Err("managed contract must use schema 4 current-only".into());
    }
    contract["release_version"] = Value::String(
        bridgeforge_core::product_version::forge(root)?,
    );
    crate::baseline::compatibility_baseline(&contract)?;
    let projected = if crate::source_projection::available(root) {
        Some(crate::source_projection::render_with_overlay(
            root, true, overlay,
        )?)
    } else {
        None
    };
    let history = if projected.is_some() {
        let history = contract
            .get("asset_history")
            .ok_or("managed contract asset_history is missing")?
            .clone();
        contract["retired_assets"] = crate::baseline::retired_assets_from_history(&history)?;
        Some(history)
    } else {
        None
    };
    let sources = if let Some(files) = &projected {
        files.keys().cloned().collect()
    } else {
        hook_sources(root)?
    };
    let existing = contract["assets"]
        .as_array()
        .ok_or("contract assets are missing")?;
    let mut assets = existing
        .iter()
        .filter(|asset| {
            let source = asset["source"].as_str().unwrap_or("");
            let target = asset["target"].as_str().unwrap_or("");
            !source.ends_with(".py")
                && !target.ends_with(".py")
                && !source.starts_with("templates/hooks/")
                && !target.starts_with(".codex/hooks/")
                && asset["id"] != "codex.generated-bin.gitignore"
        })
        .cloned()
        .collect::<Vec<_>>();
    for relative in &sources {
        let source = if projected.is_some() && relative == ".gitignore" {
            crate::source_projection::template_relative(root, "templates/hooks/.gitignore")
        } else if projected.is_some() {
            crate::source_projection::physical_path(root, relative)
        } else {
            format!("templates/hooks/{relative}")
        };
        let existing_id = history
            .as_ref()
            .and_then(|l| l["relocations"].as_array())
            .and_then(|entries| entries.iter().find(|a| a["source"] == source
                || a["source"] == relative.as_str()
                || a["source"].as_str().is_some_and(|s| crate::source_projection::template_relative(root, s) == source)))
            .or_else(|| existing.iter().find(|a| a["source"] == source || a["target"] == format!(".codex/hooks/{relative}")))
            .and_then(|a| a["id"].as_str())
            .map(str::to_owned);
        let mut asset = json!({
            "id": existing_id.unwrap_or_else(|| hook_asset_id(relative)),
            "source": source,
            "target": format!(".codex/hooks/{relative}"),
            "strategy": "whole",
            "current_sha256": "",
        });
        if projected.is_some() && matches!(relative.as_str(), "Cargo.toml" | "Cargo.lock") {
            asset["render"] = json!(if relative == "Cargo.toml" {
                "public-runtime-workspace"
            } else {
                "public-runtime-lock"
            });
        }
        assets.push(asset);
    }
    for asset in &mut assets {
        let source = asset["source"].as_str().unwrap_or("");
        let payload = if let Some(bytes) = bridgeforge_skeleton::content::inline_source(asset)? {
            bytes.to_vec()
        } else if let Some(bytes) = projected.as_ref().and_then(|files| files.get(&crate::source_projection::logical_path(source))) {
            bytes.clone()
        } else if let Some(bytes) = overlay.get(source) {
            bytes.clone()
        } else {
            fs::read(safe_join(root, source)?).map_err(|error| format!("{source}: {error}"))?
        };
        let payload = if factory {
            factory_payload(asset["id"].as_str().ok_or("asset id missing")?, payload)?
        } else {
            payload
        };
        asset["current_sha256"] = Value::String(sha_git(&payload));
        if let Some(blocks) = asset
            .get_mut("managed_blocks")
            .filter(|value| value.is_object())
        {
            let projection = crate::baseline::markdown_projection(&payload, blocks)?;
            blocks["current_projection_sha256"] = Value::String(canonical_sha(&projection)?);
        }
        if asset["merge_policy"].as_str() == Some("codex-hooks") {
            let document = crate::json::parse_unique_json(&payload, "hooks.json")?;
            let handlers = crate::baseline::hook_handlers(&document)?;
            let mut required = Vec::new();
            for (id, record) in handlers {
                if !id.starts_with("bridgeforge-codex.project-hook.v1:") {
                    continue;
                }
                let mut projected = asset["merge_validation"]["required_handlers"]
                    .as_array()
                    .and_then(|entries| entries.iter().find(|entry| entry["id"] == id))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                projected["id"] = Value::String(id);
                projected["event"] = record["event"].clone();
                projected["matcher"] = record["matcher"].clone();
                projected["sha256"] = Value::String(canonical_sha(&record["handler"])?);
                required.push(projected);
            }
            if required.is_empty() {
                return Err("hooks.json has no managed project handlers".into());
            }
            asset["merge_validation"]["required_handlers"] = Value::Array(required);
        }
        if let Some(public) = asset
            .get_mut("agents_zones")
            .and_then(|zones| zones.get_mut("public"))
        {
            let block = marker_block(
                &payload,
                public["begin"]
                    .as_str()
                    .ok_or("AGENTS public begin is missing")?,
                public["end"]
                    .as_str()
                    .ok_or("AGENTS public end is missing")?,
            )?;
            public["current_sha256"] = Value::String(sha_git(&block));
        }
        if let Some(region) = asset.get_mut("region") {
            let block = marker_block(
                &payload,
                region["begin"].as_str().ok_or("region begin is missing")?,
                region["end"].as_str().ok_or("region end is missing")?,
            )?;
            region["current_sha256"] = Value::String(sha_git(&block));
        }
    }
    contract["assets"] = Value::Array(assets);
    let tree_sha = if let Some(files) = &projected {
        crate::source_projection::tree_sha(files)?
    } else {
        source_tree_sha(root, &sources)?
    };
    let lock_bytes = if let Some(files) = &projected {
        files["Cargo.lock"].clone()
    } else {
        fs::read(bridgeforge_core::source_projection::template_path(&root, "templates/hooks/Cargo.lock")).map_err(|error| error.to_string())?
    };
    let lock_sha = sha_git(&lock_bytes);
    contract["generated_assets"] = Value::Array(vec![
        generated_asset(
            "bridgeforge-hook",
            &format!("{}/build-receipt-hook.json", artifact_paths::PROJECT_BIN),
            &tree_sha,
            &lock_sha,
        )?,
        generated_asset(
            "bridgeforge",
            &format!("{}/build-receipt-cli.json", artifact_paths::PROJECT_BIN),
            &tree_sha,
            &lock_sha,
        )?,
    ]);
    if projected.is_some() {
        for item in contract["generated_assets"].as_array_mut().unwrap() {
            item["source_root"] = json!(".");
            item["source_projection"] = json!(crate::source_projection::PUBLIC);
        }
    }
    if projected.is_some() {
        contract["artifact_layout"] = json!(artifact_paths::CONTRACT);
    }
    if factory {
        factory_binary_targets(&mut contract)?;
    }
    let mut encoded = serde_json::to_vec_pretty(&contract).map_err(|error| error.to_string())?;
    encoded.push(b'\n');
    Ok(encoded)
}

pub fn render_distribution_manifest(root: &Path) -> Result<Vec<u8>, String> {
    let mut manifest = bridgeforge_core::product_manifest::section(root, "shared_skills")?;
    let platforms = manifest["platforms"]
        .as_object_mut()
        .ok_or("distribution manifest platforms are missing")?;
    for platform in platforms.values_mut() {
        let skills = platform["skills"]
            .as_array_mut()
            .ok_or("platform skills are missing")?;
        for skill in skills {
            let files = skill["files"]
                .as_array_mut()
                .ok_or("skill files are missing")?;
            for item in files {
                let source = item["source"].as_str().ok_or("skill source is missing")?;
                let payload = fs::read(safe_join(root, source)?)
                    .map_err(|error| format!("{source}: {error}"))?;
                item["sha256"] = Value::String(sha_git(&payload));
            }
        }
    }
    let mut encoded = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    encoded.push(b'\n');
    Ok(encoded)
}

/// Render only explicit factory mirrors; public/project extension regions keep
/// the same ownership and preservation semantics as the installation renderer.
fn factory_entrypoints(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let contract: Value = serde_json::from_slice(&render_factory_contract(root, &BTreeMap::new())?)
        .map_err(|e| e.to_string())?;
    let mut outputs = BTreeMap::new();
    for asset in contract["assets"]
        .as_array()
        .ok_or("factory assets missing")?
    {
        let id = asset["id"].as_str().ok_or("factory asset id missing")?;
        if !bridgeforge_skeleton::template::factory_managed_asset(asset) { continue; }
        let template_source = asset["source"].as_str().is_some_and(|s|
            s.starts_with("source/forge/templates/") || s.starts_with("templates/"));
        if !template_source && asset.get("content").is_none() {
            continue;
        }
        if asset["target"].as_str().is_some_and(|s|
            s.starts_with(".codex/hooks/") && s != ".codex/hooks/.gitignore") {
            continue;
        }
        let target = asset["target"].as_str().ok_or("factory target missing")?;
        let payload = if let Some(bytes) = bridgeforge_skeleton::content::inline_source(asset)? {
            bytes.to_vec()
        } else {
            let source = asset["source"].as_str().ok_or("factory source missing")?;
            fs::read(safe_join(root, source)?).map_err(|e| e.to_string())?
        };
        let payload = factory_payload(id, payload)?;
        let current =
            crate::managed_paths::read_metadata(&safe_join(root, target)?, 16 * 1024 * 1024)?;
        let rendered = bridgeforge_skeleton::content::render(
            id,
            &payload,
            current.as_deref(),
            asset,
            root,
            false,
        )?;
        outputs.insert(target.into(), rendered);
    }
    Ok(outputs)
}

pub(crate) fn render_product_manifest(root: &Path, overlay: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut product = bridgeforge_core::product_manifest::load(root)?;
    product["project_skeleton"] = serde_json::from_slice(&render_managed_contract_with_overlay(root, overlay)?).map_err(|e| e.to_string())?;
    product["shared_skills"] = serde_json::from_slice(&render_distribution_manifest(root)?).map_err(|e| e.to_string())?;
    product["user_instructions"] = serde_json::from_slice(&crate::user_agents::render_manifest(root)?).map_err(|e| e.to_string())?;
    let mut bytes = serde_json::to_vec_pretty(&product).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Only generic Template assets are embedded. Source projections and project
/// sidecars are deliberately excluded from the consumer verification rules.
pub fn render_template_standard(root: &Path) -> Result<Vec<u8>, String> {
    let contract: Value = serde_json::from_slice(&render_managed_contract(root)?)
        .map_err(|e| e.to_string())?;
    let mut output = String::from("// Generated by factory manifest from Template; do not edit.\npub fn assets() -> Vec<Asset> { vec![\n");
    for asset in contract["assets"].as_array().ok_or("template assets missing")? {
        let template = asset["source"].as_str().is_some_and(|s|
            s.starts_with("source/forge/templates/") || s.starts_with("templates/"));
        if !template && asset.get("content").is_none() { continue; }
        if asset["target"].as_str().is_some_and(|s|
            s.starts_with(".codex/hooks/") && s != ".codex/hooks/.gitignore") { continue; }
        let payload = if let Some(content) = asset["content"].as_str() {
            content.to_owned()
        } else {
            fs::read_to_string(safe_join(root, asset["source"].as_str().unwrap())?)
                .map_err(|e| e.to_string())?
        }.replace("\r\n", "\n").replace('\r', "\n");
        output.push_str(&format!("Asset {{ rule: serde_json::from_str({}).expect(\"generated template rule\"), payload: {}.as_bytes().to_vec() }},\n",
            serde_json::to_string(&serde_json::to_string(asset).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?,
            serde_json::to_string(&payload).map_err(|e| e.to_string())?));
    }
    output.push_str("] }\n");
    Ok(output.into_bytes())
}

pub(crate) fn render_targets(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    if crate::source_projection::available(root)
        && fs::read(root.join("source/forge/core/src/template_assets.rs")).ok().as_ref()
            != Some(&render_template_standard(root)?) {
        return Err("compiled Template rules are stale; run factory template-standard before preparing builds".into());
    }
    let mut targets = vec![
        (root.join(bridgeforge_core::product_manifest::FILE), render_product_manifest(root, &BTreeMap::new())?),
    ];
    if crate::source_projection::available(root) {
        targets.extend(
            factory_entrypoints(root)?
                .into_iter()
                .map(|(p, b)| (root.join(p), b)),
        );
        targets.extend(
            crate::dogfood::render(root)?
                .into_iter()
                .map(|(p, b)| (root.join(p), b)),
        );
        targets.extend(
            crate::source_projection::render(root, true)?
                .into_iter()
                .map(|(p, b)| (root.join(".codex/hooks").join(p), b)),
        );
    }
    if crate::source_projection::available(root) && root.join("scripts/tests/Cargo.toml").is_file()
    {
        targets.push((
            root.join("scripts/tests/Cargo.lock"),
            consumer_lock(root, &BTreeMap::new())?,
        ));
    }
    Ok(targets)
}

fn projection_targets(root: &Path) -> Result<Vec<(PathBuf, Option<Vec<u8>>)>, String> {
    projection_targets_from_writes(root, render_targets(root)?)
}

fn projection_targets_from_writes(
    root: &Path,
    writes: Vec<(PathBuf, Vec<u8>)>,
) -> Result<Vec<(PathBuf, Option<Vec<u8>>)>, String> {
    let contract = writes
        .iter()
        .find(|(path, _)| path == &root.join(bridgeforge_core::product_manifest::FILE))
        .ok_or("managed contract output missing")?;
    let product = bridgeforge_core::product_manifest::parse(&contract.1)?;
    let contract = &product["project_skeleton"];
    crate::baseline::validate_contract(contract)?;
    let mut targets = writes
        .into_iter()
        .map(|(p, b)| (p, Some(b)))
        .collect::<Vec<_>>();
    for asset in contract["retired_assets"].as_array().into_iter().flatten() {
        let relative = asset["target"]
            .as_str()
            .ok_or("retired projection target missing")?;
        let path = crate::managed_paths::safe_join(root, relative, "retired factory projection")?;
        if targets.iter().any(|(p, _)| p == &path) {
            return Err("retired projection is still an active output".into());
        }
        if let Some(bytes) = crate::managed_paths::read_metadata(&path, 32 * 1024 * 1024)? {
            let hash = sha_git(&bytes);
            if !asset["known_hashes"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|h| h.as_str() == Some(hash.as_str()))
            {
                return Err(format!(
                    "retired factory projection has external changes; preserved: {relative}"
                ));
            }
        }
        targets.push((path, None));
    }
    Ok(targets)
}

pub fn rebuild(root: &Path, check: bool) -> Result<bool, String> {
    let targets = projection_targets(root)?;
    let mut before = BTreeMap::new();
    for (path, _) in &targets {
        for ancestor in path.ancestors().filter(|p| p.exists()) {
            if crate::persistence::is_link_or_reparse(ancestor).map_err(|e| e.to_string())? {
                return Err(format!(
                    "manifest target traverses a link: {}",
                    path.display()
                ));
            }
        }
        before.insert(
            path.clone(),
            crate::managed_paths::read_metadata(path, 32 * 1024 * 1024)?,
        );
    }
    let changed = targets
        .iter()
        .any(|(path, payload)| before[path].as_deref() != payload.as_deref());
    if changed && !check {
        let switches_runtime = targets.iter().any(|(path, payload)| {
            [".codex/hooks.json", ".githooks/pre-commit"]
                .iter()
                .any(|p| path == &root.join(p))
                && before[path].as_deref() != payload.as_deref()
        });
        if switches_runtime && crate::source_projection::available(root) {
            let candidate: Value = serde_json::from_slice(&render_managed_contract(root)?)
                .map_err(|e| e.to_string())?;
            let runtime = runtime_contract(root, &candidate)?;
            for asset in runtime["generated_assets"]
                .as_array()
                .ok_or("runtime assets missing")?
            {
                crate::build_receipt::verify_generated(root, asset).map_err(|_| "runtime entrypoint changes require transactional build-assets --factory preparation".to_string())?;
            }
        }
        install_targets(&targets, &before, &|index| {
            if (index == 0 || index == targets.len()) && projection_targets(root)? != targets {
                return Err("manifest inputs changed during generation".into());
            }
            Ok(())
        })?;
    }
    Ok(changed)
}

fn install_targets(
    targets: &[(PathBuf, Option<Vec<u8>>)],
    before: &BTreeMap<PathBuf, Option<Vec<u8>>>,
    observer: &dyn Fn(usize) -> Result<(), String>,
) -> Result<(), String> {
    let mut installed = Vec::new();
    let result = (|| -> Result<(), String> {
        for (index, (path, payload)) in targets.iter().enumerate() {
            observer(index)?;
            if before[path].as_deref() == payload.as_deref() {
                continue;
            }
            if crate::managed_paths::read_metadata(path, 32 * 1024 * 1024)? != before[path] {
                return Err(format!(
                    "manifest target changed before write: {}",
                    path.display()
                ));
            }
            match payload {
                Some(bytes) => {
                    crate::persistence::atomic_write(path, bytes).map_err(|e| e.to_string())?
                }
                None => fs::remove_file(path).map_err(|e| e.to_string())?,
            }
            installed.push((path, payload));
        }
        observer(targets.len())?;
        Ok(())
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        for (path, payload) in installed.into_iter().rev() {
            let current = match crate::managed_paths::read_metadata(path, 32 * 1024 * 1024) {
                Ok(current) => current,
                Err(error) => {
                    failures.push(format!(
                        "external rollback state preserved at {}: {error}",
                        path.display()
                    ));
                    continue;
                }
            };
            if current.as_deref() != payload.as_deref() {
                failures.push(format!("external change preserved: {}", path.display()));
                continue;
            }
            let rollback = match &before[path] {
                Some(bytes) => {
                    crate::persistence::atomic_write(path, bytes).map_err(|e| e.to_string())
                }
                None => fs::remove_file(path).map_err(|e| e.to_string()),
            };
            if let Err(e) = rollback {
                failures.push(format!("{}: {e}", path.display()));
            }
        }
        return Err(if failures.is_empty() {
            error
        } else {
            format!("{error}; rollback: {}", failures.join("; "))
        });
    }
    Ok(())
}

/// Pure candidate materialization, shared by factory development and installation.
/// The caller owns drift checks, journaling, writes and rollback.
pub fn development_projection(
    root: &Path,
    native: &BTreeMap<String, Vec<u8>>,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if !crate::source_projection::available(root) {
        return Ok(BTreeMap::new());
    }
    let mut outputs = crate::source_projection::render_with_overlay(root, true, native)?
        .into_iter()
        .map(|(p, b)| (format!(".codex/hooks/{p}"), b))
        .collect::<BTreeMap<_, _>>();
    outputs.extend(crate::dogfood::render(root)?);
    outputs.insert(bridgeforge_core::product_manifest::FILE.into(), render_product_manifest(root, native)?);
    outputs.extend(factory_entrypoints(root)?);

    if root.join("scripts/tests/Cargo.toml").is_file() {
        outputs.insert(
            "scripts/tests/Cargo.lock".into(),
            consumer_lock(root, native)?,
        );
    }
    outputs.retain(|p, b| fs::read(root.join(p)).ok().as_ref() != Some(b));
    Ok(outputs)
}

fn consumer_lock(root: &Path, native: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, String> {
    use toml_edit::{DocumentMut, Item};
    let parse = |bytes: &[u8]| -> Result<DocumentMut, String> {
        std::str::from_utf8(bytes)
            .map_err(|e| e.to_string())?
            .parse::<DocumentMut>()
            .map_err(|e| e.to_string())
    };
    let bytes = native
        .get("Cargo.lock")
        .cloned()
        .map(Ok)
        .unwrap_or_else(|| fs::read(root.join("Cargo.lock")).map_err(|e| e.to_string()))?;
    let mut combined = parse(&bytes)?;
    let consumer =
        parse(&fs::read(root.join("scripts/tests/Cargo.lock")).map_err(|e| e.to_string())?)?;
    let manifest =
        parse(&fs::read(root.join("scripts/tests/Cargo.toml")).map_err(|e| e.to_string())?)?;
    let name = manifest["package"]["name"]
        .as_str()
        .ok_or("test harness package name missing")?
        .to_owned();
    let key = |t: &toml_edit::Table| -> Result<(String, String, Option<String>), String> {
        Ok((
            t["name"].as_str().ok_or("lock name missing")?.into(),
            t["version"].as_str().ok_or("lock version missing")?.into(),
            t.get("source").and_then(Item::as_str).map(str::to_owned),
        ))
    };
    let entries = combined
        .get_mut("package")
        .and_then(Item::as_array_of_tables_mut)
        .ok_or("native lock packages missing")?;
    for entry in consumer
        .get("package")
        .and_then(Item::as_array_of_tables)
        .ok_or("consumer lock packages missing")?
        .iter()
    {
        let candidate = key(entry)?;
        let existing = entries.iter().map(&key).collect::<Result<Vec<_>, _>>()?;
        // Canonical native versions and local dependency edges replace consumers.
        if candidate.2.is_none() && existing.iter().any(|k| k.0 == candidate.0 && k.2.is_none()) {
            continue;
        }
        if !existing.contains(&candidate) {
            entries.push(entry.clone());
        }
    }
    // Cloned tables retain positions from a different document. Normalize them
    // before serialization so reading our own projection cannot reorder it.
    for (position, entry) in entries.iter_mut().enumerate() {
        entry.set_position(position.try_into().map_err(|_| "too many lock packages")?);
    }
    crate::source_projection::lock_dependency_closure(
        combined.to_string().as_bytes(),
        &BTreeSet::from([name]),
    )
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_manifest.rs"]
mod tests;
