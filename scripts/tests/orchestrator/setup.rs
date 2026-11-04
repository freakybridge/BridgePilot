use super::*;
use bridgeforge_core::{ProcessOutput, ProcessRequest, ProcessRunner};
use bridgepilot_install::environment;
use bridgepilot_install::install;
use bridgepilot_install::user_skill;
use serde_json::{Value, json};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct Installer {
    git: AtomicBool,
    gh: AtomicBool,
    calls: AtomicUsize,
    fail: bool,
    timeout: bool,
    stale_path: bool,
    requests: Mutex<Vec<Vec<String>>>,
}
impl Installer {
    fn new(ready: bool) -> Self {
        Self {
            git: AtomicBool::new(ready),
            gh: AtomicBool::new(ready),
            calls: AtomicUsize::new(0),
            fail: false,
            timeout: false,
            stale_path: false,
            requests: Mutex::new(vec![]),
        }
    }
}
impl ProcessRunner for Installer {
    fn run(&self, q: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        if q.program == "git" || q.program == "gh" {
            let git = q.program == "git";
            let ready = if git {
                self.git.load(Ordering::SeqCst)
            } else {
                self.gh.load(Ordering::SeqCst)
            };
            return Ok(ProcessOutput {
                code: if ready { 0 } else { 1 },
                stdout: if ready {
                    if git {
                        b"git version 2.50.0.windows.1\n".to_vec()
                    } else {
                        b"gh version 2.80.0 (fixture)\n".to_vec()
                    }
                } else {
                    vec![]
                },
                stderr: vec![],
                timed_out: false,
            });
        }
        assert_eq!(q.program, "winget");
        let args: Vec<String> = q
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|a| a == "--silent"));
        assert!(args.iter().any(|a| a == "--disable-interactivity"));
        assert!(args.windows(2).any(|a| a == ["--scope", "user"]));
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(args.clone());
        if !self.fail && !self.timeout && !self.stale_path {
            if args.iter().any(|a| a == "Git.Git") {
                self.git.store(true, Ordering::SeqCst)
            } else {
                assert!(args.iter().any(|a| a == "GitHub.cli"));
                self.gh.store(true, Ordering::SeqCst)
            }
        }
        Ok(ProcessOutput {
            code: if self.fail { 5 } else { 0 },
            stdout: vec![],
            stderr: vec![],
            timed_out: self.timeout,
        })
    }
}
#[test]
fn prerequisites_reuse_existing_versions_and_install_only_two_explicit_packages() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let home = installed(&f, "environment");
    let ready = Installer::new(true);
    assert_eq!(
        environment::check_with(&ready, &home).unwrap()["ready"],
        true
    );
    assert_eq!(
        environment::prepare_with(&ready, &home, "user", "fixture").unwrap()["ready"],
        true
    );
    assert_eq!(ready.calls.load(Ordering::SeqCst), 0);
    assert!(!home.join(".runtime/bridgepilot-environment").exists());
    let absent = Installer::new(false);
    assert_eq!(
        environment::prepare_with(&absent, &home, "user", "fixture").unwrap()["ready"],
        true
    );
    assert_eq!(absent.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        environment::prepare_with(&absent, &home, "user", "fixture").unwrap()["ready"],
        true
    );
    assert_eq!(absent.calls.load(Ordering::SeqCst), 2);
}
#[test]
fn prerequisites_do_not_elevate_or_resend_unknown_installation() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let home = installed(&f, "environment");
    let mut failed = Installer::new(false);
    failed.fail = true;
    assert_eq!(
        environment::prepare_with(&failed, &home, "user", "fixture").unwrap()["status"],
        "environment_install_attention_required"
    );
    assert_eq!(failed.calls.load(Ordering::SeqCst), 1);
    let g = Fixture::new("complete");
    g.store.update(|s| s.pause(true, &[])).unwrap();
    let ghome = installed(&g, "environment");
    let mut unknown = Installer::new(false);
    unknown.timeout = true;
    environment::prepare_with(&unknown, &ghome, "user", "fixture").unwrap();
    assert_eq!(
        environment::prepare_with(&unknown, &ghome, "user", "fixture").unwrap()["status"],
        "environment_install_unknown"
    );
    assert_eq!(unknown.calls.load(Ordering::SeqCst), 1);
    assert!(environment::prepare_with(&unknown, &g.root, "user", "other-context").is_err());
    assert_eq!(unknown.calls.load(Ordering::SeqCst), 1);
    assert!(environment::prepare_with(&unknown, &ghome, "auto", "fixture").is_err());
}
#[test]
fn successful_install_with_stale_path_is_not_reinstalled() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let home = installed(&f, "environment");
    let mut runner = Installer::new(false);
    runner.stale_path = true;
    assert_eq!(
        environment::prepare_with(&runner, &home, "user", "fixture").unwrap()["status"],
        "environment_path_refresh_required"
    );
    assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
    environment::prepare_with(&runner, &home, "user", "fixture").unwrap();
    assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
}
fn installed(f: &Fixture, name: &str) -> PathBuf {
    let package = f.root.join(format!("package-{name}"));
    install::bundle(
        &package,
        Path::new(env!("CARGO_BIN_EXE_bp")),
        Path::new(env!("CARGO_BIN_EXE_bp-launcher")),
    )
    .unwrap();
    let home = f.root.join(format!("home-{name}"));
    install::install(&package, &home, Some(&f.root), false).unwrap();
    home
}
#[test]
fn user_skill_projection_is_previewable_idempotent_and_preserves_user_edits() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let home = installed(&f, "primary");
    let profile = f.root.join("codex-profile");
    fs::create_dir(&profile).unwrap();
    let skills = profile.join("skills");
    user_skill::install(&home, &skills, true).unwrap();
    assert!(!skills.exists());
    user_skill::install(&home, &skills, false).unwrap();
    assert_eq!(user_skill::status(&home, &skills).unwrap()["current"], true);
    let updater = "bridgepilot/scripts/bridgepilot_update.ps1";
    assert_eq!(
        fs::read(home.join("source/orchestrator/skills").join(updater)).unwrap(),
        fs::read(skills.join(updater)).unwrap()
    );
    for name in ["bridgepilot", "bridgepilot-update", "bridgepilot-cleanup", "bridgepilot-git-sync"] {
        let source = home.join(format!("source/orchestrator/skills/{name}/SKILL.md"));
        assert_eq!(
            fs::read(source).unwrap(),
            fs::read(skills.join(name).join("SKILL.md")).unwrap()
        );
        let binding: Value = serde_json::from_slice(
            &fs::read(skills.join(name).join("bridgepilot-local.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(binding["product"], "BridgePilot");
        assert_eq!(binding["home"], json!(fs::canonicalize(&home).unwrap()));
    }
    let entry = skills.join("bridgepilot/SKILL.md");
    let original = fs::read(&entry).unwrap();
    user_skill::install(&home, &skills, false).unwrap();
    assert_eq!(original, fs::read(&entry).unwrap());
    let other = installed(&f, "other");
    assert!(user_skill::install(&other, &skills, false).is_err());
    fs::write(&entry, b"user customization").unwrap();
    assert!(user_skill::install(&home, &skills, false).is_err());
    assert_eq!(fs::read(&entry).unwrap(), b"user customization");
}
#[test]
fn git_sync_entry_requires_complete_assets_and_preserves_unknown_or_edited_files() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let home = installed(&f, "git-sync");
    let skills = f.root.join("git-sync-profile");
    let entry = skills.join("bridgepilot-git-sync");
    let script = entry.join("scripts/list_projects.py");
    user_skill::install(&home, &skills, true).unwrap();
    assert!(!skills.exists());
    user_skill::install(&home, &skills, false).unwrap();
    let before = fs::read(&script).unwrap();
    assert_eq!(before, fs::read(home.join("source/orchestrator/skills/bridgepilot-git-sync/scripts/list_projects.py")).unwrap());
    assert_eq!(user_skill::status(&home, &skills).unwrap()["skills"]["bridgepilot-git-sync"]["current"], true);
    user_skill::install(&home, &skills, false).unwrap();
    assert_eq!(before, fs::read(&script).unwrap());
    fs::write(&script, b"user script edit").unwrap();
    assert!(user_skill::install(&home, &skills, false).unwrap_err().contains("user-edited"));
    assert_eq!(fs::read(&script).unwrap(), b"user script edit");
    fs::write(&script, before).unwrap();
    let unowned = entry.join("scripts/custom.py");
    fs::write(&unowned, b"user file").unwrap();
    assert!(user_skill::status(&home, &skills).is_err());
    assert_eq!(fs::read(&unowned).unwrap(), b"user file");
    let other = f.root.join("unowned-git-sync-profile");
    let other_home = installed(&f, "unowned-git-sync");
    fs::create_dir_all(other.join("bridgepilot-git-sync")).unwrap();
    fs::write(other.join("bridgepilot-git-sync/SKILL.md"), b"local extension").unwrap();
    assert!(user_skill::install(&other_home, &other, true).unwrap_err().contains("no BP ownership"));
    assert!(!other.join("bridgepilot").exists());
}

#[test]
fn git_sync_bundle_rejects_missing_script_and_stripped_registry_without_writes() {
    let f = Fixture::new("complete");
    let bundle = f.root.join("git-sync-package");
    install::bundle(&bundle, Path::new(env!("CARGO_BIN_EXE_bp")), Path::new(env!("CARGO_BIN_EXE_bp-launcher"))).unwrap();
    let manifest = bundle.join("package.json");
    let original = fs::read(&manifest).unwrap();
    let mut package: Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(package["assets"].as_array().unwrap().len(), 15);
    let home = f.root.join("missing-script-home");
    package["assets"].as_array_mut().unwrap().truncate(13);
    fs::write(&manifest, serde_json::to_vec(&package).unwrap()).unwrap();
    assert!(install::install(&bundle, &home, None, true).unwrap_err().contains("capability"));
    assert!(!home.exists());
    fs::write(&manifest, original).unwrap();
    fs::remove_file(bundle.join("payload/bridgepilot-git-sync-list-projects.py")).unwrap();
    assert!(install::install(&bundle, &home, None, true).is_err());
    assert!(!home.exists());
}
#[test]
fn source_entry_fresh_install_does_not_create_a_private_archive() {
    let f = Fixture::new("complete");
    let package = f.root.join("source-entry-package");
    install::bundle(
        &package,
        Path::new(env!("CARGO_BIN_EXE_bp")),
        Path::new(env!("CARGO_BIN_EXE_bp-launcher")),
    )
    .unwrap();
    let home = f.root.join("source-entry-home");
    install::install(&package, &home, None, true).unwrap();
    assert!(!home.exists());
    install::install(&package, &home, None, false).unwrap();
    let skills = f.root.join("source-entry-skills");
    user_skill::install(&home, &skills, false).unwrap();
    install::install(&package, &home, None, false).unwrap();
    user_skill::install(&home, &skills, false).unwrap();
    assert_eq!(user_skill::status(&home, &skills).unwrap()["current"], true);
    assert!(!home.join(".runtime/config/bridgepilot.json").exists());
    assert!(
        !home
            .join("doc/5_project_knowledgebase/bridgepilot/bridgepilot.sqlite3")
            .exists()
    );
}

#[test]
fn product_versions_are_attested_by_actual_core_before_any_install_writes() {
    let f = Fixture::new("complete");
    let package = f.root.join("versioned-package");
    install::bundle(&package, Path::new(env!("CARGO_BIN_EXE_bp")), Path::new(env!("CARGO_BIN_EXE_bp-launcher"))).unwrap();
    let path = package.join("package.json");
    let original = fs::read(&path).unwrap();
    let mut manifest: Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(manifest["schema"], 2);
    assert!(manifest["components"]["orchestrator"]["version"].is_string());
    assert_eq!(manifest["components"]["forge"]["delivery"], "source-reference");
    let home = f.root.join("versioned-home");
    install::install(&package, &home, None, true).unwrap();
    assert!(!home.exists());
    manifest["components"]["orchestrator"]["version"] = json!("0.99.0");
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(install::install(&package, &home, None, true).unwrap_err().contains("component versions"));
    assert!(!home.exists());
    fs::write(&path, original).unwrap();
    install::install(&package, &home, None, false).unwrap();
    let identity = install::inspect(&home).unwrap();
    assert_eq!(identity.schema, 2);
    assert!(identity.current.components.is_some());
    install::install(&package, &home, None, false).unwrap();
    assert_eq!(install::inspect(&home).unwrap(), identity);
}
#[test]
fn cleanup_entry_upgrades_two_entry_install_and_preserves_unknown_or_edited_skills() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let home = installed(&f, "cleanup");
    let skills = f.root.join("cleanup-profile");
    fs::create_dir(&skills).unwrap();
    user_skill::install(&home, &skills, false).unwrap();
    let entry = skills.join("bridgepilot-cleanup");
    let before = fs::read(entry.join("SKILL.md")).unwrap();
    fs::remove_file(entry.join("SKILL.md")).unwrap();
    fs::remove_file(entry.join("bridgepilot-local.json")).unwrap();
    fs::remove_dir(&entry).unwrap();
    assert_eq!(
        user_skill::status(&home, &skills).unwrap()["skills"]["bridgepilot-cleanup"]["installed"],
        false
    );
    user_skill::install(&home, &skills, true).unwrap();
    assert!(!entry.exists());
    user_skill::install(&home, &skills, false).unwrap();
    assert_eq!(fs::read(entry.join("SKILL.md")).unwrap(), before);
    let binding = fs::read(entry.join("bridgepilot-local.json")).unwrap();
    fs::remove_file(entry.join("bridgepilot-local.json")).unwrap();
    assert!(user_skill::install(&home, &skills, true).is_err());
    assert_eq!(fs::read(entry.join("SKILL.md")).unwrap(), before);
    fs::write(entry.join("bridgepilot-local.json"), binding).unwrap();
    fs::write(entry.join("SKILL.md"), b"user edited cleanup").unwrap();
    assert!(user_skill::install(&home, &skills, false).is_err());
    assert_eq!(
        fs::read(entry.join("SKILL.md")).unwrap(),
        b"user edited cleanup"
    );
}
#[test]
fn user_skill_adds_independent_entry_to_legacy_install_and_preserves_conflicts() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let home = installed(&f, "independent");
    let skills = f.root.join("independent-skills");
    user_skill::install(&home, &skills, false).unwrap();
    let update = skills.join("bridgepilot-update");
    // The original single-entry installation has the same primary ownership binding.
    fs::remove_file(update.join("SKILL.md")).unwrap();
    fs::remove_file(update.join("bridgepilot-local.json")).unwrap();
    fs::remove_dir(&update).unwrap();
    let before = fs::read(skills.join("bridgepilot/bridgepilot-local.json")).unwrap();
    assert_eq!(
        user_skill::status(&home, &skills).unwrap()["current"],
        false
    );
    user_skill::install(&home, &skills, true).unwrap();
    assert!(!update.exists());
    user_skill::install(&home, &skills, false).unwrap();
    assert_eq!(user_skill::status(&home, &skills).unwrap()["current"], true);
    assert_eq!(
        before,
        fs::read(skills.join("bridgepilot/bridgepilot-local.json")).unwrap()
    );
    fs::write(update.join("SKILL.md"), b"user update workflow").unwrap();
    assert!(user_skill::install(&home, &skills, false).is_err());
    assert!(user_skill::status(&home, &skills).is_err());
    assert_eq!(
        fs::read(update.join("SKILL.md")).unwrap(),
        b"user update workflow"
    );

    let conflict = f.root.join("conflicting-skills");
    fs::create_dir_all(conflict.join("bridgepilot-update")).unwrap();
    fs::write(
        conflict.join("bridgepilot-update/SKILL.md"),
        b"unowned workflow",
    )
    .unwrap();
    let other = installed(&f, "unowned-independent");
    assert!(user_skill::install(&other, &conflict, false).is_err());
    assert!(!conflict.join("bridgepilot").exists());
    assert_eq!(
        fs::read(conflict.join("bridgepilot-update/SKILL.md")).unwrap(),
        b"unowned workflow"
    );
}
#[test]
fn user_skill_does_not_adopt_unowned_skill_or_modified_installed_source() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let home = installed(&f, "owned");
    let skills = f.root.join("profile-skills");
    fs::create_dir_all(skills.join("bridgepilot")).unwrap();
    fs::write(skills.join("bridgepilot/SKILL.md"), b"existing user skill").unwrap();
    assert!(user_skill::install(&home, &skills, false).is_err());
    assert!(!skills.join("bridgepilot/bridgepilot-local.json").exists());
    let other = f.root.join("other-profile");
    fs::create_dir(&other).unwrap();
    fs::write(
        home.join("source/orchestrator/skills/bridgepilot/references/init.md"),
        b"modified source",
    )
    .unwrap();
    assert!(user_skill::install(&home, &other, true).is_err());
}

