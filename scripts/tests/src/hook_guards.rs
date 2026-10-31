use bridgeforge_core::{ProcessOutput, ProcessRequest, ProcessRunner, SystemProcessRunner};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SHELLS: &[&str] = &["Bash", "PowerShell", "shell_command"];
const EDITS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

#[test]
fn high_cost_stop_emits_ui_warning_without_continuation() {
    use std::io::Write;
    let fixture = Fixture::new();
    let home = fixture.home.join(".codex");
    let transcript = home.join("sessions/main.jsonl");
    fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    fs::write(
        &transcript,
        format!(
            "{}\n",
            json!({"type":"session_meta","payload":{"id":"cost-fixture","source":"cli"}})
        ),
    )
    .unwrap();
    let binary = std::env::var_os("BRIDGEFORGE_TEST_HOOK")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            repository().join(if cfg!(windows) {
                ".build/bridgeforge/bin/bridgeforge-hook.exe"
            } else {
                ".build/bridgeforge/bin/bridgeforge-hook"
            })
        });
    for round in 1..=31 {
        let turn = format!("turn-{round}");
        let timestamp = format!("2026-09-07T12:00:{round:02}Z");
        writeln!(fs::OpenOptions::new().append(true).open(&transcript).unwrap(),"{}",json!({"timestamp":timestamp,"type":"turn_context","payload":{"turn_id":turn,"model":"gpt-6-astra","effort":"high"}})).unwrap();
        let mut request = ProcessRequest::new(&binary, &fixture.root);
        request.args = vec!["stop".into()];
        request.timeout = Duration::from_secs(10);
        request.env.insert(
            "BRIDGEFORGE_HOOK_ROOT".into(),
            fixture.root.clone().into_os_string(),
        );
        request
            .env
            .insert("CODEX_HOME".into(), home.clone().into_os_string());
        request.stdin=serde_json::to_vec(&json!({"hook_event_name":"Stop","session_id":"cost-fixture","turn_id":turn,"model":"gpt-6-astra","transcript_path":transcript,"stop_hook_active":false,"last_assistant_message":"完成"})).unwrap();
        for duplicate in [false, true] {
            let output = SystemProcessRunner.run(&request).unwrap();
            assert_eq!(
                output.code,
                0,
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!output.timed_out);
            let value: Value = if output.stdout.is_empty() {
                json!({})
            } else {
                serde_json::from_slice(&output.stdout).unwrap()
            };
            assert_eq!(
                value.get("systemMessage").is_some(),
                round % 10 == 0 && !duplicate,
                "round={round}, duplicate={duplicate}: {value}"
            );
            assert!(value.get("decision").is_none());
            assert!(value.get("continue").is_none());
            if let Some(text) = value["systemMessage"].as_str() {
                assert!(text.contains(&format!(" {round} 轮")));
            }
        }
    }
    assert_eq!(
        fs::read(home.join("config.toml")).unwrap(),
        b"protected = true\n"
    );
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

