#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

use bridgeforge_core::{ProcessOutput, ProcessRequest, ProcessRunner};
use bridgeforge_workflow::project_sync::build_generated_assets;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    contract: Value,
}

#[test]
fn concurrent_legacy_receipt_change_is_preserved_on_build_rollback() {
    let fixture = Fixture::new();
    let path = fixture.root.join(".codex/bin/build-receipt.json");
    fs::write(&path, legacy_receipt_bytes()).unwrap();
    let runner = Runner {
        root: &fixture.root,
        mode: "legacy-receipt-drift",
        calls: Cell::new(0),
    };
    let error = build_generated_assets(&fixture.root, &fixture.contract, &runner).unwrap_err();
    assert!(error.contains("changed during build"));
    assert_eq!(fs::read(path).unwrap(), b"concurrent unknown receipt");
    fixture.assert_preserved();
}

fn legacy_receipt_bytes() -> Vec<u8> {
    let hash = format!("sha256:{}", "a".repeat(64));
    serde_json::to_vec(
        &json!({"schema_version":1,"generated_asset_id":"codex.hooks",
        "platform":"windows-x86_64","source_tree_sha256":hash,"cargo_lock_sha256":hash,
        "build_recipe_sha256":hash,"self_test_sha256":hash,"binary_sha256":hash,
        "cargo_version":"cargo 1.94.1"}),
    )
    .unwrap()
}

#[test]
fn build_assets_retires_known_receipt_only_after_success() {
    for mode in ["ok", "build-fail", "self-test-fail"] {
        let fixture = Fixture::new();
        let path = fixture.root.join(".codex/bin/build-receipt.json");
        let bytes = legacy_receipt_bytes();
        fs::write(&path, &bytes).unwrap();
        let runner = Runner {
            root: &fixture.root,
            mode,
            calls: Cell::new(0),
        };
        let result = build_generated_assets(&fixture.root, &fixture.contract, &runner);
        if mode == "ok" {
            assert_eq!(result.unwrap().len(), 2);
            assert!(!path.exists());
        } else {
            assert!(result.is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            fixture.assert_preserved();
        }
        // The stable lock file remains; releasing the handle must permit another build.
        assert!(
            fixture
                .root
                .join(".runtime/bridgeforge-codex/project-sync.lock")
                .is_file()
        );
        let retry = Runner {
            root: &fixture.root,
            mode: "ok",
            calls: Cell::new(0),
        };
        assert_eq!(
            build_generated_assets(&fixture.root, &fixture.contract, &retry)
                .unwrap()
                .len(),
            2
        );
    }
}

#[test]
fn unknown_or_non_file_legacy_receipt_blocks_before_build_and_is_preserved() {
    let known = legacy_receipt_bytes();
    let mut duplicate = b"{\"schema_version\":99,".to_vec();
    duplicate.extend_from_slice(&known[1..]);
    for bytes in [
        b"not-json".as_slice(),
        b"{\"schema_version\":99}".as_slice(),
        b"directory".as_slice(),
        duplicate.as_slice(),
    ] {
        let fixture = Fixture::new();
        let path = fixture.root.join(".codex/bin/build-receipt.json");
        if bytes == b"directory" {
            fs::create_dir(&path).unwrap();
        } else {
            fs::write(&path, bytes).unwrap();
        }
        let runner = Runner {
            root: &fixture.root,
            mode: "ok",
            calls: Cell::new(0),
        };
        assert!(build_generated_assets(&fixture.root, &fixture.contract, &runner).is_err());
        assert_eq!(runner.calls.get(), 0);
        if bytes == b"directory" {
            assert!(path.is_dir());
        } else {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        fixture.assert_preserved();
    }
}

#[test]
fn build_assets_holds_project_lock_during_build_and_releases_after_error() {
    let fixture = Fixture::new();
    struct Contender<'a> {
        fixture: &'a Fixture,
        calls: Cell<usize>,
    }
    impl ProcessRunner for Contender<'_> {
        fn run(&self, _: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            self.calls.set(self.calls.get() + 1);
            let nested = Runner {
                root: &self.fixture.root,
                mode: "ok",
                calls: Cell::new(0),
            };
            let error = build_generated_assets(&self.fixture.root, &self.fixture.contract, &nested)
                .unwrap_err();
            assert!(error.contains("lock unavailable"));
            assert_eq!(nested.calls.get(), 0);
            Err(std::io::Error::other("injected build failure"))
        }
    }
    let contender = Contender {
        fixture: &fixture,
        calls: Cell::new(0),
    };
    assert!(build_generated_assets(&fixture.root, &fixture.contract, &contender).is_err());
    assert_eq!(contender.calls.get(), 1);
    fixture.assert_preserved();
    let runner = Runner {
        root: &fixture.root,
        mode: "ok",
        calls: Cell::new(0),
    };
    build_generated_assets(&fixture.root, &fixture.contract, &runner).unwrap();
}

