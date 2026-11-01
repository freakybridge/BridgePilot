use super::*;
use bridgeforge_core::{ProcessRequest, ProcessRunner, SystemProcessRunner};
use bridgepilot_transfer::handoff;
use bridgepilot_install::install;
use bridgepilot_transfer::sync;
use rusqlite::OptionalExtension;
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Transport {
    remote: PathBuf,
    lose_ack: AtomicBool,
    fail_next_read: AtomicBool,
    pushes: AtomicUsize,
    drop_push: AtomicBool,
    public_remote: AtomicBool,
    fail_prepare_after: AtomicUsize,
}
impl Transport {
    fn new(remote: PathBuf) -> Self {
        Self {
            remote,
            lose_ack: AtomicBool::new(false),
            fail_next_read: AtomicBool::new(false),
            pushes: AtomicUsize::new(0),
            drop_push: AtomicBool::new(false),
            public_remote: AtomicBool::new(false),
            fail_prepare_after: AtomicUsize::new(usize::MAX),
        }
    }
}
impl ProcessRunner for Transport {
    fn run(&self, q: &ProcessRequest) -> std::io::Result<bridgeforge_core::ProcessOutput> {
        if q.program == "gh" {
            let left = self.fail_prepare_after.load(Ordering::SeqCst);
            if left != usize::MAX {
                if left == 0 {
                    self.fail_prepare_after.store(usize::MAX, Ordering::SeqCst);
                    return Err(std::io::Error::other("controlled preparation interruption"));
                }
                self.fail_prepare_after.fetch_sub(1, Ordering::SeqCst);
            }
            return Ok(bridgeforge_core::ProcessOutput {
                code: 0,
                stdout: serde_json::to_vec(
                    &json!({"private":!self.public_remote.load(Ordering::SeqCst),"full_name":"test-owner/BridgePilotData"}),
                )
                .unwrap(),
                stderr: Vec::new(),
                timed_out: false,
            });
        }
        let mut request = q.clone();
        for arg in &mut request.args {
            if arg == "https://github.com/test-owner/BridgePilotData.git" {
                *arg = self.remote.as_os_str().into();
            }
        }
        if request.args.iter().any(|a| a == "ls-remote")
            && self.fail_next_read.swap(false, Ordering::SeqCst)
        {
            return Err(std::io::Error::other("controlled response loss"));
        }
        let push = request.args.iter().any(|a| a == "push");
        if push && self.drop_push.swap(false, Ordering::SeqCst) {
            return Err(std::io::Error::other(
                "controlled push did not reach remote",
            ));
        }
        let result = SystemProcessRunner.run(&request)?;
        if push {
            self.pushes.fetch_add(1, Ordering::SeqCst);
            if self.lose_ack.swap(false, Ordering::SeqCst) {
                self.fail_next_read.store(true, Ordering::SeqCst);
                return Err(std::io::Error::other(
                    "controlled lost push acknowledgement",
                ));
            }
        }
        Ok(result)
    }
}
fn git(root: &Path, args: &[&str]) -> String {
    let mut q = ProcessRequest::new("git", root);
    q.args = args.iter().map(Into::into).collect();
    let result = SystemProcessRunner.run(&q).unwrap();
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim_end().into()
}
fn bare(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    git(root, &["init", "--bare", path.to_str().unwrap()]);
    path
}
fn configured(root: &Path) -> Store {
    let store = Store::init(root).unwrap();
    store.machine().unwrap();
    sync::configure(
        &store,
        sync::Config {
            repository: "test-owner/BridgePilotData".into(),
            branch: "main".into(),
            source: "test:explicit private transport".into(),
        },
    )
    .unwrap();
    store
}

