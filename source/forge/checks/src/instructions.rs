//! Reusable instruction-source validation. Callers own event/output adaptation.
use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
use walkdir::WalkDir;

#[derive(Clone, Debug, Default, Serialize)]
pub struct InstructionReport {
    pub issues: Vec<String>,
    #[serde(skip)]
    pub entries_visited: usize,
}

impl InstructionReport {
    pub fn diagnostics(&self) -> String {
        self.issues
            .iter()
            .map(|item| format!("[instruction-source] {item}\n"))
            .collect()
    }
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

const PUBLIC_BEGIN: &str = "<!-- BRIDGEFORGE:PUBLIC:BEGIN -->";
const PUBLIC_END: &str = "<!-- BRIDGEFORGE:PUBLIC:END -->";
const PROJECT_BEGIN: &str = "<!-- BRIDGEFORGE:PROJECT:BEGIN -->";
const PROJECT_END: &str = "<!-- BRIDGEFORGE:PROJECT:END -->";
const RETIRED_INSTRUCTION_RUNTIME: &[&str] = &[
    "skill-routing.json",
    "clarify_reminder.py",
    "focus_reminder.py",
    "project_memory_writer.py",
    "memory_rebuild_index.py",
    "memory_lint.py",
];

fn line_finish(text: &str, marker: usize) -> usize {
    text[marker..]
        .find('\n')
        .map(|offset| marker + offset + 1)
        .unwrap_or(text.len())
}

fn zone_parts(text: &str) -> Result<(&str, &str), String> {
    let markers = [PUBLIC_BEGIN, PUBLIC_END, PROJECT_BEGIN, PROJECT_END];
    if markers
        .iter()
        .any(|marker| text.matches(marker).count() != 1)
    {
        return Err("AGENTS zone markers must each appear exactly once".into());
    }
    let positions: Vec<_> = markers
        .iter()
        .map(|marker| text.find(marker).unwrap())
        .collect();
    if !positions.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err("AGENTS zone markers are reversed or nested".into());
    }
    let public_finish = line_finish(text, positions[1]);
    let project_finish = line_finish(text, positions[3]);
    let outside = format!(
        "{}{}{}",
        &text[..positions[0]],
        &text[public_finish..positions[2]],
        &text[project_finish..]
    );
    if !outside.trim().is_empty() {
        return Err("AGENTS content exists outside the public/project zones".into());
    }
    Ok((
        &text[positions[0]..public_finish],
        &text[positions[2]..project_finish],
    ))
}

fn visible_heading_positions(
    text: &str,
    headings: &[&str],
) -> Result<BTreeMap<String, Vec<usize>>, String> {
    let mut result: BTreeMap<String, Vec<usize>> = headings
        .iter()
        .map(|heading| ((*heading).to_string(), Vec::new()))
        .collect();
    let mut fence: Option<(char, usize)> = None;
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let stripped = line.trim_end_matches(['\r', '\n']);
        let leading = stripped.len() - stripped.trim_start_matches(' ').len();
        let visible = stripped.trim_start_matches(' ');
        let marker_len = visible
            .chars()
            .take_while(|value| *value == '`' || *value == '~')
            .count();
        if leading <= 3 && marker_len >= 3 {
            let marker = visible.chars().next().unwrap();
            match fence {
                Some((open, minimum))
                    if marker == open
                        && marker_len >= minimum
                        && visible[marker_len..].trim().is_empty() =>
                {
                    fence = None;
                }
                None => fence = Some((marker, marker_len)),
                _ => {}
            }
        } else if fence.is_none()
            && leading <= 3
            && let Some(positions) = result.get_mut(visible)
        {
            positions.push(offset);
        }
        offset += line.len();
    }
    if fence.is_some() {
        return Err("AGENTS contains an unclosed fenced code block".into());
    }
    Ok(result)
}

