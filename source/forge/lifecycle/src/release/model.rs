//! Language-neutral values exchanged by release adapters. Native versions stay opaque.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Inputs = BTreeMap<String, Option<Vec<u8>>>;
pub(super) type Writes = BTreeMap<String, Vec<u8>>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(super) enum Language {
    Rust,
    Python,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct NativePackage {
    pub manifest: String,
    pub name: String,
    pub version: String,
    pub language: Language,
    pub workspace: String,
    pub source_roots: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct NativeDependency {
    pub owner: String,
    pub target: String,
    pub kind: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct LanguageSnapshot {
    pub packages: BTreeMap<String, NativePackage>,
    pub dependencies: Vec<NativeDependency>,
    pub inputs: Inputs,
    pub roots: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PackageChange {
    pub manifest: String,
    pub name: String,
    pub version_before: String,
    pub version_after: String,
    pub reason: String,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PackageEvent {
    pub id: String,
    pub language: String,
    pub operation: String,
    pub manifest_before: Option<String>,
    pub manifest_after: Option<String>,
    pub name_before: Option<String>,
    pub name_after: Option<String>,
    pub version_before: Option<String>,
    pub version_after: Option<String>,
    pub related: Vec<String>,
}