#[cfg(windows)]
#[test]
fn legacy_receipt_delete_failure_rolls_back_installed_assets() {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Fixture::new();
    let path = fixture.root.join(".codex/bin/build-receipt.json");
    let bytes = legacy_receipt_bytes();
    fs::write(&path, &bytes).unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let runner = Runner {
        root: &fixture.root,
        mode: "ok",
        calls: Cell::new(0),
    };
    let error = build_generated_assets(&fixture.root, &fixture.contract, &runner).unwrap_err();
    assert!(error.contains("transaction rolled back"), "{error}");
    fixture.assert_preserved();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    drop(held);
}
impl Fixture {
    fn new() -> Self {
        let root =
            test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "build-provenance").join(format!(
                "bf-provenance-{}-{}-{}",
                std::process::id(),
                FIXTURE_ID.fetch_add(1, Ordering::Relaxed),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        for base in ["templates/hooks", ".codex/hooks"] {
            fs::create_dir_all(root.join(base).join("src")).unwrap();
            fs::write(root.join(base).join("Cargo.toml"), "[workspace]\n").unwrap();
            fs::write(root.join(base).join("Cargo.lock"), "version = 4\n").unwrap();
            fs::write(root.join(base).join("src/main.rs"), "fn main() {}\n").unwrap();
        }
        fs::write(root.join("VERSION"), "1.8.0\n").unwrap();
        let initial = json!({"schema_version":4,"baseline_model": "current-only", "compatibility_baseline": "1.0.0",
            "assets":[],"generated_assets":[]});
        fixture_product::write(
            root.join("source/forge/bridgeforge-manifest.json"),
            serde_json::to_vec(&initial).unwrap(),
        )
        .unwrap();
        let contract = serde_json::from_slice(
            &bridgeforge_factory::manifest::render_managed_contract(&root).unwrap(),
        )
        .unwrap();
        // Legacy-receipt scenarios retain their old parent independently of
        // the new generated tool destinations under .deps/.
        fs::create_dir_all(root.join(".codex/bin")).unwrap();
        let fixture = Self { root, contract };
        for target in fixture.targets() {
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, b"previous installed asset").unwrap();
        }
        fixture
    }
    fn targets(&self) -> Vec<PathBuf> {
        self.contract["generated_assets"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|item| {
                let platform = if cfg!(windows) {
                    "windows-x86_64"
                } else if cfg!(target_os = "linux") {
                    "linux-x86_64"
                } else {
                    "macos-x86_64"
                };
                [
                    self.root
                        .join(item["binary_targets"][platform].as_str().unwrap()),
                    self.root.join(item["receipt_target"].as_str().unwrap()),
                ]
            })
            .collect()
    }
    fn assert_preserved(&self) {
        for path in self.targets() {
            assert_eq!(fs::read(path).unwrap(), b"previous installed asset");
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Runner<'a> {
    root: &'a Path,
    mode: &'a str,
    calls: Cell<usize>,
}
impl ProcessRunner for Runner<'_> {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        self.calls.set(self.calls.get() + 1);
        assert!(
            request
                .cwd
                .starts_with(self.root.join(".build/bridgeforge/transient"))
                && request.cwd != self.root.join(".codex/hooks"),
            "must compile the isolated project-local snapshot: {}",
            request.cwd.display()
        );
        let mut stdout = Vec::new();
        let mut code = 0;
        if request.program == "cargo" {
            assert_eq!(request.args[0], "build");
            assert_eq!(request.args[1], "--locked");
            assert_eq!(request.args[3], "release");
            assert_eq!(
                PathBuf::from(&request.args[5]),
                request.cwd.join("Cargo.toml")
            );
            assert_eq!(
                fs::read(request.cwd.join("src/main.rs"))?,
                b"fn main() {}\n"
            );
            let target = PathBuf::from(&request.args[7]).join("release");
            assert_eq!(request.args[8], "--bin");
            assert!(matches!(
                request.args[9].to_str(),
                Some("bridgeforge" | "bridgeforge-hook")
            ));
            fs::create_dir_all(&target)?;
            if self.mode == "missing-second-binary" && self.calls.get() == 3 {
                return Ok(ProcessOutput {
                    code: 0,
                    stdout,
                    stderr: Vec::new(),
                    timed_out: false,
                });
            }
            for binary in ["bridgeforge", "bridgeforge-hook"] {
                fs::write(
                    target.join(if cfg!(windows) {
                        format!("{binary}.exe")
                    } else {
                        binary.into()
                    }),
                    binary.as_bytes(),
                )?;
            }
            match self.mode {
                "legacy-receipt-drift" => fs::write(
                    self.root.join(".codex/bin/build-receipt.json"),
                    b"concurrent unknown receipt",
                )?,
                "original-source" => {
                    fs::write(self.root.join(".codex/hooks/src/main.rs"), b"changed")?
                }
                "original-lock" => {
                    fs::write(self.root.join(".codex/hooks/Cargo.lock"), b"changed")?
                }
                "snapshot-source" => fs::write(request.cwd.join("src/main.rs"), b"changed")?,
                "snapshot-lock" => fs::write(request.cwd.join("Cargo.lock"), b"changed")?,
                "build-fail" => code = 1,
                _ => {}
            }
        } else {
            let name = Path::new(&request.program)
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap();
            stdout = serde_json::to_vec(&json!({"schema":1,"name":name,"status":"ok"})).unwrap();
            if self.mode == "binary-drift" {
                fs::write(&request.program, b"changed")?;
            }
            if self.mode == "self-test-fail" {
                stdout = b"{}".to_vec();
            }
        }
        Ok(ProcessOutput {
            code,
            stdout,
            stderr: Vec::new(),
            timed_out: false,
        })
    }
}