fn local_record(store: &Store, key: &str) -> Option<Vec<u8>> {
    store
        .read_connection()
        .unwrap()
        .query_row("SELECT value FROM local_records WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()
        .unwrap()
}

fn transport_pending(store: &Store) -> sync::Pending {
    serde_json::from_slice(&local_record(store, "transport.pending").unwrap()).unwrap()
}

fn transport_receipt(store: &Store) -> serde_json::Value {
    serde_json::from_slice(&local_record(store, "transport.last_receipt").unwrap()).unwrap()
}

fn remove_transport_cache(f: &Fixture) {
    let cache = f.root.join(".runtime/bridgepilot/transport");
    let resolved = fs::canonicalize(&cache).unwrap();
    assert!(resolved.starts_with(fs::canonicalize(&f.root).unwrap()));
    fs::remove_dir_all(&cache).unwrap();
    assert!(!cache.exists());
}

#[test]
fn private_sync_roundtrip_rejects_an_archive_fork_and_excludes_local_material() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    sync::configure(
        &f.store,
        sync::Config {
            repository: "test-owner/BridgePilotData".into(),
            branch: "main".into(),
            source: "test:approved".into(),
        },
    )
    .unwrap();
    let snapshot = f.store.read().unwrap();
    handoff::enrol(&f.store, snapshot.revision, "test:local migration").unwrap();
    let remote = bare(&f.root, "archive.git");
    let transport = Transport::new(remote.clone());
    let engine = sync::Engine::new(&f.store, &transport).unwrap();
    let uploaded = engine.upload().unwrap();
    assert_eq!(uploaded["snapshot_sha256"].as_str().unwrap().len(), 64);
    fs::write(
        f.root.join(".runtime/config/do-not-upload.txt"),
        "private local fixture",
    )
    .unwrap();
    let files = git(
        &f.root,
        &[
            "--git-dir",
            remote.to_str().unwrap(),
            "ls-tree",
            "-r",
            "--name-only",
            "refs/heads/main",
        ],
    );
    assert!(!files.contains(".runtime"));
    assert!(!files.contains("project/"));
    assert_eq!(
        files,
        "doc/5_project_knowledgebase/bridgepilot/bridgepilot.sqlite3"
    );
    let target = configured(&f.root.join("machine-b"));
    let target_engine = sync::Engine::new(&target, &transport).unwrap();
    let downloaded = target_engine.download().unwrap();
    assert_eq!(uploaded["snapshot_sha256"], downloaded["snapshot_sha256"]);
    assert_eq!(
        f.store.read().unwrap().archive_id,
        target.read().unwrap().archive_id
    );
    assert!(
        target
            .update(|s| {
                s.projects.get_mut("p").unwrap().name = "unauthorized".into();
                Ok(())
            })
            .is_err()
    );
    // A controlled stale clone of the SAME machine demonstrates application
    // forks which Git fast-forward rules alone cannot detect.
    fs::copy(
        f.root.join(".runtime/config/bridgepilot-machine.json"),
        target
            .workspace
            .join(".runtime/config/bridgepilot-machine.json"),
    )
    .unwrap();
    f.store
        .update(|s| {
            s.projects.get_mut("p").unwrap().name = "first fork".into();
            Ok(())
        })
        .unwrap();
    target
        .update(|s| {
            s.projects.get_mut("p").unwrap().name = "second fork".into();
            Ok(())
        })
        .unwrap();
    engine.upload().unwrap();
    assert!(target_engine.upload().is_err());
}

