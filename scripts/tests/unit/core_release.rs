use super::*;
use crate::{ProcessOutput, SystemProcessRunner};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct ReleaseRepository(PathBuf);

impl Drop for ReleaseRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git_ok(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn release_repository(name: &str, contract: &Value, files: &[(&str, &[u8])]) -> ReleaseRepository {
    let mut contract = contract.clone();
    contract
        .as_object_mut()
        .unwrap()
        .entry("release_version")
        .or_insert(json!("8.0.0"));
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "release")
        .join(format!("bridgeforge-release-{name}-{token}"));
    fs::create_dir_all(root.join(".codex")).unwrap();
    fs::write(root.join(".gitignore"), b".runtime/\n").unwrap();
    git_ok(&root, &["init"]);
    git_ok(&root, &["config", "user.name", "BridgeForge Test"]);
    git_ok(
        &root,
        &["config", "user.email", "bridgeforge@example.invalid"],
    );
    fs::write(
        root.join(".codex/managed-skeleton.json"),
        serde_json::to_vec_pretty(&contract).unwrap(),
    )
    .unwrap();
    for (relative, payload) in files {
        let target = root.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(target, payload).unwrap();
    }
    git_ok(&root, &["add", "."]);
    git_ok(&root, &["commit", "-m", "baseline"]);
    ReleaseRepository(root)
}

struct TimeoutRunner;

#[test]
fn immutable_history_reader_batches_payloads_and_never_caches_mutable_head() {
    use std::cell::RefCell;
    struct Counting {
        trees: RefCell<usize>,
        batches: RefCell<usize>,
        shows: RefCell<usize>,
    }
    impl ProcessRunner for Counting {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if request.args.iter().any(|arg| arg == "ls-tree") {
                *self.trees.borrow_mut() += 1;
            }
            if request.args.iter().any(|arg| arg == "cat-file") {
                *self.batches.borrow_mut() += 1;
            }
            if request.args.iter().any(|arg| arg == "show") {
                *self.shows.borrow_mut() += 1;
            }
            SystemProcessRunner.run(request)
        }
    }
    let repo = release_repository(
        "batch-reader",
        &json!({"assets":[]}),
        &[("first.bin", b"\0\xff\n"), ("second.txt", b"second\n")],
    );
    let oid = history::git(&repo.0, &SystemProcessRunner, &["rev-parse", "HEAD"]).unwrap();
    let runner = Counting {
        trees: RefCell::new(0),
        batches: RefCell::new(0),
        shows: RefCell::new(0),
    };
    let reader = history_cache::Reader::new(&repo.0, &runner);
    reader
        .prefetch(
            oid.trim(),
            &[
                "first.bin".into(),
                "second.txt".into(),
                "missing.txt".into(),
            ],
        )
        .unwrap();
    for _ in 0..3 {
        assert_eq!(
            revision_payload(&repo.0, oid.trim(), "first.bin", &reader)
                .unwrap()
                .unwrap(),
            b"\0\xff\n"
        );
        assert_eq!(
            revision_payload(&repo.0, oid.trim(), "second.txt", &reader)
                .unwrap()
                .unwrap(),
            b"second\n"
        );
        assert!(
            revision_payload(&repo.0, oid.trim(), "missing.txt", &reader)
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(*runner.trees.borrow(), 1);
    assert_eq!(*runner.batches.borrow(), 1);
    assert_eq!(*runner.shows.borrow(), 0);
    let inspection = schema4::InspectionRunner(&reader);
    assert_eq!(
        revision_payload(&repo.0, oid.trim(), "second.txt", &inspection)
            .unwrap()
            .unwrap(),
        b"second\n"
    );
    assert_eq!(
        *runner.trees.borrow(),
        1,
        "read-only decorators reuse the same immutable tree"
    );
    assert_eq!(
        *runner.shows.borrow(),
        0,
        "read-only decorators must not bypass prefetched blobs"
    );
    assert_eq!(
        head_payload(&repo.0, "second.txt", &reader)
            .unwrap()
            .unwrap(),
        b"second\n"
    );
    fs::write(repo.0.join("second.txt"), b"new HEAD\n").unwrap();
    git_ok(&repo.0, &["add", "."]);
    git_ok(&repo.0, &["commit", "-m", "fix: new head"]);
    assert_eq!(
        head_payload(&repo.0, "second.txt", &reader)
            .unwrap()
            .unwrap(),
        b"new HEAD\n"
    );
    assert_eq!(*runner.shows.borrow(), 2);
    assert!(!repo.0.join(".runtime").exists());
}

#[test]
fn immutable_history_reader_rejects_failed_or_malformed_batches() {
    struct BadBatch {
        timeout: bool,
    }
    impl ProcessRunner for BadBatch {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if request.args.iter().any(|arg| arg == "cat-file") {
                return Ok(ProcessOutput {
                    code: 0,
                    stdout: b"invalid blob 999\nshort".to_vec(),
                    stderr: Vec::new(),
                    timed_out: self.timeout,
                });
            }
            SystemProcessRunner.run(request)
        }
    }
    let repo = release_repository(
        "bad-batch",
        &json!({"assets":[]}),
        &[("file.bin", b"content")],
    );
    let oid = history::git(&repo.0, &SystemProcessRunner, &["rev-parse", "HEAD"]).unwrap();
    for timeout in [false, true] {
        let runner = BadBatch { timeout };
        let reader = history_cache::Reader::new(&repo.0, &runner);
        assert!(reader.prefetch(oid.trim(), &["file.bin".into()]).is_err());
        assert!(revision_payload(&repo.0, oid.trim(), "file.bin", &reader).is_err());
    }
}

