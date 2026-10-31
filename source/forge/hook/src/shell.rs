//! Conservative recognition of the supported command subset, not a Shell evaluator.
//! Quoted data is never searched for command names. Unknown programs remain the
//! platform's responsibility; recognized writes with opaque operands fail closed.
use std::path::{Path, PathBuf};

use crate::util::normalize_path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    PowerShell,
    Posix,
}

pub fn dialect(payload: &serde_json::Value) -> Dialect {
    let explicit = crate::util::tool_input(payload)
        .and_then(|input| input.get("shell"))
        .and_then(serde_json::Value::as_str)
        .map(program);
    match explicit.as_deref() {
        Some("bash" | "sh" | "zsh") => Dialect::Posix,
        Some("pwsh" | "powershell") => Dialect::PowerShell,
        _ if crate::util::tool_name(payload) == "Bash" => Dialect::Posix,
        _ if crate::util::tool_name(payload) == "PowerShell" || cfg!(windows) => {
            Dialect::PowerShell
        }
        _ => Dialect::Posix,
    }
}

#[derive(Clone, Debug, Default)]
pub struct Word {
    pub text: String,
    pub dynamic: bool,
    // Only a leading quote makes a PowerShell expression; embedded quotes are
    // also valid in command names such as Get-'Content'.
    pub quoted: bool,
}

#[derive(Debug, Default)]
pub struct Command {
    pub words: Vec<Word>,
    pub redirects: Vec<Word>,
    pub uncertain: bool,
    pub invoked: bool,
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub commands: Vec<Command>,
    pub compound: bool,
}

pub fn program(word: &str) -> String {
    let base = word
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(word)
        .to_ascii_lowercase();
    base.strip_suffix(".exe").unwrap_or(&base).to_string()
}

pub fn parse(text: &str) -> Parsed {
    let chars: Vec<char> = text.chars().collect();
    let mut parsed = Parsed::default();
    let mut command = Command::default();
    let mut word = Word::default();
    let mut started = false;
    let mut quote = None;
    let mut redirect = false;
    let mut i = 0;
    let flush =
        |command: &mut Command, word: &mut Word, started: &mut bool, redirect: &mut bool| {
            if *started {
                if *redirect {
                    command.redirects.push(std::mem::take(word));
                    *redirect = false;
                } else {
                    command.words.push(std::mem::take(word));
                }
                *started = false;
            }
        };
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == q {
                if q == '\'' && chars.get(i + 1) == Some(&'\'') {
                    word.text.push('\'');
                    i += 2;
                    continue;
                }
                quote = None;
            } else if c == '`' && q == '"' {
                // Backtick expansion/escaping is intentionally not evaluated.
                word.dynamic = true;
                if let Some(next) = chars.get(i + 1) {
                    word.text.push(*next);
                    i += 1;
                }
            } else {
                word.text.push(c);
                if q == '"' && (c == '$' || c == '`') {
                    word.dynamic = true;
                }
            }
            i += 1;
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            if !started {
                word.quoted = true;
            }
            started = true;
        } else if c == '#' && !started {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '<' && chars.get(i + 1) == Some(&'#') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '#' && chars[i + 1] == '>') {
                i += 1;
            }
            if i + 1 >= chars.len() {
                command.uncertain = true;
                break;
            }
            i += 2;
            continue;
        } else if c == '>' {
            // File descriptor prefixes and merges are not path operands.
            if started && (word.text.chars().all(|c| c.is_ascii_digit()) || word.text == "*") {
                word = Word::default();
                started = false;
            }
            flush(&mut command, &mut word, &mut started, &mut redirect);
            if redirect {
                command.uncertain = true;
            }
            if chars.get(i + 1) == Some(&'>') {
                i += 1;
            }
            if chars.get(i + 1) == Some(&'&') {
                i += 2;
                let begin = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i == begin {
                    command.uncertain = true;
                }
                continue;
            }
            redirect = true;
        } else if matches!(c, ';' | '|' | '\n' | '&') {
            // PowerShell's leading call operator is not a second command.
            if c == '&'
                && !started
                && command.words.is_empty()
                && !redirect
                && chars.get(i + 1) != Some(&'&')
            {
                command.invoked = true;
                i += 1;
                continue;
            }
            flush(&mut command, &mut word, &mut started, &mut redirect);
            if redirect {
                command.uncertain = true;
            }
            if !command.words.is_empty() || !command.redirects.is_empty() {
                parsed.commands.push(std::mem::take(&mut command));
            }
            if c != '\n' && c != ';' {
                parsed.compound = true;
            }
            redirect = false;
            if chars.get(i + 1) == Some(&c) && (c == '&' || c == '|') {
                i += 1;
            }
        } else if c.is_whitespace() {
            flush(&mut command, &mut word, &mut started, &mut redirect);
        } else {
            started = true;
            word.text.push(c);
            if matches!(c, '$' | '`' | '(' | ')' | '{' | '}' | ',' | '<') {
                word.dynamic = true;
            }
        }
        i += 1;
    }
    flush(&mut command, &mut word, &mut started, &mut redirect);
    if quote.is_some() || redirect {
        command.uncertain = true;
    }
    if redirect {
        command.redirects.push(Word {
            text: String::new(),
            dynamic: true,
            quoted: false,
        });
    }
    if !command.words.is_empty() || !command.redirects.is_empty() || command.uncertain {
        parsed.commands.push(command);
    }
    parsed
}

