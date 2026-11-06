use chrono::Local;
use regex::Regex;
use serde_json::{Value, json};
use sha1::{Digest as Sha1Digest, Sha1};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::StepResult;
use crate::util::{
    atomic_write, json_string, normalize_path, path_is_inside, relative_display, repo_root,
    tool_input,
};

// Factory-test counters only; no production telemetry or persisted state.
#[cfg(all(test, bridgeforge_factory_tests))]
pub static ENCODING_FILES_READ: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(all(test, bridgeforge_factory_tests))]
pub static INSTRUCTION_SCANS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(all(test, bridgeforge_factory_tests))]
pub static INSTRUCTION_ENTRIES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
pub fn encoding(payloads: &[Value]) -> StepResult {
    let root = repo_root();
    let mut edited = BTreeSet::new();
    for payload in payloads {
        if let Some(input) = tool_input(payload) {
            for key in ["file_path", "path"] {
                let raw = json_string(input, key);
                if raw.is_empty() {
                    continue;
                }
                let path = normalize_path(&root, &raw);
                if path_is_inside(&root, &path)
                    && path.is_file()
                    && bridgeforge_checks::encoding::is_text(&path)
                {
                    edited.insert(path);
                }
            }
        }
    }
    let paths = edited.into_iter().collect::<Vec<PathBuf>>();
    let report =
        match bridgeforge_checks::encoding::inspect_paths_with_observer(&root, &paths, |_path| {
            #[cfg(all(test, bridgeforge_factory_tests))]
            ENCODING_FILES_READ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }) {
            Ok(report) => report,
            Err(error) => return StepResult::failed("encoding", error),
        };
    let bom = report
        .bom_paths
        .iter()
        .map(|path| format!("[encoding]   {path}"))
        .collect::<Vec<_>>();
    if !bom.is_empty() {
        return StepResult::blocked(format!(
            "[encoding] hard gate: UTF-8 BOM is forbidden\n{}\n[encoding] Fix: save these files as UTF-8 without BOM.\n",
            bom.join("\n")
        ));
    }
    if report.suspicious.is_empty() {
        StepResult::ok()
    } else {
        StepResult {
            code: 0,
            stdout: String::new(),
            stderr: format!(
                "[encoding] suspicious replacement text detected\n{}\n[encoding] Stop and confirm the original text; automatic repair is unsafe.\n",
                report
                    .suspicious
                    .into_iter()
                    .map(|item| format!("[encoding]   {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        }
    }
}

pub fn artifact_layout(payload: &Value) -> StepResult {
    let Some(input) = tool_input(payload) else {
        return StepResult::ok();
    };
    let raw = json_string(input, "file_path");
    let root = normalize_path(&repo_root(), ".");
    let path = normalize_path(&root, &raw);
    if !crate::util::path_is_inside(&root, &path) {
        return StepResult::ok();
    }
    let relative = crate::util::relative_display(&root, &path);
    // Deleted patch targets have no final file to inspect.
    if !path.exists() {
        return StepResult::ok();
    }
    let report = bridgeforge_checks::artifact_layout::edited(&root, &relative);
    if report.advisories.is_empty() {
        return StepResult::ok();
    }
    StepResult::output(
        report
            .advisories
            .iter()
            .map(|f| {
                format!(
                    "[artifact-layout] WARNING {}: {}: {}",
                    f.code, f.path, f.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub fn requirements(payload: &Value) -> StepResult {
    let Some(input) = tool_input(payload) else {
        return StepResult::ok();
    };
    let raw = json_string(input, "file_path");
    let path = normalize_path(&repo_root(), &raw);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if !name.starts_with("requirements")
        || path.extension().and_then(|value| value.to_str()) != Some("txt")
        || !path.is_file()
    {
        return StepResult::ok();
    }
    let Ok(bytes) = fs::read(&path) else {
        return StepResult::ok();
    };
    let mut violations = Vec::new();
    if std::str::from_utf8(&bytes).is_err() {
        violations.push("文件不是合法 UTF-8 — 清理非法字节".to_string());
    }
    let text = String::from_utf8_lossy(&bytes);
    let absolute = Regex::new(r"(?i)@\s*file://").unwrap();
    for (index, line) in text.lines().enumerate() {
        if absolute.is_match(line) {
            violations.push(format!("L{} 绝对路径 URL（`@ file://`）— 换机即 fail；改用 libs/ + 顶部 `--find-links libs/` + `name==version`", index + 1));
        }
        let non_ascii: Vec<char> = line
            .chars()
            .filter(|value| !value.is_ascii())
            .take(5)
            .collect();
        if !non_ascii.is_empty() {
            violations.push(format!("L{} 含非 ASCII 字符 {:?} — pip 在 Windows 按 GBK 解码会报错；注释只用英文，中文挪 README", index + 1, non_ascii));
        }
    }
    if violations.is_empty() {
        return StepResult::ok();
    }
    StepResult::output(format!(
        "[requirements-check] {name} 违反可移植性红线（AGENTS.md §1.3；codex-project-operating-guide.md）:\n{}",
        violations
            .into_iter()
            .map(|item| format!("[requirements-check]   - {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

pub fn cargo_default_run(payload: &Value) -> StepResult {
    let Some(input) = tool_input(payload) else {
        return StepResult::ok();
    };
    let raw = json_string(input, "file_path").replace('\\', "/");
    if !raw.ends_with("Cargo.toml") {
        return StepResult::ok();
    }
    let root = repo_root();
    let path = normalize_path(&root, &raw);
    let Ok(text) = fs::read_to_string(&path) else {
        return StepResult::ok();
    };
    let bin_count = Regex::new(r"(?m)^\s*\[\[bin\]\]")
        .unwrap()
        .find_iter(&text)
        .count();
    if bin_count < 2
        || Regex::new(r"(?m)^\s*default-run\s*=")
            .unwrap()
            .is_match(&text)
    {
        return StepResult::ok();
    }
    StepResult {
        code: 0,
        stdout: String::new(),
        stderr: format!(
            "[cargo-default-run] {} contains {bin_count} [[bin]] sections but no default-run.\n   Plain `cargo run` will fail with: could not determine which binary to run.\n   Fix: add `default-run = \"<main-bin-name>\"` under [package], or make launch scripts call `cargo run --bin <name>` explicitly.\n",
            relative_display(&root, &path)
        ),
    }
}

pub fn fallback_smell(payload: &Value) -> StepResult {
    let Some(input) = tool_input(payload) else {
        return StepResult::ok();
    };
    let raw = json_string(input, "file_path");
    let suffix = Path::new(&raw)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !["py", "pyw", "js", "jsx", "ts", "tsx", "mjs", "cjs"].contains(&suffix.as_str()) {
        return StepResult::ok();
    }
    let mut parts = Vec::new();
    for key in ["new_string", "content"] {
        if let Some(value) = input.get(key).and_then(Value::as_str) {
            parts.push((value, None));
        }
    }
    if let Some(edits) = input.get("edits").and_then(Value::as_array) {
        for edit in edits {
            if let Some(value) = edit.get("new_string").and_then(Value::as_str) {
                parts.push((value, None));
            }
        }
    }
    if let Some(hunks) = input.get("patch_hunks").and_then(Value::as_array) {
        for hunk in hunks {
            if let (Some(text), Some(added)) = (
                hunk.get("new_string").and_then(Value::as_str),
                hunk.get("added_lines").and_then(Value::as_array),
            ) {
                parts.push((text, Some(added)));
            }
        }
    }
    if parts.is_empty() {
        return StepResult::ok();
    }
    let py = Regex::new(r"except\s*(?:\(?\s*(?:Exception|BaseException)\s*\)?(?:\s+as\s+\w+)?)?\s*:\s*(?:\n[ \t]*)?pass\b").unwrap();
    let js = Regex::new(r"catch\s*(?:\([^)]*\))?\s*\{\s*\}").unwrap();
    let mut hits = Vec::new();
    for (text, added_lines) in parts {
        for (pattern, label) in [
            (&py, "裸/宽 except 后直接 pass（静默吞异常）"),
            (&js, "空 catch 块（静默吞异常）"),
        ] {
            for item in pattern.find_iter(text) {
                // Context completes split constructs; only changed matches should warn.
                if let Some(added) = added_lines {
                    let first = text[..item.start()]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count();
                    let last = text[..item.end()]
                        .trim_end_matches('\n')
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count();
                    if !added
                        .iter()
                        .filter_map(Value::as_u64)
                        .any(|line| (first..=last).contains(&(line as usize)))
                    {
                        continue;
                    }
                }
                hits.push(format!(
                    "{label}: {}",
                    item.as_str()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
    }
    if hits.is_empty() {
        return StepResult::ok();
    }
    StepResult::output(format!(
        "[fallback-smell] {} 疑似兜底坏味道（仅裸吞异常，软提醒不阻塞）:\n{}\n[fallback-smell] 静默吞异常会把真报错藏成假成功 → 先确认根因，别用 pass 掩盖",
        Path::new(&raw)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(&raw),
        hits.into_iter()
            .map(|item| format!("[fallback-smell]   - {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

pub fn edit_requires_instruction_check(payloads: &[Value]) -> bool {
    let root = normalize_path(&repo_root(), ".");
    payloads.iter().any(|payload| {
        let Some(input) = tool_input(payload) else {
            return false;
        };
        let target = normalize_path(&root, &json_string(input, "file_path"));
        let relative = relative_display(&root, &target).to_ascii_lowercase();
        target
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("AGENTS.md"))
            || matches!(
                relative.as_str(),
                bridgeforge_core::product_manifest::FILE
                    | ".codex/hooks.json"
                    | "templates/hooks.json"
                    | "source/forge/templates/hooks.json"
                    | ".codex/config.toml"
                    | "doc/readme.md"
                    | "doc/3_reference/codex-hook-signals.md"
            )
            || relative.starts_with(".codex/rules/")
            || relative.starts_with("templates/rules/")
            || relative.starts_with("source/forge/templates/rules/")
    })
}

pub fn instruction_source(_payload: &Value) -> StepResult {
    #[cfg(all(test, bridgeforge_factory_tests))]
    INSTRUCTION_SCANS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let report = bridgeforge_checks::instructions::inspect(&repo_root());
    #[cfg(all(test, bridgeforge_factory_tests))]
    INSTRUCTION_ENTRIES.fetch_add(report.entries_visited, std::sync::atomic::Ordering::Relaxed);
    StepResult {
        code: 0,
        stdout: String::new(),
        stderr: report.diagnostics(),
    }
}

pub fn precommit_instruction_source() -> i32 {
    let step = instruction_source(&Value::Null);
    eprint!("{}", step.stderr);
    if step.stderr.is_empty() { 0 } else { 2 }
}

fn response_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.into();
    }
    let mut result = Vec::new();
    if let Some(map) = value.as_object() {
        for key in ["stdout", "stderr", "output", "text", "error"] {
            if let Some(value) = map.get(key).and_then(Value::as_str) {
                result.push(value);
            }
        }
        if let Some(content) = map.get("content").and_then(Value::as_array) {
            for item in content {
                if let Some(value) = item.get("text").and_then(Value::as_str) {
                    result.push(value);
                }
            }
        }
    }
    result.join("\n")
}

pub fn test_receipt(payload: &Value) -> StepResult {
    let Some(input) = tool_input(payload) else {
        return StepResult::ok();
    };
    let command = json_string(input, "command");
    let Some((kind, attributable)) =
        crate::shell::test_command(&command, crate::shell::dialect(payload))
    else {
        return StepResult::ok();
    };
    let response = payload.get("tool_response").unwrap_or(&Value::Null);
    let interrupted = response
        .get("interrupted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut exit = None;
    let mut source = "unknown";
    for key in [
        "exit_code",
        "exitCode",
        "returncode",
        "returnCode",
        "code",
        "status",
    ] {
        if let Some(value) = response.get(key) {
            if let Some(number) = value.as_i64() {
                exit = Some(number);
                source = "explicit";
                break;
            }
            if let Some(number) = value.as_str().and_then(|text| text.parse::<i64>().ok()) {
                exit = Some(number);
                source = "explicit";
                break;
            }
        }
    }
    let response_output = response_text(response);
    let running = ["session_id", "cell_id"]
        .iter()
        .any(|key| response.get(*key).is_some_and(|value| !value.is_null()))
        || response
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| matches!(status, "running" | "pending"))
        || response_output.contains("Process running with session ID")
        || response_output.contains("Script running with cell ID");
    if running || interrupted || !attributable {
        exit = None;
        source = "unknown";
    }
    let timestamp = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut hasher = Sha1::new();
    hasher.update(command.as_bytes());
    let sha1 = format!("{:x}", hasher.finalize());
    let receipt = json!({"ts":timestamp,"kind":kind,"cmd":command.chars().take(200).collect::<String>(),"sha1":&sha1[..12],"exit_code":exit,"source":source,"interrupted":interrupted,"attributable":attributable});
    let root = repo_root();
    let directory = root.join(bridgeforge_core::artifact_paths::TEST_RECEIPTS);
    if let Err(error) = fs::create_dir_all(&directory) {
        return StepResult::failed("test-receipt directory", error);
    }
    let mut file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("receipts.jsonl"))
    {
        Ok(file) => file,
        Err(error) => return StepResult::failed("test-receipt open", error),
    };
    if let Err(error) = writeln!(file, "{}", receipt).and_then(|_| file.sync_all()) {
        return StepResult::failed("test-receipt write", error);
    }
    let samples = directory.join("payload_samples");
    let sample_result = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(&samples)?;
        let sample = samples.join(format!("{}.json", timestamp.replace(':', "")));
        let bytes = serde_json::to_vec_pretty(&truncate_sample(payload))?;
        atomic_write(&sample, &bytes)?;
        let mut entries: Vec<_> = fs::read_dir(&samples)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("json")
            })
            .collect();
        entries.sort_by_key(|entry| entry.metadata().and_then(|value| value.modified()).ok());
        let remove_count = entries.len().saturating_sub(5);
        for entry in entries.into_iter().take(remove_count) {
            fs::remove_file(entry.path())?;
        }
        Ok(())
    })();
    if let Err(error) = sample_result {
        return StepResult::failed(
            "test-receipt sample/retention (receipt already appended)",
            error,
        );
    }
    StepResult::output(format!(
        "[test-receipt] {} exit={} source={} sha1={} -> .runtime/tests/test-receipts/receipts.jsonl",
        kind,
        exit.map(|value| value.to_string())
            .unwrap_or_else(|| "None".into()),
        source,
        &sha1[..12]
    ))
}

fn truncate_sample(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(text.chars().take(2000).collect()),
        Value::Array(items) => Value::Array(items.iter().map(truncate_sample).collect()),
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(key, value)| (key.clone(), truncate_sample(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

pub fn precommit_encoding() -> i32 {
    match bridgeforge_checks::encoding::inspect_index(
        &repo_root(),
        &bridgeforge_core::SystemProcessRunner,
    ) {
        Ok(hits) if hits.is_empty() => 0,
        Ok(hits) => {
            eprintln!(
                "[encoding] staged/index content failed\n{}",
                hits.join("\n")
            );
            2
        }
        Err(error) => {
            eprintln!("[encoding] cannot verify index: {error}");
            2
        }
    }
}
