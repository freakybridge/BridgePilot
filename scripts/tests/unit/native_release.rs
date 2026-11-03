use super::*;
use crate::{ProcessOutput, SystemProcessRunner};
use bridgeforge_core::native_release::Asset;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
};

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    plan: Plan,
}
fn process(root: &Path, args: &[&str]) {
    let mut q = ProcessRequest::new("git", root);
    q.args = args.iter().map(Into::into).collect();
    let r = SystemProcessRunner.run(&q).unwrap();
    assert_eq!(r.code, 0, "{}", String::from_utf8_lossy(&r.stderr));
}
impl Fixture {
    fn new() -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/native-release")
            .join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let base = std::path::absolute(base).unwrap();
        fs::create_dir_all(&base).unwrap();
        process(&base, &["init", "--bare", "remote.git"]);
        process(&base, &["init", "-b", "main", "project"]);
        let root = base.join("project");
        for (key, value) in [
            ("user.name", "release-test"),
            ("user.email", "release@example.invalid"),
            ("core.autocrlf", "false"),
            ("core.hooksPath", ".runtime/no-hooks"),
        ] {
            process(&root, &["config", key, value]);
        }
        fs::write(root.join(".gitignore"), ".runtime/\n.build/\n.deps/\n").unwrap();
        fs::write(root.join("VERSION"), "1.0.0\n").unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers=[]\n[workspace.metadata.bridgeforge.native-release]\nbuild=[\"mock-build\"]\nprogram=\".build/mock.exe\"\nargs=[\"prepare\"]\n").unwrap();
        process(&root, &["add", "."]);
        process(&root, &["commit", "-m", "chore: initial"]);
        process(&root, &["remote", "add", "origin", "../remote.git"]);
        process(&root, &["push", "-u", "origin", "main"]);
        fs::create_dir_all(root.join(".build/package")).unwrap();
        let assets = ["one.bin", "two.bin"]
            .into_iter()
            .map(|name| {
                let bytes = name.as_bytes();
                let path = format!(".build/package/{name}");
                fs::write(root.join(&path), bytes).unwrap();
                Asset {
                    name: name.into(),
                    path,
                    sha256: raw_sha(bytes),
                    size: bytes.len() as u64,
                }
            })
            .collect();
        let plan = Plan {
            schema: 1,
            product: "Fixture".into(),
            version: "1.0.0".into(),
            repository: "fixture/repository".into(),
            tag: "bp-v1.0.0".into(),
            title: "Fixture 1.0.0".into(),
            notes: "fixture notes".into(),
            assets,
        };
        Self { base, root, plan }
    }

    fn current_project() -> Self {
        let fixture = Self::new();
        for asset in bridgeforge_core::template_standard::assets() {
            let bytes = bridgeforge_skeleton::content::render(
                asset.rule["id"].as_str().unwrap(),
                &asset.payload,
                None,
                &asset.rule,
                &fixture.root,
                true,
            )
            .unwrap();
            let path = fixture.root.join(asset.rule["target"].as_str().unwrap());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("source/forge/bridgeforge-manifest.json").is_file())
            .unwrap();
        let target = fixture.root.join(".deps/bridgeforge/bin");
        let public = std::env::var_os("BRIDGEFORGE_TEST_RUNTIME")
            .map(PathBuf::from)
            .unwrap_or_else(|| repository.join(".build/bridgeforge/bin"));
        fs::create_dir_all(&target).unwrap();
        for name in [
            format!("bridgeforge{}", std::env::consts::EXE_SUFFIX),
            format!("bridgeforge-hook{}", std::env::consts::EXE_SUFFIX),
            "build-receipt-cli.json".into(),
            "build-receipt-hook.json".into(),
        ] {
            fs::copy(public.join(&name), target.join(name)).unwrap();
        }
        // Real prepared public tools are fixture inputs, with their original receipts.
        bridgeforge_skeleton::template::verify(&fixture.root, true).unwrap();
        process(&fixture.root, &["add", "."]);
        process(
            &fixture.root,
            &["commit", "-m", "chore: valid current project"],
        );
        process(&fixture.root, &["push"]);
        fixture
    }

    fn state_path(&self) -> PathBuf {
        self.root
            .join(".runtime/bridgeforge-codex/native-release/current.json")
    }
    fn state(&self) -> State {
        serde_json::from_slice(&fs::read(self.state_path()).unwrap()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}
fn ok(value: &Value) -> ProcessOutput {
    ProcessOutput {
        code: 0,
        stdout: serde_json::to_vec(value).unwrap(),
        stderr: vec![],
        timed_out: false,
    }
}
fn empty() -> ProcessOutput {
    ProcessOutput {
        code: 0,
        stdout: vec![],
        stderr: vec![],
        timed_out: false,
    }
}
fn missing() -> ProcessOutput {
    ProcessOutput {
        code: 1,
        stdout: vec![],
        stderr: b"HTTP 404".to_vec(),
        timed_out: false,
    }
}

struct Runner {
    plan: Plan,
    remote: RefCell<Option<Value>>,
    tag: RefCell<Option<String>>,
    commands: RefCell<Vec<Vec<String>>>,
    interrupt_upload: Cell<usize>,
    lose_create_reply: Cell<bool>,
    lose_tag_reply: Cell<bool>,
    lose_publish_reply: Cell<bool>,
    fail_build: Cell<bool>,
    fail_build_at: Cell<usize>,
    fail_commit: Cell<bool>,
    fail_push: Cell<bool>,
    replace_state_on_prepare: Cell<bool>,
    source_version: Option<String>,
    history: RefCell<Vec<(String, String, Value)>>,
    reuse_publication: bool,
}
impl Runner {
    fn new(plan: Plan) -> Self {
        Self {
            plan,
            remote: RefCell::new(None),
            tag: RefCell::new(None),
            commands: RefCell::new(vec![]),
            interrupt_upload: Cell::new(0),
            lose_create_reply: Cell::new(false),
            lose_tag_reply: Cell::new(false),
            lose_publish_reply: Cell::new(false),
            fail_build: Cell::new(false),
            fail_build_at: Cell::new(0),
            fail_commit: Cell::new(false),
            fail_push: Cell::new(false),
            replace_state_on_prepare: Cell::new(false),
            source_version: None,
            history: RefCell::new(vec![]),
            reuse_publication: false,
        }
    }
    fn count(&self, word: &str) -> usize {
        self.commands
            .borrow()
            .iter()
            .filter(|args| args.iter().any(|a| a == word))
            .count()
    }
}
impl ProcessRunner for Runner {
    fn run(&self, q: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        let program = Path::new(&q.program)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let args: Vec<String> = q
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let mut observed = vec![program.clone()];
        observed.extend(args.clone());
        self.commands.borrow_mut().push(observed);
        if program == "git" {
            if (self.fail_commit.get() && args.first().is_some_and(|s| s == "commit"))
                || (self.fail_push.get() && args.first().is_some_and(|s| s == "push"))
            {
                return Ok(ProcessOutput {
                    code: 1,
                    stderr: b"injected Git stage failure".to_vec(),
                    ..empty()
                });
            }
            if args.windows(2).any(|a| a == ["remote", "get-url"]) {
                return Ok(ProcessOutput {
                    stdout: b"https://github.com/fixture/repository.git\n".to_vec(),
                    ..empty()
                });
            }
            return SystemProcessRunner.run(q);
        }
        if program == "mock-build" {
            return Ok(if self.fail_build.get() || self.fail_build_at.get() == self.count("mock-build") {
                ProcessOutput {
                    code: 1,
                    stderr: b"build failed".to_vec(),
                    ..empty()
                }
            } else {
                empty()
            });
        }
        if program == "mock" {
            if args.first().is_some_and(|a| a == "source") {
                use bridgeforge_core::native_release::{SourceIdentity, SourcePlan, SourceEdit};
                let before = fs::read_to_string(q.cwd.join("VERSION"))?;
                let mut identity = SourceIdentity { product: self.plan.product.clone(), repository: self.plan.repository.clone(),
                    version: before.trim().into(), tag_prefix: "bp-v".into(), paths: vec!["VERSION".into()] };
                if !args.iter().any(|a| a == "--input") { return Ok(ok(&serde_json::to_value(identity).unwrap())); }
                let selected = self.source_version.as_ref().unwrap();
                identity.version = selected.clone();
                let edits = if before.trim() == selected { vec![] } else {
                    vec![SourceEdit { path: "VERSION".into(), before, after: format!("{selected}\n") }]
                };
                let context: bridgeforge_core::native_release::SourceContext = serde_json::from_slice(&fs::read(args.last().unwrap())?).unwrap();
                let reuse_release = if self.reuse_publication { context.baseline } else { None };
                let release_assets = if self.reuse_publication { self.plan.assets.iter().map(|asset| asset.name.clone()).collect() } else { vec![] };
                return Ok(ok(&serde_json::to_value(SourcePlan { schema: 1, identity, edits, reuse_release, release_assets }).unwrap()));
            }
            let output = PathBuf::from(args.last().unwrap());
            fs::create_dir_all(&output)?;
            let mut plan = self.plan.clone();
            if self.source_version.is_some() {
                plan.version = fs::read_to_string(q.cwd.join("VERSION"))?.trim().into();
                plan.tag = format!("bp-v{}", plan.version);
            }
            for asset in &mut plan.assets {
                let path = output.join(&asset.name);
                fs::copy(q.cwd.join(&asset.path), &path)?;
                asset.path = path
                    .strip_prefix(&q.cwd)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
            }
            if self.replace_state_on_prepare.replace(false) {
                fs::write(
                    q.cwd
                        .join(".runtime/bridgeforge-codex/native-release/current.json"),
                    b"{\"external\":true}\n",
                )?;
            }
            return Ok(ok(&serde_json::to_value(plan).unwrap()));
        }
        assert_eq!(program, "gh", "unexpected subprocess");
        if args.first().is_some_and(|a| a == "release") {
            let interrupt = self.interrupt_upload.get();
            self.interrupt_upload.set(interrupt.saturating_sub(1));
            if interrupt == 1 {
                return Ok(ProcessOutput {
                    timed_out: true,
                    code: 1,
                    ..empty()
                });
            }
            let mut state = self.remote.borrow_mut();
            let assets = state.as_mut().unwrap()["assets"].as_array_mut().unwrap();
            for path in args.iter().filter(|a| a.ends_with(".bin")) {
                let name = Path::new(path).file_name().unwrap().to_str().unwrap();
                let expected = self.plan.assets.iter().find(|a| a.name == name).unwrap();
                assets.push(json!({"name":name,"size":expected.size,"digest":expected.sha256,"state":"uploaded"}));
                if interrupt > 1 {
                    return Ok(ProcessOutput {
                        timed_out: true,
                        code: 1,
                        ..empty()
                    });
                }
            }
            return Ok(empty());
        }
        let method = &args[4];
        let endpoint = &args[5];
        if endpoint.contains("/git/ref/tags/") {
            if let Some((_, commit, _)) = self.history.borrow().iter().find(|(tag, _, _)| endpoint.ends_with(&format!("/{tag}"))) {
                return Ok(ok(&json!({"object":{"type":"commit","sha":commit}})));
            }
            if self.source_version.is_some() && self.tag.borrow().is_none() { return Ok(missing()); }
            return Ok(self
                .tag
                .borrow()
                .as_ref()
                .map(|sha| ok(&json!({"object":{"type":"commit","sha":sha}})))
                .unwrap_or_else(missing));
        }
        if method == "GET" {
            if endpoint.contains("/releases/tags/") {
                if let Some((_, _, value)) = self.history.borrow().iter().find(|(tag, _, _)| endpoint.ends_with(&format!("/{tag}"))) {
                    return Ok(ok(value));
                }
                return Ok(self
                    .remote
                    .borrow()
                    .as_ref()
                    .filter(|v| v["draft"] == false)
                    .map(ok)
                    .unwrap_or_else(missing));
            }
            if endpoint.contains("/releases?per_page=") {
                let mut values = self.history.borrow().iter().map(|(_,_,v)| v.clone()).collect::<Vec<_>>();
                values.extend(self.remote.borrow().iter().cloned());
                return Ok(ok(&json!(values)));
            }
            assert!(
                endpoint.ends_with("/releases/11"),
                "unexpected GitHub read {endpoint}"
            );
            return Ok(self
                .remote
                .borrow()
                .as_ref()
                .map(ok)
                .unwrap_or_else(missing));
        }
        if method == "POST" {
            if endpoint.ends_with("/git/refs") {
                let v: Value = serde_json::from_slice(&q.stdin).unwrap();
                *self.tag.borrow_mut() = Some(v["sha"].as_str().unwrap().into());
                if self.lose_tag_reply.replace(false) {
                    return Ok(ProcessOutput {
                        timed_out: true,
                        code: 1,
                        ..empty()
                    });
                }
                return Ok(ok(
                    &json!({"ref":v["ref"],"object":{"type":"commit","sha":v["sha"]}}),
                ));
            }
            let mut v: Value = serde_json::from_slice(&q.stdin).unwrap();
            v["id"] = json!(11);
            v["assets"] = json!([]);
            v["html_url"] = json!(format!("https://github.com/fixture/repository/releases/tag/{}", v["tag_name"].as_str().unwrap()));
            *self.remote.borrow_mut() = Some(v.clone());
            if self.lose_create_reply.replace(false) {
                return Ok(ProcessOutput {
                    timed_out: true,
                    code: 1,
                    ..empty()
                });
            }
            return Ok(ok(&v));
        }
        assert_eq!(method, "PATCH");
        let mut state = self.remote.borrow_mut();
        let v = state.as_mut().unwrap();
        v["draft"] = json!(false);
        v["published_at"] = json!("2026-11-03T00:00:00Z");
        *self.tag.borrow_mut() = Some(v["target_commitish"].as_str().unwrap().into());
        if self.lose_publish_reply.replace(false) {
            return Ok(ProcessOutput {
                timed_out: true,
                code: 1,
                ..empty()
            });
        }
        Ok(ok(v))
    }
}

#[test]
fn complete_release_and_repeat_do_not_rebuild_repush_or_reupload() {
    let f = Fixture::new();
    let runner = Runner::new(f.plan.clone());
    let first = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(first.code, 0, "{}", first.stderr);
    assert_eq!(first.receipt.as_ref().unwrap()["status"], "released");
    assert_eq!(first.receipt.as_ref().unwrap()["assets_verified"], 2);
    let second = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(second.code, 0, "{}", second.stderr);
    assert_eq!(second.receipt.unwrap()["resumed"], true);
    assert_eq!(runner.count("mock-build"), 1);
    assert_eq!(runner.count("fetch"), 1);
    assert_eq!(runner.count("upload"), 1);
    assert_eq!(runner.count("POST"), 2);
    assert_eq!(runner.count("PATCH"), 1);
    assert_eq!(
        fs::read_to_string(f.root.join("VERSION")).unwrap(),
        "1.0.0\n"
    );
}

fn next_source_release(f: &Fixture, runner: &mut Runner) {
    let first = run(&f.root, runner, git_sync::GitSyncOptions::default());
    assert_eq!(first.code, 0, "{}", first.stderr);
    let old = runner.remote.borrow_mut().take().unwrap();
    let commit = runner.tag.borrow_mut().take().unwrap();
    runner.history.borrow_mut().push((f.plan.tag.clone(), commit, old));
    let cargo = fs::read_to_string(f.root.join("Cargo.toml")).unwrap();
    fs::write(f.root.join("Cargo.toml"), format!("{cargo}source-args=[\"source\"]\n")).unwrap();
    fs::write(f.root.join("feature.txt"), "feature").unwrap();
    runner.source_version = Some("1.1.0".into());
}

#[test]
fn next_release_versions_before_push_and_repeats_without_another_increment() {
    let f = Fixture::current_project();
    let mut runner = Runner::new(f.plan.clone());
    next_source_release(&f, &mut runner);
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.receipt.unwrap()["version"], "1.1.0");
    assert_eq!(fs::read_to_string(f.root.join("VERSION")).unwrap(), "1.1.0\n");
    let builds = runner.count("mock-build");
    let pushes = runner.count("push");
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(runner.count("mock-build"), builds);
    assert_eq!(runner.count("push"), pushes);
    assert_eq!(runner.history.borrow()[0].0, "bp-v1.0.0");
}

