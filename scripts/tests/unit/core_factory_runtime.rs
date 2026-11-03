#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

#[path = "../support/factory_root.rs"]
mod factory_repository;

use super::*;
use crate::{ProcessOutput, ProcessRequest};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct Factory(PathBuf);

#[test]
#[ignore = "explicit real Cargo performance experiment; isolated candidate, no publication"]
fn release_cache_benchmark() {
    let label = std::env::var("BFC_BENCH_LABEL").unwrap_or_else(|_| "candidate".into());
    benchmark_fixture(Factory::new(), false, &label);
}

#[test]
#[ignore = "explicit controlled real Cargo comparison; two frozen identical fixtures"]
fn release_cache_controlled_comparison() {
    let baseline = Factory::new();
    let candidate = Factory::new();
    assert_eq!(
        crate::source_projection::render(&baseline.0, false).unwrap(),
        crate::source_projection::render(&candidate.0, false).unwrap()
    );
    eprintln!("controlled comparison: both candidate fixtures frozen and byte-identical");
    let before = benchmark_fixture(baseline, true, "controlled-reference");
    let after = benchmark_fixture(candidate, false, "controlled-persistent");
    for (old, new) in before.iter().zip(&after) {
        assert_eq!(old["phase"], new["phase"]);
        assert_eq!(
            old["inputs"], new["inputs"],
            "comparison candidates must have identical build identities"
        );
    }
    assert_eq!(after[1]["metrics"]["cargo_build_invocations"], 0);
    assert_eq!(after[1]["metrics"]["cache_hits"], 3);
}

fn benchmark_fixture(fixture: Factory, reference: bool, label: &str) -> Vec<Value> {
    let root = &fixture.0;
    let repository = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"));
    let mut git = ProcessRequest::new("git", root);
    git.args = vec!["init".into(), "-q".into()];
    assert_eq!(crate::SystemProcessRunner.run(&git).unwrap().code, 0);
    assert!(
        label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    );
    let destination = repository
        .join(".runtime/tests/release-cache")
        .join(format!("{label}.json"));
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    let mut results = Vec::new();
    let mut candidate = BTreeMap::new();
    for phase in ["cold", "same", "version", "module"] {
        if phase == "version" {
            let mut cargo = fs::read_to_string(root.join(crate::source_projection::physical_path(root, "source/cli/Cargo.toml")))
                .unwrap()
                .parse::<toml_edit::DocumentMut>()
                .unwrap();
            let old = cargo["package"]["version"]
                .as_str()
                .unwrap()
                .parse::<bridgeforge_core::SemVer>()
                .unwrap();
            let next = format!("{}.{}.{}", old.major, old.minor, old.patch + 1);
            cargo["package"]["version"] = toml_edit::value(&next);
            let mut lock = fs::read_to_string(root.join("Cargo.lock"))
                .unwrap()
                .parse::<toml_edit::DocumentMut>()
                .unwrap();
            for package in lock["package"].as_array_of_tables_mut().unwrap().iter_mut() {
                if matches!(
                    package["name"].as_str(),
                    Some("bridgeforge-cli" | "bridgeforge-hook")
                ) {
                    package["version"] = toml_edit::value(&next);
                }
            }
            let hook_path = root.join(crate::source_projection::physical_path(root, "source/hook/Cargo.toml"));
            let mut hook = fs::read_to_string(&hook_path).unwrap().parse::<toml_edit::DocumentMut>().unwrap();
            hook["package"]["version"] = toml_edit::value(&next);
            candidate.insert(hook_path, hook.to_string().into_bytes());
            candidate.insert(root.join(crate::source_projection::physical_path(root, "source/cli/Cargo.toml")), cargo.to_string().into_bytes());
            candidate.insert(root.join("Cargo.lock"), lock.to_string().into_bytes());
        }
        if phase == "module" {
            let path = root.join(crate::source_projection::physical_path(root, "source/core/src/lib.rs"));
            let mut bytes = fs::read(&path).unwrap();
            bytes.extend_from_slice(b"\n// Isolated performance candidate: one module changes.\n");
            candidate.insert(path, bytes);
        }
        let observed = crate::build_metrics::Observed {
            runner: &crate::SystemProcessRunner,
            metrics: Default::default(),
        };
        let started = std::time::Instant::now();
        let run = || {
            crate::git_sync_plan::WritePlan::prepare(
                root,
                candidate.clone(),
                BTreeMap::new(),
                true,
                &observed,
            )
        };
        let plan = if reference {
            crate::git_sync_plan::with_legacy_reference(run)
        } else {
            run()
        }
        .unwrap();
        let inputs = plan
            .writes
            .iter()
            .filter(|(path, _)| {
                path.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with("build-receipt-"))
            })
            .map(|(_, bytes)| {
                let receipt: Value = serde_json::from_slice(bytes).unwrap();
                let mut identity = receipt.as_object().unwrap().clone();
                identity.remove("binary_sha256");
                Value::Object(identity)
            })
            .collect::<Vec<_>>();
        results.push(json!({"phase":phase,"inputs":inputs,"elapsed_ms":started.elapsed().as_millis(),"reused":plan.generated_reused,"built":plan.generated_built,"metrics":observed.metrics.into_inner()}));
        fs::write(
            &destination,
            serde_json::to_vec_pretty(&json!({"label":label,"results":results})).unwrap(),
        )
        .unwrap();
        eprintln!("benchmark {label}/{phase}: {}", results.last().unwrap());
    }
    results
}

