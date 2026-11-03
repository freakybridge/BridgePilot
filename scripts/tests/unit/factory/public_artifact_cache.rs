#[allow(dead_code)]
#[path = "../../support/product_manifest.rs"]
mod fixture_product;

use bridgeforge_core::{ProcessOutput, ProcessRequest, ProcessRunner};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "public-artifact-cache").join(
            format!(
                "bf-public-cache-{}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
        );
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Re-enter this test from an isolated installed executable, so current_exe,
/// installation scope and provenance checks all run without test-only bypasses.
#[test]
fn bootstrap_product_identity_is_explicit_and_preserves_provenance_guards() {
    use bridgeforge_workflow::public_artifacts::{prepare, prepare_with_product_version};
    use sha2::{Digest, Sha256};
    struct Compiler;
    impl ProcessRunner for Compiler {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            assert!(request.program == "cargo" || request.program == "rustc");
            assert_eq!(request.args, ["--version"]);
            Ok(ProcessOutput {
                code: 0,
                stdout: format!("{} 1.94.1\n", request.program.to_string_lossy()).into_bytes(),
                stderr: vec![],
                timed_out: false,
            })
        }
    }
    struct ChangedCompiler;
    impl ProcessRunner for ChangedCompiler {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            let mut output = Compiler.run(request)?;
            output.stdout = format!("{} 1.94.2\n", request.program.to_string_lossy()).into_bytes();
            Ok(output)
        }
    }
    const CHILD: &str = "BRIDGEFORGE_F4_BOOTSTRAP_FIXTURE";
    if let Some(root) = std::env::var_os(CHILD).map(PathBuf::from) {
        let codex = root.join("profile/.codex");
        let binary = std::env::current_exe().unwrap();
        let environment =
            bridgeforge_core::runtime::build_environment(&root.join("templates/hooks"), &Compiler);
        assert_eq!(environment.code, 0, "{}", environment.stderr);
        let identity = environment.receipt.unwrap()["identity"]
            .as_str()
            .unwrap()
            .to_string();
        let proof = json!({"schema":1,"owner":"bridgeforge-user-cli-v1","identity":identity,
            "binary_sha256":format!("{:x}",Sha256::digest(fs::read(&binary).unwrap()))});
        let proof_path = codex.join("bin/bridgeforge-cli-provenance.json");
        fs::write(&proof_path, serde_json::to_vec(&proof).unwrap()).unwrap();
        assert_ne!(
            env!("CARGO_PKG_VERSION"),
            "6.0.0",
            "independent test package must differ from the product"
        );
        let prepare_version = |version| {
            prepare_with_product_version(
                &root,
                &codex,
                Some(&binary),
                Some(&identity),
                None,
                version,
                &Compiler,
            )
        };
        let prepared = prepare_version("6.0.0").unwrap();
        assert_eq!(prepared["status"], "prepared");
        assert_eq!(prepared["version"], "6.0.0");
        assert!(
            prepare_version("0.1.0")
                .unwrap_err()
                .contains("bootstrap CLI version mismatch")
        );
        assert!(
            prepare_version("")
                .unwrap_err()
                .contains("bootstrap CLI version mismatch")
        );
        assert!(
            prepare(
                &root,
                &codex,
                Some(&binary),
                Some(&identity),
                None,
                &Compiler
            )
            .unwrap_err()
            .contains("explicit running product version")
        );
        assert!(
            prepare_with_product_version(
                &root,
                &codex,
                Some(&binary),
                Some(&identity),
                None,
                "6.0.0",
                &ChangedCompiler
            )
            .unwrap_err()
            .contains("compiler/source identity changed")
        );
        let other = codex.join(format!("bin/other{}", std::env::consts::EXE_SUFFIX));
        fs::copy(&binary, &other).unwrap();
        assert!(
            prepare_with_product_version(
                &root,
                &codex,
                Some(&other),
                Some(&identity),
                None,
                "6.0.0",
                &Compiler
            )
            .unwrap_err()
            .contains("verified running CLI")
        );
        let mut tampered = proof.clone();
        tampered["binary_sha256"] = json!("0".repeat(64));
        fs::write(&proof_path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(
            prepare_version("6.0.0")
                .unwrap_err()
                .contains("provenance changed")
        );
        fs::write(&proof_path, serde_json::to_vec(&proof).unwrap()).unwrap();
        fs::write(
            root.join("templates/hooks/src/main.rs"),
            b"fn main(){println!(\"drift\");}\n",
        )
        .unwrap();
        assert!(
            prepare_version("6.0.0")
                .unwrap_err()
                .contains("compiler/source identity changed")
        );
        return;
    }
    let f = Fixture::new();
    let workspace = f.0.join("templates/hooks");
    fs::create_dir_all(workspace.join("src")).unwrap();
    fs::write(workspace.join("Cargo.toml"), b"[workspace]\n").unwrap();
    fs::write(workspace.join("Cargo.lock"), b"version=4\n").unwrap();
    fs::write(workspace.join("src/main.rs"), b"fn main(){}\n").unwrap();
    fs::write(f.0.join("VERSION"), b"6.0.0\n").unwrap();
    fixture_product::write(
        f.0.join("source/forge/bridgeforge-manifest.json"),
        br#"{"release_version":"6.0.0","generated_assets":[]}"#,
    )
    .unwrap();
    let binary = f.0.join(format!(
        "profile/.codex/bin/bridgeforge{}",
        std::env::consts::EXE_SUFFIX
    ));
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::copy(std::env::current_exe().unwrap(), &binary).unwrap();
    let mut request = ProcessRequest::new(binary.into_os_string(), &f.0);
    request.args = [
        "bootstrap_product_identity_is_explicit_and_preserves_provenance_guards",
        "--test-threads=1",
        "--nocapture",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    request
        .env
        .insert(CHILD.into(), f.0.clone().into_os_string());
    request.env_remove = ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"]
        .into_iter()
        .map(Into::into)
        .collect();
    let result = bridgeforge_core::SystemProcessRunner.run(&request).unwrap();
    assert!(!result.timed_out);
    assert_eq!(
        result.code,
        0,
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
fn project_plan_builds_are_reused_without_writing_project_targets() {
    use std::cell::Cell;
    struct Builds {
        builds: Cell<usize>,
        hits: Cell<usize>,
    }
    impl ProcessRunner for Builds {
        fn artifact_cache_enabled(&self) -> bool {
            true
        }
        fn cache_hit(&self) {
            self.hits.set(self.hits.get() + 1);
        }
        fn run(&self, r: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if (r.program == "cargo" || r.program == "rustc")
                && r.args.first().is_some_and(|a| a == "--version")
            {
                return Ok(ProcessOutput {
                    code: 0,
                    stdout: format!("{} 1.94.1\n", r.program.to_string_lossy()).into_bytes(),
                    stderr: vec![],
                    timed_out: false,
                });
            }
            if r.program == "cargo" {
                self.builds.set(self.builds.get() + 1);
                let output = PathBuf::from(&r.args[7]).join("release");
                fs::create_dir_all(&output)?;
                let name = r.args[9].to_string_lossy();
                fs::write(
                    output.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
                    name.as_bytes(),
                )?;
            }
            let name = if r.program.to_string_lossy().contains("bridgeforge-hook") {
                "bridgeforge-hook"
            } else {
                "bridgeforge"
            };
            Ok(ProcessOutput {
                code: 0,
                stdout: serde_json::to_vec(&json!({"schema":1,"name":name,"status":"ok"})).unwrap(),
                stderr: vec![],
                timed_out: false,
            })
        }
    }
    let f = Fixture::new();
    let workspace = f.0.join("templates/hooks");
    fs::create_dir_all(workspace.join("src")).unwrap();
    fs::write(workspace.join("Cargo.toml"), b"[workspace]\n").unwrap();
    fs::write(workspace.join("Cargo.lock"), b"version = 4\n").unwrap();
    fs::write(workspace.join("src/main.rs"), b"fn main() {}\n").unwrap();
    fs::write(f.0.join("VERSION"), b"8.0.0\n").unwrap();
    fs::create_dir_all(f.0.join("templates")).unwrap();
    fixture_product::write(f.0.join("source/forge/bridgeforge-manifest.json"),br#"{"schema_version":4,"baseline_model":"current-only","compatibility_baseline":"1.0.0","assets":[],"generated_assets":[]}"#).unwrap();
    let contract: Value =
        serde_json::from_slice(&crate::manifest::render_managed_contract(&f.0).unwrap()).unwrap();
    let r = Builds {
        builds: Cell::new(0),
        hits: Cell::new(0),
    };
    let first =
        crate::generated_assets::generated_writes_reusing(&f.0, "source_root", &f.0, &contract, &r)
            .unwrap();
    assert_eq!(r.builds.get(), 2);
    let second =
        crate::generated_assets::generated_writes_reusing(&f.0, "source_root", &f.0, &contract, &r)
            .unwrap();
    assert_eq!(r.builds.get(), 2);
    assert_eq!(r.hits.get(), 2);
    assert_eq!(first, second);
    assert!(!f.0.join(".codex").exists());
    assert!(!f.0.join(".runtime").exists());
    fs::write(
        workspace.join("src/main.rs"),
        b"fn main() {println!(\"changed\");}\n",
    )
    .unwrap();
    assert!(
        crate::generated_assets::generated_writes_reusing(&f.0, "source_root", &f.0, &contract, &r)
            .is_err()
    );
    assert_eq!(r.builds.get(), 2);
}

#[allow(dead_code)]
#[path = "../../support/artifact_paths.rs"]
mod test_artifacts;
