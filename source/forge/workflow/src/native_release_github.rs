//! GitHub publication of a native project's explicit artifacts. No project-specific package rules.
use super::*;

pub(super) fn match_remote(url: &str, repository: &str) -> Result<(), Stop> {
    let normalized = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let expected = [
        format!("https://github.com/{repository}"),
        format!("git@github.com:{repository}"),
        format!("ssh://git@github.com/{repository}"),
    ];
    if !expected.iter().any(|s| s.eq_ignore_ascii_case(normalized)) {
        return Err(conflict(
            "release repository differs from the actual Git push remote",
        ));
    }
    Ok(())
}

pub(super) fn publish(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
) -> Result<String, Stop> {
    let repo = format!("https://github.com/{}", plan.repository);
    let mut req = ProcessRequest::new("gh", root);
    req.args = [
        "api",
        "--hostname",
        "github.com",
        "--method",
        "POST",
        &format!("repos/{}/git/refs", plan.repository),
        "--input",
        "-",
    ]
    .iter()
    .map(Into::into)
    .collect();
    req.stdin = serde_json::to_vec(&json!({"ref":format!("refs/tags/{}",plan.tag),"sha":commit}))
        .map_err(|e| condition(e.to_string()))?;
    let tag = runner.run(&req).map_err(|e| condition(e.to_string()))?;
    if tag.code != 0 || tag.timed_out {
        return Err(condition(format!(
            "GitHub tag creation failed: {}",
            String::from_utf8_lossy(&tag.stderr)
        )));
    }
    let mut req = ProcessRequest::new("gh", root);
    req.args = [
        "release",
        "create",
        &plan.tag,
        "--repo",
        &repo,
        "--title",
        &plan.title,
        "--notes-file",
        "-",
    ]
    .iter()
    .map(Into::into)
    .collect();
    for asset in &plan.assets {
        req.args.push(
            plan.asset_path(root, asset)
                .map_err(condition)?
                .into_os_string(),
        );
    }
    req.stdin = plan.notes.as_bytes().to_vec();
    req.timeout = Duration::from_secs(600);
    let out = runner.run(&req).map_err(|e| condition(e.to_string()))?;
    if out.code != 0 || out.timed_out {
        return Err(condition(format!(
            "GitHub release creation failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().into())
}
