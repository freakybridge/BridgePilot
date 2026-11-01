//! A deterministic, read-only task view. Every displayed cell is produced here,
//! rather than reconstructed from chat history by the caller.
use crate::{
    Result, database,
    domain::*,
    store::Store,
    tracking::{ExecutionMode, HoldScope, TaskTracking},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const COLUMNS: [&str; 6] = ["序号", "项目", "任务", "阶段", "进展", "阻点"];

#[derive(Debug, Serialize)]
pub struct Row {
    pub number: u64,
    pub task_id: String,
    pub project_id: String,
    pub project: String,
    pub task: String,
    pub phase: String,
    pub progress: String,
    pub blocker: String,
    pub source: String,
    pub thread_id: Option<String>,
    pub observed_at: Option<u64>,
    #[serde(skip)]
    rank: u8,
}

fn phase(t: &Task, metadata: Option<&TaskTracking>) -> (u8, String) {
    let (mut rank, label) = match t.state {
        TaskState::Unknown => (0, "结果未知"),
        TaskState::Blocked => (1, "受阻"),
        TaskState::AwaitingConfirm => (2, "待确认"),
        TaskState::Pausing => (3, "暂停中"),
        TaskState::Running => (4, "执行中"),
        TaskState::Reviewing => (5, "复核中"),
        TaskState::AwaitingRelease => (6, "待发布"),
        TaskState::Ready => (7, "待执行"),
        TaskState::Paused => (8, "已暂停"),
        TaskState::Released => (9, "已发布"),
        TaskState::ReportDelivered => (9, "报告已交付"),
        TaskState::Cancelled => (9, "已取消"),
    };
    let mut label = label.to_owned();
    if let Some(m) = metadata {
        if m.mode == ExecutionMode::Observed
            && !t.state.delivered()
            && !m.hold.as_ref().is_some_and(|h| h.scope == HoldScope::Task)
        {
            (rank, label) = if m.observation_error.is_some() {
                (1, "观察受限·保留最后读回".into())
            } else if m
                .latest
                .as_ref()
                .is_some_and(|p| p.spec_hash == t.spec_hash && p.kind == "goal_observed_active")
            {
                (4, "托管目标进行中·观察受限".into())
            } else {
                match m.observation.as_ref().map(|o| o.status.as_str()) {
                    Some("inProgress") => (4, "观察执行中".into()),
                    Some("completed" | "interrupted" | "failed" | "idle") => {
                        (5, "观察结束·待核验".into())
                    }
                    Some("unknown") => (0, "观察受限·当前状态未核实".into()),
                    _ => (7, "已关联对话·待观察".into()),
                }
            };
        }
        if m.publication_hold.is_some()
            || m.hold
                .as_ref()
                .is_some_and(|h| h.scope == HoldScope::Publication)
        {
            label.push_str("·发布暂缓");
        }
    }
    (rank, label)
}

fn latest_run<'a>(s: &'a State, t: &Task) -> Option<&'a Run> {
    s.runs
        .values()
        .filter(|r| r.task == t.id && r.spec_hash == t.spec_hash)
        .max_by_key(|r| r.ordinal)
}

fn progress(s: &State, t: &Task, m: Option<&TaskTracking>) -> String {
    if let Some(p) = m
        .and_then(|m| m.latest.as_ref())
        .filter(|p| p.spec_hash == t.spec_hash)
    {
        return p.summary.clone();
    }
    if let Some(run) = latest_run(s, t) {
        return match run.state {
            RunState::Completed => "执行回合已结束，交付仍按核验结果判定".into(),
            RunState::Interrupted => "上次执行已中断，保留原执行记录".into(),
            RunState::Failed => "上次执行失败，等待处理实际错误".into(),
            RunState::Unknown => "执行结果尚未核销，禁止重复派发".into(),
            RunState::Running => "执行器已记录启动，尚无后续事件".into(),
            RunState::Pausing => "已请求停止，等待实际停止回执".into(),
            _ => "执行准备中，尚未记录模型回合启动".into(),
        };
    }
    "尚无当前任务修订的执行事件".into()
}

