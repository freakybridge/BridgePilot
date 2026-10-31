use crate::{CommandOutcome, ProcessRequest, ProcessRunner, ProjectContext};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeReceipt {
    pub schema: u32,
    pub status: String,
    pub project_root: PathBuf,
    pub manifest: PathBuf,
    pub lockfile: PathBuf,
    pub binary: PathBuf,
    pub cargo_version: String,
    pub rustc_version: String,
    pub python_required: bool,
}

fn tool_version(program: &str, root: &Path, runner: &dyn ProcessRunner) -> Result<String, String> {
    let mut request = ProcessRequest::new(program, root);
    request.args = vec!["--version".into()];
    request.timeout = Duration::from_secs(10);
    let output = runner
        .run(&request)
        .map_err(|_| format!("{program} is unavailable"))?;
    if output.timed_out || output.code != 0 {
        return Err(format!("{program} version check failed"));
    }
    let text = String::from_utf8(output.stdout).map_err(|_| "tool version is not UTF-8")?;
    let mut parts = text.split_whitespace();
    if parts.next() != Some(program) {
        return Err(format!("invalid {program} version"));
    }
    let version = parts
        .next()
        .ok_or("missing tool version")?
        .parse::<crate::SemVer>()?;
    let minimum = format!("{}.0", env!("CARGO_PKG_RUST_VERSION")).parse::<crate::SemVer>()?;
    if version < minimum {
        return Err(format!(
            "{program} must be at least {}",
            env!("CARGO_PKG_RUST_VERSION")
        ));
    }
    Ok(text.trim().to_string())
}

pub fn validate_product(
    root: &Path,
    binary: &Path,
    runner: &dyn ProcessRunner,
) -> Result<RuntimeReceipt, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let source = crate::source_projection::source_root(&root);
    let manifest = source.join("Cargo.toml");
    let lockfile = source.join("Cargo.lock");
    if !manifest.is_file() || !lockfile.is_file() || !binary.is_file() {
        return Err("managed product runtime is missing or incomplete".into());
    }
    let product_version = std::fs::read_to_string(root.join("VERSION"))
        .map_err(|e| e.to_string())?
        .trim()
        .to_owned();
    product_version.parse::<crate::SemVer>()?;
    let cargo_version = tool_version("cargo", &root, runner)?;
    let rustc_version = tool_version("rustc", &root, runner)?;
    let mut request = ProcessRequest::new("cargo", &root);
    request.args = vec![
        "metadata".into(),
        "--locked".into(),
        "--offline".into(),
        "--no-deps".into(),
        "--format-version".into(),
        "1".into(),
        "--manifest-path".into(),
        manifest.clone().into_os_string(),
    ];
    let result = runner
        .run(&request)
        .map_err(|_| "managed workspace validation failed")?;
    if result.timed_out || result.code != 0 {
        return Err("managed workspace/lockfile validation failed".into());
    }
    let mut self_test = ProcessRequest::new(binary.as_os_str(), &root);
    self_test.args = vec!["self-test".into(), "--json".into()];
    let result = runner
        .run(&self_test)
        .map_err(|_| "managed binary self-test failed")?;
    let receipt: serde_json::Value =
        serde_json::from_slice(&result.stdout).map_err(|_| "invalid self-test receipt")?;
    if result.timed_out
        || result.code != 0
        || receipt["schema"] != 1
        || receipt["name"] != "bridgeforge"
        || receipt["status"] != "ok"
        || receipt["version"] != product_version
    {
        return Err("managed binary self-test identity mismatch".into());
    }
    Ok(RuntimeReceipt {
        schema: 1,
        status: "ok".into(),
        project_root: root,
        manifest,
        lockfile,
        binary: binary.to_path_buf(),
        cargo_version,
        rustc_version,
        python_required: false,
    })
}

fn binary_path(root: &Path) -> PathBuf {
    crate::artifact_paths::binary(
        root,
        root.join("bridgeforge-manifest.json").is_file(),
        "bridgeforge",
    )
}

