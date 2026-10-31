use crate::CommandOutcome;
use std::io::Write;
pub fn emit(outcome: CommandOutcome) -> i32 {
    if let Some(receipt) = outcome.receipt {
        println!("{receipt}");
    } else if !outcome.stdout.is_empty() {
        print!("{}", outcome.stdout);
    }
    if !outcome.stderr.is_empty() {
        let _ = std::io::stderr().write_all(outcome.stderr.as_bytes());
    }
    outcome.code
}
