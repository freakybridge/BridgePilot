#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

#[path = "../support/factory_root.rs"]
mod factory_repository;

use super::*;
use bridgeforge_core::build_manifest::normalized_hash as sha_git;
use serde_json::json;
use std::fs;

#[test]
fn hook_merge_preserves_project_handlers_and_replaces_only_managed_identities() {
    let managed = json!({"type":"command","bridgeforgeCodexId":"bridgeforge-codex.project-hook.v1:stop","command":"new-public"});
    let canonical = json!({"hooks":{"Stop":[{"hooks":[managed.clone()]}]}});
    let private = json!({"matcher":"custom","timeout":77,"extra":true,"hooks":[
        {"type":"command","command":"python .codex/hooks/encoding_check.py","bridgeforgeProjectHookId":"demo:0","custom":42}
    ]});
    let local = json!({"description":"project","customRoot":true,"bridgeforgeProjectHooks":{"opaque":true},"hooks":{"Stop":[
        {"hooks":[{"type":"command","bridgeforgeCodexId":"bridgeforge-codex.project-hook.v1:stop","command":"old-public"}]}, private.clone()
    ]}});
    let source = serde_json::to_vec(&canonical).unwrap();
    let merged = merge_project_hooks(&source, Some(&serde_json::to_vec(&local).unwrap())).unwrap();
    let value: Value = serde_json::from_slice(&merged).unwrap();
    assert_eq!(value["hooks"]["Stop"], json!([{"hooks":[managed]},private]));
    for key in ["description", "customRoot", "bridgeforgeProjectHooks"] {
        assert_eq!(value[key], local[key]);
    }
    assert_eq!(merge_project_hooks(&source, Some(&merged)).unwrap(), merged);
    let mut ambiguous = local;
    ambiguous["hooks"]["Stop"][0]["hooks"][0]["bridgeforgeCodexId"] = json!("unknown-owner");
    assert!(merge_project_hooks(&source, Some(&serde_json::to_vec(&ambiguous).unwrap())).is_err());
}

#[test]
fn markdown_upgrade_inserts_sections_and_preserves_project_content() {
    let source = b"# Docs\n\n## Start\n| Task | Read |\n|---|---|\n| arch | new |\n\n## Lifecycle\ncanonical\n\n## Index\n| File | Purpose |\n|---|---|\n| managed | new |\n\n";
    let current = b"# Custom\r\ndelivery_layout: milestone\r\n\r\n## Project\r\nkeep bytes\r\n\r\n## Index\r\n| Old | Header |\r\n|---|---|\r\n| custom | personal |\r\n| managed | old |\r\n\r\n";
    let asset = json!({"managed_blocks":{"headings":["## Lifecycle"],"keyed_tables":[
        {"heading":"## Start","managed_keys":["arch"]},{"heading":"## Index","managed_keys":["managed"]}]}});
    let merged =
        merge_managed_markdown(source, current, &asset, Path::new("Example"), true).unwrap();
    let text = String::from_utf8(merged.clone()).unwrap();
    assert!(text.contains("## Project\r\nkeep bytes\r\n\r\n"));
    assert!(text.contains("| custom | personal |\r\n"));
    assert!(text.contains("delivery_layout: milestone\r\n"));
    assert!(text.find("## Start").unwrap() < text.find("## Lifecycle").unwrap());
    assert!(text.find("## Lifecycle").unwrap() < text.find("## Index").unwrap());
    assert_eq!(
        merge_managed_markdown(source, &merged, &asset, Path::new("Example"), false).unwrap(),
        merged
    );
    assert!(merge_managed_markdown(source, current, &asset, Path::new("Example"), false).is_err());
    let duplicate = format!("{text}\n## Lifecycle\nambiguous");
    assert!(
        merge_managed_markdown(
            source,
            duplicate.as_bytes(),
            &asset,
            Path::new("Example"),
            true
        )
        .is_err()
    );
    let changed_columns = text.replace(
        "| File | Purpose |\n|---|---|",
        "| File | Purpose | Extra |\n|---|---|---|",
    );
    assert!(
        merge_managed_markdown(
            source,
            changed_columns.as_bytes(),
            &asset,
            Path::new("Example"),
            true
        )
        .is_err()
    );
}

