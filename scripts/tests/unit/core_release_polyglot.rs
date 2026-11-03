use super::*;

fn identity(id: &str, path: &str) -> Value {
    json!({"id":id,"language":"python","manifest":path,"name":id})
}
fn config() -> Value {
    json!({"schema_version":3,"release_policy":"explicit_release","strategy":"independent","adapters":{"python":{"projects":["a/pyproject.toml","b/pyproject.toml"],"local_sources":{"b/pyproject.toml":{"a":"a/pyproject.toml"}}}},"packages":[identity("a","a/pyproject.toml"),identity("b","b/pyproject.toml")],"product_inputs":["README.md",".gitignore"]})
}

#[test]
fn product_input_redundancy_preserves_declared_bytes_and_owner() {
    let mut value = config();
    value["product_inputs"] = json!([
        "scripts/",
        "scripts/start.ps1",
        "scripts/tools/",
        "scripts/tools/run.ps1"
    ]);
    let bytes = serde_json::to_vec(&value).unwrap();
    let parsed = parse(&bytes).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&parsed.product_inputs).unwrap(),
        value["product_inputs"]
    );
    let snapshot = Snapshot::default();
    for path in [
        "scripts/start.ps1",
        "scripts/tools/run.ps1",
        "scripts/future.ps1",
    ] {
        assert_eq!(
            owner(&snapshot, path, &parsed).unwrap(),
            Vec::<String>::new()
        );
    }
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), value);
}

#[test]
fn product_input_redundancy_does_not_hide_shared_conflicts() {
    let mut product_shared = config();
    product_shared["product_inputs"] = json!(["scripts/", "scripts/start.ps1"]);
    product_shared["shared_inputs"] = json!([{"path":"scripts/start.ps1","packages":["a"]}]);
    assert!(
        parse(&serde_json::to_vec(&product_shared).unwrap())
            .unwrap_err()
            .contains("overlapping")
    );
    for owners in [json!(["a"]), json!(["b"])] {
        let mut shared_shared = config();
        shared_shared["shared_inputs"] = json!([
            {"path":"scripts/","packages":["a"]},
            {"path":"scripts/start.ps1","packages":owners}
        ]);
        assert!(
            parse(&serde_json::to_vec(&shared_shared).unwrap())
                .unwrap_err()
                .contains("overlapping")
        );
    }
}

#[test]
fn product_input_redundancy_keeps_duplicate_and_unsafe_path_guards() {
    for paths in [
        vec!["scripts/", "scripts/"],
        vec!["scripts/", "scripts/start.ps1", "scripts/start.ps1"],
    ] {
        let mut duplicate = config();
        duplicate["product_inputs"] = json!(paths);
        assert!(
            parse(&serde_json::to_vec(&duplicate).unwrap())
                .unwrap_err()
                .contains("overlapping")
        );
    }
    let mut unsafe_path = config();
    unsafe_path["product_inputs"] = json!(["scripts/", "scripts/../private"]);
    assert!(
        parse(&serde_json::to_vec(&unsafe_path).unwrap())
            .unwrap_err()
            .contains("unsafe")
    );
}
