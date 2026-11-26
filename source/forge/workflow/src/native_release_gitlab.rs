//! GitLab tag creation, package upload and release publication.
use super::*;

const PACKAGE: &str = "bridgeforge-release";

pub(super) fn match_remote(url: &str, repository: &str, origin: &str) -> Result<(), Stop> {
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
    origin: &str,
    method: &str,
    api_path: &str,
    body: Option<Value>,
    upload: Option<&Path>,
) -> Result<(), Stop> {
    let secret = token(root, origin)?;
    let url = format!("{origin}{api_path}");
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
        &url,
        "--write-out",
        "\n%{http_code}",
    ]
    .iter()
    .map(Into::into)
    .collect();
    // Keep redirects disabled so the token stays on its configured host.
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
    let result = runner.run(&q);
    let Ok(out) = result else {
        return Err(condition(
            "GitLab request failed; remote outcome not verified",
        ));
    };
    if out.timed_out || out.code != 0 {
        // Never echo a server response or curl stderr containing credential data.
        return Err(condition(format!(
            "GitLab {method} transport outcome uncertain; remote outcome not verified"
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
    if !(200..300).contains(&status) {
        return Err(condition(format!("GitLab {method} returned HTTP {status}")));
    }
    Ok(())
}
pub(super) fn publish(
    root: &Path,
    runner: &dyn ProcessRunner,
    plan: &Plan,
    commit: &str,
    origin: &str,
) -> Result<String, Stop> {
    let project = format!("/api/v4/projects/{}", encode(&plan.repository));
    http(
        root,
        runner,
        origin,
        "POST",
        &format!("{project}/repository/tags"),
        Some(json!({"tag_name":plan.tag,"ref":commit})),
        None,
    )?;
    let mut links = Vec::new();
    for asset in &plan.assets {
        let path = plan.asset_path(root, asset).map_err(condition)?;
        let upload_path = format!(
            "{project}/packages/generic/{PACKAGE}/{}/{}",
            encode(&format!("{}-{commit}", plan.tag)),
            encode(&asset.name)
        );
        http(root, runner, origin, "PUT", &upload_path, None, Some(&path))?;
        links.push(
            json!({"name":asset.name,"url":format!("{origin}{upload_path}"),"link_type":"package"}),
        );
    }
    let body = json!({"tag_name":plan.tag,"name":plan.title,"description":plan.notes,"assets":{"links":links}});
    http(
        root,
        runner,
        origin,
        "POST",
        &format!("{project}/releases"),
        Some(body),
        None,
    )?;
    Ok(format!(
        "{}/{}/-/releases/{}",
        origin,
        plan.repository,
        encode(&plan.tag)
    ))
}
