use super::*;
#[path = "../support/git_repository.rs"]
mod repository;
use crate::{ProcessOutput, ProcessRequest, SystemProcessRunner};
use repository::*;
use std::cell::RefCell;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn unfinished_development_transaction_blocks_sync_before_network() {
    struct LocalInspectionOnly(std::cell::Cell<usize>, PathBuf);
    impl ProcessRunner for LocalInspectionOnly {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            self.0.set(self.0.get() + 1);
            if request.program == "git"
                && (request.args == ["rev-parse", "--git-dir"]
                    || request.args == ["rev-parse", "--path-format=absolute", "--git-common-dir"])
            {
                return Ok(ProcessOutput {
                    code: 0,
                    stdout: self.1.join(".git").to_string_lossy().as_bytes().to_vec(),
                    stderr: Vec::new(),
                    timed_out: false,
                });
            }
            Err(std::io::Error::other(
                "only local identity inspection is permitted before recovery",
            ))
        }
    }
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "git-sync").join(format!(
        "bf-sync-unfinished-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join(".runtime/bridgeforge-codex")).unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(
        root.join(".runtime/bridgeforge-codex/develop-transaction.json"),
        b"{}",
    )
    .unwrap();
    let runner = LocalInspectionOnly(std::cell::Cell::new(0), root.clone());
    let result = super::sync(&root, &runner, GitSyncOptions::default());
    assert!(
        result
            .stderr
            .contains("unfinished historical development transaction"),
        "{}",
        result.stderr
    );
    assert_eq!(
        runner.0.get(),
        1,
        "only repository discovery runs before the unfinished-transaction guard"
    );
    assert_eq!(
        fs::read(root.join(".runtime/bridgeforge-codex/develop-transaction.json")).unwrap(),
        b"{}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn runtime_maintenance_still_serializes_linked_worktrees() {
    let repo = real_repository("shared-lock");
    let linked = repo.0.join("linked");
    git_ok(
        &repo.0,
        &[
            "worktree",
            "add",
            "-b",
            "lock-linked",
            linked.to_str().unwrap(),
        ],
    );
    let main_git = Git {
        root: &repo.0,
        runner: &SystemProcessRunner,
    };
    let linked_git = Git {
        root: &linked,
        runner: &SystemProcessRunner,
    };
    let held = RuntimeLock::acquire(&main_git).unwrap();
    assert!(RuntimeLock::acquire(&linked_git).is_err());
    drop(held);
    drop(RuntimeLock::acquire(&linked_git).unwrap());
    assert!(repo.0.join(".git/bridgeforge-git-sync.lock").exists());
}

#[test]
fn ordinary_sync_does_not_acquire_runtime_maintenance_lock() {
    let (repo, remote) = managed_repository("sync-without-maintenance-lock");
    let held = lock_factory_runtime(&repo.0, &SystemProcessRunner).unwrap();
    let result = sync(&repo.0, &SystemProcessRunner, GitSyncOptions::default());
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(result.receipt.unwrap()["status"], "synced");
    drop(held);
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn distinct_push_target_receives_commits_even_when_upstream_has_parity() {
    let (repo, upstream) = managed_repository("distinct-push");
    let fork = repo.0.with_extension("fork.git");
    git_ok(
        &repo.0,
        &[
            "clone",
            "--bare",
            upstream.to_str().unwrap(),
            fork.to_str().unwrap(),
        ],
    );
    git_ok(&repo.0, &["remote", "add", "fork", fork.to_str().unwrap()]);
    fs::write(
        repo.0.join("tracked.txt"),
        b"new commit already on upstream\n",
    )
    .unwrap();
    git_ok(&repo.0, &["add", "tracked.txt"]);
    git_ok(&repo.0, &["commit", "-m", "advance upstream"]);
    git_ok(&repo.0, &["push", "origin", "HEAD"]);
    git_ok(&repo.0, &["config", "remote.pushDefault", "fork"]);
    git_ok(&repo.0, &["config", "push.default", "current"]);
    git_ok(&repo.0, &["fetch", "fork"]);
    let git = Git {
        root: &repo.0,
        runner: &SystemProcessRunner,
    };
    assert_eq!(git.ahead_behind().unwrap(), (0, 0));
    assert_eq!(git.ahead_behind_target("@{push}").unwrap(), (1, 0));
    let result = sync(&repo.0, &SystemProcessRunner, GitSyncOptions::default());
    assert_eq!(result.code, 0, "{}", result.stderr);
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt["status"], "synced");
    assert_eq!(receipt["push_performed"], true);
    assert_eq!(git.ahead_behind_target("@{push}").unwrap(), (0, 0));
    let remote_head = git
        .required(&["ls-remote", "fork", "HEAD"], Duration::from_secs(10))
        .unwrap();
    assert!(remote_head.starts_with(receipt["commit"].as_str().unwrap()));
    fs::remove_dir_all(upstream).unwrap();
    fs::remove_dir_all(fork).unwrap();
}

struct FakeRunner {
    outputs: RefCell<Vec<ProcessOutput>>,
}

#[test]
fn remote_context_rejects_invalid_cli_combinations_before_git_mutation() {
    let (repo, remote) = managed_repository("remote-context-flags");
    let index = fs::read(repo.0.join(".git/index")).unwrap();
    let head = git_output(&repo.0, &["rev-parse", "HEAD"]);
    for flags in [
        vec!["--dry-run"],
        vec!["--check"],
        vec!["--release"],
        vec!["--preflight"],
        vec!["--skip-fetch"],
        vec!["--release-analysis-file", ".runtime/analysis.json"],
    ] {
        let mut args = vec![
            "git-sync".to_string(),
            "--sync-remote".into(),
            "--root".into(),
            repo.0.to_string_lossy().into_owned(),
        ];
        args.extend(flags.into_iter().map(str::to_string));
        let result = crate::git_command::run(&args, &SystemProcessRunner);
        assert_ne!(result.code, 0, "{args:?}");
        assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), index);
        assert_eq!(git_output(&repo.0, &["rev-parse", "HEAD"]), head);
        assert!(!repo.0.join(".git/FETCH_HEAD").exists());
    }
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn remote_context_divergence_and_restore_conflict_preserve_work() {
    for committed in [false, true] {
        let (repo, remote) = managed_repository("remote-context-conflict");
        git_ok(&repo.0, &["config", "core.autocrlf", "false"]);
        let peer = repo.0.join(".runtime/peer");
        fs::create_dir_all(peer.parent().unwrap()).unwrap();
        git_ok(
            &repo.0,
            &["clone", remote.to_str().unwrap(), peer.to_str().unwrap()],
        );
        git_ok(&peer, &["config", "user.name", "Peer"]);
        git_ok(&peer, &["config", "user.email", "peer@example.invalid"]);
        git_ok(&peer, &["config", "core.autocrlf", "false"]);
        fs::write(peer.join("tracked.txt"), b"remote line\n").unwrap();
        git_ok(&peer, &["commit", "-am", "fix: remote line"]);
        git_ok(&peer, &["push"]);
        fs::write(repo.0.join("tracked.txt"), b"local line\n").unwrap();
        if committed {
            git_ok(&repo.0, &["commit", "-am", "fix: local line"]);
        }
        let head = git_output(&repo.0, &["rev-parse", "HEAD"]);
        let result = super::sync_remote(&repo.0, &SystemProcessRunner, GitSyncOptions::default());
        assert_ne!(result.code, 0);
        let receipt = result.receipt.unwrap();
        assert_eq!(receipt["push_performed"], false);
        if committed {
            assert_eq!(receipt["status"], "diverged");
            assert_eq!(git_output(&repo.0, &["rev-parse", "HEAD"]), head);
            assert_eq!(
                fs::read(repo.0.join("tracked.txt")).unwrap(),
                b"local line\n"
            );
        } else {
            assert_eq!(receipt["status"], "autostash-retained");
            assert_eq!(receipt["autostash_retained"], true);
            assert_eq!(
                git_output(&repo.0, &["show", "stash@{0}:tracked.txt"]),
                "local line"
            );
            assert!(!git_output(&repo.0, &["diff", "--name-only", "--diff-filter=U"]).is_empty());
        }
        fs::remove_dir_all(remote).unwrap();
    }
}

