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

fn api(
    root: &Path,
    runner: &dyn ProcessRunner,
    method: &str,
    endpoint: &str,
    body: Option<Value>,
    allow_missing: bool,
) -> Result<Option<Value>, Stop> {
    let mut req = ProcessRequest::new("gh", root);
    req.args = vec![
        "api".into(),
        "--hostname".into(),
        "github.com".into(),
        "--method".into(),
        method.into(),
        endpoint.into(),
        "-H".into(),
        "Accept: application/vnd.github+json".into(),
        "-H".into(),
        "X-GitHub-Api-Version: 2022-11-28".into(),
    ];
    if let Some(body) = body {
        req.args.extend(["--input".into(), "-".into()]);
        req.stdin = serde_json::to_vec(&body).map_err(|e| condition(e.to_string()))?;
    }
    req.timeout = Duration::from_secs(120);
    let mut output = runner.run(&req);
    let retry_read = method == "GET"
        && match &output {
            Err(_) => true,
            Ok(out) => {
                out.timed_out
                    || (out.code != 0 && !String::from_utf8_lossy(&out.stderr).contains("HTTP 4"))
            }
        };
    if retry_read {
        output = runner.run(&req);
    }
    let out = output.map_err(|e| condition(e.to_string()))?;
    if out.timed_out {
        return Err(condition(format!(
            "GitHub {method} outcome unknown; run git-sync release to reconcile"
        )));
    }
    if out.code != 0 {
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        if allow_missing && text.contains("HTTP 404") {
            return Ok(None);
        }
        return Err(condition(format!(
            "GitHub {method} {endpoint} failed: {text}"
        )));
    }
    let value = bridgeforge_core::json::parse_unique_json(&out.stdout, "GitHub release response")
        .map_err(condition)?;
    Ok(Some(value))
}

fn release(root: &Path, runner: &dyn ProcessRunner, plan: &Plan) -> Result<Option<Value>, Stop> {
    if let Some(published) = api(
        root,
        runner,
        "GET",
        &format!("repos/{}/releases/tags/{}", plan.repository, plan.tag),
        None,
        true,
    )? {
        return Ok(Some(published));
    }
    // A pending draft tag is not resolvable by the REST tags endpoint. This
    // bounded discovery is needed only when no published release is available.
    let mut found = None;
    for page in 1..=10 {
        let values = api(
            root,
            runner,
            "GET",
            &format!(
                "repos/{}/releases?per_page=100&page={page}",
                plan.repository
            ),
            None,
            false,
        )?
        .unwrap();
        let values = values
            .as_array()
            .ok_or_else(|| condition("GitHub release listing malformed"))?;
        for value in values.iter().filter(|v| v["tag_name"] == plan.tag) {
            if found.is_some() {
                return Err(conflict(
                    "multiple releases claim this tag; existing drafts preserved",
                ));
            }
            found = Some(value.clone());
        }
        if values.len() < 100 {
            return Ok(found);
        }
    }
    Err(condition(
        "draft lookup exceeded its bounded window; no release created",
    ))
}

fn by_id(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    value: &Value,
) -> Result<Value, Stop> {
    let id = value["id"]
        .as_u64()
        .ok_or_else(|| condition("release id missing"))?;
    api(
        root,
        runner,
        "GET",
        &format!("repos/{}/releases/{id}", plan.repository),
        None,
        false,
    )?
    .ok_or_else(|| condition("release disappeared"))
}

fn assets(value: &Value) -> Result<&Vec<Value>, Stop> {
    value["assets"]
        .as_array()
        .ok_or_else(|| condition("GitHub release asset list missing"))
}

