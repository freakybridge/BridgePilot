//! Safe static metadata readers and loss-minimising writers for Python projects.
use super::python_dependencies;
use std::collections::BTreeMap;
use toml_edit::{DocumentMut, Item};

#[derive(Clone, Debug)]
pub(super) enum Slot {
    Array(Vec<String>, usize),
    Poetry(Vec<String>),
    Ini(String, String, usize),
}

#[derive(Clone, Debug)]
pub(super) struct Dependency {
    pub name: String,
    pub requirement: String,
    pub kind: String,
    pub local_path: Option<String>,
    pub external: bool,
    pub workspace: bool,
    pub slot: Slot,
}

#[derive(Clone, Debug)]
pub(super) struct Metadata {
    pub name: String,
    pub version: Option<String>,
    pub version_key: Option<Vec<String>>,
    pub dependencies: Vec<Dependency>,
    pub roots: Vec<String>,
    pub ini: bool,
}

pub(super) fn document(bytes: &[u8], path: &str) -> Result<DocumentMut, String> {
    std::str::from_utf8(bytes)
        .map_err(|_| format!("Python metadata is not UTF-8: {path}"))?
        .parse::<DocumentMut>()
        .map_err(|e| format!("Invalid Python TOML {path}: {e}"))
}

fn strings(item: &Item, label: &str) -> Result<Vec<String>, String> {
    item.as_array()
        .ok_or_else(|| format!("{label} must be an array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{label} must contain only strings"))
        })
        .collect()
}

fn at<'a>(doc: &'a DocumentMut, keys: &[&str]) -> Option<&'a Item> {
    let mut item = doc.as_item();
    for key in keys {
        item = item.get(key)?;
    }
    Some(item)
}

fn dep_array(
    doc: &DocumentMut,
    keys: Vec<String>,
    kind: &str,
    output: &mut Vec<Dependency>,
) -> Result<(), String> {
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    if let Some(item) = at(doc, &refs) {
        for (index, raw) in strings(item, kind)?.into_iter().enumerate() {
            let requirement = python_dependencies::parse(&raw)?;
            output.push(Dependency {
                name: requirement.name,
                requirement: raw,
                kind: kind.into(),
                local_path: None,
                external: requirement.direct,
                workspace: false,
                slot: Slot::Array(keys.clone(), index),
            });
        }
    }
    Ok(())
}

fn poetry_dependencies(
    doc: &DocumentMut,
    keys: Vec<String>,
    kind: &str,
    output: &mut Vec<Dependency>,
) -> Result<(), String> {
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let Some(item) = at(doc, &refs) else {
        return Ok(());
    };
    let table = item
        .as_table_like()
        .ok_or_else(|| format!("Poetry {kind} must be a table"))?;
    for (name, value) in table.iter() {
        if name.eq_ignore_ascii_case("python") {
            continue;
        }
        let normalized = python_dependencies::normalize_name(name)?;
        let mut path = keys.clone();
        path.push(name.to_string());
        let (requirement, local_path, external) = if let Some(raw) = value.as_str() {
            (raw.to_string(), None, false)
        } else if let Some(table) = value.as_table_like() {
            let sources = ["path", "git", "url"]
                .into_iter()
                .filter(|key| table.get(key).is_some())
                .count();
            if sources > 1 {
                return Err(format!(
                    "Poetry dependency {name} has conflicting path/git/url sources"
                ));
            }
            if sources > 0 && table.get("version").is_some() {
                return Err(format!(
                    "Poetry source dependency {name} cannot contain version; put a standard constraint in project.dependencies and keep the source table separate"
                ));
            }
            if table.get("git").is_some() || table.get("url").is_some() {
                // Such requirements are external unless an explicit mapping is
                // supplied; their revision/artifact must never be invented.
                (
                    table
                        .get("version")
                        .and_then(Item::as_str)
                        .unwrap_or("")
                        .to_string(),
                    None,
                    true,
                )
            } else {
                (
                    table
                        .get("version")
                        .and_then(Item::as_str)
                        .unwrap_or("")
                        .to_string(),
                    table.get("path").and_then(Item::as_str).map(str::to_string),
                    false,
                )
            }
        } else {
            return Err(format!("Unsupported Poetry dependency variants for {name}"));
        };
        python_dependencies::accepts(&requirement, "0.0.0")?;
        output.push(Dependency {
            name: normalized,
            requirement,
            kind: kind.into(),
            local_path,
            external,
            workspace: false,
            slot: Slot::Poetry(path),
        });
    }
    Ok(())
}