#[test]
fn remote_context_preserves_dirty_files_and_never_commits_or_pushes() {
    let (repo, remote) = versioned_repository("remote-context");
    let head = git_output(&repo.0, &["rev-parse", "HEAD"]);
    fs::write(repo.0.join("tracked.txt"), b"local work\n").unwrap();
    let result = super::sync_remote(&repo.0, &SystemProcessRunner, GitSyncOptions::default());
    assert_eq!(result.code, 0, "{}", result.stderr);
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt["status"], "remote-ready");
    assert_eq!(receipt["head"], head);
    assert_eq!(receipt["working_tree"], "dirty");
    assert_eq!(receipt["push_performed"], false);
    assert_eq!(receipt["commit_performed"], false);
    assert_eq!(receipt["version_bumped"], false);
    assert_eq!(
        fs::read(repo.0.join("tracked.txt")).unwrap(),
        b"local work\n"
    );
    assert_eq!(fs::read(repo.0.join("VERSION")).unwrap(), b"1.0.0\n");
    assert!(!repo.0.join(".git/bridgeforge-git-sync.lock").exists());
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn remote_context_fast_forwards_before_analysis_and_restores_local_work() {
    let (repo, remote) = managed_repository("remote-context-fast-forward");
    git_ok(&repo.0, &["config", "core.autocrlf", "false"]);
    let peer = repo.0.join(".runtime/peer");
    fs::create_dir_all(peer.parent().unwrap()).unwrap();
    git_ok(
        &repo.0,
        &["clone", remote.to_str().unwrap(), peer.to_str().unwrap()],
    );
    git_ok(&peer, &["config", "user.name", "Peer"]);
    git_ok(&peer, &["config", "user.email", "peer@example.invalid"]);
    git_ok(&peer, &["config", "core.autocrlf", "false"]);
    fs::write(peer.join("remote.txt"), b"remote work\n").unwrap();
    git_ok(&peer, &["add", "remote.txt"]);
    git_ok(&peer, &["commit", "-m", "feat: remote change"]);
    git_ok(&peer, &["push"]);
    let target = git_output(&peer, &["rev-parse", "HEAD"]);
    fs::write(repo.0.join("tracked.txt"), b"local work\n").unwrap();
    fs::write(repo.0.join("untracked.txt"), b"new local work\n").unwrap();
    let result = super::sync_remote(&repo.0, &SystemProcessRunner, GitSyncOptions::default());
    assert_eq!(result.code, 0, "{}", result.stderr);
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt["head"], target);
    assert_eq!(receipt["behind"], 0);
    assert_eq!(receipt["autostash_retained"], false);
    assert_eq!(
        fs::read(repo.0.join("tracked.txt")).unwrap(),
        b"local work\n"
    );
    assert_eq!(
        fs::read(repo.0.join("untracked.txt")).unwrap(),
        b"new local work\n"
    );
    assert_eq!(
        fs::read(repo.0.join("remote.txt")).unwrap(),
        b"remote work\n"
    );
    assert!(git_output(&repo.0, &["stash", "list"]).is_empty());
    fs::remove_dir_all(remote).unwrap();
}