fn path(base: &Path, word: &Word) -> Result<PathBuf, String> {
    if word.dynamic
        || word.text.is_empty()
        || word.text.contains(['*', '?', '[', ']', '%'])
        || word.text.starts_with('~')
    {
        return Err(format!("unresolved path operand: {}", word.text));
    }
    // Drive-relative paths and providers are not filesystem absolute paths.
    if let Some((prefix, suffix)) = word.text.split_once(':') {
        if prefix.len() != 1
            || !prefix.as_bytes()[0].is_ascii_alphabetic()
            || !suffix.starts_with(['/', '\\'])
        {
            return Err(format!(
                "unsupported path provider or drive-relative path: {}",
                word.text
            ));
        }
    }
    Ok(normalize_path(base, &word.text))
}

pub fn is_write(name: &str) -> bool {
    matches!(
        name,
        "set-content"
            | "add-content"
            | "out-file"
            | "tee-object"
            | "tee"
            | "new-item"
            | "copy-item"
            | "move-item"
            | "rename-item"
            | "remove-item"
            | "del"
            | "erase"
            | "rm"
            | "mv"
            | "move"
            | "cp"
            | "copy"
    )
}

// Extract PowerShell named operands without mistaking option values for paths.
fn operands(command: &Command) -> Result<(Vec<Word>, Vec<(String, Word)>), String> {
    let mut positionals = Vec::new();
    let mut named = Vec::new();
    let mut i = 1;
    let mut positional_only = false;
    while i < command.words.len() {
        let word = &command.words[i];
        let key = word.text.to_ascii_lowercase();
        if key == "--" {
            positional_only = true;
            i += 1;
            continue;
        }
        if !positional_only && key.starts_with('-') {
            if matches!(
                key.as_str(),
                "-force"
                    | "-recurse"
                    | "-append"
                    | "-noclobber"
                    | "-nonewline"
                    | "-passthru"
                    | "-confirm:$false"
                    | "-whatif"
                    | "-r"
                    | "-f"
                    | "-rf"
                    | "-fr"
                    | "-v"
            ) {
                i += 1;
                continue;
            }
            if matches!(
                key.as_str(),
                "-path"
                    | "-literalpath"
                    | "-destination"
                    | "-newname"
                    | "-name"
                    | "-filepath"
                    | "-value"
                    | "-encoding"
                    | "-itemtype"
                    | "-erroraction"
                    | "-ea"
            ) {
                let value = command.words.get(i + 1).ok_or("missing option value")?;
                named.push((key, value.clone()));
                i += 2;
                continue;
            }
            return Err(format!("unsupported write option: {}", word.text));
        }
        positionals.push(word.clone());
        i += 1;
    }
    Ok((positionals, named))
}

pub fn write_targets(text: &str, cwd: &Path, dialect: Dialect) -> Result<Vec<PathBuf>, String> {
    write_targets_inner(text, cwd, dialect, 0)
}

