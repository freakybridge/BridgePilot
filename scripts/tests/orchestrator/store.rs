use super::*;

struct Fixture {
    root: PathBuf,
    store: Store,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/bridgepilot-status-sqlite/store")
            .join(new_id("case"));
        let root = std::path::absolute(root).unwrap();
        let store = Store::init(&root).unwrap();
        Self { root, store }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn project(s: &mut State) -> Result<()> {
    s.projects.insert(
        "p".into(),
        crate::domain::Project {
            id: "p".into(),
            name: "p".into(),
            repository: "repo:p".into(),
            paused: false,
        },
    );
    Ok(())
}

#[test]
fn immutable_revision_cas_and_history_survive_restart() {
    let f = Fixture::new();
    let id = f.store.read().unwrap().archive_id;
    let (s, ()) = f.store.transact(0, project).unwrap();
    assert_eq!(s.revision, 1);
    assert!(f.store.transact(0, project).is_err());
    let reopened = Store::open(&f.root).unwrap();
    let loaded = reopened.read().unwrap();
    assert_eq!(loaded.archive_id, id);
    assert_eq!(loaded.state.projects.len(), 1);
    let conn = reopened.read_connection().unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM revisions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(
        database::read_at(&conn, 0)
            .unwrap()
            .state
            .projects
            .is_empty()
    );
    assert_eq!(database::read_at(&conn, 1).unwrap().state.projects.len(), 1);
    assert!(!f.store.archive.join("revisions").exists());
}
#[test]
fn interrupted_sql_transaction_rolls_back_state_and_history_together() {
    let f = Fixture::new();
    let head = f.store.head().unwrap();
    assert!(
        f.store
            .transact_inner(0, project, || Err("simulated crash before COMMIT".into()))
            .is_err()
    );
    assert_eq!(f.store.head().unwrap(), head);
    assert!(f.store.read().unwrap().state.projects.is_empty());
    let conn = f.store.read_connection().unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM revisions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(conn);
    f.store.transact(0, project).unwrap();
    assert_eq!(f.store.read().unwrap().revision, 1);
}
#[test]
fn corruption_and_missing_head_are_not_silently_repaired() {
    let f = Fixture::new();
    fs::write(f.store.database_path(), b"corrupt").unwrap();
    assert!(f.store.read().is_err());
    assert!(f.store.transact(0, project).is_err());
    assert!(Store::init(&f.root).is_err());
}
#[test]
fn invalid_mutation_and_modified_legacy_guard_never_commit() {
    let f = Fixture::new();
    assert!(
        f.store
            .transact(0, |s| {
                s.projects.insert(
                    "bad/../id".into(),
                    crate::domain::Project {
                        id: "bad/../id".into(),
                        name: "bad".into(),
                        repository: "x".into(),
                        paused: false,
                    },
                );
                Ok(())
            })
            .is_err()
    );
    assert_eq!(f.store.read().unwrap().revision, 0);
    fs::write(
        f.store.archive.join("HEAD.json"),
        b"{\"schema\":1,\"schema\":1}",
    )
    .unwrap();
    assert!(f.store.read().is_err());
}
#[test]
fn process_lock_blocks_second_writer_without_deleting_lock_file() {
    let f = Fixture::new();
    let path = f.store.runtime.join("locks/writer.lock");
    let _guard = FileLock::acquire(&path).unwrap();
    assert!(f.store.transact(0, project).is_err());
    assert!(path.is_file());
}
#[test]
fn cleanup_of_local_index_preserves_archive_and_rebuilds() {
    let f = Fixture::new();
    f.store.transact(0, project).unwrap();
    let machine = f.store.machine().unwrap();
    f.store.rebuild_index().unwrap();
    fs::remove_dir_all(&f.store.runtime).unwrap();
    assert_eq!(f.store.read().unwrap().revision, 1);
    assert_eq!(f.store.machine().unwrap(), machine);
    assert!(f.store.rebuild_index().unwrap().is_file());
}
#[test]
fn read_status_does_not_recreate_runtime_or_lock_files() {
    let f = Fixture::new();
    fs::remove_dir_all(&f.store.runtime).unwrap();
    f.store.read().unwrap();
    assert!(!f.store.runtime.exists());
}
#[test]
fn product_repository_cannot_become_user_archive() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("bridgeforge-manifest.json").is_file())
        .expect("product repository root");
    assert!(Store::init(root).is_err());
}

