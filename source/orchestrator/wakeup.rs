//! Native chat heartbeat registration. The native scheduler owns wake delivery;
//! BP keeps its scope and actual receipt, and never invents a second OS service.
use crate::{
    Result,
    domain::{RunState, State, TaskState},
    new_id,
    store::Store,
    valid_id,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::{fs, path::PathBuf};

fn native_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .unwrap_or_default(),
            )
            .join(".codex")
        })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WakeRequest {
    pub controller_thread_id: String,
    pub controller_project: String,
    pub projects: Vec<String>,
    pub interval_minutes: u32,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WakeStatus {
    Planned,
    Attempted,
    Active,
    Paused,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wakeup {
    pub id: String,
    pub machine: String,
    #[serde(default = "native_home")]
    pub native_home: PathBuf,
    pub request: WakeRequest,
    pub status: WakeStatus,
    pub native_id: Option<String>,
    pub prompt: String,
    pub native_receipt: Option<Value>,
    pub observed_source: Option<String>,
}
pub fn validate_request(s: &State, machine: &str, r: &WakeRequest) -> Result<()> {
    let mut seen = BTreeSet::new();
    if r.controller_thread_id.trim().is_empty()
        || r.source.trim().is_empty()
        || r.projects.is_empty()
        || !(1..=60).contains(&r.interval_minutes)
        || !s
            .bindings
            .contains_key(&format!("{machine}:{}", r.controller_project))
        || r.projects
            .iter()
            .any(|p| !seen.insert(p) || !s.bindings.contains_key(&format!("{machine}:{p}")))
    {
        return Err("heartbeat requires actual controller, known local projects, unique scope and interval 1..60 minutes".into());
    }
    Ok(())
}
pub fn validate(s: &State) -> Result<()> {
    let mut controllers = BTreeSet::new();
    let mut native_ids = BTreeSet::new();
    for (id, w) in &s.wakeups {
        if id != &w.id
            || !valid_id(id)
            || !valid_id(&w.machine)
            || !w.native_home.is_absolute()
            || w.prompt.trim().is_empty()
        {
            return Err("invalid native wakeup identity".into());
        }
        validate_request(s, &w.machine, &w.request)?;
        if !controllers.insert((&w.machine, &w.request.controller_thread_id)) {
            return Err("overlapping native controller heartbeats".into());
        }
        if let Some(native) = &w.native_id {
            if !valid_id(native) || !native_ids.insert(native) {
                return Err("duplicate native wakeup receipt".into());
            }
        }
        if matches!(w.status, WakeStatus::Active | WakeStatus::Paused)
            && (w.native_id.is_none()
                || w.native_receipt.is_none()
                || w.observed_source
                    .as_deref()
                    .is_none_or(|v| v.trim().is_empty()))
        {
            return Err("active heartbeat requires actual observed native receipt".into());
        }
    }
    Ok(())
}
fn prompt(store: &Store, s: &State, machine: &str, r: &WakeRequest, id: &str) -> String {
    let root = &s.bindings[&format!("{machine}:{}", r.controller_project)].root;
    format!(
        "你是 BridgePilot，继续本对话已确认的工作。原生唤醒标识：{id}，控制对话：{}。先读取 {}/AGENTS.md 和 {}/source/orchestrator/skills/bridgepilot/SKILL.md，再使用当前受管 BP 程序。私有档案工作区：{}。调用 wake-check --workspace <该工作区> --wakeup {id} --controller-thread {}，核对当前机器、实际原生注册状态、明确项目范围、暂停、确认和 Unknown；失败或配置漂移时停止依赖动作并报告，不接管其他机器。只处理以下项目中已有明确授权的任务：{}。简单任务直接执行，复杂任务复用有效 confirm；重要方向与审美判断交用户。需要客户端动作时使用当前真实原生工具和实际回执，不能伪造 ID、重复未知操作或绕过创建聊天工具的人类授权。后台执行使用同一个 BP stdio 适配器与持久档案；未核实的模型完成不等于开发或发布。不要凭本唤醒请求扩张跨项目写入、提交、推送、发布或预算授权。当前开发目标是解决 BP 桌面归类、持续唤醒和开发发布核验，沿唯一开发计划接续，不重新做已完成工作；用户本次没有整体时间上限。该开发目标未完成时，每次唤醒报告约略完成百分比、已完成与阻点，并继续已授权工作。目标完成后，只有任务有进展、完成、失败或需要用户决定时通知；空闲或状态不变保持安静，不自动创造新任务。暂停必须明确全部或指定项目；本轮项目暂停不得影响其他项目。",
        r.controller_thread_id,
        root.display(),
        root.display(),
        store.workspace.display(),
        r.controller_thread_id,
        r.projects.join(", ")
    )
}
pub fn plan(store: &Store, expected: u64, request: WakeRequest) -> Result<Value> {
    let machine = store.machine()?;
    let (snapshot,id)=store.transact(expected,|s| {
        validate_request(s,&machine,&request)?;
        if let Some(w)=s.wakeups.values().find(|w|w.machine==machine && w.request.controller_thread_id==request.controller_thread_id) {
            if w.request==request {return Ok(w.id.clone());}
            return Err("existing heartbeat requires an explicit wake-revise after observed native pause before changing its scope".into());
        }
        let id=new_id("wakeup");let text=prompt(store,s,&machine,&request,&id);
        s.wakeups.insert(id.clone(),Wakeup{id:id.clone(),machine:machine.clone(),native_home:native_home(),request:request.clone(),status:WakeStatus::Planned,native_id:None,prompt:text,native_receipt:None,observed_source:None});Ok(id)
    })?;
    let w = &snapshot.state.wakeups[&id];
    if w.status != WakeStatus::Planned {
        return Ok(
            json!({"schema":1,"status":"wakeup_existing","revision":snapshot.revision,"wakeup":id,"native_id":w.native_id,"native_status":w.status,"client_action":null,"actual_registration_required":false,"reconcile_required":matches!(w.status,WakeStatus::Attempted|WakeStatus::Unknown),"instructions":"Inspect the existing native heartbeat/controller/prompt receipt. Do not create a duplicate registration."}),
        );
    }
    Ok(
        json!({"schema":1,"status":"wakeup_planned","revision":snapshot.revision,"wakeup":id,"client_action":{"mode":if w.native_id.is_some(){"update"}else{"create"},"id":w.native_id,"kind":"heartbeat","targetThreadId":w.request.controller_thread_id,"name":"BridgePilot 持续托管","prompt":w.prompt,"rrule":format!("FREQ=MINUTELY;INTERVAL={}",w.request.interval_minutes),"status":"ACTIVE"},"actual_registration_required":true}),
    )
}
pub fn attempt(store: &Store, expected: u64, id: &str) -> Result<Value> {
    let machine = store.machine()?;
    let (s,())=store.transact(expected,|s|{let w=s.wakeups.get_mut(id).ok_or("unknown wakeup")?;
        if w.machine!=machine||w.status!=WakeStatus::Planned{return Err("heartbeat already attempted or belongs to another machine; reconcile before creating".into());}
        w.status=WakeStatus::Attempted;Ok(())})?;
    Ok(json!({"schema":1,"status":"wakeup_attempted","revision":s.revision,"wakeup":id}))
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindWakeup {
    pub wakeup: String,
    pub native_id: String,
    pub controller_thread_id: String,
    pub interval_minutes: u32,
    pub native_prompt: String,
    pub observed_status: String,
    pub source: String,
    pub receipt: Value,
}

fn observed_native(w: &Wakeup, id: &str, status: &str) -> Result<Value> {
    if !valid_id(id) {
        return Err("invalid native heartbeat identity".into());
    }
    let path = w
        .native_home
        .join("automations")
        .join(id)
        .join("automation.toml");
    crate::store::ordinary(&path)?;
    let metadata = fs::metadata(&path)
        .map_err(|e| format!("native heartbeat registration is not readable: {e}"))?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err("native heartbeat registration is not a bounded file".into());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("invalid native heartbeat registration: {e}"))?;
    let rule = format!("FREQ=MINUTELY;INTERVAL={}", w.request.interval_minutes);
    for (key, expected) in [
        ("id", id),
        ("kind", "heartbeat"),
        ("status", status),
        ("prompt", w.prompt.as_str()),
        ("target_thread_id", w.request.controller_thread_id.as_str()),
        ("rrule", rule.as_str()),
    ] {
        if doc.get(key).and_then(toml_edit::Item::as_str) != Some(expected) {
            return Err(format!(
                "actual native heartbeat {key} does not match the saved controller/scope"
            ));
        }
    }
    Ok(
        json!({"path":path,"sha256":crate::digest(&bytes),"kind":"heartbeat","status":status,"controller_thread_id":w.request.controller_thread_id,"interval_minutes":w.request.interval_minutes}),
    )
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviseWakeup {
    pub wakeup: String,
    pub request: WakeRequest,
}
pub fn revise(store: &Store, expected: u64, input: ReviseWakeup) -> Result<Value> {
    let machine = store.machine()?;
    let (s, ()) = store.transact(expected, |s| {
        validate_request(s, &machine, &input.request)?;
        let w = s.wakeups.get(&input.wakeup).ok_or("unknown wakeup")?;
        if w.machine != machine
            || w.status != WakeStatus::Paused
            || w.request.controller_thread_id != input.request.controller_thread_id
            || w.request.controller_project != input.request.controller_project
        {
            return Err(
                "heartbeat scope revision requires its actual native pause on the same controller"
                    .into(),
            );
        }
        observed_native(
            w,
            w.native_id
                .as_deref()
                .ok_or("native registration missing")?,
            "PAUSED",
        )?;
        let text = prompt(store, s, &machine, &input.request, &input.wakeup);
        let w = s.wakeups.get_mut(&input.wakeup).unwrap();
        w.request = input.request.clone();
        w.prompt = text;
        w.status = WakeStatus::Planned;
        Ok(())
    })?;
    Ok(
        json!({"schema":1,"status":"wakeup_revised","revision":s.revision,"wakeup":input.wakeup,"native_id":s.state.wakeups[&input.wakeup].native_id,"instructions":"Use wake-plan to obtain an update action for the existing native heartbeat, then record wake-attempt and bind the actual updated receipt. Do not create another automation."}),
    )
}
pub fn bind(store: &Store, expected: u64, input: BindWakeup) -> Result<Value> {
    let machine = store.machine()?;
    if !valid_id(&input.native_id) || input.source.trim().is_empty() || !input.receipt.is_object() {
        return Err("actual native heartbeat receipt and source required".into());
    }
    let status = match input.observed_status.as_str() {
        "ACTIVE" => WakeStatus::Active,
        "PAUSED" => WakeStatus::Paused,
        _ => return Err("native heartbeat status was not established".into()),
    };
    let (s, ()) = store.transact(expected, |s| {
        let w = s.wakeups.get_mut(&input.wakeup).ok_or("unknown wakeup")?;
        if w.machine != machine
            || w.status == WakeStatus::Planned
            || w.request.controller_thread_id != input.controller_thread_id
            || w.request.interval_minutes != input.interval_minutes
            || w.prompt != input.native_prompt
            || w.native_id
                .as_ref()
                .is_some_and(|id| id != &input.native_id)
        {
            return Err(
                "native heartbeat receipt does not match the planned controller/scope/prompt"
                    .into(),
            );
        }
        let actual = observed_native(w, &input.native_id, &input.observed_status)?;
        w.status = status.clone();
        w.native_id = Some(input.native_id.clone());
        w.native_receipt =
            Some(json!({"client_receipt":input.receipt,"actual_registration":actual}));
        w.observed_source = Some(input.source.clone());
        Ok(())
    })?;
    Ok(
        json!({"schema":1,"status":"wakeup_observed","revision":s.revision,"wakeup":input.wakeup,"native_id":input.native_id,"native_status":status}),
    )
}
/// Read-only wake delivery check. A heartbeat is a trigger, never execution or
/// publication authority. Repeated idle deliveries do not grow archive history.
pub fn check(store: &Store, id: &str, controller: &str) -> Result<Value> {
    let snapshot = store.read()?;
    let w = snapshot.state.wakeups.get(id).ok_or("unknown wakeup")?;
    if w.machine != store.machine()?
        || w.status != WakeStatus::Active
        || w.request.controller_thread_id != controller
    {
        return Err("native wakeup is not active for this local controller".into());
    }
    let actual = observed_native(
        w,
        w.native_id.as_deref().ok_or("native receipt missing")?,
        "ACTIVE",
    )?;
    let s = &snapshot.state;
    let in_scope =
        |project: &String| w.request.projects.contains(project) && !s.projects[project].paused;
    let ready: Vec<_> = s
        .tasks
        .values()
        .filter(|t| {
            in_scope(&t.input.project)
                && t.state == TaskState::Ready
                && t.input
                    .dependencies
                    .iter()
                    .all(|id| s.tasks[id].state.delivered())
        })
        .map(|t| t.id.clone())
        .collect();
    let reviewing: Vec<_> = s
        .tasks
        .values()
        .filter(|t| {
            in_scope(&t.input.project)
                && matches!(t.state, TaskState::Reviewing | TaskState::AwaitingRelease)
        })
        .map(|t| t.id.clone())
        .collect();
    let unknown: Vec<_> = s
        .runs
        .values()
        .filter(|r| in_scope(&s.tasks[&r.task].input.project) && r.state == RunState::Unknown)
        .map(|r| r.id.clone())
        .collect();
    let paused: Vec<_> = w
        .request
        .projects
        .iter()
        .filter(|p| s.projects[*p].paused)
        .cloned()
        .collect();
    Ok(
        json!({"schema":1,"status":"wakeup_checked","revision":snapshot.revision,"wakeup":id,"native_id":w.native_id,"actual_registration":actual,"controller_thread_id":controller,"projects":w.request.projects,"ready_tasks":ready,"reviewing_tasks":reviewing,"unknown_runs":unknown,"paused_projects":paused,"actionable":!ready.is_empty()||!reviewing.is_empty()||!unknown.is_empty(),"new_execution_authorized":false,"automatic_unknown_retry":false,"publication_authorized":false}),
    )
}
