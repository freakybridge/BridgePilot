use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const LAYERS: [&str; 6] = [
    "0_architecture",
    "1_delivery",
    "2_bugs",
    "3_reference",
    "4_archive",
    "5_project_knowledgebase",
];
const LIFECYCLES: [&str; 4] = ["active", "completed", "superseded", "archived"];
const VALIDATION: [&str; 5] = [
    "not_started",
    "in_progress",
    "awaiting_validation",
    "awaiting_user_acceptance",
    "verified",
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StructureReport {
    pub errors: Vec<Finding>,
    pub advisories: Vec<Finding>,
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn frontmatter(path: &Path) -> BTreeMap<String, String> {
    crate::archive_scan::fields(path).unwrap_or_default()
}

fn lifecycle(path: &Path, root: &Path, report: &mut StructureReport) -> Option<String> {
    let fields = match crate::archive_scan::fields(path) {
        Ok(fields) => fields,
        Err(error) => {
            report.errors.push(Finding {
                code: "invalid-document-metadata".into(),
                path: relative(root, path),
                message: error,
            });
            return None;
        }
    };
    let value = fields.get("lifecycle")?.to_lowercase();
    let rel = relative(root, path);
    if !LIFECYCLES.contains(&value.as_str()) {
        report.errors.push(Finding {
            code: "invalid-document-lifecycle".into(),
            path: rel,
            message: format!("lifecycle 不是允许值：{value}"),
        });
        return None;
    }
    let validation = fields
        .get("validation_status")
        .map(|value| value.to_lowercase())
        .unwrap_or_default();
    if !VALIDATION.contains(&validation.as_str()) {
        report.errors.push(Finding {
            code: "invalid-document-validation-status".into(),
            path: rel.clone(),
            message: "声明 lifecycle 的文档必须同时提供合法 validation_status".into(),
        });
    }
    if value == "completed" && validation != "verified" {
        report.errors.push(Finding {
            code: "completed-document-not-verified".into(),
            path: rel.clone(),
            message: "lifecycle: completed 必须同时为 validation_status: verified".into(),
        });
    }
    if value == "superseded" && !fields.contains_key("superseded_by") {
        report.errors.push(Finding {
            code: "superseded-document-missing-target".into(),
            path: rel,
            message: "lifecycle: superseded 必须同时提供 superseded_by".into(),
        });
    }
    Some(value)
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|value| value.file_type().is_symlink())
        .unwrap_or(false)
}

fn visible_dirs(path: &Path) -> Vec<PathBuf> {
    let mut result = fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|item| {
            item.is_dir()
                && !item
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .collect::<Vec<_>>();
    result.sort();
    result
}

fn contains_markdown(path: &Path) -> bool {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .any(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("md")
        })
}

fn active_doc(path: &Path) -> bool {
    frontmatter(path).get("lifecycle").map(String::as_str) == Some("active")
}

fn dead_references(doc: &Path, root: &Path, report: &mut StructureReport) {
    let link = Regex::new(r"(?m)(?P<prefix>!?)\[[^\]\n]+\]\((?P<target>[^)\n]+)\)").unwrap();
    for entry in WalkDir::new(doc)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("md")
            || relative(root, entry.path()).starts_with("doc/4_archive/")
            || !active_doc(entry.path())
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        for captures in link.captures_iter(&text) {
            if &captures["prefix"] == "!" {
                continue;
            }
            let raw = captures["target"].split('#').next().unwrap_or("").trim();
            if raw.is_empty() || raw.contains("://") || raw.starts_with("mailto:") {
                continue;
            }
            if !entry.path().parent().unwrap_or(doc).join(raw).exists() {
                report.errors.push(Finding {
                    code: "dead-doc-reference".into(),
                    path: relative(root, entry.path()),
                    message: format!("活动文档引用不存在：{raw}"),
                });
            }
        }
    }
}

/// Full worktree audit. Submission and record-only callers use explicit modes.
pub fn inspect(root: &Path) -> StructureReport {
    let mut report = inspect_documents(root);
    if root.is_dir() && !is_link(root) {
        let layout = crate::artifact_layout::worktree(root);
        report.errors.extend(layout.errors);
        report.advisories.extend(layout.advisories);
    }
    report
}