#[test]
fn immutable_history_reader_reuses_blob_identity_across_paths_and_commits() {
    use std::cell::RefCell;
    struct Counting(RefCell<Vec<Vec<u8>>>);
    impl ProcessRunner for Counting {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if request.args.iter().any(|arg| arg == "cat-file") {
                self.0.borrow_mut().push(request.stdin.clone());
            }
            SystemProcessRunner.run(request)
        }
    }
    let repo = release_repository(
        "blob-identity-reuse",
        &json!({"assets":[]}),
        &[("first.txt", b"same\n"), ("alias.txt", b"same\n")],
    );
    let old = history::git(&repo.0, &SystemProcessRunner, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string();
    fs::write(repo.0.join("changed.txt"), b"different\n").unwrap();
    git_ok(&repo.0, &["add", "."]);
    git_ok(&repo.0, &["commit", "-m", "fix: distinct object"]);
    let next = history::git(&repo.0, &SystemProcessRunner, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string();
    let runner = Counting(RefCell::new(Vec::new()));
    let reader = history_cache::Reader::new(&repo.0, &runner);
    reader
        .prefetch(&old, &["first.txt".into(), "alias.txt".into()])
        .unwrap();
    assert_eq!(runner.0.borrow().len(), 1);
    assert_eq!(
        runner.0.borrow()[0].iter().filter(|b| **b == b'\n').count(),
        1,
        "one request per object, not per path"
    );
    reader
        .prefetch(&next, &["first.txt".into(), "alias.txt".into()])
        .unwrap();
    assert_eq!(
        runner.0.borrow().len(),
        1,
        "unchanged object reused across fixed revisions"
    );
    assert_eq!(
        revision_payload(&repo.0, &next, "changed.txt", &reader)
            .unwrap()
            .unwrap(),
        b"different\n"
    );
    assert_eq!(runner.0.borrow().len(), 2, "new object must be fetched");
    assert_eq!(
        revision_payload(&repo.0, &old, "first.txt", &reader)
            .unwrap()
            .unwrap(),
        b"same\n"
    );
    reader.verify_unchanged().unwrap();
    git_ok(&repo.0, &["replace", &old, &next]);
    assert!(
        reader.verify_unchanged().is_err(),
        "history interpretation drift must still block apply"
    );
    let replaced = history_cache::Reader::new(&repo.0, &runner);
    assert_eq!(
        revision_payload(&repo.0, &old, "changed.txt", &replaced)
            .unwrap()
            .unwrap(),
        b"different\n"
    );
    assert_eq!(
        runner.0.borrow().len(),
        2,
        "replacement histories bypass the object cache"
    );
}

#[test]
fn immutable_history_hints_never_add_failures_or_lazy_fetches() {
    struct FailBlob {
        oid: String,
        promisor: bool,
        batches: std::cell::Cell<usize>,
    }
    impl ProcessRunner for FailBlob {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if self.promisor && request.args.iter().any(|a| a == "--get-regexp") {
                return Ok(ProcessOutput {
                    code: 0,
                    stdout: b"remote.origin.promisor true\n".to_vec(),
                    stderr: vec![],
                    timed_out: false,
                });
            }
            if request.args.iter().any(|a| a == "cat-file") {
                self.batches.set(self.batches.get() + 1);
            }
            if request.args.iter().any(|a| a == "cat-file")
                && String::from_utf8_lossy(&request.stdin)
                    .lines()
                    .any(|oid| oid == self.oid)
            {
                return Ok(ProcessOutput {
                    code: 1,
                    stdout: vec![],
                    stderr: b"unavailable object".to_vec(),
                    timed_out: false,
                });
            }
            SystemProcessRunner.run(request)
        }
    }
    let repo = release_repository(
        "optional-prefetch",
        &json!({"assets":[]}),
        &[
            ("needed.txt", b"required\n"),
            ("hint.txt", b"not consumed\n"),
        ],
    );
    let oid = history::git(&repo.0, &SystemProcessRunner, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string();
    let unavailable = history::git(
        &repo.0,
        &SystemProcessRunner,
        &["rev-parse", "HEAD:hint.txt"],
    )
    .unwrap()
    .trim()
    .to_string();
    let runner = FailBlob {
        oid: unavailable,
        promisor: false,
        batches: std::cell::Cell::new(0),
    };
    let reader = history_cache::Reader::new(&repo.0, &runner);
    reader.prefetch_hints(
        &BTreeSet::from([oid.as_str()]),
        &["needed.txt".into(), "hint.txt".into()],
    );
    assert_eq!(
        revision_payload(&repo.0, &oid, "needed.txt", &reader)
            .unwrap()
            .unwrap(),
        b"required\n"
    );
    assert!(
        revision_payload(&repo.0, &oid, "hint.txt", &reader).is_err(),
        "required failed reads cannot be masked by a speculative failure"
    );
    let runner = FailBlob {
        oid: history::git(
            &repo.0,
            &SystemProcessRunner,
            &["rev-parse", "HEAD:needed.txt"],
        )
        .unwrap()
        .trim()
        .into(),
        promisor: true,
        batches: std::cell::Cell::new(0),
    };
    let reader = history_cache::Reader::new(&repo.0, &runner);
    reader.prefetch_hints(&BTreeSet::from([oid.as_str()]), &["needed.txt".into()]);
    assert_eq!(
        runner.batches.get(),
        0,
        "partial clone hints must not request objects"
    );
    assert!(
        revision_payload(&repo.0, &oid, "needed.txt", &reader).is_err(),
        "promisor hints must not supply invented success"
    );
}

#[test]
fn synchronized_history_skips_unowned_gitlinks_and_large_business_blobs() {
    use std::cell::RefCell;
    struct Counting(RefCell<Vec<Vec<u8>>>);
    impl ProcessRunner for Counting {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if request.args.iter().any(|arg| arg == "cat-file") {
                self.0.borrow_mut().push(request.stdin.clone());
            }
            SystemProcessRunner.run(request)
        }
    }
    let repo = explicit_repository("gitlink-release");
    let parent = history::git(&repo.0, &SystemProcessRunner, &["rev-parse", "HEAD"]).unwrap();
    git_ok(
        &repo.0,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000",
            parent.trim(),
            "vendor/module",
        ],
    );
    fs::write(repo.0.join("business.bin"), vec![42u8; 1024 * 1024]).unwrap();
    git_ok(&repo.0, &["add", "business.bin"]);
    git_ok(&repo.0, &["commit", "-m", "feat: add business assets"]);
    let next = history::git(&repo.0, &SystemProcessRunner, &["rev-parse", "HEAD"]).unwrap();
    git_ok(
        &repo.0,
        &[
            "update-index",
            "--cacheinfo",
            "160000",
            next.trim(),
            "vendor/module",
        ],
    );
    git_ok(&repo.0, &["commit", "-m", "fix: advance submodule"]);
    let blob = history::git(
        &repo.0,
        &SystemProcessRunner,
        &["rev-parse", "HEAD:business.bin"],
    )
    .unwrap();
    let runner = Counting(RefCell::new(Vec::new()));
    let reader = history_cache::Reader::new(&repo.0, &runner);
    reader
        .prefetch(next.trim(), &["Cargo.toml".into(), "Cargo.lock".into()])
        .unwrap();
    assert!(!runner.0.borrow().iter().any(|input| {
        String::from_utf8_lossy(input)
            .lines()
            .any(|oid| oid == blob.trim())
    }));
}