fn task_input(title: &str) -> crate::domain::TaskInput {
    crate::domain::TaskInput {
        project: "p".into(),
        title: title.into(),
        goal: title.into(),
        non_goals: vec![],
        acceptance: vec!["verified".into()],
        dependencies: vec![],
        complexity: crate::domain::Complexity::Simple,
        authorization: crate::domain::Authorization {
            human_source: "test:human".into(),
            allow_write: false,
            allow_publish: false,
            allow_execution_chat: false,
        },
    }
}

#[test]
fn stable_numbers_survive_cancellation_reopen_and_transport() {
    let f = Fixture::new();
    f.store.transact(0, project).unwrap();
    let (_, first) = f
        .store
        .transact(1, |s| s.add_task(task_input("first")))
        .unwrap();
    f.store
        .transact(2, |s| {
            s.tasks.get_mut(&first).unwrap().state = crate::domain::TaskState::Cancelled;
            Ok(())
        })
        .unwrap();
    let (_, second) = f
        .store
        .transact(3, |s| s.add_task(task_input("second")))
        .unwrap();
    let reopened = Store::open(&f.root).unwrap();
    let conn = reopened.read_connection().unwrap();
    let number = |id: &str| {
        conn.query_row("SELECT task_no FROM tasks WHERE id=?1", [id], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap()
    };
    assert_eq!(number(&first), 1);
    assert_eq!(number(&second), 2);
    drop(conn);
    let files = f.store.archive_files().unwrap();
    let other = Fixture::new();
    other.store.install_transport(&files).unwrap();
    let conn = other.store.read_connection().unwrap();
    assert_eq!(
        conn.query_row("SELECT task_no FROM tasks WHERE id=?1", [&first], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT task_no FROM tasks WHERE id=?1", [&second], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn local_recovery_rows_never_enter_shared_snapshot_and_survive_import() {
    let f = Fixture::new();
    f.store.transact(0, project).unwrap();
    let conn = f.store.write_connection().unwrap();
    let secret = b"SUPER_PRIVATE_LOCAL_RECOVERY_DO_NOT_EXPORT";
    conn.execute(
        "INSERT INTO local_records(key,value) VALUES('private',?1)",
        [secret.as_slice()],
    )
    .unwrap();
    drop(conn);
    let bytes = f
        .store
        .archive_files()
        .unwrap()
        .remove(database::FILE)
        .unwrap();
    assert!(!bytes.windows(secret.len()).any(|s| s == secret));
    database::inspect_shared_bytes(&bytes).unwrap();
    let mut state = database::inspect_bytes(&bytes).unwrap().state;
    state.projects.get_mut("p").unwrap().name = "changed".into();
    let (_, next) = database::derive_bytes(&bytes, state).unwrap();
    f.store
        .install_transport(&BTreeMap::from([(database::FILE.into(), next)]))
        .unwrap();
    let conn = f.store.read_connection().unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT value FROM local_records WHERE key='private'",
            [],
            |r| r.get::<_, Vec<u8>>(0)
        )
        .unwrap(),
        secret
    );
    assert_eq!(f.store.read().unwrap().state.projects["p"].name, "changed");
}

#[test]
fn oversized_snapshot_plus_local_outbox_is_rejected_before_replacing_online_database() {
    let f = Fixture::new();
    f.store.transact(0, project).unwrap();
    let conn = f.store.write_connection().unwrap();
    let outbox = vec![b'p'; 32 * 1024];
    conn.execute(
        "INSERT INTO local_records(key,value) VALUES('transport.pending',?1)",
        [&outbox],
    )
    .unwrap();
    drop(conn);
    let before = fs::read(f.store.database_path()).unwrap();
    let before_head = f.store.head().unwrap();
    let shared = f
        .store
        .archive_files()
        .unwrap()
        .remove(database::FILE)
        .unwrap();
    let mut state = database::inspect_shared_bytes(&shared).unwrap().state;
    state.projects.get_mut("p").unwrap().name = "remote changed project".repeat(1024);
    let (_, candidate) = database::derive_bytes(&shared, state).unwrap();
    let local = database::local_records(&f.store.read_connection().unwrap()).unwrap();
    let merged = database::restore_local(&candidate, &local).unwrap();
    let bound = merged.len() - 1;
    assert!(
        candidate.len() < bound,
        "fixture must only exceed bound after local records are merged"
    );
    assert!(
        before.len() < bound,
        "original database must remain within the simulated bound"
    );
    let error = f
        .store
        .install_transport_inner(&BTreeMap::from([(database::FILE.into(), candidate)]), bound)
        .unwrap_err();
    assert!(error.contains("after preserving local records"), "{error}");
    assert_eq!(fs::read(f.store.database_path()).unwrap(), before);
    assert_eq!(f.store.head().unwrap(), before_head);
    assert_eq!(f.store.read().unwrap().state.projects["p"].name, "p");
    let conn = f.store.read_connection().unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT value FROM local_records WHERE key='transport.pending'",
            [],
            |r| r.get::<_, Vec<u8>>(0)
        )
        .unwrap(),
        outbox
    );
}

