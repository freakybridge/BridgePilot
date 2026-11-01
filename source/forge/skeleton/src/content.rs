//! Content transformations for project synchronization; no filesystem I/O.

use crate::managed_markdown::{
    merge_key as table_key, merge_section as markdown_section,
    optional_merge_section as optional_markdown_section, table_header, table_ranges,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Inline payloads are text-only and mutually exclusive with file sources.
pub fn inline_source(asset: &Value) -> Result<Option<&[u8]>, String> {
    let Some(value) = asset.get("content") else {
        return Ok(None);
    };
    if asset.get("source").is_some() || asset.get("render").is_some() {
        return Err("inline asset content cannot also declare source or render".into());
    }
    value
        .as_str()
        .map(|text| Some(text.as_bytes()))
        .ok_or_else(|| "inline asset content must be a string".into())
}

fn marker_replace(
    current: &[u8],
    source: &[u8],
    begin: &str,
    end: &str,
) -> Result<Vec<u8>, String> {
    let find = |payload: &[u8]| -> Result<(usize, usize), String> {
        let text =
            String::from_utf8(payload.to_vec()).map_err(|_| "managed region is not UTF-8")?;
        let start = text
            .find(begin)
            .ok_or_else(|| format!("managed begin marker is missing: {begin}"))?;
        let tail = &text[start..];
        let stop = tail
            .find(end)
            .map(|index| start + index + end.len())
            .ok_or_else(|| format!("managed end marker is missing: {end}"))?;
        if text[start + begin.len()..].contains(begin) || text[stop..].contains(end) {
            return Err("managed markers must appear exactly once".into());
        }
        Ok((start, stop))
    };
    let (current_start, current_stop) = find(current)?;
    let (source_start, source_stop) = find(source)?;
    let mut result = Vec::new();
    result.extend_from_slice(&current[..current_start]);
    result.extend_from_slice(&source[source_start..source_stop]);
    result.extend_from_slice(&current[current_stop..]);
    Ok(result)
}

fn merge_json(required: &Value, actual: &mut Value) {
    match (required, actual) {
        (Value::Object(required), Value::Object(actual)) => {
            for (key, value) in required {
                match actual.get_mut(key) {
                    Some(current) => merge_json(value, current),
                    None => {
                        actual.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (Value::Array(required), Value::Array(actual)) => {
            for value in required {
                if !actual.contains(value) {
                    actual.push(value.clone());
                }
            }
        }
        (required, actual) => *actual = required.clone(),
    }
}

fn render_source(source: &[u8], asset: &Value, project_root: &Path) -> Result<Vec<u8>, String> {
    if asset["render"].as_str() != Some("project-name") {
        return Ok(source.to_vec());
    }
    let text = String::from_utf8(source.to_vec())
        .map_err(|_| format!("asset {} render source is not UTF-8", asset["id"]))?;
    let name = project_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("project root has no UTF-8 name")?;
    Ok(text.replace("{{PROJECT_NAME}}", name).into_bytes())
}

fn marker_span(payload: &[u8], begin: &str, end: &str) -> Result<(usize, usize), String> {
    let text = String::from_utf8(payload.to_vec()).map_err(|_| "managed content is not UTF-8")?;
    let starts = text
        .match_indices(begin)
        .map(|item| item.0)
        .collect::<Vec<_>>();
    let stops = text
        .match_indices(end)
        .map(|item| item.0)
        .collect::<Vec<_>>();
    if starts.len() != 1 || stops.len() != 1 || starts[0] >= stops[0] {
        return Err(format!(
            "managed markers are missing, duplicated, or reversed: {begin} / {end}"
        ));
    }
    Ok((starts[0], stops[0] + end.len()))
}

fn merge_agents(
    source: &[u8],
    current: Option<&[u8]>,
    asset: &Value,
    project_root: &Path,
) -> Result<Vec<u8>, String> {
    let canonical = render_source(source, asset, project_root)?;
    let Some(current) = current else {
        return Ok(canonical);
    };
    let project = &asset["agents_zones"]["project"];
    let begin = project["begin"]
        .as_str()
        .ok_or("project zone begin is missing")?;
    let end = project["end"]
        .as_str()
        .ok_or("project zone end is missing")?;
    let (canonical_start, canonical_stop) = marker_span(&canonical, begin, end)?;
    let (current_start, current_stop) = marker_span(current, begin, end)?;
    let mut merged = Vec::new();
    merged.extend_from_slice(&canonical[..canonical_start]);
    merged.extend_from_slice(&current[current_start..current_stop]);
    merged.extend_from_slice(&canonical[canonical_stop..]);
    Ok(merged)
}

fn ensure_markdown_section(
    merged: &mut String,
    source: &str,
    heading: &str,
    allow_upgrade: bool,
) -> Result<(usize, usize), String> {
    if let Some(span) = optional_markdown_section(merged, heading)? {
        return Ok(span);
    }
    if !allow_upgrade {
        return markdown_section(merged, heading);
    }
    let (start, end) = markdown_section(source, heading)?;
    let mut insertion = merged.len();
    for line in source[end..].lines().filter(|line| line.starts_with("## ")) {
        if let Some((offset, _)) = optional_markdown_section(merged, line)? {
            insertion = offset;
            break;
        }
    }
    let prefix = if insertion > 0 && !merged[..insertion].ends_with('\n') {
        "\n\n"
    } else {
        ""
    };
    merged.insert_str(insertion, &format!("{prefix}{}", &source[start..end]));
    markdown_section(merged, heading)
}

fn merge_keyed_table(
    source_section: &str,
    current_section: &str,
    keys: &BTreeSet<String>,
    allow_upgrade: bool,
) -> Result<String, String> {
    let target_ranges = table_ranges(current_section)?;
    if target_ranges.len() > 1 {
        let candidates = target_ranges
            .iter()
            .filter(|(start, end)| {
                current_section[*start..*end]
                    .split_inclusive('\n')
                    .filter_map(table_key)
                    .any(|key| keys.contains(&key))
            })
            .copied()
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err("managed Markdown table is missing or ambiguous".into());
        }
        let (start, end) = candidates[0];
        let table = merge_keyed_table(
            source_section,
            &current_section[start..end],
            keys,
            allow_upgrade,
        )?;
        return Ok(format!(
            "{}{}{}",
            &current_section[..start],
            table,
            &current_section[end..]
        ));
    }
    let (source_header, source_separator, columns) = table_header(source_section)?;
    let (current_header, current_separator, current_columns) = table_header(current_section)?;
    if columns != current_columns {
        return Err("managed Markdown table column count changed".into());
    }
    let upgraded;
    let current_section = if allow_upgrade {
        upgraded = current_section
            .replacen(current_header, source_header, 1)
            .replacen(current_separator, source_separator, 1);
        upgraded.as_str()
    } else {
        if current_header.trim() != source_header.trim() {
            return Err("managed Markdown table header drifted".into());
        }
        current_section
    };
    let mut required = BTreeMap::<String, String>::new();
    for line in source_section.split_inclusive('\n') {
        if let Some(key) = table_key(line)
            && keys.contains(&key)
        {
            if required.insert(key.clone(), line.to_string()).is_some() {
                return Err(format!("managed Markdown source row is duplicated: {key}"));
            }
        }
    }
    if required.len() != keys.len() {
        let missing = keys
            .difference(&required.keys().cloned().collect())
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!(
            "managed Markdown source rows are missing: {missing:?}"
        ));
    }
    let mut rendered = Vec::<String>::new();
    let mut seen = BTreeSet::new();
    let mut insertion = None;
    for line in current_section.split_inclusive('\n') {
        if insertion.is_none() && line.trim() == source_separator.trim() {
            rendered.push(line.to_string());
            insertion = Some(rendered.len());
            continue;
        }
        if let Some(key) = table_key(line)
            && keys.contains(&key)
        {
            if !seen.insert(key.clone()) {
                return Err(format!("managed Markdown target row is duplicated: {key}"));
            }
            rendered.push(required[&key].clone());
        } else {
            rendered.push(line.to_string());
        }
    }
    let insert_at = insertion.ok_or("managed Markdown table separator is missing")?;
    for key in keys {
        if !seen.contains(key) {
            rendered.insert(insert_at, required[key].clone());
        }
    }
    Ok(rendered.concat())
}

fn merge_managed_markdown(
    source: &[u8],
    current: &[u8],
    asset: &Value,
    project_root: &Path,
    allow_upgrade: bool,
) -> Result<Vec<u8>, String> {
    let source = String::from_utf8(render_source(source, asset, project_root)?)
        .map_err(|_| "managed Markdown source is not UTF-8")?;
    let mut merged =
        String::from_utf8(current.to_vec()).map_err(|_| "managed Markdown target is not UTF-8")?;
    for heading in asset["managed_blocks"]["headings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let (source_start, source_end) = markdown_section(&source, heading)?;
        let (target_start, target_end) =
            ensure_markdown_section(&mut merged, &source, heading, allow_upgrade)?;
        merged.replace_range(target_start..target_end, &source[source_start..source_end]);
    }
    for contract in asset["managed_blocks"]["keyed_tables"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let heading = contract["heading"]
            .as_str()
            .ok_or("keyed table heading is missing")?;
        let keys = contract["managed_keys"]
            .as_array()
            .ok_or("managed table keys are missing")?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        let (source_start, source_end) = markdown_section(&source, heading)?;
        let (target_start, target_end) =
            ensure_markdown_section(&mut merged, &source, heading, allow_upgrade)?;
        let section = merge_keyed_table(
            &source[source_start..source_end],
            &merged[target_start..target_end],
            &keys,
            allow_upgrade,
        )?;
        merged.replace_range(target_start..target_end, &section);
    }
    Ok(merged.into_bytes())
}

fn merge_project_hooks(source: &[u8], current: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let canonical: Value = serde_json::from_slice(source)
        .map_err(|error| format!("canonical project hooks are invalid: {error}"))?;
    let Some(current) = current else {
        let mut bytes = serde_json::to_vec_pretty(&canonical).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        return Ok(bytes);
    };
    let local: Value = serde_json::from_slice(current)
        .map_err(|error| format!("project hooks target is invalid: {error}"))?;
    let mut external = BTreeMap::<String, Vec<Value>>::new();
    let hooks = local["hooks"]
        .as_object()
        .ok_or("project hooks target has no hooks object")?;
    for (event, groups) in hooks {
        for group in groups
            .as_array()
            .ok_or("project hook event is not an array")?
        {
            let mut kept = Vec::new();
            for handler in group["hooks"]
                .as_array()
                .ok_or("project hook group has no hooks array")?
            {
                let managed_id = handler["bridgeforgeCodexId"].as_str();
                if managed_id.is_some_and(|id| id.starts_with("bridgeforge-codex.project-hook.v1:"))
                {
                    continue;
                }
                if managed_id.is_some() {
                    return Err(format!(
                        "project hook has a non-canonical managed identity: {event}"
                    ));
                }
                kept.push(handler.clone());
            }
            if !kept.is_empty() {
                let mut group = group.clone();
                group["hooks"] = Value::Array(kept);
                external.entry(event.clone()).or_default().push(group);
            }
        }
    }
    let mut result = local;
    result["hooks"] = canonical["hooks"].clone();
    let result_hooks = result["hooks"]
        .as_object_mut()
        .ok_or("canonical hooks object is invalid")?;
    for (event, groups) in external {
        result_hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or("canonical hook event is not an array")?
            .extend(groups);
    }
    if let Some(metadata) = canonical.get("bridgeforgeCodex") {
        result["bridgeforgeCodex"] = metadata.clone();
    }
    let mut bytes = serde_json::to_vec_pretty(&result).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn render(
    id: &str,
    source_payload: &[u8],
    effective_current: Option<&[u8]>,
    asset: &Value,
    project_root: &Path,
    allow_structure_upgrade: bool,
) -> Result<Vec<u8>, String> {
    let strategy = asset["strategy"].as_str().unwrap_or("whole");
    let rendered = if asset.get("agents_zones").is_some() {
        merge_agents(source_payload, effective_current, asset, project_root)?
    } else if asset.get("managed_blocks").is_some() && effective_current.is_some() {
        merge_managed_markdown(
            source_payload,
            effective_current.unwrap(),
            asset,
            project_root,
            allow_structure_upgrade,
        )?
    } else if asset["merge_policy"].as_str() == Some("codex-hooks") {
        merge_project_hooks(source_payload, effective_current)?
    } else {
        match (strategy, effective_current) {
            ("seed", Some(payload)) => payload.to_vec(),
            ("seed", None) => render_source(source_payload, asset, project_root)?,
            ("whole", _) => render_source(source_payload, asset, project_root)?,
            ("region", Some(payload)) => {
                let region = &asset["region"];
                marker_replace(
                    payload,
                    source_payload,
                    region["begin"].as_str().ok_or("region begin is missing")?,
                    region["end"].as_str().ok_or("region end is missing")?,
                )?
            }
            ("region", None) => source_payload.to_vec(),
            ("merge", Some(payload)) if asset["merge_policy"] == "artifact-roots" => {
                if bridgeforge_core::artifact_paths::ignore_problems(Some(payload))?.is_empty() {
                    payload.to_vec()
                } else {
                    let mut text = String::from_utf8(payload.to_vec())
                        .map_err(|_| ".gitignore is not UTF-8")?;
                    if !text.is_empty() && !text.ends_with('\n') {
                        text.push('\n');
                    }
                    text.push_str(
                        std::str::from_utf8(source_payload)
                            .map_err(|_| "artifact ignore source is not UTF-8")?,
                    );
                    text.into_bytes()
                }
            }
            ("merge", Some(payload)) if asset["merge_policy"] == "git-attributes-default-lf" => {
                crate::attributes::merge(source_payload, payload, asset)?
            }
            ("merge", Some(payload)) => {
                let required: Value = serde_json::from_slice(source_payload)
                    .map_err(|error| format!("managed merge source is invalid: {id}: {error}"))?;
                let mut actual: Value = serde_json::from_slice(payload)
                    .map_err(|error| format!("managed merge target is invalid: {id}: {error}"))?;
                merge_json(&required, &mut actual);
                let mut bytes =
                    serde_json::to_vec_pretty(&actual).map_err(|error| error.to_string())?;
                bytes.push(b'\n');
                bytes
            }
            ("merge", None) => source_payload.to_vec(),
            (_, _) => return Err(format!("unsupported asset strategy: {id}: {strategy}")),
        }
    };
    Ok(rendered)
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_project_sync_content.rs"]
mod tests;
