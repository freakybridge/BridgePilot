//! Withdrawal proofs operate only on isolated, locally committed fixture histories.
use super::*;

fn publish(root: &Path, runner: &Checks) -> String {
    let q = release_request(root, "patch");
    for (path, bytes) in build(root, "fix: fixture release", &q, runner)
        .unwrap()
        .unwrap()
        .writes
    {
        fs::write(path, bytes).unwrap();
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "fixture formal release"]);
    git(root, &["rev-parse", "HEAD"])
}

fn withdrawn(document_input: bool) -> (Repo, Value) {
    let r = zero_fixture();
    configure(&r.0, false);
    let mut cfg: Value = serde_json::from_slice(&fs::read(r.0.join(CONFIG)).unwrap()).unwrap();
    cfg["development"]["a"]["checks"][0]["inputs"] = json!(["CHANGELOG.md"]);
    write(&r.0, CONFIG, &serde_json::to_vec(&cfg).unwrap());
    if document_input {
        let mut cfg: Value = serde_json::from_slice(&fs::read(r.0.join(CONFIG)).unwrap()).unwrap();
        cfg["product_inputs"]
            .as_array_mut()
            .unwrap()
            .push(json!("doc/runtime-policy.md"));
        write(&r.0, CONFIG, &serde_json::to_vec(&cfg).unwrap());
        write(&r.0, "doc/runtime-policy.md", b"original product policy\n");
    }
    let runner = Checks::new();
    change(&r.0, true);
    let q = request(&r.0, "patch", false, &runner);
    let result = close(&r.0, &q, &runner);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let restored = publish(&r.0, &runner);
    write(&r.0, "pkg/src/lib.rs", b"pub fn value()->u8{3}\n");
    let q = request(&r.0, "patch", false, &runner);
    let result = close(&r.0, &q, &runner);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let withdrawn = publish(&r.0, &runner);
    let retained_log = fs::read(r.0.join("CHANGELOG.md")).unwrap();
    git(
        &r.0,
        &[
            "restore",
            "--source",
            &restored,
            "--staged",
            "--worktree",
            "--",
            ".",
        ],
    );
    write(&r.0, "CHANGELOG.md", &retained_log);
    git(&r.0, &["add", "CHANGELOG.md"]);
    if document_input {
        write(&r.0, "doc/runtime-policy.md", b"changed product policy\n");
        git(&r.0, &["add", "doc/runtime-policy.md"]);
    }
    git(&r.0, &["commit", "-m", "authorized fixture withdrawal"]);
    let rollback = git(&r.0, &["rev-parse", "HEAD"]);
    let declaration = json!({"rollback":rollback,"restored_release":restored,"withdrawn_release":withdrawn,
        "reason":"explicit isolated fixture authorization", "retained_documentation":if document_input {vec!["CHANGELOG.md","doc/runtime-policy.md"]} else {vec!["CHANGELOG.md"]}});
    (r, declaration)
}

fn declare(root: &Path, declaration: Value) {
    let mut cfg: Value = serde_json::from_slice(&fs::read(root.join(CONFIG)).unwrap()).unwrap();
    cfg["release_withdrawals"] = json!([declaration]);
    write(root, CONFIG, &serde_json::to_vec(&cfg).unwrap());
}

#[test]
fn configuration_update_preserves_verified_withdrawal_history() {
    let (r, record) = withdrawn(false);
    declare(&r.0, record);
    git(&r.0, &["add", CONFIG]);
    git(&r.0, &["commit", "-m", "record verified fixture withdrawal"]);
    let before = fs::read(r.0.join(CONFIG)).unwrap();
    let ledger = fs::read(r.0.join(development::RECORD)).unwrap();
    let previous: Value = serde_json::from_slice(&before).unwrap();
    let mut proposal = previous.clone();
    proposal.as_object_mut().unwrap().remove("baseline");
    proposal["development"]["a"]["checks"][0]["inputs"] =
        json!(["CHANGELOG.md", "README.md"]);
    let proposal_path = ".runtime/withdrawal-config-proposal.json";
    write(&r.0, proposal_path, &serde_json::to_vec(&proposal).unwrap());
    let plan = configuration_plan(&r.0, &Checks::new(), Some(proposal_path), None);
    assert_eq!(plan.code, 0, "{}", plan.stderr);
    let plan = plan.receipt.unwrap();
    assert_eq!(fs::read(r.0.join(CONFIG)).unwrap(), before);
    assert_eq!(plan["configuration"]["baseline"], previous["baseline"]);
    assert_eq!(
        plan["configuration"]["release_withdrawals"],
        previous["release_withdrawals"]
    );
    let applied = configuration_apply(
        &r.0,
        &Checks::new(),
        Some(proposal_path),
        None,
        plan["fingerprint"].as_str().unwrap(),
        true,
    );
    assert_eq!(applied.code, 0, "{}", applied.stderr);
    let current: Value = serde_json::from_slice(&fs::read(r.0.join(CONFIG)).unwrap()).unwrap();
    assert_eq!(current["baseline"], previous["baseline"]);
    assert_eq!(current["release_withdrawals"], previous["release_withdrawals"]);
    assert_eq!(current["development"], proposal["development"]);
    assert_eq!(fs::read(r.0.join(development::RECORD)).unwrap(), ledger);
    let preview = development::preview(&r.0, &Request::default(), &Checks::new());
    assert_eq!(preview.code, 0, "{}", preview.stderr);
}

