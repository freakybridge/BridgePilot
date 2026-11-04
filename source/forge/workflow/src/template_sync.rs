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
}

fn aggregate(root:&Path,product:&Path,product_bytes:&[u8],before:&BTreeMap<PathBuf,Option<Vec<u8>>>,writes:&BTreeMap<PathBuf,Vec<u8>>,
    json_paths:&[String],recipe:Option<&crate::project_release::Recipe>,release_target:&Value,source_inputs:&str)->Result<String,String> {
    let records=before.iter().map(|(p,b)|json!({"path":p.strip_prefix(root).unwrap().to_string_lossy().replace('\\',"/"),"sha256":b.as_ref().map(|b|hash(b))})).collect::<Vec<_>>();
    let desired=writes.iter().map(|(p,b)|json!({"path":p.strip_prefix(root).unwrap().to_string_lossy().replace('\\',"/"),"sha256":hash(b)})).collect::<Vec<_>>();
    build_manifest::canonical_sha(&json!({"schema":1,"model":"template-only-v1","root":root,"product":product,"product_sha256":hash(product_bytes),
        "identity":build_identity::require()?,"json_paths":json_paths,"inputs":records,"desired":desired,"release_recipe":recipe,
        "release_target":release_target,"source_inputs":source_inputs}))
}

fn revalidate(plan:&Plan)->Result<(),String> {
    if plan.root==plan.product || plan.root.join("source/forge/factory/Cargo.toml").is_file() {
        return Err("factory dogfood cannot use downstream project-sync".into());
    }
    require_no_pending_development(&plan.root)?;
    if state(&plan.root,".runtime/bridgeforge-codex/release-preparation/commit-guard.json")?.is_some() {
        return Err("unfinished Git commit guard; preserve and resolve it before template retirement".into());
    }
    let product=managed_paths::read_metadata(&plan.product.join(product_manifest::FILE),32*1024*1024)?.ok_or("product manifest disappeared")?;
    let contract=product_manifest::parse(&product)?;
    let mut runtime=crate::public_artifacts::read(&plan.prepared,&contract["project_skeleton"])?;
    if let Some(recipe)=&plan.release_recipe {
        let cli=runtime.get(&format!("{}/bridgeforge{}",bridgeforge_core::artifact_paths::PROJECT_BIN,std::env::consts::EXE_SUFFIX)).ok_or("prepared CLI missing")?;
        let release=crate::project_release_install::payloads(recipe,cli)?;runtime.extend(release);
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
    if aggregate(&plan.root,&plan.product,&product,&before,&plan.writes,&json_paths,plan.release_recipe.as_ref(),&target,&source)?!=plan.fingerprint {
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
    if state(&root, ".runtime/bridgeforge-codex/release-preparation/commit-guard.json")?.is_some() {
        return Err("unfinished Git commit guard; preserve and resolve it before template retirement".into());
    }
    let product_bytes = managed_paths::read_metadata(&product.join(product_manifest::FILE), 32 * 1024 * 1024)?
        .ok_or("official product manifest missing")?;
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
    let json_paths = bridgeforge_skeleton::template::json_paths(&root)?;
    for path in &json_paths {
        if path != ".codex/hooks.json" && !RETIRED.contains(&path.as_str()) {
            return Err(format!("unknown Codex JSON requires explicit review; preserved: {path}"));
        }
    }
    let mut changed = Vec::new();
    for asset in template_standard::assets() {
        let relative = asset.rule["target"].as_str().ok_or("template target missing")?;
        let current = state(&root, relative)?;
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
    let prepared=match prepared {Some(path)=>path.to_path_buf(),None=>crate::public_artifacts::prepared_for_product(&product,&crate::public_artifacts::codex_home()?,contract)?};
    let mut runtime=crate::public_artifacts::read(&prepared,contract)?;
    if let Some(recipe)=&installation.recipe {
        let cli=runtime.get(&format!("{}/bridgeforge{}",bridgeforge_core::artifact_paths::PROJECT_BIN,std::env::consts::EXE_SUFFIX)).ok_or("prepared CLI missing")?;
        let release=crate::project_release_install::payloads(recipe,cli)?;runtime.extend(release);
    }
    let mut binaries=BTreeSet::new();
    for (relative,bytes) in runtime {
        let path=managed_paths::safe_join(&root,&relative,"prepared installation target")?;
        let old=before.get(&path).ok_or("runtime target has no reviewed input snapshot")?;
        if relative.ends_with(std::env::consts::EXE_SUFFIX) && !relative.ends_with(".json") {binaries.insert(path.clone());}
        if old.as_deref()!=Some(bytes.as_slice()) {changed.push(relative);writes.insert(path,bytes);}
    }
    let fingerprint=aggregate(&root,&product,&product_bytes,&before,&writes,&json_paths,installation.recipe.as_ref(),&installation.report["target"],&source_inputs)?;
    let retired = retirements.keys().map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/")).collect::<Vec<_>>();
    let risk = changed.iter().filter(|p| before[&root.join(p)].is_some()).cloned().chain(retired.iter().cloned()).collect::<Vec<_>>();
    let safe = changed.iter().filter(|p| before[&root.join(p)].is_none()).cloned().collect::<Vec<_>>();
    let report = json!({"schema":1,"status":"planned","model":"template-only-v1",
        "aggregate_fingerprint":fingerprint,"safe":safe,"risk":risk,"retired":retired,"gaps":installation.gaps,"blockers":[],"release_installation":installation.report,
        "target_json_paths":["hooks.json"],"version":contract["release_version"],
        "public_installation":"receipt-bound; current tool bytes are included in aggregate_fingerprint"});
    Ok(Plan {root, product, product_bytes, before, writes, retirements, fingerprint, report,release_recipe:installation.recipe,prepared,binaries,source_inputs})
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
            if fs::read(plan.product.join(product_manifest::FILE)).map_err(|e| e.to_string())? != plan.product_bytes {
                return Err("product changed during installation".into());
            }
            if bridgeforge_core::source_projection::input_fingerprint(&plan.product,true)?!=plan.source_inputs {return Err("product source changed during installation".into());}
            bridgeforge_skeleton::template::verify(&plan.root, true)?;
            if let Some(r)=&plan.release_recipe {verify_target(&plan,&crate::native_release::installation_target(&plan.root,&r.repository,r.gitlab_origin.as_deref())?)?;}
            crate::project_release::verify_driver(&plan.root).map(|_| ())
        })?;
    Ok(json!({"schema":1,"status":"succeeded","model":"template-only-v1",
        "aggregate_fingerprint":fingerprint,"retired":plan.report["retired"],
        "written":writes.len(),"recovery":recovery,"release_installation":crate::project_release::verify_driver(&plan.root)?,"baseline":bridgeforge_skeleton::template::verify(&plan.root, true)?}))
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/template_only.rs"]
mod tests;
