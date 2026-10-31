use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AllowFinding {
    pub index: usize,
    pub entry: String,
    pub reasons: Vec<String>,
}

fn ipv4_in_network_context(value: &str) -> bool {
    for token in value.split(|character: char| character.is_whitespace() || character == '@') {
        let candidate = token
            .trim_matches(|character: char| !character.is_ascii_digit() && character != '.')
            .split([':', '/'])
            .next()
            .unwrap_or("");
        let octets = candidate.split('.').collect::<Vec<_>>();
        if octets.len() == 4
            && octets.iter().all(|part| {
                !part.is_empty()
                    && part.len() <= 3
                    && part.parse::<u8>().is_ok()
                    && (part == &"0" || !part.starts_with('0'))
            })
            && (value.trim_start().starts_with(candidate)
                || value.contains(&format!("@{candidate}"))
                || value.contains(&format!(" {candidate}")))
        {
            return true;
        }
    }
    false
}

pub fn reasons(entry: &str) -> Vec<String> {
    let lower = entry.to_lowercase();
    let mut result = Vec::new();
    if !entry.contains("://")
        && entry.as_bytes().windows(3).any(|window| {
            window[0].is_ascii_alphabetic() && window[1] == b':' && window[2] == b'\\'
        })
    {
        result.push("绝对路径(Windows)".into());
    }
    if lower.contains("/users/") || lower.contains("/home/") {
        result.push("绝对路径(Unix用户目录)".into());
    }
    if lower.contains("get-process -id ") || lower.contains("kill -9 ") {
        result.push("PID".into());
    }
    if ipv4_in_network_context(entry) {
        result.push("IP(IPv4)".into());
    }
    if lower.contains("git clone ") || lower.ends_with("git clone") {
        result.push("一次性命令(git clone)".into());
    }
    if lower.contains("cargo build --manifest-path") {
        result.push("一次性命令(cargo build --manifest-path)".into());
    }
    if ["cl.exe", "msbuild.exe", "link.exe"]
        .iter()
        .any(|command| lower.split_whitespace().any(|part| part.ends_with(command)))
    {
        result.push("一次性命令(cl.exe/msbuild)".into());
    }
    result
}

pub fn audit(path: &Path) -> Result<Vec<AllowFinding>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let document: Value = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("cannot read settings.json: {error}"))?,
    )
    .map_err(|error| format!("invalid settings.json: {error}"))?;
    let allow = document
        .pointer("/permissions/allow")
        .and_then(Value::as_array)
        .ok_or("permissions.allow is not an array")?;
    Ok(allow
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let entry = value.as_str()?;
            let reasons = reasons(entry);
            (!reasons.is_empty()).then(|| AllowFinding {
                index,
                entry: entry.into(),
                reasons,
            })
        })
        .collect())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_audit_user_allow.rs"]
mod tests;
