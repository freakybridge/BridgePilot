//! In-memory timing only. Nested phase and child-process durations overlap.
use super::*;
use crate::ProcessOutput;
use std::cell::RefCell;

pub(super) struct Timings(RefCell<BTreeMap<&'static str, u128>>);

impl Timings {
    pub(super) fn new() -> Self {
        Self(RefCell::new(
            [
                "release_analysis",
                "input_validation",
                "artifact_preparation",
                "automatic_apply",
                "rollback",
                "baseline_checks",
                "fetch",
                "fast_forward",
                "stash",
                "commit_and_hooks",
                "push",
                "local_git",
                "build_processes",
                "other_processes",
            ]
            .into_iter()
            .map(|key| (key, 0))
            .collect(),
        ))
    }

    fn record(&self, key: &'static str, started: Instant) {
        *self.0.borrow_mut().entry(key).or_default() += started.elapsed().as_millis();
    }

    pub(super) fn measure<T>(&self, key: &'static str, operation: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let result = operation();
        self.record(key, started);
        result
    }

    pub(super) fn attach(&self, outcome: &mut CommandOutcome, elapsed: u128) {
        if outcome.receipt.is_none() {
            // A blocked operation may already have changed local or remote state.
            // Do not invent clean/parity/push fields on a formerly receipt-less error.
            outcome.receipt =
                Some(json!({"schema":1,"status":"blocked","reason":outcome.stderr.trim_end()}));
        }
        if let Some(receipt) = outcome.receipt.as_mut().and_then(|r| r.as_object_mut()) {
            receipt.insert(
                "sync_elapsed_ms".into(),
                json!(u64::try_from(elapsed).unwrap_or(u64::MAX)),
            );
            receipt.insert("timings_ms".into(), json!(&*self.0.borrow()));
            receipt.insert("timing_semantics".into(), json!("sync_elapsed_ms is total; phase timings include child processes; commit_and_hooks includes commit hooks; fast_forward includes pull and its fetch; push includes pre-push hooks; nested durations must not be summed"));
        }
    }
}

pub(super) struct Observed<'a> {
    pub(super) runner: &'a dyn ProcessRunner,
    pub(super) timings: &'a Timings,
}

impl ProcessRunner for Observed<'_> {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        let program = Path::new(&request.program)
            .file_stem()
            .and_then(|p| p.to_str());
        let first = request.args.first().and_then(|arg| arg.to_str());
        let key = match (program, first) {
            (Some("git"), Some("fetch")) => "fetch",
            (Some("git"), Some("pull")) => "fast_forward",
            (Some("git"), Some("stash")) => "stash",
            (Some("git"), Some("commit")) => "commit_and_hooks",
            (Some("git"), Some("push")) => "push",
            (Some("git"), _) => "local_git",
            (Some("cargo"), Some("build")) => "build_processes",
            _ => "other_processes",
        };
        self.timings.measure(key, || self.runner.run(request))
    }

    fn artifact_cache_enabled(&self) -> bool {
        self.runner.artifact_cache_enabled()
    }
    fn cache_hit(&self) {
        self.runner.cache_hit();
    }
    fn cache_miss(&self, reason: &'static str) {
        self.runner.cache_miss(reason);
    }
}