#[test]
#[ignore = "child process proves strict inspection needs neither a temp directory nor Git"]
fn strict_inspection_child() {
    struct NoProcesses;
    impl ProcessRunner for NoProcesses {
        fn run(&self, _: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            panic!("strict inspection launched a process");
        }
    }
    let root = PathBuf::from(std::env::var_os("BFC_STRICT_FACTORY_ROOT").unwrap());
    let result = prepare(&root, true, &NoProcesses);
    assert_eq!(result.code, 0, "{}", result.stderr);
}

#[test]
fn strict_inspection_succeeds_with_no_temp_parent_and_no_git_on_path() {
    let fixture = Factory::new();
    let runner = Runner::new(&fixture.0, "ok");
    let built = prepare(&fixture.0, false, &runner);
    assert_eq!(built.code, 0, "{}", built.stderr);
    let before = tree(&fixture.0);
    let missing = fixture.0.join("strict-temp-must-not-exist");
    let mut request = ProcessRequest::new(
        std::env::current_exe().unwrap().into_os_string(),
        &fixture.0,
    );
    request.args = [
        "--exact",
        "factory_runtime::tests::strict_inspection_child",
        "--ignored",
        "--nocapture",
    ]
    .iter()
    .map(Into::into)
    .collect();
    request.env.insert(
        "BFC_STRICT_FACTORY_ROOT".into(),
        fixture.0.clone().into_os_string(),
    );
    request
        .env
        .insert("TEMP".into(), missing.clone().into_os_string());
    request
        .env
        .insert("TMP".into(), missing.clone().into_os_string());
    request.env.insert("PATH".into(), "".into());
    let result = crate::SystemProcessRunner.run(&request).unwrap();
    assert_eq!(
        result.code,
        0,
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!missing.exists());
    assert_eq!(tree(&fixture.0), before);
}

