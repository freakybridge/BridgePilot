use super::*;

pub fn explicit_release_policy(root: &Path) -> Result<bool, String> {
    let Some(payload) = release_input(&root.join(".codex/bridgeforge-version.json"))? else {
        return Ok(true);
    };
    let value = crate::json::parse_unique_json(&payload, "version sync config")?;
    setup::confirmation(&value)?;
    if matches!(value["schema_version"].as_u64(), Some(2 | 3 | 4)) {
        config::parse(&payload)?;
        return Ok(true);
    }
    if value["schema_version"].as_u64() != Some(1) {
        return Err("version sync config must use schema_version=1".into());
    }
    match value.get("release_policy") {
        // Legacy configuration remains readable, but cannot authorize a release.
        // Only the explicit CLI release flag may create version writes.
        None => Ok(true),
        Some(Value::String(policy)) if policy == "per_commit" => Ok(true),
        Some(Value::String(policy)) if policy == "explicit_release" => Ok(true),
        _ => Err("release_policy must be per_commit or explicit_release".into()),
    }
}

pub(super) fn git(
    root: &Path,
    runner: &dyn ProcessRunner,
    args: &[&str],
) -> Result<String, String> {
    let mut request = ProcessRequest::new("git", root);
    request.args = args.iter().map(std::ffi::OsString::from).collect();
    request.timeout = Duration::from_secs(45);
    let output = runner.run(&request).map_err(|e| e.to_string())?;
    if output.timed_out || output.code != 0 {
        return Err(format!(
            "release history query failed (timed_out={}): {}",
            output.timed_out,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

pub(super) fn paths(raw: &str) -> Vec<String> {
    raw.split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn version_at(
    root: &Path,
    revision: &str,
    runner: &dyn ProcessRunner,
) -> Result<Option<SemVer>, String> {
    revision_payload(root, revision, "VERSION", runner)?
        .map(|bytes| {
            std::str::from_utf8(&bytes)
                .map_err(|e| e.to_string())?
                .trim()
                .parse()
        })
        .transpose()
}

fn changelog_sections(payload: &[u8], version: SemVer) -> Result<usize, String> {
    let heading = format!("## [{version}]");
    Ok(std::str::from_utf8(payload)
        .map_err(|e| e.to_string())?
        .lines()
        .filter(|line| {
            line.strip_prefix(&heading)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(" - "))
        })
        .count())
}

fn verify_historical_manifests(
    root: &Path,
    revision: &str,
    version: SemVer,
    runner: &dyn ProcessRunner,
    initial_product_manifests: Option<&[String]>,
) -> Result<(), String> {
    let config = revision_payload(root, revision, ".codex/bridgeforge-version.json", runner)?;
    if let Some(cfg) = config
        .as_deref()
        .map(super::schema4::parse)
        .transpose()?
        .flatten()
    {
        return super::schema4::verify_historical(root, revision, &cfg, runner);
    }
    if let Some(payload) = &config {
        if let Some(cfg) = polyglot::parse(payload)? {
            return polyglot::verify_historical(root, revision, &cfg, runner);
        }
        if let Some(cfg) = super::config::parse(payload)? {
            independent::historical_workspace(root, revision, &cfg, runner)?;
            return Ok(());
        }
    }
    let mut manifests = Vec::new();
    if let Some(payload) = config {
        let config = crate::json::parse_unique_json(&payload, "release baseline version config")?;
        if config["schema_version"].as_u64() != Some(1) {
            return Err("invalid release baseline version config".into());
        }
        if super::config::auto_manifests(&config)? {
            for path in ["Cargo.toml", "package.json"] {
                if revision_payload(root, revision, path, runner)?.is_some() {
                    manifests.push(path.into());
                }
            }
        } else {
            let entries = config["manifests"]
                .as_array()
                .ok_or("release baseline manifests are missing")?;
            if entries.is_empty() {
                return Err("release baseline manifests are empty".into());
            }
            for entry in entries {
                manifests.push(
                    entry
                        .as_str()
                        .ok_or("invalid release baseline manifest path")?
                        .to_string(),
                );
            }
        }
    } else if let Some(paths) = initial_product_manifests {
        // Only first configuration may assign roles to previously unconfigured
        // native manifests. Existing historical configurations remain authoritative.
        manifests.extend_from_slice(paths);
    } else {
        for path in ["Cargo.toml", "package.json"] {
            if revision_payload(root, revision, path, runner)?.is_some() {
                manifests.push(path.into());
            }
        }
    }
    for path in manifests {
        safe_relative(root, &path)?;
        let payload = revision_payload(root, revision, &path, runner)?
            .ok_or("release baseline native manifest missing")?;
        let found = match Path::new(&path).file_name().and_then(|s| s.to_str()) {
            Some("Cargo.toml") => cargo_version_text(
                std::str::from_utf8(&payload).map_err(|e| e.to_string())?,
                &path,
            )?,
            Some("package.json") => {
                let value: Value = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                value["version"]
                    .as_str()
                    .map(str::parse::<SemVer>)
                    .transpose()?
            }
            _ => {
                return Err(format!(
                    "unsupported release baseline native manifest: {path}"
                ));
            }
        };
        if found != Some(version) {
            return Err(format!(
                "release baseline native manifest disagrees with VERSION: {path}"
            ));
        }
    }
    Ok(())
}

pub(super) fn release_boundary(
    root: &Path,
    head: &str,
    runner: &dyn ProcessRunner,
    initial_product_manifests: Option<&[String]>,
) -> Result<String, String> {
    let schema4 = super::schema4::load(root)?;
    if let Some(cfg) = &schema4 {
        if git(root, runner, &["merge-base", &cfg.baseline.revision, head])?.trim()
            != cfg.baseline.revision
            || git(
                root,
                runner,
                &["merge-base", &cfg.baseline.migration_head, head],
            )?
            .trim()
                != cfg.baseline.migration_head
        {
            return Err("schema 4 migration baseline is not an ancestor of HEAD".into());
        }
    }
    let candidates = git(
        root,
        runner,
        &[
            "log",
            "--first-parent",
            "--format=%H",
            head,
            "--",
            "VERSION",
        ],
    )?;
    for candidate in candidates.lines().filter(|s| !s.is_empty()) {
        if let Some(cfg) = &schema4 {
            if candidate == cfg.baseline.revision {
                if version_at(root, candidate, runner)?
                    .map(|v| v.to_string())
                    .as_deref()
                    != Some(cfg.baseline.product_version.as_str())
                {
                    return Err("schema 4 migration product version disagrees with history".into());
                }
                return Ok(candidate.to_string());
            }
        }
        let current =
            version_at(root, candidate, runner)?.ok_or("release history deletes VERSION")?;
        let parents = git(
            root,
            runner,
            &["rev-list", "--parents", "-n", "1", candidate],
        )?;
        let parent = parents.split_whitespace().nth(1);
        let previous = parent
            .map(|p| version_at(root, p, runner))
            .transpose()?
            .flatten();
        let Some(previous) = previous else {
            if let Some(parent) = parent {
                if !git(
                    root,
                    runner,
                    &["log", "-1", "--format=%H", parent, "--", "VERSION"],
                )?
                .trim()
                .is_empty()
                {
                    return Err("release history reintroduces VERSION after deletion".into());
                }
            }
            verify_historical_manifests(
                root,
                candidate,
                current,
                runner,
                initial_product_manifests,
            )?;
            return Ok(candidate.to_string());
        };
        if current == previous {
            continue;
        }
        if schema4.is_some() {
            let payload =
                revision_payload(root, candidate, ".codex/bridgeforge-version.json", runner)?
                    .ok_or("post-migration release has no schema 4 config")?;
            if super::schema4::parse(&payload)?.is_none() {
                return Err("post-migration release must use schema 4".into());
            }
        }
        if current < previous {
            return Err("release history decreases VERSION".into());
        }
        let current_log =
            revision_payload(root, candidate, "CHANGELOG.md", runner)?.unwrap_or_default();
        let previous_log =
            revision_payload(root, parent.unwrap(), "CHANGELOG.md", runner)?.unwrap_or_default();
        if changelog_sections(&current_log, current)? != 1
            || changelog_sections(&previous_log, current)? != 0
        {
            return Err(format!(
                "VERSION change at {candidate} has no unique newly added CHANGELOG release section"
            ));
        }
        verify_historical_manifests(root, candidate, current, runner, None)?;
        return Ok(candidate.to_string());
    }
    Err("explicit release requires a committed VERSION baseline".into())
}

pub fn validate_release_baseline(
    root: &Path,
    runner: &dyn ProcessRunner,
) -> Result<(String, String), String> {
    validate_baseline(root, runner, None, true, None)
}

pub(super) fn validate_baseline(
    root: &Path,
    runner: &dyn ProcessRunner,
    candidate: Option<&rust::RustConfig>,
    validate_workspace: bool,
    initial_product_manifests: Option<&[String]>,
) -> Result<(String, String), String> {
    validate_baseline_state(
        root,
        runner,
        candidate,
        validate_workspace,
        initial_product_manifests,
    )
    .map(|(head, boundary, _)| (head, boundary))
}

pub(super) fn validate_baseline_state(
    root: &Path,
    runner: &dyn ProcessRunner,
    candidate: Option<&rust::RustConfig>,
    validate_workspace: bool,
    initial_product_manifests: Option<&[String]>,
) -> Result<(String, String, Option<schema4::withdrawal::History>), String> {
    explicit_release_policy(root)?;
    if git(root, runner, &["rev-parse", "--is-shallow-repository"])?.trim() != "false" {
        return Err("explicit release requires complete Git history".into());
    }
    let head = git(root, runner, &["rev-parse", "HEAD"])?;
    let head = head.trim();
    let configuration = super::schema4::load(root)?;
    if !configuration.as_ref().is_some_and(|cfg| cfg.current_model)
        && schema4::withdrawal::has_current_history(root, head, runner)?
    {
        return Err(
            "committed product-packages configuration cannot be removed or downgraded".into(),
        );
    }
    let history = configuration
        .as_ref()
        .filter(|cfg| cfg.current_model)
        .map(|cfg| schema4::withdrawal::audit(root, cfg, head, runner))
        .transpose()?;
    let boundary = match &history {
        Some(history) => history.boundary.clone(),
        None => release_boundary(root, head, runner, initial_product_manifests)?,
    };
    let boundary = boundary.as_str();
    let version = fs::read_to_string(root.join("VERSION")).map_err(|e| e.to_string())?;
    let baseline = revision_payload(root, boundary, "VERSION", runner)?
        .ok_or("release baseline VERSION is missing")?;
    if version.trim()
        != std::str::from_utf8(&baseline)
            .map_err(|e| e.to_string())?
            .trim()
    {
        return Err(
            "VERSION differs from the committed release baseline; review manual version edits"
                .into(),
        );
    }
    if let Some(cfg) = super::schema4::load(root)? {
        if validate_workspace {
            super::schema4::validate_current(root, boundary, &cfg, runner)?;
        }
    } else if let Some(cfg) = candidate {
        if validate_workspace {
            independent::validate_current(root, boundary, cfg, runner)?;
        }
    } else if let Some(cfg) = polyglot::load_config(root)? {
        if validate_workspace {
            polyglot::validate_current(root, boundary, &cfg, runner)?;
        }
    } else if let Some(cfg) = config::load(root)? {
        if validate_workspace {
            independent::validate_current(root, boundary, &cfg, runner)?;
        }
    } else if validate_workspace {
        validate_native_versions(root, version.trim().parse::<SemVer>()?)?;
    }
    Ok((head.to_string(), boundary.to_string(), history))
}