fn checkpoint_fixture(f: &Fixture) -> (PathBuf, handoff::Checkpoint) {
    let root = f.store.read().unwrap().state.runs[&f.run]
        .binding
        .root
        .clone();
    git(&root, &["init", "--initial-branch", "main"]);
    git(&root, &["config", "user.name", "BP fixture"]);
    git(&root, &["config", "user.email", "bp-fixture@localhost"]);
    git(&root, &["add", "AGENTS.md"]);
    git(&root, &["commit", "-m", "controlled checkpoint"]);
    // Fixture-only control markers are ignored rather than checkpointed.
    fs::write(root.join(".gitignore"), ".bp-fixture-*\n").unwrap();
    git(&root, &["add", ".gitignore"]);
    git(&root, &["commit", "-m", "controlled ignores"]);
    let remote = bare(&f.root, "code.git");
    git(
        &root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&root, &["push", "-u", "origin", "main"]);
    f.store
        .update(|s| {
            s.projects.get_mut("p").unwrap().repository = remote.to_str().unwrap().into();
            s.pause(true, &[])?;
            Ok(())
        })
        .unwrap();
    let snapshot = f.store.read().unwrap();
    let checkpoint = handoff::probe_checkpoint(
        &SystemProcessRunner,
        &root,
        &snapshot.state.projects["p"],
        "origin",
        "refs/heads/main",
        "test:actual fixed commit",
    )
    .unwrap();
    (remote, checkpoint)
}

#[test]
fn handoff_accept_is_remote_first_and_recovers_the_same_commit_after_response_loss() {
    let f = Fixture::new("complete");
    let (code, checkpoint) = checkpoint_fixture(&f);
    sync::configure(
        &f.store,
        sync::Config {
            repository: "test-owner/BridgePilotData".into(),
            branch: "main".into(),
            source: "test:approved".into(),
        },
    )
    .unwrap();
    handoff::enrol(
        &f.store,
        f.store.read().unwrap().revision,
        "test:source owner",
    )
    .unwrap();
    let remote = bare(&f.root, "archive.git");
    let transport = Transport::new(remote);
    let old = sync::Engine::new(&f.store, &transport).unwrap();
    old.upload().unwrap();
    handoff::release(
        &old,
        handoff::ReleaseRequest {
            checkpoints: vec![checkpoint],
            source: "test:explicit full handoff".into(),
        },
    )
    .unwrap();
    assert!(
        f.store
            .update(|s| {
                s.projects.get_mut("p").unwrap().name = "old write".into();
                Ok(())
            })
            .is_err()
    );
    let new = configured(&f.root.join("machine-b"));
    let new_engine = sync::Engine::new(&new, &transport).unwrap();
    new_engine.download().unwrap();
    let copy = f.root.join("code-b");
    git(
        &f.root,
        &[
            "clone",
            "--branch",
            "main",
            code.to_str().unwrap(),
            copy.to_str().unwrap(),
        ],
    );
    let machine = new.machine().unwrap();
    let snapshot = new.read().unwrap();
    let ticket = snapshot
        .state
        .custody
        .as_ref()
        .unwrap()
        .handoff
        .as_ref()
        .unwrap()
        .id
        .clone();
    let competitor = configured(&f.root.join("machine-c"));
    let competitor_engine = sync::Engine::new(&competitor, &transport).unwrap();
    competitor_engine.download().unwrap();
    transport.lose_ack.store(true, Ordering::SeqCst);
    assert!(
        handoff::accept(
            &new_engine,
            handoff::AcceptRequest {
                transfer: ticket.clone(),
                bindings: vec![Binding {
                    project: "p".into(),
                    machine: machine.clone(),
                    root: fs::canonicalize(copy).unwrap(),
                    desktop_project_id: "new-project-p".into(),
                    host: "local".into()
                }],
                source: "test:accept new machine".into()
            }
        )
        .is_err()
    );
    assert_eq!(
        new.read().unwrap().state.custody.as_ref().unwrap().owner,
        None
    );
    let before = transport.pushes.load(Ordering::SeqCst);
    new_engine.reconcile().unwrap();
    assert_eq!(before, transport.pushes.load(Ordering::SeqCst));
    assert_eq!(
        new.read()
            .unwrap()
            .state
            .custody
            .as_ref()
            .unwrap()
            .owner
            .as_deref(),
        Some(machine.as_str())
    );
    assert!(
        handoff::accept(
            &competitor_engine,
            handoff::AcceptRequest {
                transfer: ticket,
                bindings: vec![Binding {
                    project: "p".into(),
                    machine: competitor.machine().unwrap(),
                    root: f.root.join("code-b"),
                    desktop_project_id: "competitor-p".into(),
                    host: "local".into()
                }],
                source: "test:competing acceptance".into()
            }
        )
        .is_err()
    );
    assert_eq!(
        competitor
            .read()
            .unwrap()
            .state
            .custody
            .as_ref()
            .unwrap()
            .owner,
        None
    );
    new.update(|s| s.resume_projects(&["p".into()])).unwrap();
    let original = new.read().unwrap().state.runs[&f.run].machine.clone();
    handoff::continue_tasks(
        &new,
        new.read().unwrap().revision,
        handoff::ContinueRequest {
            tasks: vec![f.task.clone()],
            source: "test:explicit continuation".into(),
        },
    )
    .unwrap();
    let current = new.read().unwrap();
    assert_eq!(current.state.runs[&f.run].machine, original);
    assert_eq!(current.state.tasks[&f.task].state, TaskState::Ready);
    let (_, fresh) = new
        .transact(current.revision, |s| s.prepare_run(&f.task, &machine))
        .unwrap();
    let snapshot = new.read().unwrap();
    assert_eq!(snapshot.state.runs[&fresh].thread_id, None);
}

