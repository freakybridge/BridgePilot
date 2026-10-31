use std::{env, fs, path::PathBuf};

fn main() {
    let package = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("package directory"));
    let root = package.ancestors().skip(1).find(|path| {
        fs::read_to_string(path.join("Cargo.toml"))
            .is_ok_and(|text| text.lines().any(|line| line.trim() == "[workspace]"))
    }).expect("owning workspace manifest");
    let manifest = root.join("Cargo.toml");
    let lock = root.join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", lock.display());
    println!("cargo:rustc-env=BRIDGEFORGE_WORKSPACE_MANIFEST={}", manifest.display());
    println!("cargo:rustc-env=BRIDGEFORGE_WORKSPACE_LOCK={}", lock.display());
}
