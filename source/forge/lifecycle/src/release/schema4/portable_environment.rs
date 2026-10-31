//! Opt-in build/test environment contract. Host locations are not evidence identities.
//! Undeclared application variables are removed from checks, never silently inherited.
use super::*;
use std::ffi::OsString;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Contract {
    #[serde(default)]
    pub variables: Vec<String>,
    pub tools: Vec<String>,
    /// Stable role -> project-relative path or ${VARIABLE}/suffix. Values are paths,
    /// not identities; file bytes and relative directory membership form the proof.
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
    #[serde(default)]
    pub cargo_configs: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct Guard {
    contract: Contract,
    facts: BTreeMap<String, String>,
    rust: bool,
    python: Option<PythonEnvironment>,
}

impl Guard {
    pub(super) fn new(
        contract: &Contract,
        facts: BTreeMap<String, String>,
        capture: &Capture,
    ) -> Self {
        Self {
            contract: contract.clone(),
            facts,
            rust: !capture.tool_identities.is_empty(),
            python: capture
                .environment_guard
                .as_ref()
                .map(|g| g.configuration.clone()),
        }
    }

    pub(super) fn verify(&self, root: &Path) -> Result<(), String> {
        let mut tools = BTreeMap::new();
        if self.rust {
            for (program, arg) in [("rustc", "-vV"), ("cargo", "-Vv")] {
                let mut request = ProcessRequest::new(program, root);
                request.args = vec![arg.into()];
                let output = crate::SystemProcessRunner
                    .run(&request)
                    .map_err(|e| e.to_string())?;
                if output.code != 0 || output.timed_out {
                    return Err("cannot revalidate portable toolchain".into());
                }
                tools.insert(format!("tool:{program}"), digest(&output.stdout));
            }
        }
        let python = self
            .python
            .as_ref()
            .map(|p| p.capture_with_portable(root).map(|(_, identity)| identity))
            .transpose()?;
        if self
            .contract
            .snapshot_with(root, &tools, python.as_deref())?
            != self.facts
        {
            return Err(
                "portable validation environment changed before apply; inspect inputs again".into(),
            );
        }
        Ok(())
    }
}

fn secret_name(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    [
        "TOKEN",
        "PASSWORD",
        "SECRET",
        "CREDENTIAL",
        "PRIVATE_KEY",
        "API_KEY",
    ]
    .iter()
    .any(|part| name.contains(part))
}

fn host_variable(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "PATH"
            | "PATHEXT"
            | "SYSTEMROOT"
            | "WINDIR"
            | "COMSPEC"
            | "SYSTEMDRIVE"
            | "TEMP"
            | "TMP"
            | "TMPDIR"
            | "HOME"
            | "USERPROFILE"
            | "APPDATA"
            | "LOCALAPPDATA"
            | "PROGRAMFILES"
            | "PROGRAMFILES(X86)"
            | "PROGRAMW6432"
            | "PROGRAMDATA"
            | "HOMEDRIVE"
            | "HOMEPATH"
            | "CARGO_HOME"
            | "RUSTUP_HOME"
            | "CARGO_TARGET_DIR"
    )
}

fn build_variable(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    if name == "CARGO_TARGET_DIR" {
        return false;
    }
    matches!(
        name.as_str(),
        "RUSTFLAGS"
            | "RUSTDOCFLAGS"
            | "CARGO_ENCODED_RUSTFLAGS"
            | "CARGO_ENCODED_RUSTDOCFLAGS"
            | "RUSTUP_TOOLCHAIN"
            | "RUSTC_BOOTSTRAP"
            | "CARGO_BUILD_TARGET"
            | "CARGO_BUILD_RUSTFLAGS"
            | "CARGO_BUILD_RUSTDOCFLAGS"
            | "CARGO_INCREMENTAL"
            | "MACOSX_DEPLOYMENT_TARGET"
            | "IPHONEOS_DEPLOYMENT_TARGET"
            | "SOURCE_DATE_EPOCH"
    ) || name.starts_with("CARGO_PROFILE_")
        || name.starts_with("CARGO_TARGET_")
        || name.starts_with("CARGO_CFG_")
        || name.starts_with("CARGO_FEATURE_")
        || tool_override(&name)
}