#[test]
fn cancelled_release_keeps_recovery_record_until_fence_is_prepared() {
    let f = Fixture::new("complete");
    let (_, checkpoint) = checkpoint_fixture(&f);
    sync::configure(
        &f.store,
        sync::Config {
            repository: "test-owner/BridgePilotData".into(),
            branch: "main".into(),
            source: "test:approved".into(),
        },
    )
    .unwrap();
    handoff::enrol(&f.store, f.store.read().unwrap().revision, "test:owner").unwrap();
    let remote = bare(&f.root, "archive.git");
    let transport = Transport::new(remote.clone());
    let engine = sync::Engine::new(&f.store, &transport).unwrap();
    engine.upload().unwrap();
    transport.drop_push.store(true, Ordering::SeqCst);
    assert!(
        handoff::release(
            &engine,
            handoff::ReleaseRequest {
                checkpoints: vec![checkpoint],
                source: "test:release".into()
            }
        )
        .is_err()
    );
    let before = local_record(&f.store, "transport.pending").unwrap();
    transport.fail_prepare_after.store(1, Ordering::SeqCst);
    assert!(engine.cancel_pending_release("test:cancel").is_err());
    assert_eq!(before, local_record(&f.store, "transport.pending").unwrap());
    assert!(
        f.store
            .update(|s| s.resume_projects(&["p".into()]))
            .is_err()
    );
    engine
        .cancel_pending_release("test:cancel after connectivity restored")
        .unwrap();
    assert!(local_record(&f.store, "transport.pending").is_none());
    assert!(
        !f.root
            .join(".runtime/bridgepilot/transport/pending.json")
            .exists()
    );
    let cancelled = git(
        &f.root,
        &[
            "--git-dir",
            remote.to_str().unwrap(),
            "rev-parse",
            "refs/heads/main",
        ],
    );
    let previous: sync::Pending = serde_json::from_slice(&before).unwrap();
    let mut delayed = ProcessRequest::new("git", &f.root);
    delayed.args = vec![
        "--git-dir".into(),
        f.root
            .join(".runtime/bridgepilot/transport/repository.git")
            .into_os_string(),
        "push".into(),
        remote.into_os_string(),
        format!("{}:refs/heads/main", previous.candidate).into(),
    ];
    assert_ne!(SystemProcessRunner.run(&delayed).unwrap().code, 0);
    assert!(!cancelled.is_empty());
    f.store
        .update(|s| s.resume_projects(&["p".into()]))
        .unwrap();
}

