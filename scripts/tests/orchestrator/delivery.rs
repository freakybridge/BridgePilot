use super::*;
use bridgepilot_execution::delivery::{self, DevelopmentReview, PublicationReview};
use bridgepilot_tasks::digest;
use serde_json::{Value, json};

fn git_test(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn repository(f: &Fixture, publish: bool) -> (PathBuf, PathBuf, PathBuf) {
    let s = f.store.read().unwrap();
    let root = s.state.runs[&f.run].binding.root.clone();
    fs::write(root.join("VERSION"), "1.0.0\n").unwrap();
    fs::write(root.join(".gitignore"), ".build/\n.runtime/\n.bp-*\n").unwrap();
    fs::create_dir(root.join(".codex")).unwrap();
    fs::write(
        root.join(".codex/bridgeforge-version.json"),
        "{\"schema\":4}",
    )
    .unwrap();
    fs::write(
        root.join(".codex/package-closures.json"),
        "{\"entries\":[]}",
    )
    .unwrap();
    let tool = root.join(".build/bridgeforge/bin/bridgeforge.exe");
    fs::create_dir_all(tool.parent().unwrap()).unwrap();
    fs::copy(Fixture::executable(), &tool).unwrap();
    let preview = json!({"schema":1,"status":"task-development-current","readiness_scope":"task","affected_packages":[],"pending_reasons":{},"capabilities":["package-development-v1","version-coverage-v1","task-scoped-development-v1"],"fingerprint":"sha256:controlled-native-preview","packages":{"p":{"version":"0.1.0"}}});
    fs::write(
        root.join(".bp-fixture-development.json"),
        preview.to_string(),
    )
    .unwrap();
    git_test(&root, &["init", "-b", "main"]);
    git_test(&root, &["config", "user.name", "Controlled BP test"]);
    git_test(&root, &["config", "user.email", "bp-test@example.invalid"]);
    git_test(&root, &["add", "."]);
    git_test(&root, &["commit", "-m", "controlled development baseline"]);
    let remote = f.root.join("controlled-remote.git");
    git_test(&root, &["init", "--bare", remote.to_str().unwrap()]);
    git_test(
        &root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git_test(&root, &["push", "-u", "origin", "main"]);
    f.store
        .update(|s| {
            s.projects.get_mut("p").unwrap().repository = remote.to_str().unwrap().into();
            let task = s.tasks.get_mut(&f.task).unwrap();
            task.input.authorization.allow_write = true;
            task.input.authorization.allow_publish = publish;
            task.spec_hash = digest(&serde_json::to_vec(&task.input).unwrap());
            s.runs.get_mut(&f.run).unwrap().spec_hash = task.spec_hash.clone();
            Ok(())
        })
        .unwrap();
    (root, tool, remote)
}
fn result_hash(f: &Fixture) -> String {
    digest(
        &serde_json::to_vec(
            f.store.read().unwrap().state.runs[&f.run]
                .result
                .as_ref()
                .unwrap(),
        )
        .unwrap(),
    )
}
fn development(f: &Fixture, tool: &Path) -> DevelopmentReview {
    DevelopmentReview {
        run: f.run.clone(),
        result_sha256: result_hash(f),
        source: "test:controlled managed preview plus real Git".into(),
        note: "actual read-only verification".into(),
        tool: tool.into(),
        packages: vec!["p".into()],
    }
}

#[test]
fn managed_development_requires_current_scope_and_does_not_publish() {
    let f = Fixture::new("complete");
    let (root, tool, _) = repository(&f, false);
    codex::execute(&f.store, &f.run, Fixture::executable(), 15).unwrap();
    let path = root.join(".bp-fixture-development.json");
    let original = fs::read(&path).unwrap();
    let mut pending: Value = serde_json::from_slice(&original).unwrap();
    pending["status"] = json!("development-analysis-required");
    fs::write(&path, pending.to_string()).unwrap();
    let before = f.store.read().unwrap().revision;
    assert!(delivery::review_development(&f.store, before, development(&f, &tool)).is_err());
    assert_eq!(before, f.store.read().unwrap().revision);
    fs::write(path, original).unwrap();
    let proof = delivery::review_development(&f.store, before, development(&f, &tool)).unwrap();
    assert_eq!(proof["status"], "development_verified");
    assert_eq!(proof["publication_verified"], false);
    assert_eq!(
        f.store.read().unwrap().state.tasks[&f.task].state,
        TaskState::AwaitingRelease
    );
    let input = PublicationReview {
        run: f.run.clone(),
        result_sha256: result_hash(&f),
        source: "test:unauthorized publication claim".into(),
        receipt_path: root.join(".runtime/nonexistent.json"),
        receipt_sha256: "not-a-receipt".into(),
        remote: "origin".into(),
        remote_ref: "refs/heads/main".into(),
    };
    assert!(
        delivery::review_publication(&f.store, f.store.read().unwrap().revision, input)
            .unwrap_err()
            .contains("authority")
    );
}

#[test]
fn publication_verifies_live_push_url_and_preserves_unrelated_dirty_work() {
    let f = Fixture::new("complete");
    let (root, tool, fetch_remote) = repository(&f, true);
    let push_remote = f.root.join("controlled-push-remote.git");
    git_test(
        &root,
        &[
            "clone",
            "--bare",
            fetch_remote.to_str().unwrap(),
            push_remote.to_str().unwrap(),
        ],
    );
    git_test(
        &root,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            push_remote.to_str().unwrap(),
        ],
    );
    f.store
        .update(|s| {
            s.projects.get_mut("p").unwrap().repository = push_remote.to_str().unwrap().into();
            Ok(())
        })
        .unwrap();
    codex::execute(&f.store, &f.run, Fixture::executable(), 15).unwrap();
    delivery::review_development(
        &f.store,
        f.store.read().unwrap().revision,
        development(&f, &tool),
    )
    .unwrap();
    fs::write(root.join("VERSION"), "1.1.0\n").unwrap();
    git_test(&root, &["add", "VERSION"]);
    git_test(&root, &["commit", "-m", "controlled release commit"]);
    let head = git_test(&root, &["rev-parse", "HEAD"]);
    fs::create_dir_all(root.join(".runtime")).unwrap();
    let receipt = json!({"schema":1,"status":"synced","release_requested":true,"push_performed":true,"version_bumped":true,"ahead":0,"behind":0,"commit":head,"push_target":"origin/main","version_before":"1.0.0","version_after":"1.1.0","release_details":{"schema_version":4,"version_after":"1.1.0","packages":[{"id":"p","version_after":"0.1.0"}]}});
    let path = root.join(".runtime/controlled-managed-release.json");
    fs::write(&path, receipt.to_string()).unwrap();
    let input = PublicationReview {
        run: f.run.clone(),
        result_sha256: result_hash(&f),
        source: "test:controlled receipt and real local bare Git remote".into(),
        receipt_path: path.clone(),
        receipt_sha256: digest(&fs::read(&path).unwrap()),
        remote: "origin".into(),
        remote_ref: "refs/heads/main".into(),
    };
    let before = f.store.read().unwrap().revision;
    // Fetch already contains the commit, but the actual push URL does not.
    // Querying the remote alias here would incorrectly certify publication.
    git_test(&root, &["push", fetch_remote.to_str().unwrap(), "main"]);
    let error = delivery::review_publication(&f.store, before, input.clone()).unwrap_err();
    assert!(
        error.contains("live push target"),
        "unexpected verification boundary: {error}"
    );
    assert_eq!(before, f.store.read().unwrap().revision);
    git_test(&root, &["push", "origin", "main"]);
    fs::write(root.join("unrelated-user-work.txt"), "preserved\n").unwrap();
    let proof = delivery::review_publication(&f.store, before, input).unwrap();
    assert_eq!(proof["status"], "publication_verified");
    assert_eq!(proof["proof"]["code"]["working_tree"], "dirty");
    assert_eq!(
        f.store.read().unwrap().state.tasks[&f.task].state,
        TaskState::Released
    );
    assert_eq!(
        fs::read_to_string(root.join("unrelated-user-work.txt")).unwrap(),
        "preserved\n"
    );
}
