//! GitLab transport for the existing fixed-source release coordinator.
//! Packages are uploaded and verified before publishing a Release with its links.
use super::*;
use bridgeforge_core::native_release::{Asset, PublishedVersion, SourceIdentity, SourcePlan};
use std::collections::BTreeMap;

const PACKAGE: &str = "bridgeforge-release";

pub(super) fn match_remote(url: &str, repository: &str, origin: &str) -> Result<(), Stop> {
    bridgeforge_core::native_release::validate_repository(repository, Some(origin))
        .map_err(decision)?;
    let actual = url.trim_end_matches('/');
    let actual = actual.strip_suffix(".git").unwrap_or(actual);
    let authority = origin.split_once("://").unwrap().1;
    let host = authority.split(':').next().unwrap();
    if actual != format!("{origin}/{repository}")
        && actual != format!("git@{host}:{repository}")
        && actual != format!("ssh://git@{host}/{repository}")
    {
        return Err(conflict(
            "GitLab identity differs from the actual push remote",
        ));
    }
    Ok(())
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|c| {
            if c.is_ascii_alphanumeric() || b"-._~".contains(&c) {
                (c as char).to_string()
            } else {
                format!("%{c:02X}")
            }
        })
        .collect()
}

fn origin(plan: &Plan) -> Result<&str, Stop> {
    let value = plan
        .gitlab_origin
        .as_deref()
        .ok_or_else(|| decision("GitLab origin missing"))?;
    bridgeforge_core::native_release::validate_repository(&plan.repository, Some(value))
        .map_err(decision)?;
    Ok(value)
}

