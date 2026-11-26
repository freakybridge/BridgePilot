//! Project-owned native recipe interpreted by the installed, provenance-bound Rust driver.
//! Versions stay in native files; this recipe never stores a version ledger.
use bridgeforge_core::native_release::{Plan, ProductVersion, raw_sha};
use bridgeforge_core::{
    ProcessRequest, ProcessRunner, SemVer, SystemProcessRunner, managed_paths, persistence,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use toml_edit::{Array, DocumentMut, Item, Table, value};

pub const PROGRAM: &str = ".build/project-release/bin/bridgeforge.exe";
pub const RECEIPT: &str = ".build/project-release/bin/receipt.json";
pub const OWNER: &str = "bridgeforge-project-release-v1";
const KEY: &str = "project-release";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub schema: u32,
    pub product: String,
    pub repository: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitlab_origin: Option<String>,
    pub version_file: String,
    pub changelog: String,
    pub tag_prefix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    #[serde(default)]
    pub build: Vec<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<String>>,
}

fn array(values: &[String]) -> Item {
    let mut a = Array::new();
    for v in values {
        a.push(v.as_str())
    }
    value(a)
}
fn text(table: &Table, key: &str) -> Result<String, String> {
    table
        .get(key)
        .and_then(Item::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("project release {key} string missing"))
}
fn strings(table: &Table, key: &str) -> Result<Vec<String>, String> {
    table
        .get(key)
        .and_then(Item::as_array)
        .ok_or_else(|| format!("project release {key} array missing"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("project release {key} must contain strings"))
        })
        .collect()
}

pub fn recipe(root: &Path) -> Result<Option<Recipe>, String> {
    let Some(bytes) = managed_paths::read_metadata(&root.join("Cargo.toml"), 4 * 1024 * 1024)?
    else {
        return Ok(None);
    };
    let doc = std::str::from_utf8(&bytes)
        .map_err(|e| e.to_string())?
        .parse::<DocumentMut>()
        .map_err(|e| e.to_string())?;
    let Some(table) = doc
        .get("workspace")
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("bridgeforge"))
        .and_then(|v| v.get(KEY))
        .and_then(Item::as_table)
    else {
        return Ok(None);
    };
    if table.iter().any(|(k, _)| {
        ![
            "schema",
            "product",
            "repository",
            "gitlab-origin",
            "version-file",
            "changelog",
            "tag-prefix",
            "include",
            "build",
            "artifacts",
        ]
        .contains(&k)
    }) {
        return Err("unknown project release recipe field".into());
    }
    let build = table
        .get("build")
        .and_then(Item::as_array)
        .ok_or("project release build array missing")?
        .iter()
        .map(|v| {
            v.as_array()
                .ok_or("build command must be an argv array".to_string())?
                .iter()
                .map(|a| {
                    a.as_str()
                        .map(str::to_owned)
                        .ok_or("build argv must be strings".into())
                })
                .collect()
        })
        .collect::<Result<Vec<Vec<String>>, String>>()?;
    let r = Recipe {
        schema: table
            .get("schema")
            .and_then(Item::as_integer)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("project release schema missing")?,
        product: text(table, "product")?,
        repository: text(table, "repository")?,
        gitlab_origin: table
            .get("gitlab-origin")
            .map(|_| text(table, "gitlab-origin"))
            .transpose()?,
        version_file: text(table, "version-file")?,
        changelog: text(table, "changelog")?,
        tag_prefix: text(table, "tag-prefix")?,
        include: table.get("include").map(|_| strings(table, "include")).transpose()?,
        build,
        artifacts: table.get("artifacts").map(|_| strings(table, "artifacts")).transpose()?,
    };
    validate(root, &r)?;
    Ok(Some(r))
}

pub fn validate(root: &Path, r: &Recipe) -> Result<(), String> {
    if r.schema != 1
        || r.product.trim().is_empty()
        || !bridgeforge_core::native_release::identifier(&format!("{}1.0.0", r.tag_prefix))
    {
        return Err("invalid project release recipe identity".into());
    }
    bridgeforge_core::native_release::validate_repository(
        &r.repository,
        r.gitlab_origin.as_deref(),
    )?;
    for rel in [&r.version_file, &r.changelog] {
        if !source_path(root, rel)?.is_file() {
            return Err(format!("native project file missing: {rel}"));
        }
    }
    if r.build.len() > 5 {
        return Err("native release has too many sequential build commands; declare a project-owned aggregate build entry".into());
    }
    for command in &r.build {
        if command.is_empty()
            || command
                .iter()
                .any(|s| s.is_empty() || s.contains(['\0', '\n', '\r']))
        {
            return Err("invalid native build argv".into());
        }
    }
    let version =
        fs::read_to_string(source_path(root, &r.version_file)?).map_err(|e| e.to_string())?;
    version.parse::<ProductVersion>()?;
    Ok(())
}