fn versioned_repository(name: &str) -> (RealRepository, PathBuf) {
    let (repo, remote) = managed_repository(name);
    for (name, bytes) in [
        ("VERSION", "1.0.0\n"),
        ("CHANGELOG.md", "# Changelog\n"),
        ("Cargo.toml", "[package]\nname='sample'\nversion='1.0.0'\n"),
        (
            "Cargo.lock",
            "version=4\n[[package]]\nname='sample'\nversion='1.0.0'\n",
        ),
    ] {
        fs::write(repo.0.join(name), bytes).unwrap();
    }
    git_ok(&repo.0, &["add", "."]);
    git_ok(&repo.0, &["commit", "-m", "chore: native version fixture"]);
    git_ok(&repo.0, &["push"]);
    (repo, remote)
}

#[test]
fn ordinary_sync_preserves_native_versions_across_commits() {
    let (repo, remote) = versioned_repository("native-versions");
    let names = ["VERSION", "Cargo.toml", "Cargo.lock", "CHANGELOG.md"];
    let before = names.map(|name| fs::read(repo.0.join(name)).unwrap());
    for message in ["feat: add behavior", "fix: repair behavior"] {
        fs::write(repo.0.join("tracked.txt"), message).unwrap();
        let outcome = sync(
            &repo.0,
            &SystemProcessRunner,
            GitSyncOptions {
                message: Some(message.into()),
                ..Default::default()
            },
        );
        assert_eq!(outcome.code, 0, "{}", outcome.stderr);
        let receipt = outcome.receipt.unwrap();
        assert_eq!(receipt["release_requested"], false);
        assert_eq!(receipt["version_bumped"], false);
        assert_eq!(receipt["working_tree"], "clean");
        assert_eq!(receipt["ahead"], 0);
        assert_eq!(receipt["behind"], 0);
        for (name, bytes) in names.iter().zip(&before) {
            assert_eq!(&fs::read(repo.0.join(name)).unwrap(), bytes, "{name}");
        }
    }
    assert!(!repo.0.join(".codex/bridgeforge-version.json").exists());
    assert!(!repo.0.join(".codex/package-closures.json").exists());
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn ordinary_sync_blocks_hook_rewrites_and_later_retry() {
    let (repo, remote) = versioned_repository("prepared-hook-rewrite");
    git_ok(&repo.0, &["config", "core.hooksPath", ".git/hooks"]);
    fs::write(repo.0.join("tracked.txt"), b"validated change\n").unwrap();
    let hook = repo.0.join(".git/hooks/pre-commit");
    fs::write(&hook, b"#!/bin/sh\nprintf 'unverified hook change\\n' > tracked.txt\ngit add tracked.txt\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let result = sync(
        &repo.0,
        &SystemProcessRunner,
        GitSyncOptions {
            message: Some("fix: preserve verified content".into()),
            ..Default::default()
        },
    );
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(
        result
            .stderr
            .contains("commit hook changed verified content"),
        "{}",
        result.stderr
    );
    let git = Git {
        root: &repo.0,
        runner: &SystemProcessRunner,
    };
    assert_eq!(git.ahead_behind().unwrap(), (1, 0));
    assert_eq!(
        fs::read(repo.0.join("tracked.txt")).unwrap(),
        b"unverified hook change\n"
    );
    let retry = sync(&repo.0, &SystemProcessRunner, GitSyncOptions::default());
    assert_eq!(retry.code, 2);
    assert!(retry.stderr.contains("manual review required"));
    assert_eq!(git.ahead_behind().unwrap(), (1, 0));
    fs::remove_dir_all(remote).unwrap();
}

impl ProcessRunner for FakeRunner {
    fn run(&self, _: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        Ok(self.outputs.borrow_mut().remove(0))
    }
}

#[test]
fn clean_parity_has_stable_receipt() {
    let runner = FakeRunner {
        outputs: RefCell::new(vec![
            out(0, ".git"),
            out(0, "origin/main"),
            out(0, "origin/main"),
            out(0, ""),
            out(0, "0 0"),
            out(0, "0 0"),
            out(0, ""),
            out(0, "0 0"),
            out(0, "abc"),
        ]),
    };
    let result = sync(
        Path::new("."),
        &runner,
        GitSyncOptions {
            remote: "origin".into(),
            skip_fetch: true,
            ..GitSyncOptions::default()
        },
    );
    assert_eq!(result.code, 0);
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt["status"], "synced");
    assert!(receipt["sync_elapsed_ms"].as_u64().is_some());
}

