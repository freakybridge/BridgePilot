use super::*;
use bridgepilot_orchestrator::wakeup::{self, BindWakeup, WakeRequest};
use serde_json::json;

fn native_registration(f: &Fixture, id: &str, status: &str) {
    f.store
        .update(|s| {
            s.wakeups.get_mut(id).unwrap().native_home =
                f.root.join(".runtime/controlled-native-home");
            Ok(())
        })
        .unwrap();
    let s = f.store.read().unwrap();
    let w = &s.state.wakeups[id];
    let folder = w
        .native_home
        .join("automations/actual-controlled-heartbeat");
    fs::create_dir_all(&folder).unwrap();
    let mut doc = toml_edit::DocumentMut::new();
    for (key, value) in [
        ("id", "actual-controlled-heartbeat"),
        ("kind", "heartbeat"),
        ("status", status),
        ("prompt", w.prompt.as_str()),
        ("target_thread_id", w.request.controller_thread_id.as_str()),
        (
            "rrule",
            format!("FREQ=MINUTELY;INTERVAL={}", w.request.interval_minutes).as_str(),
        ),
    ] {
        doc[key] = toml_edit::value(value);
    }
    fs::write(folder.join("automation.toml"), doc.to_string()).unwrap();
}

#[test]
fn native_wakeup_requires_observed_receipt_and_preserves_pause_scope() {
    let f = Fixture::new("complete");
    let req = WakeRequest {
        controller_thread_id: "fixture-controller".into(),
        controller_project: "p".into(),
        projects: vec!["p".into()],
        interval_minutes: 5,
        source: "test:human-native-wakeup-request".into(),
    };
    let planned = wakeup::plan(&f.store, f.store.read().unwrap().revision, req.clone()).unwrap();
    let id = planned["wakeup"].as_str().unwrap();
    assert!(wakeup::check(&f.store, id, "fixture-controller").is_err());
    let repeated = wakeup::plan(&f.store, f.store.read().unwrap().revision, req.clone()).unwrap();
    assert_eq!(planned["wakeup"], repeated["wakeup"]);
    let mut binding = BindWakeup {
        wakeup: id.into(),
        native_id: "actual-controlled-heartbeat".into(),
        controller_thread_id: "fixture-controller".into(),
        interval_minutes: 5,
        native_prompt: planned["client_action"]["prompt"].as_str().unwrap().into(),
        observed_status: "ACTIVE".into(),
        source: "test:native-view".into(),
        receipt: json!({"controlled_native_scheduler":true}),
    };
    assert!(wakeup::bind(&f.store, f.store.read().unwrap().revision, binding.clone()).is_err());
    wakeup::attempt(&f.store, f.store.read().unwrap().revision, id).unwrap();
    let repeated = wakeup::plan(&f.store, f.store.read().unwrap().revision, req.clone()).unwrap();
    assert!(repeated["client_action"].is_null());
    assert_eq!(repeated["actual_registration_required"], false);
    assert!(wakeup::attempt(&f.store, f.store.read().unwrap().revision, id).is_err());
    native_registration(&f, id, "ACTIVE");
    binding.controller_thread_id = "wrong-controller".into();
    assert!(wakeup::bind(&f.store, f.store.read().unwrap().revision, binding.clone()).is_err());
    binding.controller_thread_id = "fixture-controller".into();
    wakeup::bind(&f.store, f.store.read().unwrap().revision, binding).unwrap();
    let repeated = wakeup::plan(&f.store, f.store.read().unwrap().revision, req).unwrap();
    assert!(repeated["client_action"].is_null());
    assert_eq!(repeated["native_id"], "actual-controlled-heartbeat");
    let before = f.store.read().unwrap().revision;
    let checked = wakeup::check(&f.store, id, "fixture-controller").unwrap();
    assert_eq!(checked["ready_tasks"].as_array().unwrap().len(), 1);
    assert_eq!(before, f.store.read().unwrap().revision);
    f.store.update(|s| s.pause(false, &["p".into()])).unwrap();
    let checked = wakeup::check(&f.store, id, "fixture-controller").unwrap();
    assert!(checked["ready_tasks"].as_array().unwrap().is_empty());
    assert_eq!(checked["publication_authorized"], false);
    assert!(wakeup::check(&f.store, id, "another-controller").is_err());
    native_registration(&f, id, "PAUSED");
    assert!(wakeup::check(&f.store, id, "fixture-controller").is_err());
}

#[test]
fn paused_heartbeat_scope_update_reuses_native_identity_without_another_creation() {
    let f = Fixture::new("complete");
    let req = WakeRequest {
        controller_thread_id: "fixture-controller".into(),
        controller_project: "p".into(),
        projects: vec!["p".into()],
        interval_minutes: 5,
        source: "test:scope".into(),
    };
    let planned = wakeup::plan(&f.store, f.store.read().unwrap().revision, req.clone()).unwrap();
    let id = planned["wakeup"].as_str().unwrap();
    wakeup::attempt(&f.store, f.store.read().unwrap().revision, id).unwrap();
    native_registration(&f, id, "PAUSED");
    let binding = BindWakeup {
        wakeup: id.into(),
        native_id: "actual-controlled-heartbeat".into(),
        controller_thread_id: "fixture-controller".into(),
        interval_minutes: 5,
        native_prompt: planned["client_action"]["prompt"].as_str().unwrap().into(),
        observed_status: "PAUSED".into(),
        source: "test:native-view".into(),
        receipt: json!({"controlled":true}),
    };
    wakeup::bind(&f.store, f.store.read().unwrap().revision, binding.clone()).unwrap();
    let same = wakeup::plan(&f.store, f.store.read().unwrap().revision, req.clone()).unwrap();
    assert!(same["client_action"].is_null());
    let mut revised = req;
    revised.interval_minutes = 6;
    wakeup::revise(
        &f.store,
        f.store.read().unwrap().revision,
        wakeup::ReviseWakeup {
            wakeup: id.into(),
            request: revised.clone(),
        },
    )
    .unwrap();
    let plan = wakeup::plan(&f.store, f.store.read().unwrap().revision, revised).unwrap();
    assert_eq!(plan["client_action"]["mode"], "update");
    assert_eq!(plan["client_action"]["id"], "actual-controlled-heartbeat");
    wakeup::attempt(&f.store, f.store.read().unwrap().revision, id).unwrap();
    native_registration(&f, id, "ACTIVE");
    let mut binding = binding;
    binding.interval_minutes = 6;
    binding.native_prompt = plan["client_action"]["prompt"].as_str().unwrap().into();
    binding.observed_status = "ACTIVE".into();
    wakeup::bind(&f.store, f.store.read().unwrap().revision, binding).unwrap();
    assert_eq!(f.store.read().unwrap().state.wakeups.len(), 1);
    assert!(wakeup::check(&f.store, id, "fixture-controller").is_ok());
}

#[test]
fn native_wakeup_rejects_duplicate_or_unregistered_project_scope() {
    let f = Fixture::new("complete");
    for scope in [vec!["p".into(), "p".into()], vec!["unknown".into()]] {
        let before = f.store.read().unwrap().revision;
        let request = WakeRequest {
            controller_thread_id: "controller".into(),
            controller_project: "p".into(),
            projects: scope,
            interval_minutes: 5,
            source: "test:human".into(),
        };
        assert!(wakeup::plan(&f.store, before, request).is_err());
        assert_eq!(before, f.store.read().unwrap().revision);
    }
}
