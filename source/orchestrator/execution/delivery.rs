//! Old Forge ledger-based automatic delivery proofs are retired. Keep explicit
//! compatibility errors; never reinterpret ordinary Git/native test success as
//! the old managed release certification or mutate a task into Released.
use crate::{Result, store::Store};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

const RETIRED: &str = "Forge configuration/ledger-based development and publication verification is retired; use project-native build/test/version and explicit human review; task state was not changed";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentReview {
    pub run: String,
    pub result_sha256: String,
    pub source: String,
    pub note: String,
    pub tool: PathBuf,
    pub packages: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationReview {
    pub run: String,
    pub result_sha256: String,
    pub source: String,
    pub receipt_path: PathBuf,
    pub receipt_sha256: String,
    pub remote: String,
    pub remote_ref: String,
}
pub fn development_probe(_root: &Path, _tool: &Path, _ids: &[String]) -> Result<Value> { Err(RETIRED.into()) }
pub fn review_development(_store: &Store, _expected: u64, _input: DevelopmentReview) -> Result<Value> { Err(RETIRED.into()) }
pub fn review_publication(_store: &Store, _expected: u64, _input: PublicationReview) -> Result<Value> { Err(RETIRED.into()) }
