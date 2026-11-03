#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

use bridgeforge_core::{ProcessOutput, ProcessRequest, ProcessRunner};
use bridgeforge_workflow::project_sync::{
    SyncMode, apply_plan, build_plan, build_plan_with_inputs,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn gpt6_behavior_cases_have_unique_ids_and_resolvable_inputs() {
    // Validates test inputs only, not model behavior or an A/B result.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let suite: Value =
        serde_json::from_str(include_str!("../fixtures/gpt6-behavior-cases.json")).unwrap();
    assert_eq!(suite["schema"], 1);
    let baseline = suite["baseline_ref"].as_str().unwrap();
    assert!(baseline.len() == 40 && baseline.bytes().all(|b| b.is_ascii_hexdigit()));
    let mut ids = std::collections::BTreeSet::new();
    for case in suite["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        assert!(!id.is_empty() && ids.insert(id));
        for field in ["context", "user"] {
            assert!(
                !case[field].as_str().unwrap().trim().is_empty(),
                "{id}: {field}"
            );
        }
        for field in ["instructions", "expect", "forbid"] {
            let entries = case[field].as_array().unwrap();
            assert!(!entries.is_empty(), "{id}: {field}");
            for value in entries {
                let text = value.as_str().unwrap();
                assert!(!text.trim().is_empty());
                if field == "instructions" {
                    let path = Path::new(text);
                    assert!(
                        path.components()
                            .all(|part| matches!(part, std::path::Component::Normal(_)))
                    );
                    assert!(bridgeforge_core::source_projection::template_path(&root, text).is_file(), "{id}: {text}");
                }
            }
        }
    }
    let required = [
        "diagnose_only",
        "ambiguous_scope",
        "authorized_continue",
        "discussion_only",
        "scope_boundary",
        "readonly_review",
    ]
    .into_iter()
    .collect();
    assert!(
        ids.is_superset(&required),
        "original behavior cases must remain"
    );
}

#[test]
fn distributed_roles_match_skill_references_and_dogfood() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let contract: Value =
        serde_json::from_slice(&fixture_product::read(root.join("source/forge/bridgeforge-manifest.json")).unwrap())
            .unwrap();
    let roles = contract["assets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|asset| {
            asset["id"]
                .as_str()
                .unwrap_or("")
                .starts_with("codex.agent.")
        })
        .map(|asset| {
            let name = asset["id"]
                .as_str()
                .unwrap()
                .strip_prefix("codex.agent.")
                .unwrap();
            let source = fs::read_to_string(root.join(asset["source"].as_str().unwrap()))
                .unwrap()
                .replace("\r\n", "\n");
            let mirror = fs::read_to_string(root.join(asset["target"].as_str().unwrap()))
                .unwrap()
                .replace("\r\n", "\n");
            assert_eq!(source, mirror);
            assert!(
                source
                    .lines()
                    .any(|line| line == format!("name = \"{name}\""))
            );
            assert!(
                !source
                    .lines()
                    .filter_map(|line| line.split_once('='))
                    .any(|(key, _)| {
                        matches!(key.trim(), "model" | "effort" | "model_reasoning_effort")
                    }),
                "role must not select session settings: {name}"
            );
            name.to_string()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        roles,
        [
            "implementation-worker",
            "light-explorer",
            "review-auditor",
            "xhigh-auditor"
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );
    for directory in ["templates/agents", ".codex/agents"] {
        let actual = fs::read_dir(bridgeforge_core::source_projection::template_path(&root, directory))
            .unwrap()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(actual, roles);
    }
    let mut pending = vec![bridgeforge_core::source_projection::template_path(&root, "templates/skills"), root.join(".codex/skills")];
    let mut referenced = std::collections::BTreeSet::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            for (index, token) in text.split('`').enumerate() {
                if index % 2 == 1
                    && ["-worker", "-explorer", "-auditor"]
                        .iter()
                        .any(|suffix| token.ends_with(suffix))
                {
                    assert!(
                        roles.contains(token),
                        "unresolved role {token} in {}",
                        path.display()
                    );
                    referenced.insert(token.to_string());
                }
            }
        }
    }
    assert!(referenced.contains("implementation-worker") && referenced.contains("review-auditor"));
}