fn tool_override(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    matches!(
        name.as_str(),
        "RUSTC"
            | "RUSTDOC"
            | "RUSTC_WRAPPER"
            | "RUSTC_WORKSPACE_WRAPPER"
            | "CARGO_BUILD_RUSTC"
            | "CARGO_BUILD_RUSTDOC"
            | "CARGO_BUILD_RUSTC_WRAPPER"
            | "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER"
            | "CC"
            | "CXX"
            | "AR"
            | "LD"
    ) || (name.starts_with("CARGO_TARGET_")
        && (name.ends_with("_LINKER") || name.ends_with("_RUNNER")))
        || ["CC_", "CXX_", "AR_"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

fn environment() -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    for (key, value) in std::env::vars_os() {
        let key = key
            .into_string()
            .map_err(|_| "non-Unicode environment key cannot be certified")?;
        let value = value
            .into_string()
            .map_err(|_| format!("non-Unicode environment value: {key}"))?;
        let key = if cfg!(windows) {
            key.to_ascii_uppercase()
        } else {
            key
        };
        values.insert(key, value);
    }
    Ok(values)
}

fn variable<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Option<&'a String> {
    values.get(&if cfg!(windows) {
        key.to_ascii_uppercase()
    } else {
        key.into()
    })
}

pub(super) fn resolve_tool(root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.starts_with("${") {
        let path = location(root, name, &environment()?)?;
        if !path.is_file() {
            return Err("declared environment tool location is missing".into());
        }
        return path.canonicalize().map_err(|e| e.to_string());
    }
    let path = Path::new(name);
    let candidates = if path.is_absolute() || name.contains('/') || name.contains('\\') {
        vec![if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        }]
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .filter(|p| p.is_absolute())
            .map(|p| p.join(path))
            .collect()
    };
    for candidate in candidates {
        let paths = if cfg!(windows) && candidate.extension().is_none() {
            vec![
                candidate.with_extension("exe"),
                candidate.with_extension("com"),
            ]
        } else {
            vec![candidate]
        };
        for path in paths {
            if path.is_file() {
                return path.canonicalize().map_err(|e| e.to_string());
            }
        }
    }
    Err(format!("portable evidence tool is missing: {name}"))
}

fn location(
    root: &Path,
    configured: &str,
    values: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    let path = if let Some(rest) = configured.strip_prefix("${") {
        let (name, tail) = rest
            .split_once('}')
            .ok_or("invalid environment input location")?;
        if secret_name(name)
            || (!tail.is_empty() && !tail.starts_with('/') && !tail.starts_with('\\'))
        {
            return Err("invalid environment input location".into());
        }
        PathBuf::from(
            variable(values, name)
                .ok_or_else(|| format!("environment input location missing: {name}"))?,
        )
        .join(tail.trim_start_matches(['/', '\\']))
    } else {
        PathBuf::from(configured)
    };
    Ok(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

fn bytes_identity(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut file =
        fs::File::open(path).map_err(|e| format!("cannot inspect environment input: {e}"))?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let size = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("sha256:{:x}", hash.finalize()))
}

fn input_identity(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Ok("absent".into());
    }
    for ancestor in path.ancestors().filter(|p| p.exists()) {
        if crate::persistence::is_link_or_reparse(ancestor).map_err(|e| e.to_string())? {
            return Err("environment inputs must not traverse links".into());
        }
    }
    if path.is_file() {
        return bytes_identity(path);
    }
    if !path.is_dir() {
        return Err("environment input is not a file or directory".into());
    }
    let mut entries = BTreeMap::new();
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if crate::persistence::is_link_or_reparse(entry.path()).map_err(|e| e.to_string())? {
            return Err("environment input contains links".into());
        }
        let relative = entry
            .path()
            .strip_prefix(path)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        entries.insert(
            relative,
            if entry.file_type().is_dir() {
                "directory".into()
            } else if entry.file_type().is_file() {
                bytes_identity(entry.path())?
            } else {
                return Err("environment input contains a special file".into());
            },
        );
    }
    Ok(digest(
        &serde_json::to_vec(&entries).map_err(|e| e.to_string())?,
    ))
}

impl Contract {
    pub(super) fn validate_arguments(&self, args: &[String]) -> Result<(), String> {
        for (index, arg) in args.iter().enumerate() {
            let value = if arg == "--config" {
                args.get(index + 1).map(String::as_str)
            } else {
                arg.strip_prefix("--config=")
            };
            if let Some(value) = value {
                if value.contains('=') || !self.cargo_configs.iter().any(|p| p == value) {
                    return Err("portable --config arguments require a declared cargo_configs file; inline overrides are not modeled".into());
                }
            }
            if arg.contains("linker=") || arg.contains("--sysroot") {
                return Err("portable command-line linker/sysroot overrides are not modeled; use declared tools and inputs".into());
            }
        }
        Ok(())
    }