pub fn validate(
    root: Option<&Path>,
    runner: &dyn ProcessRunner,
    require_binary: bool,
) -> Result<RuntimeReceipt, String> {
    let context = ProjectContext::discover(root)?;
    let manifest = context.codex_root().join("hooks/Cargo.toml");
    let lockfile = context.codex_root().join("hooks/Cargo.lock");
    if !manifest.is_file() || !lockfile.is_file() {
        return Err("managed Rust workspace is missing or incomplete".into());
    }
    let binary = binary_path(context.root());
    if require_binary && !binary.is_file() {
        return Err(format!(
            "managed BridgeForge binary is missing: {}",
            binary.display()
        ));
    }
    let mut request = ProcessRequest::new("cargo", context.root());
    request.args = vec![OsString::from("--version")];
    request.timeout = Duration::from_secs(10);
    let output = runner
        .run(&request)
        .map_err(|error| format!("Cargo is unavailable: {error}"))?;
    if output.timed_out || output.code != 0 {
        return Err("Cargo version check failed".into());
    }
    let version = String::from_utf8(output.stdout)
        .map_err(|_| "Cargo version output is not UTF-8")?
        .trim()
        .to_string();
    if !version.starts_with("cargo ") {
        return Err("Cargo version output is invalid".into());
    }
    Ok(RuntimeReceipt {
        schema: 1,
        status: "ok".into(),
        project_root: context.root().to_path_buf(),
        manifest,
        lockfile,
        binary,
        cargo_version: version,
        rustc_version: tool_version("rustc", context.root(), runner)?,
        python_required: false,
    })
}

pub fn outcome(
    root: Option<&Path>,
    runner: &dyn ProcessRunner,
    require_binary: bool,
) -> CommandOutcome {
    match validate(root, runner, require_binary) {
        Ok(receipt) => CommandOutcome::with_receipt(
            serde_json::to_value(receipt).expect("runtime receipt is serializable"),
        ),
        Err(error) => CommandOutcome::blocked(format!("[bridgeforge-runtime] BLOCKED: {error}\n")),
    }
}

pub fn health_contract() -> serde_json::Value {
    json!({"schema": 1, "runtime": "rust-cargo", "pythonRequired": false})
}

/// Optional compilation identity for updater reuse. No installation or writes.
pub fn build_environment(root: &Path, runner: &dyn ProcessRunner) -> CommandOutcome {
    let result = (|| -> Result<serde_json::Value, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let Some(environment) = crate::artifact_cache::environment(&root, runner) else {
            return Ok(json!({"schema":1,"status":"uncacheable"}));
        };
        let projected = if crate::source_projection::available(&root) {
            Some(crate::source_projection::render(&root, true)?)
        } else {
            None
        };
        let source = if let Some(files) = &projected {
            crate::source_projection::tree_sha(files)?
        } else {
            let sources = crate::manifest::generated_sources(&root)?;
            crate::manifest::generated_source_sha(&root, &sources)?
        };
        use sha2::Digest;
        let lock = if let Some(files) = &projected {
            files["Cargo.lock"].clone()
        } else {
            std::fs::read(root.join("Cargo.lock")).map_err(|e| e.to_string())?
        };
        let lock = format!("sha256:{:x}", sha2::Sha256::digest(&lock));
        let identity = crate::manifest::canonical_sha(
            &json!({"schema":1,"source":source,"lock":lock,
            "environment":environment,"recipe":crate::manifest::generated_build_recipe("bridgeforge")}),
        )?;
        Ok(json!({"schema":1,"status":"cacheable","identity":identity}))
    })();
    match result {
        Ok(value) => CommandOutcome::with_receipt(value),
        Err(e) => CommandOutcome::blocked(format!("[build-environment] {e}\n")),
    }
}