pub(super) fn parse(bytes: &[u8], path: &str) -> Result<Metadata, String> {
    if path.ends_with("setup.cfg") {
        return parse_ini(bytes, path);
    }
    let doc = document(bytes, path)?;
    if at(&doc, &["tool", "setuptools_scm"]).is_some()
        || at(&doc, &["tool", "versioningit"]).is_some()
        || at(&doc, &["tool", "hatch", "version", "source"]).and_then(Item::as_str) == Some("vcs")
        || at(&doc, &["tool", "pdm", "version", "source"]).and_then(Item::as_str) == Some("scm")
        || at(&doc, &["tool", "poetry-dynamic-versioning", "enable"]).and_then(Item::as_bool)
            == Some(true)
    {
        return Err(format!(
            "SCM/tag-derived Python versions need a tag release contract and are unsupported: {path}"
        ));
    }
    let project = doc.get("project");
    let poetry = at(&doc, &["tool", "poetry"]);
    let mut dependencies = Vec::new();
    let mut roots = Vec::new();
    let (name, version, version_key) = if let Some(project) = project {
        let name = project
            .get("name")
            .and_then(Item::as_str)
            .ok_or_else(|| format!("Missing static [project].name: {path}"))?;
        let dynamic = project
            .get("dynamic")
            .map(|item| strings(item, "project.dynamic"))
            .transpose()?
            .unwrap_or_default();
        if dynamic.iter().any(|value| {
            matches!(
                value.as_str(),
                "dependencies" | "optional-dependencies" | "name"
            )
        }) {
            return Err(format!(
                "Dynamic Python name/dependencies are unsupported; declare static metadata: {path}"
            ));
        }
        let version = project
            .get("version")
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("project.version must be a string: {path}"))
            })
            .transpose()?;
        if version.is_some() && dynamic.iter().any(|v| v == "version") {
            return Err(format!("Python version is both static and dynamic: {path}"));
        }
        if version.is_none() && !dynamic.iter().any(|value| value == "version") {
            return Err(format!(
                "Python project without a static version must declare version in project.dynamic: {path}"
            ));
        }
        dep_array(
            &doc,
            vec!["project".into(), "dependencies".into()],
            "runtime",
            &mut dependencies,
        )?;
        if let Some(optional) = project.get("optional-dependencies") {
            let mut extra_names = std::collections::BTreeSet::new();
            for (extra, _) in optional
                .as_table_like()
                .ok_or("project.optional-dependencies must be a table")?
                .iter()
            {
                if !extra_names.insert(python_dependencies::normalize_name(extra)?) {
                    return Err(format!("Duplicate normalized Python extra: {extra}"));
                }
                dep_array(
                    &doc,
                    vec![
                        "project".into(),
                        "optional-dependencies".into(),
                        extra.into(),
                    ],
                    &format!("extra:{extra}"),
                    &mut dependencies,
                )?;
            }
        }
        (
            name.to_string(),
            version,
            Some(vec!["project".into(), "version".into()]),
        )
    } else if let Some(poetry) = poetry {
        let name = poetry
            .get("name")
            .and_then(Item::as_str)
            .ok_or_else(|| format!("Missing static tool.poetry.name: {path}"))?;
        let version = poetry
            .get("version")
            .and_then(Item::as_str)
            .ok_or_else(|| format!("Missing static tool.poetry.version: {path}"))?;
        (
            name.to_string(),
            Some(version.to_string()),
            Some(vec!["tool".into(), "poetry".into(), "version".into()]),
        )
    } else {
        return Err(format!(
            "No static Python distribution metadata in {path}; setup.py/SCM execution is unsupported"
        ));
    };
    if let Some(groups) = doc.get("dependency-groups") {
        let groups = groups
            .as_table_like()
            .ok_or("dependency-groups must be a table")?;
        let mut group_names = std::collections::BTreeSet::new();
        for (name, _) in groups.iter() {
            if !group_names.insert(python_dependencies::normalize_name(name)?) {
                return Err(format!(
                    "Duplicate normalized Python dependency group: {name}"
                ));
            }
        }
        for (group, values) in groups.iter() {
            let values = values
                .as_array()
                .ok_or("dependency group must be an array")?;
            for (index, value) in values.iter().enumerate() {
                if let Some(raw) = value.as_str() {
                    let requirement = python_dependencies::parse(raw)?;
                    dependencies.push(Dependency {
                        name: requirement.name,
                        requirement: raw.into(),
                        kind: format!("group:{group}"),
                        local_path: None,
                        external: requirement.direct,
                        workspace: false,
                        slot: Slot::Array(vec!["dependency-groups".into(), group.into()], index),
                    });
                } else {
                    let included = value
                        .as_inline_table()
                        .and_then(|t| t.get("include-group"))
                        .and_then(toml_edit::Value::as_str)
                        .ok_or("Unsupported dependency group item")?;
                    if !group_names.contains(&python_dependencies::normalize_name(included)?) {
                        return Err(format!(
                            "Unknown included Python dependency group: {included}"
                        ));
                    }
                }
            }
        }
        validate_group_cycles(groups)?;
    }
    dep_array(
        &doc,
        vec!["build-system".into(), "requires".into()],
        "build",
        &mut dependencies,
    )?;
    poetry_dependencies(
        &doc,
        vec!["tool".into(), "poetry".into(), "dependencies".into()],
        "runtime",
        &mut dependencies,
    )?;
    poetry_dependencies(
        &doc,
        vec!["tool".into(), "poetry".into(), "dev-dependencies".into()],
        "development",
        &mut dependencies,
    )?;
    if let Some(groups) = at(&doc, &["tool", "poetry", "group"]) {
        for (group, _) in groups
            .as_table_like()
            .ok_or("Poetry group must be a table")?
            .iter()
        {
            poetry_dependencies(
                &doc,
                vec![
                    "tool".into(),
                    "poetry".into(),
                    "group".into(),
                    group.into(),
                    "dependencies".into(),
                ],
                &format!("group:{group}"),
                &mut dependencies,
            )?;
        }
    }
    // uv sources are explicit local identity evidence. Registry requirements with
    // an equal name are deliberately not inferred as local dependencies.
    if let Some(sources) = at(&doc, &["tool", "uv", "sources"]) {
        for (source_name, value) in sources
            .as_table_like()
            .ok_or("tool.uv.sources must be a table")?
            .iter()
        {
            let source_name = python_dependencies::normalize_name(source_name)?;
            if value.is_array() {
                return Err(format!(
                    "Conditional uv source variants require explicit supported source mapping: {source_name}"
                ));
            }
            let table = value.as_table_like().ok_or("uv source must be a table")?;
            let local_path = table.get("path").and_then(Item::as_str);
            let workspace = table
                .get("workspace")
                .and_then(Item::as_bool)
                .unwrap_or(false);
            for dependency in dependencies.iter_mut().filter(|d| d.name == source_name) {
                dependency.local_path = local_path.map(str::to_string);
                dependency.workspace = workspace;
            }
        }
    }
    if let Some(packages) = at(&doc, &["tool", "poetry", "packages"]).and_then(Item::as_array) {
        for value in packages {
            let table = value
                .as_inline_table()
                .ok_or("Poetry packages must contain inline tables")?;
            let include = table
                .get("include")
                .and_then(toml_edit::Value::as_str)
                .ok_or("Poetry package requires include")?;
            if include.contains(['*', '?', '[']) {
                // Discovery does not guess the import package expanded by a
                // backend glob. The project directory remains the conservative
                // ownership boundary unless import_roots is explicit.
                continue;
            }
            let from = table
                .get("from")
                .and_then(toml_edit::Value::as_str)
                .unwrap_or("");
            roots.push(if from.is_empty() {
                include.into()
            } else {
                format!("{from}/{include}")
            });
        }
    }
    Ok(Metadata {
        name: python_dependencies::normalize_name(&name)?,
        version,
        version_key,
        dependencies,
        roots,
        ini: false,
    })
}

