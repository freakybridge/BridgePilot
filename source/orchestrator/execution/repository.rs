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