#[test]
fn markdown_upgrade_selects_the_only_table_with_managed_keys() {
    let source = b"# Docs\n\n## Index\n<!-- example\n| File | Purpose |\n|------|---------|\n| managed | new |\n-->\n";
    let current = b"# Docs\r\n\r\n## Index\r\n| File | Purpose |\r\n|---|---|\r\n| project | keep |\r\n\r\n<!-- example\r\n| Old | Header |\r\n|------|---------|\r\n| managed | old |\r\n-->\r\n";
    let asset = json!({"managed_blocks":{"headings":[],"keyed_tables":[
        {"heading":"## Index","managed_keys":["managed"]}]}});
    let merged =
        merge_managed_markdown(source, current, &asset, Path::new("Example"), true).unwrap();
    let text = String::from_utf8(merged.clone()).unwrap();
    assert!(text.contains("| File | Purpose |\r\n|---|---|\r\n| project | keep |\r\n"));
    assert!(text.contains(
        "<!-- example\r\n| File | Purpose |\n|------|---------|\n| managed | new |\n-->"
    ));
    assert_eq!(
        merge_managed_markdown(source, &merged, &asset, Path::new("Example"), false).unwrap(),
        merged
    );

    let ambiguous = text.replace("| project | keep |", "| managed | project copy |");
    assert!(
        merge_managed_markdown(
            source,
            ambiguous.as_bytes(),
            &asset,
            Path::new("Example"),
            true
        )
        .is_err()
    );
}