#[test]
fn receipt_hashes_match_measured_build_inputs() {
    let fixture = Fixture::new();
    let runner = Runner {
        root: &fixture.root,
        mode: "ok",
        calls: Cell::new(0),
    };
    let receipts = build_generated_assets(&fixture.root, &fixture.contract, &runner).unwrap();
    assert_eq!(receipts.len(), 2);
    assert_eq!(runner.calls.get(), 4);
    for (receipt, item) in receipts
        .iter()
        .zip(fixture.contract["generated_assets"].as_array().unwrap())
    {
        for key in [
            "source_tree_sha256",
            "lockfile_sha256",
            "build_recipe_sha256",
            "self_test_sha256",
        ] {
            assert_eq!(receipt[key], item[key], "{key}");
        }
        let name = item["build"]["binary_name"].as_str().unwrap();
        assert_eq!(
            receipt["binary_sha256"],
            format!("sha256:{:x}", Sha256::digest(name.as_bytes()))
        );
        let stored: Value = serde_json::from_slice(
            &fs::read(fixture.root.join(item["receipt_target"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        assert_eq!(&stored, receipt);
    }
}

#[test]
fn rejects_stale_source_or_lock_before_running_cargo() {
    for relative in ["src/main.rs", "Cargo.lock"] {
        let fixture = Fixture::new();
        fs::write(fixture.root.join(".codex/hooks").join(relative), b"changed").unwrap();
        let runner = Runner {
            root: &fixture.root,
            mode: "ok",
            calls: Cell::new(0),
        };
        let error = build_generated_assets(&fixture.root, &fixture.contract, &runner).unwrap_err();
        assert!(error.contains("hash mismatch"), "{error}");
        assert_eq!(runner.calls.get(), 0);
        fixture.assert_preserved();
    }
}

#[test]
fn rejects_false_recipe_and_self_test_hashes() {
    for field in [
        "source_tree_sha256",
        "lockfile_sha256",
        "build_recipe_sha256",
        "self_test_sha256",
        "recipe",
    ] {
        let mut fixture = Fixture::new();
        if field == "recipe" {
            fixture.contract["generated_assets"][0]["build"]["args"][1] = json!("--offline");
        } else {
            fixture.contract["generated_assets"][0][field] = json!("sha256:wrong");
        }
        let runner = Runner {
            root: &fixture.root,
            mode: "ok",
            calls: Cell::new(0),
        };
        assert!(
            build_generated_assets(&fixture.root, &fixture.contract, &runner).is_err(),
            "{field}"
        );
        assert_eq!(runner.calls.get(), 0);
        fixture.assert_preserved();
    }
}

#[test]
fn drift_or_failure_never_replaces_installed_assets() {
    for mode in [
        "original-source",
        "original-lock",
        "snapshot-source",
        "snapshot-lock",
        "binary-drift",
        "build-fail",
        "self-test-fail",
    ] {
        let fixture = Fixture::new();
        let runner = Runner {
            root: &fixture.root,
            mode,
            calls: Cell::new(0),
        };
        let error = build_generated_assets(&fixture.root, &fixture.contract, &runner).unwrap_err();
        assert!(
            error.contains("changed") || error.contains("failed") || error.contains("mismatch"),
            "{mode}: {error}"
        );
        fixture.assert_preserved();
    }
}

#[test]
fn shared_workspace_does_not_skip_second_asset_contract_checks() {
    let mut fixture = Fixture::new();
    fixture.contract["generated_assets"][1]["self_test_sha256"] = json!("sha256:wrong");
    let runner = Runner {
        root: &fixture.root,
        mode: "ok",
        calls: Cell::new(0),
    };
    let error = build_generated_assets(&fixture.root, &fixture.contract, &runner).unwrap_err();
    assert!(error.contains("contract mismatch"), "{error}");
    assert_eq!(runner.calls.get(), 2);
    fixture.assert_preserved();
}

#[test]
fn successful_cargo_without_fresh_binary_cannot_reuse_another_build() {
    let fixture = Fixture::new();
    let runner = Runner {
        root: &fixture.root,
        mode: "missing-second-binary",
        calls: Cell::new(0),
    };
    let error = build_generated_assets(&fixture.root, &fixture.contract, &runner).unwrap_err();
    assert!(error.contains("cannot read built binary"), "{error}");
    assert_eq!(runner.calls.get(), 3);
    fixture.assert_preserved();
}

#[test]
fn both_binaries_share_one_isolated_dependency_target() {
    struct RecordingRunner<'a> {
        inner: Runner<'a>,
        targets: std::cell::RefCell<Vec<PathBuf>>,
    }
    impl ProcessRunner for RecordingRunner<'_> {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if request.program == "cargo" {
                self.targets
                    .borrow_mut()
                    .push(PathBuf::from(&request.args[7]));
            }
            self.inner.run(request)
        }
    }
    let fixture = Fixture::new();
    let runner = RecordingRunner {
        inner: Runner {
            root: &fixture.root,
            mode: "ok",
            calls: Cell::new(0),
        },
        targets: Default::default(),
    };
    assert_eq!(
        build_generated_assets(&fixture.root, &fixture.contract, &runner)
            .unwrap()
            .len(),
        2
    );
    let targets = runner.targets.borrow();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0], targets[1]);
    assert!(targets[0].starts_with(fixture.root.join(".build/bridgeforge/transient")));
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