    fn path_variables(&self) -> BTreeSet<String> {
        self.inputs
            .values()
            .chain(&self.tools)
            .filter_map(|path| {
                path.strip_prefix("${")
                    .and_then(|rest| rest.split_once('}'))
                    .map(|(name, _)| {
                        if cfg!(windows) {
                            name.to_ascii_uppercase()
                        } else {
                            name.into()
                        }
                    })
            })
            .collect()
    }

    fn declared_tool(&self, root: &Path, name: &str) -> Result<String, String> {
        self.declared_tool_at(root, root, name)
    }

    fn declared_tool_at(&self, root: &Path, base: &Path, name: &str) -> Result<String, String> {
        let path = resolve_tool(base, name)?;
        if !self
            .tools
            .iter()
            .any(|tool| resolve_tool(root, tool).ok().as_ref() == Some(&path))
        {
            return Err("portable environment uses an undeclared compiler/linker/wrapper; declare its tool identity".into());
        }
        bytes_identity(&path)
    }

    fn cargo_configuration(&self, root: &Path, path: &Path) -> Result<String, String> {
        if !path.exists() {
            return Ok("absent".into());
        }
        let bytes = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut document = bytes
            .parse::<toml_edit::DocumentMut>()
            .map_err(|_| "cannot parse portable Cargo configuration")?;
        fn unsupported_value(value: &toml_edit::Value) -> bool {
            if let Some(text) = value.as_str() {
                return text.contains("linker=") || text.contains("--sysroot");
            }
            if let Some(array) = value.as_array() {
                return array.iter().any(unsupported_value);
            }
            value
                .as_inline_table()
                .is_some_and(|t| t.iter().any(|(_, v)| unsupported_value(v)))
        }
        fn unsupported_item(item: &toml_edit::Item) -> bool {
            item.as_value().is_some_and(unsupported_value)
                || item
                    .as_table()
                    .is_some_and(|t| t.iter().any(|(_, i)| unsupported_item(i)))
                || item
                    .as_array_of_tables()
                    .is_some_and(|a| a.iter().any(|t| t.iter().any(|(_, i)| unsupported_item(i))))
        }
        if document.iter().any(|(_, item)| unsupported_item(item)) {
            return Err("portable Cargo linker/sysroot flags are not modeled; declare the effective toolchain".into());
        }
        // Cargo resolves slash-containing relative paths from the parent of the
        // configuration directory, including explicitly supplied --config files.
        let base = path.parent().and_then(Path::parent).unwrap_or(root);
        // Config can introduce commands after the parent environment was filtered.
        // Bind known single-program overrides; reject command strings/unknown execution.
        if document.iter().any(|(key, _)| {
            !matches!(
                key,
                "build" | "target" | "term" | "source" | "registries" | "registry" | "net"
            )
        }) {
            return Err("portable Cargo configuration section is not modeled; retain host-bound evidence for external inputs or commands".into());
        }
        // Remote registry mirrors are ordinary transport configuration. Local
        // replacements and credential programs introduce new executable/file inputs.
        let unsupported = || {
            "portable Cargo configuration section is not modeled: local sources, credentials or unknown transport fields".to_string()
        };
        let remote_index = |value: &toml_edit::Item| {
            value.as_str().is_some_and(|value| {
                ["https://", "http://", "sparse+https://", "sparse+http://"]
                    .iter()
                    .any(|prefix| value.starts_with(prefix))
                    && !value.contains(['@', '?', '#'])
            })
        };
        for (section, fields) in [
            ("source", &["registry", "replace-with"][..]),
            ("registries", &["index", "protocol"][..]),
        ] {
            if let Some(item) = document.get(section) {
                let table = item.as_table().ok_or_else(unsupported)?;
                for (_, item) in table.iter() {
                    let entry = item.as_table().ok_or_else(unsupported)?;
                    for (key, value) in entry.iter() {
                        if !fields.contains(&key) {
                            return Err(unsupported());
                        }
                        let valid = match key {
                            "registry" | "index" => remote_index(value),
                            "replace-with" => value.as_str().is_some_and(|v| !v.is_empty()),
                            "protocol" => value
                                .as_str()
                                .is_some_and(|v| matches!(v, "sparse" | "git")),
                            _ => false,
                        };
                        if !valid {
                            return Err(unsupported());
                        }
                    }
                }
            }
        }
        if let Some(item) = document.get("registry") {
            let table = item.as_table().ok_or_else(unsupported)?;
            if table
                .iter()
                .any(|(key, value)| key != "default" || value.as_str().is_none())
            {
                return Err(unsupported());
            }
        }
        let mut implicit_tools = BTreeMap::new();
        if let Some(item) = document.get("net") {
            let table = item.as_table().ok_or_else(unsupported)?;
            for (key, value) in table.iter() {
                let valid = match key {
                    "retry" => value.as_integer().is_some_and(|v| v >= 0),
                    "offline" | "git-fetch-with-cli" => value.as_bool().is_some(),
                    _ => false,
                };
                if !valid {
                    return Err(unsupported());
                }
            }
            if table.get("git-fetch-with-cli").and_then(|v| v.as_bool()) == Some(true) {
                implicit_tools.insert("git", bytes_identity(&resolve_tool(root, "git")?)?);
            }
        }
        if ["build", "target"]
            .iter()
            .any(|key| document.get(key).is_some_and(|item| !item.is_table()))
        {
            return Err("portable Cargo inline build/target tables are not modeled".into());
        }
        if let Some(build) = document.get_mut("build").and_then(|i| i.as_table_mut()) {
            for key in [
                "rustc",
                "rustdoc",
                "rustc-wrapper",
                "rustc-workspace-wrapper",
            ] {
                if let Some(item) = build.get_mut(key) {
                    let name = item.as_str().ok_or("unsupported Cargo tool override")?;
                    *item = toml_edit::value(self.declared_tool_at(root, base, name)?);
                }
            }
        }
        if let Some(targets) = document.get_mut("target").and_then(|i| i.as_table_mut()) {
            for (_, item) in targets.iter_mut() {
                let target = item
                    .as_table_mut()
                    .ok_or("unsupported Cargo target configuration")?;
                for key in ["linker", "runner"] {
                    if let Some(item) = target.get_mut(key) {
                        let name = item.as_str().ok_or(
                            "portable Cargo runner/linker must be one declared executable",
                        )?;
                        *item = toml_edit::value(self.declared_tool_at(root, base, name)?);
                    }
                }
            }
        }
        Ok(digest(
            &serde_json::to_vec(
                &json!({"configuration":document.to_string(),"implicit_tools":implicit_tools}),
            )
            .map_err(|e| e.to_string())?,
        ))
    }

