use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use walkdir::{DirEntry, WalkDir};

use crate::StepResult;
use crate::util::{
    atomic_write, normalize_path, path_is_inside, relative_display, repo_root, tool_input,
};

const SCHEMA: u8 = 1;
const FIND_MAP: &str = ".runtime/bridgeforge-codex/find-doc.map.md";
const SYNC_MAP: &str = ".runtime/bridgeforge-codex/sync-docs.map.md";
const DIRTY_MARKER: &str = ".runtime/bridgeforge-codex/project-map-dirty";
const GENERATED_MARKER: &str = "<!-- bridgeforge-project-map schema=1";
const EXCLUDED_DIRECTORIES: &[&str] = &[
    ".git",
    ".runtime",
    ".codex",
    ".cache",
    ".venv",
    "coverage",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    "__pycache__",
];
const CODE_EXTENSIONS: &[&str] = &[
    "c", "cc", "cpp", "cs", "css", "go", "h", "hpp", "html", "java", "js", "jsx", "kt", "kts", "m",
    "mm", "php", "proto", "ps1", "py", "rb", "rs", "scala", "scss", "sh", "sql", "swift", "ts",
    "tsx", "vue",
];
const REFERENCE_EXTENSIONS: &[&str] = &[
    "c", "cc", "cpp", "cs", "css", "go", "h", "hpp", "html", "java", "js", "json", "jsx", "kt",
    "kts", "m", "md", "mm", "php", "proto", "ps1", "py", "rb", "rs", "scala", "scss", "sh", "sql",
    "swift", "toml", "ts", "tsx", "vue", "yaml", "yml",
];

fn is_excluded_directory(entry: &DirEntry) -> bool {
    entry.depth() > 0
        && entry.file_type().is_dir()
        && entry
            .file_name()
            .to_str()
            .is_some_and(|name| EXCLUDED_DIRECTORIES.contains(&name))
}

fn walk_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for item in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_excluded_directory(entry))
    {
        let entry = item.map_err(|error| format!("project scan failed: {error}"))?;
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort_by_key(|path| relative_display(root, path));
    Ok(files)
}

