use super::*;

struct Fixture {
    root: PathBuf,
    r: Recipe,
}
fn command(root: &Path, args: &[&str]) {
    let mut q = ProcessRequest::new("git", root);
    q.args = args.iter().map(Into::into).collect();
    let out = SystemProcessRunner.run(&q).unwrap();
    assert_eq!(out.code, 0, "{}", String::from_utf8_lossy(&out.stderr));
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/project-release")
            .join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(root.join("source/a")).unwrap();
        fs::create_dir_all(root.join("source/b")).unwrap();
        fs::write(root.join("VERSION"), "1.0.0\n").unwrap();
        fs::write(
            root.join("CHANGELOG.md"),
            "# Changes\n\n## [1.0.0]\n\n- First version.\n",
        )
        .unwrap();
        fs::write(
            root.join("source/a/pyproject.toml"),
            "[project]\nname='a'\nversion='1.0.0'\n",
        )
        .unwrap();
        fs::write(root.join("source/a/a.py"), "value=1\n").unwrap();
        fs::write(
            root.join("source/b/pyproject.toml"),
            "[project]\nname='b'\nversion='1.0.0'\ndependencies=['a==1.0.0']\n",
        )
        .unwrap();
        fs::write(
            root.join(".gitignore"),
            ".runtime/\n.build/\n.deps/\n.env\n",
        )
        .unwrap();
        command(&root, &["init", "-b", "main"]);
        command(&root, &["config", "user.name", "fixture"]);
        command(&root, &["config", "user.email", "fixture@example.invalid"]);
        command(&root, &["config", "core.hooksPath", ".runtime/no-hooks"]);
        command(&root, &["config", "commit.gpgsign", "false"]);
        command(
            &root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/fixture/project.git",
            ],
        );
        command(&root, &["config", "branch.main.remote", "origin"]);
        command(&root, &["config", "branch.main.merge", "refs/heads/main"]);
        command(&root, &["add", "."]);
        command(&root, &["commit", "-m", "chore: initial"]);
        command(&root, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let r = Recipe {
            schema: 1,
            product: "Fixture".into(),
            repository: "fixture/project".into(),
            gitlab_origin: None,
            version_file: "VERSION".into(),
            changelog: "CHANGELOG.md".into(),
            tag_prefix: "v".into(),
            include: vec![
                "VERSION".into(),
                "CHANGELOG.md".into(),
                "Cargo.toml".into(),
                "source".into(),
            ],
            build: vec![],
            artifacts: vec![],
        };
        Self {
            root: root.canonicalize().unwrap(),
            r,
        }
    }
    fn install(&self) {
        fs::write(
            self.root.join("Cargo.toml"),
            manifest(&self.root, &self.r).unwrap(),
        )
        .unwrap();
        for (rel, bytes) in
            crate::project_release_install::payloads(&self.r, b"fixture-binary").unwrap()
        {
            let path = self.root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
    }
    fn context(&self) -> SourceContext {
        SourceContext {
            schema: 1,
            identity: identity(&self.root).unwrap(),
            baseline: None,
            message: Some("fix: preserve behavior".into()),
        }
    }
}

#[test]
fn native_recipe_install_is_idempotent_and_custom_entry_is_never_overwritten() {
    let f = Fixture::new();
    let initial = crate::project_release_install::inspect(&f.root, None).unwrap();
    assert!(!initial.gaps.is_empty());
    assert!(initial.writes.is_empty());
    assert!(!f.root.join("Cargo.toml").exists());
    let planned = crate::project_release_install::inspect(&f.root, Some(&f.r)).unwrap();
    assert!(planned.gaps.is_empty());
    let first = manifest(&f.root, &f.r).unwrap();
    fs::write(f.root.join("Cargo.toml"), &first).unwrap();
    assert_eq!(manifest(&f.root, &f.r).unwrap(), first);
    let mut doc = String::from_utf8(first)
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    doc["workspace"]["metadata"]["bridgeforge"]["native-release"]["program"] =
        value(".build/custom.exe");
    fs::write(f.root.join("Cargo.toml"), doc.to_string()).unwrap();
    assert!(manifest(&f.root, &f.r).unwrap_err().contains("preserved"));
}