#[test]
fn applied_version_survives_final_build_failure_and_corrected_source() {
    let f = Fixture::current_project();
    let mut runner = Runner::new(f.plan.clone());
    next_source_release(&f, &mut runner);
    runner.fail_build_at.set(runner.count("mock-build") + 2);
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_ne!(out.code, 0);
    assert_eq!(fs::read_to_string(f.root.join("VERSION")).unwrap(), "1.1.0\n");
    fs::write(f.root.join("feature.txt"), "corrected feature").unwrap();
    runner.fail_build_at.set(0);
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.receipt.unwrap()["version"], "1.1.0");
}

#[test]
fn resumed_prepared_state_finishes_pending_source_transaction_before_commit() {
    let f = Fixture::current_project();
    let mut runner = Runner::new(f.plan.clone());
    next_source_release(&f, &mut runner);
    runner.fail_commit.set(true);
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_ne!(out.code, 0);
    let source = f.root.join(".runtime/bridgeforge-codex/native-release/source.json");
    let mut tx: Value = serde_json::from_slice(&fs::read(&source).unwrap()).unwrap();
    tx["finished"] = json!(false);
    fs::write(&source, serde_json::to_vec(&tx).unwrap()).unwrap();
    runner.fail_commit.set(false);
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    let tx: Value = serde_json::from_slice(&fs::read(source).unwrap()).unwrap();
    assert_eq!(tx["finished"], true);
    assert_eq!(run(&f.root, &runner, git_sync::GitSyncOptions::default()).code, 0);
}