impl ProcessRunner for TimeoutRunner {
    fn run(&self, _request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        Ok(ProcessOutput {
            code: -1,
            stdout: Vec::new(),
            stderr: Vec::new(),
            timed_out: true,
        })
    }
}

#[test]
fn head_payload_timeout_is_a_hard_error() {
    let error = head_payload(Path::new("."), "managed.md", &TimeoutRunner).unwrap_err();
    assert!(error.contains("timed out"), "{error}");
}

#[test]
fn gitattributes_adoption_preserves_existing_project_rules() {
    let head_contract = json!({
        "contract_target": ".codex/managed-skeleton.json",
        "assets": []
    });
    let current_contract = json!({
        "contract_target": ".codex/managed-skeleton.json",
        "assets": [{
            "id": "root.gitattributes",
            "target": ".gitattributes",
            "strategy": "merge",
            "merge_policy": "git-attributes-default-lf",
            "merge_validation": {
                "required": {"pattern": "*", "text": "auto", "eol": "lf"}
            }
        }]
    });
    let repo = release_repository(
        "gitattributes-adoption",
        &head_contract,
        &[(".gitattributes", b"*.bat text eol=crlf\n")],
    );
    fs::write(
        repo.0.join(".codex/managed-skeleton.json"),
        serde_json::to_vec_pretty(&current_contract).unwrap(),
    )
    .unwrap();
    fs::write(
        repo.0.join(".gitattributes"),
        b"* text=auto eol=lf\n*.bat text eol=crlf\n",
    )
    .unwrap();
    let changed = [
        ".codex/managed-skeleton.json".into(),
        ".gitattributes".into(),
    ];
    assert_eq!(
        classify(&repo.0, &changed, &SystemProcessRunner).unwrap(),
        ReleaseKind::SkeletonOnly
    );
    fs::write(
        repo.0.join(".gitattributes"),
        b"* text=auto eol=lf\n*.bat text eol=crlf working-tree-encoding=UTF-8\n",
    )
    .unwrap();
    assert_eq!(
        classify(&repo.0, &changed, &SystemProcessRunner).unwrap(),
        ReleaseKind::Business
    );
}

