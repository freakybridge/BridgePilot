#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

#[cfg(test)]
mod build_provenance;
#[cfg(test)]
mod high_cost_native;
#[cfg(test)]
mod hook_guards;
#[cfg(test)]
mod hook_hardening_performance;
#[cfg(test)]
mod memory_sync;
#[cfg(test)]
mod process_runtime;
#[cfg(test)]
mod project_map_performance;
#[cfg(test)]
mod runtime_flows;
#[cfg(test)]
mod security_guards;
#[cfg(test)]
mod version_governance;

#[cfg(test)]
mod tests {
    use crate::{fixture_product, test_artifacts};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDirectory(PathBuf);

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sha(payload: &[u8]) -> String {
        let normalized = if payload.contains(&0) {
            payload.to_vec()
        } else {
            String::from_utf8_lossy(payload)
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .into_bytes()
        };
        format!("sha256:{:x}", Sha256::digest(normalized))
    }

    fn temp_directory(label: &str) -> TempDirectory {
        let token = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "lib").join(format!(
            "bridgeforge-factory-{label}-{}-{token}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        TempDirectory(path)
    }

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn collect(base: &Path, relative: &Path, output: &mut BTreeMap<String, Vec<u8>>) {
        let directory = base.join(relative);
        for entry in fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if matches!(
                name.to_str(),
                Some("target" | ".git" | ".venv" | "__pycache__" | ".runtime" | ".deps" | ".build")
            ) {
                continue;
            }
            let child = relative.join(name);
            if entry.file_type().unwrap().is_dir() {
                collect(base, &child, output);
            } else {
                output.insert(
                    child.to_string_lossy().replace('\\', "/"),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }

    fn files(base: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut output = BTreeMap::new();
        collect(base, Path::new(""), &mut output);
        output
    }

    #[test]
    fn repository_contains_no_python_files() {
        let repository = root();
        let all = files(&repository);
        let python = all
            .keys()
            .filter(|path| path.ends_with(".py"))
            .collect::<Vec<_>>();
        assert!(python.is_empty(), "Python files remain: {python:?}");
    }

    #[test]
    fn rust_source_is_identical_in_public_projection_and_dogfood() {
        let template = bridgeforge_core::source_projection::render(&root(), true).unwrap();
        let dogfood = files(&root().join(".codex/hooks"));
        let changed = template
            .keys()
            .chain(dogfood.keys())
            .filter(|path| template.get(*path) != dogfood.get(*path))
            .collect::<std::collections::BTreeSet<_>>();
        assert!(
            changed.is_empty(),
            "template/dogfood drifted paths: {changed:?}"
        );
    }

    #[test]
    fn windows_subsystem_is_split_between_cli_and_hook() {
        let repository = root();
        let cli = fs::read_to_string(repository.join(
            bridgeforge_core::source_projection::physical_path(
                &repository,
                "source/cli/src/main.rs",
            ),
        ))
        .unwrap();
        let hook = fs::read_to_string(repository.join(
            bridgeforge_core::source_projection::physical_path(
                &repository,
                "source/hook/src/main.rs",
            ),
        ))
        .unwrap();
        assert!(
            !cli.contains("windows_subsystem"),
            "bridgeforge CLI must remain a console application"
        );
        assert!(
            hook.contains("windows_subsystem = \"windows\""),
            "bridgeforge Hook must remain windowless on Windows"
        );
    }

    #[test]
    fn baseline_rejects_generated_receipt_from_different_source_tree() {
        let temporary = temp_directory("receipt-provenance");
        let project = &temporary.0;
        fs::create_dir_all(project.join(".deps/bridgeforge/bin")).unwrap();
        fs::create_dir_all(project.join(".codex")).unwrap();
        fs::write(project.join(".codex/.bridgeforge_codex_version"), "1.0.0\n").unwrap();
        fs::write(project.join("managed.txt"), "managed\n").unwrap();
        let binary = b"fake bridgeforge binary";
        fs::write(
            project.join(".deps/bridgeforge/bin/bridgeforge.exe"),
            binary,
        )
        .unwrap();
        fs::write(project.join(".deps/bridgeforge/bin/bridgeforge"), binary).unwrap();

        let declared_source = format!("sha256:{}", "0".repeat(64));
        let receipt_source = format!("sha256:{}", "1".repeat(64));
        let stable_hash = format!("sha256:{}", "2".repeat(64));
        let platform = if cfg!(windows) {
            "windows-x86_64"
        } else if cfg!(target_os = "linux") {
            "linux-x86_64"
        } else {
            "macos-x86_64"
        };
        let receipt = json!({
            "schema_version": 2,
            "generated_asset_id": "codex.bridgeforge-cli",
            "platform": platform,
            "binary_sha256": sha(binary),
            "source_tree_sha256": receipt_source,
            "lockfile_sha256": stable_hash,
            "build_recipe_sha256": stable_hash,
            "self_test_sha256": stable_hash,
        });
        fs::write(
            project.join(".deps/bridgeforge/bin/build-receipt-cli.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        let contract = json!({
            "schema_version": 4,
            "release_version": "1.0.0",
            "host": "codex",
            "stamp": ".codex/.bridgeforge_codex_version",
            "contract_target": ".codex/managed-skeleton.json",
            "assets": [{
                "id": "managed.asset",
                "source": "managed.txt",
                "target": "managed.txt",
                "strategy": "whole",
                "current_sha256": sha(b"managed\n"),
            }],
            "baseline_model": "current-only", "compatibility_baseline": "1.0.0",
            "generated_assets": [{
                "id": "codex.bridgeforge-cli",
                "source_root": ".codex/hooks",
                "target_source_root": ".codex/hooks",
                "manifest": "Cargo.toml",
                "lockfile": "Cargo.lock",
                "binary_targets": {
                    "windows-x86_64": ".deps/bridgeforge/bin/bridgeforge.exe",
                    "linux-x86_64": ".deps/bridgeforge/bin/bridgeforge",
                    "macos-x86_64": ".deps/bridgeforge/bin/bridgeforge",
                },
                "receipt_target": ".deps/bridgeforge/bin/build-receipt-cli.json",
                "build": {},
                "self_test": {},
                "source_tree_sha256": declared_source,
                "lockfile_sha256": stable_hash,
                "build_recipe_sha256": stable_hash,
                "self_test_sha256": stable_hash,
            }],
        });
        fs::write(
            project.join(".codex/managed-skeleton.json"),
            serde_json::to_vec_pretty(&contract).unwrap(),
        )
        .unwrap();

        let error = bridgeforge_skeleton::baseline::verify(project, None, true).unwrap_err();
        assert!(error.contains("generated asset receipt drifted"), "{error}");
    }

    #[test]
    fn rebuilding_hook_contract_refreshes_matchers_and_handler_hashes() {
        let temporary = temp_directory("hook-contract-projection");
        let project = &temporary.0;
        for directory in ["templates/hooks", ".codex/hooks"] {
            fs::create_dir_all(project.join(directory)).unwrap();
            for name in ["Cargo.toml", "Cargo.lock"] {
                fs::write(project.join(directory).join(name), b"fixture\n").unwrap();
            }
        }
        fs::write(project.join("VERSION"), "1.0.0\n").unwrap();
        let contract = json!({
            "schema_version": 4,
            "release_version": "1.0.0",
            "baseline_model": "current-only", "compatibility_baseline": "1.0.0",
            "host": "codex",
            "stamp": ".codex/.bridgeforge_codex_version",
            "contract_target": ".codex/managed-skeleton.json",
            "generated_assets": [],
            "assets": [{
                "id": "codex.hooks-config",
                "source": "templates/hooks.json",
                "target": ".codex/hooks.json",
                "strategy": "merge",
                "current_sha256": sha(b"stale"),
                "merge_policy": "codex-hooks",
                "merge_validation": {
                    "format": "codex-hooks-current-v1",
                    "required_handlers": [],
                },
            }],
        });
        let source_contract = project.join("source/forge/bridgeforge-manifest.json");
        let active_contract = project.join(".codex/managed-skeleton.json");
        fixture_product::write(&source_contract, serde_json::to_vec(&contract).unwrap()).unwrap();
        let mut hooks = json!({"hooks":{"PreToolUse":[{
            "matcher":"Bash",
            "hooks":[{
                "bridgeforgeCodexId":"bridgeforge-codex.project-hook.v1:pre-tool",
                "type":"command",
                "command":"bridgeforge-hook pre-tool",
            }],
        }]}});
        for round in 0..3 {
            match round {
                1 => {
                    hooks["hooks"]["PreToolUse"][0]["matcher"] = json!(
                        "Bash|PowerShell|shell_command|Edit|Write|MultiEdit|NotebookEdit|apply_patch"
                    )
                }
                2 => {
                    hooks["hooks"]["PreToolUse"][0]["hooks"][0]["comment"] =
                        json!("updated managed handler")
                }
                _ => {}
            }
            let payload = serde_json::to_vec(&hooks).unwrap();
            fs::write(project.join("templates/hooks.json"), &payload).unwrap();
            fs::write(project.join(".codex/hooks.json"), &payload).unwrap();
            if round > 0 {
                let error =
                    bridgeforge_skeleton::baseline::verify(project, None, false).unwrap_err();
                let expected = if round == 1 {
                    "matcher drifted"
                } else {
                    "payload drifted"
                };
                assert!(error.contains(expected), "{error}");
            }
            let before = files(project);
            let rebuilt = bridgeforge_factory::manifest::render_managed_contract(project).unwrap();
            assert_eq!(files(project), before, "render must not write files");
            fixture_product::write(&source_contract, &rebuilt).unwrap();
            fs::write(&active_contract, &rebuilt).unwrap();
            bridgeforge_skeleton::baseline::verify(project, None, false).unwrap();
            assert_eq!(
                bridgeforge_factory::manifest::render_managed_contract(project).unwrap(),
                rebuilt,
                "projection must be deterministic"
            );
        }
        let duplicate = hooks["hooks"]["PreToolUse"][0]["hooks"][0].clone();
        hooks["hooks"]["PreToolUse"][0]["hooks"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        fs::write(
            project.join("templates/hooks.json"),
            serde_json::to_vec(&hooks).unwrap(),
        )
        .unwrap();
        let before = files(project);
        let error = bridgeforge_factory::manifest::render_managed_contract(project).unwrap_err();
        assert!(error.contains("handler is duplicated"), "{error}");
        assert_eq!(
            files(project),
            before,
            "invalid registration must not write files"
        );
    }

    #[test]
    fn managed_manifests_are_current_and_python_free() {
        let repository = root();
        assert!(!bridgeforge_factory::manifest::rebuild(&repository, true).unwrap());
        let contract = fixture_product::read_to_string(
            repository.join("source/forge/bridgeforge-manifest.json"),
        )
        .unwrap();
        assert!(!contract.contains(".py\""));
        assert!(contract.contains("codex.bridgeforge-cli"));
        assert!(contract.contains("codex.hooks"));
        assert!(contract.contains(".agents/skills/git-sync/references/first-use.md"));
    }

    #[test]
    fn precommit_and_updater_use_rust_runtime_only() {
        let repository = root();
        for relative in [
            ".githooks/pre-commit",
            "templates/.githooks/pre-commit",
            "scripts/bridgeforge_codex_shared_update.ps1",
        ] {
            let text = fs::read_to_string(bridgeforge_core::source_projection::template_path(
                &repository,
                relative,
            ))
            .unwrap()
            .to_lowercase();
            assert!(
                text.contains("bridgeforge") || text.contains("cargo"),
                "{relative}"
            );
            assert!(!text.contains(".venv"), "{relative}");
            assert!(!text.contains("python.exe"), "{relative}");
        }
        for retired in [
            "scripts/codex_memory_sync_hook.ps1",
            "scripts/codex_memory_sync_hook.cmd",
        ] {
            assert!(!repository.join(retired).exists(), "{retired}");
        }
        let precommit = fs::read_to_string(bridgeforge_core::source_projection::template_path(
            &repository,
            "templates/.githooks/pre-commit",
        ))
        .unwrap();
        assert!(precommit.contains("for CLI in .deps/bridgeforge/bin"));
        assert!(precommit.contains("build-receipt-cli.json"));
        assert!(precommit.contains("schema_version"));
        assert!(!precommit.contains("cargo build"));
        assert!(!precommit.contains("grep "));
        assert!(!precommit.contains(".runtime/bridgeforge-target"));
        assert!(!precommit.contains("--skip-generated-runtime"));
    }

    #[test]
    fn active_skills_do_not_invoke_retired_python_entries() {
        let repository = root();
        for base in [
            bridgeforge_core::source_projection::template_path(&repository, "templates/skills"),
            repository.join(".codex/skills"),
        ] {
            for (path, payload) in files(&base) {
                if !path.ends_with(".md") {
                    continue;
                }
                let text = String::from_utf8(payload).unwrap().to_lowercase();
                assert!(!text.contains("scripts/python"), "{path}");
                assert!(!text.contains(".venv\\scripts\\python"), "{path}");
                assert!(!text.contains(".venv/scripts/python"), "{path}");
                assert!(!text.contains("batch_control.py"), "{path}");
            }
        }
    }
}
#[cfg(test)]
mod distribution_regressions;

#[cfg(test)]
mod artifact_layout_runtime;

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
