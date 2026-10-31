//! Opt-in paired release-binary benchmark. All fixture writes are outside the repository.
use bridgeforge_core::{ProcessRequest, ProcessRunner, SystemProcessRunner};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .into()
}

fn run(binary: &Path, root: &Path, event: &str, payload: &Value) -> (u128, Vec<u8>, Vec<u8>) {
    let mut request = ProcessRequest::new(binary, root);
    request.args = vec![event.into()];
    request.stdin = serde_json::to_vec(payload).unwrap();
    request.timeout = Duration::from_secs(60);
    request
        .env
        .insert("BRIDGEFORGE_HOOK_ROOT".into(), root.as_os_str().into());
    let started = Instant::now();
    let output = SystemProcessRunner.run(&request).unwrap();
    let elapsed = started.elapsed().as_micros();
    assert!(!output.timed_out);
    assert_eq!(
        output.code,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (elapsed, output.stdout, output.stderr)
}

#[test]
#[ignore = "requires BFC_BENCH_OLD and BFC_BENCH_NEW release executables"]
fn paired_hook_release_benchmark() {
    let old = PathBuf::from(std::env::var_os("BFC_BENCH_OLD").expect("old release binary"));
    let new = PathBuf::from(std::env::var_os("BFC_BENCH_NEW").expect("new release binary"));
    let fixture = Fixture(
        test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "hook-hardening-performance").join(
            format!(
                "bf-hook-benchmark-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
        ),
    );
    let root = &fixture.0;
    let repository = repository();
    for relative in [
        "AGENTS.md",
        "templates/AGENTS.md",
        ".codex/managed-skeleton.json",
        "doc/README.md",
        "doc/3_reference/codex-hook-signals.md",
    ] {
        let target = root.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(bridgeforge_core::source_projection::template_path(&repository, relative), target).unwrap();
    }
    let body = "normal fixture text\n".repeat(800);
    for i in 0..300 {
        let target = root.join(format!("doc/notes/{i}/note.md"));
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, &body).unwrap();
    }
    for directory in [".runtime/ignored", ".git/objects", "doc/4_archive/old"] {
        for i in 0..1000 {
            let target = root.join(directory).join(format!("{i}/leaf.txt"));
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, b"ignored").unwrap();
        }
    }
    let mut patch = String::from("*** Begin Patch\n");
    for i in 0..10 {
        patch.push_str(&format!(
            "*** Update File: doc/notes/{i}/note.md\n@@\n-old\n+normal fixture text\n"
        ));
    }
    patch.push_str("*** End Patch");
    let cases = [
        (
            "pre-tool",
            "pre-tool",
            json!({"tool_name":"PowerShell","tool_input":{"command":"Get-Content README.md"}}),
        ),
        (
            "ordinary-edit",
            "post-edit",
            json!({"tool_name":"Edit","tool_input":{"file_path":"doc/notes/0/note.md"}}),
        ),
        (
            "instruction-edit",
            "post-edit",
            json!({"tool_name":"Edit","tool_input":{"file_path":"AGENTS.md"}}),
        ),
        (
            "multi-edit",
            "post-edit",
            json!({"tool_name":"apply_patch","tool_input":{"command":patch}}),
        ),
    ];
    let mut reports = Vec::new();
    for (label, event, payload) in cases {
        let first_old = run(&old, root, event, &payload);
        let first_new = run(&new, root, event, &payload);
        assert_eq!(
            (&first_old.1, &first_old.2),
            (&first_new.1, &first_new.2),
            "unapproved output change in {label}"
        );
        let mut before = Vec::new();
        let mut after = Vec::new();
        for i in 0..22 {
            let (a, b) = if i % 2 == 0 {
                (
                    run(&old, root, event, &payload),
                    run(&new, root, event, &payload),
                )
            } else {
                let b = run(&new, root, event, &payload);
                let a = run(&old, root, event, &payload);
                (a, b)
            };
            assert_eq!((&a.1, &a.2), (&b.1, &b.2), "output drift: {label}");
            if i >= 2 {
                before.push(a.0);
                after.push(b.0);
            }
        }
        before.sort();
        after.sort();
        let summarize = |values: &Vec<u128>, first: u128| json!({"first_us":first,"median_us":(values[9]+values[10])/2,"p95_us":values[18],"max_us":values[19],"samples":20});
        reports.push(json!({"case":label,"old":summarize(&before,first_old.0),"new":summarize(&after,first_new.0),"text_files":300,"excluded_leaf_files":3000}));
    }
    let report = json!({"schema":1,"old_binary":old,"new_binary":new,"paired_alternating":true,"results":reports});
    println!("{report}");
    if let Some(output) = std::env::var_os("BFC_BENCH_REPORT") {
        fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