#[test]
fn invalid_head_payload_never_becomes_a_trusted_same_contract_baseline() {
    let contract = json!({
        "contract_target": ".codex/managed-skeleton.json",
        "assets": [{
            "id": "managed.whole",
            "target": "managed.txt",
            "strategy": "whole",
            "current_sha256": digest(b"good\n")
        }]
    });
    let repo = release_repository("invalid-head", &contract, &[("managed.txt", b"bad\n")]);
    fs::write(repo.0.join("managed.txt"), b"good\n").unwrap();
    let error = classify(&repo.0, &["managed.txt".into()], &SystemProcessRunner).unwrap_err();
    assert!(
        error.contains("HEAD ownership baseline is invalid"),
        "{error}"
    );
}

#[test]
fn invalid_old_payload_during_contract_transition_is_conservative_business() {
    let head_contract = json!({
        "contract_target": ".codex/managed-skeleton.json",
        "assets": [{
            "id": "managed.whole",
            "target": "managed.txt",
            "strategy": "whole",
            "current_sha256": digest(b"claimed-old\n")
        }]
    });
    let current_contract = json!({
        "contract_target": ".codex/managed-skeleton.json",
        "assets": [{
            "id": "managed.whole",
            "target": "managed.txt",
            "strategy": "whole",
            "current_sha256": digest(b"new\n")
        }]
    });
    let repo = release_repository(
        "invalid-old-transition",
        &head_contract,
        &[("managed.txt", b"not-claimed-old\n")],
    );
    fs::write(
        repo.0.join(".codex/managed-skeleton.json"),
        serde_json::to_vec_pretty(&current_contract).unwrap(),
    )
    .unwrap();
    fs::write(repo.0.join("managed.txt"), b"new\n").unwrap();
    assert_eq!(
        classify(
            &repo.0,
            &[".codex/managed-skeleton.json".into(), "managed.txt".into()],
            &SystemProcessRunner,
        )
        .unwrap(),
        ReleaseKind::Business
    );
}

