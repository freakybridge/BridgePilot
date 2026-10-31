//! A withdrawal restores content, never the availability of published versions.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::release) struct Declaration {
    pub rollback: String,
    pub restored_release: String,
    pub withdrawn_release: String,
    pub reason: String,
    #[serde(default)]
    pub retained_documentation: Vec<String>,
}

#[derive(Clone, Debug)]
pub(in crate::release) struct History {
    pub boundary: String,
    pub product_high_water: SemVer,
    pub package_high_water: BTreeMap<String, NativePackage>,
}

pub(super) fn declarations(value: &Value) -> Result<Vec<Declaration>, String> {
    let declarations: Vec<Declaration> = value
        .get("release_withdrawals")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| format!("invalid release_withdrawals: {e}"))?
        .unwrap_or_default();
    let mut seen = BTreeSet::new();
    for declaration in &declarations {
        if declaration.reason.trim().is_empty() || !seen.insert(&declaration.rollback) {
            return Err("withdrawals require unique rollback commits and a reason".into());
        }
        for revision in [
            &declaration.rollback,
            &declaration.restored_release,
            &declaration.withdrawn_release,
        ] {
            if !matches!(revision.len(), 40 | 64)
                || !revision.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("withdrawals require immutable Git commit identifiers".into());
            }
        }
        let mut paths = BTreeSet::new();
        for path in &declaration.retained_documentation {
            safe_relative(Path::new("."), path)?;
            if !(path == "CHANGELOG.md" || path.starts_with("doc/") && path.ends_with(".md"))
                || !paths.insert(path)
            {
                return Err("withdrawal retained_documentation requires unique exact Markdown documentation paths".into());
            }
        }
    }
    Ok(declarations)
}

fn preserve(prior: &[Declaration], next: &[Declaration]) -> Result<(), String> {
    if prior.iter().any(|entry| !next.contains(entry)) {
        return Err("committed release withdrawal declarations must be preserved verbatim".into());
    }
    Ok(())
}

pub(in crate::release) fn has_current_history(
    root: &Path,
    head: &str,
    runner: &dyn ProcessRunner,
) -> Result<bool, String> {
    require_unmodified_history(root, runner)?;
    let revisions = history::git(
        root,
        runner,
        &["log", "--first-parent", "--format=%H", head, "--", CONFIG],
    )?;
    let queries: Vec<_> = revisions
        .lines()
        .map(|revision| format!("{revision}:{CONFIG}"))
        .collect();
    if queries.is_empty() {
        return Ok(false);
    }
    for bytes in blobs(root, &queries, runner)?.into_iter().flatten() {
        let value = crate::json::parse_unique_json(&bytes, "historical release configuration")?;
        if value["release_model"] == "product-packages" {
            return Ok(true);
        }
    }
    Ok(false)
}

fn require_unmodified_history(root: &Path, runner: &dyn ProcessRunner) -> Result<(), String> {
    if !history_cache::state(root, runner)?.1 {
        return Err(
            "withdrawal proof requires Git history without replacement refs or grafts".into(),
        );
    }
    Ok(())
}

// One Git process for the small historical configuration/version stream. Native
// package snapshots are reconstructed only for actual withdrawal proofs.
fn blobs(
    root: &Path,
    queries: &[String],
    runner: &dyn ProcessRunner,
) -> Result<Vec<Option<Vec<u8>>>, String> {
    let mut request = ProcessRequest::new("git", root);
    request.args = ["cat-file", "--batch"].map(Into::into).to_vec();
    request.stdin = format!("{}\n", queries.join("\n")).into_bytes();
    request.timeout = Duration::from_secs(45);
    let output = runner.run(&request).map_err(|e| e.to_string())?;
    if output.code != 0 || output.timed_out {
        return Err("cannot read withdrawal history blobs".into());
    }
    let mut bytes = output.stdout.as_slice();
    let mut result = Vec::new();
    for query in queries {
        let end = bytes
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("truncated withdrawal batch header")?;
        let header = std::str::from_utf8(&bytes[..end]).map_err(|e| e.to_string())?;
        bytes = &bytes[end + 1..];
        if header == format!("{query} missing") {
            result.push(None);
            continue;
        }
        let fields: Vec<_> = header.split_whitespace().collect();
        if fields.len() != 3 || fields[1] != "blob" {
            return Err("invalid withdrawal batch object".into());
        }
        let count: usize = fields[2]
            .parse()
            .map_err(|_| "invalid withdrawal blob length")?;
        if count > 16 * 1024 * 1024 || bytes.len() <= count || bytes[count] != b'\n' {
            return Err("oversized or truncated withdrawal blob".into());
        }
        result.push(Some(bytes[..count].to_vec()));
        bytes = &bytes[count + 1..];
    }
    if !bytes.is_empty() {
        return Err("unexpected withdrawal batch output".into());
    }
    Ok(result)
}

