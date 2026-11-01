use crate::{
    Result, digest,
    domain::*,
    store::{Snapshot, Store},
};
pub use bridgepilot_storage::events::{
    append, latest_receipt_matches, receipt_reference, record, transition,
};
pub use bridgepilot_tasks::tracking::*;
use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) fn observation_failed(
    store: &Store,
    expected: u64,
    id: &str,
    error: &str,
    source: &str,
) -> Result<()> {
    let before = store.read()?;
    if before.revision != expected {
        return Err("archive changed before recording observation failure".into());
    }
    let task = before.state.tasks.get(id).ok_or("unknown observed task")?;
    let error = error.chars().take(600).collect::<String>();
    let failure = ObservationError {
        attempted_at: now(),
        revision: expected,
        error: error.clone(),
        source: source.into(),
    };
    let event = Event::native(
        task,
        None,
        &format!("observation-error:{expected}"),
        "observation_failed",
        "关联对话观察受限，保留最后一次实际读回".into(),
        Some(error),
        source.into(),
        serde_json::to_value(&failure).map_err(|e| e.to_string())?,
    );
    store.transact_with_sql(expected, |s, tx| {
        let meta = s
            .tracking
            .get_mut(id)
            .ok_or("observed task metadata missing")?;
        if meta.mode != ExecutionMode::Observed {
            return Err("cannot record an observer failure for owned execution".into());
        }
        meta.observation_error = Some(failure);
        append(s, tx, &event)?;
        Ok(())
    })?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub task: TaskInput,
    pub source_key: String,
    pub source_uri: String,
    #[serde(default)]
    pub controller_thread_id: Option<String>,
    #[serde(default)]
    pub execution_thread_id: Option<String>,
    #[serde(default)]
    pub mode: ExecutionMode,
}

pub fn register(store: &Store, expected: u64, input: Value) -> Result<(Snapshot, String)> {
    let registration: Registration = if input.get("task").is_some() {
        serde_json::from_value(input).map_err(|e| e.to_string())?
    } else {
        let task: TaskInput = serde_json::from_value(input).map_err(|e| e.to_string())?;
        Registration {
            source_key: format!(
                "request:{}",
                digest(&serde_json::to_vec(&task).map_err(|e| e.to_string())?)
            ),
            source_uri: task.authorization.human_source.clone(),
            task,
            controller_thread_id: None,
            execution_thread_id: None,
            mode: ExecutionMode::Owned,
        }
    };
    store.transact_with_sql(expected,|s,tx| {
        if let Some((id,_))=s.tracking.iter().find(|(_,t)|t.source_key==registration.source_key) {
            let spec=digest(&serde_json::to_vec(&registration.task).map_err(|e|e.to_string())?);
            if s.tasks[id].spec_hash!=spec {return Err("task source already registered with a different specification; revise it explicitly".into());}
            return Ok(id.clone());
        }
        let id=s.add_task(registration.task)?;
        if s.tracking.contains_key(&id) {return Err("matching task already has another provenance; reuse its existing task identity".into());}
        s.tracking.insert(id.clone(),TaskTracking {source_key:registration.source_key.clone(),source_uri:registration.source_uri.clone(),
            mode:registration.mode,controller_thread_id:registration.controller_thread_id,execution_thread_id:registration.execution_thread_id,..Default::default()});
        let e=Event::native(&s.tasks[&id],None,&registration.source_key,"registered","任务已登记，等待执行条件满足".into(),None,
            registration.source_uri,json!({"source_key":registration.source_key}));
        append(s,tx,&e)?;Ok(id)
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldRequest {
    pub task: String,
    pub scope: HoldScope,
    pub reason: String,
    pub human_source: String,
    pub release: bool,
}

pub fn hold(store: &Store, expected: u64, input: HoldRequest) -> Result<Snapshot> {
    store.transact_with_sql(expected,|s,tx| {
        let task=s.tasks.get(&input.task).ok_or("unknown task")?.clone();
        if input.reason.trim().is_empty()||input.human_source.trim().is_empty(){return Err("explicit task hold decision is required".into());}
        if task.state.delivered()||task.state==TaskState::Cancelled {return Err("cannot hold a delivered or cancelled task".into());}
        if input.scope==HoldScope::Publication && s.runs.values().any(|r|r.task==input.task&&r.state.active()){
            return Err("pause and reconcile the active task before changing its publication decision".into());
        }
        if input.scope==HoldScope::Task && !input.release && s.tracking.get(&input.task).is_some_and(|m|m.mode==ExecutionMode::Observed
            && m.observation.as_ref().is_none_or(|o|matches!(o.status.as_str(),"inProgress"|"unknown"))){
            return Err("observed thread must be explicitly stopped and read back before certifying task pause".into());
        }
        if input.scope==HoldScope::Task {
            if input.release {
                if s.runs.values().any(|r|r.task==input.task&&r.state.active()){return Err("reconcile active/unknown execution before resuming a task".into());}
                let t=s.tasks.get_mut(&input.task).unwrap();
                if t.state==TaskState::Paused {t.state=t.resume_state.take().unwrap_or(TaskState::Ready);}
            } else {
                let t=s.tasks.get_mut(&input.task).unwrap();
                if !matches!(t.state,TaskState::Paused|TaskState::Pausing) {t.resume_state=Some(t.state);}
                t.state=TaskState::Paused;
                for r in s.runs.values_mut().filter(|r|r.task==input.task&&r.state.active()) {
                    if r.state==RunState::Unknown {t.state=TaskState::Unknown;}
                    else if r.state==RunState::Prepared&&r.owner_pid.is_none()&&r.server_pid.is_none(){
                        if s.operations.values().any(|op|op.run==r.id&&matches!(op.state,OperationState::Attempted|OperationState::Unknown)){
                            r.state=RunState::Unknown;t.state=TaskState::Unknown;
                        }else{
                            for op in s.operations.values_mut().filter(|op|op.run==r.id&&op.state==OperationState::Intent){
                                op.state=OperationState::Observed;op.result=Some(json!({"cancelled_before_attempt":true}));
                            }
                            r.state=RunState::Interrupted;r.result=Some(json!({"stopped_before_execution":true}));t.resume_state=Some(TaskState::Ready);
                        }
                    }else{r.state=RunState::Pausing;t.state=TaskState::Pausing;}
                }
            }
        }
        let m=s.tracking.entry(input.task.clone()).or_insert_with(||TaskTracking {source_key:format!("legacy-task:{}",task.id),
            source_uri:task.input.authorization.human_source.clone(),..Default::default()});
        if m.hold.as_ref().is_some_and(|h|h.scope==HoldScope::Publication){m.publication_hold=m.hold.take();}
        let decision=if input.scope==HoldScope::Publication{&mut m.publication_hold}else{&mut m.hold};
        if input.release&&decision.is_none(){return Err("no hold exists for the requested decision scope".into());}
        *decision=if input.release {None}else{Some(Hold {scope:input.scope,reason:input.reason.clone(),human_source:input.human_source.clone()})};
        let key=digest(&serde_json::to_vec(&(&input.task,expected,input.scope,&input.reason,&input.human_source,input.release)).map_err(|e|e.to_string())?);
        let e=Event::native(&task,None,&key,if input.release{"hold_cleared"}else{"held"},
            if input.release {"已记录明确恢复决定".into()}else{"已记录明确暂停／暂缓决定".into()},
            (!input.release).then(||input.reason.clone()),input.human_source,json!({"scope":input.scope,"release":input.release,"reason":input.reason}));
        append(s,tx,&e)?;Ok(())
    }).map(|(s,())|s)
}
