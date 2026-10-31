use super::*;

#[derive(Clone, Debug, Serialize)]
pub(super) struct Change {
    pub path: String,
    pub role: String,
    pub packages: Vec<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub mode_changed: bool,
}

pub(super) struct Capture {
    pub head: String,
    pub boundary: String,
    pub inventory: Vec<String>,
    pub status: String,
    pub current: polyglot::Snapshot,
    pub baseline: polyglot::Snapshot,
    pub changes: Vec<Change>,
    // Git-normalized differences before public skeleton ownership filtering.
    pub git_content_changes: BTreeSet<String>,
    pub git_mode_changes: BTreeSet<String>,
    pub inputs: BTreeMap<PathBuf, Option<Vec<u8>>>,
    pub fingerprint: String,
    pub validation_fingerprint: String,
    pub product_version: String,
    pub release_history: Option<super::withdrawal::History>,
    pub environment_guard: Option<EnvironmentGuard>,
    pub portable_python: Option<String>,
    pub environment: String,
    pub tool_identities: BTreeMap<String, String>,
    pub portable_guards: Vec<portable_environment::Guard>,
    pub release_packages: BTreeSet<String>,
    pub build_directories: BTreeMap<String, BTreeSet<String>>,
    pub normalization: Option<content_identity::Rules>,
    pub validation_normalization: Option<content_identity::Rules>,
    pub modes: BTreeMap<String, String>,
}

pub(super) fn modes(
    root: &Path,
    runner: &dyn ProcessRunner,
) -> Result<BTreeMap<String, String>, String> {
    let mut result = BTreeMap::new();
    for entry in history::git(root, runner, &["ls-tree", "-r", "-z", "HEAD"])?
        .split('\0')
        .filter(|s| !s.is_empty())
    {
        let (metadata, path) = entry.split_once('\t').ok_or("malformed file mode tree")?;
        result.insert(
            path.into(),
            metadata
                .split_whitespace()
                .next()
                .ok_or("missing file mode")?
                .into(),
        );
    }
    let raw = history::git(
        root,
        runner,
        &["diff", "--raw", "--no-renames", "-z", "HEAD", "--"],
    )?;
    let entries = raw
        .split('\0')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if entries.len() % 2 != 0 {
        return Err("malformed file mode diff".into());
    }
    for entry in entries.chunks_exact(2) {
        result.insert(
            entry[1].into(),
            entry[0]
                .split_whitespace()
                .nth(1)
                .ok_or("missing diff mode")?
                .into(),
        );
    }
    result.remove(development::RECORD);
    Ok(result)
}

// --name-only can report stat-only candidates when inspection disables index
// refresh. --numstat reads the diff content and omits those false positives,
// while retaining binary, empty-file and mode-only changes. -z/--no-renames
// make the third tab-delimited field an unambiguous, possibly tabbed path.
fn content_changed_paths(
    root: &Path,
    revision: &str,
    runner: &dyn ProcessRunner,
) -> Result<BTreeSet<String>, String> {
    let raw = history::git(
        root,
        runner,
        &["diff", "--numstat", "--no-renames", "-z", revision, "--"],
    )?;
    raw.split('\0')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let fields = entry.splitn(3, '\t').collect::<Vec<_>>();
            if fields.len() != 3 || fields[2].is_empty() {
                return Err("malformed content diff path".into());
            }
            Ok(fields[2].to_string())
        })
        .collect()
}

