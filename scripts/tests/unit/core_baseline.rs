#[path = "../support/factory_root.rs"]
mod factory_repository;

use super::*;

#[test]
fn minimum_supported_skeleton_version_rejects_old_projects_without_writes() {
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "baseline")
        .join(format!("bf-support-floor-{token}"));
    fs::create_dir_all(root.join(".codex")).unwrap();
    for version in ["0.1.0", "7.0.5", "8.0.0", "8.0.1", "9.0.0"] {
        let stamp = root.join(".codex/.bridgeforge_codex_version");
        fs::write(&stamp, version).unwrap();
        let result = require_supported_project(&root);
        assert_eq!(
            result.is_ok(),
            version.parse::<bridgeforge_core::SemVer>().unwrap()
                >= MIN_SUPPORTED_VERSION.parse().unwrap(),
            "{version}: {result:?}"
        );
        assert_eq!(fs::read_to_string(stamp).unwrap(), version);
        assert!(!root.join(".runtime").exists());
    }
    fs::remove_file(root.join(".codex/.bridgeforge_codex_version")).unwrap();
    for unknown in [
        b"{}".as_slice(),
        br#"{"release_version":null}"#,
        br#"{"release_version":7}"#,
        b"not-json",
    ] {
        let path = root.join(".codex/managed-skeleton.json");
        fs::write(&path, unknown).unwrap();
        assert!(require_supported_project(&root).is_err());
        assert_eq!(fs::read(path).unwrap(), unknown);
        assert!(!root.join(".runtime").exists());
    }
    fs::write(
        root.join(".codex/managed-skeleton.json"),
        br#"{"release_version":"7.0.5"}"#,
    )
    .unwrap();
    assert!(
        require_supported_project(&root)
            .unwrap_err()
            .contains("minimum supported")
    );
    assert!(!root.join(".runtime").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn minimum_supported_version_does_not_use_business_or_package_versions() {
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "baseline")
        .join(format!("bf-support-business-{token}"));
    fs::create_dir_all(root.join(".codex")).unwrap();
    fs::write(root.join("VERSION"), b"0.1.0\n").unwrap();
    fs::write(
        root.join("Cargo.toml"),
        b"[package]\nname='business'\nversion='0.1.0'\n",
    )
    .unwrap();
    assert!(
        require_supported_project(&root).is_ok(),
        "new projects remain supported"
    );
    fs::write(root.join(".codex/.bridgeforge_codex_version"), b"8.0.0\n").unwrap();
    assert!(require_supported_project(&root).is_ok());
    assert_eq!(fs::read(root.join("VERSION")).unwrap(), b"0.1.0\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn strict_attribute_inspection_accepts_proven_defaults_and_never_falls_back() {
    for payload in [
        b"# defaults\n* text=auto eol=lf\n".as_slice(),
        b"* eol=lf text=auto\r\n",
        b"* text=auto eol=lf\n*.bin -text\n",
    ] {
        verify_gitattributes_read_only(payload).unwrap();
    }
    for payload in [
        b"# no rule\n".as_slice(),
        b"* text=auto eol=lf\nnested/* -text\n",
        b"[attr]default text=auto eol=lf\n* default\n",
    ] {
        assert!(verify_gitattributes_read_only(payload).is_err());
    }
}

#[test]
fn markdown_characterization_projection_boundaries_and_keys() {
    assert_eq!(
        heading_section("前言\n## 索引\n正文\n# 尾部".as_bytes(), "## 索引").unwrap(),
        "## 索引\n正文\n".as_bytes()
    );
    assert_eq!(
        heading_section(b"```md\n## Index\ninside\n```\n", "## Index").unwrap(),
        b"## Index\ninside\n```\n"
    );
    let unicode_rows =
        table_rows("| K | V |\n|---|---|\n| [`标题`](文档/Ä.md) | 保留 |\n".as_bytes()).unwrap();
    assert_eq!(
        unicode_rows["文档/ä.md"],
        "| [`标题`](文档/Ä.md) | 保留 |\n".as_bytes()
    );
    for ending in ["\n", "\r\n", "\r"] {
        let text = "# Root\n## Index\nbody\n### Child\nnested\n# End\ntail".replace('\n', ending);
        assert_eq!(
            heading_section(text.as_bytes(), "## Index").unwrap(),
            b"## Index\nbody\n### Child\nnested\n"
        );
    }
    assert_eq!(
        heading_section(b"## Index\nlast", "## Index").unwrap(),
        b"## Index\nlast"
    );
    assert_eq!(
        heading_section(b"## Index\nx\n  ## Next\ny", "## Index").unwrap(),
        b"## Index\nx\n"
    );
    assert_eq!(
        heading_section(b"## Index\n## Index", "## Index").unwrap_err(),
        "Markdown heading is missing or duplicated: ## Index"
    );
    assert_eq!(
        heading_section(&[0xff], "## Index").unwrap_err(),
        "managed Markdown is not UTF-8"
    );
    let rows = table_rows(b"| Name | Value |\n|---|---|\n| [`Label`](Target) | x |\n| [Label](Other) | y |\n| `MiXeD` | z |\n| | empty |\n").unwrap();
    assert_eq!(
        rows.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["", "[label](other)", "mixed", "target"]
    );
    assert_eq!(rows["target"], b"| [`Label`](Target) | x |\n");
    assert_eq!(
        table_rows(b"| K | V |\n|---|---|\n| A | x |\n| a | y |").unwrap_err(),
        "managed Markdown table key is duplicated: a"
    );
}

#[test]
fn markdown_characterization_projection_multitable_and_hash() {
    let payload = b"## Index\n| K | V |\n|---|---|\n| project | keep |\n<!-- example\n| K | V |\n|---|---|\n| managed | new |\n-->\n";
    let blocks = serde_json::json!({"headings":[],"keyed_tables":[{"heading":"## Index","managed_keys":["MANAGED"]}]});
    let expected = serde_json::json!({"headings":{},"keyed_tables":{"## Index":{"managed":sha(b"| managed | new |\n")}}});
    assert_eq!(markdown_projection(payload, &blocks).unwrap(), expected);
    let asset = serde_json::json!({"id":"fixture", "strategy":"merge", "managed_blocks":{
        "headings":[], "keyed_tables":blocks["keyed_tables"],
        "current_projection_sha256":canonical_sha(&expected).unwrap()
    }});
    verify_asset_payload(&asset, payload).unwrap();
    let drifted = String::from_utf8(payload.to_vec())
        .unwrap()
        .replace("managed | new", "managed | changed");
    assert_eq!(
        verify_asset_payload(&asset, drifted.as_bytes()).unwrap_err(),
        "managed Markdown projection drifted: fixture"
    );
}

fn factory_root() -> PathBuf {
    factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn contract_and_asset(id: &str) -> (Value, Value) {
    let contract = bridgeforge_core::product_manifest::project(&factory_root()).unwrap();
    let asset = contract["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["id"].as_str() == Some(id))
        .unwrap()
        .clone();
    (contract, asset)
}

#[test]
fn rejects_unsafe_target() {
    assert!(safe_target(Path::new("."), &Value::String("../x".into()), "x").is_err());
    assert!(safe_target(Path::new("."), &Value::String("a\\b".into()), "x").is_err());
}

#[test]
fn contract_schema_is_exact_and_asset_identities_are_unique() {
    let (mut contract, _) = contract_and_asset("codex.hooks-config");
    contract["release_model"] = Value::String("product-packages".into());
    assert!(validate_contract(&contract).is_ok());
    contract["release_model"] = Value::String("synchronized".into());
    assert!(
        validate_contract(&contract)
            .unwrap_err()
            .contains("release_model")
    );
    let (mut contract, _) = contract_and_asset("codex.hooks-config");
    contract["unexpected"] = Value::Bool(true);
    assert!(validate_contract(&contract).is_err());

    let (mut contract, _) = contract_and_asset("codex.hooks-config");
    let duplicate = contract["assets"][0].clone();
    contract["assets"].as_array_mut().unwrap().push(duplicate);
    assert!(validate_contract(&contract).is_err());

    assert!(
        parse_unique_json(br#"{"schema_version":4,"schema_version":4}"#, "contract")
            .unwrap_err()
            .contains("duplicate JSON key")
    );
}

#[test]
fn asset_history_and_inline_sources_reject_inconsistent_contracts() {
    let (contract, _) = contract_and_asset("root.artifact-ignore");
    validate_contract(&contract).unwrap();
    let index = contract["assets"]
        .as_array()
        .unwrap()
        .iter()
        .position(|a| a["id"] == "root.artifact-ignore")
        .unwrap();
    for content in [serde_json::json!(42), serde_json::json!("changed")] {
        let mut invalid = contract.clone();
        invalid["assets"][index]["content"] = content;
        assert!(validate_contract(&invalid).is_err());
    }
    let mut ambiguous = contract.clone();
    ambiguous["assets"][index]["source"] = serde_json::json!("templates/missing");
    assert!(
        validate_contract(&ambiguous)
            .unwrap_err()
            .contains("cannot also declare")
    );
    let mut invalid = contract.clone();
    invalid["asset_history"]["retirements"][0]["known_hashes"][0]["revision"] =
        serde_json::json!("unknown");
    assert!(
        validate_contract(&invalid)
            .unwrap_err()
            .contains("provenance")
    );
    let mut drifted = contract.clone();
    drifted["retired_assets"].as_array_mut().unwrap().pop();
    assert!(
        validate_contract(&drifted)
            .unwrap_err()
            .contains("differ from asset history")
    );
}

#[test]
fn agents_contract_and_payload_require_both_unique_zones() {
    let (_, mut asset) = contract_and_asset("root.agents");
    asset["agents_zones"]["project"]
        .as_object_mut()
        .unwrap()
        .remove("end");
    let (mut contract, _) = contract_and_asset("root.agents");
    let position = contract["assets"]
        .as_array()
        .unwrap()
        .iter()
        .position(|candidate| candidate["id"].as_str() == Some("root.agents"))
        .unwrap();
    contract["assets"][position] = asset;
    assert!(validate_contract(&contract).is_err());

    let (_, asset) = contract_and_asset("root.agents");
    let source = factory_root().join(asset["source"].as_str().unwrap());
    let payload = fs::read(source).unwrap();
    verify_asset_payload(&asset, &payload).unwrap();
    let drifted = String::from_utf8(payload)
        .unwrap()
        .replace("<!-- BRIDGEFORGE:PROJECT:END -->", "")
        .into_bytes();
    assert!(verify_asset_payload(&asset, &drifted).is_err());
}

#[test]
fn gitattributes_uses_effective_git_semantics() {
    let root = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"))
        .join(".runtime/tests/attributes");
    fs::create_dir_all(&root).unwrap();
    verify_gitattributes(b"* text=auto eol=lf\n", Some(&root)).unwrap();
    let error = verify_gitattributes(b"* text=auto eol=lf\n* eol=crlf\n", Some(&root)).unwrap_err();
    assert!(error.contains("overridden"), "{error}");
}

#[test]
fn public_attribute_rules_agree_with_git_and_preserve_hook_lf() {
    let root = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"))
        .join(".runtime/tests/attributes-git-differential");
    fs::create_dir_all(&root).unwrap();
    let payload = b"* text=auto eol=lf\n*.bat text eol=crlf\n*.cmd text eol=crlf\n*.ps1 text eol=crlf\n.githooks/** text eol=lf\n";
    let asset = serde_json::json!({"merge_validation":{"required_rules":[
        "* text=auto eol=lf", "*.bat text eol=crlf", "*.cmd text eol=crlf"
    ]}});
    crate::attributes::verify(payload, &asset).unwrap();
    fs::write(root.join(".gitattributes"), payload).unwrap();
    let git = |args: &[&str]| {
        let mut request = ProcessRequest::new("git", &root);
        request.args = args.iter().map(Into::into).collect();
        let result = SystemProcessRunner.run(&request).unwrap();
        assert_eq!(
            result.code,
            0,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    };
    git(&["init", "-q"]);
    let output = git(&[
        "-c",
        "core.attributesFile=NUL",
        "check-attr",
        "eol",
        "--",
        "probe.bat",
        "nested/probe.cmd",
        "probe.ps1",
        "probe.vbs",
        ".githooks/pre-commit",
    ]);
    for expected in [
        "probe.bat: eol: crlf",
        "nested/probe.cmd: eol: crlf",
        "probe.ps1: eol: crlf",
        "probe.vbs: eol: lf",
        ".githooks/pre-commit: eol: lf",
    ] {
        assert!(output.contains(expected), "{output}");
    }
}

#[test]
fn ordinary_attribute_check_keeps_git_macro_and_binary_support() {
    let root = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"))
        .join(".runtime/tests/attributes-full-git");
    fs::create_dir_all(&root).unwrap();
    let payload = b"[attr]windows text eol=crlf\n* text=auto eol=lf\n*.bat windows\n*.cmd windows\n*.png binary\n";
    let asset = serde_json::json!({"merge_validation":{"required_rules":[
        "* text=auto eol=lf", "*.bat text eol=crlf", "*.cmd text eol=crlf"
    ]}});
    assert!(crate::attributes::verify(payload, &asset).is_err());
    verify_gitattributes_asset(payload, Some(&root), &asset).unwrap();
    let (_, projected) = crate::attributes::projection(&asset, payload).unwrap();
    assert!(
        String::from_utf8(projected)
            .unwrap()
            .contains("*.png binary")
    );
}

#[test]
fn real_managed_markdown_projection_and_link_keys_are_verified() {
    let (_, asset) = contract_and_asset("codex.doc.readme");
    let source = factory_root().join(asset["source"].as_str().unwrap());
    let payload = fs::read(source).unwrap();
    verify_asset_payload(&asset, &payload).unwrap();
    let drifted = String::from_utf8(payload)
        .unwrap()
        .replace(
            "3_reference/codex-hook-signals.md",
            "3_reference/codex-hook-signals-drifted.md",
        )
        .into_bytes();
    assert!(verify_asset_payload(&asset, &drifted).is_err());
}

#[test]
fn real_hooks_require_exact_identity_event_matcher_and_handler_hash() {
    let (_, asset) = contract_and_asset("codex.hooks-config");
    let source = factory_root().join(asset["source"].as_str().unwrap());
    let payload = fs::read(source).unwrap();
    verify_asset_payload(&asset, &payload).unwrap();

    let mut document: Value = serde_json::from_slice(&payload).unwrap();
    document["hooks"]["Stop"][0]["hooks"][0]["command"] = Value::String("wrong".into());
    assert!(verify_asset_payload(&asset, &serde_json::to_vec(&document).unwrap()).is_err());

    let duplicate = document["hooks"]["Stop"][0]["hooks"][0].clone();
    document["hooks"]["SessionStart"][0]["hooks"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(verify_asset_payload(&asset, &serde_json::to_vec(&document).unwrap()).is_err());
}

#[test]
fn generic_merge_requires_the_full_declared_json_subset() {
    let asset = serde_json::json!({
        "id": "fixture.options",
        "strategy": "merge",
        "merge_validation": {
            "format": "json-subset-current-v1",
            "required": {"options": {"enabled": true}}
        }
    });
    let payload: &[u8] = br#"{"options":{"enabled":true},"custom":42}"#;
    verify_asset_payload(&asset, &payload).unwrap();
    let mut document: Value = serde_json::from_slice(&payload).unwrap();
    document["options"]
        .as_object_mut()
        .unwrap()
        .remove("enabled");
    assert!(verify_asset_payload(&asset, &serde_json::to_vec(&document).unwrap()).is_err());
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
