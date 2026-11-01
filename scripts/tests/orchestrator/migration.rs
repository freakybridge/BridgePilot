use super::*;
use crate::domain::{Authorization, Complexity, Project, TaskInput};
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    store: Store,
    head: Head,
    original: BTreeMap<String, Vec<u8>>,
}
impl Fixture {
    fn new() -> Self {
        let root = std::path::absolute(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.runtime/tests/bridgepilot-status-sqlite/migration")
                .join(crate::new_id("case")),
        )
        .unwrap();
        fs::create_dir_all(root.join("doc/5_project_knowledgebase/bridgepilot/revisions")).unwrap();
        let store = Store::open(&root).unwrap();
        let mut snapshot = Snapshot {
            schema: 1,
            archive_id: crate::new_id("archive"),
            revision: 0,
            parent: None,
            state: crate::domain::State::default(),
        };
        let mut original = BTreeMap::new();
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let h = Head {
            schema: 1,
            archive_id: snapshot.archive_id.clone(),
            revision: 0,
            digest: digest(&bytes),
        };
        original.insert(format!("revisions/0-{}.json", h.digest), bytes);
        snapshot.revision = 1;
        snapshot.parent = Some(h);
        snapshot.state.projects.insert(
            "p".into(),
            Project {
                id: "p".into(),
                name: "project".into(),
                repository: "repo:p".into(),
                paused: false,
            },
        );
        for title in ["first", "second"] {
            snapshot
                .state
                .add_task(TaskInput {
                    project: "p".into(),
                    title: title.into(),
                    goal: title.into(),
                    non_goals: vec![],
                    acceptance: vec!["actual evidence".into()],
                    dependencies: vec![],
                    complexity: Complexity::Simple,
                    authorization: Authorization {
                        human_source: "test:human".into(),
                        allow_write: false,
                        allow_publish: false,
                        allow_execution_chat: false,
                    },
                })
                .unwrap();
        }
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let head = Head {
            schema: 1,
            archive_id: snapshot.archive_id.clone(),
            revision: 1,
            digest: digest(&bytes),
        };
        original.insert(format!("revisions/1-{}.json", head.digest), bytes);
        original.insert("HEAD.json".into(), serde_json::to_vec(&head).unwrap());
        for (name, bytes) in &original {
            fs::write(store.archive.join(name), bytes).unwrap();
        }
        Self {
            root,
            store,
            head,
            original,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn preview_is_read_only_and_legacy_writer_requires_explicit_migration() {
    let f = Fixture::new();
    let result = preview(&f.store).unwrap();
    assert_eq!(result["writes"], false);
    assert_eq!(result["revisions"], 2);
    assert!(!f.store.database_path().exists());
    assert!(!f.store.runtime.exists());
    assert!(
        f.store
            .transact(1, |_| Ok(()))
            .unwrap_err()
            .contains("explicit legacy migration")
    );
    for (path, bytes) in &f.original {
        assert_eq!(fs::read(f.store.archive.join(path)).unwrap(), *bytes);
    }
}

#[test]
fn complete_history_numbers_and_original_bytes_survive_migration() {
    let f = Fixture::new();
    let result = apply(&f.store, 1, "test:approved").unwrap();
    assert_eq!(result["status"], "archive_migrated");
    assert_eq!(f.store.head().unwrap(), f.head);
    let backup = PathBuf::from(result["backup"].as_str().unwrap());
    for (path, bytes) in &f.original {
        assert_eq!(fs::read(backup.join(path)).unwrap(), *bytes);
    }
    assert_eq!(
        fs::read(f.store.archive.join("HEAD.json")).unwrap(),
        RETIRED_HEAD
    );
    assert!(!f.store.archive.join("revisions").exists());
    let conn = f.store.read_connection().unwrap();
    assert!(
        database::read_at(&conn, 0)
            .unwrap()
            .state
            .projects
            .is_empty()
    );
    assert_eq!(database::read_at(&conn, 1).unwrap().state.tasks.len(), 2);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM legacy_revisions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    let mut q = conn
        .prepare("SELECT id,task_no FROM tasks ORDER BY id")
        .unwrap();
    let numbers = q
        .query_map([], |r| r.get::<_, i64>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(numbers, vec![1, 2]);
    let result = apply(&f.store, 1, "test:approved again").unwrap();
    assert_eq!(result["changed"], false);
}

#[test]
fn corruption_and_unreferenced_revisions_fail_without_creating_candidate() {
    let f = Fixture::new();
    fs::write(f.store.archive.join("revisions/orphan.json"), b"{}").unwrap();
    assert!(preview(&f.store).unwrap_err().contains("unreferenced"));
    assert!(apply(&f.store, 1, "test").is_err());
    assert!(!f.store.archive.join(CANDIDATE).exists());
    fs::remove_file(f.store.archive.join("revisions/orphan.json")).unwrap();
    let revision = f
        .original
        .keys()
        .find(|n| n.starts_with("revisions/1-"))
        .unwrap();
    fs::write(f.store.archive.join(revision), b"{}").unwrap();
    assert!(
        apply(&f.store, 1, "test")
            .unwrap_err()
            .contains("digest mismatch")
    );
    assert!(!f.store.database_path().exists());
}

#[test]
fn interrupted_migration_resumes_each_activation_boundary_without_losing_originals() {
    for stage in [
        "candidate_saved",
        "head_backed_up",
        "legacy_backed_up",
        "guard_installed",
        "sqlite_activated",
    ] {
        let f = Fixture::new();
        let error = apply_inner(&f.store, 1, "test:authorized", |at| {
            if at == stage {
                Err(format!("controlled interruption at {at}"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(error.contains("controlled interruption"));
        if stage != "sqlite_activated" {
            assert!(Store::init(&f.root).is_err());
        }
        recover(&f.store, "test:recover").unwrap();
        assert_eq!(f.store.head().unwrap(), f.head);
        assert_eq!(f.store.read().unwrap().state.tasks.len(), 2);
        assert!(!f.store.archive.join(CANDIDATE).exists());
        let backup = f
            .store
            .archive
            .join(format!("legacy-json-backup/1-{}", &f.head.digest[..16]));
        for (path, bytes) in &f.original {
            assert_eq!(fs::read(backup.join(path)).unwrap(), *bytes);
        }
    }
}

#[test]
fn legacy_duplicate_keys_and_post_candidate_external_edits_are_not_overwritten() {
    let f = Fixture::new();
    fs::write(
        f.store.archive.join("HEAD.json"),
        b"{\"schema\":1,\"schema\":1}",
    )
    .unwrap();
    assert!(preview(&f.store).is_err());
    assert!(!f.store.database_path().exists());
    fs::write(
        f.store.archive.join("HEAD.json"),
        f.original.get("HEAD.json").unwrap(),
    )
    .unwrap();
    apply_inner(&f.store, 1, "test", |at| {
        if at == "candidate_saved" {
            Err("stop".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    fs::write(f.store.archive.join("HEAD.json"), b"external edit").unwrap();
    assert!(recover(&f.store, "test").is_err());
    assert_eq!(
        fs::read(f.store.archive.join("HEAD.json")).unwrap(),
        b"external edit"
    );
}