fn verify_assets(plan: &Plan, release: &Value, allow_missing: bool) -> Result<Vec<String>, Stop> {
    let assets = assets(release)?;
    let mut seen = BTreeSet::new();
    for remote in assets {
        let name = remote["name"]
            .as_str()
            .ok_or_else(|| conflict("unnamed remote release artifact"))?;
        if !seen.insert(name) {
            return Err(conflict("duplicate remote artifact name"));
        }
        let expected = plan
            .assets
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| conflict(format!("unowned remote artifact preserved: {name}")))?;
        if remote["state"] != "uploaded"
            || remote["size"].as_u64() != Some(expected.size)
            || remote["digest"].as_str() != Some(&expected.sha256)
        {
            return Err(conflict(format!(
                "remote artifact differs or has no verifiable digest; preserved: {name}"
            )));
        }
    }
    let missing: Vec<_> = plan
        .assets
        .iter()
        .filter(|a| !seen.contains(a.name.as_str()))
        .map(|a| a.name.clone())
        .collect();
    if !allow_missing && !missing.is_empty() {
        return Err(condition(format!(
            "release artifacts incomplete: {}",
            missing.join(", ")
        )));
    }
    Ok(missing)
}

pub(super) fn tag_commit(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
) -> Result<Option<String>, Stop> {
    let Some(mut value) = api(
        root,
        runner,
        "GET",
        &format!("repos/{}/git/ref/tags/{}", plan.repository, plan.tag),
        None,
        true,
    )?
    else {
        return Ok(None);
    };
    for _ in 0..4 {
        let object = &value["object"];
        let sha = object["sha"]
            .as_str()
            .ok_or_else(|| condition("remote tag object missing"))?
            .to_owned();
        if object["type"] == "commit" {
            return Ok(Some(sha));
        }
        if object["type"] != "tag" {
            return Err(conflict("release tag does not point to a commit"));
        }
        value = api(
            root,
            runner,
            "GET",
            &format!("repos/{}/git/tags/{sha}", plan.repository),
            None,
            false,
        )?
        .unwrap();
    }
    Err(conflict("release tag nesting is unsupported"))
}

fn owned(release: &Value, plan: &Plan, commit: &str, marker: &str) -> Result<(), Stop> {
    if release["tag_name"] != plan.tag
        || release["prerelease"] != false
        || !release["body"].as_str().is_some_and(|s| s.contains(marker))
    {
        return Err(conflict(
            "existing release does not match this publication; no existing content overwritten",
        ));
    }
    if release["draft"] == true && release["target_commitish"] != commit {
        return Err(conflict(
            "existing draft targets different source; draft preserved",
        ));
    }
    Ok(())
}

fn identity_plan(identity: &bridgeforge_core::native_release::SourceIdentity, version: &str) -> Plan {
    Plan { schema: 1, product: identity.product.clone(), version: version.into(),
        repository: identity.repository.clone(), tag: format!("{}{version}", identity.tag_prefix),
        title: identity.product.clone(), notes: String::new(), assets: vec![] }
}

/// Reconstruct the original ordered asset identity, then bind it to the release's
/// publication marker. No local artifact or new build is treated as old evidence.
pub(super) fn existing_plan(root: &Path, runner: &dyn ProcessRunner,
    selected: &bridgeforge_core::native_release::SourcePlan) -> Result<Plan, Stop> {
    let published = selected.reuse_release.as_ref().ok_or_else(|| conflict("missing publication selection"))?;
    let mut plan = identity_plan(&selected.identity, &published.version);
    let remote = release(root, runner, &plan)?.ok_or_else(|| condition("selected formal release disappeared"))?;
    let remote_assets = assets(&remote)?;
    let mut seen = BTreeSet::new();
    for name in &selected.release_assets {
        if !bridgeforge_core::native_release::identifier(name) || !seen.insert(name) {
            return Err(conflict("invalid native publication asset inventory"));
        }
        let matches: Vec<_> = remote_assets.iter().filter(|asset| asset["name"] == *name).collect();
        if matches.len() != 1 { return Err(conflict("published asset inventory is incomplete or ambiguous")); }
        let asset = matches[0];
        let sha256 = asset["digest"].as_str().filter(|s| s.len() == 71 && s.starts_with("sha256:") && s[7..].bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| conflict("published asset has no verifiable digest"))?;
        plan.assets.push(bridgeforge_core::native_release::Asset { name: name.clone(),
            path: format!(".build/native-release/existing/{name}"), sha256: sha256.into(),
            size: asset["size"].as_u64().ok_or_else(|| conflict("published asset size missing"))? });
    }
    verify_existing_value(root, runner, &plan, &published.commit, &remote)?;
    Ok(plan)
}