#[test]
fn mixed_product_release_materializer_updates_consumer_lock_without_bumping_harness() {
    let fixture = Factory::new();
    let root = &fixture.0;
    let repository = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"));
    fs::create_dir_all(root.join("scripts/tests")).unwrap();
    for name in ["Cargo.toml", "Cargo.lock"] {
        fs::copy(
            repository.join("scripts/tests").join(name),
            root.join("scripts/tests").join(name),
        )
        .unwrap();
    }
    let before = tree(root);
    let mut cargo = fs::read_to_string(root.join(crate::source_projection::physical_path(root, "source/cli/Cargo.toml")))
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    // Always exercise an actual product-version change, including after the
    // factory itself has released the version used by an earlier fixture.
    let current = cargo["package"]["version"]
        .as_str()
        .unwrap()
        .parse::<bridgeforge_core::SemVer>()
        .unwrap();
    let next = format!("{}.{}.{}", current.major, current.minor, current.patch + 1);
    cargo["package"]["version"] = toml_edit::value(next.as_str());
    let mut lock = fs::read_to_string(root.join("Cargo.lock"))
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    for package in lock["package"].as_array_of_tables_mut().unwrap().iter_mut() {
        if matches!(
            package["name"].as_str(),
            Some("bridgeforge-cli" | "bridgeforge-hook")
        ) {
            package["version"] = toml_edit::value(next.as_str());
        }
    }
    let writes = BTreeMap::from([
        (root.join(crate::source_projection::physical_path(root, "source/hook/Cargo.toml")), {
            let mut hook = fs::read_to_string(root.join(crate::source_projection::physical_path(root, "source/hook/Cargo.toml"))).unwrap().parse::<toml_edit::DocumentMut>().unwrap();
            hook["package"]["version"] = toml_edit::value(&next);
            hook.to_string().into_bytes()
        }),
        (root.join(crate::source_projection::physical_path(root, "source/cli/Cargo.toml")), cargo.to_string().into_bytes()),
        (root.join("Cargo.lock"), lock.to_string().into_bytes()),
    ]);
    struct CandidateRunner(Runner);
    impl ProcessRunner for CandidateRunner {
        fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
            if request.program == "git" && request.args.first().is_some_and(|a| a == "check-ignore")
            {
                // This fixture exercises candidate materialization, with no
                // persistent cache policy in its placeholder Git directory.
                return Ok(ProcessOutput {
                    code: 1,
                    stdout: vec![],
                    stderr: vec![],
                    timed_out: false,
                });
            }
            if Path::new(&request.program)
                .file_stem()
                .and_then(|s| s.to_str())
                == Some("bridgeforge-factory")
            {
                let cargo = fs::read_to_string(request.cwd.join("source/cli/Cargo.toml"))?
                    .parse::<toml_edit::DocumentMut>()
                    .unwrap();
                let factory = fs::read_to_string(request.cwd.join("source/factory/Cargo.toml"))?
                    .parse::<toml_edit::DocumentMut>()
                    .unwrap();
                return Ok(ProcessOutput{code:0,stdout:serde_json::to_vec(&json!({"schema":1,"name":"bridgeforge-factory","status":"ok",
                    "version":factory["package"]["version"].as_str().unwrap(),"product_version":cargo["package"]["version"].as_str().unwrap()})).unwrap(),stderr:vec![],timed_out:false});
            }
            self.0.run(request)
        }
    }
    let runner = CandidateRunner(Runner::new(root, "ok"));
    let plan =
        crate::git_sync_plan::WritePlan::prepare(root, writes, BTreeMap::new(), true, &runner)
            .unwrap();
    let consumer = &plan.writes[&root.join("scripts/tests/Cargo.lock")];
    let parsed = std::str::from_utf8(consumer)
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    let packages = parsed["package"].as_array_of_tables().unwrap();
    let version = |name: &str| {
        packages
            .iter()
            .find(|p| p["name"].as_str() == Some(name))
            .unwrap()["version"]
            .as_str()
            .unwrap()
    };
    assert_eq!(version("bridgeforge-hook"), next);
    let original = fs::read_to_string(root.join("scripts/tests/Cargo.lock"))
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    for package in original["package"]
        .as_array_of_tables()
        .unwrap()
        .iter()
        .filter(|p| p.get("source").is_none() && p["name"].as_str() != Some("bridgeforge-hook"))
    {
        assert_eq!(
            version(package["name"].as_str().unwrap()),
            package["version"].as_str().unwrap()
        );
    }
    assert!(
        !plan
            .writes
            .contains_key(&root.join("scripts/tests/Cargo.toml"))
    );
    assert_eq!(
        tree(root),
        before,
        "candidate preparation must not install release state"
    );
    fs::write(
        root.join("scripts/tests/Cargo.lock"),
        b"external consumer lock",
    )
    .unwrap();
    assert!(
        plan.verify_unchanged(root)
            .unwrap_err()
            .contains("input changed concurrently")
    );
    assert_eq!(
        fs::read(root.join("scripts/tests/Cargo.lock")).unwrap(),
        b"external consumer lock"
    );
}
impl Factory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let repository = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"));
        let root = repository.join(".runtime/tests/fr").join(format!(
            "{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(root.join(".git")).unwrap();
        let copy = |source: &str, target: &str| {
            let destination = root.join(target);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(repository.join(source), destination).unwrap();
        };
        for path in [
            "VERSION",
            "CHANGELOG.md",
            "AGENTS.md",
            "source/forge/bridgeforge-manifest.json",
            "Cargo.toml",
            "Cargo.lock",
            "templates/hooks/.gitignore",
            crate::user_agents::SOURCE,
        ] {
            let source = crate::source_projection::template_relative(repository, path);
            copy(&source, &source);
        }
        let contract: Value = serde_json::from_slice(
            &fixture_product::read(repository.join("source/forge/bridgeforge-manifest.json")).unwrap(),
        )
        .unwrap();
        for asset in contract["assets"].as_array().unwrap() {
            if let Some(bytes) = bridgeforge_skeleton::content::inline_source(asset).unwrap() {
                let target = root.join(asset["target"].as_str().unwrap());
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                fs::write(target, bytes).unwrap();
                continue;
            }
            let source = asset["source"].as_str().unwrap();
            let target = asset["target"].as_str().unwrap();
            copy(source, source);
            if target != "AGENTS.md" && asset.get("render").is_none() {
                copy(source, target);
            }
        }
        // Copy all nine canonical packages, including private factory sources.
        // The public mirror must come from the pure eight-package projection.
        for relative in crate::source_projection::render(repository, false)
            .unwrap()
            .keys()
        {
            if relative.starts_with("source/") {
                let source = crate::source_projection::physical_path(repository, relative);
                copy(&source, &source);
            }
        }
        for (relative, bytes) in crate::source_projection::render(&root, true).unwrap() {
            let target = root.join(".codex/hooks").join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, bytes).unwrap();
        }
        let distribution: Value = serde_json::from_slice(
            &fixture_product::read_shared(root.join("source/forge/bridgeforge-manifest.json")).unwrap(),
        )
        .unwrap();
        for platform in distribution["platforms"].as_object().unwrap().values() {
            for skill in platform["skills"].as_array().unwrap() {
                for file in skill["files"].as_array().unwrap() {
                    let source = file["source"].as_str().unwrap();
                    copy(source, source);
                }
            }
        }
        // Fixture bootstrap installs an internally consistent candidate, without
        // claiming the public manifest command can activate missing executables.
        for (path, bytes) in manifest::render_targets(&root).unwrap() {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        identity(&root).unwrap();
        Self(root)
    }
}
impl Drop for Factory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Runner {
    root: PathBuf,
    calls: AtomicUsize,
    builds: AtomicUsize,
    failure: &'static str,
}
impl Runner {
    fn new(root: &Path, failure: &'static str) -> Self {
        Self {
            root: root.canonicalize().unwrap(),
            calls: AtomicUsize::new(0),
            builds: AtomicUsize::new(0),
            failure,
        }
    }
}
impl ProcessRunner for Runner {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let executable = Path::new(&request.program)
            .file_stem()
            .unwrap()
            .to_string_lossy();
        let args: Vec<_> = request
            .args
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        let mut out = ProcessOutput {
            code: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            timed_out: false,
        };
        if executable == "git" {
            assert!(args.iter().any(|s| s == "--git-common-dir"));
            out.stdout = self.root.join(".git").to_string_lossy().as_bytes().to_vec();
        } else if executable == "cargo" {
            self.builds.fetch_add(1, Ordering::Relaxed);
            if self.failure == "build" {
                out.code = 1;
                return Ok(out);
            }
            let value = |flag: &str| args[args.iter().position(|s| s == flag).unwrap() + 1].clone();
            assert!(args.contains(&"--locked".into()));
            let name = value("--bin");
            let target =
                PathBuf::from(value("--target-dir"))
                    .join("release")
                    .join(if cfg!(windows) {
                        format!("{name}.exe")
                    } else {
                        name.clone()
                    });
            fs::create_dir_all(target.parent().unwrap())?;
            fs::write(target, format!("verified-fixture-{name}"))?;
            if self.failure == "source" {
                fs::write(
                    self.root.join(crate::source_projection::physical_path(&self.root, "source/hook/src/main.rs")),
                    b"concurrent change",
                )?;
            }
            if self.failure == "target" {
                fs::create_dir_all(self.root.join(".build/bridgeforge/bin"))?;
                fs::write(
                    self.root
                        .join(".build/bridgeforge/bin/build-receipt-cli.json"),
                    b"external change",
                )?;
            }
            if self.failure == "legacy-new" {
                fs::create_dir_all(self.root.join(".codex/bin"))?;
                fs::write(self.root.join(".codex/bin/unknown.txt"), b"external")?;
            }
        } else {
            let contract: Value = serde_json::from_slice(&manifest::render_managed_contract(&self.root).unwrap()).unwrap();
            let runtime = manifest::runtime_contract(&self.root, &contract).unwrap();
            let asset = runtime["generated_assets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["build"]["binary_name"] == executable.as_ref())
                .unwrap();
            out.stdout = serde_json::to_vec(&asset["self_test"]["expected_json"]).unwrap();
            if self.failure == "installed-test"
                && Path::new(&request.program).starts_with(self.root.join(".build/bridgeforge/bin"))
            {
                out.code = 1;
            }
        }
        Ok(out)
    }
}

fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            (
                e.path().strip_prefix(root).unwrap().to_path_buf(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn factory_prepare_checks_without_writes_and_reuses_healthy_cli_and_hook() {
    let fixture = Factory::new();
    let runner = Runner::new(&fixture.0, "");
    let original = tree(&fixture.0);
    assert_eq!(prepare(&fixture.0, true, &runner).code, 2);
    assert_eq!(runner.calls.load(Ordering::Relaxed), 0);
    assert_eq!(tree(&fixture.0), original);
    let result = prepare(&fixture.0, false, &runner);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(runner.builds.load(Ordering::Relaxed), 3);
    let installed = tree(&fixture.0);
    let contract = identity(&fixture.0).unwrap().0;
    let times: Vec<_> = contract["generated_assets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|asset| {
            let (binary, receipt) = paths(&fixture.0, asset).unwrap();
            [binary, receipt]
        })
        .map(|path| {
            (
                path.clone(),
                fs::metadata(path).unwrap().modified().unwrap(),
            )
        })
        .collect();
    let calls = runner.calls.load(Ordering::Relaxed);
    assert_eq!(prepare(&fixture.0, true, &runner).code, 0);
    assert_eq!(runner.calls.load(Ordering::Relaxed), calls);
    assert_eq!(prepare(&fixture.0, false, &runner).code, 0);
    assert_eq!(runner.builds.load(Ordering::Relaxed), 3);
    assert_eq!(tree(&fixture.0), installed);
    for (path, time) in times {
        assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), time);
    }
    fs::write(
        fixture
            .0
            .join(".build/bridgeforge/bin/build-receipt-cli.json"),
        b"invalid",
    )
    .unwrap();
    assert_eq!(prepare(&fixture.0, false, &runner).code, 0);
    assert_eq!(runner.builds.load(Ordering::Relaxed), 4);
}

