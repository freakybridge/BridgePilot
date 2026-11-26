//! Template-only updater: immutable public builds, generic Template rules, and
//! explicit retirement. No consumer configuration/ledger is interpreted.
use bridgeforge_core::{build_identity, build_manifest, managed_paths, persistence,
    product_manifest, project_lock::ProjectLock, template_standard};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, fs, path::{Path, PathBuf}};

pub const RETIRED: &[&str] = &[
    ".codex/managed-skeleton.json", ".codex/bridgeforge-version.json",
    ".codex/package-closures.json", ".codex/artifact-layout.json",
    ".codex/project-hooks.json", ".codex/settings.json", ".codex/project-hook-policy.json",
    ".codex/project-hook-migrations.json", ".codex/hooks/source/checks/src/todo_contract.json",
    ".codex/hooks/source/core/src/legacy_artifact_receipts.json",
    ".codex/.bridgeforge_codex_version", ".codex/.bridgeforge_version",
];

pub fn require_no_pending_development(root: &Path) -> Result<(), String> {
    let relative = ".runtime/bridgeforge-codex/develop-transaction.json";
    if state(root, relative)?.is_some() {
        return Err("unfinished historical development transaction; preserve its recovery scene and resolve it before synchronization; retired develop cannot be used as an online bypass".into());
    }
    Ok(())
}

fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(bytes))
}
fn state(root: &Path, relative: &str) -> Result<Option<Vec<u8>>, String> {
    crate::project_sync::transaction_file_state(&managed_paths::safe_join(root, relative, "template transaction")?)
}

fn verify_workflow_root(root: &Path, relative: &str) -> Result<(), String> {
    let skill = relative.strip_prefix(".codex/skills/")
        .and_then(|path| path.strip_suffix("/SKILL.md"))
        .filter(|name| !name.contains('/'));
    if skill.is_none() && relative != ".codex/maintenance/bridgeforge-codex/MAINTENANCE.md" {
        return Ok(());
    }

    if state(root, relative)?.is_none() {
        let entry = managed_paths::safe_join(root, relative, "project workflow entry")?;
        let directory = entry.parent().ok_or("project workflow entry has no parent")?;
        if directory.exists() && unowned_workflow_content(root, directory)? {
            return Err(format!("unowned nonempty project workflow is preserved: {relative}"));
        }
    }
    Ok(())
}

fn unowned_workflow_content(root: &Path, directory: &Path) -> Result<bool, String> {
    let assets = template_standard::assets();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(path) = pending.pop() {
        if persistence::is_link_or_reparse(&path).map_err(|e|e.to_string())? {
            return Err("linked project workflow content is preserved".into());
        }
        for entry in fs::read_dir(&path).map_err(|e|e.to_string())? {
            let path = entry.map_err(|e|e.to_string())?.path();
            if persistence::is_link_or_reparse(&path).map_err(|e|e.to_string())? {
                return Err("linked project workflow content is preserved".into());
            }
            if path.is_dir() { pending.push(path); continue; }
            let relative = path.strip_prefix(root).map_err(|e|e.to_string())?.to_string_lossy().replace('\\', "/");
            let Some(asset) = assets.iter().find(|a|a.rule["target"] == relative) else { return Ok(true); };
            let current = state(root, &relative)?;
            verify_workflow_asset(&relative, &asset.rule, current.as_deref())?;
        }
    }
    Ok(false)
}

fn verify_workflow_asset(relative: &str, rule: &Value, current: Option<&[u8]>) -> Result<(), String> {
    if let (Some(history), Some(current)) = (rule["previous_sha256"].as_array(), current) {
        let digest = Value::String(build_manifest::normalized_hash(current));
        if digest != rule["current_sha256"] && !history.contains(&digest) {
            return Err(format!("unowned or modified project workflow is preserved: {relative}"));
        }
    }
    Ok(())
}

/// Shared factory/consumer ownership gate before rendering any whole workflow file.
pub fn verify_workflow_target(root: &Path, relative: &str, rule: &Value, current: Option<&[u8]>) -> Result<(), String> {
    verify_workflow_root(root, relative)?;
    verify_workflow_asset(relative, rule, current)
}

fn public_source(product: &Path) -> Result<(BTreeMap<String, Vec<u8>>,String), String> {
    let (source,inputs) = bridgeforge_core::source_projection::render_with_fingerprint(product, true)?;
    let identity = build_identity::require()?;
    if identity["source_tree_sha256"] != bridgeforge_core::source_projection::tree_sha(&source)?
        || identity["lockfile_sha256"] != build_manifest::normalized_hash(source.get("Cargo.lock").ok_or("public lockfile missing")?) {
        return Err("product source changed or differs from compiled updater; prepare current public artifacts".into());
    }
    Ok((source,inputs))
}

