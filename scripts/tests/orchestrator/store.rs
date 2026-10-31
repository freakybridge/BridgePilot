use super::*;

struct Fixture {
    root: PathBuf,
    store: Store,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.runtime/tests/bridgepilot-stage2/store")
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
    assert_eq!(
        fs::read_dir(f.store.archive.join("revisions"))
            .unwrap()
            .count(),
        2
    );
}
#[test]
fn interrupted_commit_leaves_orphan_but_head_remains_authoritative() {
    let f = Fixture::new();
    let head = read_bytes(&f.store.archive.join("HEAD.json")).unwrap();
    assert!(
        f.store
            .transact_inner(0, project, || Err("simulated crash before HEAD".into()))
            .is_err()
    );
    assert_eq!(
        read_bytes(&f.store.archive.join("HEAD.json")).unwrap(),
        head
    );
    assert!(f.store.read().unwrap().state.projects.is_empty());
    assert_eq!(
        fs::read_dir(f.store.archive.join("revisions"))
            .unwrap()
            .count(),
        2
    );
    f.store.transact(0, project).unwrap();
    assert_eq!(f.store.read().unwrap().revision, 1);
}
#[test]
fn corruption_and_missing_head_are_not_silently_repaired() {
    let f = Fixture::new();
    let h: Head = read_json(&f.store.archive.join("HEAD.json")).unwrap();
    fs::write(f.store.path(&h).unwrap(), b"corrupt").unwrap();
    assert!(f.store.read().is_err());
    assert!(f.store.transact(0, project).is_err());
    fs::remove_file(f.store.archive.join("HEAD.json")).unwrap();
    assert!(Store::init(&f.root).is_err());
}
#[test]
fn invalid_mutation_and_duplicate_json_never_commit() {
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
        .nth(2)
        .unwrap();
    assert!(Store::init(root).is_err());
}
