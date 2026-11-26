//! Project-owned native release preparation protocol. No version ledger.
use crate::SemVer;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::{fmt, str::FromStr};

/// The optional fourth field identifies the BP distribution used for a product release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProductVersion {
    pub base: SemVer,
    pub bp: Option<u64>,
}

impl FromStr for ProductVersion {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let (base, bp) = match text.match_indices('.').nth(2) {
            Some((separator, _)) => {
                let suffix = text[separator + 1..].parse::<u64>()
                    .map_err(|_| format!("invalid product version: {text}"))?;
                (&text[..separator], Some(suffix))
            }
            None => (text, None),
        };
        let result = Self {
            base: base.parse()?,
            bp,
        };
        if result.to_string() != text {
            return Err(format!("noncanonical product version: {text}"));
        }
        Ok(result)
    }
}

impl fmt::Display for ProductVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.base.fmt(f)?;
        if let Some(value) = self.bp {
            write!(f, ".{value}")?;
        }
        Ok(())
    }
}

impl ProductVersion {
    pub fn bp_base(self) -> Result<SemVer, String> {
        let qualified = self.with_bp(self.base)?;
        if self.bp.is_some() && self != qualified {
            return Err(format!(
                "BP product version {self} does not match its own base {}",
                self.base
            ));
        }
        Ok(self.base)
    }

    pub fn with_bp(self, origin: SemVer) -> Result<Self, String> {
        if origin.major == 0 || origin.minor > 9 || origin.patch > 99 {
            return Err(format!(
                "BP version {origin} requires major >= 1, minor 0..9 and patch 0..99"
            ));
        }
        let code = origin
            .major
            .checked_mul(1000)
            .and_then(|n| n.checked_add(origin.minor * 100 + origin.patch))
            .ok_or_else(|| format!("BP version code overflow: {origin}"))?;
        Ok(Self {
            base: self.base,
            bp: Some(code),
        })
    }

