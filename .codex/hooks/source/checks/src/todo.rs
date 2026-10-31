//! Read-only TODO checks. Agents own authorized additions and acceptance updates.
use crate::project_structure::Finding;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn contract() -> Value {
    serde_json::from_str(include_str!("todo_contract.json")).expect("compiled TODO contract")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TodoEntry {
    pub id: String,
    pub date: String,
    pub content: String,
    pub status: String,
    pub origin: String,
    pub references: String,
    #[serde(skip)]
    pub targets: Vec<PathBuf>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct TodoReport {
    pub errors: Vec<Finding>,
    pub entries: Vec<TodoEntry>,
}

fn cells(line: &str) -> Vec<String> {
    let line = line.trim().trim_start_matches('|').trim_end_matches('|');
    let mut result = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            if c != '|' {
                cell.push('\\');
            }
            cell.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '|' {
            result.push(cell.trim().to_string());
            cell.clear();
        } else {
            cell.push(c);
        }
    }
    if escaped {
        cell.push('\\');
    }
    result.push(cell.trim().to_string());
    result
}

fn issue(report: &mut TodoReport, code: &str, path: &str, message: impl Into<String>) {
    report.errors.push(Finding {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    });
}

fn real_path(root: &Path, path: &Path) -> Result<(), String> {
    let mut cursor = Some(path);
    while let Some(p) = cursor {
        if crate::persistence::is_link_or_reparse(p).map_err(|e| e.to_string())? {
            return Err(format!("linked TODO path is forbidden: {}", p.display()));
        }
        if p == root {
            return Ok(());
        }
        cursor = p.parent();
    }
    Err("TODO path escapes project".into())
}

fn resolve(root: &Path, base: &Path, raw: &str) -> Result<Option<PathBuf>, String> {
    let target = raw
        .trim()
        .trim_matches(['<', '>'])
        .split('#')
        .next()
        .unwrap_or("");
    if target.contains("://") || target.starts_with("mailto:") {
        return Ok(None);
    }
    let rel = Path::new(target);
    if target.is_empty()
        || rel.is_absolute()
        || rel
            .components()
            .any(|p| matches!(p, Component::Prefix(_) | Component::RootDir))
    {
        return Err("TODO reference must be a project-relative file".into());
    }
    let mut path = base.to_path_buf();
    for part in rel.components() {
        match part {
            Component::Normal(name) => path.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                path.pop();
            }
            _ => return Err("unsafe TODO reference".into()),
        }
        if !path.starts_with(root) {
            return Err("TODO reference escapes project".into());
        }
    }
    if !path.starts_with(root) {
        return Err("TODO reference escapes project".into());
    }
    real_path(root, &path)?;
    if !path.is_file() {
        return Err(format!("TODO reference is missing: {target}"));
    }
    let resolved = path.canonicalize().map_err(|e| e.to_string())?;
    if !resolved.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
        return Err("TODO reference escapes project".into());
    }
    real_path(root, &path)?;
    Ok(Some(resolved))
}

