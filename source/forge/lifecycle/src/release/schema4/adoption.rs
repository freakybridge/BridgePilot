//! First registration of a native package is distinct from managed identity
//! transitions. Its prior version is witnessed in Git, never added to the
//! immutable migration baseline or inferred from the product VERSION file.
use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::release) struct Adoption {
    pub id: String,
    pub revision: String,
    pub source: lifecycle::Identity,
    pub version: String,
}

pub(super) fn validate_configuration(
    declarations: &[Adoption],
    native: &polyglot::Config,
    current_model: bool,
) -> Result<(), String> {
    if !declarations.is_empty() && !current_model {
        return Err("package_adoptions requires the product-packages model".into());
    }
    let mut ids = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for declaration in declarations {
        let known_identity = native
            .packages
            .iter()
            .chain(
                native
                    .lifecycle
                    .iter()
                    .flat_map(|t| t.from.iter().chain(&t.to)),
            )
            .any(|p| p.id == declaration.id && p.language == Language::Rust);
        let source = &declaration.source;
        if !known_identity
            || source.id != declaration.id
            || source.name.is_empty()
            || source.language != Language::Rust
            || !matches!(declaration.revision.len(), 40 | 64)
            || !declaration.revision.bytes().all(|b| b.is_ascii_hexdigit())
            || !ids.insert(&declaration.id)
            || !sources.insert((&source.manifest, &source.name))
        {
            return Err("package adoption requires unique, continuous Rust identities and immutable revisions".into());
        }
        declaration.version.parse::<SemVer>()?;
        safe_relative(Path::new("."), &source.manifest)?;
        let workspace = source
            .workspace
            .as_ref()
            .ok_or("package adoption requires an explicit source workspace")?;
        safe_relative(Path::new("."), workspace)?;
        if !source.manifest.ends_with("Cargo.toml")
            || !workspace.ends_with("Cargo.toml")
            || native
                .product_packages
                .iter()
                .any(|p| p.id == source.id || p.manifest == source.manifest)
        {
            return Err("package adoption cannot overlap product carriers".into());
        }
    }
    Ok(())
}

pub(super) fn preserve(prior: &[Adoption], next: &[Adoption]) -> Result<(), String> {
    if prior.iter().any(|old| !next.iter().any(|new| new == old)) {
        return Err("committed package adoption provenance must be preserved verbatim".into());
    }
    Ok(())
}

