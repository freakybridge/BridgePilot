//! Factory application adapter. Public runtimes do not link this implementation.
use crate::{ProcessOutput, ProcessRequest, ProcessRunner};
use bridgeforge_core::factory_bridge::FactoryTransaction;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct FactoryRunner<'a> {
    inner: &'a dyn ProcessRunner,
}

impl<'a> FactoryRunner<'a> {
    pub fn new(inner: &'a dyn ProcessRunner) -> Self {
        Self { inner }
    }
}

impl ProcessRunner for FactoryRunner<'_> {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        self.inner.run(request)
    }
    fn artifact_cache_enabled(&self) -> bool {
        self.inner.artifact_cache_enabled()
    }
    fn cache_hit(&self) {
        self.inner.cache_hit();
    }
    fn cache_miss(&self, reason: &'static str) {
        self.inner.cache_miss(reason);
    }
    fn build_group_finished(&self, group: &'static str, elapsed_ms: u128) {
        self.inner.build_group_finished(group, elapsed_ms);
    }
    fn factory_transaction(
        &self,
        root: &Path,
        writes: BTreeMap<PathBuf, Vec<u8>>,
        release_inputs: BTreeMap<PathBuf, Option<Vec<u8>>>,
        executor: &dyn ProcessRunner,
    ) -> Result<FactoryTransaction, String> {
        let mut plan =
            crate::git_sync_plan::WritePlan::prepare(root, writes, release_inputs, true, executor)?;
        Ok(FactoryTransaction {
            generated_reused: plan.generated_reused,
            generated_built: plan.generated_built,
            writes: std::mem::take(&mut plan.writes),
            binaries: std::mem::take(&mut plan.binaries),
            before: std::mem::take(&mut plan.before),
            verify: Box::new(move |root| plan.verify_unchanged(root)),
        })
    }
}
