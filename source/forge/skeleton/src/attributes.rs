//! Git attributes ownership and a conservative, filesystem-free policy check.

use regex::Regex;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub pattern: String,
    pub attributes: BTreeMap<String, String>,
}

fn parse_line(line: &str) -> Result<Option<Rule>, String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() < 2 || fields[0].contains(['\\', '"', '[', ']', '!']) {
        return Err("zero-write attribute inspection cannot prove complex syntax".into());
    }
    let mut attributes = BTreeMap::new();
    for field in &fields[1..] {
        let (key, value) = match *field {
            "text" => ("text", "set"),
            "-text" => ("text", "unset"),
            "!text" => ("text", "unspecified"),
            "text=auto" => ("text", "auto"),
            "eol=lf" => ("eol", "lf"),
            "eol=crlf" => ("eol", "crlf"),
            "-eol" => ("eol", "unset"),
            "!eol" => ("eol", "unspecified"),
            _ => return Err("zero-write attribute inspection cannot prove complex syntax".into()),
        };
        attributes.insert(key.into(), value.into());
    }
    Ok(Some(Rule {
        pattern: fields[0].into(),
        attributes,
    }))
}

fn parse(payload: &[u8]) -> Result<Vec<Rule>, String> {
    std::str::from_utf8(payload)
        .map_err(|_| ".gitattributes is not valid UTF-8".to_string())?
        .lines()
        .filter_map(|line| parse_line(line).transpose())
        .collect()
}

pub fn required(asset: &Value) -> Result<Vec<Rule>, String> {
    if let Some(lines) = asset["merge_validation"].get("required_rules") {
        let lines = lines
            .as_array()
            .ok_or("required attribute rules must be an array")?;
        if lines.is_empty() {
            return Err("required attribute rules are empty".into());
        }
        let rules: Vec<Rule> = lines
            .iter()
            .map(|line| {
                parse_line(
                    line.as_str()
                        .ok_or("required attribute rule must be text")?,
                )?
                .ok_or("required attribute rule is empty".into())
            })
            .collect::<Result<_, String>>()?;
        let default = parse(b"* text=auto eol=lf\n")?.remove(0);
        if !rules.contains(&default)
            || rules.iter().enumerate().any(|(index, rule)| {
                rules[..index]
                    .iter()
                    .any(|previous| previous.pattern == rule.pattern)
            })
        {
            return Err(
                "required attributes must contain the default LF policy and unique patterns".into(),
            );
        }
        Ok(rules)
    } else {
        parse(b"* text=auto eol=lf\n")
    }
}

fn matches(pattern: &str, path: &str) -> Result<bool, String> {
    let anchored = pattern.contains('/');
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut expression = if anchored {
        "^".to_string()
    } else {
        "(?:^|/)".into()
    };
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                let boundary = i == 0 || chars[i - 1] == '/';
                if !boundary || (i + 2 < chars.len() && chars[i + 2] != '/') {
                    return Err("zero-write attribute inspection cannot prove complex glob".into());
                }
                if chars.get(i + 2) == Some(&'/') {
                    expression.push_str("(?:.*/)?");
                    i += 3;
                } else {
                    expression.push_str(".*");
                    i += 2;
                }
                continue;
            }
            '*' => expression.push_str("[^/]*"),
            '?' => expression.push_str("[^/]"),
            c => expression.push_str(&regex::escape(&c.to_string())),
        }
        i += 1;
    }
    expression.push('$');
    Regex::new(&expression)
        .map_err(|error| error.to_string())
        .map(|regex| regex.is_match(path))
}

pub fn probes(asset: &Value) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    let mut expected = BTreeMap::new();
    for rule in required(asset)? {
        let probes = if rule.pattern == "*" {
            vec![
                "BRIDGEFORGE_DEFAULT_EOL_PROBE".into(),
                "nested/BRIDGEFORGE_DEFAULT_EOL_PROBE".into(),
                ".codex/BRIDGEFORGE_DEFAULT_EOL_PROBE.py".into(),
                "doc/BRIDGEFORGE_DEFAULT_EOL_PROBE.md".into(),
            ]
        } else if let Some(extension) = rule.pattern.strip_prefix("*.") {
            vec![
                format!("BRIDGEFORGE_EOL_PROBE.{extension}"),
                format!("nested/BRIDGEFORGE_EOL_PROBE.{extension}"),
            ]
        } else {
            return Err("required attribute pattern is not a supported public policy".into());
        };
        for path in probes {
            expected.insert(path, rule.attributes.clone());
        }
    }
    Ok(expected)
}

pub fn verify(payload: &[u8], asset: &Value) -> Result<(), String> {
    let actual = parse(payload)?;
    for rule in required(asset)? {
        if !actual.contains(&rule) {
            return Err(format!(
                "required .gitattributes rule is missing: {}",
                rule.pattern
            ));
        }
    }
    for (path, expected) in probes(asset)? {
        let mut effective = BTreeMap::new();
        for candidate in &actual {
            if matches(&candidate.pattern, &path)? {
                effective.extend(candidate.attributes.clone());
            }
        }
        if expected
            .iter()
            .any(|(key, value)| effective.get(key) != Some(value))
        {
            return Err(format!(".gitattributes policy is overridden for {path}"));
        }
    }
    Ok(())
}

