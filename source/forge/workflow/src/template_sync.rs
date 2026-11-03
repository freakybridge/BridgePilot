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

fn public_source(product: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let source = bridgeforge_core::source_projection::render(product, true)?;
    let identity = build_identity::require()?;
    if identity["source_tree_sha256"] != bridgeforge_core::source_projection::tree_sha(&source)?
        || identity["lockfile_sha256"] != build_manifest::normalized_hash(source.get("Cargo.lock").ok_or("public lockfile missing")?) {
        return Err("product source changed or differs from compiled updater; prepare current public artifacts".into());
    }
    Ok(source)
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
}

pub fn plan(root: &Path, product: &Path) -> Result<Plan, String> {
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
    for (relative, payload) in public_source(&product)? {
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
    let records = before.iter().map(|(p, b)| json!({
        "path":p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\',"/"),
        "sha256":b.as_ref().map(|b|hash(b))})).collect::<Vec<_>>();
    let desired = writes.iter().map(|(p,b)| json!({"path":p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\',"/"),"sha256":hash(b)})).collect::<Vec<_>>();
    let fingerprint = build_manifest::canonical_sha(&json!({"schema":1,"model":"template-only-v1",
        "root":root,"product":product,"product_sha256":hash(&product_bytes),
        "identity":identity,"json_paths":json_paths,"inputs":records,"desired":desired}))?;
    let retired = retirements.keys().map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/")).collect::<Vec<_>>();
    let risk = changed.iter().filter(|p| before[&root.join(p)].is_some()).cloned().chain(retired.iter().cloned()).collect::<Vec<_>>();
    let safe = changed.iter().filter(|p| before[&root.join(p)].is_none()).cloned().collect::<Vec<_>>();
    let report = json!({"schema":1,"status":"planned","model":"template-only-v1",
        "aggregate_fingerprint":fingerprint,"safe":safe,"risk":risk,"retired":retired,"gaps":[],"blockers":[],
        "target_json_paths":["hooks.json"],"version":contract["release_version"],
        "public_installation":"receipt-bound; current tool bytes are included in aggregate_fingerprint"});
    Ok(Plan {root, product, product_bytes, before, writes, retirements, fingerprint, report})
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
    if fingerprint != plan.fingerprint { return Err("aggregate fingerprint mismatch".into()); }
    if plan.report["risk"].as_array().is_some_and(|r| !r.is_empty()) && !confirmed_risk { return Err("replacement/retirement requires --confirmed-risk".into()); }
    let _lock = ProjectLock::acquire(&plan.root)?;
    if self::plan(&plan.root, &plan.product)?.fingerprint != fingerprint { return Err("project or product drifted; regenerate plan".into()); }
    let product_value = product_manifest::parse(&plan.product_bytes)?;
    let payloads = crate::public_artifacts::read(prepared, &product_value["project_skeleton"])?;
    let mut snapshots = plan.before.clone();
    let mut writes = plan.writes.clone();
    let mut binaries = BTreeSet::new();
    for (relative, bytes) in payloads {
        let path = managed_paths::safe_join(&plan.root, &relative, "prepared runtime target")?;
        let before = snapshots.get(&path).ok_or("prepared runtime target was not in the reviewed plan")?.clone();
        if crate::project_sync::transaction_file_state(&path)? != before { return Err("public runtime changed after planning".into()); }
        if relative.ends_with(std::env::consts::EXE_SUFFIX) && !relative.ends_with(".json") { binaries.insert(path.clone()); }
        if before.as_deref() != Some(bytes.as_slice()) { writes.insert(path.clone(), bytes); }
        snapshots.insert(path, before);
    }
    let recovery = backup(&plan, &snapshots)?;
    crate::project_sync::install_template_transaction(&plan.root, &writes, &snapshots,
        &binaries, &plan.retirements, || {
            if fs::read(plan.product.join(product_manifest::FILE)).map_err(|e| e.to_string())? != plan.product_bytes {
                return Err("product changed during installation".into());
            }
            public_source(&plan.product)?;
            bridgeforge_skeleton::template::verify(&plan.root, true).map(|_| ())
        })?;
    Ok(json!({"schema":1,"status":"succeeded","model":"template-only-v1",
        "aggregate_fingerprint":fingerprint,"retired":plan.report["retired"],
        "written":writes.len(),"recovery":recovery,"baseline":bridgeforge_skeleton::template::verify(&plan.root, true)?}))
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/template_only.rs"]
mod tests;
