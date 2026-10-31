#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    std::process::exit(bridgeforge_hook::run(std::env::args().skip(1).collect()));
}