#[test]
fn public_instruction_region_matches_template_and_dogfood() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let public = |text: &str| {
        text.split_once("<!-- BRIDGEFORGE:PUBLIC:BEGIN -->")
            .unwrap()
            .1
            .split_once("<!-- BRIDGEFORGE:PUBLIC:END -->")
            .unwrap()
            .0
            .replace("\r\n", "\n")
    };
    let template = fs::read_to_string(bridgeforge_core::source_projection::template_path(&root, "templates/AGENTS.md")).unwrap();
    let dogfood = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    let shared = public(&template);
    // Text matching cannot prove authorization behavior; exercise that in the
    // isolated scope_boundary scenario. This check owns region integrity only.
    assert!(!shared.trim().is_empty());
    for text in [&template, &dogfood] {
        for marker in [
            "<!-- BRIDGEFORGE:PUBLIC:BEGIN -->",
            "<!-- BRIDGEFORGE:PUBLIC:END -->",
            "<!-- BRIDGEFORGE:PROJECT:BEGIN -->",
            "<!-- BRIDGEFORGE:PROJECT:END -->",
        ] {
            assert_eq!(text.matches(marker).count(), 1, "invalid marker: {marker}");
        }
    }
    assert_eq!(shared, public(&dogfood));
}

#[test]
fn project_maps_are_not_fixed_managed_payloads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let contract: Value =
        serde_json::from_slice(&fixture_product::read(root.join("source/forge/bridgeforge-manifest.json")).unwrap())
            .unwrap();
    for asset in contract["assets"].as_array().unwrap() {
        let target = asset["target"].as_str().unwrap();
        assert!(
            !target.starts_with(".runtime/")
                && !matches!(target, ".codex/find-doc.map.md" | ".codex/sync-docs.map.md"),
            "generated maps must not become distributed fixed payloads: {target}"
        );
    }
    assert!(
        !bridgeforge_core::source_projection::template_path(&root, "templates/skills/find-doc/references/map-reminder-sop.md")
            .exists(),
        "manual map reminder SOP must be retired"
    );
}