#[test]
fn oversized_unsigned_domain_values_fail_without_truncation_or_partial_commit() {
    let f = Fixture::new();
    let machine = f.store.machine().unwrap();
    assert!(
        f.store
            .transact(0, |s| {
                project(s)?;
                s.custody = Some(crate::handoff::Custody {
                    epoch: u64::MAX,
                    owner: Some(machine),
                    handoff: None,
                    checkpoints: BTreeMap::new(),
                    verified_bindings: BTreeMap::new(),
                    continuations: BTreeMap::new(),
                    source: "test".into(),
                });
                Ok(())
            })
            .unwrap_err()
            .contains("SQLite integer limit")
    );
    let snapshot = f.store.read().unwrap();
    assert_eq!(snapshot.revision, 0);
    assert!(snapshot.state.projects.is_empty());
}

#[test]
fn readonly_connection_does_not_allow_writes_or_create_runtime() {
    let f = Fixture::new();
    fs::remove_dir_all(&f.store.runtime).unwrap();
    let before = fs::read(f.store.database_path()).unwrap();
    let conn = f.store.read_connection().unwrap();
    assert!(
        conn.execute(
            "INSERT INTO local_records(key,value) VALUES('bad',x'00')",
            []
        )
        .is_err()
    );
    drop(conn);
    assert_eq!(fs::read(f.store.database_path()).unwrap(), before);
    assert!(!f.store.runtime.exists());
    assert!(
        !f.store
            .database_path()
            .with_file_name("bridgepilot.sqlite3-journal")
            .exists()
    );
}

#[test]
fn unfinished_initializer_blocks_writes_and_recovers_without_resetting_archive() {
    let f = Fixture::new();
    let initial = fs::read(f.store.database_path()).unwrap();
    let candidate = f.store.archive.join("bridgepilot.initial.sqlite3");
    fs::write(&candidate, &initial).unwrap();
    assert!(
        f.store
            .transact(0, project)
            .unwrap_err()
            .contains("initialization activation")
    );
    let store = Store::init(&f.root).unwrap();
    assert!(!candidate.exists());
    store.transact(0, project).unwrap();
    fs::write(&candidate, &initial).unwrap();
    assert!(
        Store::init(&f.root)
            .unwrap_err()
            .contains("differs from unfinished initializer")
    );
    assert_eq!(store.read().unwrap().revision, 1);
    fs::remove_file(candidate).unwrap();
}