// Running Windows images can be renamed, but cannot be overwritten or unlinked.
// Keep that image in the ignored runtime directory until a later maintenance run.
pub fn write_binary(root: &Path, path: &Path, payload: &[u8]) -> Result<(), String> {
    use std::{
        fs,
        io::Write,
        time::{SystemTime, UNIX_EPOCH},
    };
    for ancestor in path.ancestors().filter(|p| p.exists()) {
        if crate::persistence::is_link_or_reparse(ancestor).map_err(|e| e.to_string())? {
            return Err(format!("binary path traverses a link: {}", path.display()));
        }
    }
    let parent = path.parent().ok_or("binary has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let destination = parent
        .canonicalize()
        .map_err(|e| e.to_string())?
        .join(path.file_name().ok_or("binary has no filename")?);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let stage = parent.join(format!(
        ".bridgeforge-binary-{}-{nonce}.tmp",
        std::process::id()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&stage)
            .map_err(|e| e.to_string())?;
        file.write_all(payload)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        drop(file);
        match fs::rename(&stage, &destination) {
            Ok(()) => Ok(()),
            Err(error) => {
                #[cfg(windows)]
                if can_relocate_running_image(root, &destination)? {
                    use sha2::{Digest, Sha256};
                    let directory = image_directory(root)?;
                    let before = fs::read(&destination).map_err(|e| e.to_string())?;
                    let backup = directory.join(format!(
                        "{:x}-{}-{nonce}.exe",
                        Sha256::digest(&before),
                        std::process::id()
                    ));
                    fs::rename(&destination, &backup)
                        .map_err(|e| format!("cannot relocate running image: {e}"))?;
                    if let Err(install) = fs::rename(&stage, &destination) {
                        fs::rename(&backup, &destination).map_err(|restore| format!("image install failed: {install}; restore failed: {restore}; original image retained at {}", backup.display()))?;
                        return Err(format!(
                            "image install failed; original restored: {install}"
                        ));
                    }
                    return Ok(());
                }
                #[cfg(not(windows))]
                let _ = root;
                Err(error.to_string())
            }
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&stage);
    }
    result.map_err(|e| format!("cannot install binary {}: {e}", path.display()))
}

#[cfg(windows)]
fn can_relocate_running_image(root: &Path, destination: &Path) -> Result<bool, String> {
    // Keep arbitrary-name self replacement, including updater stage programs.
    if std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .ok()
        .as_deref()
        == Some(destination)
    {
        return Ok(true);
    }
    if !matches!(
        destination.file_name().and_then(|p| p.to_str()),
        Some("bridgeforge.exe" | "bridgeforge-hook.exe" | "bridgeforge-factory.exe")
    ) {
        return Ok(false);
    }
    let directory = root.join(crate::artifact_paths::bin_dir(
        root.join("bridgeforge-manifest.json").is_file(),
    ));
    for ancestor in directory.ancestors().filter(|path| path.exists()) {
        if crate::persistence::is_link_or_reparse(ancestor).map_err(|e| e.to_string())? {
            return Err("managed binary directory traverses a link".into());
        }
    }
    let directory = match directory.canonicalize() {
        Ok(directory) => directory,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    Ok(destination.parent() == Some(directory.as_path()))
}

fn image_directory(root: &Path) -> Result<PathBuf, String> {
    let directory = root.join(".build/bridgeforge/git-sync-images");
    for ancestor in directory.ancestors().filter(|p| p.exists()) {
        if crate::persistence::is_link_or_reparse(ancestor).map_err(|e| e.to_string())? {
            return Err("running-image directory traverses a link".into());
        }
    }
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(directory)
}

pub fn cleanup_images(root: &Path) -> Result<(), String> {
    image_directory(root)?;
    for path in image_cleanup_candidates(root)? {
        // A still-running sync may retain its image; never kill it to clean up.
        if let Err(error) = std::fs::remove_file(path) {
            if error.kind() != std::io::ErrorKind::PermissionDenied {
                return Err(error.to_string());
            }
        }
    }
    Ok(())
}

pub fn image_cleanup_candidates(root: &Path) -> Result<Vec<PathBuf>, String> {
    Ok(image_cleanup_selections(root)?
        .into_iter()
        .map(|selection| selection.path)
        .collect())
}

pub fn image_cleanup_selections(
    root: &Path,
) -> Result<Vec<crate::artifact_cleanup::Selection>, String> {
    use sha2::{Digest, Sha256};
    let directory = crate::managed_paths::safe_join(
        root,
        ".build/bridgeforge/git-sync-images",
        "old image cleanup",
    )?;
    let mut candidates = Vec::new();
    if !directory.exists() {
        return Ok(candidates);
    }
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let parts = name
            .strip_suffix(".exe")
            .unwrap_or("")
            .split('-')
            .collect::<Vec<_>>();
        if parts.len() != 3
            || parts[0].len() != 64
            || !parts[0].bytes().all(|b| b.is_ascii_hexdigit())
            || !parts[1..]
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            || crate::persistence::is_link_or_reparse(&entry.path()).map_err(|e| e.to_string())?
        {
            continue;
        }
        let payload = std::fs::read(entry.path()).map_err(|e| e.to_string())?;
        if format!("{:x}", Sha256::digest(payload)) == parts[0] {
            candidates.push(crate::artifact_cleanup::Selection {
                path: entry.path(),
                members: std::collections::BTreeMap::from([(
                    "".into(),
                    Some(format!("sha256:{}", parts[0])),
                )]),
            });
        }
    }
    Ok(candidates)
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_runtime.rs"]
mod tests;