#[test]
fn private_transport_refuses_public_remote_without_any_upload() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    sync::configure(
        &f.store,
        sync::Config {
            repository: "test-owner/BridgePilotData".into(),
            branch: "main".into(),
            source: "test:approved".into(),
        },
    )
    .unwrap();
    handoff::enrol(&f.store, f.store.read().unwrap().revision, "test:owner").unwrap();
    let transport = Transport::new(bare(&f.root, "archive.git"));
    transport.public_remote.store(true, Ordering::SeqCst);
    assert!(
        sync::Engine::new(&f.store, &transport)
            .unwrap()
            .upload()
            .is_err()
    );
    assert_eq!(transport.pushes.load(Ordering::SeqCst), 0);
}

#[test]
fn initial_sync_with_no_remote_ack_can_be_fenced_without_replaying_original_push() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    sync::configure(
        &f.store,
        sync::Config {
            repository: "test-owner/BridgePilotData".into(),
            branch: "main".into(),
            source: "test:approved".into(),
        },
    )
    .unwrap();
    handoff::enrol(&f.store, f.store.read().unwrap().revision, "test:owner").unwrap();
    let transport = Transport::new(bare(&f.root, "archive.git"));
    let engine = sync::Engine::new(&f.store, &transport).unwrap();
    transport.drop_push.store(true, Ordering::SeqCst);
    assert!(engine.upload().is_err());
    assert!(engine.reconcile().is_err());
    let before = transport_pending(&f.store);
    engine
        .fence_pending("test:explicitly resolve unpublished initial sync")
        .unwrap();
    let receipt = transport_receipt(&f.store);
    assert_ne!(receipt["operation"]["candidate"], before.candidate);
    assert_eq!(
        receipt["operation"]["superseded"][0]["commit"],
        before.candidate
    );
    assert_eq!(transport.pushes.load(Ordering::SeqCst), 1);
    engine.upload().unwrap();
}

#[test]
fn lost_acknowledgement_is_query_only_reconciled_after_runtime_cache_cleanup() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    sync::configure(
        &f.store,
        sync::Config {
            repository: "test-owner/BridgePilotData".into(),
            branch: "main".into(),
            source: "test:authorized SQLite recovery".into(),
        },
    )
    .unwrap();
    handoff::enrol(&f.store, f.store.read().unwrap().revision, "test:owner").unwrap();
    let transport = Transport::new(bare(&f.root, "archive.git"));
    let engine = sync::Engine::new(&f.store, &transport).unwrap();
    transport.lose_ack.store(true, Ordering::SeqCst);
    assert!(engine.upload().is_err());
    let pending = transport_pending(&f.store);
    let before = local_record(&f.store, "transport.pending").unwrap();
    let revision = f.store.read().unwrap().revision;
    let pushes = transport.pushes.load(Ordering::SeqCst);
    assert_eq!(pushes, 1);
    remove_transport_cache(&f);
    assert_eq!(local_record(&f.store, "transport.pending").unwrap(), before);
    assert!(
        f.store
            .update(|s| {
                s.projects.get_mut("p").unwrap().name = "forbidden pending write".into();
                Ok(())
            })
            .is_err()
    );
    engine.reconcile().unwrap();
    assert_eq!(transport.pushes.load(Ordering::SeqCst), pushes);
    assert!(local_record(&f.store, "transport.pending").is_none());
    assert_eq!(
        transport_receipt(&f.store)["operation"]["candidate"],
        pending.candidate
    );
    assert_eq!(f.store.read().unwrap().revision, revision);
    assert!(
        !f.root
            .join(".runtime/bridgepilot/transport/pending.json")
            .exists()
    );
    assert!(
        !f.root
            .join(".runtime/bridgepilot/transport/last-receipt.json")
            .exists()
    );
}

