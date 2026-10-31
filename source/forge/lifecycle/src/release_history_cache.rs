//! One analysis only. Mutable refs and worktree queries always reach Git.
use super::*;
use crate::ProcessOutput;
use std::cell::RefCell;
use std::rc::Rc;

type Tree = BTreeMap<Vec<u8>, (Vec<u8>, String)>;

pub(super) fn state(root: &Path, runner: &dyn ProcessRunner) -> Result<(String, bool), String> {
    let custom_base = std::env::var_os("GIT_REPLACE_REF_BASE");
    let base = custom_base
        .as_deref()
        .map(|value| value.to_str().ok_or("non-UTF8 replacement ref base"))
        .transpose()?
        .unwrap_or("refs/replace");
    let replacements = history::git(
        root,
        runner,
        &["for-each-ref", "--format=%(refname):%(objectname)", base],
    )?;
    let graft_path = match std::env::var_os("GIT_GRAFT_FILE") {
        Some(path) => {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        }
        None => PathBuf::from(
            history::git(
                root,
                runner,
                &[
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-path",
                    "info/grafts",
                ],
            )?
            .trim(),
        ),
    };
    let grafts = release_input(&graft_path)?;
    let enabled = replacements.trim().is_empty()
        && grafts.is_none()
        && custom_base.is_none()
        && std::env::var_os("GIT_GRAFT_FILE").is_none();
    let state = crate::manifest::canonical_sha(
        &json!({"replacements":replacements,"grafts":grafts.map(|bytes|digest(&bytes)),"custom_base":custom_base.map(|s|digest(s.as_encoded_bytes())),"no_replace":std::env::var_os("GIT_NO_REPLACE_OBJECTS").map(|s|digest(s.as_encoded_bytes()))}),
    )?;
    Ok((state, enabled))
}

pub(super) struct Reader<'a> {
    root: &'a Path,
    runner: &'a dyn ProcessRunner,
    trees: RefCell<BTreeMap<String, Rc<Tree>>>,
    payloads: RefCell<BTreeMap<(String, String), Option<Vec<u8>>>>,
    blobs: RefCell<BTreeMap<String, Vec<u8>>>,
    initial_state: Option<String>,
    enabled: bool,
}

fn immutable(revision: &str) -> bool {
    matches!(revision.len(), 40 | 64) && revision.bytes().all(|b| b.is_ascii_hexdigit())
}

impl<'a> Reader<'a> {
    #[cfg(all(test, bridgeforge_factory_tests))]
    pub(super) fn uncached(root: &'a Path, runner: &'a dyn ProcessRunner) -> Self {
        let mut reader = Self::new(root, runner);
        reader.enabled = false;
        reader
    }

    pub(super) fn new(root: &'a Path, runner: &'a dyn ProcessRunner) -> Self {
        let state = state(root, runner).ok();
        Self {
            root,
            runner,
            trees: RefCell::new(BTreeMap::new()),
            payloads: RefCell::new(BTreeMap::new()),
            blobs: RefCell::new(BTreeMap::new()),
            initial_state: state.as_ref().map(|(state, _)| state.clone()),
            enabled: state.is_some_and(|(_, enabled)| enabled),
        }
    }

    pub(super) fn verify_unchanged(&self) -> Result<(), String> {
        if state(self.root, self.runner)?.0
            != self
                .initial_state
                .clone()
                .ok_or("cannot capture release history interpretation")?
        {
            return Err(
                "Git history interpretation changed during release planning; no apply".into(),
            );
        }
        Ok(())
    }

    /// Speculation must not add failures or trigger a partial clone's lazy fetch.
    /// Required reads still use prefetch/payload and propagate their own errors.
    pub(super) fn prefetch_hints(&self, revisions: &BTreeSet<&str>, paths: &[String]) {
        if !self.enabled {
            return;
        }
        let mut request = ProcessRequest::new("git", self.root);
        request.args = [
            "config",
            "--get-regexp",
            "^(extensions\\.partialclone|remote\\..*\\.promisor)$",
        ]
        .iter()
        .map(Into::into)
        .collect();
        let Ok(output) = self.runner.run(&request) else {
            return;
        };
        // Exit 1 with no output means no promisor configuration. Unknown state
        // conservatively disables the optimization, not ordinary verification.
        if output.timed_out || output.code != 1 || !output.stdout.is_empty() {
            return;
        }
        for revision in revisions {
            let _ = self.prefetch(revision, paths);
        }
    }

    fn tree(&self, revision: &str) -> Result<Rc<Tree>, String> {
        if let Some(tree) = self.trees.borrow().get(revision) {
            return Ok(tree.clone());
        }
        let mut request = ProcessRequest::new("git", self.root);
        request.args = ["ls-tree", "-r", "-z", revision]
            .iter()
            .map(std::ffi::OsString::from)
            .collect();
        let output = self.runner.run(&request).map_err(|e| e.to_string())?;
        if output.timed_out || output.code != 0 {
            return Err("cannot read immutable history tree".into());
        }
        let mut tree = Tree::new();
        for entry in output
            .stdout
            .split(|b| *b == 0)
            .filter(|entry| !entry.is_empty())
        {
            let tab = entry
                .iter()
                .position(|b| *b == b'\t')
                .ok_or("malformed history tree")?;
            let header = std::str::from_utf8(&entry[..tab]).map_err(|e| e.to_string())?;
            let parts = header.split_whitespace().collect::<Vec<_>>();
            if parts.len() != 3 {
                return Err("malformed history tree header".into());
            }
            let path = entry[tab + 1..].to_vec();
            tree.insert(path, (entry[..tab].to_vec(), parts[2].into()));
        }
        let tree = Rc::new(tree);
        self.trees
            .borrow_mut()
            .insert(revision.into(), tree.clone());
        Ok(tree)
    }