pub fn pre_commit(root: &Path) -> StructureReport {
    let mut report = inspect_documents(root);
    if root.is_dir() && !is_link(root) {
        let layout = crate::artifact_layout::pre_commit(root);
        report.errors.extend(layout.errors);
        report.advisories.extend(layout.advisories);
    }
    report
}

/// Preserve the lightweight document-only contract used by release preparation.
pub fn inspect_documents(root: &Path) -> StructureReport {
    let mut report = StructureReport::default();
    if !root.is_dir() || is_link(root) {
        report.errors.push(Finding {
            code: "unsafe-project-root".into(),
            path: root.display().to_string(),
            message: "项目根必须是普通目录，禁止链接".into(),
        });
        return report;
    }
    let doc = root.join("doc");
    if !doc.exists() {
        return report;
    }
    if !doc.is_dir() || is_link(&doc) {
        report.errors.push(Finding {
            code: "unsafe-doc-root".into(),
            path: "doc".into(),
            message: "doc 必须是项目内普通目录".into(),
        });
        return report;
    }
    let readme_path = doc.join("README.md");
    let Ok(readme) = fs::read_to_string(&readme_path) else {
        report.errors.push(Finding {
            code: "missing-doc-index".into(),
            path: "doc/README.md".into(),
            message: "doc/README.md 是唯一总索引".into(),
        });
        return report;
    };
    let layout = Regex::new(r"(?m)^delivery_layout:\s*(flat|milestone)\s*$")
        .unwrap()
        .captures(&readme)
        .map(|value| value[1].to_string());
    if layout.is_none() {
        report.errors.push(Finding {
            code: "missing-delivery-layout".into(),
            path: "doc/README.md".into(),
            message: "缺少 delivery_layout: flat|milestone".into(),
        });
    }
    for entry in fs::read_dir(&doc)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
    {
        let name = entry.file_name().to_string_lossy().to_string();
        let directory_instructions =
            name == "AGENTS.md" && entry.file_type().is_ok_and(|file_type| file_type.is_file());
        if name != "README.md" && !LAYERS.contains(&name.as_str()) && !directory_instructions {
            report.errors.push(Finding {
                code: "unexpected-doc-entry".into(),
                path: relative(root, &entry.path()),
                message: "doc/ 顶层只允许六层目录、README.md 和普通文件 AGENTS.md".into(),
            });
        }
    }
    let delivery = doc.join("1_delivery");
    let mut topics = if delivery.is_dir() {
        visible_dirs(&delivery)
    } else {
        Vec::new()
    };
    if layout.as_deref() == Some("milestone") {
        topics = topics
            .into_iter()
            .flat_map(|item| visible_dirs(&item))
            .collect()
    }
    for topic in topics {
        if is_link(&topic) {
            report.errors.push(Finding {
                code: "unsafe-doc-entry".into(),
                path: relative(root, &topic),
                message: "doc 内禁止链接".into(),
            });
            continue;
        }
        if !contains_markdown(&topic) {
            continue;
        }
        let mut states = Vec::new();
        for entry in fs::read_dir(&topic)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("requirements_"))
                && path.extension().and_then(|value| value.to_str()) == Some("md")
                && let Some(state) = lifecycle(&path, root, &mut report)
            {
                states.push(state)
            }
        }
        if states.iter().any(|state| state == "active") {
            let name = topic.file_name().unwrap().to_string_lossy();
            if !readme.contains(name.as_ref()) {
                report.errors.push(Finding {
                    code: "unindexed-delivery-topic".into(),
                    path: relative(root, &topic),
                    message: format!("活跃 delivery topic 未进入 doc/README.md：{name}"),
                });
            }
        }
    }
    report.errors.extend(crate::todo::inspect(root).errors);
    if report.errors.is_empty() {
        match crate::archive_scan::scan(root) {
            Ok(candidates) => {
                for candidate in candidates {
                    report.advisories.push(Finding {
                        code: if candidate.evidence == "legacy" {
                            "legacy-archive-candidate".into()
                        } else {
                            format!("{}-archive-candidate", candidate.kind)
                        },
                        path: relative(root, &candidate.path),
                        message: format!(
                            "{}; review with $archive-scan",
                            candidate.reasons.join("; ")
                        ),
                    });
                }
            }
            Err(error) => report.errors.push(Finding {
                code: "archive-scan-failed".into(),
                path: "doc".into(),
                message: error,
            }),
        }
    }
    dead_references(&doc, root, &mut report);
    report
}
