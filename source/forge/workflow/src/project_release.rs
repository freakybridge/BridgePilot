//! Project-owned native recipe interpreted by the installed, provenance-bound Rust driver.
//! Versions stay in native files; this recipe never stores a version ledger.
use bridgeforge_core::native_release::{
    Asset, Plan, raw_sha,
};
use bridgeforge_core::{
    ProcessRequest, ProcessRunner, SemVer, SystemProcessRunner, managed_paths, persistence,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use toml_edit::{Array, DocumentMut, Item, Table, value};

#[path = "project_release_archive.rs"]
mod archive;

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
    pub include: Vec<String>,
    #[serde(default)]
    pub build: Vec<Vec<String>>,
    #[serde(default)]
    pub artifacts: Vec<String>,
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
        include: strings(table, "include")?,
        build,
        artifacts: strings(table, "artifacts")?,
    };
    validate(root, &r)?;
    Ok(Some(r))
}

pub fn validate(root: &Path, r: &Recipe) -> Result<(), String> {
    if r.schema != 1
        || r.product.trim().is_empty()
        || r.include.is_empty()
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
    for rel in &r.include {
        source_path(root, rel)?;
    }
    let mut seen = BTreeSet::new();
    for rel in &r.artifacts {
        if !rel.starts_with(".build/") || !seen.insert(rel) {
            return Err("artifact paths must be explicit, unique .build files".into());
        }
        managed_paths::safe_join(root, rel, "native artifact")?;
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
    version.trim().parse::<SemVer>()?;
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
    t["include"] = array(&r.include);
    t["artifacts"] = array(&r.artifacts);
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
    bridgeforge_core::build_identity::require()?;
    let r = recipe(root)?.ok_or("project release recipe missing")?;
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
        || proof["build_identity"] != bridgeforge_core::build_identity::identity()
        || proof["recipe_sha256"] != raw_sha(&serde_json::to_vec(&r).map_err(|e| e.to_string())?)
    {
        return Err(
            "project release driver or recipe changed; repair through skeleton installation".into(),
        );
    }
    Ok(
        json!({"schema":1,"status":"ready","product":r.product,"repository":r.repository,"gitlab_origin":r.gitlab_origin,"program":PROGRAM}),
    )
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut q = ProcessRequest::new("git", root);
    q.args = args.iter().map(Into::into).collect();
    q.timeout = Duration::from_secs(60);
    q.env.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
    let out = SystemProcessRunner.run(&q).map_err(|e| e.to_string())?;
    if out.code != 0 || out.timed_out {
        return Err(format!(
            "native source query failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out.stdout)
}

fn inventory(root: &Path, r: &Recipe) -> Result<Vec<String>, String> {
    let raw = git(
        root,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    )?;
    let mut result = BTreeSet::new();
    for raw in raw.split(|c| *c == 0).filter(|s| !s.is_empty()) {
        let path = std::str::from_utf8(raw).map_err(|e| e.to_string())?;
        if !r
            .include
            .iter()
            .any(|p| path == p || path.starts_with(&format!("{p}/")))
        {
            continue;
        }
        let file = source_path(root, path)?;
        if file.is_file() {
            result.insert(path.to_owned());
        }
    }
    for needed in [&r.version_file, &r.changelog] {
        if !result.contains(needed) {
            return Err(format!("native release include list omits {needed}"));
        }
    }
    Ok(result.into_iter().collect())
}

fn packages(root: &Path, files: &[String]) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    for rel in files.iter().filter(|p| {
        p.starts_with("source/") && (p.ends_with("/Cargo.toml") || p.ends_with("/pyproject.toml"))
    }) {
        let path = source_path(root, rel)?;
        let doc = fs::read_to_string(&path)
            .map_err(|e| e.to_string())?
            .parse::<DocumentMut>()
            .map_err(|e| e.to_string())?;
        let key = if rel.ends_with("/Cargo.toml") {
            "package"
        } else {
            "project"
        };
        let Some(package) = doc.get(key) else {
            continue;
        };
        let Some(name) = package.get("name").and_then(Item::as_str) else {
            continue;
        };
        let mut version = package
            .get("version")
            .and_then(Item::as_str)
            .map(str::to_owned);
        let mut owner = rel.clone();
        let mut inherited = false;
        if version.is_none()
            && package
                .get("version")
                .and_then(|v| v.get("workspace"))
                .and_then(Item::as_bool)
                == Some(true)
        {
            for parent in path
                .parent()
                .unwrap()
                .ancestors()
                .take_while(|p| p.starts_with(root))
            {
                let manifest = parent.join("Cargo.toml");
                if !manifest.is_file() {
                    continue;
                }
                let workspace = fs::read_to_string(manifest)
                    .map_err(|e| e.to_string())?
                    .parse::<DocumentMut>()
                    .map_err(|e| e.to_string())?;
                if let Some(v) = workspace
                    .get("workspace")
                    .and_then(|v| v.get("package"))
                    .and_then(|v| v.get("version"))
                    .and_then(Item::as_str)
                {
                    version = Some(v.into());
                    owner = parent
                        .join("Cargo.toml")
                        .strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .replace('\\', "/");
                    inherited = true;
                    break;
                }
            }
        }
        let version = version
            .ok_or_else(|| format!("native package version is not static/resolvable: {rel}"))?;
        result.push(json!({"name":name,"version":version,"manifest":rel,"version_manifest":owner,"workspace_version":inherited}));
    }
    Ok(result)
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

pub fn prepare(root: &Path, output: &Path) -> Result<Value, String> {
    verify_driver(root)?;
    let supplied = std::path::absolute(root).map_err(|e| e.to_string())?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let relative = output
        .strip_prefix(&root)
        .or_else(|_| output.strip_prefix(&supplied))
        .map_err(|_| "release output is outside project")?
        .to_str()
        .ok_or("release output must be UTF-8")?
        .replace('\\', "/");
    if !relative.starts_with(".build/") {
        return Err("release output must be in .build".into());
    }
    let output = managed_paths::safe_join(&root, &relative, "release output")?;
    if output.exists() {
        return Err("release output already exists; preserve previous artifact".into());
    }
    let r = recipe(&root)?.unwrap();
    let version = fs::read_to_string(source_path(&root, &r.version_file)?).map_err(|e| e.to_string())?;
    let version = version.trim();
    let input = inventory(&root, &r)?;
    for argv in &r.build {
        let mut q = ProcessRequest::new(&argv[0], &root);
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
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let mut paths = input.clone();
    paths.extend(r.artifacts.iter().cloned());
    paths.sort();
    paths.dedup();
    let mut snapshot = archive::write(&root, &paths, &output.join("source.tar"))?;
    snapshot.retain(|path,_| input.binary_search(path).is_ok());
    let manifest = json!({"schema":1,"product":r.product,"version":version,"repository":r.repository,"gitlab_origin":r.gitlab_origin,
        "source_files":snapshot,"packages":packages(&root,&input)?,"usage":"Use the existing project checkout and startup entry; this is not a standalone installer."});
    persistence::atomic_write(
        &output.join("versions.json"),
        &serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut sums = String::new();
    for name in ["source.tar", "versions.json"] {
        sums.push_str(&format!(
            "{}  {name}\n",
            &raw_sha(&fs::read(output.join(name)).map_err(|e| e.to_string())?)[7..]
        ));
    }
    persistence::atomic_write(&output.join("SHA256SUMS.txt"), sums.as_bytes())
        .map_err(|e| e.to_string())?;
    let notes = fs::read_to_string(source_path(&root, &r.changelog)?).map_err(|e| e.to_string())?;
    let heading = format!("## [{}]", version);
    let at = notes.find(&heading).ok_or("native release notes missing")?;
    let rest = &notes[at..];
    let end = rest[heading.len()..]
        .find("\n## ")
        .map(|n| n + heading.len())
        .unwrap_or(rest.len());
    let assets = ["source.tar", "versions.json", "SHA256SUMS.txt"].into_iter()
        .map(|name| Asset { name:name.into(), path:format!("{relative}/{name}") }).collect();
    let plan = Plan {
        schema: 1,
        product: r.product.clone(),
        version: version.to_owned(),
        repository: r.repository,
        gitlab_origin: r.gitlab_origin,
        tag: format!("{}{}", r.tag_prefix, version),
        title: format!("{} {}", r.product, version),
        notes: rest[..end].trim().into(),
        assets,
    };
    serde_json::to_value(plan).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
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