fn normalized_text(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let text =
        std::str::from_utf8(bytes).map_err(|_| format!("{} is not valid UTF-8", path.display()))?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

fn hash_inputs(kind: &str, inputs: &[(String, String)]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"bridgeforge-project-map\0");
    hasher.update([SCHEMA]);
    hasher.update(kind.as_bytes());
    hasher.update([0]);
    for (path, content) in inputs {
        hasher.update((path.len() as u64).to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update((content.len() as u64).to_le_bytes());
        hasher.update(content.as_bytes());
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('`', "'")
}

fn display_source(path: &str, heading: &str) -> String {
    if heading.is_empty() {
        path.to_string()
    } else {
        format!("{path} § {heading}")
    }
}

fn clean_heading(raw: &str) -> String {
    raw.trim()
        .trim_matches('#')
        .replace(['`', '*', '_'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn add_topic(topics: &mut BTreeMap<String, BTreeSet<String>>, raw: &str, source: String) {
    let topic = clean_heading(raw).to_lowercase();
    if topic.is_empty() || topic.chars().count() > 96 {
        return;
    }
    topics.entry(topic).or_default().insert(source);
}

fn find_doc_map(root: &Path, files: &[PathBuf]) -> Result<String, String> {
    let heading = Regex::new(r"(?m)^#{1,6}\s+(.+?)\s*$").unwrap();
    let inline = Regex::new(r"`([^`\r\n]+)`").unwrap();
    let mut inputs = Vec::new();
    let mut topics = BTreeMap::<String, BTreeSet<String>>::new();
    for path in files {
        if path.file_name().and_then(|name| name.to_str()) != Some("AGENTS.md") {
            continue;
        }
        let relative = relative_display(root, path);
        if relative.starts_with("templates/")
            || relative.starts_with("source/forge/templates/")
            || (relative.starts_with("doc/") && relative != "doc/AGENTS.md")
        {
            continue;
        }
        let text = normalized_text(path)?;
        inputs.push((relative.clone(), text.clone()));
        if let Some(parent) = Path::new(&relative).parent()
            && !parent.as_os_str().is_empty()
        {
            let scope = parent.to_string_lossy().replace('\\', "/");
            add_topic(&mut topics, &scope, relative.clone());
            if let Some(name) = parent.file_name().and_then(|name| name.to_str()) {
                add_topic(&mut topics, name, relative.clone());
            }
        }
        let mut current_heading = String::new();
        for line in text.lines() {
            if let Some(captures) = heading.captures(line) {
                current_heading = clean_heading(&captures[1]);
                add_topic(
                    &mut topics,
                    &current_heading,
                    display_source(&relative, &current_heading),
                );
                continue;
            }
            for captures in inline.captures_iter(line) {
                let token = captures[1].trim();
                if token.is_empty()
                    || token.chars().count() > 64
                    || token.contains(['/', '\\'])
                    || token.contains("http://")
                    || token.contains("https://")
                {
                    continue;
                }
                add_topic(
                    &mut topics,
                    token,
                    display_source(&relative, &current_heading),
                );
            }
        }
    }
    let fingerprint = hash_inputs("find-doc", &inputs);
    let mut output = format!(
        "<!-- bridgeforge-project-map schema={SCHEMA} kind=find-doc input={fingerprint} -->\n# find-doc 项目自动索引\n\n> 此文件由 BridgeForge 自动生成，禁止手工维护。\n> 输入指纹：`{fingerprint}`\n\n## topic_to_sources\n\n| 主题或代码词 | 已证明的指令源 |\n|---|---|\n"
    );
    if topics.is_empty() {
        output.push_str("| （当前没有可证明的主题映射） | （使用 find-doc 搜索 fallback） |\n");
    } else {
        for (topic, sources) in topics {
            let sources = sources
                .into_iter()
                .map(|source| format!("`{}`", markdown_cell(&source)))
                .collect::<Vec<_>>()
                .join("<br>");
            output.push_str(&format!("| `{}` | {sources} |\n", markdown_cell(&topic)));
        }
    }
    output.push_str("\n未命中主题时，`$find-doc` 必须继续使用文档搜索 fallback。\n");
    Ok(output)
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| extensions.contains(&value.to_ascii_lowercase().as_str()))
}

fn is_code_path(path: &Path) -> bool {
    has_extension(path, CODE_EXTENSIONS)
}

fn is_referenceable_path(path: &Path) -> bool {
    has_extension(path, REFERENCE_EXTENSIONS)
}

fn source_root(relative: &str) -> String {
    let parts = relative.split('/').collect::<Vec<_>>();
    if let Some(index) = parts.iter().position(|part| *part == "src") {
        return format!("{}/**", parts[..=index].join("/"));
    }
    if parts.len() >= 3 {
        return format!("{}/{}/**", parts[0], parts[1]);
    }
    if parts.len() == 2 {
        return format!("{}/**", parts[0]);
    }
    relative.to_string()
}

fn reference_source(
    root: &Path,
    raw: &str,
    code_paths: &BTreeSet<String>,
    referenceable_paths: &BTreeSet<String>,
) -> Option<String> {
    // Reuse compiled patterns across all references in this Hook invocation.
    // Keep normalization order and matching semantics unchanged.
    static LINE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r":\d+(?::\d+)?$").unwrap());
    static WINDOWS_DRIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z]:").unwrap());
    let mut candidate = raw
        .trim()
        .trim_matches(['"', '\'', '(', ')', '[', ']'])
        .replace('\\', "/");
    if let Some((path, _)) = candidate.split_once('§') {
        candidate = path.trim().to_string();
    }
    candidate = LINE_SUFFIX
        .replace(&candidate, "")
        .trim_end_matches(['.', ',', ';', '。', '，', '；'])
        .to_string();
    if candidate.is_empty()
        || candidate.starts_with(['/', '~'])
        || candidate.contains("://")
        || candidate.contains("..")
        || WINDOWS_DRIVE.is_match(&candidate)
    {
        return None;
    }
    let base = candidate
        .find('*')
        .map(|index| &candidate[..index])
        .unwrap_or(&candidate)
        .trim_end_matches('/');
    if base.is_empty()
        || base.starts_with("doc/")
        || base.starts_with(".codex/")
        || base.starts_with(".runtime/")
        || base.starts_with(".git/")
    {
        return None;
    }
    let target = normalize_path(root, base);
    if !path_is_inside(root, &target) || !target.exists() {
        return None;
    }
    let base_prefix = format!("{base}/");
    let represents_source = if target.is_file() {
        referenceable_paths.contains(base)
    } else {
        code_paths.iter().any(|path| path.starts_with(&base_prefix))
    };
    if !represents_source {
        return None;
    }
    if candidate.contains('*') {
        Some(candidate)
    } else if target.is_dir() {
        Some(format!("{base}/**"))
    } else {
        Some(base.to_string())
    }
}

fn sync_docs_map(root: &Path, files: &[PathBuf]) -> Result<String, String> {
    let inline = Regex::new(r"`([^`\r\n]+)`").unwrap();
    let markdown_link = Regex::new(r"\[[^\]\r\n]*\]\(([^)\s]+)\)").unwrap();
    let mut code_paths = BTreeSet::new();
    let mut referenceable_paths = BTreeSet::new();
    for path in files {
        let relative = relative_display(root, path);
        if relative.starts_with("doc/") {
            continue;
        }
        if is_code_path(path) {
            code_paths.insert(relative.clone());
        }
        if is_referenceable_path(path) {
            referenceable_paths.insert(relative);
        }
    }
    let mut inputs = referenceable_paths
        .iter()
        .map(|path| (format!("source:{path}"), String::new()))
        .collect::<Vec<_>>();
    let mut mappings = BTreeMap::<String, BTreeSet<String>>::new();
    for path in files {
        let relative = relative_display(root, path);
        let is_manifest = path.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml");
        if is_manifest {
            inputs.push((relative.clone(), normalized_text(path)?));
        }
        let is_design = relative.starts_with("doc/0_architecture/")
            || relative.starts_with("doc/3_reference/")
            || relative == "doc/README.md";
        if !is_design || path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        let text = normalized_text(path)?;
        inputs.push((relative.clone(), text.clone()));
        for captures in inline.captures_iter(&text) {
            if let Some(source) =
                reference_source(root, &captures[1], &code_paths, &referenceable_paths)
            {
                mappings.entry(source).or_default().insert(relative.clone());
            }
        }
        for captures in markdown_link.captures_iter(&text) {
            if let Some(source) =
                reference_source(root, &captures[1], &code_paths, &referenceable_paths)
            {
                mappings.entry(source).or_default().insert(relative.clone());
            }
        }
    }
    inputs.sort_by(|left, right| left.0.cmp(&right.0));
    let fingerprint = hash_inputs("sync-docs", &inputs);
    let roots = code_paths
        .iter()
        .map(|path| source_root(path))
        .collect::<BTreeSet<_>>();
    let mut output = format!(
        "<!-- bridgeforge-project-map schema={SCHEMA} kind=sync-docs input={fingerprint} -->\n# sync-docs 项目自动索引\n\n> 此文件由 BridgeForge 自动生成，禁止手工维护。\n> 输入指纹：`{fingerprint}`\n\n## source_to_docs\n\n| 源码路径 | 有明确引用的既有文档 |\n|---|---|\n"
    );
    if mappings.is_empty() {
        output.push_str(
            "| （当前没有可证明的源码到文档映射） | （使用 sync-docs 搜索 fallback） |\n",
        );
    } else {
        for (source, documents) in mappings {
            let documents = documents
                .into_iter()
                .map(|document| format!("`{}`", markdown_cell(&document)))
                .collect::<Vec<_>>()
                .join("<br>");
            output.push_str(&format!("| `{}` | {documents} |\n", markdown_cell(&source)));
        }
    }
    output.push_str("\n## source_roots\n\n");
    if roots.is_empty() {
        output.push_str("- （未发现源码根）\n");
    } else {
        for source in roots {
            output.push_str(&format!("- `{}`\n", markdown_cell(&source)));
        }
    }
    output.push_str(
        "\n未命中路径时，`$sync-docs` 必须继续使用文档搜索 fallback；禁止据目录同名猜测关系。\n",
    );
    Ok(output)
}

fn validate_target(root: &Path, relative: &str) -> Result<PathBuf, String> {
    ensure_plain_runtime_directory(root)?;
    let path = root.join(relative);
    if path.exists() {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect {relative}: {error}"))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(format!("{relative} is not a plain file"));
        }
    }
    Ok(path)
}

