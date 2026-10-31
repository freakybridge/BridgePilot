use bridgeforge_factory::{CommandOutcome, commands};
use serde_json::json;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let profile = args.first().is_some_and(|v| v == "git-sync")
        && std::env::var_os("BRIDGEFORGE_PROFILE").is_some_and(|v| v == "1");
    if profile {
        bridgeforge_factory::release_metrics::begin();
    }
    let started = std::time::Instant::now();
    let mut outcome: CommandOutcome = commands::run(&args);
    if profile {
        let mut metrics = bridgeforge_factory::release_metrics::finish();
        metrics["cli_elapsed_us"] = json!(started.elapsed().as_micros() as u64);
        outcome.receipt.get_or_insert_with(
            || json!({"status":if outcome.code == 0 {"ok"} else {"blocked"}}),
        )["profile"] = metrics;
    }
    std::process::exit(bridgeforge_core::output::emit(outcome));
}