#[test]
fn unpublished_candidate_can_be_fenced_after_all_runtime_git_objects_are_removed() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    sync::configure(
        &f.store,
        sync::Config {
            repository: "test-owner/BridgePilotData".into(),
            branch: "main".into(),
            source: "test:authorized SQLite recovery".into(),
        },
    )
    .unwrap();
    handoff::enrol(&f.store, f.store.read().unwrap().revision, "test:owner").unwrap();
    let transport = Transport::new(bare(&f.root, "archive.git"));
    let engine = sync::Engine::new(&f.store, &transport).unwrap();
    transport.drop_push.store(true, Ordering::SeqCst);
    assert!(engine.upload().is_err());
    let pending = transport_pending(&f.store);
    let key = format!("transport.snapshot.{}", pending.candidate);
    let snapshot = local_record(&f.store, &key).unwrap();
    assert!(snapshot.starts_with(b"SQLite format 3\0"));
    bridgepilot_storage::database::inspect_shared_bytes(&snapshot).unwrap();
    remove_transport_cache(&f);
    assert_eq!(local_record(&f.store, &key).unwrap(), snapshot);
    assert!(engine.reconcile().is_err());
    assert_eq!(transport_pending(&f.store), pending);
    engine
        .fence_pending("test:explicitly abandon old candidate after cache cleanup")
        .unwrap();
    assert_eq!(transport.pushes.load(Ordering::SeqCst), 1);
    assert!(local_record(&f.store, "transport.pending").is_none());
    assert!(local_record(&f.store, &key).is_none());
    let receipt = transport_receipt(&f.store);
    assert_ne!(receipt["operation"]["candidate"], pending.candidate);
    assert_eq!(
        receipt["operation"]["superseded"][0]["commit"],
        pending.candidate
    );
    engine.upload().unwrap();
}

#[test]
fn enrolment_does_not_claim_process_exit_but_handoff_requires_it() {
    let f = Fixture::new("complete");
    let (_, checkpoint) = checkpoint_fixture(&f);
    sync::configure(
        &f.store,
        sync::Config {
            repository: "test-owner/BridgePilotData".into(),
            branch: "main".into(),
            source: "test:approved".into(),
        },
    )
    .unwrap();
    f.store
        .update(|s| {
            s.pause(true, &[])?;
            s.runs.get_mut(&f.run).unwrap().owner_pid = Some(std::process::id());
            Ok(())
        })
        .unwrap();
    handoff::enrol(
        &f.store,
        f.store.read().unwrap().revision,
        "test:backups only",
    )
    .unwrap();
    let transport = Transport::new(bare(&f.root, "archive.git"));
    let engine = sync::Engine::new(&f.store, &transport).unwrap();
    assert!(
        handoff::release(
            &engine,
            handoff::ReleaseRequest {
                checkpoints: vec![checkpoint.clone()],
                source: "test:handoff owner still alive".into()
            }
        )
        .is_err()
    );
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_bp"));
    command
        .arg("self-test")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let exited = child.id();
    assert!(child.wait().unwrap().success());
    f.store
        .update(|s| {
            s.runs.get_mut(&f.run).unwrap().owner_pid = Some(exited);
            Ok(())
        })
        .unwrap();
    handoff::release(
        &engine,
        handoff::ReleaseRequest {
            checkpoints: vec![checkpoint],
            source: "test:handoff after actual owner exit".into(),
        },
    )
    .unwrap();
}

