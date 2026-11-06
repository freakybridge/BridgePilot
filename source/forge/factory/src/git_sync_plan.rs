use crate::ProcessRunner;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[cfg(all(test, bridgeforge_factory_tests))]
thread_local! { static LEGACY_REFERENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

#[cfg(all(test, bridgeforge_factory_tests))]
pub(super) fn with_legacy_reference<T>(run: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            LEGACY_REFERENCE.set(false);
        }
    }
    LEGACY_REFERENCE.set(true);
    let _reset = Reset;
    run()
}

pub(super) struct WritePlan {
    pub generated_reused: usize,
    pub generated_built: usize,
    pub writes: BTreeMap<PathBuf, Vec<u8>>,
    pub binaries: BTreeSet<PathBuf>,
    pub before: BTreeMap<PathBuf, Option<Vec<u8>>>,
    inputs: BTreeMap<PathBuf, Vec<u8>>,
    release_inputs: BTreeMap<PathBuf, Option<Vec<u8>>>,
    source_inventory: Vec<String>,
}

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
            return Err(format!(
                "factory input traverses a link: {}",
                path.display()
            ));
        }
    }
    Ok(path)
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl WritePlan {
    pub fn prepare(
        root: &Path,
        mut writes: BTreeMap<PathBuf, Vec<u8>>,
        release_inputs: BTreeMap<PathBuf, Option<Vec<u8>>>,
        factory: bool,
        runner: &dyn ProcessRunner,
    ) -> Result<Self, String> {
        crate::release::verify_release_inputs(&release_inputs)?;
        let mut plan = Self {
            generated_reused: 0,
            generated_built: 0,
            writes: BTreeMap::new(),
            binaries: BTreeSet::new(),
            before: BTreeMap::new(),
            inputs: BTreeMap::new(),
            release_inputs,
            source_inventory: Vec::new(),
        };
        for path in writes.keys() {
            plan.before.insert(path.clone(), read_optional(path)?);
        }
        if !factory {
            plan.writes = writes;
            return Ok(plan);
        }

        let managed = join(root, bridgeforge_core::product_manifest::FILE)?;
        let managed_bytes = fs::read(&managed).map_err(|e| e.to_string())?;
        let product = bridgeforge_core::product_manifest::parse(&managed_bytes)?;
        let contract = &product["project_skeleton"];
        let skills = &product["shared_skills"];
        let mut paths = BTreeSet::from(["VERSION".to_string()]);
        paths.insert(crate::source_projection::template_relative(root, crate::user_agents::SOURCE));
        paths.insert(crate::user_agents::MANIFEST.into());
        let modern = crate::source_projection::available(root);
        if modern {
            for relative in [
                "AGENTS.md",
                ".gitignore",
                ".codex/hooks.json",
                ".githooks/pre-commit",
            ] {
                let path = join(root, relative)?;
                plan.before.insert(path.clone(), read_optional(&path)?);
                if path.exists() {
                    paths.insert(relative.into());
                }
            }
            paths.insert(crate::source_projection::template_relative(root, "templates/hooks/.gitignore"));
            if root.join("scripts/tests/Cargo.toml").is_file() {
                paths.insert("scripts/tests/Cargo.toml".into());
                paths.insert("scripts/tests/Cargo.lock".into());
            }
            plan.source_inventory = crate::source_projection::render(root, false)?
                .into_keys()
                .collect();
            // The projection's ignore file is rendered from the already-bound
            // templates/hooks input, not the repository's own Git settings.
            paths.extend(
                plan.source_inventory
                    .iter()
                    .filter(|p| p.as_str() != ".gitignore")
                    .map(|p| crate::source_projection::physical_path(root, p)),
            );
        } else {
            plan.source_inventory =
                crate::manifest::generated_sources(&bridgeforge_core::source_projection::template_path(&root, "templates/hooks"))?;
            paths.extend(
                plan.source_inventory
                    .iter()
                    .map(|p| format!("templates/hooks/{p}")),
            );
        }
        for asset in contract["assets"]
            .as_array()
            .ok_or("missing factory assets")?
        {
            if bridgeforge_skeleton::content::inline_source(asset)?.is_some() {
                continue;
            }
            let source = asset["source"].as_str().ok_or("missing asset source")?;
            let target = asset["target"].as_str().ok_or("missing asset target")?;
            if !source.ends_with(".py")
                && !target.ends_with(".py")
                && !source.starts_with("templates/hooks/")
                && !target.starts_with(".codex/hooks/")
            {
                paths.insert(source.into());
            }
        }
        for platform in skills["platforms"].as_object().into_iter().flat_map(|platforms| platforms.values()) {
            for skill in platform["skills"]
                .as_array()
                .ok_or("missing platform skills")?
            {
                for file in skill["files"].as_array().ok_or("missing skill files")? {
                    paths.insert(
                        file["source"]
                            .as_str()
                            .ok_or("missing skill source")?
                            .into(),
                    );
                }
            }
        }
        plan.inputs.insert(managed.clone(), managed_bytes);
        for relative in paths {
            let path = join(root, &relative)?;
            plan.inputs
                .insert(path.clone(), fs::read(path).map_err(|e| e.to_string())?);
        }
        let temporary = Temporary(bridgeforge_core::artifact_paths::temporary_build(
            root,
            "sync-plan",
        )?);
        for (path, payload) in plan.inputs.iter().chain(writes.iter()) {
            let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
            let target = join(&temporary.0, &relative.to_string_lossy().replace('\\', "/"))?;
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::write(target, payload).map_err(|e| e.to_string())?;
        }
        let managed_after = crate::manifest::render_managed_contract(&temporary.0)?;
        let public_after: serde_json::Value =
            serde_json::from_slice(&managed_after).map_err(|e| e.to_string())?;
        let contract_after = crate::manifest::runtime_contract(&temporary.0, &public_after)?;
        writes.insert(managed, crate::manifest::render_product_manifest(&temporary.0, &BTreeMap::new())?);
        writes.insert(
            join(root, ".codex/managed-skeleton.json")?,
            crate::manifest::render_factory_contract(&temporary.0, &BTreeMap::new())?,
        );
        if modern {
            for (relative, payload) in
                crate::manifest::development_projection(&temporary.0, &BTreeMap::new())?
            {
                let target = join(root, &relative)?;
                if writes.get(&target).is_some_and(|bytes| bytes != &payload) {
                    return Err(format!(
                        "factory derived output conflicts with native release: {relative}"
                    ));
                }
                if read_optional(&target)?.as_deref() != Some(payload.as_slice()) {
                    writes.insert(target, payload);
                }
            }
        }
        let platform = if cfg!(windows) {
            "windows-x86_64"
        } else if cfg!(target_os = "linux") {
            "linux-x86_64"
        } else {
            "macos-x86_64"
        };
        let all_assets = contract_after["generated_assets"]
            .as_array()
            .ok_or("missing generated assets")?
            .clone();
        let mut groups = BTreeMap::<bool, Vec<serde_json::Value>>::new();
        for asset in &all_assets {
            let binary = join(
                root,
                asset["binary_targets"][platform]
                    .as_str()
                    .ok_or("missing binary target")?,
            )?;
            let receipt = join(
                root,
                asset["receipt_target"]
                    .as_str()
                    .ok_or("missing receipt target")?,
            )?;
            plan.before.insert(binary.clone(), read_optional(&binary)?);
            plan.before
                .insert(receipt.clone(), read_optional(&receipt)?);
            if crate::generated_assets::installed_current(root, asset, &binary, runner) {
                plan.generated_reused += 1;
            } else {
                groups
                    .entry(asset["id"] == "codex.factory-cli")
                    .or_default()
                    .push(asset.clone());
                plan.binaries.insert(binary);
            }
        }
        for path in writes.keys() {
            if !plan.before.contains_key(path) {
                plan.before.insert(path.clone(), read_optional(path)?);
            }
        }
        // Complete one group's cache lifecycle before acquiring the next group.
        // A held lock protects both the final cache and its stable Cargo slot.
        #[cfg(all(test, bridgeforge_factory_tests))]
        let reference = LEGACY_REFERENCE.get();
        #[cfg(not(all(test, bridgeforge_factory_tests)))]
        let reference = false;
        // Test-only reference reproduces the former two-lock lifetime and
        // transient Cargo workspaces against exactly the same candidate bytes.
        let mut reference_locks = Vec::new();
        for (private, assets) in groups {
            let started = std::time::Instant::now();
            let source = source_root(&temporary.0, &assets[0])?;
            let opened = crate::artifact_cache::Cache::open_project(root, &source, runner);
            let mut cache = if reference {
                opened.ok().flatten()
            } else {
                opened?
            };
            let mut pending = Vec::new();
            for asset in &assets {
                if let Some((payload, receipt)) = cache.as_mut().and_then(|c| c.load(asset, runner))
                {
                    writes.insert(
                        join(
                            root,
                            asset["binary_targets"][platform]
                                .as_str()
                                .ok_or("binary target missing")?,
                        )?,
                        payload,
                    );
                    writes.insert(
                        join(
                            root,
                            asset["receipt_target"]
                                .as_str()
                                .ok_or("receipt target missing")?,
                        )?,
                        receipt,
                    );
                    plan.generated_reused += 1;
                } else {
                    pending.push(asset.clone());
                }
            }
            if !pending.is_empty() {
                let fallback;
                let inputs = if let Some(cache) = cache.as_ref().filter(|_| !reference) {
                    crate::build_inputs::BuildInputs::capture_persistent(
                        &source,
                        &bridgeforge_core::artifact_paths::cargo_slot(root, private),
                        &pending[0],
                        cache.compiler_environment(),
                    )?
                } else {
                    runner.cache_miss("environment_or_project_cache_unavailable");
                    fallback = Temporary(bridgeforge_core::artifact_paths::temporary_build(
                        root,
                        "cache-context",
                    )?);
                    crate::build_inputs::BuildInputs::capture(
                        &source,
                        fallback.0.join("source-0"),
                        &pending[0],
                    )?
                };
                let projection = pending[0]["source_projection"].as_str().map(str::to_owned);
                if pending.iter().any(|a| {
                    a["source_projection"].as_str().map(str::to_owned) != projection
                        || source_root(&temporary.0, a).ok().as_ref() != Some(&source)
                }) {
                    return Err("generated build group mixes sources or projections".into());
                }
                let captured = BTreeMap::from([((source.clone(), projection), &inputs)]);
                if let Some(cache) = &cache {
                    cache.verify_compiler_environment(&inputs.snapshot, runner)?;
                }
                let mut selected = contract_after.clone();
                selected["generated_assets"] = serde_json::Value::Array(pending.clone());
                let (generated, _) = crate::generated_assets::generated_writes_captured(
                    &temporary.0,
                    "source_root",
                    root,
                    &selected,
                    runner,
                    &captured,
                )?;
                inputs.verify_unchanged()?;
                plan.verify_unchanged(root)?;
                if let Some(cache) = &mut cache {
                    cache.verify_compiler_environment(&inputs.snapshot, runner)?;
                    for asset in &pending {
                        let binary = join(
                            root,
                            asset["binary_targets"][platform]
                                .as_str()
                                .ok_or("binary target missing")?,
                        )?;
                        let receipt = join(
                            root,
                            asset["receipt_target"]
                                .as_str()
                                .ok_or("receipt target missing")?,
                        )?;
                        if cache
                            .store(asset, &generated[&binary], &generated[&receipt], runner)
                            .is_err()
                        {
                            runner.cache_miss("optional_cache_publication_failed");
                        }
                        cache.verify_compiler_environment(&inputs.snapshot, runner)?;
                    }
                }
                inputs.verify_unchanged()?;
                plan.generated_built += pending.len();
                writes.extend(generated);
            }
            runner.build_group_finished(
                if private { "factory" } else { "public" },
                started.elapsed().as_millis(),
            );
            if reference {
                reference_locks.push(cache);
            }
        }
        plan.writes = writes;
        plan.verify_unchanged(root)?;
        Ok(plan)
    }

    pub fn verify_unchanged(&self, root: &Path) -> Result<(), String> {
        crate::release::verify_release_inputs(&self.release_inputs)?;
        for (path, expected) in &self.inputs {
            if fs::read(path).ok().as_ref() != Some(expected) {
                return Err(format!(
                    "factory build input changed concurrently: {}",
                    path.display()
                ));
            }
        }
        for (path, expected) in &self.before {
            if &read_optional(path)? != expected {
                return Err(format!(
                    "automatic target changed concurrently before apply: {}",
                    path.display()
                ));
            }
        }
        let current_inventory = if crate::source_projection::available(root) {
            crate::source_projection::render(root, false)?
                .into_keys()
                .collect::<Vec<_>>()
        } else {
            crate::manifest::generated_sources(&bridgeforge_core::source_projection::template_path(&root, "templates/hooks"))?
        };
        if !self.source_inventory.is_empty() && current_inventory != self.source_inventory {
            return Err("factory source inventory changed concurrently".into());
        }
        Ok(())
    }
}

fn source_root(root: &Path, asset: &serde_json::Value) -> Result<PathBuf, String> {
    let relative = asset["source_root"].as_str().ok_or("missing source_root")?;
    if relative == "."
        && matches!(
            asset["source_projection"].as_str(),
            Some(crate::source_projection::PUBLIC | crate::source_projection::FACTORY)
        )
    {
        Ok(root.to_path_buf())
    } else {
        join(root, relative)
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot snapshot {}: {error}", path.display())),
    }
}