    pub fn notes(self, changelog: &str, prefix: &str) -> Result<String, String> {
        let section = |number: String| {
            let heading = format!("## [{prefix}{number}]");
            let at = changelog
                .match_indices(&heading)
                .map(|(i, _)| i)
                .find(|i| *i == 0 || changelog.as_bytes()[i - 1] == b'\n')?;
            let rest = &changelog[at + heading.len()..];
            let boundary = if prefix.is_empty() {
                "\n## "
            } else {
                "\n## [BP "
            };
            let end = rest.find(boundary).unwrap_or(rest.len());
            Some((
                changelog[at..at + heading.len() + end].trim(),
                rest[..end].trim(),
            ))
        };
        if let Some((exact, _)) = section(self.to_string()) {
            return Ok(exact.into());
        }
        let missing = || {
            format!(
                "release notes missing for {prefix}{self} or its business version {}",
                self.base
            )
        };
        let code = self.bp.ok_or_else(missing)?;
        let heading = format!(
            "## [{prefix}{self}]\n\n业务版本 {}，采用 BP {}.{}.{}。",
            self.base,
            code / 1000,
            code / 100 % 10,
            code % 100
        );
        if let Some((_, body)) = section(self.base.to_string()) {
            return Ok(format!("{heading}\n\n业务版本说明\n{body}"));
        }
        let previous = format!("## [{prefix}{}.", self.base);
        if changelog
            .lines()
            .filter_map(|line| line.strip_prefix(&previous))
            .filter_map(|s| s.split_once(']'))
            .any(|(suffix, _)| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
        {
            return Ok(heading);
        }
        Err(missing())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
// Older project drivers also emit size and digest fields.
pub struct Asset {
    pub name: String,
    /// Repository-relative build output; never a user profile or archive path.
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub product: String,
    pub version: String,
    pub repository: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitlab_origin: Option<String>,
    pub tag: String,
    pub title: String,
    pub notes: String,
    pub assets: Vec<Asset>,
}

impl Plan {
    pub fn asset_path(&self, root: &Path, asset: &Asset) -> Result<std::path::PathBuf, String> {
        if !asset.path.starts_with(".build/") || !identifier(&asset.name) {
            return Err("invalid release attachment path or name".into());
        }
        let path = crate::managed_paths::safe_join(root, &asset.path, "release attachment")?;
        if self.gitlab_origin.is_none()
            && path.file_name().and_then(|n| n.to_str()) != Some(&asset.name)
        {
            return Err("attachment filename differs from public name".into());
        }
        Ok(path)
    }
}

pub fn validate_repository(repository: &str, gitlab_origin: Option<&str>) -> Result<(), String> {
    let parts: Vec<_> = repository.split('/').collect();
    if parts.len() < 2
        || parts.len() > 32
        || parts.iter().any(|p| !identifier(p))
        || (gitlab_origin.is_none() && parts.len() != 2)
    {
        return Err("invalid native release repository".into());
    }
    if let Some(origin) = gitlab_origin {
        validate_gitlab_origin(origin)?;
    }
    Ok(())
}

pub fn validate_gitlab_origin(origin: &str) -> Result<(), String> {
    let authority = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .ok_or("GitLab origin must explicitly use http or https")?;
    let mut pieces = authority.split(':');
    let host = pieces.next().unwrap_or("");
    let port = pieces.next();
    if pieces.next().is_some()
        || host.is_empty()
        || host.len() > 253
        || host.split('.').any(|p| {
            p.is_empty()
                || p.starts_with('-')
                || p.ends_with('-')
                || !p.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
        || port.is_some_and(|p| p.parse::<u16>().map_or(true, |n| n == 0))
    {
        return Err(
            "GitLab origin must be a host and optional port, without credentials, path or query"
                .into(),
        );
    }
    Ok(())
}

pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

pub fn raw_sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod product_version_tests {
    use super::*;
    #[test]
    fn bp_suffix_is_numeric_bounded_and_replaced() {
        let base: ProductVersion = "0.75.1".parse().unwrap();
        let mut previous = base;
        for (bp, n) in [
            ("4.0.0", 4000),
            ("4.0.20", 4020),
            ("4.0.99", 4099),
            ("4.1.0", 4100),
            ("9.9.99", 9999),
            ("10.0.0", 10000),
        ] {
            let v = previous.with_bp(bp.parse().unwrap()).unwrap();
            assert_eq!(v.to_string(), format!("0.75.1.{n}"));
            assert!(v > previous);
            assert_eq!(v.to_string().parse(), Ok(v));
            previous = v;
        }
        for bp in ["0.1.0", "4.10.0", "4.0.100", "18446744073709551615.0.0"] {
            assert!(base.with_bp(bp.parse().unwrap()).is_err());
        }
        assert!("4.0.0.4000".parse::<SemVer>().is_err());
        assert!(
            "4.0.0.4100"
                .parse::<ProductVersion>()
                .unwrap()
                .bp_base()
                .is_err()
        );
        for bad in ["1.2", "1.2.3.04", "1.2.3.4.5", "1.2.3.+4"] {
            assert!(bad.parse::<ProductVersion>().is_err());
        }
    }
    #[test]
    fn notes_use_exact_section_or_label_the_business_background() {
        let v: ProductVersion = "0.75.1.4100".parse().unwrap();
        let exact = "## [0.75.1.4100]\n\nNew";
        assert_eq!(
            v.notes(&format!("{exact}\n## [0.75.1]\nOld"), ""),
            Ok(exact.into())
        );
        assert!(
            v.notes("## [0.75.1]\nOld", "")
                .unwrap()
                .contains("BP 4.1.0")
        );
        assert!(
            !v.notes("## [0.75.1.4000]\nStale", "")
                .unwrap()
                .contains("Stale")
        );
        assert!(v.notes("## [0.75.0]\nWrong business version", "").is_err());
        let own: ProductVersion = "4.0.0.4000".parse().unwrap();
        assert!(
            own.notes("## [BP 4.0.0]\nComponents", "BP ")
                .unwrap()
                .contains("BP 4.0.0")
        );
    }
}
