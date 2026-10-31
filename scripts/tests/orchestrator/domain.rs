use super::*;

fn state() -> State {
    let mut s = State::default();
    for id in ["one", "two"] {
        s.projects.insert(
            id.into(),
            Project {
                id: id.into(),
                name: id.into(),
                repository: format!("repo:{id}"),
                paused: false,
            },
        );
        s.bindings.insert(
            format!("machine:{id}"),
            Binding {
                project: id.into(),
                machine: "machine".into(),
                root: std::env::current_dir().unwrap().join(id),
                desktop_project_id: format!("native-{id}"),
                host: "local".into(),
            },
        );
    }
    s
}
#[test]
fn distinct_project_ids_cannot_alias_the_same_working_tree_or_native_project() {
    let mut s = state();
    let task = s.add_task(input("one", Complexity::Simple)).unwrap();
    let prior = s.bindings["machine:two"].clone();
    s.bindings.get_mut("machine:two").unwrap().root = s.bindings["machine:one"].root.clone();
    assert!(s.prepare_run(&task, "machine").is_err());
    assert!(s.runs.is_empty());
    s.bindings.insert("machine:two".into(), prior);
    s.bindings
        .get_mut("machine:two")
        .unwrap()
        .desktop_project_id = s.bindings["machine:one"].desktop_project_id.clone();
    assert!(s.validate().is_err());
}
fn input(project: &str, complexity: Complexity) -> TaskInput {
    TaskInput {
        project: project.into(),
        title: "verify fixture".into(),
        goal: "perform bounded test".into(),
        non_goals: vec!["no publication".into()],
        acceptance: vec!["report actual evidence".into()],
        dependencies: vec![],
        complexity,
        authorization: Authorization {
            human_source: "test:human-request".into(),
            allow_write: false,
            allow_publish: false,
            allow_execution_chat: true,
        },
    }
}

