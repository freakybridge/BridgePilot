use super::*;
use crate::ProcessOutput;
use serde_json::json;
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Repo(PathBuf);
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn command(root: &Path, args: &[&str]) -> Vec<u8> {
    git(root, &SystemProcessRunner, args, vec![]).unwrap()
}
fn put(root: &Path, path: &str, bytes: &[u8]) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(root.join(path), bytes).unwrap();
}
fn config() -> Vec<u8> {
    serde_json::to_vec(&json!({"schema":1,"source_roots":["src/"],"python_artifacts":{"enabled":true,"allowed_roots":[".runtime/"]},"forbidden_source_components":["tests","examples","scripts",".runtime"],"cargo_registration":{"manifest_roots":["src/"],"forbidden_target_roots":["scripts/"]}})).unwrap()
}
fn repo() -> Repo {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let repo = Repo(
        Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().find(|p| p.join("source/forge/bridgeforge-manifest.json").is_file()).expect("factory repository")
            .join(".runtime")
            .join("tests")
            .join("artifact-layout")
            .join(format!(
                "bf-layout-unit-{}-{token}-{sequence}",
                std::process::id()
            )),
    );
    fs::create_dir_all(&repo.0).unwrap();
    command(&repo.0, &["init"]);
    command(&repo.0, &["config", "user.name", "Layout Test"]);
    command(&repo.0, &["config", "user.email", "layout@example.invalid"]);
    command(&repo.0, &["config", "core.hooksPath", ".git/hooks"]);
    put(&repo.0, "seed.txt", b"seed\n");
    put(
        &repo.0,
        ".gitignore",
        artifact_paths::IGNORE_RULES.as_bytes(),
    );
    put(&repo.0, CONFIG, &config());
    commit(&repo.0);
    repo
}
fn commit(root: &Path) {
    command(root, &["add", "."]);
    command(root, &["commit", "-m", "fixture seed"]);
}
fn report(root: &Path) -> StructureReport {
    staged(root, &SystemProcessRunner)
}

#[test]
fn layout_config_is_strict_and_project_owned() {
    assert!(parse(&config()).is_ok());
    for bytes in [
        br#"{"schema":2}"#.as_slice(),
        br#"{"schema":1,"schema":1}"#,
        br#"{"schema":1,"typo":true}"#,
        br#"{"schema":1,"source_roots":["../src/"]}"#,
        br#"{"schema":1,"source_roots":["C:\\src"]}"#,
        br#"{"schema":1,"source_roots":["scripts/"]}"#,
        br#"{"schema":1,"python_artifacts":{"enabled":true}}"#,
    ] {
        assert!(parse(bytes).is_err(), "{bytes:?}");
    }
}