#[test]
fn factory_prepare_stops_on_manifest_mirror_source_build_or_target_drift() {
    for failure in [
        "manifest",
        "build",
        "source",
        "target",
        "installed-test",
    ] {
        let fixture = Factory::new();
        let runner = Runner::new(&fixture.0, failure);
        if failure == "manifest" {
            fs::write(
                fixture.0.join("source/forge/bridgeforge-manifest.json"),
                b"invalid contract",
            )
            .unwrap();
        }
        let result = prepare(&fixture.0, false, &runner);
        assert_eq!(result.code, 2, "{failure}: {}", result.stderr);
        assert!(
            !fixture
                .0
                .join(".build/bridgeforge/bin/bridgeforge.exe")
                .exists()
        );
        assert!(
            !fixture
                .0
                .join(".build/bridgeforge/bin/bridgeforge")
                .exists()
        );
        assert!(
            !fixture
                .0
                .join(".build/bridgeforge/bin/bridgeforge-factory.exe")
                .exists()
        );
        assert!(
            !fixture
                .0
                .join(".build/bridgeforge/bin/bridgeforge-factory")
                .exists()
        );
        if failure == "target" {
            assert_eq!(
                fs::read(
                    fixture
                        .0
                        .join(".build/bridgeforge/bin/build-receipt-cli.json")
                )
                .unwrap(),
                b"external change"
            );
        }
        if failure == "manifest" {
            assert_eq!(runner.calls.load(Ordering::Relaxed), 0);
        }
    }
}

