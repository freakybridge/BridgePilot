//! A repository rename preserves the provider's immutable identity. Comparing
//! URLs alone cannot distinguish a rename from replacing a registered project.
use crate::Result;
use bridgeforge_core::{ProcessRequest, ProcessRunner};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

fn github_name(url: &str) -> Result<&str> {
    let name = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("git@github.com:"))
        .ok_or("repository replacement is forbidden; only a verified GitHub rename is supported")?
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let parts: Vec<_> = name.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    {
        return Err("invalid GitHub repository identity".into());
    }
    Ok(name)
}
pub fn verify_rename(
    runner: &dyn ProcessRunner,
    cwd: &Path,
    old: &str,
    new: &str,
) -> Result<Value> {
    let before = github_name(old)?;
    let after = github_name(new)?;
    if before.split('/').next() != after.split('/').next() {
        return Err("repository ownership transfer is outside a project rename".into());
    }
    let mut observed = Vec::new();
    for name in [before, after] {
        let mut request = ProcessRequest::new("gh", cwd);
        request.args = vec![
            "api".into(),
            "--hostname".into(),
            "github.com".into(),
            format!("repos/{name}").into(),
        ];
        request.timeout = Duration::from_secs(45);
        request.env.insert("GH_PROMPT_DISABLED".into(), "1".into());
        let output = runner.run(&request).map_err(|e| e.to_string())?;
        if output.code != 0 || output.timed_out || output.stdout.len() > 1024 * 1024 {
            return Err("cannot verify the registered repository rename with GitHub; retain the old binding".into());
        }
        observed.push(bridgeforge_core::json::parse_unique_json(
            &output.stdout,
            "GitHub rename response",
        )?);
    }
    let id = observed[0]["id"]
        .as_u64()
        .ok_or("GitHub immutable repository ID missing")?;
    if id == 0
        || observed[1]["id"].as_u64() != Some(id)
        || observed
            .iter()
            .any(|v| v["full_name"].as_str() != Some(after))
    {
        return Err(
            "repository identity changed; a project rename cannot replace its repository".into(),
        );
    }
    Ok(
        json!({"previous_repository":old,"repository":new,"github_repository_id":id,"canonical_name":after,"live_verified":true}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Provider(Mutex<std::collections::VecDeque<u64>>);
    impl ProcessRunner for Provider {
        fn run(
            &self,
            request: &ProcessRequest,
        ) -> std::io::Result<bridgeforge_core::ProcessOutput> {
            assert_eq!(request.program, "gh");
            assert_eq!(request.args[0], "api");
            assert_eq!(request.args[1], "--hostname");
            assert_eq!(request.args[2], "github.com");
            let id = self
                .0
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected provider request");
            Ok(bridgeforge_core::ProcessOutput {
                code: 0,
                stdout: serde_json::to_vec(&json!({"id":id,"full_name":"owner/BridgePilot"}))
                    .unwrap(),
                stderr: Vec::new(),
                timed_out: false,
            })
        }
    }
    #[test]
    fn rename_requires_the_same_live_immutable_repository_id() {
        let provider = Provider(Mutex::new([42, 42].into()));
        let proof = verify_rename(
            &provider,
            Path::new("."),
            "https://github.com/owner/BridgeForgeCodex.git",
            "https://github.com/owner/BridgePilot.git",
        )
        .unwrap();
        assert_eq!(proof["github_repository_id"], 42);
        let different = Provider(Mutex::new([42, 99].into()));
        assert!(
            verify_rename(
                &different,
                Path::new("."),
                "https://github.com/owner/BridgeForgeCodex.git",
                "https://github.com/owner/BridgePilot.git"
            )
            .is_err()
        );
    }
    #[test]
    fn a_rename_does_not_allow_another_owner_or_an_unverified_provider() {
        let unused = Provider(Mutex::new(Default::default()));
        assert!(
            verify_rename(
                &unused,
                Path::new("."),
                "https://github.com/owner/old.git",
                "https://github.com/another/BridgePilot.git"
            )
            .is_err()
        );
        assert!(
            verify_rename(
                &unused,
                Path::new("."),
                "https://elsewhere/owner/old.git",
                "https://github.com/owner/BridgePilot.git"
            )
            .is_err()
        );
    }
}