fn runtime_targets(root: &Path, contract: &Value) -> Result<BTreeMap<PathBuf, Option<Vec<u8>>>, String> {
    let assets = contract["generated_assets"].as_array().ok_or("public builds missing")?;
    if assets.len() != 2 { return Err("expected exactly two public tools".into()); }
    let platform = bridgeforge_core::build_receipt::platform_key()?;
    let mut ids = BTreeSet::new();
    let mut snapshots = BTreeMap::new();
    for asset in assets {
        let (name, receipt_name) = match asset["id"].as_str() {
            Some("codex.bridgeforge-cli") => ("bridgeforge", "build-receipt-cli.json"),
            Some("codex.hooks") => ("bridgeforge-hook", "build-receipt-hook.json"),
            _ => return Err("unknown public tool identity".into()),
        };
        if !ids.insert(name) { return Err("duplicate public tool identity".into()); }
        let binary_relative = format!("{}/{name}{}", bridgeforge_core::artifact_paths::PROJECT_BIN, std::env::consts::EXE_SUFFIX);
        let receipt_relative = format!("{}/{receipt_name}", bridgeforge_core::artifact_paths::PROJECT_BIN);
        if asset["binary_targets"][platform] != binary_relative || asset["receipt_target"] != receipt_relative {
            return Err("public installation targets differ from the native tool layout".into());
        }
        let binary = state(root, &binary_relative)?;
        let receipt = state(root, &receipt_relative)?;
        verify_owned_runtime(asset["id"].as_str().unwrap(), binary.as_deref(), receipt.as_deref())?;
        snapshots.insert(root.join(binary_relative), binary);
        snapshots.insert(root.join(receipt_relative), receipt);
    }
    Ok(snapshots)
}

fn verify_owned_runtime(id: &str, binary: Option<&[u8]>, receipt: Option<&[u8]>) -> Result<(), String> {
    if binary.is_none() && receipt.is_none() { return Ok(()); }
    let owned = receipt.and_then(|b| crate::json::parse_unique_json(b, "installed public receipt").ok()).is_some_and(|r| {
        r["schema_version"] == 2 && r["generated_asset_id"] == id
            && r["platform"].as_str() == bridgeforge_core::build_receipt::platform_key().ok()
            && binary.is_none_or(|b| r["binary_sha256"].as_str() == Some(hash(b).as_str()))
    });
    if !owned { return Err(format!("unowned public installation is preserved: {id}")); }
    Ok(())
}

pub struct Plan {
    root: PathBuf,
    product: PathBuf,
    product_bytes: Vec<u8>,
    before: BTreeMap<PathBuf, Option<Vec<u8>>>,
    writes: BTreeMap<PathBuf, Vec<u8>>,
    retirements: BTreeMap<PathBuf, Vec<u8>>,
    pub fingerprint: String,
    pub report: Value,
    release_recipe: Option<crate::project_release::Recipe>,
    prepared: PathBuf,
    binaries: BTreeSet<PathBuf>,
    source_inputs: String,
    bp_version_hash: String,
}

fn bp_version(product: &Path) -> Result<(String, String), String> {
    let bytes = managed_paths::read_metadata(&product.join("VERSION"), 1024)?
        .ok_or("BP product VERSION missing from installation source")?;
    let version: bridgeforge_core::native_release::ProductVersion =
        std::str::from_utf8(&bytes).map_err(|e| e.to_string())?.parse()?;
    Ok((version.bp_base()?.to_string(), hash(&bytes)))
}

fn aggregate(root:&Path,product:&Path,product_bytes:&[u8],before:&BTreeMap<PathBuf,Option<Vec<u8>>>,writes:&BTreeMap<PathBuf,Vec<u8>>,
    json_paths:&[String],recipe:Option<&crate::project_release::Recipe>,release_target:&Value,source_inputs:&str,bp_version_hash:&str)->Result<String,String> {
    let records=before.iter().map(|(p,b)|json!({"path":p.strip_prefix(root).unwrap().to_string_lossy().replace('\\',"/"),"sha256":b.as_ref().map(|b|hash(b))})).collect::<Vec<_>>();
    let desired=writes.iter().map(|(p,b)|json!({"path":p.strip_prefix(root).unwrap().to_string_lossy().replace('\\',"/"),"sha256":hash(b)})).collect::<Vec<_>>();
    build_manifest::canonical_sha(&json!({"schema":1,"model":"template-only-v1","root":root,"product":product,"product_sha256":hash(product_bytes),
        "identity":build_identity::require()?,"json_paths":json_paths,"inputs":records,"desired":desired,"release_recipe":recipe,
        "release_target":release_target,"source_inputs":source_inputs,"bp_version_hash":bp_version_hash}))
}