    pub(super) fn validate(
        &self,
        programs: impl Iterator<Item = impl AsRef<str>>,
    ) -> Result<(), String> {
        if self.tools.is_empty()
            || self.tools.iter().any(|t| t.trim().is_empty())
            || self.tools.iter().collect::<BTreeSet<_>>().len() != self.tools.len()
        {
            return Err("portable environment requires unique explicit tools".into());
        }
        for program in programs {
            if !self.tools.iter().any(|t| t == program.as_ref()) {
                return Err(format!(
                    "portable environment must declare check tool: {}",
                    program.as_ref()
                ));
            }
        }
        for name in &self.variables {
            if name.is_empty()
                || secret_name(name)
                || host_variable(name)
                || name
                    .bytes()
                    .any(|b| !b.is_ascii_alphanumeric() && b != b'_')
            {
                return Err(format!(
                    "unsupported portable environment variable name: {name}"
                ));
            }
        }
        if self
            .inputs
            .iter()
            .any(|(role, path)| role.is_empty() || path.is_empty() || secret_name(role))
        {
            return Err(
                "environment inputs require non-secret stable roles and explicit locations".into(),
            );
        }
        Ok(())
    }

    pub(super) fn snapshot(
        &self,
        root: &Path,
        capture: &Capture,
    ) -> Result<BTreeMap<String, String>, String> {
        self.snapshot_with(
            root,
            &capture.tool_identities,
            capture.portable_python.as_deref(),
        )
    }

