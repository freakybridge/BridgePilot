use bridgeforge_core::{ProcessRequest, ProcessRunner, SystemProcessRunner};
use regex::Regex;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::StepResult;
use crate::util::{
    ToolKind, json_string, normalize_path, path_is_inside, repo_root, tool_input, tool_kind,
    tool_name,
};

fn command(payload: &Value) -> String {
    tool_input(payload)
        .map(|value| json_string(value, "command"))
        .unwrap_or_default()
}

fn contains_single_pipe(command: &str) -> bool {
    let bytes = command.as_bytes();
    bytes.iter().enumerate().any(|(index, value)| {
        *value == b'|'
            && (index == 0 || bytes[index - 1] != b'|')
            && (index + 1 == bytes.len() || bytes[index + 1] != b'|')
    })
}

pub fn git_add_all(payload: &Value) -> StepResult {
    git_add_all_with_runner(payload, &repo_root(), &SystemProcessRunner)
}

pub fn git_add_all_with_runner(
    payload: &Value,
    root: &Path,
    runner: &dyn ProcessRunner,
) -> StepResult {
    let command = command(payload);
    static GIT_ADD: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"(?i)\bgit\b[^&|;]*\sadd\b(?P<args>[^&|;]*)").unwrap()
    });
    let bulk = GIT_ADD.captures_iter(&command).any(|capture| {
        capture.name("args").is_some_and(|args| {
            args.as_str().split_whitespace().any(|part| {
                let part = part.trim_matches(['\'', '"']).replace('\\', "/");
                matches!(part.as_str(), "-A" | "--all" | "." | "./")
            })
        })
    });
    if !bulk {
        return StepResult::ok();
    }
    let mut request = ProcessRequest::new("git", root);
    request.args = vec![
        "status".into(),
        "--porcelain=v1".into(),
        "-z".into(),
        "--untracked-files=all".into(),
    ];
    request.timeout = Duration::from_secs(10);
    let output = match runner.run(&request) {
        Ok(output) => output,
        Err(error) => {
            return StepResult::blocked(format!(
                "[git-add-guard] BLOCKED：无法查询 Git 状态，已阻止批量暂存：{error}\n"
            ));
        }
    };
    if output.timed_out || output.code != 0 {
        let reason = if output.timed_out {
            "Git 状态查询超时".to_string()
        } else {
            format!("Git 状态查询失败（退出码 {}）", output.code)
        };
        return StepResult::blocked(format!(
            "[git-add-guard] BLOCKED：{reason}，已阻止批量暂存；请排除查询故障后重试。\n"
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let sensitive = Regex::new(r"(?i)^(?:\.env(?:\..*)?|.*\.(?:key|pem|pfx|p12)|id_(?:rsa|ed25519)(?:\..*)?|\.git-credentials|\.npmrc|\.pypirc)$").unwrap();
    let mut flagged = Vec::new();
    let mut entries = text.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        let Some(path) = entry.get(3..) else {
            return StepResult::blocked("[git-add-guard] invalid Git status record".into());
        };
        // In -z porcelain the destination is first, followed by the source path.
        if entry.as_bytes()[..2]
            .iter()
            .any(|status| matches!(status, b'R' | b'C'))
        {
            if entries.next().is_none() {
                return StepResult::blocked("[git-add-guard] incomplete Git rename record".into());
            }
        }
        let normalized = path.replace('\\', "/");
        let base = normalized.rsplit('/').next().unwrap_or(&normalized);
        if [".example", ".sample", ".template", ".dist"]
            .iter()
            .any(|suffix| base.ends_with(suffix))
        {
            continue;
        }
        let reason = if sensitive.is_match(base) {
            Some("credential or sensitive file")
        } else if format!("/{normalized}").contains("/.runtime/") {
            Some("runtime artifact (.runtime/)")
        } else {
            None
        };
        if let Some(reason) = reason {
            flagged.push(format!("  - {path}  ({reason})"));
        }
    }
    if flagged.is_empty() {
        return StepResult::ok();
    }
    StepResult::blocked(format!(
        "[git-add-guard] BLOCKED 未完成：批量暂存包含高风险文件：\n{}\n   下一步：审查后使用 `git add <精确路径>`；确属生成物或秘密文件时，先加入 .gitignore，再重试批量暂存。\n",
        flagged.join("\n")
    ))
}

pub fn non_ascii_shell(payload: &Value) -> StepResult {
    let command = command(payload);
    if command.is_empty() || command.is_ascii() {
        return StepResult::ok();
    }
    let redirect = Regex::new(r"(^|[^<>=])>{1,2}($|[^>])")
        .unwrap()
        .is_match(&command);
    let write_command = Regex::new(r"(?i)\b(?:Set-Content|Out-File|Add-Content|Tee-Object|tee)\b")
        .unwrap()
        .is_match(&command);
    let shell_transit = redirect
        || command.contains("@'")
        || command.contains("@\"")
        || command.contains("<<")
        || command.contains("$(")
        || contains_single_pipe(&command);
    let dynamic = Regex::new(r"(?i)(?:^|[\s|;&])(?:python(?:3)?|py|node|deno|ruby|perl|bash|sh|pwsh|powershell)(?:\.exe)?\s+(?:-|-[ce]\b|-Command\b|-EncodedCommand\b)").unwrap().is_match(&command);
    let write_api = Regex::new(r#"(?i)\b(?:fs\.(?:writeFileSync|writeFile|appendFileSync|appendFile)|writeFileSync|write_text|open\s*\([^)]*['\"](?:w|a))\b"#).unwrap().is_match(&command);
    let mut reasons = Vec::new();
    if redirect {
        reasons.push("shell redirection writes command text to a file");
    }
    if write_command {
        reasons.push("shell file-writing command receives non-ASCII text");
    }
    if shell_transit && dynamic {
        reasons.push("non-ASCII text is routed through shell transit into dynamic execution");
    }
    if dynamic && write_api {
        reasons.push("inline dynamic script can write non-ASCII text");
    }
    if shell_transit && write_api {
        reasons.push("shell transit carries non-ASCII text into a write API");
    }
    if reasons.is_empty() {
        return StepResult::ok();
    }
    let details = reasons
        .into_iter()
        .map(|reason| format!("[non-ascii-shell-guard]   - {reason}"))
        .collect::<Vec<_>>()
        .join("\n");
    StepResult::blocked(format!(
        "[non-ascii-shell-guard] Blocked risky shell command: non-ASCII text is crossing a shell write/dynamic-exec boundary.\n{details}\n[non-ascii-shell-guard] Use apply_patch/Edit/Write, copy an existing UTF-8 file, or keep script source ASCII and read UTF-8 input explicitly.\n"
    ))
}

fn edit_target(payload: &Value) -> String {
    tool_input(payload)
        .map(|value| {
            let first = json_string(value, "file_path");
            if first.is_empty() {
                json_string(value, "path")
            } else {
                first
            }
        })
        .unwrap_or_default()
}

fn outside(root: &Path, raw: &str) -> Option<PathBuf> {
    let candidate = normalize_path(root, raw);
    (!path_is_inside(root, &candidate)).then_some(candidate)
}

pub fn cross_project_write(payload: &Value) -> StepResult {
    let root = repo_root();
    let name = tool_name(payload);
    let risk = if tool_kind(&name) == Some(ToolKind::Edit) {
        let raw = edit_target(payload);
        outside(&root, &raw).map(|path| (format!("{name} outside project root"), path))
    } else if tool_kind(&name) == Some(ToolKind::Shell) {
        let command = command(payload);
        let cwd = crate::util::command_directory(payload, &root);
        match crate::shell::write_targets(&command, &cwd, crate::shell::dialect(payload)) {
            Ok(targets) => targets
                .into_iter()
                .find(|path| !path_is_inside(&root, path))
                .map(|path| ("external shell write/delete/move".into(), path)),
            Err(error) => {
                let protected = user_config_write(payload);
                if protected.code != 0 {
                    return protected;
                }
                return StepResult::blocked(format!(
                    "[cross-project-write-guard] BLOCKED: cannot resolve supported write command: {error}. Split the operation into direct commands with literal paths and an explicit tool working directory.\n"
                ));
            }
        }
    } else {
        None
    };
    let Some((reason, target)) = risk else {
        return StepResult::ok();
    };
    StepResult::blocked(format!(
        "[cross-project-write-guard] BLOCKED 未完成：拒绝跨项目写入。\n[cross-project-write-guard]   当前项目：{}\n[cross-project-write-guard]   目标路径：{}\n[cross-project-write-guard]   操作：{}\n[cross-project-write-guard]   下一步：已有授权只继承一次；请在目标项目的受管任务中执行该项目内操作。当前入口按路径边界判定，用户确认不会改变结果，禁止原样重复重试。目标或范围未获授权时先澄清，禁止修改 Hook 根路径来绕过检查。\n",
        root.display(),
        target.display(),
        reason
    ))
}

pub fn user_config_write(payload: &Value) -> StepResult {
    let name = tool_name(payload);
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    let protected = normalize_path(
        &repo_root(),
        &home.join(".codex/config.toml").to_string_lossy(),
    );
    let is_target = |raw: &str| {
        let compact = raw
            .trim()
            .trim_matches(['\'', '"'])
            .replace('\\', "/")
            .to_lowercase();
        let symbolic = Regex::new(r"(?i)^(?:~|\$HOME|\$\{HOME\}|\$env:(?:USERPROFILE|HOME)|%(?:USERPROFILE|HOME)%)/\.codex/config\.toml$").unwrap();
        symbolic.is_match(&compact) || normalize_path(&repo_root(), raw) == protected
    };
    let blocked = if tool_kind(&name) == Some(ToolKind::Edit) {
        let target = edit_target(payload);
        !target.is_empty() && is_target(&target)
    } else if tool_kind(&name) == Some(ToolKind::Shell) {
        let command = command(payload);
        let cwd = crate::util::command_directory(payload, &repo_root());
        match crate::shell::write_targets(&command, &cwd, crate::shell::dialect(payload)) {
            Ok(targets) => targets
                .iter()
                .any(|path| is_target(&path.to_string_lossy())),
            // The preceding cross-project guard reports opaque write operands.
            Err(error) => error
                .strip_prefix("unresolved path operand: ")
                .is_some_and(is_target),
        }
    } else {
        false
    };
    if !blocked {
        return StepResult::ok();
    }
    StepResult::blocked(format!(
        "[user-config-write-guard] BLOCKED 未完成：禁止写入用户级 Codex 模型配置。\n[user-config-write-guard]   受保护路径：{}\n[user-config-write-guard]   下一步：保留只读访问；骨架不得修改该文件。\n",
        protected.display()
    ))
}
