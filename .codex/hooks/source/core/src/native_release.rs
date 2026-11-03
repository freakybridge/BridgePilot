//! Project-owned native release preparation protocol. No version ledger.
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Read-only native source adapter identity. File ownership is explicit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub product: String,
    pub repository: String,
    pub version: String,
    pub tag_prefix: String,
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedVersion {
    pub version: String,
    pub commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceContext {
    pub schema: u32,
    pub identity: SourceIdentity,
    pub baseline: Option<PublishedVersion>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEdit {
    pub path: String,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePlan {
    pub schema: u32,
    pub identity: SourceIdentity,
    pub edits: Vec<SourceEdit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reuse_release: Option<PublishedVersion>,
    /// Original artifact order, owned by the native adapter, including its package manifest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub release_assets: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub name: String,
    /// Repository-relative build output; never a user profile or archive path.
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub product: String,
    pub version: String,
    pub repository: String,
    pub tag: String,
    pub title: String,
    pub notes: String,
    pub assets: Vec<Asset>,
}

impl Plan {
    pub fn validate(&self, root: &Path) -> Result<(), String> {
        if self.schema != 1 || self.product.trim().is_empty() || self.title.trim().is_empty() {
            return Err("invalid native release identity".into());
        }
        self.version
            .parse::<crate::SemVer>()
            .map_err(|e| e.to_string())?;
        let repository: Vec<_> = self.repository.split('/').collect();
        if repository.len() != 2
            || repository.iter().any(|s| !identifier(s))
            || !identifier(&self.tag)
        {
            return Err("invalid GitHub repository or release tag".into());
        }
        if self.assets.is_empty() {
            return Err("native release has no declared artifacts".into());
        }
        let mut names = std::collections::BTreeSet::new();
        for asset in &self.assets {
            if !identifier(&asset.name)
                || !names.insert(&asset.name)
                || !asset.path.starts_with(".build/")
            {
                return Err("invalid, duplicate or non-build release artifact".into());
            }
            let path = crate::managed_paths::safe_join(root, &asset.path, "release artifact")?;
            let bytes = crate::managed_paths::read_metadata(&path, 512 * 1024 * 1024)?
                .ok_or_else(|| format!("release artifact missing: {}", asset.path))?;
            if asset.size != bytes.len() as u64 || asset.sha256 != raw_sha(&bytes) {
                return Err(format!("release artifact changed: {}", asset.name));
            }
        }
        Ok(())
    }
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
