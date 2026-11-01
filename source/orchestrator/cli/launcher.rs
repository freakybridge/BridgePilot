fn main() {
    match bridgepilot_install::install::launch() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("BridgePilot launcher: {error}");
            std::process::exit(1);
        }
    }
}