#[test]
#[cfg(windows)]
fn legacy_server_pid_reuse_proof_is_scoped_and_preserves_original_run() {
    let f = Fixture::new("complete");
    let (_, checkpoint) = checkpoint_fixture(&f);
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_bp"));
    command
        .arg("self-test")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let owner = child.id();
    assert!(child.wait().unwrap().success());
    let receipt = json!({"schema":1,"status":"reconciled_before_start","run":f.run,"thread_id":"fixture-native-thread","task_turn_was_not_attempted":true});
    f.store
        .update(|s| {
            let r = s.runs.get_mut(&f.run).unwrap();
            r.owner_pid = Some(owner);
            r.server_pid = Some(std::process::id());
            r.result = Some(receipt.clone());
            Ok(())
        })
        .unwrap();
    sync::configure(
        &f.store,
        sync::Config {
            repository: "test-owner/BridgePilotData".into(),
            branch: "main".into(),
            source: "test:approved".into(),
        },
    )
    .unwrap();
    handoff::enrol(
        &f.store,
        f.store.read().unwrap().revision,
        "test:backups only",
    )
    .unwrap();
    let transport = Transport::new(bare(&f.root, "archive.git"));
    let engine = sync::Engine::new(&f.store, &transport).unwrap();
    assert!(
        handoff::release(
            &engine,
            handoff::ReleaseRequest {
                checkpoints: vec![checkpoint.clone()],
                source: "test:unverified reused PID".into()
            }
        )
        .is_err()
    );
    let historical = f.root.join(".runtime/tests/historical-native-receipt.json");
    fs::create_dir_all(historical.parent().unwrap()).unwrap();
    fs::write(&historical, serde_json::to_vec(&receipt).unwrap()).unwrap();
    let launch = f.root.join(".runtime/tests/launch.txt");
    fs::write(
        &launch,
        format!(
            "supervise --workspace {} --codex {}",
            f.store.workspace.display(),
            Fixture::executable().display()
        ),
    )
    .unwrap();
    let supervisor = f.root.join(".runtime/tests/supervisor.jsonl");
    fs::write(
        &supervisor,
        serde_json::to_vec(&json!({"event":"worker_started","run":f.run,"pid":owner})).unwrap(),
    )
    .unwrap();
    let original = f.store.read().unwrap();
    handoff::legacy_server_probe(
        &f.store,
        &f.run,
        Fixture::executable(),
        &historical,
        &launch,
        &supervisor,
        "test:actual controlled source",
    )
    .unwrap();
    assert_eq!(f.store.read().unwrap().revision, original.revision);
    assert_eq!(
        serde_json::to_value(&f.store.read().unwrap().state.runs[&f.run]).unwrap(),
        serde_json::to_value(&original.state.runs[&f.run]).unwrap()
    );
    let saved = fs::read(&launch).unwrap();
    fs::write(&launch, "tampered source").unwrap();
    assert!(
        handoff::release(
            &engine,
            handoff::ReleaseRequest {
                checkpoints: vec![checkpoint.clone()],
                source: "test:tampered proof".into()
            }
        )
        .is_err()
    );
    fs::write(&launch, saved).unwrap();
    handoff::release(
        &engine,
        handoff::ReleaseRequest {
            checkpoints: vec![checkpoint],
            source: "test:verified legacy server exit".into(),
        },
    )
    .unwrap();
}

#[test]
fn checkpoint_rejects_dirty_and_unpublished_project_changes() {
    let f = Fixture::new("complete");
    let (_, checkpoint) = checkpoint_fixture(&f);
    let s = f.store.read().unwrap();
    let root = &s.state.runs[&f.run].binding.root;
    fs::write(root.join("unsaved.txt"), "uncheckpointed").unwrap();
    assert!(
        handoff::probe_checkpoint(
            &SystemProcessRunner,
            root,
            &s.state.projects["p"],
            "origin",
            &checkpoint.reference,
            "test:dirty"
        )
        .is_err()
    );
    git(root, &["add", "unsaved.txt"]);
    git(root, &["commit", "-m", "local only"]);
    assert!(
        handoff::probe_checkpoint(
            &SystemProcessRunner,
            root,
            &s.state.projects["p"],
            "origin",
            &checkpoint.reference,
            "test:unpushed"
        )
        .is_err()
    );
}