#[test]
fn obsolete_pushed_version_is_replaced_without_rewriting_its_commit_or_tag() {
    let f = Fixture::current_project();
    let mut runner = Runner::new(f.plan.clone());
    next_source_release(&f, &mut runner);
    process(&f.root, &["add", "."]);
    process(&f.root, &["commit", "-m", "feat: pushed but not published"]);
    process(&f.root, &["push"]);
    let current = anchor(&f.root, &runner).unwrap();
    let mut old = f.state();
    old.status = "pushed".into(); old.result = None;
    old.commit = Some(current.head.clone()); old.anchor = Some(current.clone());
    old.fingerprint = fingerprint(&f.root, &runner).unwrap();
    fs::write(f.state_path(), serde_json::to_vec(&old).unwrap()).unwrap();
    assert!(source::can_replace_obsolete_attempt(&f.root, &runner, &old, &current).unwrap());
    let mut drift = current.clone(); drift.url.push_str("-other");
    assert!(!source::can_replace_obsolete_attempt(&f.root, &runner, &old, &drift).unwrap());
    runner.history.borrow_mut()[0].2["draft"] = json!(true);
    assert!(!source::can_replace_obsolete_attempt(&f.root, &runner, &old, &current).unwrap());
    runner.history.borrow_mut()[0].2["draft"] = json!(false);
    let tag = runner.history.borrow()[0].1.clone();
    runner.history.borrow_mut()[0].1 = current.head.clone();
    assert!(!source::can_replace_obsolete_attempt(&f.root, &runner, &old, &current).unwrap());
    runner.history.borrow_mut()[0].1 = tag.clone();
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.receipt.as_ref().unwrap()["version"], "1.1.0");
    assert!(out.receipt.as_ref().unwrap()["previous_attempt"].is_string());
    assert_eq!(runner.history.borrow()[0].1, tag);
    assert_eq!(git(&f.root, &runner, &["merge-base", &current.head, "HEAD"]).unwrap(), current.head);
}

