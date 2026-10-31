fn main() {
    match bridgepilot_orchestrator::cli::run(std::env::args().skip(1).collect()) {
        Ok(value) => println!("{}", serde_json::to_string(&value).expect("JSON receipt")),
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::json!({"schema":1,"status":"blocked","error":error})
            );
            std::process::exit(2);
        }
    }
}