#[test]
fn checkpoint_refuses_a_file_edit_during_remote_verification() {
    struct EditingRead {
        root: PathBuf,
        edited: AtomicBool,
    }
    impl ProcessRunner for EditingRead {
        fn run(&self, q: &ProcessRequest) -> std::io::Result<bridgeforge_core::ProcessOutput> {
            let result = SystemProcessRunner.run(q)?;
            if q.args.iter().any(|a| a == "ls-remote") && !self.edited.swap(true, Ordering::SeqCst)
            {
                fs::write(
                    self.root.join("AGENTS.md"),
                    "edited while remote was queried",
                )?;
            }
            Ok(result)
        }
    }
    let f = Fixture::new("complete");
    let (_, checkpoint) = checkpoint_fixture(&f);
    let s = f.store.read().unwrap();
    let root = &s.state.runs[&f.run].binding.root;
    let runner = EditingRead {
        root: root.clone(),
        edited: AtomicBool::new(false),
    };
    assert!(
        handoff::probe_checkpoint(
            &runner,
            root,
            &s.state.projects["p"],
            "origin",
            &checkpoint.reference,
            "test:concurrent edit"
        )
        .is_err()
    );
    assert!(runner.edited.load(Ordering::SeqCst));
    assert!(f.store.read().unwrap().state.custody.is_none());
}

#[test]
fn portable_install_is_read_only_in_preview_preserves_configuration_and_rejects_tampering() {
    let f = Fixture::new("complete");
    f.store.update(|s| s.pause(true, &[])).unwrap();
    let package = f.root.join("portable");
    let core = Path::new(env!("CARGO_BIN_EXE_bp"));
    let launcher = Path::new(env!("CARGO_BIN_EXE_bp-launcher"));
    install::bundle(&package, core, launcher).unwrap();
    let home = f.root.join("installed");
    install::install(&package, &home, Some(&f.root), true).unwrap();
    assert!(!home.exists());
    install::install(&package, &home, Some(&f.root), false).unwrap();
    let upgrade_skill = "source/orchestrator/skills/bridgeforge-update/SKILL.md";
    let installed_skill = fs::read(home.join(upgrade_skill)).unwrap();
    assert_eq!(
        installed_skill,
        include_bytes!("../../../source/orchestrator/skills/bridgeforge-update/SKILL.md")
    );
    let skills = f.root.join("user-skills");
    bridgepilot_install::user_skill::install(&home, &skills, true).unwrap();
    assert!(!skills.join("bridgepilot").exists());
    bridgepilot_install::user_skill::install(&home, &skills, false).unwrap();
    assert_eq!(
        bridgepilot_install::user_skill::status(&home, &skills).unwrap()["current"],
        true
    );
    fs::write(home.join(upgrade_skill), "user upgrade customization").unwrap();
    assert!(install::install(&package, &home, Some(&f.root), false).is_err());
    assert_eq!(
        fs::read(home.join(upgrade_skill)).unwrap(),
        b"user upgrade customization"
    );
    fs::write(home.join(upgrade_skill), &installed_skill).unwrap();
    let config = fs::read(home.join(".runtime/config/bridgepilot.json")).unwrap();
    install::install(&package, &home, Some(&f.root), false).unwrap();
    assert_eq!(
        config,
        fs::read(home.join(".runtime/config/bridgepilot.json")).unwrap()
    );
    let result = std::process::Command::new(home.join(".build/bridgepilot/bin/bp.exe"))
        .arg("self-test")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::write(home.join("AGENTS.md"), "user customization").unwrap();
    assert!(install::install(&package, &home, Some(&f.root), false).is_err());
    assert_eq!(
        fs::read_to_string(home.join("AGENTS.md")).unwrap(),
        "user customization"
    );
    fs::write(
        package.join("payload/bridgeforge-update-skill.md"),
        "tampered payload",
    )
    .unwrap();
    let rejected_home = f.root.join("tampered-install");
    assert!(install::install(&package, &rejected_home, Some(&f.root), true).is_err());
    assert!(!rejected_home.exists());
}
