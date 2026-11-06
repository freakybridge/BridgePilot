#[path = "../support/factory_root.rs"]
mod factory_repository;

use super::*;
#[cfg(windows)]
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn dedicated_memory_retarget_preserves_enablement_external_hooks_and_drift() {
    let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
        .join(format!("bf-hook-scope-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&home).unwrap();
    let before_runtime = home.join("bin/bridgeforge.exe");
    let dedicated = home.join(".bridgeforge-codex/native-memory-sync/bin/bridgeforge-memory-hook.exe");
    let hook_path = home.join("hooks.json");
    let settings = home.join("config.toml");
    fs::write(&settings, b"[features]\nmemories = false\n").unwrap();
    assert!(!retarget_user_hooks(&home, &before_runtime, &dedicated).unwrap());
    assert!(!hook_path.exists());
    let mut document = expected_document(&before_runtime, &home);
    let external = json!([{"hooks":[{"type":"command", "command":"personal-hook"}]}]);
    document["hooks"]["UserPromptSubmit"] = external.clone();
    document["hooks"]["SessionStart"].as_array_mut().unwrap().push(external[0].clone());
    fs::write(&hook_path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(retarget_user_hooks(&home, &before_runtime, &dedicated).unwrap());
    assert!(user_hooks_healthy(&home, &dedicated));
    assert!(!user_hooks_healthy(&home, &before_runtime));
    assert!(!retarget_user_hooks(&home, &before_runtime, &dedicated).unwrap());
    let migrated = load_document(&fs::read(&hook_path).unwrap(), "test").unwrap();
    assert_eq!(migrated["hooks"]["UserPromptSubmit"], external);
    assert_eq!(migrated["hooks"]["SessionStart"][1], external[0]);
    assert_eq!(fs::read(&settings).unwrap(), b"[features]\nmemories = false\n");
    let mut missing = document.clone();
    let repeated = missing["hooks"]["SessionStart"][0].clone();
    missing["hooks"]["SessionStart"].as_array_mut().unwrap().push(repeated);
    missing["hooks"].as_object_mut().unwrap().remove("SessionEnd");
    let duplicate = serde_json::to_vec(&missing).unwrap();
    fs::write(&hook_path, &duplicate).unwrap();
    assert!(retarget_user_hooks(&home, &before_runtime, &dedicated).is_err());
    assert_eq!(fs::read(&hook_path).unwrap(), duplicate);
    document["hooks"]["SessionStart"][0]["hooks"][0]["command"] = json!("unexpected");
    let drift = serde_json::to_vec(&document).unwrap();
    fs::write(&hook_path, &drift).unwrap();
    assert!(retarget_user_hooks(&home, &before_runtime, &dedicated).is_err());
    assert_eq!(fs::read(&hook_path).unwrap(), drift);
    fs::remove_dir_all(&home).unwrap();
}

#[cfg(windows)]
fn python_hook_fixture(home: &Path) -> Value {
    let parent = home.parent().unwrap().to_string_lossy();
    let mut document = json!({"description":"retained", "hooks":{}});
    for event in ["SessionStart", "Stop", "SessionEnd"] {
        let mut handler: Value = serde_json::from_str(r#"{
          "type":"command",
          "command":"root=\"$(git rev-parse --show-toplevel)\" && \"$root/.venv/Scripts/python.exe\" 'C:\\Users\\test\\.bridgeforge-codex\\scripts\\codex_memory_sync.py' hook-run --event SessionStart --project-root \"$root\"",
          "commandWindows":"powershell.exe -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File \"C:\\Users\\test\\.bridgeforge-codex\\scripts\\codex_memory_sync_hook.ps1\" SessionStart",
          "bridgeforgeCodexId":"bridgeforge-codex.native-memory-sync.v1:SessionStart",
          "timeout":120
        }"#).unwrap();
        for key in ["command", "commandWindows", "bridgeforgeCodexId"] {
            handler[key] = Value::String(
                handler[key]
                    .as_str()
                    .unwrap()
                    .replace(r"C:\Users\test", &parent)
                    .replace("SessionStart", event),
            );
        }
        if event == "Stop" {
            handler["async"] = json!(true);
        }
        if event == "SessionEnd" {
            handler["timeout"] = json!(3);
        }
        document["hooks"][event] = json!([{"hooks":[handler]}]);
    }
    document["hooks"]["UserPromptSubmit"] = json!([{"hooks":[{
        "type":"command", "command":"external-keep", "timeout":7
    }]}]);
    document
}

#[cfg(windows)]
#[test]
fn exact_official_python_handlers_migrate_to_rust_and_preserve_external_hooks() {
    let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
        .join(format!(
            "bf-python-hook-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .join(".codex");
    fs::create_dir_all(&home).unwrap();
    let before = python_hook_fixture(&home);
    let binary = home.join("bin/bridgeforge.exe");
    fs::write(home.join("hooks.json"), before.to_string()).unwrap();
    assert!(!user_hooks_healthy(&home, &binary));
    assert!(merge_user_hooks(&home, &binary).unwrap());
    assert!(user_hooks_healthy(&home, &binary));
    let after = load_document(&fs::read(home.join("hooks.json")).unwrap(), "test").unwrap();
    assert_eq!(after["description"], before["description"]);
    assert_eq!(
        after["hooks"]["UserPromptSubmit"],
        before["hooks"]["UserPromptSubmit"]
    );
    assert!(!after.to_string().contains("python.exe"));
    assert!(!merge_user_hooks(&home, &binary).unwrap());
    fs::remove_dir_all(home.parent().unwrap()).unwrap();
}

#[cfg(windows)]
#[test]
fn partial_official_python_hooks_migrate_and_fill_missing_events() {
    let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
        .join(format!(
            "bf-python-partial-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .join(".codex");
    fs::create_dir_all(&home).unwrap();
    let mut before = python_hook_fixture(&home);
    before["hooks"].as_object_mut().unwrap().remove("Stop");
    before["hooks"]
        .as_object_mut()
        .unwrap()
        .remove("SessionEnd");
    let binary = home.join("bin/bridgeforge.exe");
    fs::write(home.join("hooks.json"), before.to_string()).unwrap();
    assert!(merge_user_hooks(&home, &binary).unwrap());
    assert!(user_hooks_healthy(&home, &binary));
    let after = load_document(&fs::read(home.join("hooks.json")).unwrap(), "test").unwrap();
    assert_eq!(after["description"], before["description"]);
    assert_eq!(
        after["hooks"]["UserPromptSubmit"],
        before["hooks"]["UserPromptSubmit"]
    );
    for event in HOOK_EVENTS {
        assert_eq!(after["hooks"][event].as_array().unwrap().len(), 1);
    }
    assert!(!merge_user_hooks(&home, &binary).unwrap());
    fs::remove_dir_all(home.parent().unwrap()).unwrap();
}

#[cfg(windows)]
#[test]
fn modified_python_handler_blocks_entire_migration_without_writes() {
    let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
        .join(format!(
            "bf-python-drift-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .join(".codex");
    fs::create_dir_all(&home).unwrap();
    for change in [
        "command",
        "commandWindows",
        "timeout",
        "matcher",
        "identity",
    ] {
        let mut document = python_hook_fixture(&home);
        let group = &mut document["hooks"]["Stop"][0];
        match change {
            "matcher" => group["matcher"] = json!("custom"),
            "timeout" => group["hooks"][0]["timeout"] = json!(121),
            "identity" => {
                group["hooks"][0]["bridgeforgeCodexId"] =
                    json!("bridgeforge-codex.native-memory-sync.v1:Other")
            }
            key => group["hooks"][0][key] = json!("custom command"),
        }
        let payload = document.to_string();
        fs::write(home.join("hooks.json"), &payload).unwrap();
        assert!(
            merge_user_hooks(&home, &home.join("bin/bridgeforge.exe")).is_err(),
            "{change}"
        );
        assert_eq!(
            fs::read_to_string(home.join("hooks.json")).unwrap(),
            payload
        );
    }
    fs::remove_dir_all(home.parent().unwrap()).unwrap();
}

#[test]
fn expected_handlers_use_rust_binary_only() {
    let value = expected_document(
        Path::new("/tools/bridgeforge"),
        Path::new("/home/me/.codex"),
    );
    let text = value.to_string();
    assert!(text.contains("bridgeforge"));
    assert!(!text.contains("python"));
    assert!(!text.contains(".venv"));
    let hooks = value["hooks"].as_object().unwrap();
    assert_eq!(hooks.len(), 3);
    for (event, timeout, asynchronous) in [
        ("SessionStart", 120, None),
        ("Stop", 120, Some(true)),
        ("SessionEnd", 3, None),
    ] {
        let groups = hooks[event].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        let handlers = groups[0]["hooks"].as_array().unwrap();
        assert_eq!(handlers.len(), 1);
        let handler = &handlers[0];
        assert_eq!(handler["type"], "command");
        assert_eq!(handler["timeout"], timeout);
        assert_eq!(handler.get("async").and_then(Value::as_bool), asynchronous);
        assert!(
            handler["command"]
                .as_str()
                .unwrap()
                .contains(&format!("memory-sync hook-run --event {event}"))
        );
        assert!(
            handler["commandWindows"]
                .as_str()
                .unwrap()
                .contains(&format!("memory-sync hook-run --event {event}"))
        );
    }
}

#[cfg(windows)]
#[test]
fn powershell_host_preserves_native_arguments_stdin_streams_and_exit_code() {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let folder =
        test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config").join(format!(
            "bf shell ' dollar$ tick` {}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&folder).unwrap();
    let build_folder = test_artifacts::build_root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
        .join(folder.file_name().unwrap());
    fs::create_dir_all(&build_folder).unwrap();
    let binary = build_folder.join("probe.exe");
    let root = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"));
    let built = Command::new("rustc")
        .arg(root.join("scripts/tests/fixtures/native_memory_shell_probe.rs"))
        .arg("-o")
        .arg(&binary)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let home = folder.join(".codex");
    let command =
        expected_document(&binary, &home)["hooks"]["SessionStart"][0]["hooks"][0]["commandWindows"]
            .as_str()
            .unwrap()
            .to_owned();
    let input = "{\"text\":\"中文\\nline \\\"quoted\\\"\"}\n".as_bytes();
    for shell in ["powershell.exe", "pwsh.exe"] {
        for code in [0, 1, 2] {
            let mut child = Command::new(shell)
                .args(["-NoProfile", "-NonInteractive", "-Command", &command])
                .env("BF_PROBE_HOME", command_path(&home))
                .env("BF_PROBE_EXIT", code.to_string())
                .creation_flags(0x08000000)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(input).unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(code),
                "{shell}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, input, "{shell}");
            assert_eq!(output.stderr, b"probe-stderr\n", "{shell}");
        }
        let missing = expected_document(&folder.join("missing.exe"), &home)["hooks"]["SessionStart"][0]["hooks"][0]["commandWindows"].as_str().unwrap().to_owned();
        let output = Command::new(shell)
            .args(["-NoProfile", "-NonInteractive", "-Command", &missing])
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "{shell}: missing executable must fail"
        );
        assert!(!output.stderr.is_empty());
    }
    fs::remove_dir_all(build_folder).unwrap();
    fs::remove_dir_all(folder).unwrap();
}

#[cfg(windows)]
#[test]
fn exact_legacy_rust_hooks_migrate_but_command_suffixes_block() {
    let home =
        test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config").join(format!(
            "bf-rust-legacy-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&home).unwrap();
    let binary = home.join("bin/bridgeforge.exe");
    let mut legacy = expected_document(&binary, &home);
    for event in HOOK_EVENTS {
        legacy["hooks"][event][0]["hooks"][0]["commandWindows"] = format!(
            "{} memory-sync hook-run --event {event} --codex-home {}",
            windows_quote(&command_path(&binary)),
            windows_quote(&command_path(&home))
        )
        .into();
    }
    let mut drift = legacy.clone();
    let handler = &mut drift["hooks"]["Stop"][0]["hooks"][0];
    handler["commandWindows"] =
        format!("{}; other.exe", handler["commandWindows"].as_str().unwrap()).into();
    fs::write(home.join("hooks.json"), drift.to_string()).unwrap();
    assert!(merge_user_hooks(&home, &binary).is_err());
    assert_eq!(
        fs::read_to_string(home.join("hooks.json")).unwrap(),
        drift.to_string()
    );
    for command_backslash in [false, true] {
        for windows_backslash in [false, true] {
            let mut candidate = legacy.clone();
            for event in HOOK_EVENTS {
                for (key, backslash) in [
                    ("command", command_backslash),
                    ("commandWindows", windows_backslash),
                ] {
                    if backslash {
                        candidate["hooks"][event][0]["hooks"][0][key] = candidate["hooks"][event]
                            [0]["hooks"][0][key]
                            .as_str()
                            .unwrap()
                            .replace('/', "\\")
                            .into();
                    }
                }
            }
            fs::write(home.join("hooks.json"), candidate.to_string()).unwrap();
            assert!(!user_hooks_healthy(&home, &binary));
            assert!(merge_user_hooks(&home, &binary).unwrap());
            assert!(user_hooks_healthy(&home, &binary));
            assert!(!merge_user_hooks(&home, &binary).unwrap());
        }
    }
    fs::remove_dir_all(home).unwrap();
}

#[cfg(windows)]
#[test]
fn native_memory_hook_generation_uses_stable_windows_path_spelling() {
    assert_eq!(
        expected_document(
            Path::new(r"C:\Program Files\BridgeForge\bridgeforge.exe"),
            Path::new(r"C:\Users\test\.codex"),
        ),
        expected_document(
            Path::new("C:/Program Files/BridgeForge/bridgeforge.exe"),
            Path::new("C:/Users/test/.codex"),
        )
    );
}

#[cfg(windows)]
#[test]
fn native_memory_hook_path_aliases_are_healthy_and_repair_is_noop() {
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
        .join(format!("memory hook aliases {token}"));
    let forward_home = PathBuf::from(home.to_string_lossy().replace('\\', "/"));
    let backward_home = PathBuf::from(home.to_string_lossy().replace('/', "\\"));
    let binary = Path::new(r"C:\Program Files\BridgeForge\bridgeforge.exe");
    fs::create_dir_all(&home).unwrap();
    for style in ["forward", "backward", "mixed"] {
        let mut document = expected_document(binary, &home);
        for event in HOOK_EVENTS {
            for key in ["command", "commandWindows"] {
                let value = document["hooks"][event][0]["hooks"][0][key]
                    .as_str()
                    .unwrap();
                let value = match style {
                    "backward" => value.replace('/', "\\"),
                    "mixed" => value.replacen('/', "\\", 1),
                    _ => value.to_string(),
                };
                document["hooks"][event][0]["hooks"][0][key] = Value::String(value);
            }
        }
        document["hooks"]["UserPromptSubmit"] = json!([{
            "hooks": [{"type": "command", "command": "external /keep\\literal", "timeout": 7}]
        }]);
        let before = super::super::ownership::render_document(&document).unwrap();
        fs::write(home.join("hooks.json"), &before).unwrap();
        for spelling in [&forward_home, &backward_home] {
            assert!(user_hooks_healthy(spelling, binary), "{style}");
            assert!(!merge_user_hooks(spelling, binary).unwrap(), "{style}");
            assert_eq!(fs::read(home.join("hooks.json")).unwrap(), before);
        }
    }
    fs::remove_dir_all(home).unwrap();
}

#[cfg(windows)]
#[test]
fn native_memory_path_aliases_do_not_hide_real_hook_changes() {
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
        .join(format!("memory-hook-drift-{token}"));
    let binary = Path::new(r"C:\tools\bridgeforge.exe");
    fs::create_dir_all(&home).unwrap();
    for change in [
        "binary", "home", "suffix", "event", "timeout", "async", "type", "id", "extra",
    ] {
        let mut document = expected_document(binary, &home);
        let handler = &mut document["hooks"]["SessionEnd"][0]["hooks"][0];
        let command = handler["commandWindows"]
            .as_str()
            .unwrap()
            .replace('/', "\\");
        handler["commandWindows"] = Value::String(command.clone());
        match change {
            "binary" => {
                handler["commandWindows"] = command.replace("bridgeforge.exe", "other.exe").into()
            }
            "home" => {
                handler["commandWindows"] = command
                    .replace("memory-hook-drift-", "different-home-")
                    .into()
            }
            "suffix" => handler["commandWindows"] = format!("{command} && other.exe").into(),
            "event" => {
                handler["commandWindows"] =
                    command.replace("--event SessionEnd", "--event Stop").into()
            }
            "timeout" => handler["timeout"] = 120.into(),
            "async" => handler["async"] = true.into(),
            "type" => handler["type"] = "prompt".into(),
            "id" => handler[MANAGED_ID_KEY] = format!("{HOOK_ID}:unknown").into(),
            "extra" => handler["unrecognized"] = true.into(),
            _ => unreachable!(),
        }
        let before = super::super::ownership::render_document(&document).unwrap();
        fs::write(home.join("hooks.json"), &before).unwrap();
        assert!(!user_hooks_healthy(&home, binary), "{change}");
        assert!(merge_user_hooks(&home, binary).is_err(), "{change}");
        assert_eq!(
            fs::read(home.join("hooks.json")).unwrap(),
            before,
            "{change}"
        );
    }
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn native_memory_path_comparison_preserves_quoting_and_non_drive_paths() {
    let binary = Path::new("C:/Program Files/a'b/bridgeforge.exe");
    let home = Path::new("C:/Users/test/.codex");
    let expected = expected_document(binary, home);
    let command = expected["hooks"]["SessionEnd"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(hook_command_matches(
        command,
        binary,
        home,
        "SessionEnd",
        shell_quote
    ));
    assert!(!hook_command_matches(
        &command.replace("'\\''", "'/''"),
        binary,
        home,
        "SessionEnd",
        shell_quote
    ));
    for path in [
        "relative/path",
        "/home/test/.codex",
        "//server/share/.codex",
        "//?/C:/Users/test/.codex",
    ] {
        let quoted = shell_quote(path);
        assert!(!quoted_path_matches(
            &quoted.replace('/', "\\"),
            Path::new(path),
            shell_quote
        ));
    }
}

#[cfg(windows)]
#[test]
fn native_memory_powershell_single_quotes_allow_trailing_path_separator() {
    for path in ["C:/Users/test/.codex/", "C:/Users/test/.codex//", "C:/"] {
        let quoted = windows_quote(path);
        assert!(quoted_path_matches(&quoted, Path::new(path), windows_quote));
        assert!(!quoted_path_matches(
            &quoted.replace('/', "\\"),
            Path::new(path),
            windows_quote,
        ));
    }
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
        .join(format!("memory-hook-trailing-{token}"));
    let spelling = PathBuf::from(format!("{}/", home.to_string_lossy().replace('\\', "/")));
    let binary = Path::new("C:/tools/bridgeforge.exe");
    fs::create_dir_all(&home).unwrap();
    let mut document = expected_document(binary, &spelling);
    let handler = &mut document["hooks"]["SessionEnd"][0]["hooks"][0];
    handler["commandWindows"] = handler["commandWindows"]
        .as_str()
        .unwrap()
        .replace('/', "\\")
        .into();
    let before = super::super::ownership::render_document(&document).unwrap();
    fs::write(home.join("hooks.json"), &before).unwrap();
    assert!(user_hooks_healthy(&spelling, binary));
    assert!(!merge_user_hooks(&spelling, binary).unwrap());
    assert_eq!(fs::read(home.join("hooks.json")).unwrap(), before);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn user_hooks_writer_refuses_an_active_shared_lock() {
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
        .join(format!("bridgeforge-user-hooks-lock-{token}"));
    fs::create_dir_all(home.join(".bridgeforge-codex")).unwrap();
    let held = UserHooksLock::acquire(&home).unwrap();
    let error = merge_user_hooks(&home, Path::new("bridgeforge")).unwrap_err();
    assert!(error.to_string().contains("update is running"));
    drop(held);
    assert!(merge_user_hooks(&home, Path::new("bridgeforge")).is_ok());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn annotated_quoted_and_inline_tables_are_edited_without_duplication() {
    for input in [
        "# keep me\n[features] # user comment\nmemories = false # preference\n",
        "[\"features\"] # quoted table\n\"memories\" = false # preference\n",
        "features = { memories = false } # inline table\n",
        "features.memories = false # dotted key\n",
    ] {
        let token = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
            .join(format!("bridgeforge-toml-{token}"));
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join("config.toml"), input).unwrap();
        assert!(enable_memories(&home, true).unwrap());
        let updated = fs::read_to_string(home.join("config.toml")).unwrap();
        let parsed = parse_config(&updated).unwrap();
        assert_eq!(parsed["features"]["memories"].as_bool(), Some(true));
        assert!(memories_enabled(&home));
        for comment in [
            "keep me",
            "user comment",
            "preference",
            "quoted table",
            "inline table",
            "dotted key",
        ] {
            if input.contains(comment) {
                assert!(updated.contains(comment), "lost comment: {comment}");
            }
        }
        assert!(!enable_memories(&home, true).unwrap());
        assert_eq!(
            fs::read_to_string(home.join("config.toml")).unwrap(),
            updated
        );
        fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn invalid_or_wrong_shaped_toml_is_rejected_without_writing() {
    for input in [
        "[features]\n[features]\n",
        "features = 42\n",
        "unclosed = [\n",
    ] {
        let token = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "memory-user-config")
            .join(format!("bridgeforge-toml-invalid-{token}"));
        fs::create_dir_all(&home).unwrap();
        let config = home.join("config.toml");
        fs::write(&config, input).unwrap();
        assert!(enable_memories(&home, true).is_err());
        assert_eq!(fs::read_to_string(&config).unwrap(), input);
        assert!(!memories_enabled(&home));
        assert!(!home.join("config.toml.tmp").exists());
        fs::remove_dir_all(home).unwrap();
    }
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