fn validate_group_cycles(groups: &dyn toml_edit::TableLike) -> Result<(), String> {
    fn visit(
        name: &str,
        groups: &dyn toml_edit::TableLike,
        stack: &mut Vec<String>,
        done: &mut std::collections::BTreeSet<String>,
    ) -> Result<(), String> {
        if stack.iter().any(|s| s == name) {
            return Err(format!(
                "Cyclic Python dependency group: {} -> {name}",
                stack.join(" -> ")
            ));
        }
        if done.contains(name) {
            return Ok(());
        }
        stack.push(name.into());
        let normalized = python_dependencies::normalize_name(name)?;
        let items = groups
            .iter()
            .find(|(key, _)| {
                python_dependencies::normalize_name(key).ok().as_ref() == Some(&normalized)
            })
            .map(|(_, value)| value)
            .and_then(Item::as_array)
            .ok_or("Invalid dependency group")?;
        for value in items {
            if let Some(next) = value
                .as_inline_table()
                .and_then(|t| t.get("include-group"))
                .and_then(toml_edit::Value::as_str)
            {
                let next = python_dependencies::normalize_name(next)?;
                visit(&next, groups, stack, done)?;
            }
        }
        stack.pop();
        done.insert(name.into());
        Ok(())
    }
    let mut done = std::collections::BTreeSet::new();
    for (name, _) in groups.iter() {
        visit(
            &python_dependencies::normalize_name(name)?,
            groups,
            &mut Vec::new(),
            &mut done,
        )?;
    }
    Ok(())
}