#[test]
fn real_template_upgrades_legacy_markdown_without_old_contract_or_losing_index() {
    let factory = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"));
    let contract: Value =
        serde_json::from_slice(&fixture_product::read(factory.join("source/forge/bridgeforge-manifest.json")).unwrap())
            .unwrap();
    let asset = contract["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "codex.doc.readme")
        .unwrap();
    let source = fs::read(bridgeforge_core::source_projection::template_path(&factory, "templates/doc/README.md")).unwrap();
    let mut old = String::from_utf8(source.clone()).unwrap();
    for heading in ["## 文档生命周期"] {
        let (start, end) = markdown_section(&old, heading).unwrap();
        old.replace_range(start..end, "");
    }
    old = old.replace("| 文件 | 说明 |", "| 旧标题 | 项目说明 |");
    old.push_str("\n## 项目自有\n不得丢失\n");
    old.push_str("\n## 项目知识库\n[my topic](5_project_knowledgebase/topic.md)\n");
    old = old.replace(
        "| 项目知识 | [`5_project_knowledgebase/`](5_project_knowledgebase/)；项目自有话题与长期资料 |",
        "| 项目知识 | my existing knowledge navigation |",
    );
    let merged =
        merge_managed_markdown(&source, old.as_bytes(), asset, Path::new("Example"), true).unwrap();
    assert!(String::from_utf8_lossy(&merged).contains("## 项目自有\n不得丢失\n"));
    assert!(
        String::from_utf8_lossy(&merged)
            .contains("## 项目知识库\n[my topic](5_project_knowledgebase/topic.md)\n")
    );
    assert!(String::from_utf8_lossy(&merged).contains("my existing knowledge navigation"));
    crate::baseline::verify_asset_payload(asset, &merged).unwrap();
    assert_eq!(
        merge_managed_markdown(&source, &merged, asset, Path::new("Example"), false).unwrap(),
        merged
    );
}

#[test]
fn markdown_characterization_merge_boundaries_and_keys() {
    let unicode = "前言\n## 索引\n正文\n## 尾部";
    let (start, end) = markdown_section(unicode, "## 索引").unwrap();
    assert_eq!(&unicode[start..end], "## 索引\n正文\n");
    assert_eq!(
        table_key("| [`标题`](文档/Ä.md) | 保留 |").as_deref(),
        Some("文档/Ä.md")
    );
    let fenced = "```md\n## Index\ninside\n```\n";
    let (start, end) = markdown_section(fenced, "## Index").unwrap();
    assert_eq!(&fenced[start..end], "## Index\ninside\n```\n");
    let text = "## Index\r\nbody\r\n### Child\r\nnested\r\n# Root\r\ntail\r\n## End\r\n";
    let (start, end) = markdown_section(text, "## Index").unwrap();
    assert_eq!(
        &text[start..end],
        "## Index\r\nbody\r\n### Child\r\nnested\r\n# Root\r\ntail\r\n"
    );
    assert_eq!(
        markdown_section("## Index\rlast", "## Index").unwrap_err(),
        "managed Markdown heading is missing or duplicated: ## Index"
    );
    assert_eq!(
        markdown_section("## Index\nlast", "## Index").unwrap(),
        (0, 13)
    );
    assert_eq!(
        optional_markdown_section("## Other", "## Index").unwrap(),
        None
    );
    assert_eq!(
        markdown_section("## Index\n## Index", "## Index").unwrap_err(),
        "managed Markdown heading is missing or duplicated: ## Index"
    );
    for (line, key) in [
        ("| [`Label`](Target) | x |", Some("Target")),
        ("| [Label](Other) | x |", Some("Other")),
        ("| `MiXeD` | x |", Some("MiXeD")),
        ("| | x |", None),
        ("|---|---|", None),
    ] {
        assert_eq!(table_key(line).as_deref(), key);
    }
}

#[test]
fn markdown_characterization_merge_bytes_and_failures() {
    let source = "| K | V |\n|---|---|\n| managed | new |\n";
    let current = "| K | V |\r\n|---|---|\r\n| project | keep |\r\n| managed | old |";
    let keys = BTreeSet::from(["managed".to_string()]);
    let merged = merge_keyed_table(source, current, &keys, true).unwrap();
    assert_eq!(
        merged,
        "| K | V |\n|---|---|\n| project | keep |\r\n| managed | new |\n"
    );
    assert_eq!(
        merge_keyed_table(source, &merged, &keys, false).unwrap(),
        merged
    );
    assert_eq!(
        merge_keyed_table(
            source,
            "| K | V |\n|---|---|\n| managed | a |\n| managed | b |\n",
            &keys,
            true
        )
        .unwrap_err(),
        "managed Markdown target row is duplicated: managed"
    );
    assert_eq!(
        merge_keyed_table(source, "| K | V | Extra |\n|---|---|---|\n", &keys, true).unwrap_err(),
        "managed Markdown table column count changed"
    );
}

#[test]
fn markdown_characterization_merge_projection_roundtrip() {
    let source = b"## Index\n<!-- example\n| K | V |\n|---|---|\n| managed | new |\n-->\n";
    let current = b"## Index\n| K | V |\n|---|---|\n| project | keep |\n<!-- example\n| K | V |\n|---|---|\n| managed | old |\n-->\n";
    let mut asset = json!({"id":"fixture", "strategy":"merge", "managed_blocks":{
        "headings":[], "keyed_tables":[{"heading":"## Index","managed_keys":["managed"]}]
    }});
    let expected = json!({"headings":{},"keyed_tables":{"## Index":{"managed":sha_git(b"| managed | new |\n")}}});
    asset["managed_blocks"]["current_projection_sha256"] =
        json!(crate::manifest::canonical_sha(&expected).unwrap());
    let merged =
        merge_managed_markdown(source, current, &asset, Path::new("Example"), true).unwrap();
    assert_eq!(merged, b"## Index\n| K | V |\n|---|---|\n| project | keep |\n<!-- example\n| K | V |\n|---|---|\n| managed | new |\n-->\n");
    assert_eq!(
        crate::baseline::markdown_projection(&merged, &asset["managed_blocks"]).unwrap(),
        expected
    );
    crate::baseline::verify_asset_payload(&asset, &merged).unwrap();
}

#[test]
fn json_merge_preserves_external_keys_and_adds_required_values() {
    let required = json!({"permissions": {"allow": ["Read"], "mode": "safe"}});
    let mut actual = json!({"permissions": {"allow": ["Custom"], "extra": true}});
    merge_json(&required, &mut actual);
    assert_eq!(actual["permissions"]["extra"], true);
    assert_eq!(actual["permissions"]["mode"], "safe");
    assert_eq!(actual["permissions"]["allow"], json!(["Custom", "Read"]));
}

#[test]
fn marker_replace_preserves_project_tail() {
    let current = b"x\nBEGIN\nold\nEND\ntail\n";
    let source = b"BEGIN\nnew\nEND\n";
    assert_eq!(
        marker_replace(current, source, "BEGIN", "END").unwrap(),
        b"x\nBEGIN\nnew\nEND\ntail\n"
    );
}