    pub(super) fn prefetch(&self, revision: &str, paths: &[String]) -> Result<(), String> {
        if !self.enabled || !immutable(revision) {
            return Ok(());
        }
        let tree = self.tree(revision)?;
        let mut pending = BTreeMap::<String, Vec<(String, String)>>::new();
        for path in paths.iter().collect::<BTreeSet<_>>() {
            let key = (revision.to_string(), path.clone());
            if self.payloads.borrow().contains_key(&key) {
                continue;
            }
            match tree.get(path.as_bytes()) {
                Some((header, oid)) => {
                    if !header.split(|b| *b == b' ').any(|part| part == b"blob") {
                        continue;
                    }
                    if let Some(bytes) = self.blobs.borrow().get(oid) {
                        self.payloads.borrow_mut().insert(key, Some(bytes.clone()));
                    } else {
                        pending.entry(oid.clone()).or_default().push(key);
                    }
                }
                None => {
                    self.payloads.borrow_mut().insert(key, None);
                }
            }
        }
        if pending.is_empty() {
            return Ok(());
        }
        let mut request = ProcessRequest::new("git", self.root);
        request.args = vec!["cat-file".into(), "--batch".into()];
        request.stdin = pending
            .keys()
            .map(|oid| format!("{oid}\n"))
            .collect::<String>()
            .into_bytes();
        let output = self.runner.run(&request).map_err(|e| e.to_string())?;
        if output.timed_out || output.code != 0 {
            return Err("cannot read immutable history blobs".into());
        }
        let mut rest = output.stdout.as_slice();
        let mut loaded = Vec::new();
        let mut blobs = Vec::new();
        for (oid, keys) in pending {
            let newline = rest
                .iter()
                .position(|b| *b == b'\n')
                .ok_or("truncated history batch header")?;
            let header = std::str::from_utf8(&rest[..newline])
                .map_err(|e| e.to_string())?
                .split_whitespace()
                .collect::<Vec<_>>();
            if header.len() != 3 || header[0] != oid || header[1] != "blob" {
                return Err("invalid history batch identity".into());
            }
            let size = header[2].parse::<usize>().map_err(|e| e.to_string())?;
            rest = &rest[newline + 1..];
            if size >= rest.len() || rest[size] != b'\n' {
                return Err("truncated history batch payload".into());
            }
            let bytes = rest[..size].to_vec();
            for key in keys {
                loaded.push((key, Some(bytes.clone())));
            }
            blobs.push((oid, bytes));
            rest = &rest[size + 1..];
        }
        if !rest.is_empty() {
            return Err("unexpected history batch trailer".into());
        }
        self.payloads.borrow_mut().extend(loaded);
        self.blobs.borrow_mut().extend(blobs);
        Ok(())
    }

    fn payload(&self, revision: &str, path: &str) -> Result<Option<Vec<u8>>, String> {
        if self
            .tree(revision)?
            .get(path.as_bytes())
            .is_some_and(|(header, _)| !header.split(|b| *b == b' ').any(|part| part == b"blob"))
        {
            return Err("history payload is not a blob".into());
        }
        self.prefetch(revision, &[path.to_string()])?;
        Ok(self
            .payloads
            .borrow()
            .get(&(revision.into(), path.into()))
            .cloned()
            .flatten())
    }
}

impl ProcessRunner for Reader<'_> {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        if !self.enabled
            || request.program != "git"
            || request.cwd != self.root
            || request
                .env
                .iter()
                .any(|(key, value)| key != "GIT_OPTIONAL_LOCKS" || value != "0")
            || !request.env_remove.is_empty()
            || !request.stdin.is_empty()
        {
            return self.runner.run(request);
        }
        let args = request
            .args
            .iter()
            .map(|arg| arg.to_str())
            .collect::<Option<Vec<_>>>();
        let Some(mut args) = args.as_deref() else {
            return self.runner.run(request);
        };
        // Read-only inspection decorations do not change immutable object bytes.
        // Unknown options/environment still bypass the cache and reach Git.
        while args.len() >= 2
            && args[0] == "-c"
            && (args[1] == format!("safe.directory={}", self.root.display())
                || matches!(
                    args[1],
                    "core.fsmonitor=false" | "diff.autoRefreshIndex=false"
                ))
        {
            args = &args[2..];
        }
        let value = (|| -> Result<Option<Vec<u8>>, String> {
            match args {
                ["ls-tree", "-z", revision, "--", path] if immutable(revision) => Ok(Some(
                    self.tree(revision)?
                        .get(path.as_bytes())
                        .map(|(header, _)| {
                            [header.as_slice(), b"\t", path.as_bytes(), b"\0"].concat()
                        })
                        .unwrap_or_default(),
                )),
                ["ls-tree", "-r", "--name-only", "-z", revision] if immutable(revision) => {
                    let tree = self.tree(revision)?;
                    Ok(Some(
                        tree.keys()
                            .flat_map(|path| path.iter().copied().chain(std::iter::once(0)))
                            .collect(),
                    ))
                }
                ["show", spec]
                    if spec
                        .split_once(':')
                        .is_some_and(|(revision, _)| immutable(revision)) =>
                {
                    let (revision, path) = spec.split_once(':').unwrap();
                    self.payload(revision, path)?
                        .map(Some)
                        .ok_or_else(|| format!("history payload missing: {path}"))
                }
                _ => Ok(None),
            }
        })()
        .map_err(std::io::Error::other)?;
        match value {
            Some(stdout) => Ok(ProcessOutput {
                code: 0,
                stdout,
                stderr: Vec::new(),
                timed_out: false,
            }),
            None => self.runner.run(request),
        }
    }
}
