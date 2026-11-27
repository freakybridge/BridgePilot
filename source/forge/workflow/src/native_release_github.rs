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

fn get(root: &Path, runner: &dyn ProcessRunner, endpoint: &str) -> Result<Option<Value>, Stop> {
    retry_read(|| {
        let mut q = ProcessRequest::new("gh", root);
        q.args = [
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            "--include",
            endpoint,
        ]
        .iter()
        .map(Into::into)
        .collect();
        let out = runner.run(&q).map_err(|e| condition(e.to_string()))?;
        if out.timed_out {
            return Err(condition("GitHub read timed out"));
        }
        let response = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        let status = response
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("HTTP/"))
            .and_then(|line| line.split_whitespace().nth(1));
        if status == Some("404") {
            return Ok(None);
        }
        if out.code != 0 || status != Some("200") {
            return Err(condition(format!(
                "GitHub read {endpoint} HTTP {}: {}",
                status.unwrap_or("unknown"),
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        let (_, body) = response
            .split_once("\n\n")
            .ok_or_else(|| condition("GitHub response has no body"))?;
        serde_json::from_str(body)
            .map(Some)
            .map_err(|e| condition(e.to_string()))
    })
}

fn tag_exists(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
) -> Result<bool, Stop> {
    let prefix = format!("repos/{}/git", plan.repository);
    let Some(tag) = get(root, runner, &format!("{prefix}/ref/tags/{}", plan.tag))? else {
        return Ok(false);
    };
    if tag["ref"].as_str() != Some(&format!("refs/tags/{}", plan.tag)) {
        return Err(conflict("unexpected GitHub tag ref"));
    }
    let mut object = tag["object"].clone();
    while object["type"] == "tag" {
        let oid = object["sha"]
            .as_str()
            .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| conflict("invalid annotated tag object"))?;
        let tag = get(root, runner, &format!("{prefix}/tags/{oid}"))?
            .ok_or_else(|| conflict("tag object missing"))?;
        object = tag["object"].clone();
    }
    if object["type"] != "commit" || object["sha"] != commit {
        return Err(conflict(format!(
            "tag {} already points to a different object",
            plan.tag
        )));
    }
    Ok(true)
}

fn missing_assets(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
) -> Result<Option<Vec<PathBuf>>, Stop> {
    let endpoint = format!("repos/{}/releases/tags/{}", plan.repository, plan.tag);
    let Some(page) = get(root, runner, &endpoint)? else {
        return Ok(None);
    };
    let body = page["body"]
        .as_str()
        .unwrap_or_default()
        .replace("\r\n", "\n");
    let assets = page["assets"]
        .as_array()
        .ok_or_else(|| condition("GitHub release assets missing"))?;
    if page["tag_name"] != plan.tag
        || page["name"] != plan.title
        || page["draft"] != false
        || page["prerelease"] != false
        || body.trim_end() != plan.notes.replace("\r\n", "\n").trim_end()
        || assets.iter().any(|a| {
            !plan
                .assets
                .iter()
                .any(|expected| a["name"] == expected.name)
        })
    {
        return Err(conflict(
            "existing GitHub Release differs from published title, notes, state or assets",
        ));
    }
    let mut missing = Vec::new();
    for expected in &plan.assets {
        let path = plan.asset_path(root, expected).map_err(condition)?;
        let Some(asset) = assets.iter().find(|a| a["name"] == expected.name) else {
            missing.push(path);
            continue;
        };
        let digest = bridgeforge_core::native_release::raw_sha(
            &fs::read(path).map_err(|e| condition(e.to_string()))?,
        );
        if asset["state"] != "uploaded" || asset["digest"] != digest {
            return Err(conflict(format!(
                "cannot verify existing Release asset {}",
                expected.name
            )));
        }
    }
    Ok(Some(missing))
}

pub(super) fn check_existing(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
) -> Result<(), Stop> {
    if tag_exists(root, runner, plan, commit)? {
        missing_assets(root, runner, plan)?;
    }
    Ok(())
}

fn complete_page(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    repo: &str,
) -> Result<bool, Stop> {
    if let Some(missing) = missing_assets(root, runner, plan)? {
        if missing.is_empty() {
            return Ok(true);
        }
        let mut upload = ProcessRequest::new("gh", root);
        upload.args = ["release", "upload", &plan.tag, "--repo", repo, "--"]
            .iter()
            .map(Into::into)
            .collect();
        upload
            .args
            .extend(missing.into_iter().map(PathBuf::into_os_string));
        upload.timeout = Duration::from_secs(600);
        let result = runner.run(&upload).map_err(|e| condition(e.to_string()))?;
        if !missing_assets(root, runner, plan)?.is_some_and(|paths| paths.is_empty()) {
            return Err(condition(format!(
                "Release assets still incomplete: {}",
                String::from_utf8_lossy(&result.stderr)
            )));
        }
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn publish(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
) -> Result<(String, bool), Stop> {
    let repo = format!("https://github.com/{}", plan.repository);
    let url = format!("{repo}/releases/tag/{}", plan.tag);
    if !tag_exists(root, runner, plan, commit)? {
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
        req.stdin =
            serde_json::to_vec(&json!({"ref":format!("refs/tags/{}",plan.tag),"sha":commit}))
                .map_err(|e| condition(e.to_string()))?;
        for attempt in 0..2 {
            let out = runner.run(&req).map_err(|e| condition(e.to_string()))?;
            if (!out.timed_out && out.code == 0) || tag_exists(root, runner, plan, commit)? {
                break;
            }
            let reason = String::from_utf8_lossy(&out.stderr);
            if attempt == 0 && (out.timed_out || transient(&reason)) {
                continue;
            }
            return Err(condition(format!("GitHub tag creation failed: {reason}")));
        }
    }
    if complete_page(root, runner, plan, &repo)? {
        return Ok((url, true));
    }
    let mut req = ProcessRequest::new("gh", root);
    req.args = [
        "release",
        "create",
        &plan.tag,
        "--verify-tag",
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
    let mut retry = true;
    loop {
        let out = runner.run(&req).map_err(|e| condition(e.to_string()))?;
        if !out.timed_out && out.code == 0 {
            return Ok((url, false));
        }
        if !tag_exists(root, runner, plan, commit)? {
            return Err(conflict("GitHub tag disappeared during publication"));
        }
        if complete_page(root, runner, plan, &repo)? {
            return Ok((url, true));
        }
        let reason = String::from_utf8_lossy(&out.stderr);
        if !retry || (!out.timed_out && !transient(&reason)) {
            return Err(condition(format!(
                "GitHub Release creation failed: {reason}"
            )));
        }
        retry = false;
    }
}
