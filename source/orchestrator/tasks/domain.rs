use crate::{Result, digest, new_id, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub repository: String,
    pub paused: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub project: String,
    pub machine: String,
    pub root: PathBuf,
    pub desktop_project_id: String,
    pub host: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Complexity {
    Simple,
    Complex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    AwaitingConfirm,
    Ready,
    Running,
    Reviewing,
    AwaitingRelease,
    Released,
    ReportDelivered,
    Pausing,
    Paused,
    Blocked,
    Unknown,
    Cancelled,
}

impl TaskState {
    pub fn delivered(self) -> bool {
        matches!(self, Self::Released | Self::ReportDelivered)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authorization {
    /// Reference to the human request, not a claim of cryptographic authentication.
    pub human_source: String,
    pub allow_write: bool,
    pub allow_publish: bool,
    pub allow_execution_chat: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskInput {
    pub project: String,
    pub title: String,
    pub goal: String,
    pub non_goals: Vec<String>,
    pub acceptance: Vec<String>,
    pub dependencies: Vec<String>,
    pub complexity: Complexity,
    pub authorization: Authorization,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Confirmation {
    pub human_source: String,
    pub spec_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub input: TaskInput,
    pub spec_hash: String,
    pub confirmation: Option<Confirmation>,
    pub state: TaskState,
    pub resume_state: Option<TaskState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Prepared,
    BindingRequired,
    Running,
    Pausing,
    Completed,
    Interrupted,
    Failed,
    Unknown,
}
impl RunState {
    pub fn active(self) -> bool {
        matches!(
            self,
            Self::Prepared | Self::BindingRequired | Self::Running | Self::Pausing | Self::Unknown
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub id: String,
    pub task: String,
    pub spec_hash: String,
    pub ordinal: u64,
    pub machine: String,
    pub binding: Binding,
    pub state: RunState,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub server_pid: Option<u32>,
    pub owner_pid: Option<u32>,
    pub desktop_binding_source: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub instruction_sources: Value,
    pub rules_digest: Option<String>,
    pub result: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Intent,
    Attempted,
    Observed,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub id: String,
    pub run: String,
    pub kind: String,
    pub state: OperationState,
    pub result: Option<Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub projects: BTreeMap<String, Project>,
    pub bindings: BTreeMap<String, Binding>,
    pub tasks: BTreeMap<String, Task>,
    pub runs: BTreeMap<String, Run>,
    pub operations: BTreeMap<String, Operation>,
    #[serde(default)]
    pub wakeups: BTreeMap<String, crate::wakeup::Wakeup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custody: Option<crate::custody::Custody>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tracking: BTreeMap<String, crate::tracking::TaskTracking>,
}

impl State {
    pub fn validate(&self) -> Result<()> {
        crate::tracking::validate_state(self)?;
        crate::custody::validate(self)?;
        crate::wakeup::validate(self)?;
        for (id, p) in &self.projects {
            if id != &p.id
                || !valid_id(id)
                || p.name.trim().is_empty()
                || p.repository.trim().is_empty()
            {
                return Err("invalid project identity".into());
            }
        }
        for (key, b) in &self.bindings {
            if key != &format!("{}:{}", b.machine, b.project)
                || !self.projects.contains_key(&b.project)
                || !valid_id(&b.machine)
                || !b.root.is_absolute()
                || b.desktop_project_id.trim().is_empty()
                || b.host != "local"
            {
                return Err("invalid machine/project binding".into());
            }
            if self.bindings.values().any(|other| {
                other.machine == b.machine
                    && other.project != b.project
                    && (same_root(&other.root, &b.root)
                        || other.desktop_project_id == b.desktop_project_id)
            }) {
                return Err(
                    "distinct projects cannot alias the same local working tree or native project"
                        .into(),
                );
            }
        }
        for (id, t) in &self.tasks {
            if id != &t.id
                || !valid_id(id)
                || !self.projects.contains_key(&t.input.project)
                || t.input.title.trim().is_empty()
                || t.input.goal.trim().is_empty()
                || t.input.acceptance.is_empty()
                || t.input.acceptance.iter().any(|v| v.trim().is_empty())
                || t.input.authorization.human_source.trim().is_empty()
                || t.spec_hash != digest(&serde_json::to_vec(&t.input).map_err(|e| e.to_string())?)
            {
                return Err("invalid task or task specification digest".into());
            }
            if let Some(c) = &t.confirmation {
                if c.human_source.trim().is_empty() || c.spec_hash != t.spec_hash {
                    return Err("stale task confirmation".into());
                }
            }
            let mut visited = BTreeSet::new();
            self.visit_dependencies(id, &mut BTreeSet::new(), &mut visited)?;
        }
        let mut owners = BTreeSet::new();
        let mut ordinals = BTreeSet::new();
        for (id, r) in &self.runs {
            let task = self
                .tasks
                .get(&r.task)
                .ok_or("run references missing task")?;
            if id != &r.id
                || !valid_id(id)
                || !valid_id(&r.machine)
                || r.ordinal == 0
                || !ordinals.insert(r.ordinal)
                || r.binding.project != task.input.project
                || r.binding.machine != r.machine
                || !r.binding.root.is_absolute()
                || r.binding.host != "local"
                || r.spec_hash.len() != 64
                || !r.spec_hash.bytes().all(|b| b.is_ascii_hexdigit())
                || (r.state.active() && r.spec_hash != task.spec_hash)
                || r.thread_id.is_some() != r.desktop_binding_source.is_some()
            {
                return Err("invalid run binding or task revision".into());
            }
            if r.state.active() && !owners.insert(&task.input.project) {
                return Err("overlapping active project writers".into());
            }
        }
        for (id, op) in &self.operations {
            if id != &op.id
                || !valid_id(id)
                || !self.runs.contains_key(&op.run)
                || op.kind.is_empty()
            {
                return Err("invalid operation".into());
            }
        }
        Ok(())
    }

    fn visit_dependencies(
        &self,
        id: &str,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<()> {
        if done.contains(id) {
            return Ok(());
        }
        if !active.insert(id.to_owned()) {
            return Err("task dependency cycle".into());
        }
        let task = self.tasks.get(id).ok_or("missing task dependency")?;
        let mut unique = BTreeSet::new();
        for dep in &task.input.dependencies {
            if !unique.insert(dep) {
                return Err("duplicate task dependency".into());
            }
            self.visit_dependencies(dep, active, done)?;
        }
        active.remove(id);
        done.insert(id.to_owned());
        Ok(())
    }

    pub fn add_task(&mut self, input: TaskInput) -> Result<String> {
        let id = new_id("task");
        let hash = digest(&serde_json::to_vec(&input).map_err(|e| e.to_string())?);
        if let Some(existing) = self.tasks.values().find(|t| t.spec_hash == hash) {
            return Ok(existing.id.clone());
        }
        let state = if input.complexity == Complexity::Complex {
            TaskState::AwaitingConfirm
        } else {
            TaskState::Ready
        };
        self.tasks.insert(
            id.clone(),
            Task {
                id: id.clone(),
                input,
                spec_hash: hash,
                confirmation: None,
                state,
                resume_state: None,
            },
        );
        if let Err(error) = self.validate() {
            self.tasks.remove(&id);
            return Err(error);
        }
        Ok(id)
    }

    pub fn confirm(&mut self, id: &str, spec: &str, human_source: &str) -> Result<()> {
        let t = self.tasks.get_mut(id).ok_or("unknown task")?;
        if t.state == TaskState::Ready
            && t.confirmation
                .as_ref()
                .is_some_and(|c| c.spec_hash == spec && c.human_source == human_source)
        {
            return Ok(());
        }
        if t.spec_hash != spec
            || human_source.trim().is_empty()
            || t.state != TaskState::AwaitingConfirm
        {
            return Err("confirmation does not match waiting task".into());
        }
        t.confirmation = Some(Confirmation {
            human_source: human_source.into(),
            spec_hash: spec.into(),
        });
        t.state = TaskState::Ready;
        Ok(())
    }

    pub fn prepare_run(&mut self, task_id: &str, machine: &str) -> Result<String> {
        self.validate()?;
        crate::custody::require_owner(self, machine)?;
        if self.tracking.get(task_id).is_some_and(|t| {
            t.mode == crate::tracking::ExecutionMode::Observed
                || t.hold
                    .as_ref()
                    .is_some_and(|h| h.scope == crate::tracking::HoldScope::Task)
        }) {
            return Err("task belongs to an observed chat or has an explicit task hold; do not dispatch it again".into());
        }
        let t = self.tasks.get(task_id).ok_or("unknown task")?;
        if t.state != TaskState::Ready {
            return Err("task is not ready to execute; reconcile existing run first".into());
        }
        if t.input.complexity == Complexity::Complex
            && t.confirmation
                .as_ref()
                .is_none_or(|c| c.spec_hash != t.spec_hash)
        {
            return Err("complex task requires confirm".into());
        }
        let p = self
            .projects
            .get(&t.input.project)
            .ok_or("unknown project")?;
        if p.paused || !t.input.authorization.allow_execution_chat {
            return Err("project paused or execution chat not authorized".into());
        }
        if !self.bindings.contains_key(&format!("{machine}:{}", p.id)) {
            return Err("project has no binding on this machine".into());
        }
        if t.input
            .dependencies
            .iter()
            .any(|id| self.tasks.get(id).is_none_or(|d| !d.state.delivered()))
        {
            return Err("task dependency not delivered".into());
        }
        if self.runs.values().any(|r| {
            r.state.active()
                && self
                    .tasks
                    .get(&r.task)
                    .is_some_and(|other| other.input.project == p.id)
        }) {
            return Err("project already has active or unknown execution".into());
        }
        let binding = self.bindings[&format!("{machine}:{}", p.id)].clone();
        crate::custody::require_binding(self, &p.id, &binding)?;
        let previous = self
            .runs
            .values()
            .filter(|r| {
                r.task == task_id
                    && r.spec_hash == t.spec_hash
                    && r.machine == machine
                    && r.state == RunState::Interrupted
                    && r.binding.root == binding.root
                    && r.binding.desktop_project_id == binding.desktop_project_id
                    && r.thread_id.is_some()
                    && r.desktop_binding_source.is_some()
            })
            .max_by_key(|r| r.ordinal)
            .cloned();
        let ordinal = self
            .runs
            .values()
            .map(|r| r.ordinal)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("run ordinal overflow")?;
        let id = new_id("run");
        self.runs.insert(
            id.clone(),
            Run {
                id: id.clone(),
                task: task_id.into(),
                spec_hash: t.spec_hash.clone(),
                ordinal,
                machine: machine.into(),
                binding,
                state: RunState::Prepared,
                thread_id: previous.as_ref().and_then(|r| r.thread_id.clone()),
                turn_id: None,
                server_pid: None,
                owner_pid: None,
                desktop_binding_source: previous.and_then(|r| r.desktop_binding_source),
                model: None,
                effort: None,
                instruction_sources: Value::Null,
                rules_digest: None,
                result: None,
            },
        );
        Ok(id)
    }

    pub fn operation(&mut self, run: &str, kind: &str) -> Result<String> {
        if !self.runs.contains_key(run) {
            return Err("unknown run".into());
        }
        let id = new_id("op");
        self.operations.insert(
            id.clone(),
            Operation {
                id: id.clone(),
                run: run.into(),
                kind: kind.into(),
                state: OperationState::Intent,
                result: None,
            },
        );
        Ok(id)
    }

    pub fn prepare_dispatch(
        &mut self,
        task: &str,
        machine: &str,
    ) -> Result<(String, Option<String>)> {
        let run = self.prepare_run(task, machine)?;
        let operation = if self.runs[&run].thread_id.is_none() {
            Some(self.operation(&run, "create_execution_chat")?)
        } else {
            Some(self.ensure_chat_preparation(&run)?)
        };
        Ok((run, operation))
    }

    pub fn ensure_chat_preparation(&mut self, run: &str) -> Result<String> {
        if self.runs.get(run).ok_or("unknown run")?.thread_id.is_none() {
            return Err("native chat binding required before handoff".into());
        }
        if let Some(op) = self
            .operations
            .values()
            .find(|op| op.run == run && op.kind == "prepare_execution_chat")
        {
            return Ok(op.id.clone());
        }
        self.operation(run, "prepare_execution_chat")
    }

    pub fn chat_ready(&self, run: &str) -> bool {
        let Some(r) = self.runs.get(run) else {
            return false;
        };
        r.thread_id.as_deref().is_some_and(|thread| {
            self.operations.values().any(|op| {
                op.run == run
                    && op.kind == "prepare_execution_chat"
                    && op.state == OperationState::Observed
                    && op
                        .result
                        .as_ref()
                        .is_some_and(|v| v["thread_id"] == thread && v["handoff_ready"] == true)
            })
        })
    }

    pub fn validate_chat_handoff(&self, run: &str, machine: &str) -> Result<()> {
        let r = self.runs.get(run).ok_or("unknown run")?;
        let t = &self.tasks[&r.task];
        if r.machine != machine
            || !matches!(r.state, RunState::Prepared | RunState::Interrupted)
            || r.thread_id.is_none()
            || self.projects[&t.input.project].paused
            || r.spec_hash != t.spec_hash
            || self.runs.values().any(|other| {
                other.id != r.id
                    && other.state.active()
                    && (self.tasks[&other.task].input.project == t.input.project
                        || other.thread_id == r.thread_id)
            })
        {
            return Err("chat restoration requires an unresolved-free local idle handoff".into());
        }
        Ok(())
    }

    pub fn pause(&mut self, all: bool, projects: &[String]) -> Result<Vec<String>> {
        if all == !projects.is_empty() {
            return Err("pause requires exactly all or a nonempty explicit project set".into());
        }
        let ids: BTreeSet<String> = if all {
            self.projects.keys().cloned().collect()
        } else {
            projects.iter().cloned().collect()
        };
        if ids.is_empty()
            || ids.len()
                != if all {
                    self.projects.len()
                } else {
                    projects.len()
                }
            || ids.iter().any(|id| !self.projects.contains_key(id))
        {
            return Err("unknown, duplicate, or empty pause scope".into());
        }
        for id in &ids {
            self.projects.get_mut(id).unwrap().paused = true;
        }
        for task in self.tasks.values_mut().filter(|t| {
            ids.contains(&t.input.project)
                && !t.state.delivered()
                && t.state != TaskState::Cancelled
        }) {
            let active = self
                .runs
                .values_mut()
                .find(|r| r.task == task.id && r.state.active());
            let prior = task.state;
            task.state = if let Some(run) = active {
                if run.state == RunState::Unknown {
                    TaskState::Unknown
                } else if run.state == RunState::Prepared
                    && run.owner_pid.is_none()
                    && run.server_pid.is_none()
                {
                    let uncertain = self.operations.values().any(|op| {
                        op.run == run.id
                            && matches!(
                                op.state,
                                OperationState::Attempted | OperationState::Unknown
                            )
                    });
                    if uncertain {
                        run.state = RunState::Unknown;
                        TaskState::Unknown
                    } else {
                        for op in self
                            .operations
                            .values_mut()
                            .filter(|op| op.run == run.id && op.state == OperationState::Intent)
                        {
                            op.state = OperationState::Observed;
                            op.result = Some(serde_json::json!({"cancelled_before_attempt":true}));
                        }
                        run.state = RunState::Interrupted;
                        run.result = Some(serde_json::json!({"stopped_before_execution":true}));
                        task.resume_state = Some(TaskState::Ready);
                        TaskState::Paused
                    }
                } else {
                    run.state = RunState::Pausing;
                    TaskState::Pausing
                }
            } else {
                if prior != TaskState::Paused {
                    task.resume_state = Some(prior);
                }
                TaskState::Paused
            };
        }
        Ok(ids.into_iter().collect())
    }

    pub fn resume_projects(&mut self, projects: &[String]) -> Result<()> {
        if projects.is_empty() || projects.iter().any(|id| !self.projects.contains_key(id)) {
            return Err("resume requires explicit known projects".into());
        }
        if self.runs.values().any(|r| {
            matches!(r.state, RunState::Unknown | RunState::Pausing)
                && self
                    .tasks
                    .get(&r.task)
                    .is_some_and(|t| projects.contains(&t.input.project))
        }) {
            return Err("cannot resume while stopping or execution ownership is unknown".into());
        }
        for id in projects {
            self.projects.get_mut(id).unwrap().paused = false;
        }
        for t in self.tasks.values_mut().filter(|t| {
            projects.contains(&t.input.project)
                && t.state == TaskState::Paused
                && !self.tracking.get(&t.id).is_some_and(|m| {
                    m.hold
                        .as_ref()
                        .is_some_and(|h| h.scope == crate::tracking::HoldScope::Task)
                })
        }) {
            t.state = t.resume_state.take().unwrap_or(TaskState::Ready);
        }
        Ok(())
    }

    pub fn revise(&mut self, id: &str, input: TaskInput) -> Result<()> {
        let prior = self.tasks.get(id).ok_or("unknown task")?;
        if input.project != prior.input.project
            || self.runs.values().any(|r| r.task == id && r.state.active())
        {
            return Err("cannot revise project identity or an active/unknown execution".into());
        }
        let hash = digest(&serde_json::to_vec(&input).map_err(|e| e.to_string())?);
        let state = if input.complexity == Complexity::Complex {
            TaskState::AwaitingConfirm
        } else {
            TaskState::Ready
        };
        self.tasks.insert(
            id.into(),
            Task {
                id: id.into(),
                input,
                spec_hash: hash,
                confirmation: None,
                state,
                resume_state: None,
            },
        );
        Ok(())
    }
}

fn same_root(left: &std::path::Path, right: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(test)]
#[path = "../../../scripts/tests/orchestrator/domain.rs"]
mod tests;