#[test]
fn push_failure_is_blocking_and_never_claims_synced() {
    let repo = real_repository("push-failure");
    let remote = repo.0.with_file_name(format!(
        "{}-remote.git",
        repo.0.file_name().unwrap().to_string_lossy()
    ));
    git_ok(
        &repo.0,
        &["init", "--bare", remote.to_string_lossy().as_ref()],
    );
    git_ok(
        &repo.0,
        &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
    );
    git_ok(&repo.0, &["push", "-u", "origin", "HEAD"]);
    fs::write(repo.0.join("ahead.txt"), b"ahead\n").unwrap();
    git_ok(&repo.0, &["add", "ahead.txt"]);
    git_ok(&repo.0, &["commit", "-m", "ahead"]);
    fs::write(repo.0.join(".git/hooks/pre-push"), b"#!/bin/sh\nexit 1\n").unwrap();
    let result = sync(
        &repo.0,
        &SystemProcessRunner,
        GitSyncOptions {
            remote: "origin".into(),
            skip_fetch: true,
            ..GitSyncOptions::default()
        },
    );
    assert_eq!(result.code, 2);
    assert!(result.stderr.contains("git push failed"));
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt["status"], "blocked");
    assert!(receipt["timings_ms"]["push"].is_number());
    assert!(receipt.get("push_performed").is_none());
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn clean_ahead_push_identity_drift_never_claims_synced() {
    let repo = real_repository("clean-push-drift");
    let remote = repo.0.with_file_name(format!(
        "{}-remote.git",
        repo.0.file_name().unwrap().to_string_lossy()
    ));
    git_ok(
        &repo.0,
        &["init", "--bare", remote.to_string_lossy().as_ref()],
    );
    git_ok(
        &repo.0,
        &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
    );
    git_ok(&repo.0, &["push", "-u", "origin", "HEAD"]);
    fs::write(repo.0.join("ahead.txt"), b"ahead\n").unwrap();
    git_ok(&repo.0, &["add", "ahead.txt"]);
    git_ok(&repo.0, &["commit", "-m", "ahead"]);
    fs::write(
        repo.0.join(".git/hooks/pre-push"),
        b"#!/bin/sh\ngit config bridgeforge.audit changed\nexit 0\n",
    )
    .unwrap();
    let result = sync(
        &repo.0,
        &SystemProcessRunner,
        GitSyncOptions {
            remote: "origin".into(),
            skip_fetch: true,
            ..GitSyncOptions::default()
        },
    );
    assert_eq!(result.code, 2);
    assert!(result.stderr.contains("after successful push"));
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt["status"], "blocked");
    assert!(receipt["sync_elapsed_ms"].is_number());
    assert!(receipt.get("push_performed").is_none());
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn failed_autostash_pop_returns_a_retained_stash_receipt() {
    let runner = FakeRunner {
        outputs: RefCell::new(vec![
            out(0, ".git"),
            out(0, "origin/main"),
            out(0, "origin/main"),
            out(0, " M local.txt"),
            out(0, "0 1"),
            out(0, "Saved working directory"),
            out(0, "Fast-forward"),
            out(1, "conflict"),
        ]),
    };
    let result = sync(
        Path::new("."),
        &runner,
        GitSyncOptions {
            remote: "origin".into(),
            skip_fetch: true,
            ..GitSyncOptions::default()
        },
    );
    assert_eq!(result.code, 2);
    assert_eq!(
        result.receipt.as_ref().unwrap()["status"],
        "autostash-retained"
    );
    assert_eq!(result.receipt.as_ref().unwrap()["autostash_retained"], true);
}

#[test]
fn split_index_is_blocked_before_transactional_sync() {
    let repo = real_repository("split-index");
    git_ok(&repo.0, &["config", "core.splitIndex", "true"]);
    git_ok(&repo.0, &["update-index", "--split-index"]);
    let git = Git {
        root: &repo.0,
        runner: &SystemProcessRunner,
    };
    let error = RepositoryIdentity::capture(&git).unwrap_err();
    assert!(error.contains("split index is not supported"));
}

#[test]
fn configured_split_index_is_blocked_before_shared_index_exists() {
    let error = verify_split_index_disabled("", "true").unwrap_err();
    assert!(error.contains("core.splitIndex=true"));
}

#[test]
fn repository_identity_batches_facts_without_caching_later_guards() {
    struct Counting(std::cell::Cell<usize>);
    impl ProcessRunner for Counting {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if request.args.first().is_some_and(|arg| arg == "rev-parse") {
                self.0.set(self.0.get() + 1);
            }
            SystemProcessRunner.run(request)
        }
    }
    let repo = real_repository("identity-batch");
    let counter = Counting(std::cell::Cell::new(0));
    let git = Git {
        root: &repo.0,
        runner: &counter,
    };
    let first = RepositoryIdentity::capture(&git).unwrap();
    assert_eq!(counter.0.get(), 2); // Batched facts plus independent split-index query.
    let independent = Git {
        root: &repo.0,
        runner: &SystemProcessRunner,
    };
    for (args, expected) in [
        (
            vec!["rev-parse", "--path-format=absolute", "--git-dir"],
            first.git_dir.as_str(),
        ),
        (
            vec!["rev-parse", "--path-format=absolute", "--git-common-dir"],
            first.common_dir.as_str(),
        ),
        (
            vec!["rev-parse", "--path-format=absolute", "--git-path", "index"],
            first.index_path.to_str().unwrap(),
        ),
        (vec!["rev-parse", "HEAD"], first.head_oid.as_str()),
    ] {
        assert_eq!(
            independent
                .required(&args, Duration::from_secs(30))
                .unwrap(),
            expected
        );
    }
    fs::write(repo.0.join("tracked.txt"), b"next\n").unwrap();
    git_ok(&repo.0, &["add", "."]);
    git_ok(&repo.0, &["commit", "-m", "next identity"]);
    let second = RepositoryIdentity::capture(&git).unwrap();
    assert_eq!(counter.0.get(), 4);
    assert_ne!(first.head_oid, second.head_oid);
    assert!(first.same_layout_and_config(&second));
}

#[test]
fn repository_identity_rejects_malformed_batch_instead_of_using_partial_facts() {
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "git-sync");
    let directory = root.to_string_lossy();
    for body in [
        "".to_owned(),
        format!("{directory}\n{directory}\n{directory}\nnot-an-oid"),
        format!("relative\n{directory}\n{directory}\n{}", "a".repeat(40)),
        format!(
            "{directory}\n{directory}\n{directory}\n{}\nextra",
            "a".repeat(40)
        ),
    ] {
        let runner = FakeRunner {
            outputs: RefCell::new(vec![out(0, &body)]),
        };
        let git = Git {
            root: &root,
            runner: &runner,
        };
        assert!(
            RepositoryIdentity::capture(&git)
                .unwrap_err()
                .contains("unexpected repository identity")
        );
    }
}

