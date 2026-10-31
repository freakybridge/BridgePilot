//! Shared frontmatter syntax; each consumer owns its business interpretation.
use std::collections::BTreeMap;

pub fn parse(text: &str) -> Result<BTreeMap<String, String>, String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normalized.lines();
    if lines.next() != Some("---") {
        return Ok(BTreeMap::new());
    }
    let mut fields = BTreeMap::new();
    for line in lines {
        if line.trim() == "---" {
            return Ok(fields);
        }
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            if fields
                .insert(
                    key.clone(),
                    value.trim().trim_matches(['\'', '"']).to_string(),
                )
                .is_some()
            {
                return Err(format!("duplicate document metadata: {key}"));
            }
        }
    }
    Err("document frontmatter is not closed".into())
}

pub fn retired(fields: &BTreeMap<String, String>) -> bool {
    match fields
        .get("lifecycle")
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("completed" | "archived") => fields
            .get("validation_status")
            .is_some_and(|v| v.eq_ignore_ascii_case("verified")),
        Some("superseded") => fields
            .get("superseded_by")
            .is_some_and(|v| !v.trim().is_empty()),
        _ => false,
    }
}
