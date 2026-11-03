use super::*;

#[test]
fn legacy_test_receipts_relocate_exact_known_files_and_preserve_unknowns() {
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "artifact-migration")
        .join(format!("{}", std::process::id()));
    fs::create_dir_all(root.join(".runtime/test_receipts/payload_samples")).unwrap();
    let record = root.join(".runtime/test_receipts/receipts.jsonl");
    let sample = root.join(".runtime/test_receipts/payload_samples/2026-10-30T123456.json");
    let bytes=br#"{"cmd":"cargo test","kind":"test","ts":"2026-10-30T12:34:56","sha1":"012345abcdef","exit_code":0}"#;
    fs::write(&record, bytes).unwrap();
    fs::write(&sample, b"{\"tool_name\":\"exec_command\"}").unwrap();
    let found = legacy_test_outputs(&root).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[&record], bytes);
    assert_eq!(
        relocated_test_output(&root, &record).unwrap(),
        root.join(".runtime/tests/legacy-test-receipts/receipts.jsonl")
    );
    let unknown = root.join(".runtime/test_receipts/user-notes.txt");
    fs::write(&unknown, b"keep").unwrap();
    assert_eq!(legacy_test_outputs(&root).unwrap(), found);
    assert_eq!(fs::read(&unknown).unwrap(), b"keep");
    assert_eq!(fs::read(&record).unwrap(), bytes);
    fs::write(&record, b"{\"unknown\":true}").unwrap();
    assert!(
        legacy_test_outputs(&root)
            .unwrap_err()
            .contains("unknown legacy test receipt")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_test_receipts_preserve_project_reports_without_parsing_or_traversing_them() {
    let root = runtime_fixture("project-reports");
    let directory = root.join(".runtime/test_receipts");
    fs::create_dir_all(directory.join("payload_samples")).unwrap();
    fs::create_dir_all(directory.join("project_reports/payload_samples")).unwrap();
    let reports = [
        "artifact-layout-20261023-r1.xml",
        "artifact-layout-20261023-startup-retry.xml",
        "artifact-layout-20261023-startup-short.xml",
        "db_failure_forensics_20261019.json",
        "statistics_publication_20261019.json",
        "statistics_reconnect_20261019.json",
        "statistics_root_cause_20261019.json",
        "payload_samples/project-notes.json",
        "project_reports/receipts.jsonl",
        "project_reports/payload_samples/2026-10-30T123456.json",
    ];
    for path in reports {
        fs::write(directory.join(path), b"opaque project data; not JSON").unwrap();
    }
    let sample = directory.join("payload_samples/2026-10-30T123456.json");
    fs::write(&sample, br#"{"tool_name":"exec_command"}"#).unwrap();
    let record = directory.join("receipts.jsonl");
    fs::write(
        &record,
        br#"{"cmd":"cargo test","kind":"test","ts":"2026-10-30","sha1":"012345abcdef"}"#,
    )
    .unwrap();
    let before = legacy_test_outputs(&root).unwrap();
    assert_eq!(before.len(), 2);
    assert!(before.contains_key(&sample));
    assert!(before.contains_key(&record));
    fs::remove_file(sample).unwrap();
    fs::remove_file(record).unwrap();
    assert!(legacy_test_outputs(&root).unwrap().is_empty());
    for path in reports {
        assert_eq!(
            fs::read(directory.join(path)).unwrap(),
            b"opaque project data; not JSON"
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_test_receipts_reject_invalid_managed_samples_and_root_types() {
    let root = runtime_fixture("test-output-guards");
    let directory = root.join(".runtime/test_receipts");
    fs::create_dir_all(directory.join("payload_samples")).unwrap();
    let sample = directory.join("payload_samples/2026-10-30T123456.json");
    fs::write(&sample, b"invalid managed JSON").unwrap();
    assert!(legacy_test_outputs(&root).is_err());
    assert_eq!(fs::read(&sample).unwrap(), b"invalid managed JSON");
    fs::remove_file(&sample).unwrap();
    fs::create_dir(&sample).unwrap();
    assert!(
        legacy_test_outputs(&root)
            .unwrap_err()
            .contains("not a regular file")
    );
    fs::remove_dir(&sample).unwrap();
    fs::create_dir(directory.join("receipts.jsonl")).unwrap();
    assert!(
        legacy_test_outputs(&root)
            .unwrap_err()
            .contains("not a regular file")
    );
    fs::remove_dir(directory.join("receipts.jsonl")).unwrap();
    fs::remove_dir(directory.join("payload_samples")).unwrap();
    fs::remove_dir(&directory).unwrap();
    fs::write(&directory, b"not a directory").unwrap();
    assert!(
        legacy_test_outputs(&root)
            .unwrap_err()
            .contains("not a directory")
    );
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn legacy_test_receipts_reject_visible_junctions_and_keep_report_directories_opaque() {
    use std::os::windows::process::CommandExt;
    let root = runtime_fixture("test-output-junction");
    let directory = root.join(".runtime/test_receipts");
    let outside = root.join("external");
    fs::create_dir_all(directory.join("project_reports")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), b"external data").unwrap();
    let hidden = directory.join("project_reports/link");
    let visible = directory.join("report-link");
    for path in [&hidden, &visible] {
        let output = std::process::Command::new("cmd.exe")
            .creation_flags(0x08000000)
            .args(["/c", "mklink", "/J"])
            .arg(path.to_string_lossy().replace('/', "\\"))
            .arg(outside.to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction {} -> {}: stdout={} stderr={}",
            path.display(),
            outside.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if path == &hidden {
            assert!(legacy_test_outputs(&root).unwrap().is_empty());
        } else {
            assert!(
                legacy_test_outputs(&root)
                    .unwrap_err()
                    .contains("contains a link")
            );
        }
    }
    assert_eq!(
        fs::read(outside.join("keep.txt")).unwrap(),
        b"external data"
    );
    fs::remove_dir(visible).unwrap();
    fs::remove_dir(hidden).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;

fn runtime_fixture(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "legacy-runtime-preservation")
        .join(format!("{}-{nonce}-{label}", std::process::id()));
    let directory = root.join(".codex/bin");
    fs::create_dir_all(&directory).unwrap();
    let mut receipt = serde_json::json!({});
    receipt["schema_version"] = serde_json::json!(2);
    receipt["generated_asset_id"] = serde_json::json!("codex.hooks");
    receipt["platform"] = serde_json::json!(crate::build_receipt::platform_key().unwrap());
    receipt["binary_sha256"] = serde_json::json!(format!("sha256:{:x}", Sha256::digest(b"public")));
    fs::write(
        directory.join(format!("bridgeforge-hook{}", std::env::consts::EXE_SUFFIX)),
        b"public",
    )
    .unwrap();
    fs::write(
        directory.join("build-receipt-hook.json"),
        serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
    fs::write(directory.join(".gitignore"), b"*\r\n!.gitignore\r\n").unwrap();
    root
}

#[test]
fn legacy_runtime_fingerprint_retirement_preserves_all_existing_files() {
    let root = runtime_fixture("retired-fingerprints");
    let directory = root.join(".codex/bin");
    fs::write(directory.join("private.txt"), b"keep private").unwrap();
    let before = fs::read_dir(&directory).unwrap().map(|entry| {
        let path = entry.unwrap().path(); let bytes = fs::read(&path).unwrap(); (path, bytes)
    }).collect::<BTreeMap<_,_>>();
    assert!(legacy_runtime(&root).unwrap_err().contains("fingerprint migration is retired"));
    for (path,bytes) in before { assert_eq!(fs::read(path).unwrap(),bytes); }
    fs::remove_dir_all(root).unwrap();
}