#[test]
fn linked_worktree_uses_its_own_index_and_common_identity() {
    let repo = real_repository("linked-worktree");
    let linked = repo.0.join("linked");
    git_ok(
        &repo.0,
        &[
            "worktree",
            "add",
            "-b",
            "linked-test",
            linked.to_string_lossy().as_ref(),
        ],
    );
    let git = Git {
        root: &linked,
        runner: &SystemProcessRunner,
    };
    let identity = RepositoryIdentity::capture(&git).unwrap();
    assert_ne!(identity.git_dir, identity.common_dir);
    assert!(identity.index_path.is_file());
    assert!(identity.index_path.to_string_lossy().contains("worktrees"));
}

#[test]
fn repository_config_drift_blocks_automatic_restore() {
    let repo = real_repository("config-drift");
    let git = Git {
        root: &repo.0,
        runner: &SystemProcessRunner,
    };
    let identity = RepositoryIdentity::capture(&git).unwrap();
    let original_index = fs::read(&identity.index_path).unwrap();
    let marker = repo.0.join("automatic.txt");
    fs::write(&marker, b"new\n").unwrap();
    git_ok(&repo.0, &["config", "bridgeforge.audit", "changed"]);
    let error = restore_snapshots_guarded(
        &git,
        &identity,
        &original_index,
        original_index.clone(),
        vec![FileSnapshot {
            binary: false,
            path: marker.clone(),
            before: Some(b"old\n".to_vec()),
            planned: vec![Some(b"new\n".to_vec())],
        }],
    )
    .unwrap_err();
    assert!(error.contains("HIGH: repository identity drift"));
    assert_eq!(fs::read(marker).unwrap(), b"new\n");
}

#[test]
fn concurrent_index_change_blocks_automatic_restore() {
    let repo = real_repository("index-drift");
    let git = Git {
        root: &repo.0,
        runner: &SystemProcessRunner,
    };
    let identity = RepositoryIdentity::capture(&git).unwrap();
    let original_index = fs::read(&identity.index_path).unwrap();
    let marker = repo.0.join("automatic.txt");
    fs::write(&marker, b"new\n").unwrap();
    fs::write(repo.0.join("foreign.txt"), b"concurrent change\n").unwrap();
    git_ok(&repo.0, &["add", "foreign.txt"]);
    let error = restore_snapshots_guarded(
        &git,
        &identity,
        &original_index,
        original_index.clone(),
        vec![FileSnapshot {
            binary: false,
            path: marker.clone(),
            before: Some(b"old\n".to_vec()),
            planned: vec![Some(b"new\n".to_vec())],
        }],
    )
    .unwrap_err();
    assert!(error.contains("HIGH: Git index changed concurrently"));
    assert_eq!(fs::read(marker).unwrap(), b"new\n");
}

#[test]
fn concurrent_automatic_target_change_blocks_restore() {
    let repo = real_repository("automatic-target-drift");
    let git = Git {
        root: &repo.0,
        runner: &SystemProcessRunner,
    };
    let identity = RepositoryIdentity::capture(&git).unwrap();
    let original_index = fs::read(&identity.index_path).unwrap();
    let marker = repo.0.join("automatic.txt");
    fs::write(&marker, b"foreign\n").unwrap();
    let error = restore_snapshots_guarded(
        &git,
        &identity,
        &original_index,
        original_index.clone(),
        vec![FileSnapshot {
            binary: false,
            path: marker.clone(),
            before: Some(b"old\n".to_vec()),
            planned: vec![Some(b"planned\n".to_vec())],
        }],
    )
    .unwrap_err();
    assert!(error.contains("HIGH: automatic target changed concurrently"));
    assert_eq!(fs::read(marker).unwrap(), b"foreign\n");
}

#[test]
fn concurrent_head_change_blocks_restore() {
    let repo = real_repository("head-drift");
    let git = Git {
        root: &repo.0,
        runner: &SystemProcessRunner,
    };
    let identity = RepositoryIdentity::capture(&git).unwrap();
    let original_index = fs::read(&identity.index_path).unwrap();
    fs::write(repo.0.join("concurrent.txt"), b"commit\n").unwrap();
    git_ok(&repo.0, &["add", "concurrent.txt"]);
    git_ok(&repo.0, &["commit", "-m", "concurrent"]);
    let current_index = fs::read(&identity.index_path).unwrap();
    let error =
        restore_snapshots_guarded(&git, &identity, &current_index, original_index, Vec::new())
            .unwrap_err();
    assert!(error.contains("HIGH: repository identity drift"));
}