#[test]
fn factory_prepare_preserves_unowned_legacy_files_created_during_build() {
    let fixture = Factory::new();
    let runner = Runner::new(&fixture.0, "legacy-new");
    let result = prepare(&fixture.0, false, &runner);
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(result.stderr.contains("retired"));
    assert_eq!(
        fs::read(fixture.0.join(".codex/bin/unknown.txt")).unwrap(),
        b"external"
    );
    assert_eq!(prepare(&fixture.0, true, &runner).code, 2);
    assert!(!fixture.0.join(".build/bridgeforge/bin/bridgeforge.exe").exists());
    assert_eq!(
        fs::read(fixture.0.join(".codex/bin/unknown.txt")).unwrap(),
        b"external"
    );
}

#[test]
fn factory_prepare_repairs_declared_mirror_only_after_zero_write_check_rejects_drift() {
    let fixture = Factory::new();
    let runner = Runner::new(&fixture.0, "");
    let source = crate::source_projection::render(&fixture.0, false).unwrap();
    let mirror = fixture.0.join(".codex/hooks/source/hook/src/main.rs");
    let expected = manifest::render_targets(&fixture.0).unwrap().into_iter().find(|(p,_)| p == &mirror).unwrap().1;
    fs::write(&mirror, b"stale declared mirror").unwrap();
    let before = tree(&fixture.0);
    assert_eq!(prepare(&fixture.0, true, &runner).code, 2);
    assert_eq!(runner.calls.load(Ordering::Relaxed), 0);
    assert_eq!(tree(&fixture.0), before);
    let result = prepare(&fixture.0, false, &runner);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(fs::read(&mirror).unwrap(), expected);
    assert_eq!(crate::source_projection::render(&fixture.0, false).unwrap(), source);
    assert_eq!(prepare(&fixture.0, true, &runner).code, 0);
}