/// Retire the previous distribution by exact identity. The normal transaction
/// backs up the complete hook, including its project-owned extension.
fn retired_submission_inputs(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    let hook = ".githooks/pre-commit";
    if let Some(bytes) = state(root, hook)? {
        let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
        let begin = "# >>> BRIDGEFORGE_CODEX_MANAGED_BEGIN\n";
        let end = "# <<< BRIDGEFORGE_CODEX_MANAGED_END\n";
        if let Some(body) = text.strip_prefix("#!/bin/sh\n").and_then(|s| s.strip_prefix(begin)) {
            let (managed, _) = body.split_once(end).ok_or("incomplete managed pre-commit; original preserved")?;
            let digest = build_manifest::normalized_hash(format!("{begin}{managed}{end}").as_bytes());
            // Official managed regions across the templates/codex, templates and source/forge layouts.
            if ![
                "0a443e9c968647b1833fac43d54f296c62bbe98cd7c78a674741a0ae8bff9b26",
                "37b2cc40314ac281b880a94e4bd1aff8732ff4b8ce4e0c9a9a1ec13eee788ed5",
                "8b15bd93041821d46f56bac36fcbb90be92d8cb3e7e044576e44226797fc0529",
                "9eecdaff09a501a3897bcca4619700f5f2c4e38a48d1c21a807520de944be78a",
                "c7bcf03995957e697ee51370cbc6e88325be4338e99e49c2c954901b67d259c5",
                "d329f6e5a02483b404dd7a52bca88e4fbeea01afe731bce09e06790e11845f94",
                "d34209d9ac0b95da9c55f83d8b70197311bdb1194ce6992383865240275dd83a",
                "e3f63c159daa1a9ed781586088e9ba3ad2f897fa67bbdd8d53e5a07a1774135a",
                "eb295c8ebb54e8c9e6360159de40c6d3f2769bcb7013b574b317fcafa38d6d28",
                "f65093bc363fdf656bff0c597d2194e864a6abcdf83264171b4ecc8169d730f0",
                "ff7700cc4decc17498094d171571ffeafaddaa1fa5edff36a8b1c7383a7ede86",
            ].contains(&digest.trim_start_matches("sha256:")) {
                return Err("unknown managed pre-commit revision; original preserved".into());
            }
            files.insert(root.join(hook), bytes);
        }
    }
    for (path, digests) in [
        (".codex/hooks/source/workflow/src/native_release_source.rs",&["sha256:452e9f339e06b096dc6092247616191fce667478909b38ecb963a04589d203ba", "sha256:41880f4c52f81d922536022eb18e8f37299ecaf02f8a72be8954b0e24476d51e", "sha256:cb0fc869ba78ca160cf98766c8311f10159438b1702ae0508c816adf6f69508e", "sha256:f1f066dbfdf7ade8444aaf969151a4cd1e87c3877cab83e10bf3a7498d384bcd", "sha256:f42f0e29c0188a8f399dfea7f1171995a40fa87dee1f343ca8a09b8307c5fc56"][..]),
        (".codex/hooks/source/workflow/src/native_release_recovery.rs",&["sha256:73c6b5650c04be189ba42ab4257821efb368e721cc342bc764cc56dc66dabe80","sha256:3f6882aa03ea00f044ff80f229567e901c817a3d7b577894942ab3cf44ee3a16"][..]),
    ] {
        if let Some(bytes) = state(root, path)? {
            if !digests.contains(&build_manifest::normalized_hash(&bytes).as_str()) { return Err(format!("modified retired source preserved: {path}")); }
            files.insert(root.join(path), bytes);
        }
    }
    Ok(files)
}