fn write_if_changed(path: &Path, content: &str) -> Result<(), String> {
    if fs::read(path).ok().as_deref() == Some(content.as_bytes()) {
        return Ok(());
    }
    atomic_write(path, content.as_bytes())
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn dirty_path(root: &Path) -> PathBuf {
    root.join(DIRTY_MARKER)
}

fn ensure_plain_runtime_directory(root: &Path) -> Result<PathBuf, String> {
    let runtime = root.join(".runtime");
    let state = runtime.join("bridgeforge-codex");
    for path in [&runtime, &state] {
        if path.exists() {
            let metadata = fs::symlink_metadata(path)
                .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err(format!("{} is not a plain directory", path.display()));
            }
        } else {
            if let Err(error) = fs::create_dir(path)
                && (!path.is_dir()
                    || fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()))
            {
                return Err(format!("cannot create {}: {error}", path.display()));
            }
        }
    }
    Ok(state)
}

fn remove_dirty(root: &Path) -> Result<(), String> {
    let path = dirty_path(root);
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).map_err(|error| format!("cannot clear {}: {error}", path.display()))
}

fn ensure(root: &Path) -> Result<(), String> {
    let files = walk_files(root)?;
    let find_content = find_doc_map(root, &files)?;
    let sync_content = sync_docs_map(root, &files)?;
    let find_target = validate_target(root, FIND_MAP)?;
    let sync_target = validate_target(root, SYNC_MAP)?;
    write_if_changed(&find_target, &find_content)?;
    write_if_changed(&sync_target, &sync_content)?;
    remove_dirty(root)
}

