use super::{blocked, has, path_value, value, values};
use bridgeforge_core::{CommandOutcome, EXIT_BLOCKED, ProjectContext, SystemProcessRunner};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

fn memory_paths(args: &[String]) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), String> {
    if let Some(project) = value(args, "--project-root") {
        ProjectContext::discover(Some(Path::new(&project))).map_err(|error| error.to_string())?;
    }
    let codex = value(args, "--codex-home")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("CODEX_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".codex")))
        .ok_or_else(|| {
            "--codex-home is required when CODEX_HOME and USERPROFILE are unavailable".to_string()
        })?;
    let memories = value(args, "--memories")
        .map(PathBuf::from)
        .unwrap_or_else(|| codex.join("memories"));
    let state = value(args, "--state-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| codex.join(".bridgeforge-codex/native-memory-sync"));
    for (actual, expected, flag) in [
        (&memories, codex.join("memories"), "--memories"),
        (
            &state,
            codex.join(".bridgeforge-codex/native-memory-sync"),
            "--state-dir",
        ),
    ] {
        if memory_path_identity(actual)? != memory_path_identity(&expected)? {
            return Err(format!(
                "{flag} is outside the fixed authorized Codex home scope"
            ));
        }
    }
    let ledger = codex.join("bridgeforge-codex-managed.json");
    Ok((codex, memories, state, ledger))
}

fn memory_path_identity(path: &Path) -> Result<PathBuf, String> {
    let absolute = std::path::absolute(path).map_err(|error| error.to_string())?;
    let mut normalized = PathBuf::new();
    for part in absolute.components() {
        match part {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            _ => normalized.push(part),
        }
    }
    let mut ancestor = normalized.as_path();
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or("memory scope has no existing ancestor")?;
    }
    Ok(ancestor
        .canonicalize()
        .map_err(|error| error.to_string())?
        .join(
            normalized
                .strip_prefix(ancestor)
                .map_err(|error| error.to_string())?,
        ))
}

fn authorized_memory_remote(
    args: &[String],
    ledger: &Path,
    state: &Path,
) -> Result<String, String> {
    let approved = authorized_remote(ledger, state)?;
    if let Some(explicit) = value(args, "--remote")
        && bridgeforge_memory::normalize_remote(&explicit) != approved
    {
        return Err("--remote differs from the approved native memories remote".into());
    }
    Ok(approved)
}

fn authorized_remote(ledger: &Path, state: &Path) -> Result<String, String> {
    bridgeforge_memory::require_runtime_authorization(ledger, &state.join("remote.txt"))
        .map_err(|error| error.to_string())?
        .remote
        .ok_or_else(|| "approved native memories authorization has no remote".into())
}

fn memory_operation_outcome(
    state_dir: &Path,
    result: bridgeforge_memory::MemoryResult<String>,
) -> CommandOutcome {
    match result {
        Ok(action) => {
            if action == "busy" {
                return CommandOutcome::with_receipt(
                    json!({"schema":1,"status":"busy","action":action}),
                );
            }
            let status = match action.as_str() {
                "conflicted" => "conflicted",
                "busy" => "busy",
                _ => "healthy",
            };
            if let Err(error) =
                bridgeforge_memory::record_health(state_dir, status, None, Some(&action))
            {
                return blocked("memory-sync", error);
            }
            CommandOutcome::with_receipt(json!({"schema": 1, "status": status, "action": action}))
        }
        Err(error) => {
            let detail = error.to_string();
            let _ = bridgeforge_memory::record_health(state_dir, "failed", Some(&detail), None);
            blocked("memory-sync", detail)
        }
    }
}

fn memory_failure_outcome(state_dir: &Path, detail: impl ToString) -> CommandOutcome {
    let detail = detail.to_string();
    let _ = bridgeforge_memory::record_health(state_dir, "failed", Some(&detail), None);
    blocked("memory-sync", detail)
}

fn migrate_memory_state(codex: &Path, state: &Path, ledger: &Path) -> Result<(), String> {
    bridgeforge_memory::migration::migrate_legacy_state(codex, state, ledger)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn record_hook_attempt(
    state: &Path,
    event: &str,
    status: &str,
    started_utc: &str,
    detail: Option<&str>,
) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    bridgeforge_core::persistence::atomic_write_json(
        &state.join("hook-attempt.json"),
        &json!({
            "schema": 1,
            "hookId": bridgeforge_memory::user_config::HOOK_ID,
            "event": event,
            "status": status,
            "executablePath": executable,
            "binaryVersion": env!("CARGO_PKG_VERSION"),
            "startedUtc": started_utc,
            "completedUtc": (status != "started").then(bridgeforge_memory::utc_now),
            "detail": detail,
        }),
    )
    .map_err(|error| error.to_string())
}

fn hook_failure_outcome(
    args: &[String],
    state: &Path,
    event: &str,
    started_utc: &str,
    detail: impl ToString,
) -> CommandOutcome {
    let detail = detail.to_string();
    let _ = record_hook_attempt(state, event, "failed", started_utc, Some(&detail));
    let mut outcome = memory_failure_outcome(state, detail);
    if let Ok(notice) = memory_hook_notice(args, state, event) {
        outcome.receipt = notice.receipt;
    }
    outcome
}

fn launch_memory_worker(args: &[String], state: &Path) -> Result<String, String> {
    use bridgeforge_memory::worker::{WorkerReservation, reserve_worker};
    let reservation = reserve_worker(state).map_err(|error| error.to_string())?;
    let WorkerReservation::Acquired(worker) = reservation else {
        return Ok("reused".into());
    };
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let codex = path_value(args, "--codex-home")?;
    let mut worker_args: Vec<OsString> = vec![
        "memory-sync".into(),
        "worker".into(),
        "--codex-home".into(),
        codex.into_os_string(),
        "--token".into(),
        worker.token.clone().into(),
    ];
    for flag in ["--state-dir", "--memories"] {
        if let Some(path) = value(args, flag) {
            worker_args.extend([flag.into(), path.into()]);
        }
    }
    bridgeforge_memory::background::spawn(&executable, &worker_args).map_err(|error| {
        let _ = bridgeforge_memory::worker::release_worker(state, &worker.token);
        format!("cannot launch hidden memory worker: {error}")
    })?;
    Ok("launched".into())
}

pub(super) fn memory_sync(args: &[String]) -> CommandOutcome {
    let Some(command) = args.first().map(String::as_str) else {
        return blocked("memory-sync", "a subcommand is required");
    };
    let (codex, memories, state_dir, ledger) = match memory_paths(args) {
        Ok(value) => value,
        Err(error) => return blocked("memory-sync", error),
    };
    match command {
        "setup" => {
            let Some(remote) = value(args, "--remote") else {
                return blocked("memory-sync", "setup requires --remote");
            };
            if let Err(error) = bridgeforge_memory::MemoryRemoteClient::new(&SystemProcessRunner)
                .verify_private_github_repository(&codex, &remote)
            {
                return blocked("memory-sync", error);
            }
            let binary = match std::env::current_exe() {
                Ok(value) => value,
                Err(error) => return blocked("memory-sync", error),
            };
            match bridgeforge_memory::user_config::configure(
                &codex,
                &binary,
                &remote,
                has(args, "--confirmed-enable"),
            ) {
                Ok(authorization) => match migrate_memory_state(&codex, &state_dir, &ledger) {
                    Ok(()) => CommandOutcome::with_receipt(json!({
                        "schema": 1,
                        "status": "configured",
                        "hookInstalled": true,
                        "hookConfigured": true,
                        "runtime": binary,
                        "authorization": authorization,
                    })),
                    Err(error) => memory_failure_outcome(&state_dir, error),
                },
                Err(error) => blocked("memory-sync", error),
            }
        }
        "decline" => {
            match bridgeforge_memory::record_native_memories_consent(
                &ledger,
                "declined",
                has(args, "--confirmed"),
                None,
            ) {
                Ok(changed) => CommandOutcome::with_receipt(
                    json!({"schema": 1, "status": "declined", "changed": changed}),
                ),
                Err(error) => blocked("memory-sync", error),
            }
        }
        "maintain" | "repair-hook" => {
            if let Err(error) = migrate_memory_state(&codex, &state_dir, &ledger) {
                return memory_failure_outcome(&state_dir, error);
            }
            if let Err(error) = authorized_remote(&ledger, &state_dir) {
                return blocked("memory-sync", error);
            }
            if !bridgeforge_memory::user_config::memories_enabled(&codex) {
                return blocked(
                    "memory-sync",
                    "native memories are disabled by the user; hook repair is not authorized",
                );
            }
            let binary = match std::env::current_exe() {
                Ok(value) => value,
                Err(error) => return blocked("memory-sync", error),
            };
            match bridgeforge_memory::user_config::merge_user_hooks(&codex, &binary) {
                Ok(changed) => CommandOutcome::with_receipt(
                    json!({"schema": 1, "status": "healthy", "hookRepair": if changed {"applied"} else {"unchanged"}}),
                ),
                Err(error) => blocked("memory-sync", error),
            }
        }
        "mark" => {
            let trigger = value(args, "--trigger").unwrap_or_else(|| "bridgeforge".into());
            match bridgeforge_memory::worker::mark_pending(&state_dir, &trigger) {
                Ok(state) => CommandOutcome::with_receipt(serde_json::to_value(state).unwrap()),
                Err(error) => blocked("memory-sync", error),
            }
        }
        "status" => {
            let authorization = if ledger.is_file() {
                bridgeforge_memory::native_memories_authorization(&ledger)
            } else {
                Ok(None)
            };
            let authorization_error = authorization.as_ref().err().map(ToString::to_string);
            let authorization = authorization.ok().flatten();
            let consent = authorization.as_ref().map(|value| value.decision.clone());
            let enabled = bridgeforge_memory::user_config::memories_enabled(&codex);
            let binary = std::env::current_exe().ok();
            let hook_installed = binary.as_deref().is_some_and(|binary| {
                bridgeforge_memory::user_config::user_hooks_healthy(&codex, binary)
            });
            let hook_runtime: Option<Value> = fs::read(state_dir.join("hook-runtime.json"))
                .ok()
                .and_then(|payload| serde_json::from_slice(&payload).ok());
            let hook_runtime_verified = hook_runtime
                .as_ref()
                .is_some_and(bridgeforge_memory::runtime_receipt_healthy);
            let hook_attempt: Option<Value> = fs::read(state_dir.join("hook-attempt.json"))
                .ok()
                .and_then(|payload| serde_json::from_slice(&payload).ok());
            let hook_dispatch_observed = hook_attempt.as_ref().is_some_and(|receipt| {
                receipt["schema"].as_u64() == Some(1)
                    && receipt["hookId"].as_str() == Some(bridgeforge_memory::user_config::HOOK_ID)
                    && receipt["event"].as_str().is_some_and(|event| {
                        bridgeforge_memory::user_config::HOOK_EVENTS.contains(&event)
                    })
            });
            let configured_remote = fs::read_to_string(state_dir.join("remote.txt"))
                .ok()
                .map(|value| bridgeforge_memory::normalize_remote(&value));
            let remote_configured = authorization
                .as_ref()
                .and_then(|value| value.remote.as_deref())
                .zip(configured_remote.as_deref())
                .is_some_and(|(authorized, configured)| authorized == configured);
            let pending = bridgeforge_memory::worker::read_pending(&state_dir)
                .ok()
                .flatten();
            let pending_age_seconds = pending
                .as_ref()
                .and_then(|_| bridgeforge_memory::worker::pending_age(&state_dir).ok())
                .map(|age| age.as_secs())
                .unwrap_or(0);
            let worker = bridgeforge_memory::worker::read_worker_state(&state_dir)
                .ok()
                .flatten();
            let worker_active = worker
                .as_ref()
                .is_some_and(bridgeforge_memory::worker::worker_is_live);
            let conflict: Option<Value> = fs::read(state_dir.join("active-conflict.json"))
                .ok()
                .and_then(|payload| serde_json::from_slice(&payload).ok());
            let last_receipt: Option<Value> = fs::read(state_dir.join("last-synced.json"))
                .ok()
                .and_then(|payload| serde_json::from_slice(&payload).ok());
            let health_receipt = bridgeforge_memory::read_health(&state_dir).ok().flatten();
            let migration = bridgeforge_memory::migration::status(&codex, &state_dir);
            let migration_error = migration.as_ref().err().map(ToString::to_string);
            let migration = migration.ok();
            let runtime_error = memory_state_error(&state_dir, worker_active);
            let (sync_health, active_alert_id) = if runtime_error.is_some() {
                (
                    "failed",
                    Some("native-memory:runtime-state-invalid".to_string()),
                )
            } else if migration_error.is_some() {
                (
                    "failed",
                    Some("native-memory:state-migration-invalid".to_string()),
                )
            } else if authorization_error.is_some() {
                (
                    "failed",
                    Some("native-memory:authorization-invalid".to_string()),
                )
            } else if conflict.is_some() {
                (
                    "conflicted",
                    conflict
                        .as_ref()
                        .and_then(|value| value["conflictId"].as_str())
                        .map(|id| format!("native-memory:conflict:{id}")),
                )
            } else if health_receipt
                .as_ref()
                .and_then(|value| value["status"].as_str())
                == Some("failed")
            {
                (
                    "failed",
                    health_receipt
                        .as_ref()
                        .and_then(|value| value["alertId"].as_str())
                        .map(str::to_string),
                )
            } else if pending_age_seconds > 300 {
                (
                    "degraded",
                    pending.as_ref().map(|value| {
                        format!("native-memory:pending-stale:{}", value.first_pending_utc)
                    }),
                )
            } else if worker_active {
                ("busy", None)
            } else if pending.is_some() {
                ("pending", None)
            } else if consent.as_deref() == Some("approved")
                && enabled
                && hook_installed
                && hook_runtime_verified
                && remote_configured
                && last_receipt.is_some()
            {
                ("healthy", None)
            } else {
                ("gap", None)
            };
            let alert_state: Option<Value> = fs::read(state_dir.join("alert-state.json"))
                .ok()
                .and_then(|payload| serde_json::from_slice(&payload).ok());
            let alert_id = active_alert_id.clone().filter(|active| {
                alert_state
                    .as_ref()
                    .and_then(|value| value["lastAcknowledged"].as_str())
                    != Some(active.as_str())
            });
            let code = if sync_health == "failed" {
                EXIT_BLOCKED
            } else {
                0
            };
            CommandOutcome {
                code,
                receipt: Some(json!({
                "schema": 1,
                "consent": consent,
                "enabled": enabled,
                "disabledByUser": consent.as_deref() == Some("approved") && !enabled,
                "hookInstalled": hook_installed,
                "hookConfigured": hook_installed,
                "hookDispatchObserved": hook_dispatch_observed,
                "hookRuntimeVerified": hook_runtime_verified,
                "hookAttempt": hook_attempt,
                "hookRuntime": hook_runtime,
                "remoteConfigured": remote_configured,
                "pending": pending,
                "pendingAgeSeconds": pending_age_seconds,
                "worker": worker,
                "workerActive": worker_active,
                "activeConflict": conflict,
                "lastReceipt": last_receipt,
                "healthReceipt": health_receipt,
                "stateMigration": migration,
                "stateMigrationNeeded": migration.as_ref().and_then(|value| value["needed"].as_bool()).unwrap_or(false),
                "stateMigrationCompleted": migration.as_ref().and_then(|value| value["completed"].as_bool()).unwrap_or(false),
                "stateMigrationError": migration_error,
                "syncHealth": sync_health,
                "alertId": alert_id,
                "activeAlertId": active_alert_id,
                "error": authorization_error,
                "runtimeStateError": runtime_error,
                })),
                ..CommandOutcome::default()
            }
        }
        "ack-alert" => {
            let Some(alert_id) = value(args, "--alert-id") else {
                return blocked("memory-sync", "ack-alert requires --alert-id");
            };
            match bridgeforge_memory::acknowledge_alert(&state_dir, &alert_id) {
                Ok(()) => CommandOutcome::with_receipt(
                    json!({"schema": 1, "status": "acknowledged", "alertId": alert_id}),
                ),
                Err(error) => blocked("memory-sync", error),
            }
        }
        "reconcile" => {
            if let Err(error) = migrate_memory_state(&codex, &state_dir, &ledger) {
                return memory_failure_outcome(&state_dir, error);
            }
            let remote = match authorized_memory_remote(args, &ledger, &state_dir) {
                Ok(value) => value,
                Err(error) => return blocked("memory-sync", error),
            };
            memory_operation_outcome(
                &state_dir,
                bridgeforge_memory::remote::reconcile(
                    &memories,
                    &state_dir,
                    &remote,
                    &SystemProcessRunner,
                ),
            )
        }
        "resolve" => {
            let Some(conflict_id) = value(args, "--conflict-id") else {
                return blocked("memory-sync", "resolve requires --conflict-id");
            };
            if let Err(error) = migrate_memory_state(&codex, &state_dir, &ledger) {
                return memory_failure_outcome(&state_dir, error);
            }
            let remote = match authorized_memory_remote(args, &ledger, &state_dir) {
                Ok(value) => value,
                Err(error) => return memory_failure_outcome(&state_dir, error),
            };
            let choices = match values(args, "--choose")
                .into_iter()
                .map(|choice| {
                    choice
                        .split_once('=')
                        .map(|(path, side)| (path.to_string(), side.to_string()))
                        .ok_or_else(|| format!("invalid conflict choice: {choice}"))
                })
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(value) => value,
                Err(error) => return blocked("memory-sync", error),
            };
            memory_operation_outcome(
                &state_dir,
                bridgeforge_memory::remote::resolve_conflict_with_choices(
                    &memories,
                    &state_dir,
                    &remote,
                    &conflict_id,
                    &choices,
                    &SystemProcessRunner,
                ),
            )
        }
        "kick" => {
            let trigger = value(args, "--trigger").unwrap_or_else(|| "bridgeforge".into());
            if let Err(error) = migrate_memory_state(&codex, &state_dir, &ledger) {
                return memory_failure_outcome(&state_dir, error);
            }
            if let Err(error) = authorized_remote(&ledger, &state_dir) {
                return memory_failure_outcome(&state_dir, error);
            }
            if let Err(error) = bridgeforge_memory::worker::mark_pending(&state_dir, &trigger) {
                return memory_failure_outcome(&state_dir, error);
            }
            match launch_memory_worker(args, &state_dir) {
                Ok(action) => CommandOutcome::with_receipt(
                    json!({"schema": 1, "status": "pending", "worker": action}),
                ),
                Err(error) => memory_failure_outcome(&state_dir, error),
            }
        }
        "worker" => {
            let Some(token) = value(args, "--token") else {
                return blocked("memory-sync", "worker requires --token");
            };
            match bridgeforge_memory::worker::mark_worker_started(
                &state_dir,
                &token,
                std::process::id(),
            ) {
                Ok(true) => {}
                Ok(false) => {
                    return blocked("memory-sync", "worker reservation is no longer owned");
                }
                Err(error) => return blocked("memory-sync", error),
            }
            let remote = match authorized_remote(&ledger, &state_dir) {
                Ok(value) => value,
                Err(error) => {
                    let _ = bridgeforge_memory::worker::release_worker(&state_dir, &token);
                    return memory_failure_outcome(&state_dir, error);
                }
            };
            let result = bridgeforge_memory::worker::drain_pending(&state_dir, &token, || {
                bridgeforge_memory::remote::reconcile(
                    &memories,
                    &state_dir,
                    &remote,
                    &SystemProcessRunner,
                )
            })
            .and_then(|(action, restart)| {
                if restart {
                    launch_memory_worker(args, &state_dir)
                        .map_err(bridgeforge_memory::MemorySyncError::new)?;
                }
                Ok(action)
            });
            memory_operation_outcome(&state_dir, result)
        }
        "hook-run" => {
            let Some(event) = value(args, "--event") else {
                return blocked("memory-sync", "hook-run requires --event");
            };
            if !bridgeforge_memory::user_config::HOOK_EVENTS.contains(&event.as_str()) {
                return blocked("memory-sync", format!("unsupported hook event: {event}"));
            }
            if !bridgeforge_memory::user_config::memories_enabled(&codex) {
                return CommandOutcome::ok();
            }
            let started_utc = bridgeforge_memory::utc_now();
            if let Err(error) =
                record_hook_attempt(&state_dir, &event, "started", &started_utc, None)
            {
                return blocked("memory-sync", error);
            }
            if let Err(error) = migrate_memory_state(&codex, &state_dir, &ledger) {
                return hook_failure_outcome(args, &state_dir, &event, &started_utc, error);
            }
            if let Err(error) = authorized_remote(&ledger, &state_dir) {
                return hook_failure_outcome(args, &state_dir, &event, &started_utc, error);
            }
            let trigger = event.to_lowercase();
            if let Err(error) = bridgeforge_memory::worker::mark_pending(&state_dir, &trigger) {
                return hook_failure_outcome(args, &state_dir, &event, &started_utc, error);
            }
            match launch_memory_worker(args, &state_dir) {
                Ok(worker) => {
                    let completed_utc = bridgeforge_memory::utc_now();
                    if let Err(error) = record_hook_attempt(
                        &state_dir,
                        &event,
                        "completed",
                        &started_utc,
                        Some(&format!("worker-{worker}")),
                    ) {
                        return hook_failure_outcome(args, &state_dir, &event, &started_utc, error);
                    }
                    let executable = std::env::current_exe().ok();
                    if let Err(error) = bridgeforge_core::persistence::atomic_write_json(
                        &state_dir.join("hook-runtime.json"),
                        &json!({
                            "schema": 1,
                            "hookId": bridgeforge_memory::user_config::HOOK_ID,
                            "lastEvent": event,
                            "handlerRevision": bridgeforge_memory::user_config::HOOK_ID,
                            "executablePath": executable,
                            "binaryVersion": env!("CARGO_PKG_VERSION"),
                            "startedUtc": started_utc,
                            "verifiedUtc": completed_utc,
                        }),
                    ) {
                        return hook_failure_outcome(args, &state_dir, &event, &started_utc, error);
                    }
                    match memory_hook_notice(args, &state_dir, &event) {
                        Ok(outcome) => outcome,
                        Err(error) => {
                            hook_failure_outcome(args, &state_dir, &event, &started_utc, error)
                        }
                    }
                }
                Err(error) => hook_failure_outcome(args, &state_dir, &event, &started_utc, error),
            }
        }
        _ => blocked("memory-sync", format!("unknown subcommand: {command}")),
    }
}