fn endpoint(plan: &Plan, suffix: &str) -> Result<String, Stop> {
    Ok(format!(
        "{}/api/v4/projects/{}{}",
        origin(plan)?,
        encode(&plan.repository),
        suffix
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Auth {
    origin: String,
    token: String,
}

fn token(root: &Path, expected: &str) -> Result<String, Stop> {
    // Environment credentials require explicit host scope. Project-local credentials
    // are ignored by Git and never enter source, process arguments or receipts.
    let auth = if let Ok(token) = std::env::var("GITLAB_TOKEN") {
        Auth {
            origin: std::env::var("GITLAB_ORIGIN")
                .map_err(|_| decision("GITLAB_TOKEN requires matching GITLAB_ORIGIN"))?,
            token,
        }
    } else {
        let path = bridgeforge_core::managed_paths::safe_join(
            root,
            ".runtime/config/bridgeforge-gitlab-auth.json",
            "GitLab credentials",
        )
        .map_err(condition)?;
        let bytes = bridgeforge_core::managed_paths::read_metadata(&path, 16 * 1024).map_err(condition)?
            .ok_or_else(|| condition("GitLab API authentication required: configure a host-scoped token; do not place it in source or command arguments"))?;
        serde_json::from_value::<Auth>(
            crate::json::parse_unique_json(&bytes, "GitLab credentials")
                .map_err(|_| condition("invalid GitLab credential configuration"))?,
        )
        .map_err(|_| condition("invalid GitLab credential configuration"))?
    };
    if auth.origin != expected
        || auth.token.is_empty()
        || auth.token.len() > 4096
        || auth
            .token
            .bytes()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
    {
        return Err(decision(
            "GitLab credentials are invalid or scoped to another origin",
        ));
    }
    Ok(auth.token)
}

fn quoted(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

fn http(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    method: &str,
    url: &str,
    body: Option<Value>,
    upload: Option<&Path>,
) -> Result<(u16, Vec<u8>), Stop> {
    let expected = origin(plan)?;
    if !url.starts_with(&format!(
        "{expected}/api/v4/projects/{}/",
        encode(&plan.repository)
    )) {
        return Err(conflict("GitLab request escaped the declared project"));
    }
    let secret = token(root, expected)?;
    let mut q = ProcessRequest::new(if cfg!(windows) { "curl.exe" } else { "curl" }, root);
    q.args = [
        "-q",
        "--config",
        "-",
        "--silent",
        "--show-error",
        "--connect-timeout",
        "15",
        "--max-time",
        "600",
        "--request",
        method,
        "--url",
        url,
        "--write-out",
        "\n%{http_code}",
    ]
    .iter()
    .map(Into::into)
    .collect();
    // curl does not follow redirects: a token must never be forwarded to object
    // storage or to another origin. The registry API is the verified download URL.
    let mut config = format!("header = {}\n", quoted(&format!("PRIVATE-TOKEN: {secret}")));
    if let Some(value) = body {
        config.push_str(&format!(
            "header = \"Content-Type: application/json\"\ndata-binary = {}\n",
            quoted(&value.to_string())
        ));
    }
    if let Some(path) = upload {
        q.args
            .extend(["--upload-file".into(), path.as_os_str().into()]);
    }
    q.stdin = config.into_bytes();
    q.timeout = Duration::from_secs(620);
    let attempts = if method == "GET" { 2 } else { 1 };
    for attempt in 0..attempts {
        let result = runner.run(&q);
        let Ok(out) = result else {
            if attempt + 1 < attempts {
                continue;
            }
            return Err(condition(
                "GitLab request failed; inspect the same release before retrying a write",
            ));
        };
        if out.timed_out || out.code != 0 {
            if attempt + 1 < attempts {
                continue;
            }
            // Never echo a server response or curl stderr containing credential data.
            return Err(condition(format!(
                "GitLab {method} transport outcome uncertain; resume the same release to reconcile"
            )));
        }
        let at = out
            .stdout
            .iter()
            .rposition(|c| *c == b'\n')
            .ok_or_else(|| condition("GitLab HTTP status missing"))?;
        let status: u16 = std::str::from_utf8(&out.stdout[at + 1..])
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| condition("GitLab HTTP status invalid"))?;
        if status >= 500 && attempt + 1 < attempts {
            continue;
        }
        return Ok((status, out.stdout[..at].to_vec()));
    }
    unreachable!()
}

fn api(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    method: &str,
    suffix: &str,
    body: Option<Value>,
    missing: bool,
) -> Result<Option<Value>, Stop> {
    let (status, bytes) = http(
        root,
        runner,
        plan,
        method,
        &endpoint(plan, suffix)?,
        body,
        None,
    )?;
    if missing && status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(condition(format!(
            "GitLab {method} returned HTTP {status}; no conflicting object overwritten"
        )));
    }
    crate::json::parse_unique_json(&bytes, "GitLab response")
        .map(Some)
        .map_err(|_| condition("GitLab returned invalid JSON"))
}

fn identity_plan(identity: &SourceIdentity, version: &str) -> Plan {
    Plan {
        schema: 1,
        product: identity.product.clone(),
        version: version.into(),
        repository: identity.repository.clone(),
        gitlab_origin: identity.gitlab_origin.clone(),
        tag: format!("{}{version}", identity.tag_prefix),
        title: identity.product.clone(),
        notes: String::new(),
        assets: vec![],
    }
}

fn release(root: &Path, runner: &dyn ProcessRunner, plan: &Plan) -> Result<Option<Value>, Stop> {
    api(
        root,
        runner,
        plan,
        "GET",
        &format!("/releases/{}", encode(&plan.tag)),
        None,
        true,
    )
}

fn tag_commit(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
) -> Result<Option<String>, Stop> {
    api(
        root,
        runner,
        plan,
        "GET",
        &format!("/repository/tags/{}", encode(&plan.tag)),
        None,
        true,
    )?
    .map(|value| {
        value["commit"]["id"]
            .as_str()
            .filter(|s| valid_commit(s))
            .map(str::to_owned)
            .ok_or_else(|| conflict("GitLab tag has no fixed commit"))
    })
    .transpose()
}

fn valid_commit(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|c| c.is_ascii_hexdigit())
}
fn digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}
fn package_version(plan: &Plan, commit: &str) -> String {
    format!("{}-{commit}", plan.tag)
}
fn asset_url(plan: &Plan, commit: &str, name: &str) -> Result<String, Stop> {
    endpoint(
        plan,
        &format!(
            "/packages/generic/{PACKAGE}/{}/{}",
            encode(&package_version(plan, commit)),
            encode(name)
        ),
    )
}