#[test]
fn partial_source_transaction_recovers_but_external_version_edit_is_preserved() {
    let f = Fixture::new();
    let runner = Runner::new(f.plan.clone());
    fs::write(f.root.join("CHANGELOG.md"), "before notes\n").unwrap();
    let edits = json!([
        {"path":"VERSION","before":"1.0.0\n","after":"1.1.0\n"},
        {"path":"CHANGELOG.md","before":"before notes\n","after":"after notes\n"}
    ]);
    let outside = fingerprint_without(&f.root, &runner, &BTreeSet::from(["VERSION".into(), "CHANGELOG.md".into()])).unwrap();
    let tx = json!({"schema":1,"anchor":anchor(&f.root,&runner).unwrap(),"outside":outside,"finished":false,
        "plan":{"schema":1,"identity":{"product":"Fixture","repository":"fixture/repository","version":"1.1.0","tag_prefix":"bp-v","paths":["VERSION","CHANGELOG.md"]},"edits":edits}});
    let path = f.root.join(".runtime/bridgeforge-codex/native-release/source.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec(&tx).unwrap()).unwrap();
    fs::write(f.root.join("VERSION"), "1.1.0\n").unwrap();
    source::recover(&f.root, &runner).unwrap();
    assert_eq!(fs::read_to_string(f.root.join("CHANGELOG.md")).unwrap(), "after notes\n");
    fs::write(f.root.join("VERSION"), "9.0.0\n").unwrap();
    assert!(source::recover(&f.root, &runner).is_err());
    assert_eq!(fs::read_to_string(f.root.join("VERSION")).unwrap(), "9.0.0\n");
}

fn source_adapter_publication() -> (Fixture, Runner) {
    let f = Fixture::current_project();
    let cargo = fs::read_to_string(f.root.join("Cargo.toml")).unwrap();
    fs::write(f.root.join("Cargo.toml"), format!("{cargo}source-args=[\"source\"]\n")).unwrap();
    process(&f.root, &["add", "."]);
    process(&f.root, &["commit", "-m", "chore: configure source adapter"]);
    process(&f.root, &["push"]);
    let mut runner = Runner::new(f.plan.clone());
    runner.source_version = Some(f.plan.version.clone());
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    runner.reuse_publication = true;
    (f, runner)
}

#[test]
fn documentation_sync_reuses_release_without_local_state_or_artifacts() {
    let (f, runner) = source_adapter_publication();
    let original_commit = f.state().commit.unwrap();
    let tag = runner.tag.borrow().clone();
    let writes = runner.count("POST") + runner.count("PATCH") + runner.count("upload");
    let prepared = f.state().plan.assets;
    for asset in prepared.iter().chain(&f.plan.assets) { fs::remove_file(f.root.join(&asset.path)).unwrap(); }
    fs::remove_file(f.state_path()).unwrap();
    fs::create_dir_all(f.root.join("doc")).unwrap();
    fs::write(f.root.join("doc/acceptance.md"), "accepted").unwrap();
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    let receipt = out.receipt.unwrap();
    assert_eq!(receipt["version"], "1.0.0");
    assert_eq!(receipt["commit"], original_commit);
    assert_ne!(receipt["synchronization_commit"], original_commit);
    assert_eq!(receipt["publication_reused"], true);
    assert_eq!(receipt["publication_verified"], true);
    assert_eq!(receipt["assets_verified"], 2);
    assert_eq!(*runner.tag.borrow(), tag);
    assert_eq!(runner.count("POST") + runner.count("PATCH") + runner.count("upload"), writes);
    assert_eq!(fs::read_to_string(f.root.join("VERSION")).unwrap(), "1.0.0\n");
    let repeat = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(repeat.code, 0, "{}", repeat.stderr);
    assert_eq!(repeat.receipt.unwrap()["build_invocations"], 0);
}

#[test]
fn released_verification_needs_no_local_artifacts_but_rejects_remote_tampering() {
    let f = Fixture::new();
    let runner = Runner::new(f.plan.clone());
    let first = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(first.code, 0, "{}", first.stderr);
    for asset in f.state().plan.assets { fs::remove_file(f.root.join(asset.path)).unwrap(); }
    let repeated = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(repeated.code, 0, "{}", repeated.stderr);
    let original = runner.remote.borrow().clone();
    let original_tag = runner.tag.borrow().clone();
    let writes = runner.count("POST") + runner.count("PATCH") + runner.count("upload");
    for fault in ["marker", "missing", "digest", "tag", "draft"] {
        *runner.remote.borrow_mut() = original.clone();
        *runner.tag.borrow_mut() = original_tag.clone();
        match fault {
            "marker" => runner.remote.borrow_mut().as_mut().unwrap()["body"] = json!("foreign release"),
            "missing" => { runner.remote.borrow_mut().as_mut().unwrap()["assets"].as_array_mut().unwrap().pop(); },
            "digest" => runner.remote.borrow_mut().as_mut().unwrap()["assets"][0]["digest"] = json!("sha256:bad"),
            "tag" => *runner.tag.borrow_mut() = Some("a".repeat(40)),
            "draft" => runner.remote.borrow_mut().as_mut().unwrap()["draft"] = json!(true),
            _ => unreachable!(),
        }
        let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
        assert_ne!(out.code, 0, "{fault}");
    }
    assert_eq!(runner.count("POST") + runner.count("PATCH") + runner.count("upload"), writes);
}

#[test]
fn reused_publication_push_failure_resumes_without_republishing() {
    let (f, runner) = source_adapter_publication();
    fs::create_dir_all(f.root.join("doc")).unwrap();
    fs::write(f.root.join("doc/acceptance.md"), "accepted").unwrap();
    let original_commit = f.state().commit.unwrap();
    let writes = runner.count("POST") + runner.count("PATCH") + runner.count("upload");
    runner.fail_push.set(true);
    let failed = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_ne!(failed.code, 0);
    assert_eq!(f.state().status, "source-committed", "{} {:?}", failed.stderr, failed.receipt);
    let synchronized = f.state().commit.unwrap();
    runner.fail_push.set(false);
    let resumed = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(resumed.code, 0, "{}", resumed.stderr);
    let receipt = resumed.receipt.unwrap();
    assert_eq!(receipt["commit"], original_commit);
    assert_eq!(receipt["synchronization_commit"], synchronized);
    assert_eq!(runner.count("POST") + runner.count("PATCH") + runner.count("upload"), writes);
}

#[test]
fn pushed_documentation_reuse_is_never_an_obsolete_version_attempt() {
    let f = Fixture::new();
    let cargo = fs::read_to_string(f.root.join("Cargo.toml")).unwrap();
    fs::write(f.root.join("Cargo.toml"), format!("{cargo}source-args=[\"source\"]\n")).unwrap();
    process(&f.root, &["add", "."]);
    process(&f.root, &["commit", "-m", "chore: source adapter"]);
    process(&f.root, &["push"]);
    let mut runner = Runner::new(f.plan.clone());
    runner.source_version = Some(f.plan.version.clone());
    let original = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(original.code, 0, "{}", original.stderr);
    let published_commit = f.state().commit.unwrap();
    fs::create_dir_all(f.root.join("doc")).unwrap();
    fs::write(f.root.join("doc/acceptance.md"), "accepted").unwrap();
    process(&f.root, &["add", "."]);
    process(&f.root, &["commit", "-m", "docs: accepted"]);
    process(&f.root, &["push"]);
    // Construct the already-pushed checkpoint, before the final read completes.
    let mut pending = f.state();
    pending.reuse_release = Some(bridgeforge_core::native_release::PublishedVersion {
        version: pending.plan.version.clone(), commit: published_commit });
    let current = anchor(&f.root, &runner).unwrap();
    pending.commit = Some(current.head.clone()); pending.anchor = Some(current.clone());
    pending.fingerprint = fingerprint(&f.root, &runner).unwrap();
    pending.status = "pushed".into(); pending.result = None;
    let bytes = serde_json::to_vec(&pending).unwrap();
    fs::write(f.state_path(), &bytes).unwrap();
    let builds = runner.count("mock-build");
    let commits = runner.count("commit");
    let pushes = runner.count("push");
    let tag = runner.tag.borrow().clone();
    assert!(!source::can_replace_obsolete_attempt(&f.root, &runner, &pending, &anchor(&f.root, &runner).unwrap()).unwrap());
    let mut genuinely_obsolete = pending.clone(); genuinely_obsolete.reuse_release = None;
    assert!(source::can_replace_obsolete_attempt(&f.root, &runner, &genuinely_obsolete, &current).unwrap());
    let resumed = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(resumed.code, 0, "{}", resumed.stderr);
    assert_eq!(runner.count("mock-build"), builds);
    assert_eq!(runner.count("commit"), commits);
    assert_eq!(runner.count("push"), pushes);
    fs::write(f.state_path(), &bytes).unwrap();
    fs::write(f.root.join("code.rs"), "// new code after fixed commit\n").unwrap();
    let rejected = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_ne!(rejected.code, 0);
    assert_eq!(fs::read(f.state_path()).unwrap(), bytes);
    assert_eq!(runner.count("mock-build"), builds);
    assert_eq!(runner.count("commit"), commits);
    assert_eq!(runner.count("push"), pushes);
    assert_eq!(*runner.tag.borrow(), tag);
}

#[test]
fn partial_upload_resumes_only_missing_assets_without_repeating_preparation() {
    let f = Fixture::new();
    let runner = Runner::new(f.plan.clone());
    runner.interrupt_upload.set(2);
    let first = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_ne!(first.code, 0);
    assert_eq!(first.receipt.unwrap()["status"], "incomplete");
    assert_eq!(runner.remote.borrow().as_ref().unwrap()["draft"], true);
    let second = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(second.code, 0, "{}", second.stderr);
    assert_eq!(second.receipt.unwrap()["assets_reused"], 1);
    assert_eq!(runner.count("mock-build"), 1);
    assert_eq!(runner.count("fetch"), 1);
    assert_eq!(runner.count("POST"), 2);
    assert_eq!(
        runner.remote.borrow().as_ref().unwrap()["assets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn unknown_create_and_publish_results_are_reconciled_without_duplicate_mutations() {
    let f = Fixture::new();
    let runner = Runner::new(f.plan.clone());
    runner.lose_tag_reply.set(true);
    runner.lose_create_reply.set(true);
    runner.lose_publish_reply.set(true);
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(runner.count("POST"), 2);
    assert_eq!(runner.count("PATCH"), 1);
}

#[test]
fn changed_source_and_mismatched_remote_assets_never_resume_blindly() {
    let f = Fixture::new();
    let runner = Runner::new(f.plan.clone());
    runner.interrupt_upload.set(2);
    assert_ne!(
        run(&f.root, &runner, git_sync::GitSyncOptions::default()).code,
        0
    );
    runner.remote.borrow_mut().as_mut().unwrap()["assets"][0]["digest"] = json!("sha256:external");
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_ne!(out.code, 0);
    assert_eq!(out.receipt.unwrap()["pause_category"], "conflict");
    assert_eq!(runner.count("upload"), 2);
    fs::write(f.root.join("new-source.txt"), b"external").unwrap();
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("inputs changed"));
    assert_eq!(runner.count("upload"), 2);
}

#[test]
fn existing_tag_mismatch_and_build_failure_cannot_be_reported_as_published() {
    let f = Fixture::new();
    let runner = Runner::new(f.plan.clone());
    runner.fail_build.set(true);
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_ne!(out.code, 0);
    assert_eq!(runner.count("POST"), 0);
    runner.fail_build.set(false);
    *runner.tag.borrow_mut() = Some("a".repeat(40));
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_ne!(out.code, 0);
    assert_eq!(out.receipt.unwrap()["pause_category"], "conflict");
    assert_eq!(runner.count("POST"), 0);
    assert_eq!(runner.count("push"), 0);
}

#[test]
fn release_without_adapter_and_invalid_arguments_never_fall_back_to_sync() {
    struct Never;
    impl ProcessRunner for Never {
        fn run(&self, _: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            panic!("invalid invocation must not run Git")
        }
    }
    let f = Fixture::new();
    fs::write(f.root.join("Cargo.toml"), "[workspace]\nmembers=[]\n").unwrap();
    let out = run(&f.root, &Never, git_sync::GitSyncOptions::default());
    assert_ne!(out.code, 0);
    assert_eq!(out.receipt.unwrap()["pause_category"], "decision_required");
    for args in [
        vec!["git-sync", "releaze"],
        vec!["git-sync", "release", "--skip-push"],
        vec!["git-sync", "--release"],
        vec!["git-sync", "--remote"],
        vec!["git-sync", "--preflight", "--preflight"],
        vec!["git-sync", "--publication-required"],
    ] {
        let out = crate::git_command::run(
            &args.into_iter().map(str::to_owned).collect::<Vec<_>>(),
            &Never,
        );
        assert_ne!(out.code, 0);
    }
}

#[test]
fn release_artifact_validation_rejects_tampering_and_non_build_sources() {
    let f = Fixture::new();
    let mut plan = f.plan.clone();
    plan.assets[0].path = "VERSION".into();
    assert!(plan.validate(&f.root).is_err());
    let mut plan = f.plan.clone();
    plan.assets[0].sha256 = "sha256:wrong".into();
    assert!(plan.validate(&f.root).is_err());
    assert!(
        github::match_remote(
            "https://github.com/other/repository.git",
            &f.plan.repository
        )
        .is_err()
    );
    assert!(
        !f.root
            .join(".runtime/bridgeforge-codex/native-release/current.json")
            .exists()
    );
}

#[test]
fn source_fingerprint_preserves_leading_whitespace_and_survives_commit() {
    let f = Fixture::new();
    fs::write(f.root.join(" first.txt"), b"one").unwrap();
    let first = fingerprint(&f.root, &SystemProcessRunner).unwrap();
    fs::write(f.root.join(" first.txt"), b"two").unwrap();
    let second = fingerprint(&f.root, &SystemProcessRunner).unwrap();
    assert_ne!(first, second);
    fs::remove_file(f.root.join("VERSION")).unwrap();
    let before = fingerprint(&f.root, &SystemProcessRunner).unwrap();
    process(&f.root, &["add", "."]);
    process(&f.root, &["commit", "-m", "chore: source change"]);
    assert_eq!(fingerprint(&f.root, &SystemProcessRunner).unwrap(), before);
}

#[test]
fn one_safe_upload_retry_completes_without_requiring_another_user_command() {
    let f = Fixture::new();
    let runner = Runner::new(f.plan.clone());
    runner.interrupt_upload.set(1);
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(runner.count("upload"), 2);
    assert_eq!(runner.count("mock-build"), 1);
    assert_eq!(runner.count("fetch"), 1);
}

#[test]
fn foreign_draft_is_rejected_before_any_tag_or_release_write() {
    let f = Fixture::new();
    let runner = Runner::new(f.plan.clone());
    *runner.remote.borrow_mut() = Some(
        json!({"id":11,"tag_name":f.plan.tag,"body":"foreign draft","draft":true,"prerelease":false,"assets":[]}),
    );
    let out = run(&f.root, &runner, git_sync::GitSyncOptions::default());
    assert_ne!(out.code, 0);
    assert_eq!(runner.count("POST"), 0);
    assert_eq!(runner.count("PATCH"), 0);
    assert_eq!(runner.count("upload"), 0);
    assert!(runner.tag.borrow().is_none());
}

#[test]
fn corrected_precommit_failure_reprepares_and_preserves_previous_attempt() {
    let f = Fixture::current_project();
    let runner = Runner::new(f.plan.clone());
    fs::write(f.root.join("review.txt"), b"before correction").unwrap();
    runner.fail_commit.set(true);
    assert_ne!(run(&f.root, &runner, Default::default()).code, 0);
    let old_bytes = fs::read(f.state_path()).unwrap();
    let old = f.state();
    assert_eq!(old.status, "blocked-before-commit");
    assert!(old.commit.is_none());
    assert_eq!(runner.count("POST"), 0);
    fs::write(f.root.join("review.txt"), b"corrected").unwrap();
    runner.fail_commit.set(false);
    let out = run(&f.root, &runner, Default::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    let receipt = out.receipt.unwrap();
    assert_eq!(receipt["status"], "released");
    assert_eq!(receipt["recovery_action"], "reprepared");
    assert_eq!(receipt["prepared_artifacts_reused"], false);
    assert_eq!(runner.count("mock-build"), 2);
    let archive = f.root.join(receipt["previous_attempt"].as_str().unwrap());
    assert_eq!(fs::read(archive).unwrap(), old_bytes);
    let current = f.state();
    assert_ne!(current.plan.assets[0].path, old.plan.assets[0].path);
    assert!(f.root.join(&old.plan.assets[0].path).is_file());
}

#[test]
fn unchanged_precommit_retry_reuses_existing_artifacts() {
    let f = Fixture::current_project();
    let runner = Runner::new(f.plan.clone());
    fs::write(f.root.join("review.txt"), b"pending").unwrap();
    runner.fail_commit.set(true);
    assert_ne!(run(&f.root, &runner, Default::default()).code, 0);
    runner.fail_commit.set(false);
    let out = run(&f.root, &runner, Default::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.receipt.unwrap()["prepared_artifacts_reused"], true);
    assert_eq!(runner.count("mock-build"), 1);
}

#[test]
fn legacy_prepared_recovery_requires_real_before_commit_guard() {
    for guard_kind in ["missing", "valid", "bad-schema", "bad-tree"] {
        let f = Fixture::current_project();
        let runner = Runner::new(f.plan.clone());
        fs::write(f.root.join("review.txt"), b"pending").unwrap();
        runner.fail_commit.set(true);
        assert_ne!(run(&f.root, &runner, Default::default()).code, 0);
        let mut value = serde_json::to_value(f.state()).unwrap();
        value.as_object_mut().unwrap().remove("anchor");
        value["status"] = json!("prepared");
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        fs::write(f.state_path(), &bytes).unwrap();
        let guard_path = f
            .root
            .join(".runtime/bridgeforge-codex/release-preparation/commit-guard.json");
        if guard_kind == "missing" {
            fs::remove_file(&guard_path).unwrap();
        } else if guard_kind.starts_with("bad-") {
            let mut guard: Value = serde_json::from_slice(&fs::read(&guard_path).unwrap()).unwrap();
            if guard_kind == "bad-schema" {
                guard["schema"] = json!(99);
            } else {
                guard["tree"] = json!("bad");
            }
            fs::write(&guard_path, serde_json::to_vec(&guard).unwrap()).unwrap();
        }
        fs::write(f.root.join("review.txt"), b"corrected").unwrap();
        runner.fail_commit.set(false);
        let out = run(&f.root, &runner, Default::default());
        if guard_kind == "valid" {
            assert_eq!(out.code, 0, "{}", out.stderr);
            let archive = f
                .root
                .join(out.receipt.unwrap()["previous_attempt"].as_str().unwrap());
            assert_eq!(fs::read(archive).unwrap(), bytes);
        } else {
            assert_ne!(out.code, 0);
            assert!(
                out.stderr.contains(if guard_kind == "missing" {
                    "before-commit evidence"
                } else {
                    "commit guard"
                }),
                "{}",
                out.stderr
            );
            assert_eq!(fs::read(f.state_path()).unwrap(), bytes);
            assert_eq!(runner.count("mock-build"), 1);
        }
    }
}

#[test]
fn failed_reprepare_and_concurrent_state_write_preserve_original_state() {
    for concurrent in [false, true] {
        let f = Fixture::current_project();
        let runner = Runner::new(f.plan.clone());
        fs::write(f.root.join("review.txt"), b"pending").unwrap();
        runner.fail_commit.set(true);
        assert_ne!(run(&f.root, &runner, Default::default()).code, 0);
        let old = fs::read(f.state_path()).unwrap();
        fs::write(f.root.join("review.txt"), b"corrected").unwrap();
        runner.fail_build.set(!concurrent);
        runner.replace_state_on_prepare.set(concurrent);
        let out = run(&f.root, &runner, Default::default());
        assert_ne!(out.code, 0);
        let expected: &[u8] = if concurrent {
            b"{\"external\":true}\n"
        } else {
            &old
        };
        assert_eq!(fs::read(f.state_path()).unwrap(), expected);
        assert_eq!(runner.count("POST"), 0);
        let archive = f.root.join(format!(
            ".runtime/bridgeforge-codex/native-release/attempts/{}/state.json",
            &raw_sha(&old)[7..]
        ));
        assert_eq!(fs::read(archive).unwrap(), old);
    }
}

#[test]
fn committed_source_only_resumes_push_and_rejects_changed_inputs() {
    for changed in [false, true] {
        let f = Fixture::current_project();
        let runner = Runner::new(f.plan.clone());
        fs::write(f.root.join("review.txt"), b"pending").unwrap();
        runner.fail_push.set(true);
        assert_ne!(run(&f.root, &runner, Default::default()).code, 0);
        let saved = f.state();
        assert_eq!(saved.status, "source-committed");
        assert!(saved.commit.is_some());
        runner.fail_push.set(false);
        if changed {
            fs::write(f.root.join("review.txt"), b"later").unwrap();
        }
        let out = run(&f.root, &runner, Default::default());
        if changed {
            assert_ne!(out.code, 0);
            assert!(out.stderr.contains("fixed source commit"));
            assert_eq!(runner.count("POST"), 0);
        } else {
            assert_eq!(out.code, 0, "{}", out.stderr);
            assert_eq!(f.state().commit, saved.commit);
        }
        assert_eq!(runner.count("mock-build"), 1);
        assert_eq!(runner.count("commit"), 1);
    }
}

#[test]
fn crash_after_commit_reconciles_actual_git_before_resuming_push() {
    let f = Fixture::current_project();
    let runner = Runner::new(f.plan.clone());
    fs::write(f.root.join("review.txt"), b"pending").unwrap();
    runner.fail_push.set(true);
    assert_ne!(run(&f.root, &runner, Default::default()).code, 0);
    let mut state = f.state();
    let commit = state.commit.take().unwrap();
    state.status = "source-syncing".into();
    fs::write(f.state_path(), serde_json::to_vec(&state).unwrap()).unwrap();
    runner.fail_push.set(false);
    let out = run(&f.root, &runner, Default::default());
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(f.state().commit.as_deref(), Some(commit.as_str()));
    assert_eq!(runner.count("mock-build"), 1);
    assert_eq!(runner.count("commit"), 1);
}

#[test]
fn recovery_blocks_unfinished_git_operations_and_push_target_drift() {
    for marker in ["MERGE_HEAD", "rebase-merge", "index.lock", "push-ref"] {
        let f = Fixture::current_project();
        let runner = Runner::new(f.plan.clone());
        fs::write(f.root.join("review.txt"), b"pending").unwrap();
        runner.fail_commit.set(true);
        assert_ne!(run(&f.root, &runner, Default::default()).code, 0);
        let old = fs::read(f.state_path()).unwrap();
        fs::write(f.root.join("review.txt"), b"corrected").unwrap();
        if marker == "push-ref" {
            process(
                &f.root,
                &["config", "remote.origin.push", "HEAD:refs/heads/another"],
            );
        } else if marker == "rebase-merge" {
            fs::create_dir_all(f.root.join(".git/rebase-merge")).unwrap();
        } else if marker == "index.lock" {
            fs::write(f.root.join(".git/index.lock"), b"active writer").unwrap();
        } else {
            fs::write(
                f.root.join(".git/MERGE_HEAD"),
                f.state().anchor.unwrap().head,
            )
            .unwrap();
        }
        let out = run(&f.root, &runner, Default::default());
        assert_ne!(out.code, 0);
        assert_eq!(runner.count("mock-build"), 1);
        assert_eq!(runner.count("POST"), 0);
        assert_eq!(fs::read(f.state_path()).unwrap(), old);
    }
}

#[test]
fn push_recovery_never_replaces_fixed_commit_with_same_tree_fast_forward() {
    let f = Fixture::current_project();
    let runner = Runner::new(f.plan.clone());
    fs::write(f.root.join("review.txt"), b"pending").unwrap();
    runner.fail_push.set(true);
    assert_ne!(run(&f.root, &runner, Default::default()).code, 0);
    let fixed = f.state().commit.unwrap();
    process(&f.root, &["push"]);
    process(
        &f.base,
        &["clone", "--branch", "main", "remote.git", "other"],
    );
    let other = f.base.join("other");
    for (key, value) in [
        ("user.name", "remote"),
        ("user.email", "remote@example.invalid"),
        ("core.hooksPath", ".runtime/no-hooks"),
    ] {
        process(&other, &["config", key, value]);
    }
    process(
        &other,
        &[
            "commit",
            "--allow-empty",
            "-m",
            "chore: same tree remote change",
        ],
    );
    process(&other, &["push"]);
    runner.fail_push.set(false);
    let out = run(&f.root, &runner, Default::default());
    assert_ne!(out.code, 0);
    assert!(
        out.stderr.contains("fixed source commit changed"),
        "{}",
        out.stderr
    );
    assert_eq!(f.state().commit.as_deref(), Some(fixed.as_str()));
    assert_eq!(f.state().status, "source-uncertain");
    assert_eq!(runner.count("POST"), 0);
    assert_eq!(runner.count("mock-build"), 1);
}
