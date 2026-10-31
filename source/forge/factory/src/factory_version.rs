use crate::release::SemVer;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FactoryVersionReport {
    pub schema: u32,
    pub version: String,
    pub contract_version: String,
    pub changelog_present: bool,
    pub healthy: bool,
    pub issues: Vec<String>,
}

pub fn check(root: &Path) -> FactoryVersionReport {
    let version = fs::read_to_string(root.join("VERSION"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let contract = bridgeforge_core::product_manifest::project(root).unwrap_or_default();
    let contract_version = contract["release_version"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).unwrap_or_default();
    let mut issues = Vec::new();
    if version.parse::<SemVer>().is_err() {
        issues.push("root VERSION is not strict SemVer".into());
    }
    if contract_version != version {
        issues.push(format!(
            "managed contract release_version differs from VERSION: {contract_version} != {version}"
        ));
    }
    let changelog_present = changelog.contains(&format!("## [{version}]"))
        || changelog.contains(&format!("## {version}"));
    if !changelog_present {
        issues.push(format!("CHANGELOG has no entry for {version}"));
    }
    FactoryVersionReport {
        schema: 1,
        version,
        contract_version,
        changelog_present,
        healthy: issues.is_empty(),
        issues,
    }
}