fn version(bytes: Option<&Vec<u8>>) -> Result<SemVer, String> {
    std::str::from_utf8(bytes.ok_or("release history deletes VERSION")?)
        .map_err(|e| e.to_string())?
        .trim()
        .parse()
}

fn section(bytes: Option<&Vec<u8>>, version: SemVer) -> Result<usize, String> {
    let heading = format!("## [{version}]");
    Ok(
        std::str::from_utf8(bytes.map(Vec::as_slice).unwrap_or_default())
            .map_err(|e| e.to_string())?
            .lines()
            .filter(|line| {
                line.strip_prefix(&heading)
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with(" - "))
            })
            .count(),
    )
}

fn restoration(
    root: &Path,
    declaration: &Declaration,
    runner: &dyn ProcessRunner,
) -> Result<BTreeMap<String, NativePackage>, String> {
    let raw = history::git(
        root,
        runner,
        &[
            "diff",
            "--raw",
            "--no-renames",
            "--no-ext-diff",
            "--ignore-submodules=none",
            "-z",
            &declaration.restored_release,
            &declaration.rollback,
            "--",
        ],
    )?;
    let fields: Vec<_> = raw.split('\0').filter(|s| !s.is_empty()).collect();
    if fields.len() % 2 != 0 {
        return Err("malformed withdrawal tree diff".into());
    }
    let mut changed = BTreeSet::new();
    for pair in fields.chunks_exact(2) {
        let metadata: Vec<_> = pair[0].trim_start_matches(':').split_whitespace().collect();
        if metadata.len() != 5
            || !metadata[..2]
                .iter()
                .all(|mode| matches!(*mode, "100644" | "000000"))
            || !declaration
                .retained_documentation
                .iter()
                .any(|path| path == pair[1])
        {
            return Err(format!(
                "withdrawal did not restore product content/mode: {}",
                pair[1]
            ));
        }
        changed.insert(pair[1].to_owned());
    }
    if changed != declaration.retained_documentation.iter().cloned().collect() {
        return Err(
            "withdrawal retained documentation does not match the immutable tree diff".into(),
        );
    }
    let restored = revision_payload(root, &declaration.restored_release, CONFIG, runner)?
        .ok_or("restored release has no configuration")?;
    let withdrawn = revision_payload(root, &declaration.withdrawn_release, CONFIG, runner)?
        .ok_or("withdrawn release has no configuration")?;
    let restored = parse(&restored)?.ok_or("withdrawals require schema 4 releases")?;
    let withdrawn = parse(&withdrawn)?.ok_or("withdrawals require schema 4 releases")?;
    for (revision, cfg) in [
        (&declaration.restored_release, &restored),
        (&declaration.withdrawn_release, &withdrawn),
    ] {
        let snapshot = polyglot::historical(root, revision, &cfg.native, runner)?;
        for path in &declaration.retained_documentation {
            let covered = |input: &str| {
                path.eq_ignore_ascii_case(input.trim_end_matches('/'))
                    || path.to_ascii_lowercase().starts_with(&format!(
                        "{}/",
                        input.trim_end_matches('/').to_ascii_lowercase()
                    ))
            };
            if cfg
                .native
                .product_inputs
                .iter()
                .chain(&cfg.build_inputs)
                .chain(&cfg.product_manifests)
                .any(|p| covered(p))
                || cfg.native.shared_inputs.iter().any(|p| covered(&p.path))
                || cfg
                    .development
                    .values()
                    .any(|p| p.covers_durable_input(path))
                || snapshot.inputs.keys().any(|p| covered(p))
                || snapshot
                    .packages
                    .values()
                    .any(|p| p.source_roots.iter().any(|p| covered(p)))
            {
                return Err(format!(
                    "withdrawal retained documentation is a product/package input: {path}"
                ));
            }
        }
    }
    for (left, right) in [
        (
            serde_json::to_value(&restored.native.packages),
            serde_json::to_value(&withdrawn.native.packages),
        ),
        (
            serde_json::to_value(&restored.native.lifecycle),
            serde_json::to_value(&withdrawn.native.lifecycle),
        ),
        (
            serde_json::to_value(&restored.package_adoptions),
            serde_json::to_value(&withdrawn.package_adoptions),
        ),
    ] {
        if left.map_err(|e| e.to_string())? != right.map_err(|e| e.to_string())? {
            return Err(
                "withdrawals across package identity/lifecycle changes are not supported".into(),
            );
        }
    }
    let old_ledger = development::Ledger::parse(
        revision_payload(
            root,
            &declaration.restored_release,
            development::RECORD,
            runner,
        )?
        .as_deref(),
    )?;
    development::Ledger::parse(
        revision_payload(
            root,
            &declaration.withdrawn_release,
            development::RECORD,
            runner,
        )?
        .as_deref(),
    )?
    .extends(&old_ledger)?;
    // The whole-tree comparison above also binds the rollback ledger and native
    // configuration byte-for-byte to the restored release. No synthetic entries.
    polyglot::historical(
        root,
        &declaration.withdrawn_release,
        &withdrawn.native,
        runner,
    )
    .map(|snapshot| snapshot.packages)
}

