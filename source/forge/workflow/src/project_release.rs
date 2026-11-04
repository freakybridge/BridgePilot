//! Project-owned native recipe interpreted by the installed, provenance-bound Rust driver.
//! Versions stay in native files; this recipe never stores a version ledger.
use bridgeforge_core::native_release::{
    Asset, Plan, SourceContext, SourceEdit, SourceIdentity, SourcePlan, raw_sha,
};
use bridgeforge_core::{
    ProcessRequest, ProcessRunner, SemVer, SystemProcessRunner, managed_paths, persistence,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
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
    native["source-args"] = array(&["project-release".into(), "source".into()]);
    native["prepare-timeout-seconds"] = value((r.build.len() as i64) * 3600 + 300);
    if let Some(old) = metadata.get("native-release").and_then(Item::as_table) {
        if old.len() != native.len()
            || old.get("program").and_then(Item::as_str) != Some(PROGRAM)
            || old
                .get("prepare-timeout-seconds")
                .and_then(Item::as_integer)
                != native
                    .get("prepare-timeout-seconds")
                    .and_then(Item::as_integer)
            || ["build", "args", "source-args"]
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

pub fn identity(root: &Path) -> Result<SourceIdentity, String> {
    let r = recipe(root)?.ok_or("native project release recipe missing")?;
    let version =
        fs::read_to_string(source_path(root, &r.version_file)?).map_err(|e| e.to_string())?;
    let files = inventory(root, &r)?;
    let packages = packages(root, &files)?;
    let mut paths = BTreeSet::from([r.version_file.clone(), r.changelog.clone()]);
    for p in &packages {
        paths.insert(p["manifest"].as_str().unwrap().into());
        paths.insert(p["version_manifest"].as_str().unwrap().into());
    }
    paths.extend(
        files
            .iter()
            .filter(|p| p.starts_with("source/") && p.ends_with("/Cargo.lock"))
            .cloned(),
    );
    Ok(SourceIdentity {
        product: r.product,
        repository: r.repository,
        gitlab_origin: r.gitlab_origin,
        version: version.trim().into(),
        tag_prefix: r.tag_prefix,
        paths: paths.into_iter().collect(),
    })
}

fn package_edits(
    root: &Path,
    r: &Recipe,
    reference: &str,
    changed: &BTreeSet<&str>,
    bump: u8,
) -> Result<(Vec<SourceEdit>, Vec<String>), String> {
    let files = inventory(root, r)?;
    let packages = packages(root, &files)?;
    let old_paths = git(
        root,
        &[
            "ls-tree",
            "-r",
            "--name-only",
            "-z",
            reference,
            "--",
            "source",
        ],
    )?;
    let old_paths = old_paths
        .split(|b| *b == 0)
        .filter_map(|p| std::str::from_utf8(p).ok())
        .collect::<BTreeSet<_>>();
    let mut owners = BTreeMap::<String, (String, String, bool)>::new();
    let choose = |p: &Value,
                  owners: &mut BTreeMap<String, (String, String, bool)>,
                  level: u8|
     -> Result<bool, String> {
        let manifest = p["manifest"].as_str().unwrap();
        let owner = p["version_manifest"].as_str().unwrap();
        let _ = manifest;
        if owners.contains_key(owner) || !old_paths.contains(owner) {
            return Ok(false);
        }
        let inherited = p["workspace_version"] == true;
        let old = String::from_utf8(git(root, &["show", &format!("{reference}:{owner}")])?)
            .map_err(|e| e.to_string())?
            .parse::<DocumentMut>()
            .map_err(|e| e.to_string())?;
        let value = if inherited {
            old.get("workspace").and_then(|v| v.get("package"))
        } else {
            old.get(if owner.ends_with("pyproject.toml") {
                "project"
            } else {
                "package"
            })
        };
        let Some(old) = value.and_then(|v| v.get("version")).and_then(Item::as_str) else {
            return Ok(false);
        };
        let current = p["version"].as_str().unwrap();
        let old_semver = old.parse::<SemVer>()?;
        let current_semver = current.parse::<SemVer>()?;
        if current_semver < old_semver {
            return Err(format!("native package version regressed: {owner}"));
        }
        let selected = if current_semver == old_semver {
            increment(current_semver, level)?.to_string()
        } else {
            current.into()
        };
        owners.insert(owner.into(), (old.into(), selected, inherited));
        Ok(true)
    };
    for p in &packages {
        let manifest = p["manifest"].as_str().unwrap();
        let prefix = format!("{}/", manifest.rsplit_once('/').unwrap().0);
        if changed.iter().any(|c| c.starts_with(&prefix)) {
            choose(p, &mut owners, bump)?;
        }
    }
    loop {
        let mut added = false;
        let pins = packages
            .iter()
            .filter_map(|p| {
                owners
                    .get(p["version_manifest"].as_str().unwrap())
                    .map(|(old, new, _)| {
                        (
                            p["name"].as_str().unwrap().to_string(),
                            old.clone(),
                            new.clone(),
                        )
                    })
            })
            .collect::<Vec<_>>();
        for p in packages
            .iter()
            .filter(|p| p["manifest"].as_str().unwrap().ends_with("pyproject.toml"))
        {
            let rel = p["manifest"].as_str().unwrap();
            let doc = fs::read_to_string(source_path(root, rel)?)
                .map_err(|e| e.to_string())?
                .parse::<DocumentMut>()
                .map_err(|e| e.to_string())?;
            let depends = doc
                .get("project")
                .and_then(|v| v.get("dependencies"))
                .and_then(Item::as_array)
                .is_some_and(|deps| {
                    deps.iter().filter_map(|v| v.as_str()).any(|dep| {
                        let compact = dep.replace(' ', "").replace('_', "-");
                        pins.iter().any(|(name, old, new)| {
                            old != new && compact == format!("{}=={old}", name.replace('_', "-"))
                        })
                    })
                });
            if depends {
                added |= choose(p, &mut owners, 1)?;
            }
        }
        if !added {
            break;
        }
    }
    let mut documents = BTreeMap::<String, (String, DocumentMut)>::new();
    for owner in owners.keys() {
        let before = fs::read_to_string(source_path(root, owner)?).map_err(|e| e.to_string())?;
        let mut doc = before.parse::<DocumentMut>().map_err(|e| e.to_string())?;
        let (_, new, inherited) = &owners[owner];
        if *inherited {
            doc["workspace"]["package"]["version"] = value(new.as_str());
        } else {
            doc[if owner.ends_with("pyproject.toml") {
                "project"
            } else {
                "package"
            }]["version"] = value(new.as_str());
        }
        documents.insert(owner.clone(), (before, doc));
    }
    let mut versions = BTreeMap::<String, (String, String)>::new();
    let mut notes = vec![];
    for p in &packages {
        if let Some((old, new, _)) = owners.get(p["version_manifest"].as_str().unwrap()) {
            let name = p["name"].as_str().unwrap();
            versions.insert(name.into(), (old.clone(), new.clone()));
            notes.push(format!(
                "- `{name}`: {old} → {new} (changed native inputs)."
            ));
        }
    }
    // Update local Cargo lock identities and exact Python internal pins only;
    // registry packages and unrelated constraints are never rewritten.
    for rel in files.iter().filter(|p| {
        p.starts_with("source/") && (p.ends_with("/Cargo.lock") || p.ends_with("/pyproject.toml"))
    }) {
        let before = fs::read_to_string(source_path(root, rel)?).map_err(|e| e.to_string())?;
        let mut doc = documents
            .get(rel)
            .map(|(_, d)| d.clone())
            .unwrap_or(before.parse::<DocumentMut>().map_err(|e| e.to_string())?);
        if rel.ends_with("Cargo.lock") {
            if let Some(packages) = doc
                .get_mut("package")
                .and_then(Item::as_array_of_tables_mut)
            {
                for package in packages.iter_mut() {
                    if package.get("source").is_none() {
                        if let Some((old, new)) = package
                            .get("name")
                            .and_then(Item::as_str)
                            .and_then(|name| versions.get(name))
                        {
                            if package.get("version").and_then(Item::as_str) == Some(old.as_str()) {
                                package["version"] = value(new.as_str());
                            }
                        }
                    }
                    if let Some(deps) = package.get_mut("dependencies").and_then(Item::as_array_mut)
                    {
                        for dep in deps.iter_mut() {
                            if let Some(s) = dep.as_str() {
                                if let Some((name, (_old, new))) = versions
                                    .iter()
                                    .find(|(name, (old, _))| s == format!("{name} {old}"))
                                {
                                    *dep = toml_edit::Value::from(format!("{name} {new}"));
                                }
                            }
                        }
                    }
                }
            }
        } else if let Some(deps) = doc
            .get_mut("project")
            .and_then(|p| p.get_mut("dependencies"))
            .and_then(Item::as_array_mut)
        {
            for dep in deps.iter_mut() {
                if let Some(s) = dep.as_str() {
                    let compact = s.replace(' ', "");
                    if let Some((name, (old, new))) = versions.iter().find(|(name, (old, _))| {
                        compact == format!("{name}=={old}")
                            || compact == format!("{}=={old}", name.replace('_', "-"))
                    }) {
                        let _ = old;
                        *dep = toml_edit::Value::from(format!("{name}=={new}"));
                    }
                }
            }
        }
        if doc.to_string() != before {
            documents.insert(rel.clone(), (before, doc));
        }
    }
    let edits = documents
        .into_iter()
        .filter_map(|(path, (before, doc))| {
            let after = doc.to_string();
            (before != after).then_some(SourceEdit {
                path,
                before,
                after,
            })
        })
        .collect();
    Ok((edits, notes))
}

fn level(message: &str) -> Result<u8, String> {
    let subject = message.lines().next().unwrap_or("");
    let kind = subject
        .split_once(':')
        .ok_or("code changes require a classified commit message")?
        .0;
    let breaking =
        kind.ends_with('!') || message.lines().any(|s| s.starts_with("BREAKING CHANGE:"));
    match kind.trim_end_matches('!').split('(').next().unwrap() {
        "feat" => Ok(if breaking { 3 } else { 2 }),
        "fix" | "perf" | "refactor" | "chore" | "build" | "test" | "ci" | "style" | "revert"
        | "docs" => Ok(if breaking { 3 } else { 1 }),
        _ => Err("unknown native release change classification".into()),
    }
}

fn increment(mut v: SemVer, level: u8) -> Result<SemVer, String> {
    if level == 3 && v.major > 0 {
        v.major = v.major.checked_add(1).ok_or("version overflow")?;
        v.minor = 0;
        v.patch = 0;
    } else if level >= 2 {
        v.minor = v.minor.checked_add(1).ok_or("version overflow")?;
        v.patch = 0;
    } else {
        v.patch = v.patch.checked_add(1).ok_or("version overflow")?;
    }
    Ok(v)
}

fn code(path: &str) -> bool {
    if path.starts_with("doc/")
        || matches!(
            path,
            "VERSION" | "CHANGELOG.md" | "README.md" | "INSTALL.md" | "AGENTS.md"
        )
    {
        return false;
    }
    path.starts_with("source/") || path.starts_with(".codex/") || !path.ends_with(".md")
}

pub fn source(root: &Path, context: Option<&SourceContext>) -> Result<Value, String> {
    let mut id = identity(root)?;
    let Some(context) = context else {
        return serde_json::to_value(id).map_err(|e| e.to_string());
    };
    if context.schema != 1 || context.identity != id {
        return Err("native source identity changed".into());
    }
    let r = recipe(root)?.unwrap();
    let current = id.version.parse::<SemVer>()?;
    let base = context
        .baseline
        .as_ref()
        .map(|b| b.version.parse::<SemVer>())
        .transpose()?;
    let initial_base = if base.is_none() {
        String::from_utf8(git(root, &["show", &format!("HEAD:{}", r.version_file)])?)
            .map_err(|e| e.to_string())?
            .trim()
            .parse::<SemVer>()?
    } else {
        current
    };
    if base.is_some_and(|b| current < b) {
        return Err("native version is below the formal release baseline".into());
    }
    let reference = context
        .baseline
        .as_ref()
        .map(|b| b.commit.as_str())
        .unwrap_or("HEAD");
    let mut changed = git(
        root,
        &["diff", "--no-renames", "--name-only", "-z", reference, "--"],
    )?;
    changed.extend(git(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?);
    let code_paths = changed
        .split(|c| *c == 0)
        .filter_map(|p| std::str::from_utf8(p).ok())
        .filter(|p| !p.is_empty() && code(p))
        .collect::<BTreeSet<_>>();
    let mut reuse = None;
    let mut selected = current;
    let mut message = String::new();
    let mut package_changes = Vec::new();
    if code_paths.is_empty() {
        if let Some(b) = &context.baseline {
            if current == b.version.parse::<SemVer>()? {
                reuse = Some(b.clone());
            }
        }
    } else {
        let mut messages = Vec::new();
        if let Some(b) = &context.baseline {
            let range = format!("{}..HEAD", b.commit);
            let mut args = vec!["log", "--format=%B%x00", range.as_str(), "--"];
            args.extend(code_paths.iter().copied());
            messages.extend(
                String::from_utf8(git(root, &args)?)
                    .map_err(|e| e.to_string())?
                    .split('\0')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            );
        }
        let dirty = git(root, &["status", "--porcelain"])?;
        if !dirty.is_empty() {
            messages.push(
                context
                    .message
                    .as_deref()
                    .ok_or("native release with uncommitted code requires a classified message")?
                    .into(),
            );
        }
        if !messages.is_empty() {
            let bump = messages
                .iter()
                .map(|s| level(s))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .max()
                .unwrap();
            selected = current.max(increment(base.unwrap_or(initial_base), bump)?);
            message = messages
                .iter()
                .map(|s| format!("- {}", s.lines().next().unwrap_or("")))
                .collect::<Vec<_>>()
                .join("\n");
            let (edits, notes) = package_edits(root, &r, reference, &code_paths, bump)?;
            package_changes = edits;
            if !notes.is_empty() {
                message.push_str("\n\nNative package versions:\n");
                message.push_str(&notes.join("\n"));
            }
        }
    }
    id.version = selected.to_string();
    if reuse.is_some() {
        return serde_json::to_value(SourcePlan {
            schema: 1,
            identity: id,
            edits: vec![],
            reuse_release: reuse,
            release_assets: vec![
                "source.tar".into(),
                "versions.json".into(),
                "SHA256SUMS.txt".into(),
            ],
        })
        .map_err(|e| e.to_string());
    }
    let mut edits = package_changes;
    if selected != current {
        let before =
            fs::read_to_string(source_path(root, &r.version_file)?).map_err(|e| e.to_string())?;
        edits.push(SourceEdit {
            path: r.version_file.clone(),
            before,
            after: format!("{selected}\n"),
        });
    }
    let before = fs::read_to_string(source_path(root, &r.changelog)?).map_err(|e| e.to_string())?;
    let heading = format!("## [{selected}]");
    let mut after = before.clone();
    if !before
        .lines()
        .any(|s| s == heading || s.starts_with(&format!("{heading} - ")))
    {
        let at = before.find("\n## ").unwrap_or(before.len());
        if message.is_empty() {
            message = "- Native release of the current project source.".into();
        }
        after.insert_str(at, &format!("\n{heading}\n\n{message}\n"));
    }
    if after != before {
        edits.push(SourceEdit {
            path: r.changelog,
            before,
            after,
        });
    }
    serde_json::to_value(SourcePlan {
        schema: 1,
        identity: id,
        edits,
        reuse_release: None,
        release_assets: vec![],
    })
    .map_err(|e| e.to_string())
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
    let id = identity(&root)?;
    let input = inventory(&root, &r)?;
    let snapshot = input
        .iter()
        .map(|p| {
            Ok((
                p.clone(),
                raw_sha(&fs::read(source_path(&root, p)?).map_err(|e| e.to_string())?),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    for argv in &r.build {
        let mut q = ProcessRequest::new(&argv[0], &root);
        q.args = argv[1..].iter().map(Into::into).collect();
        q.timeout = Duration::from_secs(3600);
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
    let mut archive_hashes = snapshot.clone();
    for rel in &r.artifacts {
        let path = managed_paths::safe_join(&root, rel, "native artifact")?;
        if !path.is_file() {
            return Err(format!("native build did not produce {rel}"));
        }
        paths.push(rel.clone());
        archive_hashes.insert(
            rel.clone(),
            raw_sha(&fs::read(path).map_err(|e| e.to_string())?),
        );
    }
    paths.sort();
    paths.dedup();
    archive::write(&root, &paths, &archive_hashes, &output.join("source.tar"))?;
    let manifest = json!({"schema":1,"product":id.product,"version":id.version,"repository":id.repository,"gitlab_origin":id.gitlab_origin,
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
    if inventory(&root, &r)? != input {
        return Err("project source inventory changed during packaging".into());
    }
    for (rel, sha) in &snapshot {
        if &raw_sha(&fs::read(source_path(&root, rel)?).map_err(|e| e.to_string())?) != sha {
            return Err(format!("source changed during packaging: {rel}"));
        }
    }
    let notes = fs::read_to_string(source_path(&root, &r.changelog)?).map_err(|e| e.to_string())?;
    let heading = format!("## [{}]", id.version);
    let at = notes.find(&heading).ok_or("native release notes missing")?;
    let rest = &notes[at..];
    let end = rest[heading.len()..]
        .find("\n## ")
        .map(|n| n + heading.len())
        .unwrap_or(rest.len());
    let assets = ["source.tar", "versions.json", "SHA256SUMS.txt"]
        .iter()
        .map(|name| {
            let bytes = fs::read(output.join(name)).map_err(|e| e.to_string())?;
            Ok(Asset {
                name: (*name).into(),
                path: format!("{relative}/{name}"),
                sha256: raw_sha(&bytes),
                size: bytes.len() as u64,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let plan = Plan {
        schema: 1,
        product: id.product.clone(),
        version: id.version.clone(),
        repository: id.repository,
        gitlab_origin: id.gitlab_origin,
        tag: format!("{}{}", id.tag_prefix, id.version),
        title: format!("{} {}", id.product, id.version),
        notes: rest[..end].trim().into(),
        assets,
    };
    plan.validate(&root)?;
    serde_json::to_value(plan).map_err(|e| e.to_string())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/project_release.rs"]
mod tests;
