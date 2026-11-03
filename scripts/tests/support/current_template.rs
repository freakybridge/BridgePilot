//! Minimal current skeleton initialization; no legacy contracts or release setup.
use std::{fs, path::Path};

pub(crate) fn write(root: &Path) {
    for asset in bridgeforge_core::template_standard::assets() {
        let bytes = bridgeforge_skeleton::content::render(
            asset.rule["id"].as_str().unwrap(),
            &asset.payload,
            None,
            &asset.rule,
            root,
            true,
        )
        .unwrap();
        let path = root.join(asset.rule["target"].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|path| {
            path.join("source/forge/bridgeforge-manifest.json")
                .is_file()
        })
        .unwrap();
    bridgeforge_skeleton::template::verify_runtime(repository)
        .expect("prepare official tools and compile tests with their public build identity");
    let destination = root.join(".deps/bridgeforge/bin");
    fs::create_dir_all(&destination).unwrap();
    for name in [
        format!("bridgeforge{}", std::env::consts::EXE_SUFFIX),
        format!("bridgeforge-hook{}", std::env::consts::EXE_SUFFIX),
        "build-receipt-cli.json".into(),
        "build-receipt-hook.json".into(),
    ] {
        fs::copy(
            repository.join(".build/bridgeforge/bin").join(&name),
            destination.join(name),
        )
        .unwrap();
    }
    bridgeforge_skeleton::template::verify_runtime(root)
        .expect("isolated runtime must retain official bytes and receipts");
}