pub(in crate::release) fn audit(
    root: &Path,
    cfg: &Config,
    head: &str,
    runner: &dyn ProcessRunner,
) -> Result<History, String> {
    require_unmodified_history(root, runner)?;
    let chain = history::git(
        root,
        runner,
        &["rev-list", "--first-parent", "--reverse", head],
    )?;
    let chain: Vec<_> = chain.lines().collect();
    let start = chain
        .iter()
        .position(|revision| *revision == cfg.baseline.revision)
        .ok_or("release baseline is not on the first-parent chain")?;
    let migration = chain
        .iter()
        .position(|revision| *revision == cfg.baseline.migration_head)
        .ok_or("release migration is not on the first-parent chain")?;
    if migration < start {
        return Err("release migration precedes its baseline".into());
    }
    let changed = history::git(
        root,
        runner,
        &[
            "log",
            "--first-parent",
            "--format=%H",
            &format!("{}..{head}", cfg.baseline.revision),
            "--",
            "VERSION",
            CONFIG,
        ],
    )?;
    let changed: BTreeSet<_> = changed.lines().collect();
    let revisions: Vec<_> = chain
        .iter()
        .enumerate()
        .skip(start)
        .filter(|(i, revision)| *i == start || *i == migration || changed.contains(**revision))
        .map(|(i, revision)| (i, (*revision).to_owned()))
        .collect();
    let queries: Vec<_> = revisions
        .iter()
        .flat_map(|(_, revision)| {
            [
                format!("{revision}:VERSION"),
                format!("{revision}:{CONFIG}"),
                format!("{revision}:CHANGELOG.md"),
                format!("{revision}^:CHANGELOG.md"),
            ]
        })
        .collect();
    let payloads = blobs(root, &queries, runner)?;
    let mut prior = Vec::new();
    let mut previous = version(payloads[0].as_ref())?;
    if previous.to_string() != cfg.baseline.product_version {
        return Err("migration product version disagrees with history".into());
    }
    let mut result = History {
        boundary: cfg.baseline.revision.clone(),
        product_high_water: previous,
        package_high_water: BTreeMap::new(),
    };
    let mut formal = BTreeMap::from([(cfg.baseline.revision.clone(), previous)]);
    let mut latest_formal = cfg.baseline.revision.clone();
    let mut used = BTreeSet::new();
    let mut managed = false;
    for ((index, revision), payload) in revisions.iter().zip(payloads.chunks_exact(4)) {
        let configuration = match payload[1].as_deref() {
            Some(bytes) => {
                crate::json::parse_unique_json(bytes, "historical withdrawal configuration")?
            }
            None if *index <= migration && !managed => Value::Null,
            None => return Err("release history removes configuration".into()),
        };
        if *index > migration && configuration["schema_version"] != 4 {
            return Err("post-migration withdrawal history must preserve schema 4".into());
        }
        let current_model = configuration["release_model"] == "product-packages";
        if managed && !current_model {
            return Err("release history must preserve product-packages model".into());
        }
        managed |= current_model;
        let recorded = declarations(&configuration)?;
        preserve(&prior, &recorded)?;
        preserve(&recorded, &cfg.release_withdrawals)?;
        prior = recorded;
        let current = version(payload[0].as_ref())?;
        if revision == &cfg.baseline.revision {
            continue;
        }
        if current < previous {
            let declaration = cfg.release_withdrawals.iter().find(|record| &record.rollback == revision)
                .ok_or("release history decreases VERSION; an explicit verified withdrawal is required")?;
            if declaration.withdrawn_release != latest_formal
                || formal.get(&latest_formal) != Some(&previous)
                || formal.get(&declaration.restored_release) != Some(&current)
            {
                return Err("withdrawal must restore an earlier formal release and withdraw the latest active formal release".into());
            }
            for release in [
                &declaration.restored_release,
                &declaration.withdrawn_release,
            ] {
                if history::release_boundary(root, release, runner, None)? != *release {
                    return Err("withdrawal reference is not a verified formal release".into());
                }
            }
            for (id, package) in restoration(root, declaration, runner)? {
                if let Some(prior) = result.package_high_water.get(&id) {
                    require_identity(prior, &package)?;
                    if !greater(&package, &package.version, &prior.version)? {
                        continue;
                    }
                }
                result.package_high_water.insert(id, package);
            }
            used.insert(revision.clone());
            result.boundary = revision.clone();
        } else if current > previous {
            if current <= result.product_high_water {
                return Err(
                    "release history reuses or decreases a published product version".into(),
                );
            }
            if section(payload[2].as_ref(), current)? != 1
                || section(payload[3].as_ref(), current)? != 0
            {
                return Err(
                    "VERSION increase has no unique newly added CHANGELOG release section".into(),
                );
            }
            formal.insert(revision.clone(), current);
            latest_formal = revision.clone();
            result.boundary = revision.clone();
            result.product_high_water = current;
        }
        previous = current;
    }
    if cfg
        .release_withdrawals
        .iter()
        .any(|record| !used.contains(&record.rollback))
    {
        return Err(
            "withdrawal declaration does not identify a decrease on this first-parent history"
                .into(),
        );
    }
    if !used.contains(&result.boundary)
        && history::release_boundary(root, head, runner, None)? != result.boundary
    {
        return Err("latest release boundary is not a verified formal release".into());
    }
    Ok(result)
}

fn require_identity(previous: &NativePackage, current: &NativePackage) -> Result<(), String> {
    // Stable ids and legal move/rename transitions are verified by lineage.
    // Version ordering survives such moves, but cannot cross language schemes.
    if previous.language != current.language {
        return Err("withdrawn package version floor cannot cross language schemes".into());
    }
    Ok(())
}

pub(super) fn greater(
    package: &NativePackage,
    version: &str,
    previous: &str,
) -> Result<bool, String> {
    match package.language {
        Language::Rust => Ok(version.parse::<SemVer>()? > previous.parse::<SemVer>()?),
        Language::Python => Ok(version
            .parse::<pep440_rs::Version>()
            .map_err(|e| e.to_string())?
            > previous
                .parse::<pep440_rs::Version>()
                .map_err(|e| e.to_string())?),
    }
}

pub(super) fn version_base(
    package: &NativePackage,
    previous: Option<&NativePackage>,
) -> Result<NativePackage, String> {
    let mut base = package.clone();
    if let Some(previous) = previous {
        require_identity(previous, package)?;
        if greater(package, &previous.version, &base.version)? {
            base.version = previous.version.clone();
        }
    }
    Ok(base)
}