#[derive(Clone)]
struct IniEntry {
    section: String,
    key: String,
    display_key: String,
    start: usize,
    end: usize,
    value: String,
}
fn ini_entries(text: &str) -> Result<Vec<IniEntry>, String> {
    let mut entries: Vec<IniEntry> = Vec::new();
    let mut section = String::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trim = line.trim();
        if trim.starts_with('[') && trim.ends_with(']') {
            section = trim[1..trim.len() - 1].trim().to_ascii_lowercase();
        } else if trim.is_empty() || trim.starts_with(['#', ';']) {
        } else if line.starts_with([' ', '\t']) {
            let previous = entries.last_mut().ok_or("INI continuation without value")?;
            previous.value.push('\n');
            previous.value.push_str(trim);
            previous.end = offset + line.len();
        } else {
            let (key, value) = trim
                .split_once('=')
                .ok_or_else(|| format!("Unsupported setup.cfg line: {trim}"))?;
            let display_key = key.trim().to_string();
            let key = key.trim().to_ascii_lowercase();
            if entries.iter().any(|e| e.section == section && e.key == key) {
                return Err(format!("Duplicate setup.cfg key: [{section}] {key}"));
            }
            entries.push(IniEntry {
                section: section.clone(),
                key,
                display_key,
                start: offset,
                end: offset + line.len(),
                value: value.trim().into(),
            });
        }
        offset += line.len();
    }
    Ok(entries)
}

fn parse_ini(bytes: &[u8], path: &str) -> Result<Metadata, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| format!("Python metadata is not UTF-8: {path}"))?;
    let entries = ini_entries(text)?;
    let get = |section: &str, key: &str| {
        entries
            .iter()
            .find(|e| e.section == section && e.key == key)
            .map(|e| e.value.clone())
    };
    let name = get("metadata", "name").ok_or("setup.cfg requires static metadata.name")?;
    let version = get("metadata", "version");
    let version = version.filter(|v| !v.starts_with("attr:") && !v.starts_with("file:"));
    let mut dependencies = Vec::new();
    for entry in &entries {
        let kind = if entry.section == "options" && entry.key == "install_requires" {
            "runtime".to_string()
        } else if entry.section == "options"
            && matches!(entry.key.as_str(), "setup_requires" | "tests_require")
        {
            format!("development:{}", entry.key)
        } else if entry.section == "options.extras_require" {
            format!("extra:{}", entry.key)
        } else {
            continue;
        };
        if entry.value.trim().starts_with("file:") || entry.value.trim().starts_with("attr:") {
            return Err(format!(
                "Dynamic setup.cfg dependencies are unsupported: {path}"
            ));
        }
        for (index, raw) in entry.value.lines().enumerate() {
            let raw = raw.trim();
            if raw.is_empty() || raw.starts_with('#') {
                continue;
            }
            let requirement = python_dependencies::parse(raw)?;
            dependencies.push(Dependency {
                name: requirement.name,
                requirement: raw.into(),
                kind: kind.clone(),
                local_path: None,
                external: requirement.direct,
                workspace: false,
                slot: Slot::Ini(entry.section.clone(), entry.key.clone(), index),
            });
        }
    }
    Ok(Metadata {
        name: python_dependencies::normalize_name(&name)?,
        version,
        version_key: None,
        dependencies,
        roots: Vec::new(),
        ini: true,
    })
}