#[cfg(windows)]
#[test]
fn shared_source_reuse_keeps_source_and_failure_gates() {
    let fixture = Fixture::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let mut request = ProcessRequest::new("powershell.exe", root);
    request.args = vec![
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-File".into(),
        root.join("scripts/tests/shared_source_reuse.ps1")
            .into_os_string(),
        "-RepositoryRoot".into(),
        root.as_os_str().to_owned(),
        "-Base".into(),
        fixture.0.as_os_str().to_owned(),
    ];
    request.timeout = std::time::Duration::from_secs(60);
    let output = bridgeforge_core::SystemProcessRunner.run(&request).unwrap();
    assert!(!output.timed_out);
    assert_eq!(
        output.code,
        0,
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[test]
fn shared_manifest_separates_scopes_and_rolls_back_drift() {
    let fixture = Fixture::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let mut request = ProcessRequest::new("powershell.exe", root);
    request.args = vec![
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-File".into(),
        root.join("scripts/tests/shared_manifest.ps1")
            .into_os_string(),
        "-RepositoryRoot".into(),
        root.as_os_str().to_owned(),
        "-Base".into(),
        fixture.0.as_os_str().to_owned(),
    ];
    request.timeout = std::time::Duration::from_secs(60);
    let output = bridgeforge_core::SystemProcessRunner.run(&request).unwrap();
    assert!(!output.timed_out);
    assert_eq!(
        output.code,
        0,
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[test]
fn shared_cli_reuse_is_verified_in_isolated_profile() {
    let fixture = Fixture::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let mut request = ProcessRequest::new("powershell.exe", root);
    request.args = vec![
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-File".into(),
        root.join("scripts/tests/shared_cli_reuse.ps1")
            .into_os_string(),
        "-RepositoryRoot".into(),
        root.as_os_str().to_owned(),
        "-Base".into(),
        fixture.0.as_os_str().to_owned(),
    ];
    request.timeout = std::time::Duration::from_secs(60);
    let output = bridgeforge_core::SystemProcessRunner.run(&request).unwrap();
    assert!(!output.timed_out);
    assert_eq!(
        output.code,
        0,
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[test]
fn shared_bundle_commit_keeps_components_consistent_when_old_image_is_running() {
    let fixture = Fixture::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let mut request = ProcessRequest::new("powershell.exe", root);
    request.args = [
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    request.args.extend([
        root.join("scripts/tests/shared_transaction.ps1")
            .into_os_string(),
        "-RepositoryRoot".into(),
        root.as_os_str().to_owned(),
        "-Base".into(),
        fixture.0.as_os_str().to_owned(),
    ]);
    request.timeout = std::time::Duration::from_secs(60);
    let output = bridgeforge_core::SystemProcessRunner.run(&request).unwrap();
    assert!(!output.timed_out);
    assert_eq!(
        output.code,
        0,
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("shared bundle rollback and deferred committed cleanup passed")
    );
}
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "distribution")
            .join(format!(
                "d-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn write(path: &Path, content: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}
fn hash(data: impl AsRef<[u8]>) -> String {
    format!("sha256:{:x}", Sha256::digest(data.as_ref()))
}
fn contract() -> Value {
    json!({
        "schema_version":4,"baseline_model": "current-only", "compatibility_baseline": "8.0.2","host":"codex","release_version":"8.0.2",
        "stamp":".codex/.bridgeforge_codex_version","contract_target":".codex/managed-skeleton.json",
        "assets":[{"id":"example","source":"templates/example.txt","target":".codex/example.txt","strategy":"whole","current_sha256":hash(b"managed\n")}],"generated_assets":[]
    })
}
fn minimal(f: &Fixture) -> (PathBuf, PathBuf) {
    let factory = f.0.join("factory");
    let project = f.0.join("project");
    fs::create_dir_all(&project).unwrap();
    write(&bridgeforge_core::source_projection::template_path(&factory, "templates/example.txt"), b"managed\n");
    fixture_product::install(
        &factory.join("source/forge/bridgeforge-manifest.json"),
        serde_json::to_vec_pretty(&contract()).unwrap(),
    );
    (factory, project)
}

#[test]
fn current_version_repair_applies_and_receipt_proves_finalization() {
    let f = Fixture::new();
    let (factory, project) = minimal(&f);
    let first = build_plan(&project, &factory, SyncMode::Init).unwrap();
    apply_plan(first.clone(), &first.aggregate_fingerprint, false).unwrap();
    write(&project.join(".codex/example.txt"), b"drift\n");
    let plan = build_plan(&project, &factory, SyncMode::Update).unwrap();
    assert!(!plan.safe.iter().any(|a| a.id == "managed.stamp"));
    let receipt = apply_plan(plan.clone(), &plan.aggregate_fingerprint, true).unwrap();
    assert!(receipt.stamp_written_last);
    assert_eq!(receipt.project_readiness, "ready");
    assert_eq!(
        fs::read(project.join(".codex/example.txt")).unwrap(),
        b"managed\n"
    );
    assert_eq!(
        build_plan(&project, &factory, SyncMode::Update)
            .unwrap()
            .status,
        "current"
    );
}

#[test]
fn doc_structure_accepts_directory_instructions() {
    for layout in ["flat", "milestone"] {
        let f = Fixture::new();
        write(
            &f.0.join("doc/README.md"),
            format!("---\ndelivery_layout: {layout}\n---\n"),
        );
        for layer in [
            "0_architecture",
            "1_delivery",
            "2_bugs",
            "3_reference",
            "4_archive",
            "5_project_knowledgebase",
        ] {
            fs::create_dir_all(f.0.join("doc").join(layer)).unwrap();
        }
        write(
            &f.0.join("doc/AGENTS.md"),
            "# Documentation rules\n\n- Preserve project instructions.\n",
        );
        let report = bridgeforge_checks::project_structure::inspect(&f.0);
        assert!(report.errors.is_empty(), "{layout}: {report:?}");
    }
}

#[test]
fn structure_and_archive_agree_with_unfinished_independent_todos() {
    let f = Fixture::new();
    write(
        &f.0.join("doc/README.md"),
        "---\ndelivery_layout: flat\n---\nDemo\n",
    );
    write(
        &f.0.join("doc/1_delivery/demo/requirements_test.md"),
        "---\nlifecycle: completed\nvalidation_status: verified\n---\nUser accepted\n",
    );
    write(
        &f.0.join("doc/1_delivery/demo/scan.md"),
        "---\nlifecycle: active\n---\nHistorical scan\n",
    );
    let table = "| 编号 | 日期 | 内容 | 状态 | 归属 | 关联文档 |\n|---|---|---|---|---|---|\n| TODO-001 | 2026-10-11 | 后续验证 | 待验证 | 后续事项 | [来源](../1_delivery/demo/requirements_test.md) |\n";
    write(&f.0.join("doc/0_architecture/TODO-INDEX.md"), table);
    let report = bridgeforge_checks::project_structure::inspect(&f.0);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let candidates = bridgeforge_checks::archive_scan::scan(&f.0).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        report
            .advisories
            .iter()
            .filter(|item| item.code.ends_with("archive-candidate"))
            .count(),
        candidates.len()
    );
    write(
        &f.0.join("doc/0_architecture/TODO-INDEX.md"),
        table.replace("后续事项", "原需求"),
    );
    assert!(
        bridgeforge_checks::archive_scan::scan(&f.0)
            .unwrap()
            .is_empty()
    );
    assert!(
        !bridgeforge_checks::project_structure::inspect(&f.0)
            .advisories
            .iter()
            .any(|item| item.code.ends_with("archive-candidate"))
    );
}

#[test]
fn doc_structure_rejects_unexpected_entries() {
    for (name, directory) in [
        ("extra.md", false),
        ("6_unknown", true),
        ("AGENTS.md", true),
    ] {
        let f = Fixture::new();
        write(
            &f.0.join("doc/README.md"),
            "---\ndelivery_layout: flat\n---\n",
        );
        let path = f.0.join("doc").join(name);
        if directory {
            fs::create_dir_all(path).unwrap();
        } else {
            write(&path, "# Unexpected document\n");
        }
        let report = bridgeforge_checks::project_structure::inspect(&f.0);
        assert!(
            report.errors.iter().any(|finding| {
                finding.code == "unexpected-doc-entry" && finding.path == format!("doc/{name}")
            }),
            "{name}: {report:?}"
        );
    }
}

#[test]
fn knowledge_structure_accepts_old_and_new_layout_and_never_auto_archives_topics() {
    let f = Fixture::new();
    write(
        &f.0.join("doc/README.md"),
        "---\ndelivery_layout: flat\n---\n",
    );
    for layer in [
        "0_architecture",
        "1_delivery",
        "2_bugs",
        "3_reference",
        "4_archive",
    ] {
        fs::create_dir_all(f.0.join("doc").join(layer)).unwrap();
    }
    assert!(
        bridgeforge_checks::project_structure::inspect(&f.0)
            .errors
            .is_empty()
    );
    write(
        &f.0.join("doc/5_project_knowledgebase/topic/note.md"),
        "---\nlifecycle: completed\n---\n# 2000-01-01 topic\n",
    );
    assert!(
        bridgeforge_checks::project_structure::inspect(&f.0)
            .errors
            .is_empty()
    );
    assert!(
        bridgeforge_checks::archive_scan::scan(&f.0)
            .unwrap()
            .is_empty()
    );
    fs::create_dir_all(f.0.join("doc/6_unknown")).unwrap();
    assert!(
        bridgeforge_checks::project_structure::inspect(&f.0)
            .errors
            .iter()
            .any(|finding| finding.code == "unexpected-doc-entry")
    );
}

#[test]
fn knowledge_seed_initializes_and_upgrades_without_owning_topic_bytes() {
    for existing in [false, true] {
        let f = Fixture::new();
        let (factory, project) = minimal(&f);
        let mut current = contract();
        current["release_version"] = json!("8.0.10");
        current["compatibility_baseline"] = json!("8.0.6");
        fixture_product::install(
            &factory.join("source/forge/bridgeforge-manifest.json"),
            serde_json::to_vec(&current).unwrap(),
        );
        if existing {
            let plan = build_plan(&project, &factory, SyncMode::Init).unwrap();
            apply_plan(plan.clone(), &plan.aggregate_fingerprint, false).unwrap();
        }
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let real: Value = serde_json::from_slice(
            &fixture_product::read(repository.join("source/forge/bridgeforge-manifest.json")).unwrap(),
        )
        .unwrap();
        let seed = real["assets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|asset| asset["id"] == "codex.doc.knowledgebase-seed")
            .unwrap()
            .clone();
        assert_eq!(seed["strategy"], "seed");
        current["assets"].as_array_mut().unwrap().push(seed.clone());
        current["release_version"] = json!("8.1.0");
        write(&factory.join(seed["source"].as_str().unwrap()), b"");
        fixture_product::install(
            &factory.join("source/forge/bridgeforge-manifest.json"),
            serde_json::to_vec(&current).unwrap(),
        );
        let plan = build_plan(&project, &factory, SyncMode::Auto).unwrap();
        let target = project.join(seed["target"].as_str().unwrap());
        assert!(!target.exists(), "planning must not create the seed");
        apply_plan(plan.clone(), &plan.aggregate_fingerprint, false).unwrap();
        assert!(target.is_file());
        let topic = project.join("doc/5_project_knowledgebase/topic/note.md");
        write(&topic, b"project knowledge\r\n");
        write(&target, b"project-owned seed\r\n");
        current["release_version"] = json!("8.1.1");
        fixture_product::install(
            &factory.join("source/forge/bridgeforge-manifest.json"),
            serde_json::to_vec(&current).unwrap(),
        );
        let plan = build_plan(&project, &factory, SyncMode::Update).unwrap();
        apply_plan(plan.clone(), &plan.aggregate_fingerprint, false).unwrap();
        assert_eq!(fs::read(&topic).unwrap(), b"project knowledge\r\n");
        assert_eq!(fs::read(&target).unwrap(), b"project-owned seed\r\n");
        assert_eq!(
            build_plan(&project, &factory, SyncMode::Update)
                .unwrap()
                .status,
            "current"
        );
    }
}

#[test]
fn old_manifest_never_proves_deletion_ownership() {
    let f = Fixture::new();
    let (factory, project) = minimal(&f);
    write(
        &project.join(".codex/.bridgeforge_codex_version"),
        b"8.0.0\n",
    );
    write(
        &project.join(".codex/managed-skeleton.json"),
        b"unparseable retired contract",
    );
    write(&project.join(".codex/old.txt"), b"project data\n");
    let plan = build_plan(&project, &factory, SyncMode::Update).unwrap();
    assert!(!plan.gaps.is_empty());
    assert!(!plan.safe.iter().any(|a| a.operation == "delete"));
    let choices = json!({"preserve":["P:project-file:.codex/old.txt"],"delete":[]});
    let plan =
        build_plan_with_inputs(&project, &factory, SyncMode::Update, Some(&choices)).unwrap();
    let receipt = apply_plan(plan.clone(), &plan.aggregate_fingerprint, false).unwrap();
    assert_eq!(
        receipt.preserved_asset_ids,
        vec!["P:project-file:.codex/old.txt"]
    );
    assert_eq!(
        fs::read(project.join(".codex/old.txt")).unwrap(),
        b"project data\n"
    );
}

#[test]
fn standalone_project_hook_registry_is_required_preserve_during_rebuild() {
    let f = Fixture::new();
    let (factory, project) = minimal(&f);
    write(&project.join(".codex/hooks.json"), b"{\"hooks\":{}}");
    write(
        &project.join(".codex/project-hooks.json"),
        b"{\"schema_version\":1,\"hooks\":[]}",
    );
    let plan = build_plan(&project, &factory, SyncMode::Adopt).unwrap();
    assert!(
        plan.preservation_manifest["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |e| e["id"] == "R:project-hook-registry" && e["disposition"] == "required-preserve"
            )
    );
    assert!(
        !plan
            .gaps
            .iter()
            .any(|gap| gap.contains("project-hooks.json"))
    );
}

#[test]
fn concurrent_launchers_reclaim_one_dead_worker_exactly_once() {
    use bridgeforge_memory::worker::*;
    let f = Fixture::new();
    write(&f.0.join("worker.json"),json!({"schemaVersion":1,"token":"dead","pid":0,"launcherPid":0,"startedUtc":"2000-01-01T00:00:00Z"}).to_string());
    let stale_queue = f.0.join("queue/reconcile.lock");
    write(
        &stale_queue,
        json!({"pid":0,"token":"dead","utc":"2000-01-01T00:00:00Z"}).to_string(),
    );
    fs::File::options()
        .write(true)
        .open(&stale_queue)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1))
        .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let handles = (0..8)
        .map(|_| {
            let barrier = barrier.clone();
            let path = f.0.clone();
            std::thread::spawn(move || {
                barrier.wait();
                reserve_worker(&path).unwrap()
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, WorkerReservation::Acquired(_)))
            .count(),
        1
    );
    let current = read_worker_state(&f.0).unwrap().unwrap();
    for result in results {
        let state = match result {
            WorkerReservation::Acquired(s) | WorkerReservation::Reused(s) => s,
        };
        assert_eq!(state.token, current.token);
    }
    assert!(!release_worker(&f.0, "dead").unwrap());
    assert_eq!(
        read_worker_state(&f.0).unwrap().unwrap().token,
        current.token
    );
}

struct RuntimeRunner {
    old: bool,
    lock_failed: bool,
}
impl ProcessRunner for RuntimeRunner {
    fn run(&self, r: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        let (code, text) = if r.args.first().is_some_and(|a| a == "metadata") {
            (if self.lock_failed { 1 } else { 0 }, "{}".to_string())
        } else if r.program == "cargo" {
            (
                0,
                format!(
                    "cargo {} (fixture)",
                    if self.old { "1.85.0" } else { "1.88.0" }
                ),
            )
        } else if r.program == "rustc" {
            (0, "rustc 1.88.0 (fixture)".into())
        } else {
            (0,json!({"schema":1,"name":"bridgeforge","status":"ok","version":env!("CARGO_PKG_VERSION")}).to_string())
        };
        Ok(ProcessOutput {
            code,
            stdout: text.into_bytes(),
            stderr: vec![],
            timed_out: false,
        })
    }
}

#[test]
fn product_preflight_uses_product_workspace_and_enforces_toolchain_and_lock() {
    let f = Fixture::new();
    let product = f.0.join("product");
    let downstream = f.0.join("empty");
    fs::create_dir(&downstream).unwrap();
    write(&product.join("templates/hooks/Cargo.toml"), b"fixture");
    write(&product.join("templates/hooks/Cargo.lock"), b"fixture");
    write(&product.join("VERSION"), env!("CARGO_PKG_VERSION"));
    let binary = f.0.join("bridgeforge.exe");
    write(&binary, b"fixture");
    let validate = |old, lock_failed| {
        bridgeforge_core::runtime::validate_product(
            &product,
            &binary,
            &RuntimeRunner { old, lock_failed },
        )
    };
    assert!(validate(false, false).is_ok());
    assert!(validate(true, false).unwrap_err().contains("at least"));
    assert!(validate(false, true).unwrap_err().contains("lockfile"));
    assert_eq!(fs::read_dir(downstream).unwrap().count(), 0);
}

#[test]
fn lifecycle_queue_drains_events_arriving_during_sync() {
    use bridgeforge_memory::worker::*;
    let f = Fixture::new();
    mark_pending(&f.0, "SessionStart").unwrap();
    let WorkerReservation::Acquired(worker) = reserve_worker(&f.0).unwrap() else {
        panic!()
    };
    let mut calls = 0;
    let (_, restart) = drain_pending(&f.0, &worker.token, || {
        calls += 1;
        let before = fs::read(f.0.join("pending.json")).unwrap();
        if calls == 1 {
            mark_pending(&f.0, "Stop").unwrap();
        }
        clear_pending_if_unchanged(&f.0, Some(&before)).unwrap();
        Ok("noop".into())
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert!(!restart);
    assert!(read_pending(&f.0).unwrap().is_none());
    assert!(read_worker_state(&f.0).unwrap().is_none());
}

struct CredentialRunner {
    private: bool,
}
impl ProcessRunner for CredentialRunner {
    fn run(&self, r: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        let args = r
            .args
            .iter()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>();
        let (code, text) = if args.contains(&"fill".into()) {
            assert_eq!(
                r.env
                    .get(std::ffi::OsStr::new("GIT_TERMINAL_PROMPT"))
                    .unwrap(),
                "0"
            );
            (
                0,
                "protocol=https\nhost=github.com\nusername=test\npassword=fixture-secret\n"
                    .to_string(),
            )
        } else if r.program == "git" {
            (
                0,
                "https://github.com/owner/bridgeforge-codex-memories".into(),
            )
        } else if r.env.contains_key(std::ffi::OsStr::new("GH_TOKEN")) {
            assert_eq!(
                r.env.get(std::ffi::OsStr::new("GH_TOKEN")).unwrap(),
                "fixture-secret"
            );
            assert!(!args.iter().any(|a| a.contains("fixture-secret")));
            (0, if self.private { "PRIVATE" } else { "PUBLIC" }.into())
        } else {
            (1, "expired authentication".into())
        };
        Ok(ProcessOutput {
            code,
            stdout: text.into_bytes(),
            stderr: b"fixture-secret diagnostic".to_vec(),
            timed_out: false,
        })
    }
}

#[test]
fn git_credentials_verify_private_api_without_exposing_secrets() {
    use bridgeforge_memory::MemoryRemoteClient;
    let f = Fixture::new();
    let remote = "https://github.com/owner/bridgeforge-codex-memories";
    MemoryRemoteClient::new(&CredentialRunner { private: true })
        .verify_private_github_repository(&f.0, remote)
        .unwrap();
    let error = MemoryRemoteClient::new(&CredentialRunner { private: false })
        .verify_private_github_repository(&f.0, remote)
        .unwrap_err()
        .to_string();
    assert!(error.contains("private"));
    assert!(!error.contains("fixture-secret"));
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