pub fn inspect(root: &Path) -> TodoReport {
    let cfg = contract();
    let relative = cfg["path"].as_str().unwrap();
    let file = root.join(relative);
    let mut report = TodoReport::default();
    match fs::symlink_metadata(&file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return report,
        Err(e) => {
            issue(&mut report, "unreadable-todo", relative, e.to_string());
            return report;
        }
        Ok(_) => {}
    }
    if let Err(e) = real_path(root, &file) {
        issue(&mut report, "unsafe-todo", relative, e);
        return report;
    }
    let bytes = match fs::read(&file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return report,
        Err(e) => {
            issue(&mut report, "unreadable-todo", relative, e.to_string());
            return report;
        }
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) if !text.starts_with('\u{feff}') => text,
        _ => {
            issue(
                &mut report,
                "invalid-todo-encoding",
                relative,
                "TODO must be UTF-8 without BOM",
            );
            return report;
        }
    };
    let expected: Vec<_> = cfg["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect();
    let id_re = Regex::new(&format!(
        "^{}([0-9]{{3,}})$",
        regex::escape(cfg["id_prefix"].as_str().unwrap())
    ))
    .unwrap();
    let links = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    let mut seen = BTreeSet::new();
    let mut previous = 0;
    let mut header = false;
    let mut fenced = false;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if !line.starts_with('|') {
            // Markdown permits omission of the initial pipe. A damaged TODO row
            // must never disappear from the set of archive blockers.
            if line.starts_with(cfg["id_prefix"].as_str().unwrap()) && !line.contains('|') {
                issue(
                    &mut report,
                    "invalid-todo-row",
                    relative,
                    format!("line {} is an incomplete TODO row", index + 1),
                );
                continue;
            }
            if !line.contains('|')
                || !(line.starts_with(cfg["id_prefix"].as_str().unwrap())
                    || line.starts_with(&expected[0])
                    || line.starts_with('-')
                    || line.starts_with(':'))
            {
                continue;
            }
        }
        let row = cells(line);
        if row == expected {
            if header {
                issue(
                    &mut report,
                    "duplicate-todo-table",
                    relative,
                    "multiple TODO tables",
                );
            }
            header = true;
            continue;
        }
        if row
            .iter()
            .all(|v| !v.is_empty() && v.chars().all(|c| c == '-' || c == ':' || c.is_whitespace()))
        {
            continue;
        }
        if !header {
            if row
                .first()
                .is_some_and(|v| v == "编号" || v.starts_with("TODO-"))
            {
                issue(
                    &mut report,
                    "legacy-todo-format",
                    relative,
                    "TODO requires six contract columns; explicit migration required, no files changed",
                );
            }
            continue;
        }
        if row.len() != expected.len() {
            issue(
                &mut report,
                "invalid-todo-row",
                relative,
                format!("line {} has {} columns", index + 1, row.len()),
            );
            continue;
        }
        let mut entry = TodoEntry {
            id: row[0].clone(),
            date: row[1].clone(),
            content: row[2].clone(),
            status: row[3].clone(),
            origin: row[4].clone(),
            references: row[5].clone(),
            targets: Vec::new(),
        };
        let number = id_re
            .captures(&entry.id)
            .and_then(|m| m[1].parse::<u64>().ok());
        if number.is_none_or(|n| n <= previous) || !seen.insert(entry.id.clone()) {
            issue(
                &mut report,
                "invalid-todo-id",
                relative,
                format!("{}: IDs must be unique and increasing", entry.id),
            );
        }
        if let Some(n) = number {
            previous = n;
        }
        if chrono::NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d")
            .map_or(true, |date| date.to_string() != entry.date)
            || entry.content.is_empty()
        {
            issue(
                &mut report,
                "invalid-todo-content",
                relative,
                format!("{}: missing content or invalid date", entry.id),
            );
        }
        if cfg["states"].get(&entry.status).is_none() {
            issue(
                &mut report,
                "invalid-todo-state",
                relative,
                format!("{}: unknown state {}", entry.id, entry.status),
            );
        }
        if cfg["origins"].get(&entry.origin).is_none() {
            issue(
                &mut report,
                "invalid-todo-origin",
                relative,
                format!("{}: unknown origin {}", entry.id, entry.origin),
            );
        }
        let mut evidence = false;
        let evidence_id = Regex::new(&format!(
            r"(?:^|[^A-Za-z0-9_-]){}(?:$|[^A-Za-z0-9_-])",
            regex::escape(&entry.id)
        ))
        .unwrap();
        for link in links.captures_iter(&entry.references) {
            match resolve(root, file.parent().unwrap(), &link[2]) {
                Ok(Some(path)) => {
                    let role = &link[1];
                    if (entry.status == "已完成" && role.starts_with("验收"))
                        || (entry.status == "已取消" && role.starts_with("取消"))
                    {
                        if let Ok(text) = fs::read_to_string(&path) {
                            evidence |= evidence_id.is_match(&text)
                                && if entry.status == "已取消" {
                                    text.contains("取消")
                                } else {
                                    crate::document_metadata::parse(&text).is_ok_and(|m| {
                                        crate::document_metadata::retired(&m)
                                            && m.get("validation_status")
                                                .is_some_and(|s| s == "verified")
                                    })
                                };
                        }
                    }
                    entry.targets.push(path);
                }
                Ok(None) => {}
                Err(e) => issue(
                    &mut report,
                    "invalid-todo-reference",
                    relative,
                    format!("{}: {e}", entry.id),
                ),
            }
        }
        if entry.origin == "原需求" && entry.status != "待确认" && entry.targets.is_empty() {
            issue(
                &mut report,
                "missing-todo-requirement",
                relative,
                format!("{}: original requirement needs a local source", entry.id),
            );
        }
        if entry.origin == "后续事项" && entry.targets.is_empty() {
            issue(
                &mut report,
                "missing-todo-source",
                relative,
                format!(
                    "{}: follow-up needs a local source and is tracked by this stable TODO ID",
                    entry.id
                ),
            );
        }
        if matches!(entry.status.as_str(), "已完成" | "已取消") && !evidence {
            issue(
                &mut report,
                "missing-todo-closure-evidence",
                relative,
                format!(
                    "{}: closure requires a labeled local evidence link",
                    entry.id
                ),
            );
        }
        report.entries.push(entry);
    }
    if !header {
        issue(
            &mut report,
            "missing-todo-table",
            relative,
            "TODO contract table is missing; explicit migration required",
        );
    }
    report
}

pub fn blockers(report: &TodoReport, path: &Path) -> Result<Vec<String>, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    Ok(report
        .entries
        .iter()
        .filter(|entry| {
            entry.origin == "原需求"
                && !matches!(entry.status.as_str(), "已完成" | "已取消")
                && entry
                    .targets
                    .iter()
                    .any(|target| target == &path || target.starts_with(&path))
        })
        .map(|entry| entry.id.clone())
        .collect())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_todo.rs"]
mod tests;