#[test]
fn first_publication_reprepare_keeps_candidate_and_closes_internal_pin_versions() {
    let f = Fixture::new();
    f.install();
    fs::write(f.root.join("source/a/a.py"), "value=2\n").unwrap();
    let context = f.context();
    let result: SourcePlan =
        serde_json::from_value(source(&f.root, Some(&context)).unwrap()).unwrap();
    assert_eq!(result.identity.version, "1.0.1");
    let b = result
        .edits
        .iter()
        .find(|e| e.path == "source/b/pyproject.toml")
        .unwrap();
    assert!(b.after.contains("1.0.1"));
    assert!(b.after.contains("a==1.0.1"));
    for e in &result.edits {
        fs::write(f.root.join(&e.path), &e.after).unwrap();
    }
    fs::write(f.root.join("source/a/a.py"), "value=3\n").unwrap();
    let second: SourcePlan =
        serde_json::from_value(source(&f.root, Some(&f.context())).unwrap()).unwrap();
    assert_eq!(second.identity.version, "1.0.1");
    assert!(second.edits.is_empty(), "{:?}", second.edits);
}

#[test]
fn source_archive_is_deterministic_and_does_not_include_local_secrets() {
    let f = Fixture::new();
    f.install();
    fs::write(f.root.join(".env"), "not-for-publication").unwrap();
    fs::create_dir_all(f.root.join(".runtime/config")).unwrap();
    fs::write(f.root.join(".runtime/config/secret"), "not-for-publication").unwrap();
    let files = inventory(&f.root, &f.r).unwrap();
    let first = f.root.join(".build/first.tar");
    let second = f.root.join(".build/second.tar");
    let hashes = files
        .iter()
        .map(|p| (p.clone(), raw_sha(&fs::read(f.root.join(p)).unwrap())))
        .collect();
    archive::write(&f.root, &files, &hashes, &first).unwrap();
    archive::write(&f.root, &files, &hashes, &second).unwrap();
    assert_eq!(fs::read(&first).unwrap(), fs::read(second).unwrap());
    let mut q = ProcessRequest::new("tar", &f.root);
    q.args = vec!["-tf".into(), first.into_os_string()];
    let out = SystemProcessRunner.run(&q).unwrap();
    assert_eq!(out.code, 0, "{}", String::from_utf8_lossy(&out.stderr));
    let listing = String::from_utf8(out.stdout).unwrap();
    assert!(listing.contains("source/a/a.py"));
    assert!(!listing.contains(".env"));
    assert!(!listing.contains(".runtime"));
}

#[test]
fn prepare_produces_verified_native_assets_and_tampering_breaks_driver_readiness() {
    let f = Fixture::new();
    f.install();
    assert_eq!(verify_driver(&f.root).unwrap()["status"], "ready");
    let output = f.root.join(".build/release");
    let value = prepare(&f.root, &output).unwrap();
    let plan: Plan = serde_json::from_value(value).unwrap();
    plan.validate(&f.root).unwrap();
    assert_eq!(plan.assets.len(), 3);
    fs::write(f.root.join(PROGRAM), b"changed").unwrap();
    assert!(verify_driver(&f.root).is_err());
}

#[test]
fn unsafe_recipe_and_mismatched_platform_are_installation_failures() {
    let f = Fixture::new();
    let mut unsafe_recipe = f.r.clone();
    unsafe_recipe.include.push(".runtime".into());
    assert!(manifest(&f.root, &unsafe_recipe).is_err());
    let mut other = f.r.clone();
    other.repository = "other/project".into();
    let plan = crate::project_release_install::inspect(&f.root, Some(&other)).unwrap();
    assert!(!plan.gaps.is_empty());
    assert!(!f.root.join("Cargo.toml").exists());
    let mut long = f.r.clone();
    long.build.push(vec!["cargo".into(), "build".into()]);
    let doc = String::from_utf8(manifest(&f.root, &long).unwrap())
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    assert!(
        doc["workspace"]["metadata"]["bridgeforge"]["native-release"]["prepare-timeout-seconds"]
            .as_integer()
            .unwrap()
            > 3600
    );
}

#[test]
fn a_higher_unpublished_version_is_not_replaced_by_an_old_release() {
    let f = Fixture::new();
    f.install();
    command(&f.root, &["add", "Cargo.toml"]);
    command(&f.root, &["commit", "-m", "chore: install entry"]);
    let commit = String::from_utf8(git(&f.root, &["rev-parse", "HEAD"]).unwrap())
        .unwrap()
        .trim()
        .to_owned();
    fs::write(f.root.join("VERSION"), "1.0.1\n").unwrap();
    let mut context = f.context();
    context.baseline = Some(bridgeforge_core::native_release::PublishedVersion {
        version: "1.0.0".into(),
        commit,
    });
    let result: SourcePlan =
        serde_json::from_value(source(&f.root, Some(&context)).unwrap()).unwrap();
    assert_eq!(result.identity.version, "1.0.1");
    assert!(result.reuse_release.is_none());
}