fn source_path(root: &Path, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty()
        || rel == "."
        || rel.contains('\\')
        || rel
            .split('/')
            .any(|p| matches!(p, ".git" | ".runtime" | ".deps" | ".build") || p.starts_with(".env"))
    {
        return Err(
            "release source must be an explicit project path, not local state or credentials"
                .into(),
        );
    }
    managed_paths::safe_join(root, rel, "release source")
}

pub fn manifest(root: &Path, r: &Recipe) -> Result<Vec<u8>, String> {
    validate(root, r)?;
    let mut doc = match managed_paths::read_metadata(&root.join("Cargo.toml"), 4 * 1024 * 1024)? {
        Some(b) => std::str::from_utf8(&b)
            .map_err(|e| e.to_string())?
            .parse::<DocumentMut>()
            .map_err(|e| e.to_string())?,
        None => "[workspace]\nmembers = []\n"
            .parse::<DocumentMut>()
            .unwrap(),
    };
    let mut metadata = doc.as_table_mut();
    for key in ["workspace", "metadata", "bridgeforge"] {
        if !metadata.contains_key(key) {
            let mut t = Table::new();
            t.set_implicit(true);
            metadata.insert(key, Item::Table(t));
        }
        metadata = metadata.get_mut(key).and_then(Item::as_table_mut).ok_or(
            "existing native metadata is not a standard TOML table; preserve its representation",
        )?;
    }
    if metadata.get("native-release").is_some() && recipe(root)?.as_ref() != Some(r) {
        return Err("existing native release entry is project-owned; preserve it and review the exact change".into());
    }
    let mut native = Table::new();
    native["build"] = array(&[
        PROGRAM.into(),
        "project-release".into(),
        "check".into(),
        "--project-root".into(),
        ".".into(),
    ]);
    native["program"] = value(PROGRAM);
    native["args"] = array(&["project-release".into(), "prepare".into()]);
    native["prepare-timeout-seconds"] = value((r.build.len() as i64) * 3600 + 300);
    if let Some(old) = metadata.get("native-release").and_then(Item::as_table) {
        let legacy = old.contains_key("source-args");
        if old.len() != native.len() + usize::from(legacy)
            || (legacy && strings(old, "source-args")? != ["project-release", "source"])
            || old.get("program").and_then(Item::as_str) != Some(PROGRAM)
            || old
                .get("prepare-timeout-seconds")
                .and_then(Item::as_integer)
                != native
                    .get("prepare-timeout-seconds")
                    .and_then(Item::as_integer)
            || ["build", "args"]
                .iter()
                .any(|key| strings(old, key).ok() != strings(&native, key).ok())
        {
            return Err(
                "customized native release entry preserved; installation cannot overwrite it"
                    .into(),
            );
        }
    }
    metadata["native-release"] = Item::Table(native);
    let mut t = Table::new();
    t["schema"] = value(r.schema as i64);
    for (key, v) in [
        ("product", &r.product),
        ("repository", &r.repository),
        ("version-file", &r.version_file),
        ("changelog", &r.changelog),
        ("tag-prefix", &r.tag_prefix),
    ] {
        t[key] = value(v.as_str());
    }
    if let Some(origin) = &r.gitlab_origin {
        t["gitlab-origin"] = value(origin.as_str());
    }
    if let Some(paths) = &r.include { t["include"] = array(paths); }
    if let Some(paths) = &r.artifacts { t["artifacts"] = array(paths); }
    let mut commands = Array::new();
    for argv in &r.build {
        let mut a = Array::new();
        for arg in argv {
            a.push(arg.as_str());
        }
        commands.push(a);
    }
    t["build"] = value(commands);
    metadata[KEY] = Item::Table(t);
    Ok(doc.to_string().into_bytes())
}