    fn snapshot_with(
        &self,
        root: &Path,
        tool_identities: &BTreeMap<String, String>,
        python: Option<&str>,
    ) -> Result<BTreeMap<String, String>, String> {
        let _profile = crate::release_metrics::span("evidence.portable_environment");
        let values = environment()?;
        let path_variables = self.path_variables();
        let mut facts = BTreeMap::from([
            (
                "platform".into(),
                format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
            ),
            (
                "contract".into(),
                digest(&serde_json::to_vec(self).map_err(|e| e.to_string())?),
            ),
        ]);
        for name in values
            .keys()
            .filter(|name| build_variable(name))
            .chain(self.variables.iter())
        {
            if path_variables.contains(name) {
                continue;
            }
            let value = variable(&values, name);
            if name.to_ascii_uppercase().contains("FLAGS")
                && value.is_some_and(|v| v.contains("linker=") || v.contains("--sysroot"))
            {
                return Err("portable environment linker/sysroot flags are not modeled; declare the effective toolchain".into());
            }
            let identity = if tool_override(name) {
                value
                    .filter(|v| !v.is_empty())
                    .map(|v| self.declared_tool(root, v))
                    .transpose()?
                    .unwrap_or("absent".into())
            } else {
                digest(&serde_json::to_vec(&value).map_err(|e| e.to_string())?)
            };
            facts.insert(format!("variable:{name}"), identity);
        }
        for name in &self.tools {
            facts.insert(
                format!("tool:{name}"),
                bytes_identity(&resolve_tool(root, name)?)?,
            );
        }
        // Rustup shims alone do not identify the selected compiler or Cargo build.
        for (tool, identity) in tool_identities {
            facts.insert(format!("runtime:{tool}"), identity.clone());
        }
        if let Some(identity) = python {
            facts.insert("runtime:python".into(), identity.to_string());
        } else if self.tools.iter().any(|tool| {
            Path::new(tool).file_stem().is_some_and(|n| {
                n.to_string_lossy()
                    .to_ascii_lowercase()
                    .starts_with("python")
            })
        }) {
            return Err(
                "portable Python checks require python_environment runtime and dependencies".into(),
            );
        }
        // Cargo discovers configuration through all ancestor directories plus CARGO_HOME.
        // Preserve order, content and absence, but not the host's absolute directory names.
        let mut configs = Vec::new();
        for ancestor in root.ancestors() {
            for name in ["config", "config.toml"] {
                let path = ancestor.join(".cargo").join(name);
                if path.exists() {
                    configs.push(self.cargo_configuration(root, &path)?);
                }
            }
        }
        let home = variable(&values, "CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                variable(&values, if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .map(|p| PathBuf::from(p).join(".cargo"))
            });
        if let Some(home) = home {
            for name in ["config", "config.toml"] {
                configs.push(self.cargo_configuration(root, &home.join(name))?);
            }
        }
        for configured in &self.cargo_configs {
            let path = location(root, configured, &values)?;
            if !path.is_file() {
                return Err("declared Cargo configuration is missing".into());
            }
            configs.push(self.cargo_configuration(root, &path)?);
        }
        facts.insert(
            "runtime:cargo-config".into(),
            digest(&serde_json::to_vec(&configs).map_err(|e| e.to_string())?),
        );
        for (role, configured) in &self.inputs {
            facts.insert(
                format!("input:{role}"),
                input_identity(&location(root, configured, &values)?)?,
            );
        }
        Ok(facts)
    }

    pub(super) fn configure_process(
        &self,
        root: &Path,
        request: &mut ProcessRequest,
    ) -> Result<(), String> {
        request.program = resolve_tool(root, &request.program.to_string_lossy())?.into_os_string();
        let declared = self
            .variables
            .iter()
            .map(|v| {
                if cfg!(windows) {
                    v.to_ascii_uppercase()
                } else {
                    v.clone()
                }
            })
            .chain(self.path_variables())
            .collect::<BTreeSet<_>>();
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            let normalized = if cfg!(windows) {
                name.to_ascii_uppercase()
            } else {
                name.into_owned()
            };
            if !host_variable(&normalized)
                && !build_variable(&normalized)
                && !declared.contains(&normalized)
            {
                request.env_remove.push(key);
            }
        }
        // The contract does not depend on terminal locale or colour defaults.
        for (name, value) in [
            ("LANG", "C"),
            ("LC_ALL", "C"),
            ("NO_COLOR", "1"),
            ("TERM", "dumb"),
        ] {
            if !declared.contains(name) {
                request
                    .env
                    .insert(OsString::from(name), OsString::from(value));
            }
        }
        Ok(())
    }
}