fn claims_positive_autoload(text: &str) -> bool {
    let positives = [
        Regex::new(r"(?i)详细规则按需加载自\s*[^\n]*rules").unwrap(),
        Regex::new(r"(?i)Markdown[^\n]{0,80}(?:paths:|path)[^\n]{0,80}(?:自动|按需|始终)加载")
            .unwrap(),
        Regex::new(r"(?i)(?:自动|按需|始终)加载[^\n]{0,80}Markdown[^\n]{0,80}(?:paths:|path)")
            .unwrap(),
        Regex::new(r"(?i)(?:paths:|path-rule)[^\n]{0,80}(?:自动|按需|始终)加载").unwrap(),
    ];
    let negated = Regex::new(
        r"(?i)不会|不能|不支持|并不|未被|禁止.{0,24}(?:宣称|建立)|does\s+not|not\s+(?:be\s+)?auto",
    )
    .unwrap();
    text.split(['。', '！', '？', '；', ';', '，', ',', '\n'])
        .any(|clause| {
            positives.iter().any(|item| item.is_match(clause)) && !negated.is_match(clause)
        })
}

fn managed_public_hash(_root: &Path) -> Option<String> {
    bridgeforge_core::template_standard::assets().into_iter()
        .find(|asset| asset.rule["id"] == "root.agents")
        .and_then(|asset| asset.rule["agents_zones"]["public"]["current_sha256"].as_str().map(str::to_owned))
}

fn public_hash(public: &str) -> String {
    let clone = Regex::new(
        r"(?m)^(git clone <repo_url> )(?:[A-Za-z0-9._-]+|\{\{PROJECT_NAME\}\})( && cd )(?:[A-Za-z0-9._-]+|\{\{PROJECT_NAME\}\})([ \t]*)$",
    )
    .unwrap();
    let normalized = clone.replace_all(public, "$1{{PROJECT_NAME}}$2{{PROJECT_NAME}}$3");
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn instruction_text_issues(path: &Path, root: &Path, nested: bool) -> Vec<String> {
    let label = relative_display(root, path);
    let Ok(bytes) = fs::read(path) else {
        return vec![format!("cannot read {label}")];
    };
    if bytes.len() > 48 * 1024 {
        return vec![format!("{label} exceeds 49152 bytes")];
    }
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return vec![format!("{label} contains UTF-8 BOM")];
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return vec![format!("cannot read {label}: invalid UTF-8")];
    };
    let lowered = text.to_lowercase();
    let mut issues = Vec::new();
    for retired in RETIRED_INSTRUCTION_RUNTIME {
        if lowered.contains(&retired.to_lowercase()) {
            issues.push(format!("{label} references retired runtime asset {retired}; keep retirement history outside active instructions"));
        }
    }
    if claims_positive_autoload(&text) {
        issues.push(format!(
            "{label} claims unsupported Markdown paths auto-loading"
        ));
    }
    if nested { issues } else { Vec::new() }
}