#[test]
fn semver_is_strict_and_ordered() {
    assert!("1.02.3".parse::<SemVer>().is_err());
    assert!("1.2".parse::<SemVer>().is_err());
    assert!("1.9.9".parse::<SemVer>().unwrap() < "2.0.0".parse().unwrap());
}

#[test]
fn conventional_commit_parses_description_and_breaking_annotation() {
    let info =
        parse_commit_message("feat(core)!: replace runtime\n\nBREAKING CHANGE: changed").unwrap();
    assert!(info.breaking);
    assert_eq!(info.kind, "feat");
    assert!(parse_commit_message("update runtime").is_err());
}

#[test]
fn region_projection_distinguishes_public_and_project_changes() {
    let head_contract = json!({
        "contract_target": ".codex/managed-skeleton.json",
        "assets": [{
            "id": "managed.region",
            "target": "managed.md",
            "strategy": "region",
            "region": {
                "begin": "BEGIN",
                "end": "END",
                "current_sha256": digest(b"BEGIN\nold public\nEND\n")
            }
        }]
    });
    let current_contract = json!({
        "contract_target": ".codex/managed-skeleton.json",
        "assets": [{
            "id": "managed.region",
            "target": "managed.md",
            "strategy": "region",
            "region": {
                "begin": "BEGIN",
                "end": "END",
                "current_sha256": digest(b"BEGIN\nnew public\nEND\n")
            }
        }]
    });
    let repo = release_repository(
        "region",
        &head_contract,
        &[("managed.md", b"BEGIN\nold public\nEND\nproject\n")],
    );
    fs::write(
        repo.0.join(".codex/managed-skeleton.json"),
        serde_json::to_vec_pretty(&current_contract).unwrap(),
    )
    .unwrap();
    fs::write(
        repo.0.join("managed.md"),
        b"BEGIN\nnew public\nEND\nproject\n",
    )
    .unwrap();
    assert_eq!(
        classify(
            &repo.0,
            &[".codex/managed-skeleton.json".into(), "managed.md".into()],
            &SystemProcessRunner,
        )
        .unwrap(),
        ReleaseKind::SkeletonOnly
    );
    fs::write(
        repo.0.join("managed.md"),
        b"BEGIN\nnew public\nEND\nproject changed\n",
    )
    .unwrap();
    assert_eq!(
        classify(
            &repo.0,
            &[".codex/managed-skeleton.json".into(), "managed.md".into()],
            &SystemProcessRunner,
        )
        .unwrap(),
        ReleaseKind::Business
    );
}

#[test]
fn attribute_projection_tracks_public_exceptions_without_hiding_project_edits() {
    let old = json!({"strategy":"merge","merge_policy":"git-attributes-default-lf"});
    let mut new = old.clone();
    new["merge_validation"] = json!({"required_rules":[
        "* text=auto eol=lf", "*.bat text eol=crlf", "*.cmd text eol=crlf"
    ]});
    let before = b"* text=auto eol=lf\n*.ps1 text eol=crlf\n";
    let after =
        b"* text=auto eol=lf\n*.bat text eol=crlf\n*.cmd text eol=crlf\n*.ps1 text eol=crlf\n";
    let (old_public, old_project) = ownership_projection(&old, before).unwrap();
    let (new_public, new_project) = ownership_projection(&new, after).unwrap();
    assert_ne!(old_public, new_public);
    assert_eq!(old_project, new_project);
    let edited =
        b"* text=auto eol=lf\n*.bat text eol=crlf\n*.cmd text eol=crlf\n*.ps1 text eol=lf\n";
    assert_ne!(ownership_projection(&new, edited).unwrap().1, new_project);
}

