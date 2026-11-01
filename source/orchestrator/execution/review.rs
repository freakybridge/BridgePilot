//! A read-only report can be delivered only after an explicit, bound review.
use crate::{
    Result, digest,
    domain::{RunState, TaskState},
    store::{Store, ordinary},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Component, Path},
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEvidence {
    pub relative_path: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatGrouping {
    pub thread_id: String,
    pub project_id: String,
    pub cwd: String,
    pub source: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportReview {
    pub run: String,
    pub result_sha256: String,
    pub source: String,
    pub note: String,
    pub files: Vec<FileEvidence>,
    /// Empty selects all receipts. Explicit selection preserves excluded history.
    #[serde(default)]
    pub command_ids: Vec<String>,
    #[serde(default)]
    pub desktop_grouping: Option<ChatGrouping>,
}

pub fn review(store: &Store, expected: u64, input: ReportReview) -> Result<Value> {
    if input.source.trim().is_empty()
        || input.note.trim().is_empty()
        || input.files.is_empty()
        || input.files.len() > 16
    {
        return Err("actual review source, note and 1..16 file evidence entries required".into());
    }
    let snapshot = store.read()?;
    if snapshot.revision != expected {
        return Err("archive revision changed before review".into());
    }
    let run = snapshot.state.runs.get(&input.run).ok_or("unknown run")?;
    let task = &snapshot.state.tasks[&run.task];
    if run.machine != store.machine()?
        || run.state != RunState::Completed
        || task.state != TaskState::Reviewing
        || run.spec_hash != task.spec_hash
        || task.input.authorization.allow_write
        || task.input.authorization.allow_publish
    {
        return Err(
            "only current completed read-only report tasks can be delivered by this review".into(),
        );
    }
    let result = run.result.as_ref().ok_or("missing result receipt")?;
    if digest(&serde_json::to_vec(result).map_err(|e| e.to_string())?) != input.result_sha256 {
        return Err("result receipt digest changed".into());
    }
    let commands = result["tool_receipts"]
        .as_array()
        .ok_or("actual command receipts required for report review")?;
    let mut selected = std::collections::BTreeSet::new();
    for id in &input.command_ids {
        if !selected.insert(id.as_str()) || commands.iter().filter(|c| c["id"] == *id).count() != 1
        {
            return Err("review command selectors must name unique actual receipts".into());
        }
    }
    let selected_commands: Vec<_> = commands
        .iter()
        .filter(|c| {
            input.command_ids.is_empty() || c["id"].as_str().is_some_and(|id| selected.contains(id))
        })
        .collect();
    if selected_commands.is_empty()
        || selected_commands
            .iter()
            .any(|c| c["exit_code"] != 0 || c["status"] != "completed")
    {
        return Err("report includes missing or failed command evidence".into());
    }
    let root = fs::canonicalize(&run.binding.root).map_err(|e| e.to_string())?;
    ordinary(&root)?;
    let desktop_grouping = input.desktop_grouping.clone().or_else(|| {
        crate::grouping::observed_proof(&snapshot.state, &input.run)
            .and_then(|v| serde_json::from_value(v).ok())
    });
    if let Some(grouping) = &desktop_grouping {
        if grouping.source.trim().is_empty()
            || Some(grouping.thread_id.as_str()) != run.thread_id.as_deref()
            || grouping.project_id != run.binding.desktop_project_id
            || fs::canonicalize(&grouping.cwd).map_err(|e| e.to_string())? != root
        {
            return Err("actual native grouping readback does not match this run/project".into());
        }
    }
    for command in &selected_commands {
        let cwd = command["cwd"]
            .as_str()
            .ok_or("command evidence omitted cwd")?;
        if fs::canonicalize(cwd).map_err(|e| e.to_string())? != root
            || command["output_truncated"] != false
        {
            return Err("command evidence is outside this project or was truncated".into());
        }
    }
    let mut evidence = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for file in &input.files {
        let relative = Path::new(&file.relative_path);
        if file.relative_path.is_empty()
            || !seen.insert(&file.relative_path)
            || relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("report evidence must name unique project-relative files".into());
        }
        let path = root.join(relative);
        ordinary(&path)?;
        let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
        if !canonical.starts_with(&root)
            || !canonical.is_file()
            || fs::metadata(&canonical).map_err(|e| e.to_string())?.len() > 1024 * 1024
        {
            return Err("report evidence is not a bounded project file".into());
        }
        let hash = digest(&fs::read(&canonical).map_err(|e| e.to_string())?);
        if hash != file.sha256 {
            return Err("project evidence changed since report review".into());
        }
        evidence.push(json!({"relative_path":file.relative_path,"sha256":hash}));
    }
    let selected_ids: Vec<_> = selected_commands.iter().map(|c| c["id"].clone()).collect();
    let excluded_ids: Vec<_> = commands
        .iter()
        .filter(|c| !selected_commands.contains(c))
        .map(|c| c["id"].clone())
        .collect();
    let grouping_verified = desktop_grouping.is_some();
    let grouping=desktop_grouping.as_ref().map(|v|json!({"thread_id":v.thread_id,"project_id":v.project_id,"cwd":v.cwd,"source":v.source}));
    let review = json!({"source":input.source,"note":input.note,"result_sha256":input.result_sha256,"task_spec":task.spec_hash,"files":evidence,"selected_command_ids":selected_ids,"excluded_command_ids":excluded_ids,"desktop_grouping_verified":grouping_verified,"desktop_grouping":grouping,"publication_verified":false});
    let review_event = crate::tracking::Event::native(
        &task,
        Some(&input.run),
        &input.result_sha256,
        if grouping_verified {
            "report_delivered"
        } else {
            "report_reviewed"
        },
        if grouping_verified {
            "只读报告和证据已完成交付核验".into()
        } else {
            "报告证据已复核，原生归类仍待核验".into()
        },
        (!grouping_verified).then(|| "原生项目归类缺少有效读回".into()),
        input.source.clone(),
        crate::tracking::receipt_reference(&review)?,
    );
    let (snapshot, _) = store.transact_with_sql(expected, |s, tx| {
        let r = s.runs.get_mut(&input.run).ok_or("unknown run")?;
        r.result.as_mut().ok_or("missing result")?["review"] = review.clone();
        if grouping_verified {
            s.tasks.get_mut(&r.task).unwrap().state = TaskState::ReportDelivered;
        }
        crate::tracking::append(s, tx, &review_event)?;
        Ok(())
    })?;
    Ok(
        json!({"schema":1,"status":if grouping_verified {"report_delivered"}else{"report_reviewed"},"revision":snapshot.revision,"run":input.run,"review":review,"publication_verified":false}),
    )
}
