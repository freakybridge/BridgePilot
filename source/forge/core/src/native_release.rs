//! Project-owned native release preparation protocol. No version ledger.
use serde::{Deserialize, Serialize};
use std::path::Path;

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
        if self.gitlab_origin.is_none() && path.file_name().and_then(|n| n.to_str()) != Some(&asset.name) {
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