#[test]
fn legacy_factory_tools_retire_only_after_verified_install() {
    let fixture = Factory::new();
    let root = &fixture.0;
    let old = root.join(".codex/bin");
    fs::create_dir_all(&old).unwrap();
    let profile = json!({"source_tree_sha256":"retired","lockfile_sha256":"retired","build_recipe_sha256":"retired","self_test_sha256":"retired"});
    use sha2::{Digest, Sha256};
    let payload = b"known legacy binary fixture";
    let record = json!({"schema_version":2,"generated_asset_id":"codex.bridgeforge-cli",
        "platform":crate::build_receipt::platform_key().unwrap(),"binary_sha256":format!("sha256:{:x}", Sha256::digest(payload)),
        "source_tree_sha256":profile["source_tree_sha256"],"lockfile_sha256":profile["lockfile_sha256"],
        "build_recipe_sha256":profile["build_recipe_sha256"],"self_test_sha256":profile["self_test_sha256"]});
    fs::write(
        old.join(format!("bridgeforge{}", std::env::consts::EXE_SUFFIX)),
        payload,
    )
    .unwrap();
    fs::write(
        old.join("build-receipt-cli.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    fs::write(old.join(".gitignore"), b"*\n!.gitignore\n").unwrap();
    let before = tree(root);
    let failed = prepare(root, false, &Runner::new(root, "installed-test"));
    assert_ne!(failed.code, 0, "{}", failed.stderr);
    assert!(crate::project_sync::legacy_artifact_retirements(root).unwrap_err().contains("fingerprint migration is retired"));
    for (path, bytes) in &before { assert_eq!(&fs::read(root.join(path)).unwrap(), bytes); }
    let result = prepare(root, false, &Runner::new(root, "ok"));
    assert_ne!(result.code, 0);
    assert!(result.stderr.contains("fingerprint migration is retired"));
    for (path, bytes) in &before { assert_eq!(&fs::read(root.join(path)).unwrap(), bytes); }
}

#[test]
fn retirement_failure_restores_old_image_and_preserves_external_file() {
    let repository = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"));
    let root = repository
        .join(".runtime/tests/retirement")
        .join(std::process::id().to_string());
    fs::create_dir_all(&root).unwrap();
    let old = root.join("a-image.exe");
    let later = root.join("z-receipt.json");
    let new = root.join("new.bin");
    fs::write(&old, b"old executable").unwrap();
    fs::write(&later, b"old receipt").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&old, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let retired = BTreeMap::from([
        (old.clone(), b"old executable".to_vec()),
        (later.clone(), b"old receipt".to_vec()),
    ]);
    let snapshots = BTreeMap::from([
        (old.clone(), Some(b"old executable".to_vec())),
        (later.clone(), Some(b"old receipt".to_vec())),
        (new.clone(), None),
    ]);
    let outcome = project_sync::install_generated_transaction_retiring(
        &root,
        &BTreeMap::from([(new.clone(), b"new".to_vec())]),
        &snapshots,
        &BTreeSet::from([old.clone()]),
        &retired,
        || {
            fs::write(&later, b"external edit").unwrap();
            Ok(())
        },
    );
    assert!(outcome.unwrap_err().contains("rolled back"));
    assert_eq!(fs::read(&old).unwrap(), b"old executable");
    assert_eq!(fs::read(&later).unwrap(), b"external edit");
    assert!(!new.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(fs::metadata(&old).unwrap().permissions().mode() & 0o111, 0);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn generated_install_rolls_back_own_changes_and_preserves_external_changes() {
    let fixture = Factory::new();
    let a = fixture.0.join("first.bin");
    let b = fixture.0.join("second.json");
    fs::write(&a, b"old").unwrap();
    let before = BTreeMap::from([(a.clone(), Some(b"old".to_vec())), (b.clone(), None)]);
    let writes = BTreeMap::from([
        (a.clone(), b"new".to_vec()),
        (b.clone(), b"receipt".to_vec()),
    ]);
    let result = project_sync::install_generated_transaction(
        &fixture.0,
        &writes,
        &before,
        &BTreeSet::new(),
        || Err("verification failure".into()),
    );
    assert!(result.unwrap_err().contains("rolled back"));
    assert_eq!(fs::read(&a).unwrap(), b"old");
    assert!(!b.exists());
    let result = project_sync::install_generated_transaction(
        &fixture.0,
        &writes,
        &before,
        &BTreeSet::new(),
        || {
            fs::write(&a, b"external").unwrap();
            Err("concurrent".into())
        },
    );
    assert!(result.unwrap_err().contains("external change preserved"));
    assert_eq!(fs::read(&a).unwrap(), b"external");
    assert!(!b.exists());
}

#[test]
fn retired_legacy_binary_migration_rolls_back_and_preserves_private_files() {
    let root = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"))
        .join(".runtime/tests/generated-ignore-retirement")
        .join(std::process::id().to_string());
    let ignore = root.join(".codex/bin/.gitignore");
    let private = root.join(".codex/bin/project_late.exe");
    let installed = root.join("public.bin");
    fs::create_dir_all(ignore.parent().unwrap()).unwrap();
    let policy = b"*\n!.gitignore\n".to_vec();
    fs::write(&ignore, &policy).unwrap();
    let retired = BTreeMap::from([(ignore.clone(), policy.clone())]);
    let snapshots = BTreeMap::from([
        (ignore.clone(), Some(policy.clone())),
        (installed.clone(), None),
    ]);
    let result = project_sync::install_generated_transaction_retiring(
        &root,
        &BTreeMap::from([(installed.clone(), b"new public file".to_vec())]),
        &snapshots,
        &BTreeSet::new(),
        &retired,
        || {
            fs::write(&private, b"late independent program").unwrap();
            Ok(())
        },
    );
    let error = result.unwrap_err();
    assert!(error.contains("rolled back"), "{error}");
    assert!(error.contains("automatic legacy binary fingerprint migration is retired"), "{error}");
    assert_eq!(fs::read(ignore).unwrap(), policy);
    assert_eq!(fs::read(private).unwrap(), b"late independent program");
    assert!(!installed.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn retirement_preserves_original_when_copied_target_changes_during_verification() {
    let repository = factory_repository::from_manifest_dir(env!("CARGO_MANIFEST_DIR"));
    let root = repository
        .join(".runtime/tests/relocation-drift")
        .join(std::process::id().to_string());
    fs::create_dir_all(&root).unwrap();
    let old = root.join("old.bin");
    let new = root.join("new.bin");
    fs::write(&old, b"original").unwrap();
    let retired = BTreeMap::from([(old.clone(), b"original".to_vec())]);
    let snapshots = BTreeMap::from([
        (old.clone(), Some(b"original".to_vec())),
        (new.clone(), None),
    ]);
    let outcome = project_sync::install_generated_transaction_retiring(
        &root,
        &BTreeMap::from([(new.clone(), b"original".to_vec())]),
        &snapshots,
        &BTreeSet::new(),
        &retired,
        || {
            fs::write(&new, b"external").unwrap();
            Ok(())
        },
    );
    assert!(outcome.unwrap_err().contains("installed payload changed"));
    assert_eq!(fs::read(old).unwrap(), b"original");
    assert_eq!(fs::read(new).unwrap(), b"external");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn factory_prepare_serializes_with_sync_and_detects_binary_lock_and_recipe_drift() {
    let fixture = Factory::new();
    let runner = Runner::new(&fixture.0, "");
    let lock = crate::git_sync::lock_factory_runtime(&fixture.0, &runner).unwrap();
    let result = prepare(&fixture.0, false, &runner);
    assert_eq!(result.code, 2);
    assert!(result.stderr.contains("lock"));
    assert_eq!(runner.builds.load(Ordering::Relaxed), 0);
    drop(lock);
    assert_eq!(prepare(&fixture.0, false, &runner).code, 0);
    let contract = identity(&fixture.0).unwrap().0;
    let (binary, _) = paths(&fixture.0, &contract["generated_assets"][0]).unwrap();
    fs::write(&binary, b"corrupt executable").unwrap();
    assert_eq!(prepare(&fixture.0, true, &runner).code, 2);
    assert_eq!(prepare(&fixture.0, false, &runner).code, 0);
    assert_eq!(runner.builds.load(Ordering::Relaxed), 4);
    let path = fixture.0.join("Cargo.lock");
    let mut bytes = b"# changed lock input\n".to_vec();
    bytes.extend_from_slice(&fs::read(&path).unwrap());
    fs::write(path, bytes).unwrap();
    for (relative, bytes) in crate::source_projection::render(&fixture.0, true).unwrap() {
        fs::write(fixture.0.join(".codex/hooks").join(relative), bytes).unwrap();
    }
    assert_eq!(
        prepare(&fixture.0, true, &runner).code,
        2,
        "zero-write inspection must reject stale derived files"
    );
    assert_eq!(prepare(&fixture.0, false, &runner).code, 0);
    assert_eq!(runner.builds.load(Ordering::Relaxed), 7);
    let (_, path) = paths(&fixture.0, &contract["generated_assets"][0]).unwrap();
    let mut altered: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    altered["build_recipe_sha256"] = json!("sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
    fs::write(path, serde_json::to_vec(&altered).unwrap()).unwrap();
    let calls = runner.calls.load(Ordering::Relaxed);
    assert_eq!(prepare(&fixture.0, true, &runner).code, 2);
    assert_eq!(runner.calls.load(Ordering::Relaxed), calls);
    assert_eq!(prepare(&fixture.0, false, &runner).code, 0);
    assert_eq!(runner.builds.load(Ordering::Relaxed), 8);
}

#[cfg(windows)]
#[test]
fn generated_install_locked_target_restores_earlier_receipt() {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Factory::new();
    let receipt = fixture.0.join("a-receipt.json");
    let binary = fixture.0.join("z-image.exe");
    fs::write(&receipt, b"old receipt").unwrap();
    fs::write(&binary, b"old binary").unwrap();
    let before = BTreeMap::from([
        (receipt.clone(), Some(b"old receipt".to_vec())),
        (binary.clone(), Some(b"old binary".to_vec())),
    ]);
    let writes = BTreeMap::from([
        (receipt.clone(), b"new receipt".to_vec()),
        (binary.clone(), b"new binary".to_vec()),
    ]);
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&binary)
        .unwrap();
    let result = project_sync::install_generated_transaction(
        &fixture.0,
        &writes,
        &before,
        &BTreeSet::from([binary.clone()]),
        || Ok(()),
    );
    assert!(result.is_err());
    assert_eq!(fs::read(&receipt).unwrap(), b"old receipt");
    assert_eq!(fs::read(&binary).unwrap(), b"old binary");
    drop(held);
}