fn revalidate(plan:&Plan)->Result<(),String> {
    if plan.root==plan.product || plan.root.join("source/forge/factory/Cargo.toml").is_file() {
        return Err("factory dogfood cannot use downstream project-sync".into());
    }
    require_no_pending_development(&plan.root)?;
    bridgeforge_skeleton::template::retired_workflow_inputs(&plan.root)?;
    for (path, bytes) in retired_submission_inputs(&plan.root)? {
        if plan.retirements.get(&path) != Some(&bytes) {return Err("retired submission inputs changed; regenerate plan".into());}
    }
    for asset in template_standard::assets() {
        let target = asset.rule["target"].as_str().ok_or("template target missing")?;
        verify_workflow_root(&plan.root, target)?;
    }
    let product=managed_paths::read_metadata(&plan.product.join(product_manifest::FILE),32*1024*1024)?.ok_or("product manifest disappeared")?;
    let contract=product_manifest::parse(&product)?;
    let mut runtime=crate::public_artifacts::read(&plan.prepared,&contract["project_skeleton"])?;
    let (bp_version, bp_version_hash) = bp_version(&plan.product)?;
    if let Some(recipe)=&plan.release_recipe {
        let cli=runtime.get(&format!("{}/bridgeforge{}",bridgeforge_core::artifact_paths::PROJECT_BIN,std::env::consts::EXE_SUFFIX)).ok_or("prepared CLI missing")?;
        let release=crate::project_release_install::payloads(recipe,cli,&bp_version)?;runtime.extend(release);
    }
    for (relative,bytes) in runtime {
        let path=plan.root.join(relative);
        let reviewed=plan.writes.get(&path).or_else(||plan.before.get(&path).and_then(Option::as_ref));
        if reviewed!=Some(&bytes){return Err("prepared public or release driver bytes changed after planning".into());}
    }
    let source=bridgeforge_core::source_projection::input_fingerprint(&plan.product,true)?;
    let before=plan.before.keys().map(|path|Ok((path.clone(),crate::project_sync::transaction_file_state(path)?))).collect::<Result<BTreeMap<_,_>,String>>()?;
    let json_paths=bridgeforge_skeleton::template::json_paths(&plan.root)?;
    let target=plan.release_recipe.as_ref().map(|r|crate::native_release::installation_target(&plan.root,&r.repository,r.gitlab_origin.as_deref())).transpose()?.unwrap_or(Value::Null);
    if aggregate(&plan.root,&plan.product,&product,&before,&plan.writes,&json_paths,plan.release_recipe.as_ref(),&target,&source,&bp_version_hash)?!=plan.fingerprint {
        return Err("project or product drifted; regenerate plan".into());
    }
    Ok(())
}

fn verify_target(plan:&Plan,current:&Value)->Result<(),String> {
    if current!=&plan.report["release_installation"]["target"] {return Err("Git target changed during installation; preserve external state".into());}
    Ok(())
}

pub fn plan(root: &Path, product: &Path) -> Result<Plan, String> {
    plan_with_release(root,product,None)
}

pub fn plan_with_release(root: &Path, product: &Path, requested:Option<&crate::project_release::Recipe>) -> Result<Plan, String> {
    plan_with_artifacts(root,product,requested,None)
}

