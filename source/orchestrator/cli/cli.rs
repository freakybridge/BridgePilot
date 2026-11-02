use crate::{
    Result, codex, commands,
    store::{Store, read_json},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

fn args(values: Vec<String>) -> Result<(String, BTreeMap<String, String>)> {
    let command = values
        .first()
        .ok_or("command required; use bp help")?
        .clone();
    let mut options = BTreeMap::new();
    let mut index = 1;
    while index < values.len() {
        let name = &values[index];
        if !name.starts_with("--") {
            return Err(format!("unexpected argument {name}"));
        }
        let value = values
            .get(index + 1)
            .ok_or("option value required (booleans use true/false)")?;
        if value.starts_with("--")
            || options
                .insert(name[2..].to_owned(), value.clone())
                .is_some()
        {
            return Err("missing value or duplicate option".into());
        }
        index += 2;
    }
    let allowed: &[&str] = match command.as_str() {
        "help" | "self-test" => &[],
        "probe" | "project-probe" | "skill-probe" => &["codex", "project-root"],
        "development-probe" => &["project-root", "tool", "packages"],
        "bundle" => &["output", "core", "launcher"],
        "install" | "install-preview" => &["bundle", "home", "workspace"],
        "install-status" | "install-recover" | "install-rollback" => &["home"],
        "skill-install" | "skill-install-preview" | "skill-status" | "skill-recover" => {
            &["home", "skills-root"]
        }
        "environment-check" => &["context"],
        "environment-install" => &["home", "scope", "source"],
        "sync-configure" => &["workspace", "input"],
        "sync-enrol" => &["workspace", "expected", "source"],
        "sync-upload" | "sync-download" | "sync-reconcile" | "handoff-status" => &["workspace"],
        "sync-cancel-pending" | "sync-fence-pending" | "handoff-cancel" => &["workspace", "source"],
        "handoff-release" | "handoff-accept" => &["workspace", "input"],
        "legacy-server-probe" => &[
            "workspace",
            "run",
            "codex",
            "historical-receipt",
            "launch-evidence",
            "supervisor-log",
            "source",
        ],
        "handoff-continue" => &["workspace", "expected", "input"],
        "checkpoint-probe" => &["workspace", "project", "remote", "ref", "source"],
        "verify-development" | "verify-publication" => &["workspace", "expected", "input"],
        "configure" => &["product-root", "workspace"],
        "init" | "status" | "actions" | "rebuild-index" | "storage-preview" => &["workspace"],
        "storage-migrate" => &["workspace", "expected", "source"],
        "storage-recover" => &["workspace", "source"],
        "task-hold" => &["workspace", "expected", "input"],
        "task-observe" => &["workspace", "expected", "task", "codex", "source"],
        "project-add" | "task-add" | "task-revise" | "run-bind" | "pause" | "resume" => {
            &["workspace", "expected", "input"]
        }
        "task-confirm" => &["workspace", "expected", "task", "spec-hash", "human-source"],
        "run-prepare" => &["workspace", "expected", "task"],
        "operation-attempt" => &["workspace", "expected", "operation"],
        "execute" => &[
            "workspace",
            "run",
            "codex",
            "timeout-seconds",
            "launch-operation",
        ],
        "supervise" => &["workspace", "codex", "input"],
        "report-review" => &["workspace", "expected", "input"],
        "run-reconcile" => &["workspace", "run", "codex"],
        "chat-restore" => &["workspace", "run", "codex", "archive-source"],
        "chat-group" => &["workspace", "run", "codex", "source"],
        "wake-plan" | "wake-bind" | "wake-revise" => &["workspace", "expected", "input"],
        "wake-attempt" => &["workspace", "expected", "wakeup"],
        "wake-check" => &["workspace", "wakeup", "controller-thread"],
        _ => return Err(format!("unknown command {command}; use bp help")),
    };
    if let Some(key) = options.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(format!("unknown option --{key} for {command}"));
    }
    Ok((command, options))
}
fn get<'a>(options: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    options
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("--{key} required"))
}
fn number(options: &BTreeMap<String, String>, key: &str) -> Result<u64> {
    get(options, key)?
        .parse()
        .map_err(|_| format!("invalid --{key}"))
}
fn input<T: serde::de::DeserializeOwned>(options: &BTreeMap<String, String>) -> Result<T> {
    read_json(Path::new(get(options, "input")?))
}
fn receipt(snapshot: crate::store::Snapshot, result: Value) -> Value {
    json!({"schema":1,"status":"committed","archive":snapshot.archive_id,"revision":snapshot.revision,"result":result})
}

