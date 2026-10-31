//! Project artifact locations. Producers, consumers and layout checks share this
//! contract; user-level installations have a separate scope.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CONTRACT: &str = "three-directory-layout-v1";
pub const RUNTIME: &str = ".runtime";
pub const DEPS: &str = ".deps";
pub const BUILD: &str = ".build";
pub const CONFIG: &str = ".runtime/config";
pub const DOWNLOAD: &str = ".runtime/download";
pub const TESTS: &str = ".runtime/tests";
pub const TEST_RECEIPTS: &str = ".runtime/tests/test-receipts";
pub const LEGACY_TEST_RECEIPTS: &str = ".runtime/test_receipts";
pub const FACTORY_BIN: &str = ".build/bridgeforge/bin";
pub const PROJECT_BIN: &str = ".deps/bridgeforge/bin";
pub const BUILD_CACHE: &str = ".build/bridgeforge/artifact-cache";
pub const CARGO: &str = ".build/bridgeforge/cargo";
pub const IGNORE_RULES: &str = "/.runtime/\n/.deps/\n/.build/\n";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Runtime,
    Config,
    Download,
    Test,
    Dependency,
    Build,
}

pub fn normalized(path: &str) -> String {
    let path = path.replace('\\', "/");
    if cfg!(windows) {
        path.to_lowercase()
    } else {
        path
    }
}

pub fn within(path: &str, root: &str) -> bool {
    let path = normalized(path);
    let root = normalized(root).trim_end_matches('/').to_owned();
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|tail| tail.starts_with('/'))
}

pub fn local(path: &str) -> bool {
    [RUNTIME, DEPS, BUILD].iter().any(|root| within(path, root))
}

impl Kind {
    pub fn root(self) -> &'static str {
        match self {
            Self::Runtime => RUNTIME,
            Self::Config => CONFIG,
            Self::Download => DOWNLOAD,
            Self::Test => TESTS,
            Self::Dependency => DEPS,
            Self::Build => BUILD,
        }
    }

    pub fn accepts(self, path: &str, directory: bool) -> bool {
        if !within(path, self.root()) {
            return false;
        }
        let path = normalized(path);
        let parts: Vec<_> = path.trim_end_matches('/').split('/').collect();
        match self {
            Self::Test => parts.len() >= if directory { 3 } else { 4 },
            // The broad runtime class cannot bypass more specific fixed roots.
            Self::Runtime => ![CONFIG, DOWNLOAD, TESTS].iter().any(|p| within(&path, p)),
            _ => true,
        }
    }
}

pub fn bin_dir(factory: bool) -> &'static str {
    if factory { FACTORY_BIN } else { PROJECT_BIN }
}

pub fn binary(root: &Path, factory: bool, name: &str) -> PathBuf {
    root.join(bin_dir(factory)).join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    })
}

/// Stable two-group build slots. Never derive a path from an arbitrary asset id.
pub fn cargo_slot(root: &Path, factory: bool) -> PathBuf {
    root.join(CARGO)
        .join(if factory { "factory" } else { "public" })
}

pub fn temporary_build(root: &Path, purpose: &str) -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if purpose.is_empty()
        || !purpose
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("invalid build directory purpose".into());
    }
    let base =
        crate::managed_paths::safe_join(root, ".build/bridgeforge/transient", "temporary build")?;
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = base.join(format!(
        "{purpose}-{:x}-{:08x}-{:x}",
        std::process::id(),
        nonce as u32,
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
    Ok(directory)
}

pub fn glob_regex(pattern: &str) -> Result<regex::Regex, String> {
    let chars: Vec<_> = normalized(pattern).chars().collect();
    let mut expression = String::from("^");
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '*' if chars.get(index + 1) == Some(&'*') => {
                index += 1;
                if chars.get(index + 1) == Some(&'/') {
                    expression.push_str("(?:[^/]+/)*");
                    index += 1;
                } else {
                    expression.push_str(".*");
                }
            }
            '*' => expression.push_str("[^/]*"),
            '?' => expression.push_str("[^/]"),
            '[' => {
                expression.push('[');
                index += 1;
                if chars.get(index) == Some(&'!') {
                    expression.push('^');
                    index += 1;
                }
                let start = index;
                if chars.get(index) == Some(&']') {
                    expression.push_str("\\]");
                    index += 1;
                }
                while index < chars.len() && chars[index] != ']' {
                    if chars[index] == '/' {
                        return Err("Cargo glob class must not contain a separator".into());
                    }
                    if matches!(chars[index], '[' | '\\' | '^') {
                        expression.push('\\');
                    }
                    expression.push(chars[index]);
                    index += 1;
                }
                if index == start || chars.get(index) != Some(&']') {
                    return Err("invalid Cargo member glob class".into());
                }
                expression.push(']');
            }
            value => expression.push_str(&regex::escape(&value.to_string())),
        }
        index += 1;
    }
    expression.push('$');
    regex::Regex::new(&expression).map_err(|e| format!("invalid Cargo member glob: {e}"))
}

pub fn ignore_problems(bytes: Option<&[u8]>) -> Result<Vec<String>, String> {
    let text = std::str::from_utf8(bytes.unwrap_or_default()).map_err(|e| e.to_string())?;
    let mut errors = Vec::new();
    for root in [RUNTIME, DEPS, BUILD] {
        let mut explicit = false;
        let mut ignored = false;
        for raw in text.lines() {
            let line = raw.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (negative, pattern) = line.strip_prefix('!').map_or((false, line), |p| (true, p));
            let mut unescaped = String::new();
            let mut chars = pattern.chars();
            let mut cannot_match_root = false;
            while let Some(c) = chars.next() {
                if c == '\\' {
                    match chars.next() {
                        // Literal metacharacters/spaces cannot occur in any of
                        // our three fixed root names. Do not reinterpret them.
                        Some(c) if !"*?[]\\ ".contains(c) => unescaped.push(c),
                        _ => {
                            cannot_match_root = true;
                            break;
                        }
                    }
                } else {
                    unescaped.push(c);
                }
            }
            if cannot_match_root {
                continue;
            }
            let pattern = unescaped.trim_start_matches('/').trim_end_matches('/');
            if pattern == root && !negative {
                explicit = true;
            }
            if glob_regex(pattern)?.is_match(&normalized(root)) {
                ignored = !negative;
            }
        }
        if !explicit || !ignored {
            errors.push(format!("project .gitignore must exclude /{root}/ and all contents without re-including the root"));
        }
    }
    Ok(errors)
}