pub fn plan_with_artifacts(root: &Path, product: &Path, requested:Option<&crate::project_release::Recipe>, prepared:Option<&Path>) -> Result<Plan, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let product = product.canonicalize().map_err(|e| e.to_string())?;
    if root == product || root.join("source/forge/factory/Cargo.toml").is_file() {
        return Err("factory dogfood cannot use downstream project-sync".into());
    }
    require_no_pending_development(&root)?;
    let product_bytes = managed_paths::read_metadata(&product.join(product_manifest::FILE), 32 * 1024 * 1024)?
        .ok_or("official product manifest missing")?;
    let (bp_version, bp_version_hash) = bp_version(&product)?;
    let product_value = product_manifest::parse(&product_bytes)?;
    let contract = &product_value["project_skeleton"];
    let identity = build_identity::require()?;
    for generated in contract["generated_assets"].as_array().ok_or("public builds missing")? {
        for field in ["source_tree_sha256", "lockfile_sha256"] {
            if generated[field] != identity[field] {
                return Err("updater and product source identity differ; prepare the current public CLI".into());
            }
        }
    }
    let mut before = runtime_targets(&root, contract)?;
    let mut writes = BTreeMap::new();
    let mut retirements = BTreeMap::new();
    for (path, bytes) in bridgeforge_skeleton::template::retired_workflow_inputs(&root)? {
        if let Some(bytes) = &bytes { retirements.insert(path.clone(), bytes.clone()); }
        before.insert(path, bytes);
    }
    for (path, bytes) in retired_submission_inputs(&root)? {
        retirements.insert(path.clone(), bytes.clone());
        before.insert(path, Some(bytes));
    }
    let json_paths = bridgeforge_skeleton::template::json_paths(&root)?;
    for path in &json_paths {
        if path != ".codex/hooks.json" && !RETIRED.contains(&path.as_str()) {
            return Err(format!("unknown Codex JSON requires explicit review; preserved: {path}"));
        }
    }
    let mut changed = Vec::new();
    for asset in template_standard::assets() {
        let relative = asset.rule["target"].as_str().ok_or("template target missing")?;
        verify_workflow_root(&root, relative)?;
        let current = state(&root, relative)?;
        verify_workflow_asset(relative, &asset.rule, current.as_deref())?;
        let payload = bridgeforge_skeleton::content::render(
            asset.rule["id"].as_str().ok_or("template id missing")?, &asset.payload,
            current.as_deref(), &asset.rule, &root, true)?;
        // Compiled rules are independently verified before any transaction.
        bridgeforge_skeleton::baseline::verify_asset_payload_mode(&asset.rule, &payload, true, Some(&root))?;
        let path = managed_paths::safe_join(&root, relative, "template target")?;
        if current.as_deref() != Some(payload.as_slice()) {
            changed.push(relative.to_owned());
            writes.insert(path.clone(), payload);
        }
        before.insert(path, current);
    }
    for relative in RETIRED {
        let path = managed_paths::safe_join(&root, relative, "retired target")?;
        let bytes = state(&root, relative)?;
        if let Some(bytes) = &bytes { retirements.insert(path.clone(), bytes.clone()); }
        before.insert(path, bytes);
    }
    // Keep the existing non-JSON tool source dependency usable by native
    // project Hook tests. This is public tool source, never old project data.
    let (projected_source,source_inputs)=public_source(&product)?;
    for (relative, payload) in projected_source {
        let relative = format!(".codex/hooks/{relative}");
        if relative.to_ascii_lowercase().ends_with(".json") {
            return Err(format!("public source projection contains a retired JSON resource: {relative}"));
        }
        let path = managed_paths::safe_join(&root, &relative, "public source projection")?;
        let current = state(&root, &relative)?;
        if current.as_deref() != Some(payload.as_slice()) {
            changed.push(relative);
            writes.insert(path.clone(), payload);
        }
        before.insert(path, current);
    }
    let installation=crate::project_release_install::inspect(&root,requested)?;
    for (path,bytes) in &installation.before {before.insert(path.clone(),bytes.clone());}
    for (path,bytes) in installation.writes {
        changed.push(path.strip_prefix(&root).map_err(|e|e.to_string())?.to_string_lossy().replace('\\',"/"));
        writes.insert(path,bytes);
    }
    let prepared=match prepared {Some(path)=>path.to_path_buf(),None=>crate::public_artifacts::prepared_for_product(&product)?};
    let mut runtime=crate::public_artifacts::read(&prepared,contract)?;
    if let Some(recipe)=&installation.recipe {
        let cli=runtime.get(&format!("{}/bridgeforge{}",bridgeforge_core::artifact_paths::PROJECT_BIN,std::env::consts::EXE_SUFFIX)).ok_or("prepared CLI missing")?;
        let release=crate::project_release_install::payloads(recipe,cli,&bp_version)?;runtime.extend(release);
    }
    let mut binaries=BTreeSet::new();
    for (relative,bytes) in runtime {
        let path=managed_paths::safe_join(&root,&relative,"prepared installation target")?;
        let old=before.get(&path).ok_or("runtime target has no reviewed input snapshot")?;
        if relative.ends_with(std::env::consts::EXE_SUFFIX) && !relative.ends_with(".json") {binaries.insert(path.clone());}
        if old.as_deref()!=Some(bytes.as_slice()) {changed.push(relative);writes.insert(path,bytes);}
    }
    let fingerprint=aggregate(&root,&product,&product_bytes,&before,&writes,&json_paths,installation.recipe.as_ref(),&installation.report["target"],&source_inputs,&bp_version_hash)?;
    let retired = retirements.keys().map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/")).collect::<Vec<_>>();
    let risk = changed.iter().filter(|p| before[&root.join(p)].is_some()).cloned().chain(retired.iter().cloned()).collect::<Vec<_>>();
    let safe = changed.iter().filter(|p| before[&root.join(p)].is_none()).cloned().collect::<Vec<_>>();
    let report = json!({"schema":1,"status":"planned","model":"template-only-v1",
        "aggregate_fingerprint":fingerprint,"safe":safe,"risk":risk,"retired":retired,"gaps":installation.gaps,"blockers":[],"release_installation":installation.report,
        "target_json_paths":["hooks.json"],"version":contract["release_version"],"bp_product_version":bp_version,
        "public_installation":"receipt-bound; current tool bytes are included in aggregate_fingerprint"});
    Ok(Plan {root, product, product_bytes, before, writes, retirements, fingerprint, report,release_recipe:installation.recipe,prepared,binaries,source_inputs,bp_version_hash})
}

