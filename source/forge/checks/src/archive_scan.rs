//! One read-only candidate policy for CLI, structure checks and SessionStart.
use crate::{document_metadata, todo};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArchiveCandidate {
    pub kind: String,
    pub topic: String,
    pub path: PathBuf,
    pub lifecycle: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub evidence: String,
    pub reasons: Vec<String>,
}

fn plain(path: &Path) -> Result<(), String> {
    if crate::persistence::is_link_or_reparse(path).map_err(|e| e.to_string())? {
        return Err(format!(
            "linked archive input is forbidden: {}",
            path.display()
        ));
    }
    Ok(())
}

fn text(path: &Path) -> Result<String, String> {
    plain(path)?;
    fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

pub fn fields(path: &Path) -> Result<BTreeMap<String, String>, String> {
    document_metadata::parse(&text(path)?).map_err(|e| format!("{}: {e}", path.display()))
}

fn children(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    plain(path)?;
    let mut result = Vec::new();
    for item in fs::read_dir(path).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let name = item.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || matches!(name.as_str(), "evidence" | "artifacts") {
            continue;
        }
        plain(&item.path())?;
        result.push(item.path());
    }
    result.sort();
    Ok(result)
}

fn legacy_done(text: &str) -> bool {
    Regex::new(r"(?i)(?:状态|status)\s*[:：]\s*(?:已完成|已验收|已解决|done|accepted|resolved)")
        .unwrap()
        .is_match(&text.lines().take(30).collect::<Vec<_>>().join("\n"))
}

fn package_state(path: &Path, kind: &str) -> Result<Option<(String, String)>, String> {
    let records = if kind == "delivery" {
        children(path)?
            .into_iter()
            .filter(|p| {
                p.is_file()
                    && p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("requirements_"))
                    && p.extension().is_some_and(|e| e == "md")
            })
            .collect::<Vec<_>>()
    } else if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.join("README.md").is_file() {
        vec![path.join("README.md")]
    } else {
        Vec::new()
    };
    let mut states = Vec::new();
    for record in &records {
        let fields = fields(record)?;
        if !fields.contains_key("lifecycle") {
            if kind == "bug" && legacy_done(&text(record)?) {
                return Ok(Some(("unclassified".into(), "legacy".into())));
            }
            return Ok(None);
        }
        if !document_metadata::retired(&fields) {
            return Ok(None);
        }
        states.push(fields["lifecycle"].to_ascii_lowercase());
    }
    if !states.is_empty() {
        if states.iter().all(|state| state == "archived") {
            return Ok(None);
        }
        let lifecycle = if states.iter().all(|s| s == "superseded") {
            "superseded"
        } else {
            "completed"
        };
        return Ok(Some((lifecycle.into(), "lifecycle".into())));
    }
    // No formal cards: retain old signals as explicitly unclassified evidence.
    let legacy = if kind == "delivery" {
        path.join("acceptance.md")
    } else if path.is_file() {
        path.to_path_buf()
    } else {
        path.join("README.md")
    };
    if legacy.is_file() {
        let payload = text(&legacy)?;
        let meta = document_metadata::parse(&payload)?;
        if !meta.contains_key("lifecycle") && legacy_done(&payload) {
            return Ok(Some(("unclassified".into(), "legacy".into())));
        }
    }
    Ok(None)
}

pub fn scan(root: &Path) -> Result<Vec<ArchiveCandidate>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    plain(root)?;
    let todos = todo::inspect(root);
    if !todos.errors.is_empty() {
        return Err(format!(
            "TODO validation failed: {}",
            serde_json::to_string(&todos.errors).map_err(|e| e.to_string())?
        ));
    }
    let mut values = Vec::new();
    let doc = root.join("doc");
    if doc.exists() {
        plain(&doc)?;
    }
    let readme = doc.join("README.md");
    let layout = if readme.exists() {
        fields(&readme)?
            .get("delivery_layout")
            .cloned()
            .unwrap_or_else(|| "flat".into())
    } else {
        "flat".into()
    };
    if !matches!(layout.as_str(), "flat" | "milestone") {
        return Err("unknown delivery_layout".into());
    }
    let delivery = doc.join("1_delivery");
    let mut packages = Vec::new();
    for path in children(&delivery)? {
        if !path.is_dir() {
            continue;
        }
        if layout == "milestone" {
            for topic in children(&path)? {
                if topic.is_dir() {
                    packages.push((topic, "delivery"));
                }
            }
        } else {
            packages.push((path, "delivery"));
        }
    }
    for path in children(&doc.join("2_bugs"))? {
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("BUG-"))
            && (path.is_dir() || path.extension().is_some_and(|e| e == "md"))
        {
            packages.push((path, "bug"));
        }
    }
    for (path, kind) in packages {
        let Some((lifecycle, evidence)) = package_state(&path, kind)? else {
            continue;
        };
        if !todo::blockers(&todos, &path)?.is_empty() {
            continue;
        }
        let topic = path.file_name().unwrap().to_string_lossy().to_string();
        let relative = if kind == "delivery" {
            path.strip_prefix(&delivery).map_err(|e| e.to_string())?
        } else {
            path.strip_prefix(doc.join("2_bugs"))
                .map_err(|e| e.to_string())?
        };
        let target = doc
            .join("4_archive")
            .join(if kind == "delivery" {
                "delivery"
            } else {
                "bugs"
            })
            .join(relative);
        let reasons = if evidence == "legacy" {
            vec!["legacy completion signal; verify before moving".into()]
        } else {
            vec!["formal records retired; original TODOs closed; follow-ups remain independently tracked".into()]
        };
        values.push(ArchiveCandidate {
            kind: kind.into(),
            topic,
            source: path.clone(),
            path,
            target,
            lifecycle,
            evidence,
            reasons,
        });
    }
    values.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(values)
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_archive_scan.rs"]
mod tests;