// Content identity is independent of HEAD, staging and automatic records. Existing
// Git blob ids avoid reading unchanged large assets; dirty bytes use the same Git
// canonicalization, after external filters have been rejected by capture().
fn validation_fingerprint(
    root: &Path,
    cfg: &Config,
    head: &str,
    inventory: &[String],
    inputs: &BTreeMap<PathBuf, Option<Vec<u8>>>,
    environment: &str,
    runner: &dyn ProcessRunner,
) -> Result<(String, content_identity::Rules), String> {
    // Also inspect untracked/ignored native inputs before Git can invoke filters.
    let rules = content_identity::Rules::capture(
        root,
        inputs.keys().filter_map(|p| {
            p.strip_prefix(root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        }),
        runner,
    )?;
    let identity = content_identity::Identity::new(root, rules, runner);
    let mut tree = BTreeMap::<String, (String, String)>::new();
    for entry in history::git(root, runner, &["ls-tree", "-r", "-z", head])?
        .split('\0')
        .filter(|s| !s.is_empty())
    {
        let (metadata, path) = entry
            .split_once('\t')
            .ok_or("malformed Git validation tree")?;
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err("malformed Git validation tree metadata".into());
        }
        tree.insert(path.into(), (fields[0].into(), fields[2].into()));
    }
    let raw = history::git(
        root,
        runner,
        &["diff", "--raw", "--no-renames", "-z", head, "--"],
    )?;
    let entries = raw
        .split('\0')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if entries.len() % 2 != 0 {
        return Err("malformed validation mode comparison".into());
    }
    let modes = entries
        .chunks_exact(2)
        .map(|p| {
            Ok((
                p[1].to_string(),
                p[0].split_whitespace()
                    .nth(1)
                    .ok_or("malformed validation mode")?
                    .to_string(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    // One Git process canonicalizes all existing inputs. Captured bytes are
    // rechecked below, so a concurrent change cannot certify a different file.
    let hash_paths = inputs
        .iter()
        .filter(|(_, bytes)| bytes.is_some())
        .map(|(path, _)| {
            path.strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|p| {
            p != development::RECORD
                && !cfg.record_inputs.contains(p)
                && !matches!(p.as_str(), "VERSION" | "CHANGELOG.md")
        })
        .collect::<Vec<_>>();
    let mut canonical = BTreeMap::new();
    if !hash_paths.is_empty() {
        let mut request = ProcessRequest::new("git", root);
        request.args = ["hash-object", "--stdin-paths"]
            .iter()
            .map(Into::into)
            .collect();
        for path in &hash_paths {
            request.stdin.extend_from_slice(
                serde_json::to_string(path)
                    .map_err(|e| e.to_string())?
                    .as_bytes(),
            );
            request.stdin.push(b'\n');
        }
        request.timeout = Duration::from_secs(45);
        let output = runner.run(&request).map_err(|e| e.to_string())?;
        if output.timed_out || output.code != 0 {
            return Err("cannot hash validation inputs".into());
        }
        let stdout = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        let hashes = stdout.lines().collect::<Vec<_>>();
        if hashes.len() != hash_paths.len()
            || hashes
                .iter()
                .any(|h| !matches!(h.len(), 40 | 64) || !h.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("invalid Git validation input hashes".into());
        }
        canonical.extend(
            hash_paths
                .into_iter()
                .zip(hashes.into_iter().map(str::to_string)),
        );
    }
    let mut extras = BTreeMap::new();
    for (path, bytes) in inputs {
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if !inventory.contains(&relative) {
            extras.insert(
                relative.clone(),
                identity.hash(&relative, bytes.as_deref())?,
            );
        }
        let Some(_) = bytes else {
            tree.remove(&relative);
            continue;
        };
        if relative == development::RECORD
            || cfg.record_inputs.contains(&relative)
            || matches!(relative.as_str(), "VERSION" | "CHANGELOG.md")
        {
            continue;
        }
        let oid = canonical
            .get(&relative)
            .ok_or("validation input hash missing")?
            .clone();
        let mode = modes
            .get(&relative)
            .cloned()
            .or_else(|| tree.get(&relative).map(|v| v.0.clone()))
            .unwrap_or_else(|| "100644".into());
        tree.insert(relative, (mode, oid));
    }
    tree.retain(|path, _| {
        path != development::RECORD
            && !cfg.record_inputs.contains(path)
            && !matches!(path.as_str(), "VERSION" | "CHANGELOG.md")
    });
    extras.retain(|path, _| {
        path != development::RECORD
            && !cfg.record_inputs.contains(path)
            && !matches!(path.as_str(), "VERSION" | "CHANGELOG.md")
    });
    identity.rules.verify(root, runner)?;
    verify_release_inputs(inputs)?;
    Ok((
        digest(
            &serde_json::to_vec(
                &json!({"tree":tree,"native_extra_inputs":extras,"environment":environment}),
            )
            .map_err(|e| e.to_string())?,
        ),
        identity.rules,
    ))
}

pub(super) fn environment(
    root: &Path,
    cfg: &Config,
    guard: Option<&EnvironmentGuard>,
    runner: &dyn ProcessRunner,
) -> Result<String, String> {
    Ok(environment_snapshot(root, cfg, guard, runner, false)?.planning)
}

pub(super) struct EnvironmentSnapshot {
    pub planning: String,
    pub tools: BTreeMap<String, String>,
}

// Short-lived plans retain the strict host identity. Version coverage is
// computed separately and does not persist an environment certificate.
pub(super) fn environment_fingerprints(
    values: &BTreeMap<String, String>,
) -> Result<EnvironmentSnapshot, String> {
    Ok(EnvironmentSnapshot {
        planning: digest(&serde_json::to_vec(values).map_err(|e| e.to_string())?),
        tools: values
            .iter()
            .filter(|(key, _)| key.starts_with("tool:"))
            .map(|(key, value)| (key.clone(), digest(value.as_bytes())))
            .collect(),
    })
}

fn environment_snapshot(
    root: &Path,
    cfg: &Config,
    guard: Option<&EnvironmentGuard>,
    runner: &dyn ProcessRunner,
    release: bool,
) -> Result<EnvironmentSnapshot, String> {
    let mut values = std::env::vars_os()
        .filter(|(key, _)| key != "CODEX_TOOL_CALL_ID")
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if let Some(guard) = guard {
        values.insert("python:environment".into(), guard.fingerprint.clone());
    }
    if cfg.native.adapters.rust.is_some() || !cfg.product_manifests.is_empty() {
        for (program, args) in [("rustc", vec!["-vV"]), ("cargo", vec!["-Vv"])] {
            let mut request = ProcessRequest::new(program, root);
            request.args = args.into_iter().map(Into::into).collect();
            request.timeout = Duration::from_secs(30);
            let output = runner
                .run(&request)
                .map_err(|e| format!("cannot inspect release toolchain: {e}"))?;
            if output.code != 0 || output.timed_out {
                return Err(format!("cannot inspect release toolchain: {program}"));
            }
            values.insert(
                format!("tool:{program}"),
                String::from_utf8(output.stdout).map_err(|e| e.to_string())?,
            );
        }
    }
    if release {
        release_environment_fingerprints(&values)
    } else {
        environment_fingerprints(&values)
    }
}

/// Terminal/session bookkeeping does not select a release toolchain or input.
/// Keep all other variables conservatively. Explicitly declared project
/// variables (including one of these names) are separately bound by release_gate.
pub(super) fn release_environment_fingerprints(
    values: &BTreeMap<String, String>,
) -> Result<EnvironmentSnapshot, String> {
    let mut effective = values.clone();
    for name in [
        "CODEX_TOOL_CALL_ID",
        "WT_SESSION",
        "WT_PROFILE_ID",
        "TERM_SESSION_ID",
        "SHLVL",
        "_",
        "PSModuleAnalysisCachePath",
    ] {
        effective.remove(name);
    }
    environment_fingerprints(&effective)
}

fn lock(cfg: &Config, path: &str) -> bool {
    Path::new(path).file_name().and_then(|p| p.to_str()) == Some("Cargo.lock")
        || cfg
            .native
            .adapters
            .python
            .as_ref()
            .is_some_and(|p| p.locks.iter().any(|l| l.path == path))
}

pub(super) fn capture(
    root: &Path,
    cfg: &Config,
    runner: &dyn ProcessRunner,
) -> Result<Capture, String> {
    capture_mode(root, cfg, runner, false)
}

pub(super) fn capture_release(
    root: &Path,
    cfg: &Config,
    reader: &history_cache::Reader<'_>,
) -> Result<Capture, String> {
    crate::baseline::require_supported_project(root)?;
    if !cfg.current_model {
        return Err("release requires the current product-packages configuration".into());
    }
    let _profile = crate::release_metrics::span("inputs.capture");
    crate::git_sync::verify_readonly_git(root, reader)?;
    capture_with_reader(root, cfg, reader, true)
}

fn capture_mode(
    root: &Path,
    cfg: &Config,
    runner: &dyn ProcessRunner,
    release: bool,
) -> Result<Capture, String> {
    crate::baseline::require_supported_project(root)?;
    if !cfg.current_model {
        return Err("release requires the one-time product-packages cutover; update the skeleton or run --migration-plan".into());
    }
    let inspection = InspectionRunner(runner);
    let _profile = crate::release_metrics::span("inputs.capture");
    crate::git_sync::verify_readonly_git(root, &inspection)?;
    let reader = history_cache::Reader::new(root, &inspection);
    let result = capture_with_reader(root, cfg, &reader, release)?;
    reader.verify_unchanged()?;
    Ok(result)
}

fn capture_with_reader(
    root: &Path,
    cfg: &Config,
    runner: &history_cache::Reader<'_>,
    release: bool,
) -> Result<Capture, String> {
    let (head, boundary, release_history) =
        history::validate_baseline_state(root, runner, None, false, None)?;
    super::prefetch_history(cfg, &head, runner)?;
    let inventory = independent::inventory(root, runner)?;
    let status = independent::status(root, runner)?;
    let current = polyglot::snapshot(&cfg.native, inventory.clone(), false, &mut |p| {
        config::read_current(root, p)
    })?;
    let mut baseline = super::baseline_snapshot(root, cfg, &boundary, runner)?;
    let adopted =
        super::lineage::validate(root, cfg, &baseline, &current, &head, &boundary, runner)?;
    super::adoption::effective_baseline(&mut baseline, &adopted);
    for (id, p) in &current.packages {
        if cfg.baseline.retired_ids.contains(id) {
            return Err(format!("{id}: retired identity cannot be reused"));
        }
        if let Some(old) = baseline.packages.get(id) {
            if old.version != p.version && !development::version_recorded(root, id, &p.version)? {
                return Err(format!("{id}: version changed outside managed release"));
            }
        } else if revision_payload(root, &boundary, &p.manifest, runner)?.is_some()
            && !cfg.native.lifecycle.iter().any(|t| {
                t.base == boundary
                    && t.to.iter().any(|to| to.id == *id)
                    && t.from
                        .iter()
                        .any(|from| baseline.packages.contains_key(&from.id))
            })
        {
            return Err(format!(
                "{id}: existing native package is missing from the verified release baseline"
            ));
        }
    }
    let mut paths = content_changed_paths(root, &boundary, runner)?;
    let git_content_changes = paths.clone();
    let raw = history::git(
        root,
        runner,
        &["diff", "--raw", "--no-renames", "-z", &boundary, "--"],
    )?;
    let entries = raw
        .split('\0')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if entries.len() % 2 != 0 {
        return Err("malformed final Git mode comparison".into());
    }
    let mut modes = BTreeSet::new();
    for pair in entries.chunks_exact(2) {
        let fields = pair[0].split_whitespace().collect::<Vec<_>>();
        if fields.len() < 2 {
            return Err("malformed final Git mode entry".into());
        }
        if fields[0].trim_start_matches(':') != fields[1] {
            modes.insert(pair[1].to_string());
        }
    }
    paths.extend(history::paths(&history::git(
        root,
        runner,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?));
    runner.prefetch(&boundary, &paths.iter().cloned().collect::<Vec<_>>())?;
    let mut inputs = current
        .inputs
        .iter()
        .map(|(p, b)| (root.join(p), b.clone()))
        .collect::<BTreeMap<_, _>>();
    let product = fs::read_to_string(root.join("VERSION"))
        .map_err(|e| e.to_string())?
        .trim()
        .parse::<SemVer>()?;
    inputs.extend(
        super::product_mapping_inputs(
            cfg,
            &current.packages,
            &inventory,
            &mut |p| config::read_current(root, p),
            product,
        )?
        .into_iter()
        .map(|(p, b)| (root.join(p), b)),
    );
    // A net-reverted path can remain dirty to HEAD. Bind it even though it has no release impact.
    for path in content_changed_paths(root, "HEAD", runner)? {
        inputs.insert(root.join(&path), config::read_current(root, &path)?);
    }
    let factory = root.join("bridgeforge-manifest.json").is_file();
    // Construct only when a path actually needs classification. Keep the first
    // contract bytes separately so later path reads cannot overwrite the guard.
    // Use the formal release boundary for both ownership and payloads: paths
    // include committed, unreleased changes that are already identical to HEAD.
    let mut classifier: Option<(PayloadClassifier, Vec<u8>)> = None;
    let mut changes = Vec::new();
    for path in paths {
        if path == "VERSION" {
            continue;
        }
        let old = revision_payload(root, &boundary, &path, runner)?;
        let new = config::read_current(root, &path)?;
        inputs.insert(root.join(&path), new.clone());
        let mode_changed = modes.contains(&path);
        if old == new && !mode_changed {
            continue;
        }
        // A migration itself is bookkeeping; native/source changes remain visible.
        if path == CONFIG && boundary == cfg.baseline.revision {
            let value = crate::json::parse_unique_json(
                new.as_deref().ok_or("release config removed")?,
                "release config",
            )?;
            if migration::is_initial_configuration(&value, cfg, &current)? {
                continue;
            }
        }
        if !factory {
            if classifier.is_none() {
                let bytes = config::read_current(root, ".codex/managed-skeleton.json")?
                    .ok_or("current-only baseline is missing")?;
                let parsed = PayloadClassifier::new(&bytes, |p| {
                    revision_payload(root, &boundary, p, runner)
                })?;
                classifier = Some((parsed, bytes));
            }
            if classifier.as_ref().unwrap().0.classify(
                std::slice::from_ref(&path),
                |p| revision_payload(root, &boundary, p, runner),
                |p| release_input(&root.join(p)),
            )? == ReleaseKind::SkeletonOnly
            {
                continue;
            }
        }
        let build_role = cfg.build_inputs.iter().any(|p| polyglot::owns(p, &path));
        if build_role
            && (matches!(
                Path::new(&path).file_name().and_then(|p| p.to_str()),
                Some("Cargo.toml" | "pyproject.toml" | "setup.cfg")
            ) || matches!(
                Path::new(&path).extension().and_then(|e| e.to_str()),
                Some("rs" | "py" | "pyi" | "c" | "cc" | "cpp" | "h" | "hpp")
            ))
        {
            return Err(format!(
                "global build role cannot suppress package source/manifest change: {path}"
            ));
        }
        let (role, owners) = if path == development::RECORD || cfg.record_inputs.contains(&path) {
            ("record", vec![])
        } else if lock(cfg, &path) || build_role {
            ("build", vec![])
        } else if path == CONFIG || path == "CHANGELOG.md" || cfg.product_manifests.contains(&path)
        {
            ("product", vec![])
        } else {
            let mut owners = BTreeSet::new();
            if old.is_some() {
                match polyglot::owner(&baseline, &path, &cfg.native) {
                    Ok(ids) => owners.extend(ids),
                    Err(e)
                        if (factory
                            || (baseline.packages.is_empty() && current.packages.is_empty()))
                            && e.starts_with("unattributed release input:") => {}
                    Err(e) => {
                        // New configuration may supply a precise product declaration, never guess a package owner.
                        if !cfg
                            .native
                            .product_inputs
                            .iter()
                            .any(|p| polyglot::owns(p, &path))
                        {
                            return Err(e);
                        }
                    }
                }
            }
            if new.is_some() {
                match polyglot::owner(&current, &path, &cfg.native) {
                    Ok(ids) => owners.extend(ids),
                    Err(e)
                        if (factory
                            || (baseline.packages.is_empty() && current.packages.is_empty()))
                            && e.starts_with("unattributed release input:") => {}
                    Err(e) => return Err(e),
                }
            }
            (
                if owners.is_empty() {
                    "product"
                } else {
                    "package"
                },
                owners.into_iter().collect(),
            )
        };
        changes.push(Change {
            path,
            role: role.into(),
            packages: owners,
            before: old.as_deref().map(digest),
            after: new.as_deref().map(digest),
            mode_changed,
        });
    }
    if let Some((_, bytes)) = classifier {
        let path = root.join(".codex/managed-skeleton.json");
        if inputs
            .get(&path)
            .is_some_and(|previous| previous.as_ref() != Some(&bytes))
        {
            return Err("ownership contract changed during schema 4 input capture".into());
        }
        inputs.insert(path, Some(bytes));
    }
    for path in [CONFIG, "VERSION", "CHANGELOG.md", development::RECORD]
        .into_iter()
        .chain(cfg.product_manifests.iter().map(String::as_str))
    {
        inputs
            .entry(root.join(path))
            .or_insert(config::read_current(root, path)?);
    }
    let hashes = inputs
        .iter()
        .map(|(p, b)| {
            (
                p.strip_prefix(root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/"),
                b.as_deref().map(digest),
            )
        })
        .collect::<BTreeMap<_, _>>();
    // No release payload means no build/runtime evidence is consumed. Classification,
    // history, ownership, ledger and transaction checks still run on this path.
    let needs_evidence = !release || changes.iter().any(|c| c.role != "record");
    let python = cfg
        .python_environment
        .as_ref()
        .filter(|_| needs_evidence)
        .map(|environment| environment.capture_with_portable(root))
        .transpose()?;
    let (environment_guard, portable_python) = match python {
        Some((guard, portable)) => (Some(guard), Some(portable)),
        None => (None, None),
    };
    let environment_snapshot = if needs_evidence {
        environment_snapshot(root, cfg, environment_guard.as_ref(), runner, release)?
    } else {
        EnvironmentSnapshot {
            planning: String::new(),
            tools: BTreeMap::new(),
        }
    };
    let environment = environment_snapshot.planning;
    let (validation_fingerprint, validation_normalization) = if needs_evidence {
        let (fingerprint, rules) =
            validation_fingerprint(root, cfg, &head, &inventory, &inputs, &environment, runner)?;
        (fingerprint, Some(rules))
    } else {
        (String::new(), None)
    };
    let fingerprint=digest(&serde_json::to_vec(&json!({"root":root.canonicalize().map_err(|e|e.to_string())?.to_string_lossy(),"head":head,"boundary":boundary,"history":planning_history_state(root,runner)?,"status":status,"inventory":inventory,"inputs":hashes,"changes":changes,"environment":environment,"validation_fingerprint":validation_fingerprint})).map_err(|e|e.to_string())?);
    verify_release_inputs(&inputs)?;
    if independent::status(root, runner)? != status
        || independent::inventory(root, runner)? != inventory
        || history::git(root, runner, &["rev-parse", "HEAD"])?.trim() != head
    {
        return Err("repository changed during schema 4 input capture".into());
    }
    Ok(Capture {
        release_history,
        modes: if current.packages.is_empty() && baseline.packages.is_empty() {
            BTreeMap::new()
        } else {
            self::modes(root, runner)?
        },
        product_version: fs::read_to_string(root.join("VERSION"))
            .map_err(|e| e.to_string())?
            .trim()
            .into(),
        head,
        boundary,
        inventory,
        status,
        current,
        baseline,
        changes,
        git_content_changes,
        git_mode_changes: modes,
        inputs,
        fingerprint,
        validation_fingerprint,
        environment_guard,
        portable_python,
        environment,
        tool_identities: environment_snapshot.tools,
        portable_guards: Vec::new(),
        release_packages: BTreeSet::new(),
        build_directories: BTreeMap::new(),
        normalization: None,
        validation_normalization,
    })
}
