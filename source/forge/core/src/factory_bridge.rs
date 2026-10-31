//! Explicit application adapter for factory-only Git transaction materialization.
//! Public runtimes have no implementation and cannot prepare factory writes.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub struct FactoryTransaction {
    pub generated_reused: usize,
    pub generated_built: usize,
    pub writes: BTreeMap<PathBuf, Vec<u8>>,
    pub binaries: BTreeSet<PathBuf>,
    pub before: BTreeMap<PathBuf, Option<Vec<u8>>>,
    pub verify: Box<dyn Fn(&Path) -> Result<(), String>>,
}