/// Read registry digests in bounded pages instead of downloading every artifact
/// on each release check. Missing server digests require content verification.
fn files(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
) -> Result<BTreeMap<String, Asset>, Stop> {
    let version = package_version(plan, commit);
    let mut package = None;
    let mut complete = false;
    for page in 1..=10 {
        let value = api(root, runner, plan, "GET", &format!("/packages?package_type=generic&package_name={PACKAGE}&package_version={}&per_page=100&page={page}", encode(&version)), None, false)?.unwrap();
        let rows = value
            .as_array()
            .ok_or_else(|| condition("GitLab package list invalid"))?;
        for row in rows.iter().filter(|r| {
            r["name"] == PACKAGE && r["version"] == version && r["package_type"] == "generic"
        }) {
            let id = row["id"]
                .as_u64()
                .ok_or_else(|| conflict("GitLab package ID missing"))?;
            if package.replace(id).is_some() {
                return Err(conflict(
                    "duplicate GitLab release packages; existing bytes preserved",
                ));
            }
        }
        if rows.len() < 100 {
            complete = true;
            break;
        }
    }
    if !complete {
        return Err(condition("GitLab package lookup exceeded bounded window"));
    }
    let Some(id) = package else {
        return Ok(BTreeMap::new());
    };
    let mut result = BTreeMap::new();
    for page in 1..=10 {
        let value = api(
            root,
            runner,
            plan,
            "GET",
            &format!("/packages/{id}/package_files?per_page=100&page={page}"),
            None,
            false,
        )?
        .unwrap();
        let rows = value
            .as_array()
            .ok_or_else(|| condition("GitLab package files invalid"))?;
        for row in rows {
            let name = row["file_name"]
                .as_str()
                .filter(|s| bridgeforge_core::native_release::identifier(s))
                .ok_or_else(|| conflict("invalid GitLab package filename"))?;
            let size = row["size"]
                .as_u64()
                .ok_or_else(|| conflict("GitLab asset size missing"))?;
            if size > 512 * 1024 * 1024 {
                return Err(condition("GitLab asset exceeds native verification limit"));
            }
            let sha256 = match row["file_sha256"].as_str().filter(|s| digest(s)) {
                Some(sha) => format!("sha256:{}", sha.to_ascii_lowercase()),
                None => {
                    let (status, bytes) = http(
                        root,
                        runner,
                        plan,
                        "GET",
                        &asset_url(plan, commit, name)?,
                        None,
                        None,
                    )?;
                    if status != 200 || size != bytes.len() as u64 {
                        return Err(conflict("GitLab asset download could not be verified"));
                    }
                    raw_sha(&bytes)
                }
            };
            if result
                .insert(
                    name.into(),
                    Asset {
                        name: name.into(),
                        path: format!(".build/native-release/existing/{name}"),
                        size,
                        sha256,
                    },
                )
                .is_some()
            {
                return Err(conflict(
                    "duplicate GitLab asset versions; no asset overwritten",
                ));
            }
        }
        if rows.len() < 100 {
            return Ok(result);
        }
    }
    Err(condition("GitLab asset lookup exceeded bounded window"))
}

fn verify_files(
    plan: &Plan,
    actual: &BTreeMap<String, Asset>,
    partial: bool,
) -> Result<Vec<String>, Stop> {
    for (name, item) in actual {
        let expected = plan
            .assets
            .iter()
            .find(|a| a.name == *name)
            .ok_or_else(|| conflict("unowned GitLab package asset preserved"))?;
        if item.size != expected.size || item.sha256 != expected.sha256 {
            return Err(conflict(
                "GitLab package content differs; original asset preserved",
            ));
        }
    }
    let missing: Vec<_> = plan
        .assets
        .iter()
        .filter(|a| !actual.contains_key(&a.name))
        .map(|a| a.name.clone())
        .collect();
    if !partial && !missing.is_empty() {
        return Err(condition("GitLab release assets incomplete"));
    }
    Ok(missing)
}

fn marker(plan: &Plan, commit: &str) -> Result<String, Stop> {
    let identity = raw_sha(&serde_json::to_vec(&json!({"product":plan.product,"version":plan.version,"repository":plan.repository,
        "gitlab_origin":plan.gitlab_origin,"tag":plan.tag,"assets":plan.assets.iter().map(|a|json!({"name":a.name,"sha256":a.sha256,"size":a.size})).collect::<Vec<_>>()
    })).map_err(|e| condition(e.to_string()))?);
    Ok(format!("<!-- native-release-v1 {commit} {identity} -->"))
}