fn write_targets_inner(
    text: &str,
    cwd: &Path,
    dialect: Dialect,
    depth: usize,
) -> Result<Vec<PathBuf>, String> {
    if depth > 3 {
        return Err("nested shell command exceeds supported depth".into());
    }
    let parsed = parse(text);
    let base = cwd.to_path_buf();
    let mut targets = Vec::new();
    let mut location_uncertain = false;
    for command in &parsed.commands {
        let quoted_data = dialect == Dialect::PowerShell
            && !command.invoked
            && command.words.first().is_some_and(|word| word.quoted);
        let name = command
            .words
            .first()
            .map(|w| program(&w.text))
            .unwrap_or_default();
        if command.uncertain && ((!quoted_data && is_write(&name)) || !command.redirects.is_empty())
        {
            return Err("incomplete or unsupported write syntax".into());
        }
        for target in &command.redirects {
            if location_uncertain {
                return Err("working directory cannot be resolved".into());
            }
            targets.push(path(&base, target)?);
        }
        if quoted_data {
            continue;
        }
        if matches!(
            name.as_str(),
            "cd" | "chdir" | "set-location" | "push-location" | "pop-location" | "pushd" | "popd"
        ) {
            // Conditional/pipeline location changes cannot be evaluated statically.
            location_uncertain = true;
            continue;
        }
        if matches!(name.as_str(), "pwsh" | "powershell" | "bash" | "sh") {
            if let Some(index) = command
                .words
                .iter()
                .position(|w| matches!(w.text.to_ascii_lowercase().as_str(), "-command" | "-c"))
            {
                if let Some(script) = command.words.get(index + 1) {
                    if location_uncertain {
                        return Err(
                            "nested Shell follows an unresolved working-directory change".into(),
                        );
                    }
                    if script.dynamic || command.words.len() != index + 2 {
                        return Err("dynamic shell command cannot be resolved; use a separate direct command".into());
                    }
                    let nested_dialect = if matches!(name.as_str(), "bash" | "sh") {
                        Dialect::Posix
                    } else {
                        Dialect::PowerShell
                    };
                    targets.extend(write_targets_inner(
                        &script.text,
                        &base,
                        nested_dialect,
                        depth + 1,
                    )?);
                }
            }
            continue;
        }
        if name == "git" {
            let mut git_base = base.clone();
            let mut index = 1;
            let mut unknown = false;
            while index < command.words.len() {
                let item = &command.words[index];
                if item.text == "-C" {
                    index += 1;
                    git_base = path(
                        &git_base,
                        command.words.get(index).ok_or("missing git -C path")?,
                    )?;
                } else if item.text == "-c" || item.text == "--config-env" {
                    index += 1;
                    let value = command
                        .words
                        .get(index)
                        .ok_or("missing Git configuration value")?;
                    // Arbitrary Git configuration may redirect the index or invoke
                    // helpers. Do not infer a write scope from it.
                    unknown = true;
                    if value.dynamic {
                        return Err("dynamic Git configuration".into());
                    }
                } else if item.text == "--git-dir" || item.text == "--work-tree" {
                    index += 1;
                    command
                        .words
                        .get(index)
                        .ok_or("missing Git directory value")?;
                    unknown = true;
                } else if item.text.starts_with('-') {
                    if matches!(item.text.as_str(), "--version" | "--help") {
                        break;
                    }
                    if item.text.starts_with("--git-dir=")
                        || item.text.starts_with("--work-tree=")
                        || item.text.starts_with("-c")
                    {
                        unknown = true;
                    } else if !matches!(
                        item.text.as_str(),
                        "--no-optional-locks" | "--no-pager" | "--literal-pathspecs"
                    ) {
                        return Err(
                            "unsupported Git context option; use an explicit project command"
                                .into(),
                        );
                    }
                } else {
                    if matches!(
                        item.text.as_str(),
                        "add"
                            | "commit"
                            | "restore"
                            | "reset"
                            | "push"
                            | "checkout"
                            | "merge"
                            | "cherry-pick"
                            | "clean"
                            | "branch"
                            | "tag"
                            | "update-ref"
                            | "stash"
                            | "pull"
                            | "fetch"
                            | "switch"
                    ) {
                        if unknown || location_uncertain {
                            return Err("Git write context cannot be resolved".into());
                        }
                        targets.push(git_base.clone());
                    }
                    break;
                }
                index += 1;
            }
            continue;
        }
        if !is_write(&name) {
            continue;
        }
        if location_uncertain {
            return Err("write after a Shell location change; split commands and set tool working directory".into());
        }
        if command.words[0].dynamic {
            return Err("dynamic write program".into());
        }
        let (pos, named) = operands(command)?;
        let find = |names: &[&str]| -> Result<Option<Word>, String> {
            let values: Vec<_> = named
                .iter()
                .filter(|(key, _)| names.contains(&key.as_str()))
                .collect();
            if values.len() > 1 {
                return Err("duplicate or conflicting path parameters".into());
            }
            Ok(values.first().map(|(_, word)| word.clone()))
        };
        let source_named = find(&["-path", "-literalpath", "-filepath"])?;
        let source = source_named.clone().or_else(|| pos.first().cloned());
        if matches!(
            name.as_str(),
            "copy-item" | "copy" | "cp" | "move-item" | "move" | "mv" | "rename-item"
        ) {
            let destination_named = find(&["-destination", "-newname"])?;
            let offset = usize::from(source_named.is_none());
            let destination = destination_named
                .or_else(|| pos.get(offset).cloned())
                .ok_or("missing destination; specify it explicitly")?;
            if pos.len() > offset + 1 {
                return Err("multiple source operands require separate commands".into());
            }
            if name == "rename-item" {
                let source = path(&base, &source.ok_or("missing source path")?)?;
                // Rename-Item's new name is relative to the source parent.
                targets.push(path(
                    source.parent().ok_or("source has no parent")?,
                    &destination,
                )?);
                targets.push(source);
            } else {
                targets.push(path(&base, &destination)?);
                if matches!(name.as_str(), "move-item" | "move" | "mv") {
                    targets.push(path(&base, &source.ok_or("missing source path")?)?);
                }
            }
        } else if name == "new-item" {
            let target = path(&base, &source.ok_or("missing destination path")?)?;
            if let Some(name) = find(&["-name"])? {
                targets.push(path(&target, &name)?);
            } else {
                targets.push(target);
            }
            if let Some(kind) = find(&["-itemtype"])? {
                if kind.dynamic
                    || !matches!(
                        kind.text.to_ascii_lowercase().as_str(),
                        "file" | "directory"
                    )
                {
                    return Err(
                        "link/provider creation requires its dedicated authorized tool".into(),
                    );
                }
            }
        } else {
            targets.push(path(&base, &source.ok_or("missing write path")?)?);
            if matches!(
                name.as_str(),
                "rm" | "del" | "erase" | "remove-item" | "tee"
            ) {
                for extra in pos.iter().skip(usize::from(source_named.is_none())) {
                    targets.push(path(&base, extra)?);
                }
            }
        }
    }
    Ok(targets)
}