/// Verify the complete historical Cargo graph, including inherited versions,
/// source discovery and local lock edges. A source must be the first-parent
/// HEAD immediately before registration, so an older convenient version or an
/// unrelated reachable Git object cannot be used to reset native history.
pub(super) fn verify(
    root: &Path,
    cfg: &Config,
    head: &str,
    runner: &dyn ProcessRunner,
) -> Result<BTreeMap<String, NativePackage>, String> {
    let mut result = BTreeMap::new();
    if cfg.package_adoptions.is_empty() {
        return Ok(result);
    }
    let history = history::git(root, runner, &["rev-list", "--first-parent", head])?;
    let revisions = history.lines().collect::<Vec<_>>();
    for declaration in &cfg.package_adoptions {
        let position = revisions
            .iter()
            .position(|r| *r == declaration.revision)
            .ok_or("package adoption source is outside the current first-parent history")?;
        let source_payload = revision_payload(root, &declaration.revision, CONFIG, runner)?;
        let source_config = source_payload
            .as_deref()
            .map(polyglot::parse)
            .transpose()?
            .flatten();
        if source_config.as_ref().is_some_and(|native| {
            native
                .packages
                .iter()
                .chain(&native.product_packages)
                .chain(native.lifecycle.iter().flat_map(|t| &t.from))
                .any(|p| {
                    p.id == declaration.id
                        || p.manifest == declaration.source.manifest
                        || (p.language == Language::Rust && p.name == declaration.source.name)
                })
        }) {
            return Err(format!(
                "{}: adoption source was already registered or retired",
                declaration.id
            ));
        }
        if let Some(bytes) = &source_payload {
            if parse(bytes)?
                .is_some_and(|source| source.baseline.retired_ids.contains(&declaration.id))
            {
                return Err(format!(
                    "{}: adoption source identity was already retired",
                    declaration.id
                ));
            }
            // Schema 2 registers every package in its Rust workspaces. It has
            // no stable identity list, but is still managed native history.
            if let Some(legacy) = config::parse(bytes)? {
                let inventory = history::paths(&history::git(
                    root,
                    runner,
                    &["ls-tree", "-r", "--name-only", "-z", &declaration.revision],
                )?);
                let registered = rust::snapshot(&legacy, &inventory, &mut |path| {
                    revision_payload(root, &declaration.revision, path, runner)
                })?;
                if registered.packages.values().any(|p| {
                    p.manifest == declaration.source.manifest || p.name == declaration.source.name
                }) {
                    return Err(format!(
                        "{}: adoption source was already registered by the Rust adapter",
                        declaration.id
                    ));
                }
            }
        }
        // The first committed registration must follow the source revision
        // directly. Later commits retain exactly the same declaration.
        let mut first_registration = None;
        let mut first_identity = None;
        for (index, revision) in revisions[..position].iter().enumerate().rev() {
            let Some(bytes) = revision_payload(root, revision, CONFIG, runner)? else {
                continue;
            };
            let Some(committed) = parse(&bytes)? else {
                continue;
            };
            if let Some(identity) = committed
                .native
                .packages
                .iter()
                .find(|p| p.id == declaration.id)
            {
                if !committed.package_adoptions.contains(declaration) {
                    return Err(format!(
                        "{}: adoption cannot be attached after managed registration",
                        declaration.id
                    ));
                }
                if first_registration.is_none() {
                    first_registration = Some(index);
                    first_identity = Some(identity.clone());
                }
            }
        }
        if first_registration.map_or(position != 0, |index| index + 1 != position) {
            return Err(format!(
                "{}: adoption must witness the HEAD immediately before first registration",
                declaration.id
            ));
        }
        let identity = first_identity
            .as_ref()
            .or_else(|| cfg.native.packages.iter().find(|p| p.id == declaration.id))
            .ok_or("package adoption first registration is missing")?;
        if identity.name != declaration.source.name
            || identity.language != declaration.source.language
        {
            return Err("package adoption first registration must retain the native source name and language".into());
        }
        let workspace = rust::RustConfig {
            workspaces: vec![declaration.source.workspace.clone().unwrap()],
            dependency_bump: "patch".into(),
            shared_inputs: vec![],
            product_inputs: vec![],
        };
        let inventory = history::paths(&history::git(
            root,
            runner,
            &["ls-tree", "-r", "--name-only", "-z", &declaration.revision],
        )?);
        let snapshot = rust::snapshot(&workspace, &inventory, &mut |path| {
            revision_payload(root, &declaration.revision, path, runner)
        })?;
        let package = snapshot
            .packages
            .get(&declaration.source.manifest)
            .filter(|package| lifecycle::matches(&declaration.source, package))
            .ok_or("package adoption source disagrees with the immutable native identity")?;
        if package.version != declaration.version {
            return Err(format!(
                "{}: adoption version disagrees with immutable native history",
                declaration.id
            ));
        }
        result.insert(declaration.id.clone(), package.clone());
    }
    Ok(result)
}

/// This is an effective native comparison, not a repair of the migration proof.
/// Keep the original manifest/roots so removed or moved source remains owned.
pub(super) fn effective_baseline(
    baseline: &mut polyglot::Snapshot,
    adopted: &BTreeMap<String, NativePackage>,
) {
    for (id, package) in adopted {
        // A later published removal retains provenance but must not resurrect
        // the old package as a newly retired object in every subsequent release.
        if baseline.config.as_ref().is_some_and(|scope| {
            scope
                .lifecycle
                .iter()
                .any(|t| t.from.iter().any(|p| &p.id == id))
                && !scope.packages.iter().any(|p| &p.id == id)
        }) {
            continue;
        }
        baseline
            .packages
            .entry(id.clone())
            .or_insert_with(|| package.clone());
    }
}