#[test]
fn rejected_commit_hook_restores_index_but_preserves_user_worktree() {
    let (repo, remote) = managed_repository("rejected-hook");
    fs::write(repo.0.join(".git/hooks/pre-commit"), b"#!/bin/sh\nexit 1\n").unwrap();
    fs::write(repo.0.join("managed.txt"), b"new\n").unwrap();
    let adaptation = repo
        .0
        .join(".runtime/bridgeforge-codex/explicit-adaptation.json");
    fs::create_dir_all(adaptation.parent().unwrap()).unwrap();
    fs::write(&adaptation, b"legacy receipt\n").unwrap();
    git_ok(&repo.0, &["status", "--porcelain=v1"]);
    let original_index = fs::read(repo.0.join(".git/index")).unwrap();
    let outcome = sync(
        &repo.0,
        &SystemProcessRunner,
        GitSyncOptions {
            remote: "origin".into(),
            skip_fetch: true,
            skip_push: true,
            message: Some("chore: verify rejected hook recovery".into()),
            message_file: None,
            release: false,
            release_request: Default::default(),
        },
    );
    assert_eq!(outcome.code, 2, "{}", outcome.stderr);
    assert!(
        outcome.stderr.contains("Git index were rolled back"),
        "{}",
        outcome.stderr
    );
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), original_index);
    assert_eq!(fs::read(repo.0.join("managed.txt")).unwrap(), b"new\n");
    let staged = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(&repo.0)
        .status()
        .unwrap();
    assert!(staged.success());
    assert!(adaptation.is_file());
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn dirty_commit_push_identity_drift_never_claims_synced() {
    let (repo, remote) = managed_repository("dirty-push-drift");
    let adaptation = repo
        .0
        .join(".runtime/bridgeforge-codex/explicit-adaptation.json");
    fs::create_dir_all(adaptation.parent().unwrap()).unwrap();
    fs::write(&adaptation, b"legacy receipt\n").unwrap();
    fs::write(repo.0.join("managed.txt"), b"new\n").unwrap();
    fs::write(
        repo.0.join(".git/hooks/pre-push"),
        b"#!/bin/sh\ngit config bridgeforge.audit changed\nexit 0\n",
    )
    .unwrap();
    let outcome = sync(
        &repo.0,
        &SystemProcessRunner,
        GitSyncOptions {
            remote: "origin".into(),
            skip_fetch: true,
            skip_push: false,
            message: Some("chore: update managed skeleton".into()),
            message_file: None,
            release: false,
            release_request: Default::default(),
        },
    );
    assert_eq!(outcome.code, 2, "{}", outcome.stderr);
    assert!(outcome.stderr.contains("after successful push"));
    let receipt = outcome.receipt.unwrap();
    assert_eq!(receipt["status"], "blocked");
    assert!(receipt["sync_elapsed_ms"].is_number());
    assert!(receipt.get("push_performed").is_none());
    assert!(!adaptation.exists());
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn preflight_clean_and_dirty_context_never_writes_or_fetches() {
    struct ReadQueries;
    impl ProcessRunner for ReadQueries {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            assert_eq!(request.program, "git");
            assert_eq!(
                request.env.get(std::ffi::OsStr::new("GIT_OPTIONAL_LOCKS")),
                Some(&OsString::from("0"))
            );
            assert_eq!(
                &request.args[..2],
                &[OsString::from("-c"), OsString::from("core.fsmonitor=false")]
            );
            assert_eq!(
                &request.args[2..4],
                &[
                    OsString::from("-c"),
                    OsString::from("diff.autoRefreshIndex=false")
                ]
            );
            assert!(matches!(
                request.args.get(4).and_then(|v| v.to_str()),
                Some(
                    "config"
                        | "ls-files"
                        | "check-attr"
                        | "symbolic-ref"
                        | "rev-parse"
                        | "status"
                        | "diff"
                )
            ));
            SystemProcessRunner.run(request)
        }
    }
    for dirty in [false, true] {
        let (repo, remote) = versioned_repository("preflight-read-only");
        let config = repo.0.join(".git/config");
        git_ok(&repo.0, &["add", "."]);
        git_ok(
            &repo.0,
            &["commit", "--allow-empty", "-m", "chore: confirm policy"],
        );
        git_ok(&repo.0, &["push"]);
        if dirty {
            fs::write(repo.0.join("tracked.txt"), b"staged\n").unwrap();
            git_ok(&repo.0, &["add", "tracked.txt"]);
            fs::write(repo.0.join("tracked.txt"), b"unstaged\n").unwrap();
            fs::write(repo.0.join("untracked.txt"), b"new\n").unwrap();
        }
        fs::write(
            repo.0.join(".git/hooks/preflight-monitor"),
            b"#!/bin/sh\nprintf invoked > monitor-marker\nprintf 'token\\0'\n",
        )
        .unwrap();
        git_ok(
            &repo.0,
            &["config", "core.fsmonitor", ".git/hooks/preflight-monitor"],
        );
        git_ok(&repo.0, &["status", "--porcelain=v1"]);
        assert!(
            repo.0.join("monitor-marker").exists(),
            "fixture must execute the configured monitor without the read-only override"
        );
        fs::remove_file(repo.0.join("monitor-marker")).unwrap();
        // A byte-identical rewrite plus another dirty file triggers Git's
        // optional diff index refresh unless the explicit read-only flag is set.
        fs::write(repo.0.join("managed.txt"), b"old\n").unwrap();
        let paths = [repo.0.join(".git/index"), repo.0.join(".git/HEAD"), config];
        let before = paths
            .iter()
            .map(|p| {
                (
                    fs::read(p).unwrap(),
                    fs::metadata(p).unwrap().modified().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let result = preflight(&repo.0, &ReadQueries);
        assert_eq!(result.code, 0, "{}", result.stderr);
        let receipt = result.receipt.unwrap();
        assert_eq!(receipt["status"], "ready");
        assert_eq!(
            receipt["working_tree"],
            if dirty { "dirty" } else { "clean" }
        );
        assert_eq!(receipt["diff_review_required"], dirty);
        assert_eq!(receipt["remote_state"], "not_fetched");
        assert_eq!(receipt["upstream"], receipt["push_target"]);
        if dirty {
            assert!(
                receipt["status_porcelain"]
                    .as_str()
                    .unwrap()
                    .contains("untracked.txt")
            );
        }
        for (path, (bytes, time)) in paths.iter().zip(before) {
            assert_eq!(fs::read(path).unwrap(), bytes);
            assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), time);
        }
        assert!(!repo.0.join(".runtime").exists());
        assert!(!repo.0.join(".git/bridgeforge-git-sync.lock").exists());
        assert!(
            !repo.0.join("monitor-marker").exists(),
            "preflight must not execute the configured monitor"
        );
        fs::remove_dir_all(remote).unwrap();
    }
}

#[test]
fn preflight_unconfirmed_and_invalid_configuration_remain_zero_write() {
    let repo = real_repository("preflight-choice");
    let index = fs::read(repo.0.join(".git/index")).unwrap();
    let first = preflight(&repo.0, &SystemProcessRunner);
    assert_eq!(first.code, crate::EXIT_BLOCKED);
    let receipt = first.receipt.unwrap();
    assert_eq!(receipt["status"], "blocked");
    assert!(
        !first.stderr.contains("configuration-required"),
        "Git-only preflight must reach repository checks without requiring release setup"
    );
    assert!(!repo.0.join(".codex/bridgeforge-version.json").exists());
    fs::create_dir_all(repo.0.join(".codex")).unwrap();
    let config = repo.0.join(".codex/bridgeforge-version.json");
    fs::write(&config, b"{invalid").unwrap();
    let second = preflight(&repo.0, &SystemProcessRunner);
    assert_eq!(second.code, crate::EXIT_BLOCKED);
    assert_eq!(second.receipt.unwrap()["status"], "blocked");
    assert_eq!(fs::read(&config).unwrap(), b"{invalid");
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), index);
    assert!(!repo.0.join(".runtime").exists());
}