#[test]
fn attribute_policy_adoption_does_not_turn_existing_batch_rules_into_business_changes() {
    let old = json!({"contract_target":".codex/managed-skeleton.json","assets":[{
        "id":"root.gitattributes","target":".gitattributes","strategy":"merge",
        "merge_policy":"git-attributes-default-lf"
    }]});
    let mut new = old.clone();
    new["assets"][0]["merge_validation"] = json!({"required_rules":[
        "* text=auto eol=lf", "*.bat text eol=crlf", "*.cmd text eol=crlf"
    ]});
    for existing in [
        "*.bat text eol=crlf\n",
        "*.cmd text eol=crlf\n",
        "*.bat text eol=crlf\n*.cmd text eol=crlf\n",
    ] {
        let before = format!("* text=auto eol=lf\n{existing}*.ps1 text eol=crlf\n");
        let repo = release_repository(
            "attribute-policy-adoption",
            &old,
            &[(".gitattributes", before.as_bytes())],
        );
        fs::write(
            repo.0.join(".codex/managed-skeleton.json"),
            serde_json::to_vec(&new).unwrap(),
        )
        .unwrap();
        let after =
            b"* text=auto eol=lf\n*.bat text eol=crlf\n*.cmd text eol=crlf\n*.ps1 text eol=crlf\n";
        fs::write(repo.0.join(".gitattributes"), after).unwrap();
        let changed = [
            ".codex/managed-skeleton.json".into(),
            ".gitattributes".into(),
        ];
        assert_eq!(
            classify(&repo.0, &changed, &SystemProcessRunner).unwrap(),
            ReleaseKind::SkeletonOnly
        );
        fs::write(
            repo.0.join(".gitattributes"),
            String::from_utf8_lossy(after).replace("*.ps1 text eol=crlf", "*.ps1 text eol=lf"),
        )
        .unwrap();
        assert_eq!(
            classify(&repo.0, &changed, &SystemProcessRunner).unwrap(),
            ReleaseKind::Business
        );
    }
}

#[test]
fn hooks_json_projection_preserves_external_handler_ownership() {
    let handler = |command: &str| {
        json!({
            "bridgeforgeCodexId": "bridgeforge-codex.project-hook.v1:stop",
            "command": command
        })
    };
    let contract = |command: &str| {
        json!({
            "contract_target": ".codex/managed-skeleton.json",
            "assets": [{
                "id": "hooks",
                "target": ".codex/hooks.json",
                "strategy": "merge",
                "merge_policy": "codex-hooks",
                "merge_validation": {
                    "required_handlers": [{
                        "id": "bridgeforge-codex.project-hook.v1:stop",
                        "event": "Stop",
                        "matcher": "",
                        "sha256": canonical_digest(&handler(command)).unwrap()
                    }]
                }
            }]
        })
    };
    let hooks = |managed: &str, external: &str| {
        serde_json::to_vec(&json!({
        "description": "managed",
        "hooks": {"Stop": [{"hooks": [
            {"bridgeforgeCodexId": "bridgeforge-codex.project-hook.v1:stop", "command": managed},
            {"command": external}
        ]}]}
    })).unwrap()
    };
    let head = hooks("old", "external");
    let repo = release_repository("hooks", &contract("old"), &[(".codex/hooks.json", &head)]);
    fs::write(
        repo.0.join(".codex/managed-skeleton.json"),
        serde_json::to_vec_pretty(&contract("new")).unwrap(),
    )
    .unwrap();
    fs::write(repo.0.join(".codex/hooks.json"), hooks("new", "external")).unwrap();
    assert_eq!(
        classify(
            &repo.0,
            &[
                ".codex/managed-skeleton.json".into(),
                ".codex/hooks.json".into()
            ],
            &SystemProcessRunner,
        )
        .unwrap(),
        ReleaseKind::SkeletonOnly
    );
    fs::write(
        repo.0.join(".codex/hooks.json"),
        hooks("new", "external changed"),
    )
    .unwrap();
    assert_eq!(
        classify(
            &repo.0,
            &[
                ".codex/managed-skeleton.json".into(),
                ".codex/hooks.json".into()
            ],
            &SystemProcessRunner,
        )
        .unwrap(),
        ReleaseKind::Business
    );
}

