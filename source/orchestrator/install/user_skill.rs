//! BP-owned user skill projection. Source remains in the installed BP home;
//! no Forge/shared-skill registry or user-authored skill is adopted.
use crate::{
    Result, digest, install,
    store::{ordinary, read_json},
};
use bridgeforge_core::{file_lock::FileLock, persistence};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const BINDING: &str = "bridgepilot-local.json";
const FILES: [&str; 5] = [
    "SKILL.md",
    "references/init.md",
    "references/projects.md",
    "references/handoff.md",
    "references/execution.md",
];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Binding {
    schema: u32,
    product: String,
    home: PathBuf,
    skills_root: PathBuf,
    core_version: String,
    files: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    name: String,
    old: Option<Vec<u8>>,
    new: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    binding: Binding,
    changes: Vec<Change>,
}
fn journal(home: &Path) -> PathBuf {
    home.join(".runtime/bridgepilot-skill/journal.json")
}
fn marker(home: &Path) -> PathBuf {
    home.join(".runtime/config/bridgepilot-user-skill.json")
}
fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    ordinary(path)?;
    if !path.exists() {
        return Ok(None);
    }
    let m = fs::metadata(path).map_err(|e| e.to_string())?;
    if !m.is_file() || m.len() > 1024 * 1024 {
        return Err("skill file is not an ordinary bounded file".into());
    }
    fs::read(path).map(Some).map_err(|e| e.to_string())
}
fn roots(home: &Path, skills: &Path) -> Result<(PathBuf, PathBuf)> {
    ordinary(home)?;
    ordinary(skills)?;
    if !home.is_absolute() || !skills.is_absolute() {
        return Err("absolute installed home and Codex skills root required".into());
    }
    let home = fs::canonicalize(home).map_err(|e| e.to_string())?;
    // Canonicalize the existing parent without creating anything during preview.
    let skills = if skills.exists() {
        fs::canonicalize(skills).map_err(|e| e.to_string())?
    } else {
        let parent = skills.parent().ok_or("skills parent missing")?;
        fs::canonicalize(parent)
            .map_err(|e| e.to_string())?
            .join(skills.file_name().ok_or("skills name missing")?)
    };
    let target = skills.join("bridgepilot");
    if target.starts_with(&home) || home.starts_with(&target) {
        return Err("user skill and product home must be separate".into());
    }
    ordinary(&journal(&home))?;
    ordinary(&marker(&home))?;
    ordinary(&skills.join(".bridgepilot.lock"))?;
    if marker(&home).exists() {
        let existing: Value = read_json(&marker(&home))?;
        if existing["schema"] != 1 || existing["skills_root"] != json!(skills) {
            return Err("this BP home is already bound to another Codex skills root".into());
        }
    }
    Ok((home, skills))
}
fn valid(binding: &Binding, home: &Path, skills: &Path) -> Result<()> {
    if binding.schema != 1
        || binding.product != "BridgePilot"
        || binding.home != home
        || binding.skills_root != skills
        || binding.files.len() != FILES.len()
        || FILES.iter().any(|name| !binding.files.contains_key(*name))
    {
        return Err(
            "skill ownership belongs to another installation or has an invalid asset registry"
                .into(),
        );
    }
    Ok(())
}
fn members(target: &Path) -> Result<()> {
    ordinary(target)?;
    if !target.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(target).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        ordinary(&entry.path())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "references" && entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            for child in fs::read_dir(entry.path()).map_err(|e| e.to_string())? {
                let child = child.map_err(|e| e.to_string())?;
                ordinary(&child.path())?;
                let name = format!("references/{}", child.file_name().to_string_lossy());
                if !FILES.contains(&name.as_str())
                    || !child.file_type().map_err(|e| e.to_string())?.is_file()
                {
                    return Err("unowned skill reference preserved".into());
                }
            }
        } else if name != BINDING && name != "SKILL.md" {
            return Err("unowned files in bridgepilot skill are preserved".into());
        }
    }
    Ok(())
}
fn previous(home: &Path, skills: &Path) -> Result<Option<Binding>> {
    let target = skills.join("bridgepilot");
    members(&target)?;
    let Some(body) = read(&target.join(BINDING))? else {
        if target.exists()
            && fs::read_dir(&target)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
        {
            return Err("existing bridgepilot skill has no BP ownership; preserve it".into());
        }
        return Ok(None);
    };
    let binding: Binding = bridgeforge_core::json::parse_unique_json(&body, "skill binding")
        .and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string()))?;
    valid(&binding, home, skills)?;
    for (name, hash) in &binding.files {
        if read(&target.join(name))?
            .as_ref()
            .map(|b| digest(b))
            .as_ref()
            != Some(hash)
        {
            return Err(format!("user-edited skill file preserved: {name}"));
        }
    }
    Ok(Some(binding))
}
fn source(home: &Path, skills: &Path) -> Result<(Binding, BTreeMap<String, Vec<u8>>)> {
    let installation = install::inspect(home)?;
    let mut files = BTreeMap::new();
    let mut hashes = BTreeMap::new();
    for name in FILES {
        let relative = format!("source/orchestrator/skills/bridgepilot/{name}");
        let owned = installation
            .files
            .iter()
            .find(|f| f.target == relative)
            .ok_or("installed package lacks the unified skill; upgrade BP first")?;
        let bytes = read(&home.join(&relative))?.ok_or("installed skill source missing")?;
        if digest(&bytes) != owned.sha256 {
            return Err("installed skill source has user edits".into());
        }
        hashes.insert(name.into(), digest(&bytes));
        files.insert(name.into(), bytes);
    }
    Ok((
        Binding {
            schema: 1,
            product: "BridgePilot".into(),
            home: home.into(),
            skills_root: skills.into(),
            core_version: installation.current.version,
            files: hashes,
        },
        files,
    ))
}
pub fn status(home: &Path, skills: &Path) -> Result<Value> {
    let (home, skills) = roots(home, skills)?;
    if journal(&home).exists() {
        return Err("skill installation pending; use skill-recover".into());
    }
    let old = previous(&home, &skills)?;
    let (current, _) = source(&home, &skills)?;
    Ok(
        json!({"schema":1,"status":if old.as_ref()==Some(&current){"user_skill_current"}else{"user_skill_install_required"},"home":home,"skills_root":skills,"installed":old.is_some(),"current":old.as_ref()==Some(&current),"binding":old,"writes":false}),
    )
}
fn plan(home: &Path, skills: &Path) -> Result<Journal> {
    if home
        .join(".runtime/bridgepilot-install/journal.json")
        .exists()
    {
        return Err("runtime installation pending; use install-recover first".into());
    }
    if journal(home).exists() {
        return Err("skill installation pending; use skill-recover".into());
    }
    previous(home, skills)?;
    let (binding, files) = source(home, skills)?;
    let target = skills.join("bridgepilot");
    // References first, entrypoint next, binding/commit marker last.
    let mut names = FILES[1..].to_vec();
    names.push("SKILL.md");
    let mut changes = Vec::new();
    for name in names {
        changes.push(Change {
            name: name.into(),
            old: read(&target.join(name))?,
            new: files[name].clone(),
        });
    }
    changes.push(Change {
        name: BINDING.into(),
        old: read(&target.join(BINDING))?,
        new: serde_json::to_vec_pretty(&binding).map_err(|e| e.to_string())?,
    });
    Ok(Journal {
        schema: 1,
        binding,
        changes,
    })
}
fn check_journal(j: &Journal, home: &Path, skills: &Path) -> Result<()> {
    valid(&j.binding, home, skills)?;
    if j.schema != 1 || j.changes.len() != FILES.len() + 1 {
        return Err("invalid skill recovery journal".into());
    }
    let mut names = FILES[1..].to_vec();
    names.push("SKILL.md");
    names.push(BINDING);
    for (change, name) in j.changes.iter().zip(names) {
        if change.name != name {
            return Err("invalid skill recovery target".into());
        }
        let expected = if name == BINDING {
            serde_json::to_vec_pretty(&j.binding).map_err(|e| e.to_string())?
        } else {
            if j.binding.files.get(name) != Some(&digest(&change.new)) {
                return Err("skill journal body mismatch".into());
            }
            change.new.clone()
        };
        if change.new != expected {
            return Err("skill recovery binding mismatch".into());
        }
    }
    Ok(())
}
pub fn install(home: &Path, skills: &Path, preview: bool) -> Result<Value> {
    install_before_lock(home, skills, preview, || {})
}
#[cfg(feature = "test-fixtures")]
pub fn install_with_before_lock(home: &Path, skills: &Path, hook: impl FnOnce()) -> Result<Value> {
    install_before_lock(home, skills, false, hook)
}
fn install_before_lock(
    home: &Path,
    skills: &Path,
    preview: bool,
    before_lock: impl FnOnce(),
) -> Result<Value> {
    let (home, skills) = roots(home, skills)?;
    let before = plan(&home, &skills)?;
    if preview {
        return Ok(
            json!({"schema":1,"status":"user_skill_planned","target":skills.join("bridgepilot"),"files":FILES,"writes":false}),
        );
    }
    fs::create_dir_all(&skills).map_err(|e| e.to_string())?;
    fs::create_dir_all(home.join(".runtime/bridgepilot-install/locks"))
        .map_err(|e| e.to_string())?;
    before_lock();
    let _global = FileLock::acquire(&skills.join(".bridgepilot.lock"))?;
    let _home = FileLock::acquire(&home.join(".runtime/bridgepilot-install/locks/writer.lock"))?;
    let current = plan(&home, &skills)?;
    roots(&home, &skills)?;
    let marker_before = read(&marker(&home))?;
    if serde_json::to_value(&before).map_err(|e| e.to_string())?
        != serde_json::to_value(&current).map_err(|e| e.to_string())?
    {
        return Err("skill inputs changed before apply".into());
    }
    if previous(&home, &skills)?.as_ref() == Some(&current.binding) {
        return status(&home, &skills);
    }
    persistence::atomic_write_json(&journal(&home), &current).map_err(|e| e.to_string())?;
    apply(&home, &skills, &current, marker_before.as_deref())?;
    Ok(
        json!({"schema":1,"status":"user_skill_installed","home":home,"target":skills.join("bridgepilot"),"version":current.binding.core_version,"shared_forge_skills_changed":false}),
    )
}
fn apply(home: &Path, skills: &Path, j: &Journal, marker_before: Option<&[u8]>) -> Result<()> {
    let target = skills.join("bridgepilot");
    for change in &j.changes {
        let path = target.join(&change.name);
        if read(&path)? != change.old {
            return Err(
                "skill target changed during installation; preserve journal and use skill-recover"
                    .into(),
            );
        }
        persistence::atomic_write(&path, &change.new).map_err(|e| e.to_string())?;
    }
    previous(home, skills)?;
    if source(home, skills)?.0 != j.binding {
        return Err("installed skill source changed during projection; preserve journal".into());
    }
    if read(&marker(home))?.as_deref() != marker_before {
        return Err("skill ownership marker edited during projection; preserve journal".into());
    }
    persistence::atomic_write_json(&marker(home), &json!({"schema":1,"skills_root":skills}))
        .map_err(|e| e.to_string())?;
    fs::remove_file(journal(home)).map_err(|e| e.to_string())
}
pub fn recover(home: &Path, skills: &Path) -> Result<Value> {
    recover_with(home, skills, || {})
}
fn recover_with(home: &Path, skills: &Path, before_restore: impl FnOnce()) -> Result<Value> {
    let (home, skills) = roots(home, skills)?;
    if !journal(&home).exists() {
        return Ok(json!({"status":"user_skill_recovery_not_needed"}));
    }
    let _global = FileLock::acquire(&skills.join(".bridgepilot.lock"))?;
    let _home = FileLock::acquire(&home.join(".runtime/bridgepilot-install/locks/writer.lock"))?;
    roots(&home, &skills)?;
    let marker_before = read(&marker(&home))?;
    if home
        .join(".runtime/bridgepilot-install/journal.json")
        .exists()
    {
        return Err("runtime installation pending; use install-recover first".into());
    }
    let j: Journal = read_json(&journal(&home))?;
    check_journal(&j, &home, &skills)?;
    let target = skills.join("bridgepilot");
    members(&target)?;
    let mut committed = true;
    let mut observed = Vec::new();
    for change in &j.changes {
        let actual = read(&target.join(&change.name))?;
        if actual.as_ref() != Some(&change.new) {
            committed = false;
        }
        if actual != change.old && actual.as_ref() != Some(&change.new) {
            return Err("skill recovery found external edits; preserve all files".into());
        }
        observed.push(actual);
    }
    if committed {
        previous(&home, &skills)?;
        if source(&home, &skills)?.0 != j.binding {
            return Err("installed source changed before skill commit recovery".into());
        }
        if read(&marker(&home))? != marker_before {
            return Err("skill marker edited during recovery; preserve journal".into());
        }
        persistence::atomic_write_json(&marker(&home), &json!({"schema":1,"skills_root":skills}))
            .map_err(|e| e.to_string())?;
    } else {
        before_restore();
        for (change, observed) in j.changes.iter().zip(observed).rev() {
            if read(&marker(&home))? != marker_before {
                return Err("skill marker edited during rollback; preserve journal".into());
            }
            let path = target.join(&change.name);
            if read(&path)? != observed {
                return Err(
                    "skill target edited during rollback; preserve edits and journal".into(),
                );
            }
            match &change.old {
                Some(body) => persistence::atomic_write(&path, body).map_err(|e| e.to_string())?,
                None => {
                    if path.exists() {
                        fs::remove_file(&path).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }
    fs::remove_file(journal(&home)).map_err(|e| e.to_string())?;
    Ok(
        json!({"status":if committed{"user_skill_commit_recovered"}else{"user_skill_rollback_recovered"},"target":target}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(old_present: bool, complete: bool) -> (PathBuf, PathBuf, PathBuf, Journal) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/bridgepilot-user-skill")
            .join(crate::new_id("recovery"));
        let home = root.join("home");
        let skills = root.join("profile");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&skills).unwrap();
        let (home, skills) = roots(&home, &skills).unwrap();
        let target = skills.join("bridgepilot");
        let mut hashes = BTreeMap::new();
        let mut changes = Vec::new();
        let mut owned = Vec::new();
        let mut names = FILES[1..].to_vec();
        names.push("SKILL.md");
        for name in names {
            let old = old_present.then(|| format!("old {name}").into_bytes());
            let new = format!("new {name}").into_bytes();
            hashes.insert(name.into(), digest(&new));
            let relative = format!("source/orchestrator/skills/bridgepilot/{name}");
            persistence::atomic_write(&home.join(&relative), &new).unwrap();
            owned.push(install::OwnedFile {
                id: name.into(),
                target: relative,
                sha256: digest(&new),
            });
            changes.push(Change {
                name: name.into(),
                old,
                new,
            });
        }
        let binding = Binding {
            schema: 1,
            product: "BridgePilot".into(),
            home: home.clone(),
            skills_root: skills.clone(),
            core_version: "0.6.0".into(),
            files: hashes,
        };
        let mut old_binding = binding.clone();
        for change in &changes {
            if let Some(body) = &change.old {
                old_binding.files.insert(change.name.clone(), digest(body));
            }
        }
        changes.push(Change {
            name: BINDING.into(),
            old: old_present.then(|| serde_json::to_vec_pretty(&old_binding).unwrap()),
            new: serde_json::to_vec_pretty(&binding).unwrap(),
        });
        for (i, change) in changes.iter().enumerate() {
            let selected = if complete || i == 0 {
                Some(&change.new)
            } else {
                change.old.as_ref()
            };
            if let Some(bytes) = selected {
                persistence::atomic_write(&target.join(&change.name), bytes).unwrap();
            }
        }
        let core = home.join(".build/core.exe");
        let launcher = home.join(".build/bridgepilot/bin/bp.exe");
        persistence::atomic_write(&core, b"fixture core").unwrap();
        persistence::atomic_write(&launcher, b"fixture launcher").unwrap();
        let installed = install::Installation {
            schema: 1,
            product: "BridgePilot".into(),
            current: install::Version {
                version: "0.6.0".into(),
                executable: ".build/core.exe".into(),
                sha256: digest(b"fixture core"),
            },
            previous: None,
            files: owned,
            launcher_sha256: digest(b"fixture launcher"),
        };
        persistence::atomic_write_json(
            &home.join(".runtime/config/bridgepilot-installation.json"),
            &installed,
        )
        .unwrap();
        let j = Journal {
            schema: 1,
            binding,
            changes,
        };
        persistence::atomic_write_json(&journal(&home), &j).unwrap();
        (root, home, skills, j)
    }
    #[test]
    fn partial_projection_rolls_back_only_owned_bytes_and_completed_projection_commits() {
        for old in [false, true] {
            let (_root, home, skills, j) = fixture(old, false);
            assert_eq!(
                recover(&home, &skills).unwrap()["status"],
                "user_skill_rollback_recovered"
            );
            for change in j.changes {
                assert_eq!(
                    read(&skills.join("bridgepilot").join(change.name)).unwrap(),
                    change.old
                );
            }
            assert!(!journal(&home).exists());
        }
        let (_root, home, skills, _) = fixture(false, true);
        assert_eq!(
            recover(&home, &skills).unwrap()["status"],
            "user_skill_commit_recovered"
        );
        assert_eq!(status(&home, &skills).unwrap()["current"], true);
    }
    #[test]
    fn rollback_rechecks_each_file_after_observation_before_any_replacement() {
        let (_root, home, skills, _) = fixture(true, false);
        let edited = skills.join("bridgepilot/references/handoff.md");
        assert!(
            recover_with(&home, &skills, || fs::write(&edited, b"external editor")
                .unwrap())
            .is_err()
        );
        assert_eq!(fs::read(edited).unwrap(), b"external editor");
        assert!(journal(&home).exists());
    }
    #[test]
    fn recovery_rejects_unknown_files_and_runtime_pending_without_deleting_either() {
        let (_root, home, skills, _) = fixture(false, false);
        let custom = skills.join("bridgepilot/custom.md");
        fs::write(&custom, b"user file").unwrap();
        assert!(recover(&home, &skills).is_err());
        assert_eq!(fs::read(&custom).unwrap(), b"user file");
        fs::remove_file(&custom).unwrap();
        let runtime = home.join(".runtime/bridgepilot-install/journal.json");
        persistence::atomic_write(&runtime, b"pending runtime").unwrap();
        assert!(recover(&home, &skills).is_err());
        assert!(journal(&home).exists());
        assert!(runtime.exists());
    }
}