#[test]
fn preflight_blocks_clean_filters_before_running_project_helpers() {
    let (repo, remote) = versioned_repository("preflight-clean-filter");
    fs::write(
        repo.0.join(".git/hooks/preflight-clean"),
        b"#!/bin/sh\nprintf invoked > clean-marker\ncat\n",
    )
    .unwrap();
    fs::write(
        repo.0.join(".gitattributes"),
        b"tracked.txt filter=fixture\n",
    )
    .unwrap();
    git_ok(
        &repo.0,
        &[
            "config",
            "filter.fixture.clean",
            ".git/hooks/preflight-clean",
        ],
    );
    fs::write(repo.0.join("tracked.txt"), b"modified by fixture\n").unwrap();
    git_ok(&repo.0, &["status", "--porcelain=v1"]);
    git_ok(
        &repo.0,
        &["diff", "--no-ext-diff", "--no-textconv", "--stat"],
    );
    assert!(
        repo.0.join("clean-marker").exists(),
        "fixture must demonstrate ordinary Git inspection can execute the clean helper"
    );
    fs::remove_file(repo.0.join("clean-marker")).unwrap();
    let before = fs::read(repo.0.join(".git/index")).unwrap();
    let result = preflight(&repo.0, &SystemProcessRunner);
    assert_eq!(result.code, crate::EXIT_BLOCKED);
    assert!(
        result.receipt.unwrap()["reason"]
            .as_str()
            .unwrap()
            .contains("clean/process")
    );
    assert!(!repo.0.join("clean-marker").exists());
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), before);
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn preflight_rejects_truncated_attributes_and_submodule_helper_scope() {
    struct MissingAttributes;
    impl ProcessRunner for MissingAttributes {
        fn run(&self, r: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if r.args.iter().any(|v| v == "check-attr") {
                return Ok(out(0, ""));
            }
            SystemProcessRunner.run(r)
        }
    }
    let (repo, remote) = versioned_repository("preflight-attribute-guard");
    git_ok(&repo.0, &["config", "filter.fixture.clean", "cat"]);
    let result = preflight(&repo.0, &MissingAttributes);
    assert_eq!(result.code, crate::EXIT_BLOCKED);
    assert!(
        result.receipt.unwrap()["reason"]
            .as_str()
            .unwrap()
            .contains("attribute framing")
    );
    let git = Git {
        root: &repo.0,
        runner: &SystemProcessRunner,
    };
    let head = git
        .required(&["rev-parse", "HEAD"], Duration::from_secs(30))
        .unwrap();
    git_ok(
        &repo.0,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000",
            &head,
            "nested",
        ],
    );
    let before = fs::read(repo.0.join(".git/index")).unwrap();
    let result = preflight(&repo.0, &SystemProcessRunner);
    assert_eq!(result.code, crate::EXIT_BLOCKED);
    assert!(
        result.receipt.unwrap()["reason"]
            .as_str()
            .unwrap()
            .contains("submodule")
    );
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), before);
    fs::remove_dir_all(remote).unwrap();
}

#[test]
fn preflight_missing_push_target_blocks_without_fabricating_parity() {
    let (repo, temporary_remote) = versioned_repository("preflight-no-remote");
    git_ok(&repo.0, &["remote", "remove", "origin"]);
    fs::remove_dir_all(temporary_remote).unwrap();
    fs::create_dir_all(repo.0.join(".codex")).unwrap();
    let result = preflight(&repo.0, &SystemProcessRunner);
    assert_eq!(result.code, crate::EXIT_BLOCKED);
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt["status"], "blocked");
    assert!(receipt["reason"].as_str().unwrap().contains("rev-parse"));
    assert!(receipt.get("ahead").is_none());
    assert!(receipt.get("working_tree").is_none());
}