#[test]
fn complex_requires_current_confirmation_and_project_binding() {
    let mut s = state();
    let id = s.add_task(input("one", Complexity::Complex)).unwrap();
    assert!(s.prepare_run(&id, "machine").is_err());
    assert!(s.confirm(&id, "stale", "test:human").is_err());
    let spec = s.tasks[&id].spec_hash.clone();
    s.confirm(&id, &spec, "test:human").unwrap();
    assert!(s.prepare_run(&id, "another-machine").is_err());
    assert!(s.prepare_run(&id, "machine").is_ok());
}
#[test]
fn simple_task_executes_without_repeated_confirm_but_blocks_overlapping_writers() {
    let mut s = state();
    let one = s.add_task(input("one", Complexity::Simple)).unwrap();
    let two = s.add_task(input("one", Complexity::Simple)).unwrap();
    s.prepare_run(&one, "machine").unwrap();
    assert!(s.prepare_run(&two, "machine").is_err());
    let other = s.add_task(input("two", Complexity::Simple)).unwrap();
    assert!(s.prepare_run(&other, "machine").is_ok());
}
#[test]
fn ambiguous_pause_is_zero_change_and_explicit_pause_is_isolated() {
    let mut s = state();
    let one = s.add_task(input("one", Complexity::Simple)).unwrap();
    let two = s.add_task(input("two", Complexity::Simple)).unwrap();
    let a = s.prepare_run(&one, "machine").unwrap();
    let b = s.prepare_run(&two, "machine").unwrap();
    let before = serde_json::to_vec(&s).unwrap();
    assert!(s.pause(false, &[]).is_err());
    assert!(s.pause(true, &["one".into()]).is_err());
    assert!(s.pause(false, &["missing".into()]).is_err());
    assert_eq!(serde_json::to_vec(&s).unwrap(), before);
    s.pause(false, &["one".into()]).unwrap();
    assert_eq!(s.runs[&a].state, RunState::Interrupted);
    assert_eq!(s.runs[&b].state, RunState::Prepared);
    assert!(s.resume_projects(&["one".into()]).is_ok());
}
#[test]
fn pause_preserves_review_and_confirmation_phase() {
    let mut s = state();
    let id = s.add_task(input("one", Complexity::Complex)).unwrap();
    s.pause(false, &["one".into()]).unwrap();
    s.resume_projects(&["one".into()]).unwrap();
    assert_eq!(s.tasks[&id].state, TaskState::AwaitingConfirm);
    s.tasks.get_mut(&id).unwrap().state = TaskState::Reviewing;
    s.pause(false, &["one".into()]).unwrap();
    s.pause(false, &["one".into()]).unwrap();
    s.resume_projects(&["one".into()]).unwrap();
    assert_eq!(s.tasks[&id].state, TaskState::Reviewing);
    assert!(s.prepare_run(&id, "machine").is_err());
}
#[test]
fn pause_before_external_creation_cancels_intent_but_attempted_creation_stays_unknown() {
    let mut s = state();
    let task = s.add_task(input("one", Complexity::Simple)).unwrap();
    let run = s.prepare_run(&task, "machine").unwrap();
    let op = s.operation(&run, "create_execution_chat").unwrap();
    s.pause(false, &["one".into()]).unwrap();
    assert_eq!(s.runs[&run].state, RunState::Interrupted);
    assert_eq!(s.operations[&op].state, OperationState::Observed);
    s.resume_projects(&["one".into()]).unwrap();
    let next = s.prepare_run(&task, "machine").unwrap();
    let op = s.operation(&next, "create_execution_chat").unwrap();
    s.operations.get_mut(&op).unwrap().state = OperationState::Attempted;
    s.pause(false, &["one".into()]).unwrap();
    assert_eq!(s.runs[&next].state, RunState::Unknown);
    assert!(s.resume_projects(&["one".into()]).is_err());
}
#[test]
fn revised_task_does_not_reuse_a_chat_for_the_old_specification() {
    let mut s = state();
    let task = s.add_task(input("one", Complexity::Simple)).unwrap();
    let run = s.prepare_run(&task, "machine").unwrap();
    let r = s.runs.get_mut(&run).unwrap();
    r.state = RunState::Interrupted;
    r.thread_id = Some("native-thread".into());
    r.desktop_binding_source = Some("test:client".into());
    let mut changed = input("one", Complexity::Simple);
    changed.goal = "revised task".into();
    s.revise(&task, changed).unwrap();
    let next = s.prepare_run(&task, "machine").unwrap();
    assert!(s.runs[&next].thread_id.is_none());
    assert_ne!(s.runs[&next].spec_hash, s.runs[&run].spec_hash);
    s.validate().unwrap();
}
#[test]
fn unknown_execution_never_becomes_a_new_run_or_automatic_takeover() {
    let mut s = state();
    let id = s.add_task(input("one", Complexity::Simple)).unwrap();
    let run = s.prepare_run(&id, "machine").unwrap();
    s.runs.get_mut(&run).unwrap().state = RunState::Unknown;
    s.tasks.get_mut(&id).unwrap().state = TaskState::Unknown;
    assert!(s.prepare_run(&id, "machine").is_err());
    s.pause(false, &["one".into()]).unwrap();
    assert!(s.resume_projects(&["one".into()]).is_err());
}
#[test]
fn missing_and_cyclic_dependencies_and_invalid_ids_are_rejected_without_insert() {
    let mut s = state();
    let before = s.tasks.len();
    let mut t = input("one", Complexity::Simple);
    t.dependencies.push("missing".into());
    assert!(s.add_task(t).is_err());
    assert_eq!(s.tasks.len(), before);
    let id = s.add_task(input("one", Complexity::Simple)).unwrap();
    s.tasks
        .get_mut(&id)
        .unwrap()
        .input
        .dependencies
        .push(id.clone());
    let bytes = serde_json::to_vec(&s.tasks[&id].input).unwrap();
    s.tasks.get_mut(&id).unwrap().spec_hash = digest(&bytes);
    assert!(s.validate().is_err());
}
#[test]
fn duplicate_intake_and_confirmation_are_idempotent() {
    let mut s = state();
    let input = input("one", Complexity::Complex);
    let first = s.add_task(input.clone()).unwrap();
    let second = s.add_task(input).unwrap();
    assert_eq!(first, second);
    assert_eq!(s.tasks.len(), 1);
    let hash = s.tasks[&first].spec_hash.clone();
    s.confirm(&first, &hash, "test:decision").unwrap();
    s.confirm(&first, &hash, "test:decision").unwrap();
}
#[test]
fn changed_direction_invalidates_confirmation_and_requires_new_confirm() {
    let mut s = state();
    let first = s.add_task(input("one", Complexity::Complex)).unwrap();
    let hash = s.tasks[&first].spec_hash.clone();
    s.confirm(&first, &hash, "test:decision").unwrap();
    let mut changed = input("one", Complexity::Complex);
    changed.goal = "new direction".into();
    s.revise(&first, changed).unwrap();
    assert_eq!(s.tasks[&first].state, TaskState::AwaitingConfirm);
    assert!(s.prepare_run(&first, "machine").is_err());
}
#[test]
fn pause_resume_keeps_task_and_chat_identity_on_same_binding() {
    let mut s = state();
    let task = s.add_task(input("one", Complexity::Simple)).unwrap();
    let run = s.prepare_run(&task, "machine").unwrap();
    let r = s.runs.get_mut(&run).unwrap();
    r.state = RunState::Interrupted;
    r.thread_id = Some("native-thread".into());
    r.desktop_binding_source = Some("test:client".into());
    let t = s.tasks.get_mut(&task).unwrap();
    t.state = TaskState::Paused;
    t.resume_state = Some(TaskState::Ready);
    s.projects.get_mut("one").unwrap().paused = true;
    s.resume_projects(&["one".into()]).unwrap();
    let next = s.prepare_run(&task, "machine").unwrap();
    assert_eq!(s.runs[&next].thread_id.as_deref(), Some("native-thread"));
    assert_eq!(s.runs[&next].task, task);
}