pub fn verify_driver(root: &Path) -> Result<Value, String> {
    let r = recipe(root)?.ok_or("project release recipe missing")?;
    let bp = installed_bp_version(root, &r)?;
    Ok(json!({"schema":1,"status":"ready","product":r.product,"repository":r.repository,"gitlab_origin":r.gitlab_origin,"program":PROGRAM,"bp_product_version":bp.to_string()}))
}

fn installed_bp_version(root: &Path, r: &Recipe) -> Result<SemVer, String> {
    let identity = bridgeforge_core::build_identity::require()?;
    let bin = managed_paths::read_metadata(
        &managed_paths::safe_join(root, PROGRAM, "release driver")?,
        512 * 1024 * 1024,
    )?
    .ok_or("installed project release driver missing")?;
    let proof = managed_paths::read_metadata(
        &managed_paths::safe_join(root, RECEIPT, "release driver receipt")?,
        1024 * 1024,
    )?
    .ok_or("installed project release receipt missing")?;
    let proof = crate::json::parse_unique_json(&proof, "project release receipt")?;
    if proof["schema"] != 1
        || proof["owner"] != OWNER
        || proof["binary_sha256"] != raw_sha(&bin)
        || proof["build_identity"] != identity
        || proof["recipe_sha256"] != raw_sha(&serde_json::to_vec(r).map_err(|e| e.to_string())?)
    {
        return Err(
            "project release driver or recipe changed; repair through skeleton installation".into(),
        );
    }
    proof["bp_product_version"].as_str().ok_or("BP version missing in installed driver receipt; run the managed project update")?.parse()
}

pub(super) fn product_version(root: &Path, self_version: bool, write: bool) -> Result<(String, ProductVersion), String> {
    let (relative, bp) = if self_version {
        ("VERSION".to_owned(), None)
    } else {
        let recipe = recipe(root)?.ok_or("release requires a project recipe and its BP installation receipt")?;
        let bp = installed_bp_version(root, &recipe)?;
        (recipe.version_file, Some(bp))
    };
    let path = source_path(root, &relative)?;
    let before = fs::read_to_string(&path).map_err(|e| format!("{relative}: {e}"))?;
    let current: ProductVersion = before.parse()?;
    let next = current.with_bp(bp.unwrap_or(current.base))?;
    if write && current != next {
        let after = before.replacen(before.trim(), &next.to_string(), 1);
        if fs::read(&path).map_err(|e| e.to_string())? != before.as_bytes() {
            return Err(format!("{relative} changed during version preparation"));
        }
        persistence::atomic_write(&path, after.as_bytes()).map_err(|e| e.to_string())?;
    }
    Ok((relative, next))
}

#[cfg(windows)]
fn windows_powershell_modules(
    request: &mut ProcessRequest,
    inherited: &std::ffi::OsStr,
) -> Result<(), String> {
    let program = Path::new(&request.program)
        .file_name()
        .and_then(|s| s.to_str());
    if !program.is_some_and(|s| {
        s.eq_ignore_ascii_case("powershell") || s.eq_ignore_ascii_case("powershell.exe")
    }) {
        return Ok(());
    }
    // PowerShell 7 ships edition-specific modules in the Modules directory beside pwsh.exe.
    let mut paths: Vec<_> = std::env::split_paths(inherited).collect();
    let original_len = paths.len();
    paths.retain(|path| {
        !(path
            .file_name()
            .is_some_and(|s| s.eq_ignore_ascii_case(std::ffi::OsStr::new("Modules")))
            && path.parent().is_some_and(|p| p.join("pwsh.exe").is_file()))
    });
    if paths.len() != original_len {
        if paths.iter().all(|p| p.as_os_str().is_empty()) {
            request.env_remove.push("PSModulePath".into());
        } else {
            let value = std::env::join_paths(paths).map_err(|e| e.to_string())?;
            request.env.insert("PSModulePath".into(), value);
        }
    }
    Ok(())
}

pub fn prepare(root: &Path) -> Result<Value, String> {
    let r = recipe(root)?.ok_or("project release recipe missing")?;
    installed_bp_version(root, &r)?;
    serde_json::to_value(prepare_recipe(root, r)?).map_err(|e| e.to_string())
}