#[test]
fn three_directory_default_policy_and_fixed_test_paths() {
    let cfg = Config::default();
    let managed = BTreeSet::new();
    for (path, dir) in [
        ("target", true),
        ("src/node_modules", true),
        (".runtime/target", true),
        (".build/node_modules", true),
        (".runtime/coverage.xml", false),
        (".runtime/tests/report.xml", false),
        (".codex/bin/bridgeforge.exe", false),
    ] {
        assert!(
            !path_findings(path, dir, Some(&cfg), &managed).is_empty(),
            "{path}"
        );
    }
    for (path, dir) in [
        (".build/target", true),
        (".deps/node_modules", true),
        (".runtime/tests/unit/coverage.xml", false),
        (".runtime/tests/unit", true),
        (".runtime/__pycache__/x.pyc", false),
        ("src/cache.rs", false),
        ("config/settings.json", false),
        ("scripts/tests/fixtures/input.json", false),
    ] {
        assert!(
            path_findings(path, dir, Some(&cfg), &managed).is_empty(),
            "{path}"
        );
    }
    for json in [
        br#"{"schema":1,"outputs":[{"kind":"config","path":".runtime/settings.json"}]}"#.as_slice(),
        br#"{"schema":1,"outputs":[{"kind":"download","path":".runtime/config/file.zip"}]}"#,
        br#"{"schema":1,"outputs":[{"kind":"test","path":".runtime/tests/result.xml"}]}"#,
        br#"{"schema":1,"python_artifacts":{"allowed_roots":["cache/"]}}"#,
    ] {
        assert!(parse(json).is_err());
    }
    assert!(parse(br#"{"schema":1,"outputs":[{"kind":"config","path":".runtime/config/settings.json"},{"kind":"download","path":".runtime/download/archive.zip"},{"kind":"test","path":".runtime/tests/unit/report.xml"}]}"#).is_ok());
}

#[test]
fn three_directory_ignore_uses_staged_rules_and_detects_tracked_artifacts() {
    let r = repo();
    put(
        &r.0,
        ".gitignore",
        b"/.runtime/\n/.deps/\n/.build/\n!/.build/\n",
    );
    command(&r.0, &["add", ".gitignore"]);
    put(&r.0, ".gitignore", artifact_paths::IGNORE_RULES.as_bytes());
    assert!(
        report(&r.0)
            .errors
            .iter()
            .any(|f| f.code == "artifact-ignore-required")
    );
    command(&r.0, &["add", ".gitignore"]);
    assert!(report(&r.0).errors.is_empty());
    put(&r.0, ".runtime/config/settings.json", b"{}\n");
    command(&r.0, &["add", "-f", ".runtime/config/settings.json"]);
    assert!(
        report(&r.0)
            .errors
            .iter()
            .any(|f| f.code == "tracked-local-artifact")
    );
    assert!(
        worktree(&r.0)
            .errors
            .iter()
            .any(|f| f.code == "tracked-local-artifact")
    );
    command(&r.0, &["rm", "--cached", ".runtime/config/settings.json"]);
    assert!(report(&r.0).errors.is_empty());
    assert!(r.0.join(".runtime/config/settings.json").is_file());
    assert!(
        ignore_findings(Some(b"/.runtime/\n/.deps/\n/.build/\n!/.build/keep.txt\n"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(ignore_findings(None).unwrap().len(), 3);
}

#[test]
fn three_directory_ignore_patterns_match_git_root_semantics() {
    let r = repo();
    fs::create_dir_all(r.0.join(".runtime")).unwrap();
    for suffix in [
        "",
        "!**/.runtime/\n",
        "!\\.runtime/\n",
        "!/.runtime/\n",
        "!/.runtime/keep\n",
        "!**/.runtime/\n/.runtime/\n",
    ] {
        let rules = format!("{}{suffix}", artifact_paths::IGNORE_RULES);
        put(&r.0, ".gitignore", rules.as_bytes());
        let mut request = ProcessRequest::new("git", &r.0);
        request.args = [
            "-c",
            "core.excludesFile=",
            "check-ignore",
            "--no-index",
            "--quiet",
            ".runtime/",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        let output = SystemProcessRunner.run(&request).unwrap();
        assert!(!output.timed_out && [0, 1].contains(&output.code));
        assert_eq!(
            ignore_findings(Some(rules.as_bytes())).unwrap().is_empty(),
            output.code == 0,
            "{suffix}"
        );
    }
}

#[test]
fn three_directory_worktree_defaults_and_legacy_cache_config_enforce_fixed_roots() {
    let r = repo();
    put(&r.0, ".runtime/tests/report.xml", b"bad");
    put(&r.0, ".runtime/coverage.xml", b"bad");
    for config in [config(), br#"{"schema":1}"#.to_vec()] {
        put(&r.0, CONFIG, &config);
        let report = worktree(&r.0);
        for path in [".runtime/tests/report.xml", ".runtime/coverage.xml"] {
            assert!(
                report.errors.iter().any(|f| f.path == path),
                "{path}: {:?}",
                report.errors
            );
        }
    }
    for (path, directory) in [
        ("src/bridgeforge", true),
        (".runtime/download/bridgeforge.exe", false),
    ] {
        assert!(path_findings(path, directory, None, &BTreeSet::new()).is_empty());
    }
}

#[test]
fn three_directory_deleting_violation_does_not_require_historical_ignore_repair() {
    let r = repo();
    command(&r.0, &["rm", ".gitignore"]);
    put(&r.0, ".runtime/old.log", b"historical");
    commit(&r.0);
    command(&r.0, &["rm", ".runtime/old.log"]);
    assert!(report(&r.0).errors.is_empty(), "{:?}", report(&r.0).errors);
}
#[test]
fn layout_effective_defaults_and_partial_configuration_are_equivalent() {
    let defaults = effective_config(None).unwrap();
    assert_eq!(defaults, parse(br#"{"schema":1}"#).unwrap());
    assert_eq!(
        defaults,
        parse(&serde_json::to_vec(&defaults).unwrap()).unwrap()
    );
    let partial = parse(br#"{"schema":1,"source_roots":["src/"],"python_artifacts":{"allowed_roots":[".runtime/"]}}"#).unwrap();
    assert_eq!(partial.tests_root, defaults.tests_root);
    assert!(!partial.python_artifacts.enabled);
    assert!(effective_config(Some(b"invalid")).is_err());
    assert!(parse(br#"{"schema":1,"tests_root":"tests/"}"#).is_err());
}

#[test]
fn layout_defaults_cover_all_entrypoints_and_keep_managed_identity() {
    let r = repo();
    let root = &r.0;
    command(root, &["rm", CONFIG]);
    commit(root);
    put(
        root,
        MANAGED,
        br#"{"assets":[{"target":".codex/hooks/tests/a.rs"}]}"#,
    );
    for path in [
        "scripts/tests/unit/a.rs",
        ".codex/hooks/tests/a.rs",
        "src/test_engine.rs",
    ] {
        put(root, path, b"#[cfg(test)] mod unit {}\n");
        assert!(edited(root, path).advisories.is_empty());
    }
    command(root, &["add", "."]);
    assert!(report(root).errors.is_empty());
    assert!(worktree(root).errors.is_empty());
    for path in ["src/tests/a.rs", ".codex/hooks/tests/b.rs"] {
        put(root, path, b"test\n");
        assert!(
            edited(root, path)
                .advisories
                .iter()
                .any(|f| f.code == "misplaced-test-directory")
        );
    }
    command(root, &["add", "."]);
    assert_eq!(report(root).errors.len(), 2);
    assert!(
        worktree(root)
            .errors
            .iter()
            .any(|f| f.path == "src/tests/a.rs")
    );
    assert!(!root.join(CONFIG).exists());
}

#[test]
fn layout_default_policy_changes_do_not_expand_unrelated_history() {
    let r = repo();
    let root = &r.0;
    command(root, &["rm", CONFIG]);
    put(root, "src/tests/old.rs", b"historical\n");
    commit(root);
    put(root, CONFIG, br#"{"schema":1}"#);
    command(root, &["add", CONFIG]);
    assert!(
        report(root).errors.is_empty(),
        "adding equivalent defaults is not a policy change"
    );
    commit(root);
    command(root, &["rm", CONFIG]);
    assert!(
        report(root).errors.is_empty(),
        "deleting equivalent defaults preserves A mode"
    );
    put(root, "src/tests/new.rs", b"new\n");
    command(root, &["add", "src/tests/new.rs"]);
    assert_eq!(
        report(root).errors.len(),
        1,
        "deletion does not disable defaults"
    );
    assert_eq!(report(root).errors[0].path, "src/tests/new.rs");
}

#[test]
fn layout_unknown_historical_policy_is_not_an_absent_policy() {
    let r = repo();
    let root = &r.0;
    put(root, CONFIG, b"invalid\n");
    put(root, "src/tests/old.rs", b"historical\n");
    commit(root);
    command(root, &["rm", CONFIG]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.path == "src/tests/old.rs")
    );
    command(root, &["rm", "src/tests/old.rs"]);
    assert!(report(root).errors.is_empty());
}

#[test]
fn layout_default_audit_prunes_runtime_but_staged_paths_are_checked() {
    let r = repo();
    let root = &r.0;
    command(root, &["rm", CONFIG]);
    commit(root);
    for path in [
        ".runtime/fixture/tests/a.rs",
        "target/fixture/tests/a.rs",
        ".venv/pkg/tests/a.py",
    ] {
        put(root, path, b"derived\n");
    }
    let audit = worktree(root);
    assert!(
        audit
            .errors
            .iter()
            .any(|f| f.path == "target" && f.code == "misplaced-artifact")
    );
    assert!(
        audit
            .errors
            .iter()
            .any(|f| f.path == ".venv" && f.code == "misplaced-artifact")
    );
    assert!(
        audit
            .advisories
            .iter()
            .any(|f| f.path == ".runtime/fixture" && f.code == "layout-excluded-tree")
    );
    command(root, &["add", "-f", ".runtime/fixture/tests/a.rs"]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.path == ".runtime/fixture/tests/a.rs")
    );
    put(
        root,
        "src/.runtime/tests/a.rs",
        b"not a root runtime directory\n",
    );
    assert!(!worktree(root).errors.is_empty());
}

#[test]
fn layout_narrow_test_root_allows_only_ancestor_directories() {
    let r = repo();
    let root = &r.0;
    put(
        root,
        CONFIG,
        br#"{"schema":1,"tests_root":"scripts/tests/unit/"}"#,
    );
    put(root, "scripts/tests/unit/a.rs", b"test\n");
    command(root, &["add", "."]);
    assert!(report(root).errors.is_empty());
    assert!(
        edited(root, "scripts/tests/unit/a.rs")
            .advisories
            .is_empty()
    );
    assert!(worktree(root).errors.is_empty());
    for path in ["scripts/tests/other/a.rs", "scripts/tests/helper.rs"] {
        put(root, path, b"outside narrowed root\n");
        assert!(
            edited(root, path)
                .advisories
                .iter()
                .any(|f| f.code == "misplaced-test-directory")
        );
    }
    command(root, &["add", "."]);
    assert_eq!(report(root).errors.len(), 2);
    let audit = worktree(root);
    assert!(!audit.errors.iter().any(|f| f.path == "scripts/tests"));
    assert!(
        audit
            .errors
            .iter()
            .any(|f| f.path == "scripts/tests/helper.rs")
    );
    assert!(
        audit
            .errors
            .iter()
            .any(|f| f.path == "scripts/tests/other/a.rs")
    );
}

#[test]
fn layout_empty_nonroot_test_directories_do_not_hide_explicit_rules_or_files() {
    let r = repo();
    let root = &r.0;
    command(root, &["rm", CONFIG]);
    commit(root);
    fs::create_dir_all(root.join("src/tests")).unwrap();
    assert!(worktree(root).errors.is_empty());
    put(root, "src/tests/.hidden", b"not empty\n");
    assert!(
        worktree(root)
            .errors
            .iter()
            .any(|f| f.code == "misplaced-test-directory")
    );
    command(root, &["add", "src/tests/.hidden"]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.path == "src/tests/.hidden")
    );
    command(root, &["rm", "-f", "src/tests/.hidden"]);
    fs::create_dir_all(root.join("src/tests")).unwrap();
    put(
        root,
        CONFIG,
        br#"{"schema":1,"source_roots":["src/"],"forbidden_source_components":["tests"]}"#,
    );
    assert!(
        worktree(root)
            .errors
            .iter()
            .any(|f| f.code == "forbidden-source-directory")
    );
    for path in ["test", "tests"] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    assert_eq!(
        worktree(root)
            .errors
            .iter()
            .filter(|f| f.code == "legacy-test-root")
            .count(),
        2
    );
}
#[test]
fn layout_rules_have_no_project_name_or_filename_blacklist() {
    let cfg = parse(&config()).unwrap();
    let empty = BTreeSet::new();
    for path in [
        "src/lib.rs",
        "src/test_engine.rs",
        "scripts/tests/unit/a.rs",
        ".runtime/__pycache__/a.pyc",
        "src/apps/m2_benchmark/main.rs",
        "scripts-other/tool.rs",
    ] {
        assert!(
            path_findings(path, false, Some(&cfg), &empty).is_empty(),
            "{path}"
        );
    }
    for path in [
        "tests/a.rs",
        "test/a.rs",
        "src/examples/a.rs",
        "src/.runtime/a.json",
        "src/tests/a.rs",
        "__pycache__/a.pyc",
        "scripts/tests/__pycache__/a.pyc",
        ".coverage.worker",
        "coverage.xml",
    ] {
        assert!(
            !path_findings(path, false, Some(&cfg), &empty).is_empty(),
            "{path}"
        );
    }
    assert!(!path_findings(".claude/tests/probe.py", false, Some(&cfg), &empty).is_empty());
    let managed = [key(".codex/hooks/tests/a.rs")].into_iter().collect();
    assert!(path_findings(".codex/hooks/tests/a.rs", false, Some(&cfg), &managed).is_empty());
    assert!(!path_findings(".codex/hooks/tests/b.rs", false, Some(&cfg), &managed).is_empty());
}
#[test]
fn layout_a_ignores_unrelated_history_but_blocks_modified_or_added_bad_paths() {
    let r = repo();
    let root = &r.0;
    put(root, "tests/old.rs", b"old\n");
    commit(root);
    put(root, "src/lib.rs", b"pub fn ok() {}\n");
    command(root, &["add", "src/lib.rs"]);
    assert!(report(root).errors.is_empty());
    assert!(
        worktree(root)
            .errors
            .iter()
            .any(|e| e.code == "legacy-test-root")
    );
    put(root, "tests/old.rs", b"changed\n");
    command(root, &["add", "tests/old.rs"]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|e| e.code == "legacy-test-root")
    );
    command(root, &["rm", "-f", "tests/old.rs"]);
    assert!(report(root).errors.is_empty());
}
#[test]
fn layout_a_reads_staged_config_and_manifest_not_worktree() {
    let r = repo();
    let root = &r.0;
    put(root, "src/Cargo.toml", b"[package]\nname='demo'\nversion='1.0.0'\n[[test]]\nname='probe'\npath='../scripts/tests/probe.rs'\n");
    command(root, &["add", "src/Cargo.toml"]);
    put(
        root,
        "src/Cargo.toml",
        b"[package]\nname='demo'\nversion='1.0.0'\n",
    );
    assert!(
        report(root)
            .errors
            .iter()
            .any(|e| e.code == "external-cargo-registration")
    );
    command(root, &["add", "src/Cargo.toml"]);
    put(
        root,
        "src/Cargo.toml",
        b"[[test]]\npath='../scripts/tests/a.rs'\n",
    );
    assert!(report(root).errors.is_empty());
    put(root, CONFIG, br#"{"schema":99}"#);
    command(root, &["add", CONFIG]);
    put(root, CONFIG, &config());
    assert!(
        report(root)
            .errors
            .iter()
            .any(|e| e.code == "artifact-layout-incomplete")
    );
}
#[test]
fn layout_config_changes_check_only_newly_affected_history() {
    let r = repo();
    let root = &r.0;
    put(root, "src/examples/old.rs", b"old\n");
    put(root, "src/benches/old.rs", b"old\n");
    commit(root);
    let mut cfg: serde_json::Value = serde_json::from_slice(&config()).unwrap();
    cfg["forbidden_source_components"]
        .as_array_mut()
        .unwrap()
        .push(json!("benches"));
    put(root, CONFIG, &serde_json::to_vec(&cfg).unwrap());
    command(root, &["add", CONFIG]);
    let errors = report(root).errors;
    assert!(errors.iter().any(|f| f.path == "src/benches/old.rs"));
    assert!(
        !errors.iter().any(|f| f.path == "src/examples/old.rs"),
        "{errors:?}"
    );
    put(
        root,
        CONFIG,
        &serde_json::to_vec_pretty(
            &serde_json::from_slice::<serde_json::Value>(&config()).unwrap(),
        )
        .unwrap(),
    );
    command(root, &["add", CONFIG]);
    assert!(report(root).errors.is_empty());
    command(root, &["rm", "-f", CONFIG]);
    assert!(report(root).errors.is_empty());
}
#[test]
fn layout_cargo_checks_new_references_and_affected_targets_not_unrelated_debt() {
    let r = repo();
    let root = &r.0;
    let cargo = b"[workspace]\nmembers=['../scripts/tool']\n[[bin]]\nname='old'\npath='../scripts/tool/main.rs'\n";
    put(root, "src/Cargo.toml", cargo);
    commit(root);
    put(root, "src/lib.rs", b"pub fn ok() {}\n");
    command(root, &["add", "src/lib.rs"]);
    assert!(report(root).errors.is_empty());
    put(root, "scripts/tool/main.rs", b"fn main() {}\n");
    command(root, &["add", "scripts/tool/main.rs"]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.code == "external-cargo-registration")
    );
    command(root, &["rm", "-f", "scripts/tool/main.rs"]);
    assert!(
        report(root).errors.is_empty(),
        "cancelled addition has no final diff"
    );
    put(root, "scripts/tool/main.rs", b"fn main() {}\n");
    commit(root);
    command(root, &["rm", "-f", "scripts/tool/main.rs"]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.code == "external-cargo-registration"),
        "deleting a tracked target affects its registration"
    );
}
#[test]
fn layout_root_defaults_work_without_config_and_rename_uses_destination() {
    let r = repo();
    let root = &r.0;
    command(root, &["rm", CONFIG]);
    commit(root);
    put(root, "src/a.rs", b"hello\n");
    commit(root);
    fs::create_dir_all(root.join("tests")).unwrap();
    command(root, &["mv", "src/a.rs", "tests/a.rs"]);
    assert_eq!(
        report(root)
            .errors
            .iter()
            .filter(|f| f.code == "legacy-test-root")
            .count(),
        1
    );
    command(root, &["mv", "tests/a.rs", "src/a.rs"]);
    assert!(report(root).errors.is_empty());
}
#[test]
fn layout_batch_process_count_does_not_grow_with_manifest_count() {
    struct Count(Cell<usize>);
    impl ProcessRunner for Count {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            self.0.set(self.0.get() + 1);
            SystemProcessRunner.run(request)
        }
    }
    let r = repo();
    let root = &r.0;
    for n in 0..40 {
        put(
            root,
            &format!("src/{n}/Cargo.toml"),
            b"[package]\nname='demo'\nversion='1.0.0'\n",
        );
    }
    command(root, &["add", "."]);
    let runner = Count(Cell::new(0));
    assert!(staged(root, &runner).errors.is_empty());
    assert_eq!(
        runner.0.get(),
        4,
        "diff, index, config blob and manifest batch only"
    );
}

#[test]
fn layout_defaults_read_managed_identities_only_when_relevant() {
    struct Count(Cell<usize>);
    impl ProcessRunner for Count {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            self.0.set(self.0.get() + 1);
            SystemProcessRunner.run(request)
        }
    }
    let r = repo();
    let root = &r.0;
    command(root, &["rm", CONFIG]);
    put(
        root,
        MANAGED,
        br#"{"assets":[{"target":".codex/hooks/tests/a.rs"}]}"#,
    );
    commit(root);
    put(root, "src/normal.rs", b"pub fn normal() {}\n");
    command(root, &["add", "."]);
    let runner = Count(Cell::new(0));
    assert!(staged(root, &runner).errors.is_empty());
    assert_eq!(
        runner.0.get(),
        2,
        "ordinary default path needs only diff and index"
    );
    put(root, ".codex/hooks/tests/a.rs", b"managed\n");
    command(root, &["add", "."]);
    let runner = Count(Cell::new(0));
    assert!(staged(root, &runner).errors.is_empty());
    assert_eq!(
        runner.0.get(),
        3,
        "a candidate exception must read actual managed identities"
    );
    commit(root);
    put(root, MANAGED, br#"{"assets":[]}"#);
    command(root, &["add", MANAGED]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.path == ".codex/hooks/tests/a.rs" && f.code == "misplaced-test-directory"),
        "identity revocation must select the unchanged file"
    );
    put(root, MANAGED, b"invalid\n");
    command(root, &["add", MANAGED]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.code == "artifact-layout-incomplete")
    );
}
#[test]
fn layout_git_failure_never_reports_success() {
    struct Fail;
    impl ProcessRunner for Fail {
        fn run(&self, _: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            Ok(ProcessOutput {
                code: 1,
                stdout: vec![],
                stderr: b"denied".to_vec(),
                timed_out: false,
            })
        }
    }
    assert!(!staged(Path::new("."), &Fail).errors.is_empty());
}

#[test]
fn layout_literal_git_names_are_not_config_patterns() {
    let r = repo();
    let root = &r.0;
    put(root, "doc/[draft].md", b"old\n");
    commit(root);
    put(root, "src/lib.rs", b"pub fn ok() {}\n");
    command(root, &["add", "src/lib.rs"]);
    assert!(report(root).errors.is_empty());
    put(root, "src/[new].rs", b"pub fn ok() {}\n");
    command(root, &["add", "src/[new].rs"]);
    assert!(report(root).errors.is_empty());
    assert!(edited(root, "src/[new].rs").advisories.is_empty());
    assert!(parse(br#"{"schema":1,"source_roots":["src/[glob]/"]}"#).is_err());
}

#[test]
fn layout_unrelated_allowed_root_change_does_not_reclassify_history() {
    let r = repo();
    let root = &r.0;
    put(root, "old/__pycache__/a.pyc", b"old\n");
    commit(root);
    let mut cfg: serde_json::Value = serde_json::from_slice(&config()).unwrap();
    cfg["python_artifacts"]["allowed_roots"]
        .as_array_mut()
        .unwrap()
        .push(json!(".runtime/other-cache/"));
    put(root, CONFIG, &serde_json::to_vec(&cfg).unwrap());
    command(root, &["add", CONFIG]);
    assert!(
        report(root).errors.is_empty(),
        "an already-forbidden cache is still unrelated history"
    );
}

#[test]
fn layout_malformed_historical_policy_can_be_repaired_or_deleted() {
    let r = repo();
    let root = &r.0;
    put(root, CONFIG, b"invalid JSON\n");
    commit(root);
    put(root, CONFIG, &config());
    command(root, &["add", CONFIG]);
    assert!(report(root).errors.is_empty());
    put(root, "src/examples/old.rs", b"old\n");
    command(root, &["add", "src/examples/old.rs"]);
    assert!(
        !report(root).errors.is_empty(),
        "repair still enforces the effective new policy"
    );
    command(root, &["rm", "-f", "src/examples/old.rs"]);
    command(root, &["rm", "-f", CONFIG]);
    assert!(report(root).errors.is_empty());
}

#[test]
fn layout_worktree_managed_test_ancestors_do_not_exempt_siblings_or_caches() {
    let r = repo();
    let root = &r.0;
    put(
        root,
        ".codex/managed-skeleton.json",
        br#"{"assets":[{"target":".codex/hooks/tests/a.rs"}]}"#,
    );
    put(root, ".codex/hooks/tests/a.rs", b"test fixture\n");
    assert!(worktree(root).errors.is_empty());
    put(root, ".codex/hooks/tests/b.rs", b"unmanaged\n");
    assert!(
        worktree(root)
            .errors
            .iter()
            .any(|f| f.path == ".codex/hooks/tests/b.rs")
    );
    put(root, ".codex/hooks/tests/__pycache__/a.pyc", b"cache\n");
    assert!(
        worktree(root)
            .errors
            .iter()
            .any(|f| f.code == "misplaced-python-artifact")
    );
}

#[test]
fn layout_workspace_member_globs_use_staged_paths_and_detect_target_changes() {
    let r = repo();
    let root = &r.0;
    put(
        root,
        "src/Cargo.toml",
        b"[workspace]\nmembers=['../scr*/tool']\n",
    );
    put(
        root,
        "scripts/tool/Cargo.toml",
        b"[package]\nname='probe'\nversion='1.0.0'\n",
    );
    command(root, &["add", "."]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.code == "external-cargo-registration")
    );
    commit(root);
    put(root, "src/lib.rs", b"pub fn ok() {}\n");
    command(root, &["add", "src/lib.rs"]);
    assert!(report(root).errors.is_empty());
    put(root, "scripts/tool/src/lib.rs", b"pub fn probe() {}\n");
    command(root, &["add", "scripts/tool/src/lib.rs"]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.code == "external-cargo-registration")
    );
    assert!(
        worktree(root)
            .errors
            .iter()
            .any(|f| f.code == "external-cargo-registration")
    );
    for (pattern, path, matches) in [
        ("scripts/**/tool", "scripts/tool", true),
        ("scripts/**/tool", "scripts/a/tool", true),
        ("scripts/[!a]ool", "scripts/tool", true),
        ("scripts/too?", "scripts/tool", true),
        ("scripts/*", "scripts/a/tool", false),
    ] {
        assert_eq!(
            glob_regex(pattern).unwrap().is_match(path),
            matches,
            "{pattern} -> {path}"
        );
    }
}

#[test]
fn layout_inline_cargo_targets_and_invalid_member_types_are_checked() {
    let cfg = parse(&config()).unwrap();
    let hits = check_manifest(
        "src/Cargo.toml",
        b"bin=[{name='tool',path='../scripts/tool/main.rs'}]\n",
        &cfg,
        None,
        &BTreeSet::new(),
        true,
        &BTreeSet::new(),
        None,
    );
    assert!(hits.iter().any(|f| f.code == "external-cargo-registration"));
    let hits = check_manifest(
        "src/Cargo.toml",
        b"[workspace]\nmembers='../scripts/tool'\n",
        &cfg,
        None,
        &BTreeSet::new(),
        true,
        &BTreeSet::new(),
        None,
    );
    assert!(hits.iter().any(|f| f.code == "invalid-cargo-layout"));
}

#[cfg(windows)]
#[test]
fn layout_empty_junction_is_reported_as_excluded_not_an_empty_directory() {
    let r = repo();
    let root = &r.0;
    let target = root.join("empty-target");
    let project = root.join("audit-project");
    fs::create_dir_all(&target).unwrap();
    fs::create_dir_all(project.join("src")).unwrap();
    let junction = project.join("src").join("tests");
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = worktree(&project);
    fs::remove_dir(&junction).unwrap();
    assert!(
        report
            .errors
            .iter()
            .any(|f| f.path == "src/tests" && f.code == "layout-excluded-link")
    );
    assert!(target.exists(), "audit must leave the link target intact");
}

#[cfg(windows)]
#[test]
fn layout_config_through_junction_is_not_read() {
    use std::process::Command;
    let r = repo();
    let root = &r.0;
    let target = root.join("external-policy");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("artifact-layout.json"), config()).unwrap();
    // A separate fixture root prevents touching a tracked .codex directory.
    let linked = root.join("linked-project");
    fs::create_dir_all(&linked).unwrap();
    let junction = linked.join(".codex");
    let output = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = worktree(&linked);
    fs::remove_dir(&junction).unwrap();
    assert!(
        result
            .errors
            .iter()
            .any(|f| f.code == "artifact-layout-incomplete")
    );
}

#[test]
fn layout_combined_config_and_cargo_changes_preserve_reference_scope() {
    let r = repo();
    let root = &r.0;
    let manifest = b"[workspace]\nmembers=['../scr*/tool']\n";
    put(root, "src/Cargo.toml", manifest);
    put(
        root,
        "scripts/tool/Cargo.toml",
        b"[package]\nname='tool'\nversion='1.0.0'\n",
    );
    commit(root);
    let mut cfg: serde_json::Value = serde_json::from_slice(&config()).unwrap();
    cfg["python_artifacts"]["allowed_roots"]
        .as_array_mut()
        .unwrap()
        .push(json!(".runtime/other-cache/"));
    put(root, CONFIG, &serde_json::to_vec(&cfg).unwrap());
    put(
        root,
        "src/Cargo.toml",
        b"# a comment only\n[workspace]\nmembers=['../scr*/tool']\n",
    );
    command(root, &["add", "."]);
    assert!(
        report(root).errors.is_empty(),
        "unrelated policy and comment changes cannot revive historical references"
    );
    put(root, "scripts/tool/src/lib.rs", b"pub fn changed() {}\n");
    command(root, &["add", "scripts/tool/src/lib.rs"]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.code == "external-cargo-registration"),
        "glob target changes remain in scope even with a simultaneous policy change"
    );
}

#[test]
fn layout_managed_identity_removal_checks_only_newly_unmanaged_paths() {
    let r = repo();
    let root = &r.0;
    put(root, MANAGED, br#"{"assets":[{"target":".codex/hooks/tests/a.rs"},{"target":".codex/hooks/tests/b.rs"}]}"#);
    put(root, ".codex/hooks/tests/a.rs", b"managed a\n");
    put(root, ".codex/hooks/tests/b.rs", b"managed b\n");
    put(root, "legacy/tests/old.rs", b"old unowned debt\n");
    commit(root);
    put(
        root,
        MANAGED,
        br#"{"assets":[{"target":".codex/hooks/tests/b.rs"}]}"#,
    );
    command(root, &["add", MANAGED]);
    let errors = report(root).errors;
    assert!(errors.iter().any(|f| f.path == ".codex/hooks/tests/a.rs"));
    assert!(
        !errors
            .iter()
            .any(|f| f.path == ".codex/hooks/tests/b.rs" || f.path == "legacy/tests/old.rs")
    );
    // Removing the newly unowned file resolves the actual change.
    command(root, &["rm", ".codex/hooks/tests/a.rs"]);
    assert!(report(root).errors.is_empty());
}
#[test]
fn layout_invalid_toml_is_an_error_and_preview_remains_advisory() {
    let r = repo();
    let root = &r.0;
    put(root, "src/Cargo.toml", b"[invalid\n");
    command(root, &["add", "src/Cargo.toml"]);
    assert!(
        report(root)
            .errors
            .iter()
            .any(|f| f.code == "invalid-cargo-layout")
    );
    let preview = edited(root, "src/Cargo.toml");
    assert!(preview.errors.is_empty());
    assert!(!preview.advisories.is_empty());
    assert!(
        crate::project_structure::inspect_documents(root)
            .errors
            .is_empty()
    );
}
#[cfg(windows)]
#[test]
fn layout_windows_case_and_separator_are_normalized() {
    let cfg = parse(&config()).unwrap();
    assert!(
        !path_findings(
            "SRC\\EXAMPLES\\probe.rs",
            false,
            Some(&cfg),
            &BTreeSet::new()
        )
        .is_empty()
    );
    assert!(within("SRC/sub/file.rs", "src/"));
    assert!(!within("src-other/file.rs", "src/"));
}