/// Only a directly recognized process can lend its Shell exit status to a receipt.
pub fn test_command(text: &str, dialect: Dialect) -> Option<(&'static str, bool)> {
    let parsed = parse(text);
    for command in &parsed.commands {
        let words = &command.words;
        let first = words.first()?;
        if first.dynamic || (dialect == Dialect::PowerShell && first.quoted && !command.invoked) {
            continue;
        }
        let name = program(&first.text);
        let arg = |i: usize, text: &str| words.get(i).is_some_and(|w| !w.dynamic && w.text == text);
        let kind = match name.as_str() {
            "pytest" => Some("pytest"),
            "python" | "python3" | "py" if arg(1, "-m") && arg(2, "pytest") => Some("pytest"),
            "cargo"
                if arg(1, "test")
                    || (words
                        .get(1)
                        .is_some_and(|w| w.text.starts_with('+') && !w.dynamic)
                        && arg(2, "test")) =>
            {
                Some("cargo")
            }
            "npm" if arg(1, "test") || (arg(1, "run") && arg(2, "test")) => Some("npm"),
            "go" if arg(1, "test") => Some("go"),
            "tsc" => Some("tsc"),
            "make" => Some("make"),
            _ => None,
        };
        if let Some(kind) = kind {
            return Some((
                kind,
                parsed.commands.len() == 1
                    && !parsed.compound
                    && (dialect == Dialect::PowerShell || !command.invoked)
                    && !command.uncertain
                    && command.redirects.is_empty()
                    && !words.iter().any(|w| w.dynamic),
            ));
        }
    }
    None
}