#[test]
fn preflight_distinguishes_push_target_and_blocks_detached_head() {
    let (repo, remote) = versioned_repository("preflight-push-target");
    git_ok(&repo.0, &["add", "."]);
    git_ok(
        &repo.0,
        &["commit", "--allow-empty", "-m", "chore: confirm policy"],
    );
    git_ok(&repo.0, &["push"]);
    let fork = repo.0.with_extension("fork.git");
    git_ok(
        &repo.0,
        &[
            "clone",
            "--bare",
            remote.to_str().unwrap(),
            fork.to_str().unwrap(),
        ],
    );
    git_ok(&repo.0, &["remote", "add", "fork", fork.to_str().unwrap()]);
    git_ok(&repo.0, &["config", "remote.pushDefault", "fork"]);
    git_ok(&repo.0, &["config", "push.default", "current"]);
    git_ok(&repo.0, &["fetch", "fork"]);
    let result = preflight(&repo.0, &SystemProcessRunner);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let receipt = result.receipt.unwrap();
    assert!(receipt["upstream"].as_str().unwrap().starts_with("origin/"));
    assert!(
        receipt["push_target"]
            .as_str()
            .unwrap()
            .starts_with("fork/")
    );
    git_ok(&repo.0, &["checkout", "--detach"]);
    let index = fs::read(repo.0.join(".git/index")).unwrap();
    let result = preflight(&repo.0, &SystemProcessRunner);
    assert_eq!(result.code, crate::EXIT_BLOCKED);
    assert!(
        result.receipt.unwrap()["reason"]
            .as_str()
            .unwrap()
            .contains("symbolic-ref")
    );
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), index);
    fs::remove_dir_all(remote).unwrap();
    fs::remove_dir_all(fork).unwrap();
}

#[test]
fn phase_timing_observer_preserves_artifact_cache_runner_contract() {
    struct CacheAware {
        hits: std::cell::Cell<usize>,
        misses: RefCell<Vec<&'static str>>,
    }
    impl ProcessRunner for CacheAware {
        fn run(&self, _: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            Ok(out(0, "ok"))
        }
        fn artifact_cache_enabled(&self) -> bool {
            true
        }
        fn cache_hit(&self) {
            self.hits.set(self.hits.get() + 1)
        }
        fn cache_miss(&self, reason: &'static str) {
            self.misses.borrow_mut().push(reason)
        }
    }
    let runner = CacheAware {
        hits: Default::default(),
        misses: Default::default(),
    };
    let timings = timing::Timings::new();
    let observed = timing::Observed {
        runner: &runner,
        timings: &timings,
    };
    assert!(observed.artifact_cache_enabled());
    observed.cache_hit();
    observed.cache_miss("entry_unavailable_or_invalid");
    assert_eq!(runner.hits.get(), 1);
    assert_eq!(
        *runner.misses.borrow(),
        vec!["entry_unavailable_or_invalid"]
    );
}

#[test]
fn phase_timing_retains_tls_timeout_and_launch_failures_without_retry() {
    struct FailingFetch {
        kind: &'static str,
        fetches: std::cell::Cell<usize>,
    }
    impl ProcessRunner for FailingFetch {
        fn run(&self, r: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if r.program == "git" && r.args.first().is_some_and(|v| v == "fetch") {
                self.fetches.set(self.fetches.get() + 1);
                std::thread::sleep(Duration::from_millis(12));
                if self.kind == "launch" {
                    return Err(std::io::Error::other("fixture launch failure"));
                }
                return Ok(ProcessOutput {
                    code: 128,
                    stdout: Vec::new(),
                    stderr: b"SSL_ERROR_SYSCALL".to_vec(),
                    timed_out: self.kind == "timeout",
                });
            }
            SystemProcessRunner.run(r)
        }
    }
    for kind in ["tls", "timeout", "launch"] {
        let (repo, remote) = managed_repository("timed-fetch-error");
        let before = fs::read(repo.0.join(".git/index")).unwrap();
        let runner = FailingFetch {
            kind,
            fetches: Default::default(),
        };
        let result = sync(&repo.0, &runner, GitSyncOptions::default());
        assert_eq!(result.code, crate::EXIT_BLOCKED);
        assert_eq!(runner.fetches.get(), 1);
        assert!(result.stderr.contains(match kind {
            "tls" => "SSL_ERROR_SYSCALL",
            "timeout" => "timed out",
            _ => "fixture launch failure",
        }));
        let receipt = result.receipt.unwrap();
        assert_eq!(receipt["status"], "blocked");
        let fetch = receipt["timings_ms"]["fetch"].as_u64().unwrap();
        assert!(fetch >= 10);
        assert!(receipt["sync_elapsed_ms"].as_u64().unwrap() >= fetch);
        assert_eq!(receipt["timings_ms"]["commit_and_hooks"], 0);
        assert_eq!(receipt["timings_ms"]["push"], 0);
        assert!(receipt.get("push_performed").is_none());
        assert!(receipt.get("working_tree").is_none());
        assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), before);
        fs::remove_dir_all(remote).unwrap();
    }
}

#[test]
fn phase_timing_success_preserves_commit_hook_and_remote_receipt() {
    let (repo, remote) = managed_repository("timed-commit");
    fs::write(repo.0.join("tracked.txt"), b"business change\n").unwrap();
    fs::write(repo.0.join(".git/hooks/pre-commit"), b"#!/bin/sh\nexit 0\n").unwrap();
    let result = sync(
        &repo.0,
        &SystemProcessRunner,
        GitSyncOptions {
            message: Some("chore: timing fixture".into()),
            ..Default::default()
        },
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt["status"], "synced");
    assert_eq!(receipt["working_tree"], "clean");
    assert_eq!(receipt["ahead"], 0);
    assert_eq!(receipt["behind"], 0);
    assert_eq!(receipt["push_performed"], true);
    let total = receipt["sync_elapsed_ms"].as_u64().unwrap();
    for value in receipt["timings_ms"].as_object().unwrap().values() {
        assert!(value.as_u64().unwrap() <= total);
    }
    assert!(receipt["timings_ms"]["commit_and_hooks"].as_u64().unwrap() > 0);
    assert!(
        receipt["timing_semantics"]
            .as_str()
            .unwrap()
            .contains("must not be summed")
    );
    fs::remove_dir_all(remote).unwrap();
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
