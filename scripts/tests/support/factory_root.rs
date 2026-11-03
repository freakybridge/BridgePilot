//! Factory test data is located by repository identity, not crate nesting depth.
use std::path::Path;

pub(crate) fn from_manifest_dir(manifest_dir: &str) -> &Path {
    Path::new(manifest_dir)
        .ancestors()
        .find(|root| {
            root.join("VERSION").is_file()
                && root.join("source/forge/bridgeforge-manifest.json").is_file()
        })
        .unwrap_or_else(|| panic!("factory test repository not found above {manifest_dir}"))
}
