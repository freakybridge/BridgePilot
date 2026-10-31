//! Static Python requirements. No project code or marker expression is executed.
use regex::Regex;
use std::str::FromStr;

/// pep508's VerbatimUrl trait parser expands environment variables against the
/// host. Historical release snapshots must remain independent of that host.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct StaticUrl(String);
impl std::fmt::Display for StaticUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl pep508_rs::Pep508Url for StaticUrl {
    type Err = pep508_rs::VerbatimUrlError;
    fn parse_url(raw: &str, _working_dir: Option<&std::path::Path>) -> Result<Self, Self::Err> {
        if !raw.starts_with("{root:uri}/") {
            // The inherent FromStr path validates URL syntax without expanding
            // ${PROJECT_ROOT} or arbitrary environment variables.
            pep508_rs::VerbatimUrl::from_str(raw)?;
        }
        Ok(Self(raw.into()))
    }
}

pub(super) fn normalize_name(name: &str) -> Result<String, String> {
    pep508_rs::PackageName::from_str(name)
        .map(|value| value.to_string())
        .map_err(|e| format!("Invalid Python distribution name {name}: {e}"))
}

#[derive(Clone, Debug)]
pub(super) struct Requirement {
    pub name: String,
    pub specifier: String,
    pub direct: bool,
}

pub(super) fn parse(raw: &str) -> Result<Requirement, String> {
    let parsed = pep508_rs::Requirement::<StaticUrl>::from_str(raw)
        .map_err(|e| format!("Invalid Python requirement: {e}"))?;
    let (specifier, direct) = match parsed.version_or_url {
        Some(pep508_rs::VersionOrUrl::VersionSpecifier(value)) => (value.to_string(), false),
        Some(pep508_rs::VersionOrUrl::Url(value)) => (value.to_string(), true),
        None => (String::new(), false),
    };
    Ok(Requirement {
        name: parsed.name.to_string(),
        specifier,
        direct,
    })
}

/// Source-independent requirement conditions, canonicalized by PEP 508's
/// marker AST without evaluating them against the host environment.
pub(super) fn requirement_identity(raw: &str) -> Result<(Vec<String>, String), String> {
    let parsed = pep508_rs::Requirement::<StaticUrl>::from_str(raw)
        .map_err(|error| format!("Invalid Python requirement: {error}"))?;
    let mut extras = parsed
        .extras
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    extras.sort();
    extras.dedup();
    let marker = parsed
        .marker
        .contents()
        .map(|marker| marker.to_string())
        .unwrap_or_default();
    Ok((extras, marker))
}

fn release(raw: &str) -> Result<Vec<u64>, String> {
    let re = Regex::new(r"^([0-9]+(?:\.[0-9]+)*)$").unwrap();
    let captures = re
        .captures(raw)
        .ok_or_else(|| format!("Complex Python constraint requires explicit adjustment: {raw}"))?;
    captures[1]
        .split('.')
        .map(|n| {
            n.parse::<u64>()
                .map_err(|_| format!("Python version component overflows: {raw}"))
        })
        .collect()
}

fn compare(left: &[u64], right: &[u64]) -> std::cmp::Ordering {
    (0..left.len().max(right.len()))
        .map(|i| {
            left.get(i)
                .copied()
                .unwrap_or(0)
                .cmp(&right.get(i).copied().unwrap_or(0))
        })
        .find(|order| !order.is_eq())
        .unwrap_or(std::cmp::Ordering::Equal)
}

pub(super) fn accepts(raw: &str, target: &str) -> Result<bool, String> {
    let parsed_target = pep440_rs::Version::from_str(target)
        .map_err(|e| format!("Invalid Python target version: {e}"))?;
    let raw = raw.trim();
    let raw = raw
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(raw);
    if raw.is_empty() || raw == "*" {
        return Ok(true);
    }
    // Poetry's caret and tilde are not PEP 440 operators.
    if let Some(bound) = raw
        .strip_prefix('^')
        .or_else(|| raw.strip_prefix('~').filter(|_| !raw.starts_with("~=")))
    {
        let target = release(target)?;
        let lower = release(bound)?;
        let mut upper = lower.clone();
        let index = if raw.starts_with('^') {
            lower
                .iter()
                .position(|v| *v != 0)
                .unwrap_or(lower.len() - 1)
        } else {
            if lower.len() == 1 { 0 } else { 1 }
        };
        upper[index] = upper[index]
            .checked_add(1)
            .ok_or("Python constraint overflow")?;
        for value in &mut upper[index + 1..] {
            *value = 0;
        }
        return Ok(!compare(&target, &lower).is_lt() && compare(&target, &upper).is_lt());
    }
    // Poetry bare numeric versions are exact constraints.
    let converted = if raw.starts_with(|c: char| c.is_ascii_digit()) {
        format!("=={raw}")
    } else {
        raw.to_string()
    };
    let specifiers = pep440_rs::VersionSpecifiers::from_str(&converted)
        .map_err(|e| format!("Invalid Python constraint '{raw}': {e}"))?;
    Ok(specifiers.contains(&parsed_target))
}

/// Preserve a constraint when it already permits the target. Only a single
/// unambiguous exact/lower/compatible clause may be rewritten automatically.
pub(super) fn update_constraint(raw: &str, before: &str, target: &str) -> Result<String, String> {
    if raw.trim().is_empty() || raw.trim() == "*" {
        return Ok(raw.to_string());
    }
    if accepts(raw, target)? {
        return Ok(raw.to_string());
    }
    let trimmed = raw.trim();
    let re = Regex::new(r"^(==|===|>=|~=|\^|~)?\s*([0-9]+(?:\.[0-9]+)*)$").unwrap();
    let captures = re.captures(trimmed).ok_or_else(|| {
        format!(
            "Python local dependency constraint '{raw}' excludes {target}; adjust it explicitly"
        )
    })?;
    if pep440_rs::Version::from_str(&captures[2]).map_err(|e| e.to_string())?
        != pep440_rs::Version::from_str(before).map_err(|e| e.to_string())?
    {
        return Err(format!(
            "Python local constraint '{raw}' is not tied to previous version {before}; adjust it explicitly"
        ));
    }
    let operator = captures.get(1).map(|m| m.as_str()).unwrap_or("");
    Ok(format!("{operator}{target}"))
}

pub(super) fn update_requirement(raw: &str, before: &str, target: &str) -> Result<String, String> {
    let requirement = parse(raw)?;
    if requirement.direct {
        return Ok(raw.to_string());
    }
    let replacement = update_constraint(&requirement.specifier, before, target)?;
    if replacement == requirement.specifier {
        return Ok(raw.to_string());
    }
    let mut parsed =
        pep508_rs::Requirement::<StaticUrl>::from_str(raw).map_err(|e| e.to_string())?;
    parsed.version_or_url = Some(pep508_rs::VersionOrUrl::VersionSpecifier(
        pep440_rs::VersionSpecifiers::from_str(&replacement).map_err(|e| e.to_string())?,
    ));
    Ok(parsed.to_string())
}