#[test]
fn runtime_and_skill_recheck_each_others_journal_after_preflight() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let home = installed(&f, "interleave");
    let package = f.root.join("package-interleave");
    let before = fs::read(home.join(".runtime/config/bridgepilot-installation.json")).unwrap();
    let pending = home.join(".runtime/bridgepilot-skill/journal.json");
    assert!(
        install::install_with_before_lock(&package, &home, Some(&f.root), || {
            fs::create_dir_all(pending.parent().unwrap()).unwrap();
            fs::write(&pending, b"interrupted skill").unwrap();
        })
        .is_err()
    );
    assert_eq!(
        before,
        fs::read(home.join(".runtime/config/bridgepilot-installation.json")).unwrap()
    );
    fs::remove_file(&pending).unwrap();
    let profile = f.root.join("interleave-profile");
    fs::create_dir(&profile).unwrap();
    let runtime = home.join(".runtime/bridgepilot-install/journal.json");
    assert!(
        user_skill::install_with_before_lock(&home, &profile, || {
            fs::write(&runtime, b"interrupted runtime").unwrap();
        })
        .is_err()
    );
    assert!(!profile.join("bridgepilot/SKILL.md").exists());
    assert_eq!(fs::read(runtime).unwrap(), b"interrupted runtime");
}

#[test]
fn another_profile_cannot_steal_home_binding_after_preflight() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let home = installed(&f, "profile-race");
    let first = f.root.join("first-profile");
    let second = f.root.join("second-profile");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    assert!(
        user_skill::install_with_before_lock(&home, &second, || {
            user_skill::install(&home, &first, false).unwrap();
        })
        .is_err()
    );
    assert!(first.join("bridgepilot/SKILL.md").exists());
    assert!(!second.join("bridgepilot/SKILL.md").exists());
    assert_eq!(user_skill::status(&home, &first).unwrap()["current"], true);
}