fn owned(plan: &Plan, commit: &str, value: &Value) -> Result<(), Stop> {
    let expected_marker = marker(plan, commit)?;
    if value["tag_name"] != plan.tag
        || value["commit"]["id"] != commit
        || value["upcoming_release"] == true
        || !value["released_at"].is_string()
        || !value["description"]
            .as_str()
            .is_some_and(|s| s.contains(&expected_marker))
    {
        return Err(conflict(
            "GitLab release is not the owned fixed-source publication",
        ));
    }
    let links = value["assets"]["links"]
        .as_array()
        .ok_or_else(|| conflict("GitLab release links missing"))?;
    if links.len() != plan.assets.len() {
        return Err(conflict("GitLab release asset inventory differs"));
    }
    let mut seen = BTreeSet::new();
    for link in links {
        let name = link["name"]
            .as_str()
            .ok_or_else(|| conflict("GitLab asset link name missing"))?;
        if !seen.insert(name)
            || !plan.assets.iter().any(|a| a.name == name)
            || link["url"] != asset_url(plan, commit, name)?
        {
            return Err(conflict(
                "GitLab asset link is duplicated, unowned or targets another location",
            ));
        }
    }
    Ok(())
}

fn receipt(plan: &Plan, commit: &str, value: &Value, reused: usize) -> Result<Value, Stop> {
    let url = format!(
        "{}/{}/-/releases/{}",
        origin(plan)?,
        plan.repository,
        encode(&plan.tag)
    );
    if value["_links"]["self"] != url {
        return Err(conflict("GitLab release URL differs from declared project"));
    }
    Ok(
        json!({"schema":1,"operation":"native-release","status":"released","release_requested":true,"product":plan.product,
        "version":plan.version,"commit":commit,"repository":plan.repository,"platform":"gitlab","gitlab_origin":plan.gitlab_origin,
        "tag":plan.tag,"release_url":url,"published_at":value["released_at"],"assets_verified":plan.assets.len(),"assets_reused":reused,"publication_verified":true}),
    )
}

pub(super) fn baseline(
    root: &Path,
    runner: &dyn ProcessRunner,
    identity: &SourceIdentity,
) -> Result<Option<PublishedVersion>, Stop> {
    let plan = identity_plan(identity, &identity.version);
    let mut latest: Option<bridgeforge_core::SemVer> = None;
    for page in 1..=10 {
        let value = api(
            root,
            runner,
            &plan,
            "GET",
            &format!("/releases?per_page=100&page={page}"),
            None,
            false,
        )?
        .unwrap();
        let rows = value
            .as_array()
            .ok_or_else(|| condition("GitLab release listing invalid"))?;
        for row in rows
            .iter()
            .filter(|v| v["upcoming_release"] != true && v["released_at"].is_string())
        {
            let Some(raw) = row["tag_name"]
                .as_str()
                .and_then(|s| s.strip_prefix(&identity.tag_prefix))
            else {
                continue;
            };
            let version = raw.parse::<bridgeforge_core::SemVer>().map_err(decision)?;
            if latest.is_none_or(|old| version > old) {
                latest = Some(version);
            }
        }
        if rows.len() < 100 {
            return latest
                .map(|v| {
                    let version = v.to_string();
                    let commit = tag_commit(root, runner, &identity_plan(identity, &version))?
                        .ok_or_else(|| conflict("GitLab published baseline has no source tag"))?;
                    Ok(PublishedVersion { version, commit })
                })
                .transpose();
        }
    }
    Err(condition("GitLab release history exceeds bounded lookup"))
}

pub(super) fn source_available(
    root: &Path,
    runner: &dyn ProcessRunner,
    identity: &SourceIdentity,
    unchanged: bool,
) -> Result<(), Stop> {
    let plan = identity_plan(identity, &identity.version);
    if (tag_commit(root, runner, &plan)?.is_some() || release(root, runner, &plan)?.is_some())
        && !unchanged
    {
        return Err(conflict(
            "GitLab release version occupied; no source commit or push performed",
        ));
    }
    Ok(())
}