pub fn merge(source: &[u8], current: &[u8], asset: &Value) -> Result<Vec<u8>, String> {
    let owned = required(asset)?;
    if parse(source)? != owned {
        return Err("attribute template and ownership contract differ".into());
    }
    let current_text = std::str::from_utf8(current).map_err(|_| ".gitattributes is not UTF-8")?;
    let actual = parse(current)?;
    for rule in &actual {
        if owned
            .iter()
            .any(|required| required.pattern == rule.pattern && required != rule)
        {
            return Err(format!(
                "project attribute rule conflicts with public policy: {}",
                rule.pattern
            ));
        }
    }
    let missing = owned
        .iter()
        .filter(|rule| !actual.contains(rule))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        verify(current, asset)?;
        return Ok(current.to_vec());
    }
    // Keep the public default before its exceptions, followed by untouched project rules.
    let mut result = std::str::from_utf8(source)
        .map_err(|_| "attribute source is not UTF-8")?
        .to_owned();
    if !result.ends_with('\n') {
        result.push('\n');
    }
    for line in current_text.split_inclusive('\n') {
        if !parse_line(line)?.is_some_and(|rule| owned.contains(&rule)) {
            result.push_str(line);
        }
    }
    verify(result.as_bytes(), asset)?;
    Ok(result.into_bytes())
}

pub fn projection(asset: &Value, payload: &[u8]) -> Result<(Vec<bool>, Vec<u8>), String> {
    let owned = required(asset)?;
    let mut found = vec![false; owned.len()];
    let mut project = Vec::new();
    let text = std::str::from_utf8(payload).map_err(|_| ".gitattributes is not UTF-8")?;
    for line in text.lines() {
        let parsed = parse_line(line).ok().flatten();
        if let Some(index) = owned.iter().position(|rule| Some(rule) == parsed.as_ref()) {
            found[index] = true;
        } else {
            project.extend_from_slice(line.as_bytes());
            project.push(b'\n');
        }
    }
    Ok((found, project))
}

#[cfg(all(test, bridgeforge_factory_tests))]
mod tests {
    use super::*;
    use serde_json::json;

    const PUBLIC: &[u8] = b"* text=auto eol=lf\n*.bat text eol=crlf\n*.cmd text eol=crlf\n";

    fn asset() -> Value {
        json!({"merge_validation":{"required_rules":[
            "* text=auto eol=lf", "*.bat text eol=crlf", "*.cmd text eol=crlf"
        ]}})
    }

    #[test]
    fn attributes_upgrade_preserves_project_bytes_and_is_idempotent() {
        let current = b"# project\r\n* text=auto eol=lf\r\n*.ps1 text eol=crlf\r\n.githooks/** text eol=lf\r\n";
        let merged = merge(PUBLIC, current, &asset()).unwrap();
        assert_eq!(
            merged,
            [
                PUBLIC,
                b"# project\r\n*.ps1 text eol=crlf\r\n.githooks/** text eol=lf\r\n"
            ]
            .concat()
        );
        assert_eq!(merge(PUBLIC, &merged, &asset()).unwrap(), merged);
        assert_eq!(merge(PUBLIC, b"", &asset()).unwrap(), PUBLIC);
    }

    #[test]
    fn attributes_reject_missing_policies_and_explicit_project_conflicts() {
        assert!(verify(b"* text=auto eol=lf\n", &asset()).is_err());
        for conflict in [
            b"* text=auto eol=lf\n*.bat text eol=lf\n".as_slice(),
            b"* text=auto eol=lf\n* eol=crlf\n",
        ] {
            assert!(merge(PUBLIC, conflict, &asset()).is_err());
        }
        let reordered = b"*.bat text eol=crlf\n*.cmd text eol=crlf\n* text=auto eol=lf\n";
        assert!(verify(reordered, &asset()).is_err());
    }

    #[test]
    fn attributes_match_git_path_glob_boundaries() {
        assert!(matches("*.cmd", "nested/probe.cmd").unwrap());
        assert!(matches(".githooks/*", ".githooks/pre-commit").unwrap());
        assert!(!matches(".githooks/*", ".githooks/nested/probe.cmd").unwrap());
        assert!(matches(".githooks/**", ".githooks/nested/probe.cmd").unwrap());
        assert!(matches("**/probe.cmd", "probe.cmd").unwrap());
        assert!(matches("**/probe.cmd", "a/b/probe.cmd").unwrap());
        assert!(!matches("/probe.cmd", "nested/probe.cmd").unwrap());
    }

    #[test]
    fn attributes_reject_unproven_syntax_without_io_fallback() {
        for extra in [
            "[attr]custom text eol=crlf",
            "*.cmd custom",
            "\"a b\" text eol=lf",
            "*.[bc]at text eol=lf",
        ] {
            let payload = [PUBLIC, extra.as_bytes()].concat();
            assert!(verify(&payload, &asset()).is_err());
        }
        let invalid = json!({"merge_validation":{"required_rules":["*.cmd text eol=crlf"]}});
        assert!(required(&invalid).is_err());
    }

    #[test]
    fn attributes_projection_separates_new_public_rules_from_local_rules() {
        let before = b"* text=auto eol=lf\n*.ps1 text eol=crlf\n";
        let after = merge(PUBLIC, before, &asset()).unwrap();
        let (old_public, old_project) = projection(&json!({}), before).unwrap();
        let (new_public, new_project) = projection(&asset(), &after).unwrap();
        assert_eq!(old_public, vec![true]);
        assert_eq!(new_public, vec![true; 3]);
        assert_eq!(old_project, new_project);
        let edited = [PUBLIC, b"*.ps1 text eol=lf\n"].concat();
        assert_ne!(projection(&asset(), &edited).unwrap().1, new_project);
    }
}