fn prepare_recipe(root: &Path, r: Recipe) -> Result<Plan, String> {
    let version = fs::read_to_string(source_path(root, &r.version_file)?).map_err(|e| e.to_string())?;
    let version = version.trim();
    for argv in &r.build {
        let mut q = ProcessRequest::new(&argv[0], root);
        q.args = argv[1..].iter().map(Into::into).collect();
        q.timeout = Duration::from_secs(3600);
        #[cfg(windows)]
        if let Some(paths) = std::env::var_os("PSModulePath") {
            windows_powershell_modules(&mut q, &paths)?;
        }
        let out = SystemProcessRunner.run(&q).map_err(|e| e.to_string())?;
        if out.code != 0 || out.timed_out {
            return Err(format!(
                "native project build failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    let notes = fs::read_to_string(source_path(root, &r.changelog)?).map_err(|e| e.to_string())?;
    let notes = version.parse::<ProductVersion>()?.notes(&notes, "")?;
    Ok(Plan {
        schema: 1,
        product: r.product.clone(),
        version: version.to_owned(),
        repository: r.repository,
        gitlab_origin: r.gitlab_origin,
        tag: format!("{}{}", r.tag_prefix, version),
        title: format!("{} {}", r.product, version),
        notes,
        assets: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_build_and_release_notes_work_without_archive_inputs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/release-no-attachments")
            .join(std::process::id().to_string());
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("VERSION"), "1.2.3.6000").unwrap();
        fs::write(root.join("CHANGELOG.md"), "## [1.2.3.6000]\nRelease notes").unwrap();
        let spec = json!({"schema":1,"product":"Example","repository":"example/project",
            "version_file":"VERSION","changelog":"CHANGELOG.md","tag_prefix":"v",
            "build":[["rustc","--version"]]});
        let mut r: Recipe = serde_json::from_value(spec).unwrap();
        for legacy in [false, true] {
            if legacy {
                r.include = Some(vec!["source".into()]);
                r.artifacts = Some(vec![".build/obsolete.zip".into()]);
            }
            fs::write(root.join("Cargo.toml"), manifest(&root, &r).unwrap()).unwrap();
            let installed = recipe(&root).unwrap().unwrap();
            assert_eq!(installed, r);
            let plan = prepare_recipe(&root, installed).unwrap();
            assert_eq!(plan.version, "1.2.3.6000");
            assert_eq!(plan.tag, "v1.2.3.6000");
            assert!(plan.notes.contains("Release notes"));
            assert!(plan.assets.is_empty());
            assert!(!root.join(".build").exists());
            fs::remove_file(root.join("Cargo.toml")).unwrap();
        }
        r.build = vec![vec!["rustc".into(), "--invalid-build-option".into()]];
        assert!(prepare_recipe(&root, r).unwrap_err().contains("native project build failed"));
    }

    #[test]
    #[cfg(windows)]
    fn isolate_bundled_pwsh_modules_only_for_windows_powershell_builds() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.runtime/tests/release-powershell").join(std::process::id().to_string());
        fs::create_dir_all(root.join("edition")).unwrap();
        fs::write(root.join("edition/pwsh.exe"), "").unwrap();
        let bundled = root.join("edition/modules");
        let custom = root.join("custom/Modules");
        let inherited = std::env::join_paths([&bundled, &custom]).unwrap();
        for program in [
            "powershell",
            r"C:\Windows\System32\WindowsPowerShell\v1.0\PowerShell.EXE",
        ] {
            let mut req = ProcessRequest::new(program, &root);
            windows_powershell_modules(&mut req, custom.as_os_str()).unwrap();
            assert!(req.env.is_empty() && req.env_remove.is_empty());
            windows_powershell_modules(&mut req, &inherited).unwrap();
            assert_eq!(
                req.env[std::ffi::OsStr::new("PSModulePath")],
                custom.as_os_str()
            );
        }
        for program in ["pwsh.exe", "cargo"] {
            let mut req = ProcessRequest::new(program, &root);
            windows_powershell_modules(&mut req, &inherited).unwrap();
            assert!(req.env.is_empty() && req.env_remove.is_empty());
        }
        let mut req = ProcessRequest::new("powershell.exe", &root);
        windows_powershell_modules(&mut req, bundled.as_os_str()).unwrap();
        assert_eq!(req.env_remove, [std::ffi::OsString::from("PSModulePath")]);
    }

}