#[test]
fn cross_contract_marker_migration_uses_each_sides_contract() {
    let head_contract = json!({
        "contract_target": ".codex/managed-skeleton.json",
        "stamp": ".codex/version",
        "assets": [{
            "id": "managed.region",
            "target": "managed.md",
            "strategy": "region",
            "region": {
                "begin": "OLD-BEGIN",
                "end": "OLD-END",
                "current_sha256": digest(b"OLD-BEGIN\nold public\nOLD-END\n")
            }
        }]
    });
    let current_contract = json!({
        "contract_target": ".codex/managed-skeleton.json",
        "stamp": ".codex/version",
        "assets": [{
            "id": "managed.region",
            "target": "managed.md",
            "strategy": "region",
            "region": {
                "begin": "NEW-BEGIN",
                "end": "NEW-END",
                "current_sha256": digest(b"NEW-BEGIN\nnew public\nNEW-END\n")
            }
        }]
    });
    let repo = release_repository(
        "transition",
        &head_contract,
        &[("managed.md", b"OLD-BEGIN\nold public\nOLD-END\nproject\n")],
    );
    fs::write(
        repo.0.join(".codex/managed-skeleton.json"),
        serde_json::to_vec_pretty(&current_contract).unwrap(),
    )
    .unwrap();
    fs::write(
        repo.0.join("managed.md"),
        b"NEW-BEGIN\nnew public\nNEW-END\nproject\n",
    )
    .unwrap();
    assert_eq!(
        classify(
            &repo.0,
            &[".codex/managed-skeleton.json".into(), "managed.md".into()],
            &SystemProcessRunner,
        )
        .unwrap(),
        ReleaseKind::SkeletonOnly
    );
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        value => value.clone(),
    }
}

#[test]
fn relocated_asset_classification_checks_identity_payloads_and_both_paths() {
    const OLD: &str = ".codex/hooks/crates/bridgeforge-core/src/memory/ownership.rs";
    const NEW: &str = ".codex/hooks/source/memory/src/ownership.rs";
    const CONTRACT: &str = ".codex/managed-skeleton.json";
    let asset = |target: &str, payload: &[u8]| {
        json!({
            "id":"managed.memory", "target":target, "strategy":"whole",
            "current_sha256":digest(payload)
        })
    };
    // Each path is classified individually by schema 4. Including the contract
    // path in this call would hide the unchanged-content migration regression.
    for payload in [
        b"original\n".as_slice(),
        b"updated\n".as_slice(),
        b"".as_slice(),
    ] {
        let old = json!({"assets":[asset(OLD, b"original\n")]});
        let current = json!({"assets":[asset(NEW, payload)]});
        let baseline = BTreeMap::from([
            (CONTRACT, serde_json::to_vec(&old).unwrap()),
            (OLD, b"original\n".to_vec()),
        ]);
        let final_files = BTreeMap::from([(NEW, payload.to_vec())]);
        let classify_maps = |before: &BTreeMap<&str, Vec<u8>>,
                             after: &BTreeMap<&str, Vec<u8>>,
                             contract: &Value,
                             path: &str| {
            classify_payloads(
                &[path.into()],
                &serde_json::to_vec(contract).unwrap(),
                |p| Ok(before.get(p).cloned()),
                |p| Ok(after.get(p).cloned()),
            )
        };
        for path in [OLD, NEW] {
            assert_eq!(
                classify_maps(&baseline, &final_files, &current, path).unwrap(),
                ReleaseKind::SkeletonOnly,
                "{path}: {payload:?}"
            );
            let mut leftover = final_files.clone();
            leftover.insert(OLD, b"user retained or modified file\n".to_vec());
            assert_eq!(
                classify_maps(&baseline, &leftover, &current, path).unwrap(),
                ReleaseKind::Business
            );
            let mut occupied = baseline.clone();
            occupied.insert(NEW, b"preexisting project file\n".to_vec());
            assert_eq!(
                classify_maps(&occupied, &final_files, &current, path).unwrap(),
                ReleaseKind::Business
            );
            let mut edited_old = baseline.clone();
            edited_old.insert(OLD, b"unverified old payload\n".to_vec());
            assert_eq!(
                classify_maps(&edited_old, &final_files, &current, path).unwrap(),
                ReleaseKind::Business
            );
            let mut missing_old = baseline.clone();
            missing_old.remove(OLD);
            assert_eq!(
                classify_maps(&missing_old, &final_files, &current, path).unwrap(),
                ReleaseKind::Business
            );
            assert!(classify_maps(&baseline, &BTreeMap::new(), &current, path).is_err());
            let tampered = BTreeMap::from([(NEW, b"unverified new payload\n".to_vec())]);
            assert!(classify_maps(&baseline, &tampered, &current, path).is_err());
        }
        assert_eq!(
            classify_maps(&baseline, &final_files, &current, ".codex/unknown.rs").unwrap(),
            ReleaseKind::Business
        );
        let conflict = json!({"assets":[asset(NEW, payload), asset(OLD, b"original\n")]});
        assert!(
            classify_maps(&baseline, &final_files, &conflict, OLD)
                .unwrap_err()
                .contains("duplicated")
        );
        let mut conflict = current.clone();
        let mut other = asset(OLD, b"original\n");
        other["id"] = json!("different.identity");
        conflict["assets"].as_array_mut().unwrap().push(other);
        assert!(
            classify_maps(&baseline, &final_files, &conflict, OLD)
                .unwrap_err()
                .contains("identities disagree")
        );
    }
}

