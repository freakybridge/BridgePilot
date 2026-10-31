//! Parsing shared by managed Markdown merging and projection.
//!
//! These policies intentionally preserve the two existing contracts. This is
//! not a general Markdown parser: comments/fences remain ordinary input lines.

use regex::Regex;
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Clone, Copy)]
enum Policy {
    Merge,
    Projection,
}

fn section_span(text: &str, heading: &str, policy: Policy) -> Result<Option<(usize, usize)>, ()> {
    let mut start = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == heading {
            if start.replace(offset).is_some() {
                return Err(());
            }
        }
        offset += line.len();
    }
    let level = heading
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if matches!(policy, Policy::Projection) && level == 0 {
        return Err(());
    }
    let Some(start) = start else { return Ok(None) };
    let mut cursor = start;
    for line in text[start..].split_inclusive('\n') {
        let boundary = match policy {
            Policy::Merge => line.starts_with("## "),
            Policy::Projection => {
                let trimmed = line.trim_start();
                let candidate = trimmed
                    .chars()
                    .take_while(|character| *character == '#')
                    .count();
                candidate > 0 && candidate <= level && trimmed.chars().nth(candidate) == Some(' ')
            }
        };
        if cursor > start && boundary {
            return Ok(Some((start, cursor)));
        }
        cursor += line.len();
    }
    Ok(Some((start, text.len())))
}

pub fn optional_merge_section(text: &str, heading: &str) -> Result<Option<(usize, usize)>, String> {
    section_span(text, heading, Policy::Merge)
        .map_err(|()| format!("managed Markdown heading is missing or duplicated: {heading}"))
}

pub fn merge_section(text: &str, heading: &str) -> Result<(usize, usize), String> {
    optional_merge_section(text, heading)?
        .ok_or_else(|| format!("managed Markdown heading is missing or duplicated: {heading}"))
}

pub fn projection_section(payload: &[u8], heading: &str) -> Result<Vec<u8>, String> {
    let text = String::from_utf8(payload.to_vec())
        .map_err(|_| "managed Markdown is not UTF-8".to_string())?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let (start, end) = section_span(&text, heading, Policy::Projection)
        .ok()
        .flatten()
        .ok_or_else(|| format!("Markdown heading is missing or duplicated: {heading}"))?;
    Ok(text.as_bytes()[start..end].to_vec())
}

fn row_key(line: &str, policy: Policy) -> Option<String> {
    if !line.trim_start().starts_with('|') {
        return None;
    }
    match policy {
        Policy::Merge => {
            let raw = line.split('|').nth(1)?.trim();
            let key = if let Some((_, href)) = raw.split_once("](") {
                href.split(')').next().unwrap_or(href).to_string()
            } else if let Some(rest) = raw.strip_prefix("[`") {
                rest.split("`]").next().unwrap_or(rest).to_string()
            } else {
                raw.trim_matches('`').to_string()
            };
            (!key.is_empty() && !key.chars().all(|value| value == '-' || value == ':'))
                .then_some(key)
        }
        Policy::Projection => {
            // Keep the narrower historical projection syntax and case folding.
            static LINK: OnceLock<Regex> = OnceLock::new();
            let link = LINK.get_or_init(|| {
                Regex::new(r"^\[`[^`]+`\]\(([^)]+)\)$").expect("constant link regex")
            });
            let raw = line
                .trim()
                .trim_matches('|')
                .split('|')
                .next()
                .unwrap_or("")
                .trim();
            let key = link
                .captures(raw)
                .map(|captures| captures[1].to_string())
                .unwrap_or_else(|| raw.to_string());
            Some(key.trim_matches('`').to_lowercase())
        }
    }
}

pub fn merge_key(line: &str) -> Option<String> {
    row_key(line, Policy::Merge)
}

pub fn projection_rows(section: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let text = String::from_utf8_lossy(section);
    let lines = text
        .lines()
        .filter(|line| line.trim_start().starts_with('|'))
        .collect::<Vec<_>>();
    if lines.len() < 2 {
        return Err("managed Markdown table is missing".into());
    }
    let mut result = BTreeMap::new();
    // Projection historically collects all pipe rows in the section, including
    // later tables. Table selection remains a merge-only policy.
    for line in lines.into_iter().skip(2) {
        let key = row_key(line, Policy::Projection).expect("filtered table row");
        if result
            .insert(key.clone(), format!("{line}\n").into_bytes())
            .is_some()
        {
            return Err(format!("managed Markdown table key is duplicated: {key}"));
        }
    }
    Ok(result)
}

fn is_separator(line: &str) -> bool {
    line.trim_start().starts_with('|')
        && line.trim().trim_matches('|').split('|').all(|cell| {
            let cell = cell.trim().trim_matches(':');
            cell.len() >= 3 && cell.bytes().all(|byte| byte == b'-')
        })
}

pub fn table_header(section: &str) -> Result<(&str, &str, usize), String> {
    let lines = section.split_inclusive('\n').collect::<Vec<_>>();
    let separators = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| is_separator(line))
        .collect::<Vec<_>>();
    if separators.len() != 1 || separators[0].0 == 0 {
        return Err("managed Markdown table is missing or ambiguous".into());
    }
    let (index, separator) = separators[0];
    let header = lines[index - 1];
    let columns = separator.trim().trim_matches('|').split('|').count();
    if !header.trim_start().starts_with('|')
        || header.trim().trim_matches('|').split('|').count() != columns
    {
        return Err("managed Markdown table header column count changed".into());
    }
    Ok((header, separator, columns))
}

pub fn table_ranges(section: &str) -> Result<Vec<(usize, usize)>, String> {
    let lines = section.split_inclusive('\n').collect::<Vec<_>>();
    let mut offsets = Vec::with_capacity(lines.len() + 1);
    offsets.push(0);
    for line in &lines {
        offsets.push(offsets.last().unwrap() + line.len());
    }
    let mut ranges = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !is_separator(line) {
            continue;
        }
        if index == 0 {
            return Err("managed Markdown table is missing or ambiguous".into());
        }
        let mut end = index + 1;
        while end < lines.len() && lines[end].trim_start().starts_with('|') {
            end += 1;
        }
        ranges.push((offsets[index - 1], offsets[end]));
    }
    Ok(ranges)
}
