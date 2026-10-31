//! Opt-in, in-memory release profiling. No cache decisions or filesystem writes.
use serde::Serialize;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Default, Serialize)]
struct Counter {
    calls: u64,
    elapsed_us: u64,
}
thread_local! {
    static COUNTERS: RefCell<Option<BTreeMap<String, Counter>>> = const { RefCell::new(None) };
}

pub fn begin() {
    COUNTERS.with(|c| *c.borrow_mut() = Some(BTreeMap::new()));
}

pub struct Span(Option<(String, Instant)>);
pub fn span(name: impl Into<String>) -> Span {
    Span(
        COUNTERS
            .with(|c| c.borrow().is_some())
            .then(|| (name.into(), Instant::now())),
    )
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some((name, start)) = self.0.take() {
            COUNTERS.with(|c| {
                if let Some(counters) = c.borrow_mut().as_mut() {
                    let counter = counters.entry(name).or_default();
                    counter.calls += 1;
                    counter.elapsed_us += start.elapsed().as_micros().min(u64::MAX as u128) as u64;
                }
            });
        }
    }
}

pub fn finish() -> serde_json::Value {
    let counters = COUNTERS.with(|c| c.borrow_mut().take().unwrap_or_default());
    serde_json::json!({"schema":1,"unit":"microseconds","semantics":"inclusive spans overlap; process counts are launch attempts; no wall-time sum", "counters":counters})
}