fn memory_state_error(state: &Path, worker_active: bool) -> Option<String> {
    let check = || -> Result<(), String> {
        for file in [
            "pending.json",
            "worker.json",
            "health.json",
            "active-conflict.json",
            "last-synced.json",
        ] {
            let path = state.join(file);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("{file}: {error}")),
            };
            #[cfg(windows)]
            let linked = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let linked = metadata.file_type().is_symlink();
            if linked || !metadata.is_file() {
                return Err(format!("{file} is not a plain file"));
            }
            let bytes = match fs::read(&path) {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("{file}: {error}")),
            };
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|e| format!("{file}: {e}"))?;
            if !value.is_object() {
                return Err(format!("{file} is not an object"));
            }
            let valid = match file {
                "pending.json" => {
                    serde_json::from_value::<bridgeforge_memory::worker::PendingState>(
                        value.clone(),
                    )
                    .is_ok_and(|v| v.is_valid())
                }
                "worker.json" => {
                    serde_json::from_value::<bridgeforge_memory::worker::WorkerState>(value.clone())
                        .is_ok_and(|v| v.is_valid())
                }
                "health.json" => value["schema"].as_u64() == Some(1) && value["status"].is_string(),
                "active-conflict.json" => value["conflictId"].is_string(),
                "last-synced.json" => {
                    value["schemaVersion"].as_u64() == Some(2)
                        && value["content_sha256"].is_string()
                }
                _ => true,
            };
            if !valid {
                return Err(format!("{file} has invalid fields"));
            }
        }
        if !worker_active {
            bridgeforge_memory::remote::validate_synced_baseline(state)
                .map_err(|e| format!("baseline: {e}"))?;
        }
        Ok(())
    };
    check().err()
}

fn memory_hook_notice(
    args: &[String],
    state: &Path,
    event: &str,
) -> Result<CommandOutcome, String> {
    if event != "SessionStart" {
        return Ok(CommandOutcome::ok());
    }
    let mut query = args.to_vec();
    query[0] = "status".into();
    let status = memory_sync(&query)
        .receipt
        .ok_or("memory status receipt is unavailable")?;
    let message = match status["syncHealth"].as_str() {
        Some("failed") => {
            "原生 Memory 自动同步失败，待同步任务仍保留。请检查 Memory 同步状态并处理错误。"
        }
        Some("conflicted") => "原生 Memory 自动同步遇到文件冲突，已保留两边版本，需要确认后继续。",
        Some("degraded") => "原生 Memory 自动同步已超过五分钟未完成，请检查后台同步状态。",
        _ => return Ok(CommandOutcome::ok()),
    };
    let emitted = bridgeforge_memory::emit_alert_once(state, status["alertId"].as_str())
        .map_err(|error| error.to_string())?;
    Ok(if emitted.is_some() {
        CommandOutcome::with_receipt(json!({"systemMessage":message}))
    } else {
        CommandOutcome::ok()
    })
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/cli_memory.rs"]
mod tests;
