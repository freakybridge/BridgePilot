use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

const RETIRED: [&str; 6] = [
    "skill-routing.json",
    "clarify_reminder.py",
    "focus_reminder.py",
    "project_memory_writer.py",
    "memory_rebuild_index.py",
    "memory_lint.py",
];

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SkillReport {
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
}

fn frontmatter(path: &Path) -> Result<(BTreeMap<String, String>, String), Vec<String>> {
    let bytes = fs::read(path).map_err(|error| vec![format!("cannot read file: {error}")])?;
    let mut issues = Vec::new();
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        issues.push("starts with UTF-8 BOM".into())
    }
    let text =
        String::from_utf8(bytes).map_err(|error| vec![format!("not valid UTF-8: {error}")])?;
    let lines = text.lines().collect::<Vec<_>>();
    if lines.first().map(|line| line.trim()) != Some("---") {
        return Err(vec!["missing opening frontmatter line ---".into()]);
    }
    let Some(end) = lines
        .iter()
        .skip(1)
        .position(|line| line.trim() == "---")
        .map(|x| x + 1)
    else {
        return Err(vec!["missing closing frontmatter line ---".into()]);
    };
    let mut fields = BTreeMap::new();
    for line in &lines[1..end] {
        if line.trim().is_empty()
            || line.trim_start().starts_with('#')
            || line.starts_with(char::is_whitespace)
        {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        } else {
            issues.push(format!("invalid frontmatter line: {line}"))
        }
    }
    if issues.is_empty() {
        Ok((fields, text))
    } else {
        Err(issues)
    }
}

fn validate_skill(path: &Path, root: &Path) -> Vec<String> {
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let (meta, text) = match frontmatter(path) {
        Ok(value) => value,
        Err(issues) => {
            return issues
                .into_iter()
                .map(|issue| format!("{rel}: {issue}"))
                .collect();
        }
    };
    let mut issues = Vec::new();
    let expected = path
        .parent()
        .and_then(Path::file_name)
        .unwrap()
        .to_string_lossy();
    if meta.get("name").map(String::as_str) != Some(expected.as_ref()) {
        issues.push(format!("name must equal skill directory: {expected}"))
    }
    let description = meta.get("description").map(|x| x.trim()).unwrap_or("");
    if description.is_empty() {
        issues.push("missing description".into())
    } else if description.chars().count() > 500 {
        issues.push("description exceeds 500 characters".into())
    }
    if meta.contains_key("user_invocable") != meta.contains_key("argument") {
        issues.push("user_invocable and argument must appear together".into())
    }
    if text.lines().count() > 120 {
        issues.push(format!(
            "SKILL.md entry exceeds 120 lines ({})",
            text.lines().count()
        ))
    }
    let h2 = text.lines().filter(|line| line.starts_with("## ")).count();
    if h2 > 8 {
        issues.push(format!("SKILL.md entry exceeds 8 H2 sections ({h2})"))
    }
    let lower = text.to_lowercase();
    for retired in RETIRED {
        if lower.contains(&retired.to_lowercase()) {
            issues.push(format!("retired runtime asset must not appear: {retired}"))
        }
    }
    let link = Regex::new(r"\[[^\]]+\]\((references/[^)#]+\.md)(?:#[^)]*)?\)").unwrap();
    let linked = link
        .captures_iter(&text)
        .map(|item| item[1].replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    for target in &linked {
        if target.split('/').count() != 2 {
            issues.push(format!(
                "reference nesting must stay one level deep: {target}"
            ))
        } else if !path.parent().unwrap().join(target).is_file() {
            issues.push(format!("dead markdown reference: {target}"))
        }
    }
    let references = path.parent().unwrap().join("references");
    if references.is_dir() {
        for entry in fs::read_dir(references)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            let child = entry.path();
            if child.extension().and_then(|value| value.to_str()) == Some("md") {
                let target = format!(
                    "references/{}",
                    child.file_name().unwrap().to_string_lossy()
                );
                if !linked.contains(&target) {
                    issues.push(format!("orphan markdown reference: {target}"))
                }
            }
        }
    }
    issues
        .into_iter()
        .map(|issue| format!("{rel}: {issue}"))
        .collect()
}

pub fn validate_tree(skills: &Path) -> SkillReport {
    let mut report = SkillReport::default();
    if !skills.is_dir() {
        return report;
    }
    let root = skills.parent().unwrap_or(skills);
    let mut catalog = 0usize;
    let mut entries = fs::read_dir(skills)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();
    for entry in entries {
        let relative = entry.strip_prefix(root).unwrap_or(&entry).to_string_lossy();
        if !entry.is_dir()
            || fs::symlink_metadata(&entry).is_ok_and(|meta| meta.file_type().is_symlink())
        {
            report.issues.push(format!(
                "{relative}: skill root may contain ordinary directories only"
            ));
            continue;
        }
        let skill = entry.join("SKILL.md");
        if !skill.is_file() {
            report.issues.push(format!("{relative}: missing SKILL.md"));
            continue;
        }
        report.issues.extend(validate_skill(&skill, root));
        if let Ok((meta, _)) = frontmatter(&skill) {
            catalog += meta
                .get("description")
                .map(|x| x.chars().count())
                .unwrap_or(0)
        }
        for child in WalkDir::new(&entry)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if child.file_type().is_symlink() {
                report.issues.push(format!(
                    "{}: reparse entries are not allowed",
                    child.path().display()
                ))
            }
        }
    }
    if catalog > 4_000 {
        report.issues.push(format!(
            "catalog descriptions exceed 4000 characters ({catalog})"
        ))
    }
    report
}