pub fn inspect(root: &Path) -> InstructionReport {
    let mut entries_visited = 0;
    let path = root.join("AGENTS.md");
    let Ok(bytes) = fs::read(&path) else {
        return InstructionReport {
            issues: vec!["cannot read AGENTS.md".into()],
            entries_visited,
        };
    };
    let mut issues = Vec::new();
    if bytes.len() > 48 * 1024 {
        issues.push("AGENTS.md exceeds 49152 bytes".to_string());
    }
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        issues.push("AGENTS.md contains UTF-8 BOM".into());
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            issues.push("AGENTS.md is not valid UTF-8".into());
            String::from_utf8_lossy(error.as_bytes()).into_owned()
        }
    }
    .replace("\r\n", "\n")
    .replace('\r', "\n");
    let parts = zone_parts(&text);
    match &parts {
        Ok((public, project)) => {
            // The managed public hash below validates the complete public block.
            // Do not duplicate its editorial headings in the runtime contract.
            if let Err(error) = visible_heading_positions(public, &[]) {
                issues.push(format!("AGENTS.md: {error}"));
            }
            let project_headings = [
                "## 项目级专区",
                "### 项目架构红线",
                "### 项目业务与安全红线",
                "### 项目目录地图",
                "### 项目快速命令",
                "### 目录级 AGENTS 索引",
            ];
            let project_heads = visible_heading_positions(project, &project_headings);
            if let Err(error) = &project_heads {
                issues.push(format!("AGENTS.md: {error}"));
            }
            let mut ordered = Vec::new();
            for heading in project_headings {
                let positions = project_heads
                    .as_ref()
                    .ok()
                    .and_then(|items| items.get(heading));
                if positions.is_none_or(|items| items.len() != 1) {
                    issues.push(format!(
                        "AGENTS.md project zone must contain exactly one {heading}"
                    ));
                } else {
                    ordered.push(positions.unwrap()[0]);
                }
            }
            if !ordered.windows(2).all(|pair| pair[0] < pair[1]) {
                issues.push("AGENTS.md project zone headings are out of order".into());
            }
            match managed_public_hash(&root) {
                Some(expected) if public_hash(public) != expected => issues.push(
                    "AGENTS.md BridgeForge public zone was modified; move project constraints to the project zone and restore the official public block".into(),
                ),
                None => issues.push("AGENTS.md BridgeForge public zone cannot be verified because the managed contract is missing, invalid, or has no trusted public hash".into()),
                _ => {}
            }
            if text.contains("规则文件索引") || text.contains("Rule 文件索引") {
                issues.push("active Markdown rule index is forbidden".into());
            }
            if claims_positive_autoload(&text) {
                issues.push("AGENTS.md claims unsupported Markdown paths auto-loading".into());
            }
            if let Ok(template) = fs::read_to_string(bridgeforge_core::source_projection::template_path(&root, "templates/AGENTS.md"))
                && let Ok((template_public, _)) = zone_parts(&template)
            {
                let rendered = template_public.replace("{{PROJECT_NAME}}", "BridgeForgeCodex");
                if rendered != *public {
                    issues.push(
                        "factory AGENTS public regions drift from templates/AGENTS.md".into(),
                    );
                }
            }
        }
        Err(error) => issues.push(error.clone()),
    }
    for target in ["doc/README.md", "doc/3_reference/codex-hook-signals.md"] {
        if text.contains(target) && !root.join(target).is_file() {
            issues.push(format!(
                "AGENTS.md references missing managed instruction document: {target}"
            ));
        }
    }
    for retired in RETIRED_INSTRUCTION_RUNTIME {
        if text.to_lowercase().contains(&retired.to_lowercase()) {
            issues.push(format!("AGENTS.md references retired runtime asset {retired}; keep retirement history outside active instructions"));
        }
    }
    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            if matches!(entry.file_name().to_str(), Some(".git" | ".runtime")) {
                return false;
            }
            entry
                .path()
                .strip_prefix(&root)
                .ok()
                .is_none_or(|path| path != Path::new("doc/4_archive"))
        })
        .filter_map(Result::ok)
    {
        entries_visited += 1;
        if !entry.file_type().is_file() || entry.file_name() != "AGENTS.md" || entry.path() == path
        {
            continue;
        }
        let relative = entry.path().strip_prefix(&root).unwrap_or(entry.path());
        let parts: Vec<_> = relative
            .components()
            .map(|item| item.as_os_str().to_string_lossy())
            .collect();
        if parts
            .iter()
            .any(|item| item == ".git" || item == ".runtime")
            || (parts.first().is_some_and(|item| item == "doc")
                && parts.get(1).is_some_and(|item| item == "4_archive"))
        {
            continue;
        }
        issues.extend(instruction_text_issues(entry.path(), &root, true));
    }
    for directory in [bridgeforge_core::source_projection::template_path(&root, "templates/rules"), root.join(".codex/rules")] {
        if directory.is_dir()
            && WalkDir::new(&directory)
                .max_depth(1)
                .into_iter()
                .filter_map(Result::ok)
                .any(|entry| {
                    entry.path().extension().and_then(|value| value.to_str()) == Some("md")
                })
        {
            issues.push(format!(
                "factory Markdown rule directory must remain retired: {}",
                relative_display(&root, &directory)
            ));
        }
    }
    InstructionReport {
        issues,
        entries_visited,
    }
}