pub(super) fn existing_plan(
    root: &Path,
    runner: &dyn ProcessRunner,
    selected: &SourcePlan,
) -> Result<Plan, Stop> {
    let old = selected
        .reuse_release
        .as_ref()
        .ok_or_else(|| conflict("missing selected GitLab publication"))?;
    let mut plan = identity_plan(&selected.identity, &old.version);
    let actual = files(root, runner, &plan, &old.commit)?;
    let mut seen = BTreeSet::new();
    for name in &selected.release_assets {
        if !seen.insert(name) {
            return Err(conflict("duplicate native release asset"));
        }
        plan.assets.push(
            actual
                .get(name)
                .ok_or_else(|| conflict("GitLab published asset missing"))?
                .clone(),
        );
    }
    let value =
        release(root, runner, &plan)?.ok_or_else(|| conflict("selected GitLab release missing"))?;
    owned(&plan, &old.commit, &value)?;
    verify_files(&plan, &actual, false)?;
    if tag_commit(root, runner, &plan)?.as_deref() != Some(&old.commit) {
        return Err(conflict("GitLab source tag drifted"));
    }
    Ok(plan)
}

pub(super) fn verify_existing(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
) -> Result<Value, Stop> {
    let value =
        release(root, runner, plan)?.ok_or_else(|| condition("GitLab formal release missing"))?;
    owned(plan, commit, &value)?;
    verify_files(plan, &files(root, runner, plan, commit)?, false)?;
    if tag_commit(root, runner, plan)?.as_deref() != Some(commit) {
        return Err(conflict("GitLab source tag drifted"));
    }
    let mut value = receipt(plan, commit, &value, plan.assets.len())?;
    value["publication_reused"] = json!(true);
    Ok(value)
}

pub(super) fn obsolete_publication(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
) -> Result<bool, Stop> {
    let Some(tag) = tag_commit(root, runner, plan)? else {
        return Ok(false);
    };
    if tag == commit {
        return Ok(false);
    }
    let Some(value) = release(root, runner, plan)? else {
        return Ok(false);
    };
    if value["commit"]["id"] != tag
        || value["upcoming_release"] == true
        || !value["released_at"].is_string()
        || !value["description"].as_str().is_some_and(|s| {
            s.contains(&format!("<!-- native-release-v1 {tag} "))
                && !s.contains(&format!("<!-- native-release-v1 {commit} "))
        })
    {
        return Ok(false);
    }
    Ok(git(root, runner, &["merge-base", &tag, commit])? == tag)
}

pub(super) fn precheck(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    head: &str,
    dirty: bool,
) -> Result<(), Stop> {
    if tag_commit(root, runner, plan)?
        .as_deref()
        .is_some_and(|sha| dirty || sha != head)
    {
        return Err(conflict(
            "GitLab release tag occupied; stopped before commit/push",
        ));
    }
    if let Some(value) = release(root, runner, plan)? {
        if dirty {
            return Err(conflict(
                "GitLab existing release cannot accept uncommitted source",
            ));
        }
        owned(plan, head, &value)?;
        verify_files(plan, &files(root, runner, plan, head)?, false)?;
    }
    Ok(())
}