#[test]
fn withdrawal_restores_content_keeps_published_floors_and_survives_next_release() {
    let (r, record) = withdrawn(false);
    let runner = Checks::new();
    let blocked = development::preview(&r.0, &Request::default(), &runner);
    assert!(
        blocked.stderr.contains("explicit verified withdrawal"),
        "{}",
        blocked.stderr
    );
    declare(&r.0, record.clone());
    let cfg = load(&r.0).unwrap().unwrap();
    let history =
        withdrawal::audit(&r.0, &cfg, &record["rollback"].as_str().unwrap(), &runner).unwrap();
    assert_eq!(history.product_high_water.to_string(), "1.0.2");
    assert_eq!(history.package_high_water["a"].version, "0.3.2");
    let p = development::preview(&r.0, &Request::default(), &runner);
    assert_eq!(p.code, 0, "{}", p.stderr);
    write(&r.0, "pkg/src/lib.rs", b"pub fn value()->u8{4}\n");
    let q = request(&r.0, "patch", false, &runner);
    let p = close(&r.0, &q, &runner);
    assert_eq!(p.code, 0, "{}", p.stderr);
    assert_eq!(
        p.receipt.unwrap()["package_changes"][0]["version_after"],
        "0.3.3"
    );
    let mut cfg: Value = serde_json::from_slice(&fs::read(r.0.join(CONFIG)).unwrap()).unwrap();
    let from = cfg["packages"][0].clone();
    cfg["packages"] = json!([]);
    cfg["adapters"] = json!({});
    cfg["lifecycle"] = json!([{"id":"remove-a","base":record["rollback"],"operation":"remove",
        "compatibility":"compatible","from":[from],"to":[]}]);
    fs::remove_dir_all(r.0.join("pkg")).unwrap();
    write(&r.0, CONFIG, &serde_json::to_vec(&cfg).unwrap());
    let q = request(&r.0, "patch", false, &runner);
    let result = close(&r.0, &q, &runner);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let correct_ledger = fs::read(r.0.join(development::RECORD)).unwrap();
    let mut invalid = development::Ledger::parse(Some(&correct_ledger)).unwrap();
    let mut previous = String::new();
    for entry in &mut invalid.entries {
        if entry.before.as_deref() == Some("0.3.3") {
            entry.before = Some("0.3.2".into());
        }
        if entry.after.as_deref() == Some("0.3.3") {
            entry.after = Some("0.3.2".into());
        }
        entry.previous = previous;
        entry.hash.clear();
        entry.hash = digest(&serde_json::to_vec(entry).unwrap());
        previous = entry.hash.clone();
    }
    write(
        &r.0,
        development::RECORD,
        &serde_json::to_vec(&invalid).unwrap(),
    );
    let rejected = preview(&r.0, "fix: forbidden reuse", &Request::default(), &runner);
    assert!(
        rejected
            .stderr
            .contains("must exceed withdrawn published version"),
        "{}",
        rejected.stderr
    );
    write(&r.0, development::RECORD, &correct_ledger);
    let head = publish(&r.0, &runner);
    assert_eq!(
        fs::read_to_string(r.0.join("VERSION")).unwrap().trim(),
        "1.0.3"
    );
    let cfg = load(&r.0).unwrap().unwrap();
    assert_eq!(
        withdrawal::audit(&r.0, &cfg, &head, &runner)
            .unwrap()
            .boundary,
        head
    );
    let mut value: Value = serde_json::from_slice(&fs::read(r.0.join(CONFIG)).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("release_withdrawals");
    write(&r.0, CONFIG, &serde_json::to_vec(&value).unwrap());
    assert!(
        development::preview(&r.0, &Request::default(), &runner)
            .stderr
            .contains("withdrawal")
    );
    fs::remove_file(r.0.join(CONFIG)).unwrap();
    let failure = history::validate_release_baseline(&r.0, &runner).unwrap_err();
    assert!(
        failure.contains("cannot be removed or downgraded"),
        "{failure}"
    );
}

#[test]
fn withdrawal_documentation_cannot_exempt_product_inputs() {
    let (r, record) = withdrawn(true);
    declare(&r.0, record);
    let result = development::preview(&r.0, &Request::default(), &Checks::new());
    assert!(
        result
            .stderr
            .contains("documentation is a product/package input"),
        "{}",
        result.stderr
    );
}

#[test]
fn withdrawal_requires_exact_restoration_and_unmodified_git_history() {
    let (r, mut record) = withdrawn(false);
    write(&r.0, "pkg/src/lib.rs", b"pub fn injected(){}\n");
    git(&r.0, &["add", "."]);
    git(&r.0, &["commit", "--amend", "--no-edit"]);
    record["rollback"] = json!(git(&r.0, &["rev-parse", "HEAD"]));
    declare(&r.0, record.clone());
    let result = development::preview(&r.0, &Request::default(), &Checks::new());
    assert!(
        result.stderr.contains("did not restore product content"),
        "{}",
        result.stderr
    );
    let cfg = load(&r.0).unwrap().unwrap();
    write(&r.0, ".git/info/grafts", b"# fixture graft marker\n");
    let error = withdrawal::audit(
        &r.0,
        &cfg,
        record["rollback"].as_str().unwrap(),
        &Checks::new(),
    )
    .unwrap_err();
    assert!(error.contains("replacement refs or grafts"), "{error}");
    let mut empty = cfg;
    empty.release_withdrawals.clear();
    assert!(
        withdrawal::audit(
            &r.0,
            &empty,
            record["rollback"].as_str().unwrap(),
            &Checks::new()
        )
        .unwrap_err()
        .contains("replacement refs or grafts")
    );
    assert!(
        withdrawal::has_current_history(&r.0, record["rollback"].as_str().unwrap(), &Checks::new())
            .unwrap_err()
            .contains("replacement refs or grafts")
    );
}

#[test]
fn withdrawal_audit_allows_initial_configuration_without_historical_config() {
    let r = fixture("unconfigured", true);
    let mut proposal: Value = serde_json::from_slice(&fs::read(r.0.join(CONFIG)).unwrap()).unwrap();
    proposal["schema_version"] = json!(4);
    git(&r.0, &["rm", CONFIG]);
    git(&r.0, &["commit", "--amend", "--no-edit"]);
    write(
        &r.0,
        ".runtime/proposal.json",
        &serde_json::to_vec(&proposal).unwrap(),
    );
    let p = configuration_plan(
        &r.0,
        &SystemProcessRunner,
        Some(".runtime/proposal.json"),
        None,
    );
    assert_eq!(p.code, 0, "{}", p.stderr);
    let p = configuration_apply(
        &r.0,
        &SystemProcessRunner,
        Some(".runtime/proposal.json"),
        None,
        p.receipt.unwrap()["fingerprint"].as_str().unwrap(),
        true,
    );
    assert_eq!(p.code, 0, "{}", p.stderr);
    configure(&r.0, false);
    change(&r.0, true);
    let p = development::preview(&r.0, &Request::default(), &Checks::new());
    assert_eq!(p.code, 0, "{}", p.stderr);
}

#[test]
fn withdrawal_floor_follows_stable_id_through_same_language_move() {
    let previous = NativePackage {
        name: "a".into(),
        manifest: "old/Cargo.toml".into(),
        version: "0.4.0".into(),
        language: Language::Rust,
        workspace: "Cargo.toml".into(),
        source_roots: vec!["old/".into()],
    };
    let mut moved = previous.clone();
    moved.manifest = "new/Cargo.toml".into();
    moved.name = "renamed".into();
    moved.version = "0.3.0".into();
    assert_eq!(
        withdrawal::version_base(&moved, Some(&previous))
            .unwrap()
            .version,
        "0.4.0"
    );
    moved.language = Language::Python;
    assert!(withdrawal::version_base(&moved, Some(&previous)).is_err());
}

#[test]
fn withdrawal_document_roles_separate_checks_from_stability_and_delivery() {
    let mut value = policy(false);
    value["checks"][0]["inputs"] = json!(["doc/policy.md"]);
    let ordinary: development::Policy = serde_json::from_value(value.clone()).unwrap();
    assert!(!ordinary.covers_durable_input("doc/policy.md"));
    for field in ["inputs", "artifacts"] {
        let mut protected = value.clone();
        protected["stability"] = json!({"contracts":[],"checks":[{"id":"stable","program":"fixture-check",field:["doc/policy.md"]}]});
        let protected: development::Policy = serde_json::from_value(protected).unwrap();
        assert!(protected.covers_durable_input("doc/policy.md"));
    }
    value["release_artifacts"] = json!(["doc/policy.md"]);
    let delivery: development::Policy = serde_json::from_value(value).unwrap();
    assert!(delivery.covers_durable_input("doc/policy.md"));
}