fn verify_existing_value(root: &Path, runner: &dyn ProcessRunner, plan: &Plan, commit: &str, remote: &Value) -> Result<Value, Stop> {
    let identity = raw_sha(&serde_json::to_vec(&json!({"product":plan.product,"version":plan.version,
        "repository":plan.repository,"tag":plan.tag,"assets":plan.assets.iter().map(|a|json!({"name":a.name,"sha256":a.sha256,"size":a.size})).collect::<Vec<_>>()
    })).map_err(|e| condition(e.to_string()))?);
    owned(remote, plan, commit, &format!("<!-- native-release-v1 {commit} {identity} -->"))?;
    if remote["draft"] != false || !remote["published_at"].is_string()
        || tag_commit(root, runner, plan)?.as_deref() != Some(commit) {
        return Err(conflict("existing publication is not a verified formal release"));
    }
    verify_assets(plan, remote, false)?;
    let mut result = published_receipt(plan, commit, remote, plan.assets.len())?;
    result["publication_reused"] = json!(true);
    Ok(result)
}

pub(super) fn verify_existing(root: &Path, runner: &dyn ProcessRunner, plan: &Plan, commit: &str) -> Result<Value, Stop> {
    let remote = release(root, runner, plan)?.ok_or_else(|| condition("formal release missing; existing version preserved"))?;
    verify_existing_value(root, runner, plan, commit, &remote)
}

pub(super) fn baseline(root: &Path, runner: &dyn ProcessRunner, identity: &bridgeforge_core::native_release::SourceIdentity)
    -> Result<Option<bridgeforge_core::native_release::PublishedVersion>, Stop> {
    let mut latest: Option<bridgeforge_core::SemVer> = None;
    let mut complete = false;
    for page in 1..=10 {
        let values = api(root, runner, "GET", &format!("repos/{}/releases?per_page=100&page={page}", identity.repository), None, false)?.unwrap();
        let values = values.as_array().ok_or_else(|| condition("release listing malformed"))?;
        for release in values {
            if release["draft"] != false || release["prerelease"] != false { continue; }
            let Some(raw) = release["tag_name"].as_str().and_then(|s| s.strip_prefix(&identity.tag_prefix)) else { continue; };
            let version = raw.parse::<bridgeforge_core::SemVer>().map_err(decision)?;
            if latest.is_none_or(|old| version > old) { latest = Some(version); }
        }
        if values.len() < 100 { complete = true; break; }
    }
    if !complete { return Err(condition("release history exceeds bounded lookup; baseline unknown")); }
    latest.map(|version| {
        let version = version.to_string();
        let commit = tag_commit(root, runner, &identity_plan(identity, &version))?
            .ok_or_else(|| conflict("published baseline has no source tag"))?;
        Ok(bridgeforge_core::native_release::PublishedVersion { version, commit })
    }).transpose()
}

pub(super) fn source_available(root: &Path, runner: &dyn ProcessRunner,
    identity: &bridgeforge_core::native_release::SourceIdentity, unchanged: bool) -> Result<(), Stop> {
    let plan = identity_plan(identity, &identity.version);
    if tag_commit(root, runner, &plan)?.is_some() || release(root, runner, &plan)?.is_some() {
        if unchanged { return Ok(()); }
        return Err(conflict("proposed release version is occupied; no source commit or push performed"));
    }
    Ok(())
}

pub(super) fn obsolete_publication(root: &Path, runner: &dyn ProcessRunner, plan: &Plan, commit: &str) -> Result<bool, Stop> {
    let Some(tag) = tag_commit(root, runner, plan)? else { return Ok(false); };
    if tag == commit { return Ok(false); }
    let Some(value) = release(root, runner, plan)? else { return Ok(false); };
    if value["draft"] != false || value["prerelease"] != false || value["tag_name"] != plan.tag {
        return Ok(false);
    }
    if !value["body"].as_str().is_some_and(|body| body.contains(&format!("<!-- native-release-v1 {tag} "))
        && !body.contains(&format!("<!-- native-release-v1 {commit} "))) { return Ok(false); }
    Ok(git(root, runner, &["merge-base", &tag, commit])? == tag)
}

