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
            if args.windows(2).any(|a| a == ["remote", "get-url"]) {
                return Ok(ProcessOutput {
                    stdout: b"https://github.com/fixture/repository.git\n".to_vec(),
                    ..empty()
                });
            }
            return SystemProcessRunner.run(q);
        }
        if program == "mock-build" {
            return Ok(if self.fail_build.get() {
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
            return Ok(ok(&serde_json::to_value(&self.plan).unwrap()));
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
            return Ok(self
                .tag
                .borrow()
                .as_ref()
                .map(|sha| ok(&json!({"object":{"type":"commit","sha":sha}})))
                .unwrap_or_else(missing));
        }
        if method == "GET" {
            if endpoint.contains("/releases/tags/") {
                return Ok(self
                    .remote
                    .borrow()
                    .as_ref()
                    .filter(|v| v["draft"] == false)
                    .map(ok)
                    .unwrap_or_else(missing));
            }
            if endpoint.contains("/releases?per_page=") {
                return Ok(ok(&json!(
                    self.remote.borrow().iter().cloned().collect::<Vec<_>>()
                )));
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
            v["html_url"] = json!("https://github.com/fixture/repository/releases/tag/bp-v1.0.0");
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
