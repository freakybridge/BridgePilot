//! Explicit file and staged-blob encoding validation, independent of Hook events.
use bridgeforge_core::{ProcessRequest, ProcessRunner};
use regex::Regex;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

const TEXT_SUFFIXES: &[&str] = &[
    "md", "json", "py", "rs", "toml", "sh", "ps1", "yml", "yaml", "txt",
];

const TEXT_NAMES: &[&str] = &[
    "AGENTS.md",
    "CHANGELOG.md",
    "INSTALL.md",
    "README.md",
    "RETIRED.md",
    "SKILL.md",
    "VERSION",
    "pre-commit",
];
const TEXT_ROOTS: &[&str] = &[
    "templates",
    "source",
    "skills",
    "scripts",
    ".githooks",
    ".codex",
    "doc",
    "README.md",
    "INSTALL.md",
    "SKILL.md",
    "CHANGELOG.md",
    "VERSION",
    "AGENTS.md",
];

pub fn is_text(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| TEXT_SUFFIXES.contains(&value.to_lowercase().as_str()))
        || path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| TEXT_NAMES.contains(&value))
}

fn garble_hits(root: &Path, files: &[(PathBuf, Vec<u8>)]) -> Vec<String> {
    static RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"\?{3,}|\u{fffd}").unwrap());
    let mut hits = Vec::new();
    for (path, bytes) in files {
        let text = String::from_utf8_lossy(bytes);
        for (index, line) in text.lines().enumerate() {
            if !RE.is_match(line) {
                continue;
            }
            let mut snippet = line.trim().to_string();
            if snippet.chars().count() > 120 {
                snippet = snippet.chars().take(117).collect::<String>() + "...";
            }
            hits.push(format!(
                "{}:{}: {}",
                relative_display(root, path),
                index + 1,
                snippet
            ));
        }
    }
    hits
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct EncodingReport {
    pub bom_paths: Vec<String>,
    pub suspicious: Vec<String>,
}

/// Inspect an explicit caller-selected file set; no workspace traversal occurs.
pub fn inspect_paths(root: &Path, paths: &[PathBuf]) -> Result<EncodingReport, String> {
    inspect_paths_with_observer(root, paths, |_| {})
}

pub fn inspect_paths_with_observer(
    root: &Path,
    paths: &[PathBuf],
    mut before_read: impl FnMut(&Path),
) -> Result<EncodingReport, String> {
    let mut files = Vec::new();
    for path in paths.iter().cloned().collect::<BTreeSet<_>>() {
        before_read(&path);
        files.push((
            path.clone(),
            fs::read(&path).map_err(|error| error.to_string())?,
        ));
    }
    Ok(EncodingReport {
        bom_paths: files
            .iter()
            .filter(|(_, bytes)| bytes.starts_with(&[0xef, 0xbb, 0xbf]))
            .map(|(path, _)| relative_display(root, path))
            .collect(),
        suspicious: garble_hits(root, &files),
    })
}

pub fn inspect_index(root: &Path, runner: &dyn ProcessRunner) -> Result<Vec<String>, String> {
    let query = |args: &[&str], stdin: Vec<u8>| -> Result<Vec<u8>, String> {
        let mut request = ProcessRequest::new("git", root);
        request.args = args.iter().map(|value| (*value).into()).collect();
        request.stdin = stdin;
        let output = runner.run(&request).map_err(|error| error.to_string())?;
        if output.timed_out || output.code != 0 {
            return Err(format!(
                "Git index query failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(output.stdout)
    };
    let changed_bytes = query(
        &[
            "diff",
            "--cached",
            "--name-only",
            "-z",
            "--diff-filter=ACMR",
        ],
        vec![],
    )?;
    let changed = std::str::from_utf8(&changed_bytes)
        .map_err(|error| error.to_string())?
        .split('\0')
        .filter(|name| !name.is_empty())
        .collect::<BTreeSet<_>>();
    let entries = query(&["ls-files", "--stage", "-z"], vec![])?;
    let entries = std::str::from_utf8(&entries).map_err(|error| error.to_string())?;
    let mut selected = Vec::new();
    for entry in entries.split('\0').filter(|entry| !entry.is_empty()) {
        let (metadata, name) = entry.split_once('\t').ok_or("invalid Git index entry")?;
        let fields: Vec<_> = metadata.split_whitespace().collect();
        if fields.len() != 3 || fields[2] != "0" {
            return Err("unmerged or invalid index entry".into());
        }
        let scoped = TEXT_ROOTS
            .iter()
            .any(|scope| Path::new(name).starts_with(scope));
        if fields[0].starts_with("100")
            && is_text(Path::new(name))
            && (scoped || changed.contains(name))
        {
            selected.push((fields[1], name));
        }
    }
    if selected.is_empty() {
        return Ok(vec![]);
    }
    let input = selected
        .iter()
        .map(|(oid, _)| format!("{oid}\n"))
        .collect::<String>();
    let data = query(&["cat-file", "--batch"], input.into_bytes())?;
    let mut cursor = 0usize;
    let mut hits = Vec::new();
    let garble = Regex::new(r"\?{3,}|\u{fffd}").unwrap();
    for (oid, name) in selected {
        let rest = data.get(cursor..).ok_or("truncated Git blob batch")?;
        let header_length = rest
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or("missing Git blob header")?;
        let header =
            std::str::from_utf8(&rest[..header_length]).map_err(|error| error.to_string())?;
        let fields: Vec<_> = header.split_whitespace().collect();
        if fields.len() != 3 || fields[0] != oid || fields[1] != "blob" {
            return Err("unexpected Git blob response".into());
        }
        let size: usize = fields[2].parse().map_err(|_| "invalid Git blob size")?;
        cursor += header_length + 1;
        let end = cursor.checked_add(size).ok_or("Git blob size overflow")?;
        let bytes = data.get(cursor..end).ok_or("truncated Git blob body")?;
        if data.get(end) != Some(&b'\n') {
            return Err("invalid Git blob terminator".into());
        }
        cursor = end + 1;
        if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            hits.push(format!("{name}: UTF-8 BOM is forbidden in the index"));
        }
        if changed.contains(name) {
            for (index, line) in String::from_utf8_lossy(bytes).lines().enumerate() {
                if garble.is_match(line) {
                    hits.push(format!("{name}:{}: suspicious replacement text", index + 1));
                }
            }
        }
    }
    Ok(hits)
}