fn item_mut<'a>(doc: &'a mut DocumentMut, keys: &[String]) -> Result<&'a mut Item, String> {
    let mut item = doc.as_item_mut();
    for key in keys {
        item = item
            .get_mut(key)
            .ok_or_else(|| format!("Python metadata key disappeared: {}", keys.join(".")))?;
    }
    Ok(item)
}

pub(super) fn render(
    bytes: &[u8],
    path: &str,
    metadata: &Metadata,
    version: Option<&str>,
    replacements: &[(Slot, String)],
) -> Result<Vec<u8>, String> {
    if metadata.ini {
        let text = std::str::from_utf8(bytes).map_err(|_| "Invalid setup.cfg UTF-8")?;
        let entries = ini_entries(text)?;
        let mut values: BTreeMap<(String, String), String> = BTreeMap::new();
        if let Some(version) = version {
            values.insert(("metadata".into(), "version".into()), version.into());
        }
        for (slot, replacement) in replacements {
            let Slot::Ini(section, key, index) = slot else {
                return Err("Python metadata slot format mismatch".into());
            };
            let entry = entries
                .iter()
                .find(|e| &e.section == section && &e.key == key)
                .ok_or("setup.cfg dependency disappeared")?;
            let value = values
                .entry((section.clone(), key.clone()))
                .or_insert_with(|| entry.value.clone());
            let mut lines = value.lines().map(str::to_string).collect::<Vec<_>>();
            *lines
                .get_mut(*index)
                .ok_or("setup.cfg dependency line disappeared")? = replacement.clone();
            *value = lines.join("\n");
        }
        let mut output = text.to_string();
        for entry in entries.iter().rev() {
            if let Some(value) = values.get(&(entry.section.clone(), entry.key.clone())) {
                let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
                let rendered = if value.contains('\n') {
                    format!(
                        "{} ={}    {}{}",
                        entry.display_key,
                        newline,
                        value
                            .lines()
                            .filter(|l| !l.is_empty())
                            .collect::<Vec<_>>()
                            .join(&format!("{newline}    ")),
                        newline
                    )
                } else {
                    format!("{} = {}{}", entry.display_key, value, newline)
                };
                output.replace_range(entry.start..entry.end, &rendered);
            }
        }
        return Ok(output.into_bytes());
    }
    let mut doc = document(bytes, path)?;
    if let Some(version) = version {
        *item_mut(
            &mut doc,
            metadata
                .version_key
                .as_ref()
                .ok_or("No static Python version key")?,
        )? = toml_edit::value(version);
    }
    for (slot, replacement) in replacements {
        match slot {
            Slot::Array(keys, index) => {
                let array = item_mut(&mut doc, keys)?
                    .as_array_mut()
                    .ok_or("Python dependency array disappeared")?;
                let value = array
                    .get_mut(*index)
                    .ok_or("Python dependency disappeared")?;
                *value = toml_edit::Value::from(replacement.as_str());
            }
            Slot::Poetry(keys) => {
                let item = item_mut(&mut doc, keys)?;
                if item.is_str() {
                    *item = toml_edit::value(replacement);
                } else {
                    let table = item
                        .as_table_like_mut()
                        .ok_or("Poetry dependency disappeared")?;
                    table.insert("version", toml_edit::value(replacement));
                }
            }
            Slot::Ini(..) => return Err("Python metadata slot format mismatch".into()),
        }
    }
    Ok(doc.to_string().into_bytes())
}
