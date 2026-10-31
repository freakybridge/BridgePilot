use crate::{ProcessOutput, ProcessRequest, ProcessRunner};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildMetrics {
    pub timings_ms: BTreeMap<String, u128>,
    pub cargo_build_invocations: usize,
    pub cache_hits: usize,
    pub cache_misses: BTreeMap<String, usize>,
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_build_metrics.rs"]
mod tests;

pub struct Observed<'a> {
    pub runner: &'a dyn ProcessRunner,
    pub metrics: RefCell<BuildMetrics>,
}
impl ProcessRunner for Observed<'_> {
    fn factory_transaction(
        &self,
        root: &std::path::Path,
        writes: BTreeMap<std::path::PathBuf, Vec<u8>>,
        inputs: BTreeMap<std::path::PathBuf, Option<Vec<u8>>>,
        executor: &dyn ProcessRunner,
    ) -> Result<crate::factory_bridge::FactoryTransaction, String> {
        self.runner
            .factory_transaction(root, writes, inputs, executor)
    }
    fn artifact_cache_enabled(&self) -> bool {
        self.runner.artifact_cache_enabled()
    }
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        let build =
            request.program == "cargo" && request.args.first().is_some_and(|a| a == "build");
        let started = Instant::now();
        let result = self.runner.run(request);
        if build {
            let mut metrics = self.metrics.borrow_mut();
            metrics.cargo_build_invocations += 1;
            *metrics.timings_ms.entry("cargo_build".into()).or_default() +=
                started.elapsed().as_millis();
        }
        result
    }
    fn cache_hit(&self) {
        self.metrics.borrow_mut().cache_hits += 1;
        self.runner.cache_hit();
    }
    fn cache_miss(&self, reason: &'static str) {
        *self
            .metrics
            .borrow_mut()
            .cache_misses
            .entry(reason.into())
            .or_default() += 1;
        self.runner.cache_miss(reason);
    }
    fn build_group_finished(&self, group: &'static str, elapsed_ms: u128) {
        *self
            .metrics
            .borrow_mut()
            .timings_ms
            .entry(format!("build_group_{group}"))
            .or_default() += elapsed_ms;
        self.runner.build_group_finished(group, elapsed_ms);
    }
}