pub fn run(values: Vec<String>) -> Result<Value> {
    bridgepilot_execution::set_program_version(env!("CARGO_PKG_VERSION"))?;
    let (command, options) = args(values)?;
    if command == "skill-probe" {
        return codex::skill_probe(
            Path::new(get(&options, "codex")?),
            Path::new(get(&options, "project-root")?),
        );
    }
    if command == "environment-check" {
        return crate::environment::check(Path::new(get(&options, "context")?));
    }
    if command == "environment-install" {
        return crate::environment::prepare(
            Path::new(get(&options, "home")?),
            get(&options, "scope")?,
            get(&options, "source")?,
        );
    }
    if matches!(
        command.as_str(),
        "skill-install" | "skill-install-preview" | "skill-status" | "skill-recover"
    ) {
        let home = Path::new(get(&options, "home")?);
        let skills = Path::new(get(&options, "skills-root")?);
        return match command.as_str() {
            "skill-status" => crate::user_skill::status(home, skills),
            "skill-recover" => crate::user_skill::recover(home, skills),
            _ => crate::user_skill::install(home, skills, command == "skill-install-preview"),
        };
    }
    if command == "help" {
        return Ok(
            json!({"schema":1,"commands":["init","configure","status","actions","project-add","task-add","task-revise","task-confirm","task-hold","task-observe","run-prepare","operation-attempt","run-bind","chat-restore","chat-group","project-probe","execute","supervise","report-review","development-probe","verify-development","verify-publication","wake-plan","wake-attempt","wake-bind","wake-revise","wake-check","run-reconcile","pause","resume","rebuild-index","probe","self-test","bundle","install","install-preview","install-status","install-recover","install-rollback","sync-configure","sync-enrol","sync-upload","sync-download","sync-reconcile","sync-cancel-pending","sync-fence-pending","checkpoint-probe","handoff-status","handoff-release","handoff-accept","handoff-cancel","handoff-continue","legacy-server-probe","skill-probe","environment-check","environment-install","skill-install-preview","skill-install","skill-status","skill-recover","storage-preview","storage-migrate","storage-recover"],"input":"Internal JSON requests; direct mutations require --expected <archive revision>; supervise owns its bounded CAS loop","workspace":"configured private workspace; user query entry remains status"}),
        );
    }
    if command == "self-test" {
        return Ok(
            json!({"schema":1,"program":"bridgepilot","version":env!("CARGO_PKG_VERSION"),"capabilities":["bp-task-archive-v1","bp-confirm-gate-v1","bp-client-action-outbox-v1","bp-owned-stdio-v1","bp-explicit-pause-scope-v1","bp-bounded-supervision-v1","bp-readonly-report-review-v1","bp-native-chat-handoff-v1","bp-native-project-assignment-v1","bp-native-heartbeat-v1","bp-managed-development-review-v1","bp-managed-publication-review-v1","bp-private-archive-sync-v1","bp-explicit-machine-handoff-v1","bp-portable-install-v1","bp-unified-skill-v1","bp-user-skill-v1","bp-independent-update-skill-v1","bp-prerequisite-install-v1","bp-sqlite-archive-v1","bp-sqlite-status-v1","bp-task-events-v1","bp-permanent-task-number-v1","bp-json-import-v1"],"archive_reader_features":["custody-v1","sqlite-v1"],"installed_home_supported":true,"desktop_creation":"native-client-action","native_heartbeat_available":true,"wakeup_requires_observed_registration":true,"automatic_takeover":false}),
        );
    }
    if command == "probe" {
        return codex::probe(
            Path::new(get(&options, "codex")?),
            Path::new(get(&options, "project-root")?),
        );
    }
    if command == "project-probe" {
        return codex::project_probe(
            Path::new(get(&options, "codex")?),
            Path::new(get(&options, "project-root")?),
        );
    }
    if command == "bundle" {
        return crate::install::bundle(
            Path::new(get(&options, "output")?),
            Path::new(get(&options, "core")?),
            Path::new(get(&options, "launcher")?),
        );
    }
    if matches!(command.as_str(), "install" | "install-preview") {
        return crate::install::install(
            Path::new(get(&options, "bundle")?),
            Path::new(get(&options, "home")?),
            options.get("workspace").map(Path::new),
            command == "install-preview",
        );
    }
    if matches!(
        command.as_str(),
        "install-status" | "install-recover" | "install-rollback"
    ) {
        let home = Path::new(get(&options, "home")?);
        return match command.as_str() {
            "install-status" => Ok(
                json!({"status":"installed","home":home,"installation":crate::install::inspect(home)?}),
            ),
            "install-recover" => crate::install::recover(home),
            _ => crate::install::rollback(home),
        };
    }
    if command == "configure" {
        return bridgepilot_install::configuration::configure(
            Path::new(get(&options, "product-root")?),
            Path::new(get(&options, "workspace")?),
        );
    }
    if command == "development-probe" {
        let ids = get(&options, "packages")?
            .split(',')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        return crate::delivery::development_probe(
            Path::new(get(&options, "project-root")?),
            Path::new(get(&options, "tool")?),
            &ids,
        );
    }
    let workspace = Path::new(get(&options, "workspace")?);
    if command == "init" {
        let store = Store::init(workspace)?;
        let machine = store.machine()?;
        let snapshot = store.read()?;
        return Ok(
            json!({"schema":1,"status":"initialized","workspace":store.workspace,"archive":snapshot.archive_id,"revision":snapshot.revision,"machine":machine}),
        );
    }
    let store = Store::open(workspace)?;
    match command.as_str() {
        "sync-configure" => crate::sync::configure(&store, input(&options)?),
        "sync-enrol" => crate::handoff::enrol(
            &store,
            number(&options, "expected")?,
            get(&options, "source")?,
        ),
        "sync-upload" => crate::sync::upload(&store),
        "sync-download" => crate::sync::download(&store),
        "sync-reconcile" => crate::sync::reconcile(&store),
        "sync-cancel-pending" => crate::sync::cancel_pending(&store, get(&options, "source")?),
        "sync-fence-pending" => crate::sync::fence_pending(&store, get(&options, "source")?),
        "handoff-release" => crate::handoff::release_native(&store, input(&options)?),
        "legacy-server-probe" => crate::handoff::legacy_server_probe(
            &store,
            get(&options, "run")?,
            Path::new(get(&options, "codex")?),
            Path::new(get(&options, "historical-receipt")?),
            Path::new(get(&options, "launch-evidence")?),
            Path::new(get(&options, "supervisor-log")?),
            get(&options, "source")?,
        ),
        "handoff-accept" => crate::handoff::accept_native(&store, input(&options)?),
        "handoff-cancel" => crate::handoff::cancel_native(&store, get(&options, "source")?),
        "handoff-continue" => {
            crate::handoff::continue_tasks(&store, number(&options, "expected")?, input(&options)?)
        }
        "handoff-status" => Ok(
            json!({"schema":1,"machine":store.machine()?,"custody":store.read()?.state.custody,"execution_started":false}),
        ),
        "checkpoint-probe" => {
            let snapshot = store.read()?;
            let project = snapshot
                .state
                .projects
                .get(get(&options, "project")?)
                .ok_or("unknown project")?;
            let binding = snapshot
                .state
                .bindings
                .get(&format!("{}:{}", store.machine()?, project.id))
                .ok_or("project not bound on this machine")?;
            Ok(
                json!({"status":"code_checkpoint_verified","checkpoint":crate::handoff::probe_checkpoint(&bridgeforge_core::SystemProcessRunner,&binding.root,project,get(&options,"remote")?,get(&options,"ref")?,get(&options,"source")?)?}),
            )
        }
        "verify-development" => crate::delivery::review_development(
            &store,
            number(&options, "expected")?,
            input(&options)?,
        ),
        "verify-publication" => crate::delivery::review_publication(
            &store,
            number(&options, "expected")?,
            input(&options)?,
        ),
        "wake-revise" => {
            crate::wakeup::revise(&store, number(&options, "expected")?, input(&options)?)
        }
        "wake-plan" => crate::wakeup::plan(&store, number(&options, "expected")?, input(&options)?),
        "wake-attempt" => crate::wakeup::attempt(
            &store,
            number(&options, "expected")?,
            get(&options, "wakeup")?,
        ),
        "wake-bind" => crate::wakeup::bind(&store, number(&options, "expected")?, input(&options)?),
        "wake-check" => crate::wakeup::check(
            &store,
            get(&options, "wakeup")?,
            get(&options, "controller-thread")?,
        ),
        "chat-group" => crate::grouping::ensure(
            &store,
            get(&options, "run")?,
            Path::new(get(&options, "codex")?),
            get(&options, "source")?,
        ),
        "chat-restore" => codex::restore_chat(
            &store,
            get(&options, "run")?,
            Path::new(get(&options, "codex")?),
            get(&options, "archive-source")?,
        ),
        "supervise" => crate::scheduler::supervise(
            &store,
            Path::new(get(&options, "codex")?),
            &std::env::current_exe().map_err(|e| e.to_string())?,
            input(&options)?,
        ),
        "report-review" => {
            crate::review::review(&store, number(&options, "expected")?, input(&options)?)
        }
        "status" => crate::status::query(&store),
        "storage-preview" => crate::migration::preview(&store),
        "storage-migrate" => crate::migration::apply(
            &store,
            number(&options, "expected")?,
            get(&options, "source")?,
        ),
        "storage-recover" => crate::migration::recover(&store, get(&options, "source")?),
        "task-hold" => {
            crate::tracking::hold(&store, number(&options, "expected")?, input(&options)?)
                .map(|s| receipt(s, json!({"hold_recorded":true})))
        }
        "task-observe" => crate::codex::observe_task(
            &store,
            number(&options, "expected")?,
            get(&options, "task")?,
            Path::new(get(&options, "codex")?),
            get(&options, "source")?,
        ),
        "rebuild-index" => {
            Ok(json!({"schema":1,"status":"index_rebuilt","path":store.rebuild_index()?}))
        }
        "actions" => commands::actions(&store),
        "execute" => codex::execute_with_launch(
            &store,
            get(&options, "run")?,
            Path::new(get(&options, "codex")?),
            number(&options, "timeout-seconds")?,
            options.get("launch-operation").map(String::as_str),
        ),
        "run-reconcile" => codex::reconcile(
            &store,
            get(&options, "run")?,
            Path::new(get(&options, "codex")?),
        ),
        "project-add" => bridgepilot_transfer::projects::add(
            &store,
            number(&options, "expected")?,
            input(&options)?,
        ),
        "task-add" => commands::add_task(&store, number(&options, "expected")?, input(&options)?),
        "task-confirm" => commands::confirm_task(
            &store,
            number(&options, "expected")?,
            get(&options, "task")?,
            get(&options, "spec-hash")?,
            get(&options, "human-source")?,
        ),
        "task-revise" => {
            commands::revise_task(&store, number(&options, "expected")?, input(&options)?)
        }
        "run-prepare" => commands::prepare_run(
            &store,
            number(&options, "expected")?,
            get(&options, "task")?,
        ),
        "operation-attempt" => commands::attempt_operation(
            &store,
            number(&options, "expected")?,
            get(&options, "operation")?,
        ),
        "run-bind" => commands::bind_run(&store, number(&options, "expected")?, input(&options)?),
        "pause" => commands::pause(&store, number(&options, "expected")?, input(&options)?),
        "resume" => commands::resume(&store, number(&options, "expected")?, input(&options)?),
        _ => Err(format!("unknown command {command}; use bp help")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parser_rejects_duplicate_or_missing_values() {
        assert!(args(vec!["status".into(), "--workspace".into()]).is_err());
        assert!(
            args(vec![
                "status".into(),
                "--workspace".into(),
                "a".into(),
                "--workspace".into(),
                "b".into()
            ])
            .is_err()
        );
        assert!(
            args(vec![
                "pause".into(),
                "--project".into(),
                "wrong-scope".into()
            ])
            .is_err()
        );
    }
}