fn is_generated(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|text| text.starts_with(GENERATED_MARKER))
}

pub fn ensure_current() -> StepResult {
    match ensure(&repo_root()) {
        Ok(()) => StepResult::ok(),
        Err(error) => StepResult::failed("project-map", error),
    }
}

pub fn ensure_if_dirty() -> StepResult {
    let root = repo_root();
    if !dirty_path(&root).exists()
        && is_generated(&root.join(FIND_MAP))
        && is_generated(&root.join(SYNC_MAP))
    {
        return StepResult::ok();
    }
    ensure_current()
}

fn relevant(relative: &str) -> bool {
    if matches!(relative, FIND_MAP | SYNC_MAP) {
        return true;
    }
    if relative.starts_with("doc/4_archive/")
        || EXCLUDED_DIRECTORIES
            .iter()
            .any(|name| relative == *name || relative.starts_with(&format!("{name}/")))
    {
        return false;
    }
    let path = Path::new(relative);
    if path.file_name().and_then(|name| name.to_str()) == Some("AGENTS.md")
        || path.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml")
    {
        return true;
    }
    if relative.starts_with("doc/")
        && path.extension().and_then(|value| value.to_str()) == Some("md")
    {
        return true;
    }
    is_code_path(path)
}

pub fn mark_dirty(payload: &Value) -> StepResult {
    let Some(input) = tool_input(payload) else {
        return StepResult::ok();
    };
    let Some(raw) = input.get("file_path").and_then(Value::as_str) else {
        return StepResult::ok();
    };
    let root = repo_root();
    let path = normalize_path(&root, raw);
    if !path_is_inside(&root, &path) {
        return StepResult::ok();
    }
    let relative = relative_display(&root, &path);
    if !relevant(&relative) {
        return StepResult::ok();
    }
    let marker = dirty_path(&root);
    if marker.is_file() {
        return StepResult::ok();
    }
    if let Err(error) = ensure_plain_runtime_directory(&root) {
        return StepResult::failed("project-map dirty marker", error);
    }
    match atomic_write(&marker, b"dirty\n") {
        Ok(()) => StepResult::ok(),
        Err(error) => StepResult::failed("project-map dirty marker", error),
    }
}