fn blocker(
    s: &State,
    t: &Task,
    m: Option<&TaskTracking>,
    numbers: &BTreeMap<String, u64>,
) -> String {
    let mut reasons = Vec::new();
    if let Some(h) = m.and_then(|m| m.hold.as_ref()) {
        reasons.push(h.reason.clone());
    }
    if let Some(h) = m.and_then(|m| m.publication_hold.as_ref()) {
        reasons.push(h.reason.clone());
    }
    if let Some(p) = m
        .and_then(|m| m.latest.as_ref())
        .filter(|p| p.spec_hash == t.spec_hash)
    {
        if let Some(reason) = &p.blocker {
            if !reasons.contains(reason) {
                reasons.push(reason.clone());
            }
        }
    }
    let waiting = t
        .input
        .dependencies
        .iter()
        .filter(|id| s.tasks.get(*id).is_none_or(|d| !d.state.delivered()))
        .map(|id| {
            numbers
                .get(id)
                .map_or_else(|| id.clone(), |n| n.to_string())
        })
        .collect::<Vec<_>>();
    if !waiting.is_empty() {
        reasons.push(format!("等待任务 {} 交付", waiting.join("、")));
    }
    if s.projects[&t.input.project].paused {
        reasons.push("项目已明确暂停".into());
    }
    if reasons.is_empty() {
        let fallback = match t.state {
            TaskState::AwaitingConfirm => Some("等待匹配当前任务规格的人类确认"),
            TaskState::Unknown => Some("执行结果未知，需先核销"),
            TaskState::Pausing => Some("实际停止尚未确认"),
            TaskState::Blocked => Some("已记录受阻，具体原因待核实"),
            TaskState::Paused => Some("等待明确恢复指令"),
            _ => None,
        };
        if let Some(reason) = fallback {
            reasons.push(reason.into());
        }
    }
    if reasons.is_empty() {
        "—".into()
    } else {
        reasons.join("；")
    }
}

fn cell(value: &str) -> String {
    let text = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\\', "&#92;")
        .replace('|', "&#124;")
        .replace(['\r', '\n'], " ")
        .replace('`', "&#96;")
        .replace('[', "&#91;")
        .replace(']', "&#93;");
    if text.chars().count() > 360 {
        format!("{}…", text.chars().take(359).collect::<String>())
    } else {
        text
    }
}

pub fn render(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "当前没有未交付的任务。".into();
    }
    let mut table = format!("| {} |\n|---|---|---|---|---|---|\n", COLUMNS.join(" | "));
    for r in rows {
        table.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            r.number,
            cell(&r.project),
            cell(&r.task),
            cell(&r.phase),
            cell(&r.progress),
            cell(&r.blocker)
        ));
    }
    table.trim_end().into()
}

pub fn query(store: &Store) -> Result<Value> {
    let mut conn = store.read_connection()?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let snapshot = database::read_snapshot(&tx)?;
    let mut statement = tx
        .prepare("SELECT id,task_no FROM task_numbers")
        .map_err(|e| e.to_string())?;
    let numbers = statement
        .query_map([], |row| {
            let value: i64 = row.get(1)?;
            Ok((
                row.get::<_, String>(0)?,
                u64::try_from(value)
                    .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(1, value))?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()
        .map_err(|e| e.to_string())?;
    let s = &snapshot.state;
    let mut rows = Vec::new();
    for task in s
        .tasks
        .values()
        .filter(|t| !t.state.delivered() && t.state != TaskState::Cancelled)
    {
        let number = *numbers
            .get(&task.id)
            .ok_or("task is missing a permanent task number")?;
        let m = s.tracking.get(&task.id);
        let (rank, phase) = phase(task, m);
        let run = latest_run(s, task);
        rows.push(Row {
            number,
            task_id: task.id.clone(),
            project_id: task.input.project.clone(),
            project: s.projects[&task.input.project].name.clone(),
            task: task.input.title.clone(),
            phase,
            progress: progress(s, task, m),
            blocker: blocker(s, task, m, &numbers),
            rank,
            source: m.map_or_else(
                || task.input.authorization.human_source.clone(),
                |m| m.source_uri.clone(),
            ),
            thread_id: m
                .and_then(|m| {
                    m.execution_thread_id
                        .clone()
                        .or_else(|| m.controller_thread_id.clone())
                })
                .or_else(|| run.and_then(|r| r.thread_id.clone())),
            observed_at: m.and_then(|m| m.latest.as_ref().map(|p| p.observed_at)),
        });
    }
    rows.sort_by_key(|r| (r.rank, r.number));
    Ok(
        json!({"schema":2,"status":"ok","archive":snapshot.archive_id,"revision":snapshot.revision,
        "storage":"sqlite","queried_at":crate::tracking::now(),"view":{"columns":COLUMNS,"markdown":render(&rows),"rows":rows},
        "control":{"custody":s.custody,"wakeups":s.wakeups},"read_only":true}),
    )
}

#[cfg(test)]
#[path = "../../scripts/tests/orchestrator/status.rs"]
mod tests;
