//! Cargo's implicit path members. Uses the same snapshot reader for disk and history.
use super::*;

fn optional_document(
    workspace: &mut Workspace,
    path: &str,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<(), String> {
    if workspace.inputs.contains_key(path) {
        return Ok(());
    }
    let bytes = read(path)?;
    if let Some(bytes) = &bytes {
        workspace.documents.insert(path.into(), parse(path, bytes)?);
    }
    workspace.inputs.insert(path.into(), bytes);
    Ok(())
}

fn targets(workspace: &Workspace, root: &str, manifest: &str) -> Result<BTreeSet<String>, String> {
    let doc = &workspace.documents[manifest];
    let mut tables = Vec::new();
    for kind in ["dependencies", "build-dependencies", "dev-dependencies"] {
        if let Some(table) = doc.get(kind) {
            tables.push(table);
        }
    }
    if let Some(targets) = doc.get("target") {
        for (_, target) in targets
            .as_table_like()
            .ok_or("invalid Cargo target table")?
            .iter()
        {
            for kind in ["dependencies", "build-dependencies", "dev-dependencies"] {
                if let Some(table) = target.get(kind) {
                    tables.push(table);
                }
            }
        }
    }
    let mut result = BTreeSet::new();
    for table in tables {
        for (name, declared) in table
            .as_table_like()
            .ok_or("invalid Cargo dependency table")?
            .iter()
        {
            let inherited = declared.get("workspace").and_then(Item::as_bool) == Some(true);
            let (base, dependency) = if inherited {
                (
                    directory(root),
                    workspace.documents[root]
                        .get("workspace")
                        .and_then(|w| w.get("dependencies"))
                        .and_then(|d| d.get(name))
                        .ok_or_else(|| {
                            format!("{manifest}: inherited dependency {name} is missing")
                        })?,
                )
            } else {
                (directory(manifest), declared)
            };
            if let Some(path) = dependency.get("path") {
                let raw = path
                    .as_str()
                    .ok_or("Cargo dependency path must be a string")?;
                result.insert(manifest_at(&normalize(base, raw)?));
            }
        }
    }
    Ok(result)
}

fn owner(
    workspace: &mut Workspace,
    target: &str,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<String, String> {
    let doc = &workspace.documents[target];
    if let Some(pointer) = doc.get("package").and_then(|p| p.get("workspace")) {
        return Ok(manifest_at(&normalize(
            directory(target),
            pointer
                .as_str()
                .ok_or("package.workspace must be a string")?,
        )?));
    }
    if doc.get("workspace").is_some() {
        return Ok(target.into());
    }
    let mut dir = directory(target);
    while let Some((parent, _)) = dir
        .rsplit_once('/')
        .or_else(|| (!dir.is_empty()).then_some(("", dir)))
    {
        let candidate = manifest_at(parent);
        optional_document(workspace, &candidate, read)?;
        if let Some(table) = workspace
            .documents
            .get(&candidate)
            .and_then(|d| d.get("workspace"))
        {
            for excluded in strings(table.get("exclude"), "workspace.exclude")? {
                if member_pattern(&normalize(parent, &excluded)?)?.is_match(directory(target)) {
                    return Ok(target.into());
                }
            }
            return Ok(candidate);
        }
        if parent.is_empty() {
            break;
        }
        dir = parent;
    }
    Ok(target.into())
}

pub(super) fn expand(
    workspace: &mut Workspace,
    root: &str,
    paths: &mut BTreeSet<String>,
    excludes: &[Regex],
    inventory: &[String],
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<(), String> {
    // A standalone package without [workspace] has no implicit workspace members.
    if workspace.documents[root].get("workspace").is_none() {
        return Ok(());
    }
    let root_dir = directory(root);
    let mut visited = BTreeSet::new();
    loop {
        let pending = paths.difference(&visited).cloned().collect::<Vec<_>>();
        if pending.is_empty() {
            break;
        }
        for manifest in pending {
            visited.insert(manifest.clone());
            read_document(workspace, &manifest, read)?;
            for target in targets(workspace, root, &manifest)? {
                if paths.contains(&target) {
                    continue;
                }
                let target_dir = directory(&target);
                if (!root_dir.is_empty()
                    && !target_dir.starts_with(&format!("{root_dir}/"))
                    && target_dir != root_dir)
                    || excludes.iter().any(|e| e.is_match(target_dir))
                {
                    continue;
                }
                read_document(workspace, &target, read)?;
                if owner(workspace, &target, read)? != root {
                    continue;
                }
                if !inventory.contains(&target) {
                    return Err(format!(
                        "implicit Cargo member is outside repository inventory: {target}"
                    ));
                }
                paths.insert(target);
            }
        }
    }
    Ok(())
}