#[test]
fn relocated_region_keeps_project_content_in_release() {
    let asset = |target: &str| {
        json!({"id":"region", "target":target, "strategy":"region",
        "region":{"begin":"BEGIN", "end":"END", "current_sha256":digest(b"BEGIN\npublic\nEND\n")}})
    };
    let old = serde_json::to_vec(&json!({"assets":[asset("old.md")]})).unwrap();
    let current = serde_json::to_vec(&json!({"assets":[asset("new.md")]})).unwrap();
    for (content, expected) in [
        (
            b"BEGIN\npublic\nEND\nproject\n".as_slice(),
            ReleaseKind::SkeletonOnly,
        ),
        (
            b"BEGIN\npublic\nEND\nchanged\n".as_slice(),
            ReleaseKind::Business,
        ),
    ] {
        let result = classify_payloads(
            &["old.md".into()],
            &current,
            |p| {
                Ok(match p {
                    ".codex/managed-skeleton.json" => Some(old.clone()),
                    "old.md" => Some(b"BEGIN\npublic\nEND\nproject\n".to_vec()),
                    _ => None,
                })
            },
            |p| Ok((p == "new.md").then(|| content.to_vec())),
        )
        .unwrap();
        assert_eq!(result, expected);
    }
}

#[test]
fn relocated_seed_remains_project_owned() {
    let old = serde_json::to_vec(&json!({"assets":[{
        "id":"seed", "target":"old.md", "strategy":"seed"
    }]}))
    .unwrap();
    let current = serde_json::to_vec(&json!({"assets":[{
        "id":"seed", "target":"new.md", "strategy":"seed"
    }]}))
    .unwrap();
    for path in ["old.md", "new.md"] {
        let result = classify_payloads(
            &[path.into()],
            &current,
            |p| {
                Ok(match p {
                    ".codex/managed-skeleton.json" => Some(old.clone()),
                    "old.md" => Some(b"project\n".to_vec()),
                    _ => None,
                })
            },
            |p| Ok((p == "new.md").then(|| b"project\n".to_vec())),
        )
        .unwrap();
        assert_eq!(result, ReleaseKind::Business);
    }
}

fn canonical_digest(value: &Value) -> Result<String, String> {
    serde_json::to_vec(&canonical_json(value))
        .map(|payload| digest(&payload))
        .map_err(|error| error.to_string())
}

fn explicit_repository(name: &str) -> ReleaseRepository {
    release_repository(
        name,
        &json!({"contract_target": ".codex/managed-skeleton.json", "assets": []}),
        &[
            ("VERSION", b"1.47.11\n"),
            ("CHANGELOG.md", b"# Changelog\n"),
            (
                "Cargo.toml",
                b"[package]\nname = \"example\"\nversion = \"1.47.11\"\n",
            ),
            (
                "Cargo.lock",
                b"version = 4\n[[package]]\nname = \"example\"\nversion = \"1.47.11\"\n",
            ),
            ("business.txt", b"base\n"),
        ],
    )
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