struct Fixture {
    directory: PathBuf,
    root: PathBuf,
    home: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let token = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "hook-guards").join(
            format!("bridgeforge-guard-matrix-{}-{token}", std::process::id()),
        );
        let root = directory.join("project");
        let home = root.join("fake-home");
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/config.toml"), b"protected = true\n").unwrap();
        Self {
            directory,
            root,
            home,
        }
    }

    fn run(&self, event: &str, tool: &str, input: Value) -> ProcessOutput {
        let binary = std::env::var_os("BRIDGEFORGE_TEST_HOOK")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                repository().join(if cfg!(windows) {
                    ".build/bridgeforge/bin/bridgeforge-hook.exe"
                } else {
                    ".build/bridgeforge/bin/bridgeforge-hook"
                })
            });
        assert!(
            binary.is_file(),
            "build the managed Hook before testing: {}",
            binary.display()
        );
        let mut request = ProcessRequest::new(binary, &self.root);
        request.args = vec![event.into()];
        request.timeout = Duration::from_secs(10);
        // Fixtures now live inside the repository's ignored runtime tree.
        // A non-repository fixture must not discover the real parent checkout.
        request.env.insert(
            "GIT_CEILING_DIRECTORIES".into(),
            self.directory.clone().into_os_string(),
        );
        request.env_remove.extend(
            [
                "GIT_DIR",
                "GIT_WORK_TREE",
                "GIT_COMMON_DIR",
                "GIT_INDEX_FILE",
            ]
            .map(Into::into),
        );
        request.env.insert(
            "BRIDGEFORGE_HOOK_ROOT".into(),
            self.root.clone().into_os_string(),
        );
        request
            .env
            .insert("USERPROFILE".into(), self.home.clone().into_os_string());
        request
            .env
            .insert("HOME".into(), self.home.clone().into_os_string());
        request.stdin = serde_json::to_vec(&json!({"tool_name":tool,"tool_input":input})).unwrap();
        let result = SystemProcessRunner.run(&request).unwrap();
        assert!(!result.timed_out, "{event}/{tool} timed out");
        result
    }

    fn expect(&self, event: &str, tool: &str, input: Value, code: i32, diagnostic: &str) {
        let result = self.run(event, tool, input);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert_eq!(result.code, code, "{event}/{tool}: {stderr}");
        assert!(stderr.contains(diagnostic), "{event}/{tool}: {stderr}");
        assert_eq!(
            fs::read(self.home.join(".codex/config.toml")).unwrap(),
            b"protected = true\n"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn registration_covers_every_supported_tool_without_post_route_overlap() {
    for path in ["templates/hooks.json", ".codex/hooks.json"] {
        let document: Value =
            serde_json::from_slice(&fs::read(bridgeforge_core::source_projection::template_path(&repository(), path)).unwrap()).unwrap();
        let matcher = |event: &str, suffix: &str| -> BTreeSet<String> {
            let groups = document["hooks"][event].as_array().unwrap();
            let group = groups
                .iter()
                .find(|group| {
                    group["hooks"].as_array().unwrap().iter().any(|hook| {
                        hook["bridgeforgeCodexId"]
                            .as_str()
                            .unwrap_or("")
                            .ends_with(suffix)
                    })
                })
                .unwrap();
            group["matcher"]
                .as_str()
                .unwrap()
                .split('|')
                .map(str::to_owned)
                .collect()
        };
        let shells: BTreeSet<String> = SHELLS.iter().map(|name| (*name).into()).collect();
        let edits: BTreeSet<String> = EDITS
            .iter()
            .copied()
            .chain(["apply_patch"])
            .map(str::to_owned)
            .collect();
        assert_eq!(
            matcher("PreToolUse", ":pre-tool"),
            shells.union(&edits).cloned().collect(),
            "{path}"
        );
        assert_eq!(matcher("PostToolUse", ":post-shell"), shells, "{path}");
        assert_eq!(matcher("PostToolUse", ":post-edit"), edits, "{path}");
    }
}

#[test]
fn bulk_add_in_a_non_repository_is_blocked_for_every_shell_alias() {
    let fixture = Fixture::new();
    for tool in SHELLS {
        fixture.expect(
            "pre-tool",
            tool,
            json!({"command":"git add ."}),
            2,
            "[git-add-guard]",
        );
    }
}

#[test]
fn all_shell_aliases_share_both_write_guards_and_allow_read_only_commands() {
    let fixture = Fixture::new();
    for tool in SHELLS {
        fixture.expect(
            "pre-tool",
            tool,
            json!({"command":format!(
                "git -C \"{}\" add exact.txt", fixture.directory.join("outside").display()
            )}),
            2,
            "[cross-project-write-guard]",
        );
        fixture.expect(
            "pre-tool",
            tool,
            json!({"command":"Set-Content \"$HOME/.codex/config.toml\" value"}),
            2,
            "[user-config-write-guard]",
        );
        fixture.expect(
            "pre-tool",
            tool,
            json!({"command":"Get-Content \"$HOME/.codex/config.toml\""}),
            0,
            "",
        );
        fixture.expect("pre-tool", tool, json!({"command":"echo safe"}), 0, "");
        fixture.expect("post-shell", tool, json!({"command":"echo safe"}), 0, "");
        fixture.expect(
            "pre-tool",
            tool,
            json!({"command":"Set-Content local.txt 中文"}),
            2,
            "[non-ascii-shell-guard]",
        );
        fixture.expect(
            "pre-tool",
            tool,
            json!({"command":""}),
            2,
            "[hook-dispatch]",
        );
    }
}

#[test]
fn every_edit_alias_and_supported_path_field_reaches_both_guards() {
    let fixture = Fixture::new();
    for tool in EDITS {
        let mut keys = vec!["file_path", "path"];
        if *tool == "NotebookEdit" {
            keys.push("notebook_path");
        }
        for key in keys {
            fixture.expect(
                "pre-tool",
                tool,
                json!({key:fixture.directory.join("outside.ipynb")}),
                2,
                "[cross-project-write-guard]",
            );
            fixture.expect(
                "pre-tool",
                tool,
                json!({key:fixture.home.join(".codex/config.toml")}),
                2,
                "[user-config-write-guard]",
            );
            fixture.expect("pre-tool", tool, json!({key:"inside.ipynb"}), 0, "");
            fixture.expect("post-edit", tool, json!({key:"inside.ipynb"}), 0, "");
        }
    }
}

#[test]
fn missing_conflicting_and_unknown_tool_payloads_fail_closed() {
    let fixture = Fixture::new();
    for tool in EDITS {
        for input in [
            json!({}),
            json!({"file_path":""}),
            json!({"file_path":42}),
            json!({"file_path":"inside.txt","path":"../outside.txt"}),
        ] {
            fixture.expect("pre-tool", tool, input, 2, "[hook-dispatch]");
        }
    }
    fixture.expect(
        "pre-tool",
        "NotebookEdit",
        json!({
            "file_path":"inside.ipynb", "notebook_path":"../outside.ipynb"
        }),
        2,
        "conflicting target paths",
    );
    fixture.expect(
        "pre-tool",
        "unregistered_writer",
        json!({"file_path":"inside.txt"}),
        2,
        "unsupported tool",
    );
    fixture.expect(
        "post-shell",
        "NotebookEdit",
        json!({"notebook_path":"inside.ipynb"}),
        2,
        "unsupported tool",
    );
    fixture.expect(
        "post-edit",
        "shell_command",
        json!({"command":"echo safe"}),
        2,
        "unsupported tool",
    );
}

#[test]
fn apply_patch_checks_add_update_delete_and_both_move_endpoints() {
    let fixture = Fixture::new();
    for target in [
        fixture.directory.join("outside.txt"),
        fixture.home.join(".codex/config.toml"),
    ] {
        let guard = if target.starts_with(&fixture.root) {
            "[user-config-write-guard]"
        } else {
            "[cross-project-write-guard]"
        };
        for command in [
            format!(
                "*** Begin Patch\n*** Add File: {}\n+value\n*** End Patch",
                target.display()
            ),
            format!(
                "*** Begin Patch\n*** Update File: {}\n@@\n-old\n+new\n*** End Patch",
                target.display()
            ),
            format!(
                "*** Begin Patch\n*** Delete File: {}\n*** End Patch",
                target.display()
            ),
            format!(
                "*** Begin Patch\n*** Update File: inside.txt\n*** Move to: {}\n*** End Patch",
                target.display()
            ),
            format!(
                "*** Begin Patch\n*** Update File: {}\n*** Move to: inside.txt\n*** End Patch",
                target.display()
            ),
        ] {
            fixture.expect(
                "pre-tool",
                "apply_patch",
                json!({"command":command}),
                2,
                guard,
            );
        }
    }
    fixture.expect(
        "pre-tool",
        "apply_patch",
        json!({
            "command":"*** Begin Patch\n*** Add File: inside.txt\n+value\n*** End Patch"
        }),
        0,
        "",
    );
    fixture.expect(
        "pre-tool",
        "apply_patch",
        json!({"command":"not a patch"}),
        2,
        "[hook-dispatch]",
    );
}

#[test]
fn hook_topic4_cross_project_denial_explains_real_recovery_without_confirmation_loop() {
    let fixture = Fixture::new();
    let target = fixture.directory.join("outside.txt");
    let input = json!({"file_path":target});
    let first = fixture.run("pre-tool", "Edit", input.clone());
    let second = fixture.run("pre-tool", "Edit", input);
    assert_eq!(first.code, 2);
    assert_eq!(second.code, 2);
    assert_eq!(first.stderr, second.stderr);
    let message = String::from_utf8_lossy(&first.stderr);
    assert!(message.contains("用户确认不会改变结果"));
    assert!(message.contains("目标项目的受管任务"));
    assert!(!message.contains("再在保留该确认的上下文中重试"));
    assert!(!target.exists());
}

#[test]
fn hook_topic4_batch_probe() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root.join("doc")).unwrap();
    for count in [1, 10] {
        let mut patch = String::from("*** Begin Patch\n");
        for index in 0..count {
            let path = format!("doc/file-{index}.md");
            fs::write(fixture.root.join(&path), "valid\n").unwrap();
            patch.push_str(&format!("*** Update File: {path}\n@@\n-old\n+valid\n"));
        }
        patch.push_str("*** End Patch");
        let mut elapsed = Vec::new();
        let mut diagnostics = 0;
        for _ in 0..3 {
            let started = std::time::Instant::now();
            let result = fixture.run("post-edit", "apply_patch", json!({"command":patch}));
            elapsed.push(started.elapsed().as_micros());
            assert_eq!(result.code, 0);
            diagnostics = String::from_utf8_lossy(&result.stderr)
                .matches("cannot read AGENTS.md")
                .count();
            assert_eq!(diagnostics, 0);
        }
        elapsed.sort();
        println!(
            "hook-batch files={count} instruction_diagnostics={diagnostics} median_us={} samples=3",
            elapsed[1]
        );
    }
}

#[test]
fn hook_topic4_native_patch_reports_smell_once_and_map_failure_without_rollback() {
    let fixture = Fixture::new();
    let source = fixture.root.join("x.py");
    fs::write(&source, "except Exception:\n    pass\n").unwrap();
    let map_root = fixture.root.join(".runtime/bridgeforge-codex");
    fs::create_dir_all(map_root.join("project-map-dirty")).unwrap();
    let result = fixture.run("post-edit", "apply_patch", json!({"command":"*** Begin Patch\n*** Add File: x.py\n+except Exception:\n+    pass\n*** Add File: y.py\n+print('ok')\n*** End Patch"}));
    assert_eq!(result.code, 1);
    let err = String::from_utf8_lossy(&result.stderr);
    assert!(err.contains("project-map dirty marker"));
    assert_eq!(err.matches("cannot read AGENTS.md").count(), 0);
    let out: Value = serde_json::from_slice(&result.stdout).unwrap();
    let context = out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("直接检索原文件"));
    assert!(context.contains("[fallback-smell] x.py"));
    assert!(!context.contains("[fallback-smell] y.py"));
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "except Exception:\n    pass\n"
    );
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