fn backup(plan: &Plan, snapshots: &BTreeMap<PathBuf, Option<Vec<u8>>>) -> Result<PathBuf, String> {
    let base = managed_paths::safe_join(&plan.root,
        &format!(".runtime/tests/template-only-retirement/{}/before", &plan.fingerprint[7..]), "retirement recovery")?;
    for (path, bytes) in snapshots {
        if let Some(bytes) = bytes {
            let target = base.join(path.strip_prefix(&plan.root).map_err(|e| e.to_string())?);
            if let Some(existing) = managed_paths::read_metadata(&target, 512 * 1024 * 1024)? {
                if existing != *bytes { return Err("recovery snapshot collision".into()); }
            } else { persistence::atomic_write(&target, bytes).map_err(|e| e.to_string())?; }
            if fs::read(&target).map_err(|e| e.to_string())? != *bytes { return Err("recovery snapshot verification failed".into()); }
        }
    }
    Ok(base)
}

pub fn apply(plan: Plan, fingerprint: &str, confirmed_risk: bool, prepared: &Path) -> Result<Value, String> {
    if plan.report["gaps"].as_array().is_some_and(|v|!v.is_empty()) {return Err("installation incomplete: resolve native release recipe gaps before apply".into());}
    if fingerprint != plan.fingerprint { return Err("aggregate fingerprint mismatch".into()); }
    if plan.report["risk"].as_array().is_some_and(|r| !r.is_empty()) && !confirmed_risk { return Err("replacement/retirement requires --confirmed-risk".into()); }
    let _release_lock = bridgeforge_core::file_lock::FileLock::acquire(&plan.root.join(".runtime/bridgeforge-codex/native-release/operation.lock"))?;
    let _lock = ProjectLock::acquire(&plan.root)?;
    if prepared.canonicalize().map_err(|e|e.to_string())? != plan.prepared.canonicalize().map_err(|e|e.to_string())? {return Err("public artifact receipt differs from reviewed plan".into());}
    revalidate(&plan)?;
    let snapshots = &plan.before;
    let writes = &plan.writes;
    if writes.is_empty() && plan.retirements.is_empty() {
        let release=crate::project_release::verify_driver(&plan.root)?;
        return Ok(json!({"schema":1,"status":"succeeded","model":"template-only-v1","aggregate_fingerprint":fingerprint,
            "written":0,"retired":[],"recovery":null,"release_installation":release,"baseline":bridgeforge_skeleton::template::verify(&plan.root,true)?}));
    }
    let recovery = backup(&plan, snapshots)?;
    crate::project_sync::install_template_transaction(&plan.root, writes, snapshots,
        &plan.binaries, &plan.retirements, || {
            if bp_version(&plan.product)?.1 != plan.bp_version_hash || fs::read(plan.product.join(product_manifest::FILE)).map_err(|e| e.to_string())? != plan.product_bytes {
                return Err("product changed during installation".into());
            }
            if bridgeforge_core::source_projection::input_fingerprint(&plan.product,true)?!=plan.source_inputs {return Err("product source changed during installation".into());}
            bridgeforge_skeleton::template::verify(&plan.root, true)?;
            if let Some(r)=&plan.release_recipe {verify_target(&plan,&crate::native_release::installation_target(&plan.root,&r.repository,r.gitlab_origin.as_deref())?)?;}
            crate::project_release::verify_driver(&plan.root).map(|_| ())
        })?;
    let retained_directories=bridgeforge_skeleton::template::prune_workflow_directories(&plan.root);
    Ok(json!({"schema":1,"status":"succeeded","model":"template-only-v1",
        "aggregate_fingerprint":fingerprint,"retired":plan.report["retired"],
        "written":writes.len(),"recovery":recovery,"retained_directories":retained_directories,"release_installation":crate::project_release::verify_driver(&plan.root)?,"baseline":bridgeforge_skeleton::template::verify(&plan.root, true)?}))
}
