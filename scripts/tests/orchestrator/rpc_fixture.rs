//! Test-only subprocess, never built by the product's default features.
use serde_json::{Value, json};
use std::io::{BufRead, Write};
fn send(value: Value) {
    println!("{value}");
    std::io::stdout().flush().unwrap();
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("develop") {
        let value = std::fs::read_to_string(".bp-fixture-development.json").unwrap();
        println!("{value}");
        return;
    }
    let mode = std::fs::read_to_string(".bp-fixture-mode").unwrap_or_else(|_| "complete".into());
    std::fs::write(".bp-fixture-pid", std::process::id().to_string()).unwrap();
    let mut thread = String::new();
    for line in std::io::stdin().lock().lines() {
        let v: Value = serde_json::from_str(&line.unwrap()).unwrap();
        match v["method"].as_str().unwrap_or("") {
            "initialize" => {
                if mode == "record-client-version" {
                    std::fs::write(".bp-fixture-client-version", v["params"]["clientInfo"]["version"].as_str().unwrap()).unwrap();
                }
                if mode == "hang-initialize" {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                }
                send(json!({"id":v["id"],"result":{"userAgent":"controlled-fixture"}}));
            }
            "initialized" => {}
            "project/list" => {
                let project = json!({"id":"fixture-server-project","roots":[{"path":std::env::current_dir().unwrap()}]});
                let data = if mode == "ambiguous-project" {
                    vec![
                        project.clone(),
                        json!({"id":"duplicate-project","roots":project["roots"]}),
                    ]
                } else {
                    vec![project]
                };
                send(json!({"id":v["id"],"result":{"data":data,"nextCursor":null}}));
            }
            "project/read" => send(
                json!({"id":v["id"],"result":{"project":{"id":v["params"]["projectId"],"roots":[{"path":std::env::current_dir().unwrap()}]}}}),
            ),
            "thread/list" => {
                let project = std::fs::read_to_string(".bp-fixture-project").ok();
                let thread_id =
                    std::fs::read_to_string(".bp-fixture-native-thread").unwrap_or_default();
                send(
                    json!({"id":v["id"],"result":{"data":[{"id":thread_id,"projectId":project,"cwd":std::env::current_dir().unwrap()}],"nextCursor":null}}),
                );
            }
            "thread/metadata/update" => {
                if mode == "pause-group" {
                    std::fs::write(".bp-fixture-grouping", "1").unwrap();
                    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    while !std::path::Path::new(".bp-fixture-group-release").exists() {
                        assert!(std::time::Instant::now() < until);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
                std::fs::write(
                    ".bp-fixture-project",
                    v["params"]["projectId"].as_str().unwrap(),
                )
                .unwrap();
                std::fs::write(
                    ".bp-fixture-native-thread",
                    v["params"]["threadId"].as_str().unwrap(),
                )
                .unwrap();
                {
                    let mut log = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(".bp-fixture-project-updates")
                        .unwrap();
                    writeln!(log, "update").unwrap();
                }
                if mode == "drop-group-response" {
                    std::process::exit(0);
                }
                send(
                    json!({"id":v["id"],"result":{"thread":{"id":v["params"]["threadId"],"projectId":v["params"]["projectId"],"cwd":std::env::current_dir().unwrap()}}}),
                );
            }
            "thread/unarchive" => {
                if mode == "pause-unarchive" {
                    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    std::fs::write(".bp-fixture-restoring", "1").unwrap();
                    while !std::path::Path::new(".bp-fixture-restore-release").exists() {
                        assert!(std::time::Instant::now() < until);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
                let id = if mode == "wrong-unarchive" {
                    json!("wrong-native-chat")
                } else {
                    v["params"]["threadId"].clone()
                };
                send(
                    json!({"id":v["id"],"result":{"thread":{"id":id,"status":{"type":"notLoaded"}}}}),
                )
            }
            "thread/resume" => {
                thread = v["params"]["threadId"].as_str().unwrap().into();
                let sandbox = if v["params"]["sandbox"] == "read-only" {
                    "readOnly"
                } else {
                    "workspaceWrite"
                };
                let mut result = json!({"thread":{"id":thread,"turns":[]},"cwd":std::env::current_dir().unwrap(),"sandbox":{"type":sandbox},"model":"fixture-model","reasoningEffort":"fixture-effort","instructionSources":[std::env::current_dir().unwrap().join("AGENTS.md")]});
                match mode.as_str() {
                    "missing-turns" => {
                        result["thread"].as_object_mut().unwrap().remove("turns");
                    }
                    "wrong-thread" => result["thread"]["id"] = json!("another-chat"),
                    "unknown-status" => {
                        result["thread"]["turns"] = json!([{"id":"old-turn","status":"unexpected"}])
                    }
                    _ => {}
                }
                send(json!({"id":v["id"],"result":result}));
                if mode == "hang-reading-input" {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                }
            }
            "turn/start" => {
                std::fs::write(".bp-fixture-started", "started").unwrap();
                {
                    use std::io::Write;
                    let mut count = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(".bp-fixture-start-count")
                        .unwrap();
                    writeln!(count, "start").unwrap();
                }
                if mode == "hang-turn-start" {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                }
                if matches!(
                    mode.as_str(),
                    "lost-response" | "assistant-marker" | "setup-marker" | "wrong-operation"
                ) {
                    let mut content = v["params"]["input"].clone();
                    if mode == "wrong-operation" {
                        let text = content[0]["text"].as_str().unwrap();
                        let (first, rest) = text.split_once('\n').unwrap();
                        let mut header: Value = serde_json::from_str(
                            first.strip_prefix("BridgePilot dispatch ").unwrap(),
                        )
                        .unwrap();
                        header["operation"] = json!("unrelated-operation");
                        content[0]["text"] =
                            json!(format!("BridgePilot dispatch {header}\n{rest}"));
                    }
                    if mode == "setup-marker" {
                        content[0]["text"] = json!(format!(
                            "Setup acknowledgement quoting {}",
                            content[0]["text"]
                        ));
                    }
                    let kind = if mode == "assistant-marker" {
                        "agentMessage"
                    } else {
                        "userMessage"
                    };
                    let completed = json!({"id":"fixture-turn","status":"completed","items":[{"type":kind,"content":content}]});
                    std::fs::write(".bp-fixture-turn.json", completed.to_string()).unwrap();
                    std::process::exit(0);
                }
                send(
                    json!({"id":v["id"],"result":{"turn":{"id":"fixture-turn","status":"inProgress"}}}),
                );
                if mode == "complete" || mode == "barrier-complete" {
                    if mode == "barrier-complete" {
                        let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
                        while !std::path::Path::new(".bp-fixture-release").exists() {
                            assert!(
                                std::time::Instant::now() < until,
                                "completion barrier timeout"
                            );
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }
                    send(
                        json!({"method":"item/completed","params":{"threadId":thread,"turnId":"fixture-turn","item":{"id":"fixture-command","type":"commandExecution","command":"fixture-read","cwd":std::env::current_dir().unwrap(),"status":"completed","exitCode":0,"aggregatedOutput":"controlled fixture read report"}}}),
                    );
                    send(
                        json!({"method":"item/completed","params":{"threadId":thread,"turnId":"fixture-turn","item":{"type":"agentMessage","text":"fixture completion is not a publication receipt"}}}),
                    );
                    send(
                        json!({"method":"turn/completed","params":{"threadId":thread,"turn":{"id":"fixture-turn","status":"completed"}}}),
                    );
                } else if mode == "approval" {
                    send(
                        json!({"id":900,"method":"item/commandExecution/requestApproval","params":{"command":"fixture requires explicit approval"}}),
                    );
                }
            }
            "turn/interrupt" => {
                if mode == "hang-interrupt" {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                }
                send(json!({"id":v["id"],"result":{}}));
                send(
                    json!({"method":"turn/completed","params":{"threadId":thread,"turn":{"id":"fixture-turn","status":"interrupted"}}}),
                );
            }
            "thread/goal/get" => send(
                json!({"id":v["id"],"result":{"goal":if mode=="observer-goal-active"{json!({"status":"active"})}else if mode=="observer-goal-paused"{json!({"status":"paused"})}else{Value::Null}}}),
            ),
            "thread/read" => {
                if v["params"]["includeTurns"] == false {
                    let project = std::fs::read_to_string(".bp-fixture-project").ok();
                    send(
                        json!({"id":v["id"],"result":{"thread":{"id":v["params"]["threadId"],"projectId":project,"cwd":std::env::current_dir().unwrap(),"status":{"type":"notLoaded"},"turns":[]}}}),
                    );
                    continue;
                }
                if mode == "read-barrier" {
                    std::fs::write(".bp-fixture-reading", "ready").unwrap();
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    while !std::path::Path::new(".bp-fixture-release").exists() {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "fixture barrier timeout"
                        );
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
                let turn: Value = std::fs::read_to_string(".bp-fixture-turn.json")
                    .map(|v| serde_json::from_str(&v).unwrap())
                    .unwrap_or(Value::Null);
                send(
                    json!({"id":v["id"],"result":{"thread":{"id":v["params"]["threadId"],"status":{"type":if mode=="observer-active"{"active"}else{"notLoaded"}},"turns":if turn.is_null(){vec![]}else{vec![turn]}}}}),
                );
            }
            "" => {
                if v["id"] == 900 {
                    assert_eq!(v["result"]["decision"], "decline");
                }
            }
            other => send(json!({"id":v["id"],"error":{"code":-32601,"message":other}})),
        }
    }
}