pub(super) fn publish(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
) -> Result<Value, Stop> {
    if !valid_commit(commit) {
        return Err(conflict("GitLab publication requires a fixed commit"));
    }
    plan.validate(root).map_err(conflict)?;
    if tag_commit(root, runner, plan)?
        .as_deref()
        .is_some_and(|actual| actual != commit)
    {
        return Err(conflict("GitLab source tag occupied"));
    }
    if release(root, runner, plan)?.is_some() {
        return verify_existing(root, runner, plan, commit);
    }
    let publication = marker(plan, commit)?;
    let journal_path = bridgeforge_core::managed_paths::safe_join(
        root,
        &format!(
            ".runtime/bridgeforge-codex/native-release/gitlab/{}.json",
            &raw_sha(publication.as_bytes())[7..]
        ),
        "GitLab write continuation",
    )
    .map_err(conflict)?;
    let mut journal_before =
        bridgeforge_core::managed_paths::read_metadata(&journal_path, 1024 * 1024)
            .map_err(condition)?;
    let mut journal = match &journal_before {
        Some(bytes) => {
            crate::json::parse_unique_json(bytes, "GitLab write continuation").map_err(conflict)?
        }
        None => {
            json!({"schema":1,"publication":publication,"uploads":{},"release_create":"not-issued"})
        }
    };
    if journal["schema"] != 1
        || journal["publication"] != publication
        || !journal["uploads"].is_object()
        || !matches!(
            journal["release_create"].as_str(),
            Some("not-issued" | "issued" | "rejected")
        )
    {
        return Err(conflict(
            "GitLab write continuation differs; preserve original state",
        ));
    }
    let save = |value: &Value, before: &mut Option<Vec<u8>>| -> Result<(), Stop> {
        if bridgeforge_core::managed_paths::read_metadata(&journal_path, 1024 * 1024)
            .map_err(condition)?
            != *before
        {
            return Err(conflict("GitLab write continuation changed concurrently"));
        }
        let bytes = serde_json::to_vec_pretty(value).map_err(|e| condition(e.to_string()))?;
        persistence::atomic_write(&journal_path, &bytes).map_err(|e| condition(e.to_string()))?;
        *before = Some(bytes);
        Ok(())
    };
    let before = files(root, runner, plan, commit)?;
    let missing = verify_files(plan, &before, true)?;
    let reused = plan.assets.len() - missing.len();
    for name in missing {
        if !journal["uploads"][&name].is_null() && journal["uploads"][&name]["state"] != "rejected"
        {
            return Err(condition(
                "GitLab previous upload outcome remains unknown or its asset disappeared; no duplicate PUT issued",
            ));
        }
        let asset = plan.assets.iter().find(|a| a.name == name).unwrap();
        let path = bridgeforge_core::managed_paths::safe_join(root, &asset.path, "GitLab artifact")
            .map_err(conflict)?;
        journal["uploads"][&name] =
            json!({"state":"issued","sha256":asset.sha256,"size":asset.size});
        save(&journal, &mut journal_before)?;
        let upload = http(
            root,
            runner,
            plan,
            "PUT",
            &asset_url(plan, commit, &name)?,
            None,
            Some(&path),
        );
        if let Ok((status @ (400 | 401 | 403 | 404 | 405 | 413 | 415 | 422), _)) = &upload {
            // A definitive rejection is persisted before any follow-up read;
            // otherwise failed authentication during that read would freeze it as unknown.
            journal["uploads"][&name]["state"] = json!("rejected");
            journal["uploads"][&name]["http_status"] = json!(status);
            save(&journal, &mut journal_before)?;
            return Err(condition(format!(
                "GitLab upload rejected with HTTP {status}; correct the condition and resume"
            )));
        }
        if !matches!(upload, Ok((200..=299, _))) {
            // A lost response may mean the immutable upload succeeded. Reconcile;
            // do not issue a second PUT or delete an ambiguous remote object.
            let current = files(root, runner, plan, commit)?;
            verify_files(plan, &current, true)?;
            if !current.contains_key(&name) {
                return Err(condition(
                    "GitLab upload rejected, incomplete or unknown; existing progress preserved",
                ));
            }
        }
    }
    verify_files(plan, &files(root, runner, plan, commit)?, false)?;
    plan.validate(root).map_err(conflict)?;
    if journal["release_create"] == "issued" {
        return Err(condition(
            "GitLab previous release creation is still unknown; no duplicate POST issued",
        ));
    }
    let links: Result<Vec<_>, Stop> = plan
        .assets
        .iter()
        .map(|a| {
            Ok(json!({"name":a.name,"url":asset_url(plan, commit, &a.name)?,"link_type":"package"}))
        })
        .collect();
    let body = json!({"tag_name":plan.tag,"ref":commit,"name":plan.title,"description":format!("{}\n\n{}", plan.notes, marker(plan, commit)?),"assets":{"links":links?}});
    journal["release_create"] = json!("issued");
    save(&journal, &mut journal_before)?;
    let created = http(
        root,
        runner,
        plan,
        "POST",
        &endpoint(plan, "/releases")?,
        Some(body),
        None,
    );
    if let Ok((status @ (400 | 401 | 403 | 404 | 405 | 413 | 415 | 422), _)) = &created {
        journal["release_create"] = json!("rejected");
        journal["release_create_http_status"] = json!(status);
        save(&journal, &mut journal_before)?;
        return Err(condition(format!(
            "GitLab release creation rejected with HTTP {status}; correct the condition and resume"
        )));
    }
    // The final read is also the recovery for a creation response lost in transit.
    let value = release(root, runner, plan)?.ok_or_else(|| {
        created
            .err()
            .unwrap_or_else(|| condition("GitLab release creation not confirmed"))
    })?;
    owned(plan, commit, &value)?;
    if tag_commit(root, runner, plan)?.as_deref() != Some(commit) {
        return Err(conflict("GitLab publication tag mismatch"));
    }
    verify_files(plan, &files(root, runner, plan, commit)?, false)?;
    receipt(plan, commit, &value, reused)
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/native_release_gitlab.rs"]
mod tests;