pub(super) fn precheck(root: &Path, runner: &dyn ProcessRunner, plan: &Plan, head: &str, dirty: bool) -> Result<(), Stop> {
    let tag = tag_commit(root, runner, plan)?;
    if tag.as_deref().is_some_and(|tag| dirty || tag != head) {
        return Err(conflict("release tag already identifies different source; stopped before commit/push"));
    }
    if let Some(value) = release(root, runner, plan)? {
        if dirty { return Err(conflict("existing release cannot accept uncommitted source; stopped before commit/push")); }
        let identity = raw_sha(&serde_json::to_vec(&json!({"product":plan.product,"version":plan.version,
            "repository":plan.repository,"tag":plan.tag,"assets":plan.assets.iter().map(|a|json!({"name":a.name,"sha256":a.sha256,"size":a.size})).collect::<Vec<_>>()
        })).map_err(|e| condition(e.to_string()))?);
        owned(&value, plan, head, &format!("<!-- native-release-v1 {head} {identity} -->"))?;
        verify_assets(plan, &value, true)?;
    }
    Ok(())
}

pub(super) fn publish(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
) -> Result<Value, Stop> {
    if std::env::var("GH_HOST")
        .ok()
        .is_some_and(|h| !h.is_empty() && !h.eq_ignore_ascii_case("github.com"))
    {
        return Err(decision(
            "GH_HOST selects another service; confirm the publication host",
        ));
    }
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(conflict("publication requires a fixed source commit"));
    }
    plan.validate(root).map_err(conflict)?;
    let identity = raw_sha(&serde_json::to_vec(&json!({"product":plan.product,"version":plan.version,
        "repository":plan.repository,"tag":plan.tag,"assets":plan.assets.iter().map(|a|json!({"name":a.name,"sha256":a.sha256,"size":a.size})).collect::<Vec<_>>()
    })).map_err(|e| condition(e.to_string()))?);
    let marker = format!("<!-- native-release-v1 {commit} {identity} -->");
    let old_tag = tag_commit(root, runner, plan)?;
    if old_tag.as_deref().is_some_and(|actual| actual != commit) {
        return Err(conflict(
            "release tag already identifies different source; tag preserved",
        ));
    }
    let observed = release(root, runner, plan)?;
    if let Some(value) = &observed {
        owned(value, plan, commit, &marker)?;
        verify_assets(plan, value, true)?;
        if value["draft"] == false && old_tag.is_none() {
            return Err(conflict(
                "published release lost its source tag; existing release preserved",
            ));
        }
    }
    if old_tag.is_none() {
        let created = api(
            root,
            runner,
            "POST",
            &format!("repos/{}/git/refs", plan.repository),
            Some(json!({"ref":format!("refs/tags/{}", plan.tag),"sha":commit})),
            false,
        );
        match created {
            Ok(Some(value))
                if value["object"]["type"] == "commit" && value["object"]["sha"] == commit => {}
            result => match tag_commit(root, runner, plan)?.as_deref() {
                Some(actual) if actual == commit => {}
                Some(_) => {
                    return Err(conflict(
                        "release tag was created for different source; no release published",
                    ));
                }
                None => {
                    return Err(result
                        .err()
                        .unwrap_or_else(|| condition("fixed source tag could not be verified")));
                }
            },
        }
    }
    let mut current = match observed {
        Some(value) => {
            owned(&value, plan, commit, &marker)?;
            value
        }
        None => {
            let body = json!({"tag_name":plan.tag,"target_commitish":commit,"name":plan.title,
                "body":format!("{}\n\n{marker}", plan.notes),"draft":true,"prerelease":false});
            // Draft is an internal upload transaction, not an additional review/approval stage.
            match api(
                root,
                runner,
                "POST",
                &format!("repos/{}/releases", plan.repository),
                Some(body),
                false,
            ) {
                Ok(Some(value)) => value,
                result => {
                    // A timeout may mean creation succeeded. Query before any later mutation.
                    match release(root, runner, plan)? {
                        Some(value) => {
                            owned(&value, plan, commit, &marker)?;
                            value
                        }
                        None => {
                            return Err(result.err().unwrap_or_else(|| {
                                condition("release creation returned no identity")
                            }));
                        }
                    }
                }
            }
        }
    };
    owned(&current, plan, commit, &marker)?;
    let mut missing = verify_assets(plan, &current, true)?;
    let reused = plan.assets.len() - missing.len();
    for attempt in 0..=1 {
        if missing.is_empty() {
            break;
        }
        if current["draft"] != true {
            return Err(conflict(
                "published release is incomplete; existing public version preserved",
            ));
        }
        let mut argv = vec![
            "gh".into(),
            "release".into(),
            "upload".into(),
            plan.tag.clone(),
            "--repo".into(),
            format!("https://github.com/{}", plan.repository),
        ];
        for name in &missing {
            let asset = plan.assets.iter().find(|a| &a.name == name).unwrap();
            let path = root.join(&asset.path);
            if path.file_name().and_then(|n| n.to_str()) != Some(&asset.name) {
                return Err(conflict(
                    "release artifact filename differs from its declared public name",
                ));
            }
            argv.push(path.to_string_lossy().into_owned());
        }
        let upload = request(root, runner, &argv, 600);
        current = by_id(root, runner, plan, &current)?;
        owned(&current, plan, commit, &marker)?;
        missing = verify_assets(plan, &current, true)?;
        if !missing.is_empty() && attempt == 1 {
            return Err(upload.err().unwrap_or_else(|| {
                condition(format!(
                    "upload incomplete after bounded recovery; resume missing assets: {}",
                    missing.join(", ")
                ))
            }));
        }
    }
    verify_assets(plan, &current, false)?;
    if current["draft"] == false && current["published_at"].as_str().is_some() {
        return published_receipt(plan, commit, &current, reused);
    }
    if current["draft"] == true {
        plan.validate(root).map_err(conflict)?;
        let id = current["id"]
            .as_u64()
            .ok_or_else(|| condition("release id missing"))?;
        // A publication timeout is reconciled by the final read, never by blind republishing.
        let _publication = api(
            root,
            runner,
            "PATCH",
            &format!("repos/{}/releases/{id}", plan.repository),
            Some(json!({"draft":false,"make_latest":"true"})),
            false,
        );
    }
    let verified = by_id(root, runner, plan, &current)?;
    owned(&verified, plan, commit, &marker)?;
    verify_assets(plan, &verified, false)?;
    if verified["draft"] != false || verified["published_at"].as_str().is_none() {
        return Err(condition(
            "release is not publicly published; resume the same release",
        ));
    }
    if tag_commit(root, runner, plan)?.as_deref() != Some(commit) {
        return Err(conflict(
            "published release tag does not match the fixed source commit",
        ));
    }
    published_receipt(plan, commit, &verified, reused)
}

fn published_receipt(
    plan: &Plan,
    commit: &str,
    verified: &Value,
    reused: usize,
) -> Result<Value, Stop> {
    let expected = format!(
        "https://github.com/{}/releases/tag/{}",
        plan.repository, plan.tag
    );
    if verified["id"].as_u64().is_none() || verified["html_url"] != expected {
        return Err(condition("published release identity or URL is invalid"));
    }
    Ok(
        json!({"schema":1,"operation":"native-release","status":"released","release_requested":true,
        "product":plan.product,"version":plan.version,"commit":commit,"repository":plan.repository,"tag":plan.tag,
        "release_id":verified["id"],"release_url":verified["html_url"],"published_at":verified["published_at"],
        "assets_verified":plan.assets.len(),"assets_reused":reused,"publication_verified":true}),
    )
}
