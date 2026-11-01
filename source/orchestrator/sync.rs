//! Private archive transport. Git ancestry and archive ancestry are separate
//! gates. No checkout of remote files, no force push and no automatic merge.
use crate::{
    Result, database, digest,
    store::{Head, Snapshot, Store, ordinary, read_json},
};
use bridgeforge_core::{ProcessRequest, ProcessRunner, SystemProcessRunner, persistence};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf, time::Duration};

pub type Files = BTreeMap<String, Vec<u8>>;
const ARCHIVE: &str = "doc/5_project_knowledgebase/bridgepilot/";
fn git_path(path: &std::path::Path) -> Result<String> {
    let text = path.to_str().ok_or("non-UTF8 Git path")?;
    #[cfg(windows)]
    {
        if let Some(rest) = text.strip_prefix("\\\\?\\UNC\\") {
            return Ok(format!("\\\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix("\\\\?\\") {
            return Ok(rest.into());
        }
    }
    Ok(text.into())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub repository: String,
    pub branch: String,
    pub source: String,
}
impl Config {
    pub fn check(&self) -> Result<()> {
        let parts: Vec<_> = self.repository.split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|p| {
                p.is_empty()
                    || !p
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            })
            || self.branch != "main"
            || self.source.trim().is_empty()
        {
            return Err("explicit owner/repository, main branch and user source required".into());
        }
        Ok(())
    }
    fn url(&self) -> String {
        format!("https://github.com/{}.git", self.repository)
    }
    fn reference(&self) -> String {
        format!("refs/heads/{}", self.branch)
    }
}
fn config_path(s: &Store) -> PathBuf {
    s.workspace.join(".runtime/config/bridgepilot-sync.json")
}
pub fn configure(s: &Store, c: Config) -> Result<Value> {
    c.check()?;
    s.read()?;
    let path = config_path(s);
    ordinary(&path)?;
    if path.exists() {
        let old: Config = read_json(&path)?;
        if old.repository != c.repository || old.branch != c.branch {
            return Err("existing archive remote cannot be silently replaced".into());
        }
    }
    persistence::atomic_write_json(&path, &c).map_err(|e| e.to_string())?;
    Ok(
        json!({"status":"archive_remote_configured","repository":c.repository,"branch":c.branch,"archive_uploaded":false}),
    )
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub schema: u32,
    pub candidate: String,
    pub expected_remote: Option<String>,
    pub kind: String,
    pub local_before: Head,
    pub candidate_head: Head,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superseded: Vec<Candidate>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub commit: String,
    pub head: Head,
}

pub struct Engine<'a> {
    pub store: &'a Store,
    pub runner: &'a dyn ProcessRunner,
    pub config: Config,
    cache: PathBuf,
}
impl<'a> Engine<'a> {
    pub fn new(store: &'a Store, runner: &'a dyn ProcessRunner) -> Result<Self> {
        let config: Config = read_json(&config_path(store))?;
        config.check()?;
        let cache = store
            .workspace
            .join(".runtime/bridgepilot/transport/repository.git");
        ordinary(&cache)?;
        Ok(Self {
            store,
            runner,
            config,
            cache,
        })
    }
    fn local(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.store
            .read_connection()?
            .query_row("SELECT value FROM local_records WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())
    }
    fn pending(&self) -> Result<Pending> {
        let bytes = self
            .local("transport.pending")?
            .ok_or("no unresolved transport intent")?;
        let value = bridgeforge_core::json::parse_unique_json(&bytes, "SQLite transport intent")?;
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
    fn has_pending(&self) -> Result<bool> {
        Ok(self.local("transport.pending")?.is_some())
    }
    fn save_pending(&self, pending: &Pending, files: &Files) -> Result<()> {
        let mut conn = self.store.write_connection()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(pending).map_err(|e| e.to_string())?;
        let snapshot = files
            .get(database::FILE)
            .ok_or("new transport intent requires a SQLite snapshot")?;
        tx.execute("INSERT INTO local_records(key,value) VALUES('transport.pending',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[bytes]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO local_records(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![format!("transport.snapshot.{}",pending.candidate),snapshot]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub(crate) fn call(&self, program: &str, args: &[String], stdin: Vec<u8>) -> Result<Vec<u8>> {
        let mut request = ProcessRequest::new(program, &self.store.workspace);
        request.args = args.iter().map(Into::into).collect();
        request.stdin = stdin;
        request.timeout = Duration::from_secs(90);
        request.env.insert("GIT_TERMINAL_PROMPT".into(), "0".into());
        request.env.insert("GH_PROMPT_DISABLED".into(), "1".into());
        let output = self.runner.run(&request).map_err(|e| e.to_string())?;
        if output.timed_out || output.code != 0 {
            return Err(format!(
                "archive transport command failed ({}, timeout {}): {}",
                output.code,
                output.timed_out,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        if output.stdout.len() > database::MAX_BYTES {
            return Err("oversized archive transport output".into());
        }
        Ok(output.stdout)
    }
    fn git(&self, args: &[String], input: Vec<u8>) -> Result<Vec<u8>> {
        let mut all = vec![
            "--no-optional-locks".into(),
            "--git-dir".into(),
            git_path(&self.cache)?,
        ];
        all.extend_from_slice(args);
        self.call("git", &all, input)
    }
    fn text(&self, args: &[String]) -> Result<String> {
        String::from_utf8(self.git(args, Vec::new())?)
            .map(|s| s.trim_end().into())
            .map_err(|e| e.to_string())
    }
    fn prepare(&self) -> Result<()> {
        self.config.check()?;
        let result = self.call(
            "gh",
            &["api".into(), format!("repos/{}", self.config.repository)],
            Vec::new(),
        )?;
        let repo: Value = serde_json::from_slice(&result).map_err(|e| e.to_string())?;
        if repo["private"] != true || repo["full_name"] != self.config.repository {
            return Err("archive remote is not the exact authorized private repository".into());
        }
        if !self.cache.exists() {
            fs::create_dir_all(self.cache.parent().unwrap()).map_err(|e| e.to_string())?;
            self.call(
                "git",
                &["init".into(), "--bare".into(), git_path(&self.cache)?],
                Vec::new(),
            )?;
        }
        Ok(())
    }
    fn remote_oid(&self) -> Result<Option<String>> {
        let output = self.git(
            &[
                "ls-remote".into(),
                "--refs".into(),
                "--".into(),
                self.config.url(),
                self.config.reference(),
            ],
            Vec::new(),
        )?;
        let text = String::from_utf8(output).map_err(|e| e.to_string())?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        let rows: Vec<_> = text.lines().collect();
        if rows.len() != 1 {
            return Err("ambiguous remote reference".into());
        }
        let fields: Vec<_> = rows[0].split_whitespace().collect();
        if fields.len() != 2 || fields[1] != self.config.reference() || !oid(fields[0]) {
            return Err("invalid remote reference response".into());
        }
        Ok(Some(fields[0].into()))
    }
    pub(crate) fn fetch(&self) -> Result<Option<(String, Files, Snapshot)>> {
        self.prepare()?;
        let Some(commit) = self.remote_oid()? else {
            return Ok(None);
        };
        self.git(
            &[
                "fetch".into(),
                "--no-tags".into(),
                "--".into(),
                self.config.url(),
                self.config.reference(),
            ],
            Vec::new(),
        )?;
        let files = self.files_at(&commit)?;
        let snapshot = validate_files(&files)?;
        Ok(Some((commit, files, snapshot)))
    }
    pub(crate) fn files_at(&self, commit: &str) -> Result<Files> {
        if !oid(commit) {
            return Err("invalid archive Git commit".into());
        }
        let listing = self.git(
            &[
                "ls-tree".into(),
                "-rz".into(),
                "--full-tree".into(),
                commit.into(),
            ],
            Vec::new(),
        )?;
        let mut files = Files::new();
        let mut total = 0usize;
        for row in listing.split(|b| *b == 0).filter(|b| !b.is_empty()) {
            let row = std::str::from_utf8(row).map_err(|e| e.to_string())?;
            let (meta, path) = row.split_once('\t').ok_or("invalid tree entry")?;
            if !meta.starts_with("100644 blob ") {
                return Err("archive tree contains non-ordinary files".into());
            }
            let Some(name) = path.strip_prefix(ARCHIVE) else {
                return Err("unknown file in private archive repository".into());
            };
            if !archive_name(name) {
                return Err("invalid archive file path".into());
            }
            let bytes = self.git(&["show".into(), format!("{commit}:{path}")], Vec::new())?;
            total = total
                .checked_add(bytes.len())
                .ok_or("archive size overflow")?;
            if total > 256 * 1024 * 1024 || files.len() >= 10_000 {
                return Err("archive transport exceeds bounds".into());
            }
            files.insert(name.into(), bytes);
        }
        Ok(files)
    }
    fn candidate_files(&self, commit: &str) -> Result<Files> {
        // Only unresolved local candidate recovery may use this durable copy;
        // fetch/readback must always inspect the actual fetched Git object.
        if let Some(bytes) = self.local(&format!("transport.snapshot.{commit}"))? {
            database::inspect_shared_bytes(&bytes)?;
            return Ok(Files::from([(database::FILE.into(), bytes)]));
        }
        self.files_at(commit)
    }
    fn commit(&self, files: &Files, parent: Option<&str>, kind: &str) -> Result<String> {
        validate_files(files)?;
        let index = self
            .store
            .workspace
            .join(".runtime/bridgepilot/transport/index");
        ordinary(&index)?;
        let mut request = ProcessRequest::new("git", &self.store.workspace);
        let prefix = vec!["--git-dir".to_owned(), git_path(&self.cache)?];
        request
            .env
            .insert("GIT_INDEX_FILE".into(), git_path(&index)?.into());
        let mut all = prefix.clone();
        all.extend(["read-tree".into(), "--empty".into()]);
        request.args = all.iter().map(Into::into).collect();
        let output = self.runner.run(&request).map_err(|e| e.to_string())?;
        if output.code != 0 || output.timed_out {
            return Err("cannot initialize archive transport index".into());
        }
        let mut entries = Vec::new();
        for (name, bytes) in files {
            let hash = String::from_utf8(self.git(
                &["hash-object".into(), "-w".into(), "--stdin".into()],
                bytes.clone(),
            )?)
            .map_err(|e| e.to_string())?;
            if !oid(hash.trim()) {
                return Err("invalid blob identity".into());
            }
            entries
                .extend_from_slice(format!("100644 {}\t{ARCHIVE}{name}\0", hash.trim()).as_bytes());
        }
        let mut all = prefix.clone();
        all.extend(["update-index".into(), "-z".into(), "--index-info".into()]);
        request.args = all.iter().map(Into::into).collect();
        request.stdin = entries;
        let output = self.runner.run(&request).map_err(|e| e.to_string())?;
        if output.code != 0 || output.timed_out {
            return Err("cannot prepare archive transport index".into());
        }
        let mut all = prefix;
        all.push("write-tree".into());
        request.args = all.iter().map(Into::into).collect();
        request.stdin.clear();
        let output = self.runner.run(&request).map_err(|e| e.to_string())?;
        if output.code != 0 || output.timed_out {
            return Err("cannot write archive tree".into());
        }
        let tree = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        let mut args = vec![
            "-c".into(),
            "user.name=BridgePilot".into(),
            "-c".into(),
            "user.email=bridgepilot@localhost".into(),
            "commit-tree".into(),
            tree.trim().into(),
        ];
        if let Some(p) = parent {
            args.extend(["-p".into(), p.into()]);
        }
        args.extend(["-m".into(), format!("BridgePilot archive {kind}")]);
        let commit = self.text(&args)?;
        if !oid(&commit) {
            return Err("invalid archive commit".into());
        }
        Ok(commit)
    }
    pub(crate) fn publish(&self, files: Files, kind: &str, install_before: bool) -> Result<Value> {
        self.publish_inner(files, kind, install_before, None)
    }
    fn publish_inner(
        &self,
        files: Files,
        kind: &str,
        install_before: bool,
        replacing: Option<&Pending>,
    ) -> Result<Value> {
        if let Some(expected) = replacing {
            let actual = self.pending()?;
            if &actual != expected {
                return Err("pending transport changed before replacement".into());
            }
        } else if self.has_pending()? {
            return Err("pending transport requires query-only reconciliation".into());
        }
        let local_before = self.store.head()?;
        let candidate = validate_files(&files)?;
        let candidate_head = head_from_files(&files)?;
        let remote = self.fetch()?;
        if let Some((_, remote_files, _)) = &remote {
            require_ancestor(&head_from_files(remote_files)?, &files)?;
        }
        if let Some((commit, remote_files, _)) = &remote {
            if replacing.is_none()
                && head_from_files(remote_files)? == candidate_head
                && local_before == candidate_head
                && remote_files.contains_key(database::FILE)
            {
                return Ok(
                    json!({"status":"archive_current","commit":commit,"revision":candidate.revision,"snapshot_sha256":crate::digest(&remote_files[database::FILE]),"remote_verified":true}),
                );
            }
        }
        let expected = remote.as_ref().map(|r| r.0.clone());
        let commit = self.commit(&files, expected.as_deref(), kind)?;
        if matches!(kind, "handoff-release" | "handoff-accept") {
            crate::handoff::verify_local_checkpoints(
                self.runner,
                &candidate.state,
                &self.store.machine()?,
            )?;
        }
        let pending = Pending {
            schema: 1,
            candidate: commit.clone(),
            expected_remote: expected.clone(),
            kind: kind.into(),
            local_before,
            candidate_head,
            snapshot_sha256: Some(crate::digest(&files[database::FILE])),
            superseded: replacing
                .map(|old| {
                    let mut candidates = old.superseded.clone();
                    candidates.push(Candidate {
                        commit: old.candidate.clone(),
                        head: old.candidate_head.clone(),
                    });
                    candidates
                })
                .unwrap_or_default(),
        };
        self.save_pending(&pending, &files)?;
        if install_before {
            self.store.install_transport(&files)?;
        }
        self.prepare()?;
        if self.remote_oid()? != expected {
            return Err(
                "remote changed before archive publication; keep local freeze and reconcile".into(),
            );
        }
        if matches!(kind, "handoff-release" | "handoff-accept") {
            crate::handoff::verify_local_checkpoints(
                self.runner,
                &candidate.state,
                &self.store.machine()?,
            )?;
        }
        let push = self.git(
            &[
                "push".into(),
                "--".into(),
                self.config.url(),
                format!("{commit}:{}", self.config.reference()),
            ],
            Vec::new(),
        );
        // A lost acknowledgement is never an automatic resend.
        let observed = self.remote_oid();
        if observed.as_ref().ok().and_then(|v| v.as_ref()) != Some(&commit) {
            return Err(format!(
                "archive push outcome not confirmed; query-only reconciliation required: {}",
                push.err().unwrap_or_else(|| "remote differs".into())
            ));
        }
        let (readback_commit, readback_files, _) = self
            .fetch()?
            .ok_or("published archive remote readback missing; reconcile required")?;
        let readback_hash = crate::digest(
            readback_files
                .get(database::FILE)
                .ok_or("published readback omitted SQLite snapshot")?,
        );
        if readback_commit != commit
            || head_from_files(&readback_files)? != pending.candidate_head
            || Some(&readback_hash) != pending.snapshot_sha256.as_ref()
        {
            return Err(
                "published archive readback differs from exact candidate; reconcile required"
                    .into(),
            );
        }
        self.store.install_transport(&readback_files)?;
        self.finish(&pending,json!({"commit":readback_commit,"snapshot_sha256":readback_hash,"head":pending.candidate_head}))?;
        Ok(
            json!({"status":"archive_published","kind":kind,"commit":commit,"revision":candidate.revision,"snapshot_sha256":readback_hash,"remote_verified":true,"repository":self.config.repository}),
        )
    }
    fn finish(&self, pending: &Pending, readback: Value) -> Result<()> {
        let mut conn = self.store.write_connection()?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let actual: Vec<u8> = tx
            .query_row(
                "SELECT value FROM local_records WHERE key='transport.pending'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if serde_json::from_slice::<Pending>(&actual).map_err(|e| e.to_string())? != *pending {
            return Err("pending intent changed during confirmation".into());
        }
        let receipt = serde_json::to_vec(
            &json!({"status":"confirmed","operation":pending,"readback":readback}),
        )
        .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO local_records(key,value) VALUES('transport.last_receipt',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[receipt]).map_err(|e|e.to_string())?;
        tx.execute("DELETE FROM local_records WHERE key='transport.pending' OR key LIKE 'transport.snapshot.%'",[]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn reconcile(&self) -> Result<Value> {
        let _lock = self.store.writer_lock()?;
        let pending = self.pending()?;
        let Some((commit, files, _)) = self.fetch()? else {
            return Err("pending archive push is unresolved; no remote head".into());
        };
        // Require both committed Git ancestry and exact application ancestry.
        let mut candidates = vec![Candidate {
            commit: pending.candidate.clone(),
            head: pending.candidate_head.clone(),
        }];
        candidates.extend(pending.superseded.clone());
        let observed = candidates.iter().find(|candidate| {
            self.git(
                &[
                    "merge-base".into(),
                    "--is-ancestor".into(),
                    candidate.commit.clone(),
                    commit.clone(),
                ],
                Vec::new(),
            )
            .is_ok()
        });
        let Some(observed) = observed else {
            return Err(
                "pending archive candidate is not confirmed remotely; remain frozen".into(),
            );
        };
        require_ancestor(&observed.head, &files)?;
        let physical = crate::digest(
            files
                .get(database::FILE)
                .ok_or("reconciliation remote is not SQLite")?,
        );
        if commit == pending.candidate
            && pending
                .snapshot_sha256
                .as_ref()
                .is_some_and(|expected| expected != &physical)
        {
            return Err("reconciliation candidate physical snapshot digest mismatch".into());
        }
        let local = self.store.head()?;
        if local != pending.local_before {
            require_ancestor(&local, &files)?;
        }
        self.store.install_transport(&files)?;
        self.finish(
            &pending,
            json!({"commit":commit,"snapshot_sha256":physical,"head":head_from_files(&files)?}),
        )?;
        Ok(
            json!({"status":"archive_transport_reconciled","commit":commit,"snapshot_sha256":physical,"replayed":false}),
        )
    }
    pub fn upload(&self) -> Result<Value> {
        let _lock = self.store.writer_lock()?;
        let snapshot = self.store.read()?;
        if snapshot.state.runs.values().any(|r| r.state.active()) {
            return Err("synchronize only at an idle/reconciled boundary; do not block a running worker's archive writes".into());
        }
        if snapshot.state.custody.is_none() {
            return Err("explicit sync-enrol is required before the first upload".into());
        }
        crate::handoff::require_owner(&snapshot.state, &self.store.machine()?)?;
        self.publish(self.store.archive_files()?, "sync", false)
    }
    pub fn cancel_pending_release(&self, source: &str) -> Result<Value> {
        let _lock = self.store.writer_lock()?;
        if source.trim().is_empty() {
            return Err("explicit cancellation source required".into());
        }
        let pending = self.pending()?;
        if pending.kind != "handoff-release" {
            return Err("only an unresolved release can be cancelled by this command".into());
        }
        self.prepare()?;
        if self.remote_oid()? != pending.expected_remote {
            return Err(
                "remote has changed; reconcile the original release before cancellation".into(),
            );
        }
        let files = self.candidate_files(&pending.candidate)?;
        let mut snapshot = validate_files(&files)?;
        let machine = self.store.machine()?;
        let c = snapshot
            .state
            .custody
            .as_mut()
            .ok_or("missing candidate custody")?;
        if c.owner.is_some() || c.handoff.as_ref().is_none_or(|h| h.from != machine) {
            return Err("release cancellation belongs to another machine".into());
        }
        let head = self.store.head()?;
        if head != pending.local_before && head != pending.candidate_head {
            return Err("local archive changed since pending release".into());
        }
        // Freeze even if the original process crashed before installing release.
        self.store.install_transport(&files)?;
        c.epoch = c.epoch.checked_add(1).ok_or("custody epoch overflow")?;
        c.owner = Some(machine);
        c.handoff = None;
        c.source = source.into();
        let cancellation = derive_files(files, snapshot.state)?;
        self.store
            .write_connection()?
            .execute(
                "INSERT OR REPLACE INTO local_records(key,value) VALUES(?1,?2)",
                params![
                    format!("transport.abandoned.{}", pending.candidate),
                    serde_json::to_vec(&pending).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        // This sibling Git commit fences any delayed old release push. Local
        // activation remains frozen until the cancellation is confirmed remotely.
        // Keep the original recovery record until the new candidate is completely
        // prepared; publish_inner atomically replaces it before the external push.
        self.publish_inner(cancellation, "handoff-cancel", false, Some(&pending))
    }
    /// Resolve an explicitly abandoned publication by a new sibling commit.
    /// Old pushes are never replayed. The saved alternatives allow reconciliation
    /// if an already in-flight original push wins the remote race instead.
    pub fn fence_pending(&self, source: &str) -> Result<Value> {
        if source.trim().is_empty() {
            return Err("explicit pending resolution source required".into());
        }
        let pending = self.pending()?;
        if pending.kind == "handoff-release" {
            return self.cancel_pending_release(source);
        }
        let _lock = self.store.writer_lock()?;
        let actual = self.pending()?;
        if actual != pending {
            return Err("pending publication changed".into());
        }
        let remote = self.fetch()?;
        if remote.as_ref().map(|r| &r.0) != pending.expected_remote.as_ref() {
            return Err("remote changed; query-only reconcile before pending resolution".into());
        }
        let files = if pending.kind == "handoff-accept" {
            // Abandon an unconfirmed claim without publishing its new owner.
            remote.ok_or("acceptance has no released remote head")?.1
        } else if matches!(
            pending.kind.as_str(),
            "sync" | "handoff-cancel" | "pending-fence"
        ) {
            self.candidate_files(&pending.candidate)?
        } else {
            return Err("unsupported pending publication kind".into());
        };
        let snapshot = validate_files(&files)?;
        if pending.kind != "handoff-accept"
            && snapshot
                .state
                .custody
                .as_ref()
                .is_some_and(|c| c.owner.is_some())
        {
            crate::handoff::require_owner(&snapshot.state, &self.store.machine()?)?;
        }
        self.publish_inner(files, "pending-fence", false, Some(&pending))
    }
    pub fn download(&self) -> Result<Value> {
        let _lock = self.store.writer_lock()?;
        if self.has_pending()? {
            return Err("pending archive transport must be reconciled first".into());
        }
        let local = self.store.read()?;
        if local.state.runs.values().any(|r| r.state.active()) {
            return Err("cannot import while local execution is active or unknown".into());
        }
        let Some((commit, files, snapshot)) = self.fetch()? else {
            return Err("private archive remote is empty".into());
        };
        if !files.contains_key(database::FILE) {
            return Err("legacy remote archive requires explicit owner migration before new-machine restore".into());
        }
        let current = self.store.head()?;
        let empty = local.revision == 0
            && local.state.projects.is_empty()
            && local.state.tasks.is_empty()
            && local.state.custody.is_none();
        if !empty {
            require_ancestor(&current, &files)?;
        }
        self.store.install_transport(&files)?;
        Ok(
            json!({"status":"archive_downloaded","commit":commit,"revision":snapshot.revision,"snapshot_sha256":crate::digest(&files[database::FILE]),"owner":snapshot.state.custody.as_ref().map(|c|&c.owner),"execution_started":false}),
        )
    }
}
fn oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn archive_name(s: &str) -> bool {
    if s == database::FILE {
        return true;
    }
    if s == "HEAD.json" {
        return true;
    }
    let Some(name) = s.strip_prefix("revisions/") else {
        return false;
    };
    let Some(name) = name.strip_suffix(".json") else {
        return false;
    };
    let Some((revision, hash)) = name.split_once('-') else {
        return false;
    };
    revision.parse::<u64>().is_ok()
        && hash.len() == 64
        && hash.bytes().all(|b| b.is_ascii_hexdigit())
}
pub(crate) fn head_from_files(files: &Files) -> Result<Head> {
    if let Some(bytes) = files.get(database::FILE) {
        if files.len() != 1 {
            return Err("mixed SQLite and legacy archive transport".into());
        }
        return database::head_bytes(bytes);
    }
    serde_json::from_slice(files.get("HEAD.json").ok_or("archive HEAD missing")?)
        .map_err(|e| e.to_string())
}
pub(crate) fn validate_files(files: &Files) -> Result<Snapshot> {
    if let Some(bytes) = files.get(database::FILE) {
        if files.len() != 1 {
            return Err("mixed SQLite and legacy archive transport".into());
        }
        return database::inspect_shared_bytes(bytes);
    }
    if files.keys().any(|k| !archive_name(k)) {
        return Err("unowned archive transport path".into());
    }
    let head = head_from_files(files)?;
    let mut cursor = head.clone();
    let mut latest = None;
    let mut count = 0;
    loop {
        if cursor.schema != 1 || cursor.archive_id != head.archive_id || cursor.digest.len() != 64 {
            return Err("invalid transport archive identity".into());
        }
        let name = format!("revisions/{}-{}.json", cursor.revision, cursor.digest);
        let bytes = files.get(&name).ok_or("incomplete transport history")?;
        if bytes.len() > 16 * 1024 * 1024 || digest(bytes) != cursor.digest {
            return Err("transport history digest mismatch".into());
        }
        let value = bridgeforge_core::json::parse_unique_json(bytes, "transport revision")?;
        let snapshot: Snapshot = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if snapshot.schema != 1
            || snapshot.archive_id != head.archive_id
            || snapshot.revision != cursor.revision
        {
            return Err("transport revision identity mismatch".into());
        }
        snapshot.state.validate()?;
        if latest.is_none() {
            latest = Some(snapshot.clone());
        }
        count += 1;
        match snapshot.parent {
            Some(p) if snapshot.revision > 0 && p.revision == snapshot.revision - 1 => cursor = p,
            None if snapshot.revision == 0 => break,
            _ => return Err("broken transport history chain".into()),
        }
    }
    if files.len() != count + 1 {
        return Err("unreferenced or forked archive revision in transport".into());
    }
    latest.ok_or("empty transport archive".into())
}
pub(crate) fn require_ancestor(ancestor: &Head, files: &Files) -> Result<()> {
    if let Some(bytes) = files.get(database::FILE) {
        return database::require_ancestor_bytes(ancestor, bytes);
    }
    let mut cursor = head_from_files(files)?;
    loop {
        if &cursor == ancestor {
            return Ok(());
        }
        if cursor.archive_id != ancestor.archive_id || cursor.revision <= ancestor.revision {
            return Err("archive history diverged or would roll back; no automatic merge".into());
        }
        let name = format!("revisions/{}-{}.json", cursor.revision, cursor.digest);
        let value: Snapshot =
            serde_json::from_slice(files.get(&name).ok_or("missing ancestry revision")?)
                .map_err(|e| e.to_string())?;
        cursor = value.parent.ok_or("archive ancestor not found")?;
    }
}
pub(crate) fn changed_files(store: &Store, state: crate::domain::State) -> Result<Files> {
    let (_, bytes) = store.derived(state)?;
    let files = Files::from([(database::FILE.into(), bytes)]);
    validate_files(&files)?;
    Ok(files)
}
fn derive_files(mut files: Files, state: crate::domain::State) -> Result<Files> {
    if let Some(bytes) = files.get(database::FILE) {
        let (_, candidate) = database::derive_bytes(bytes, state)?;
        return Ok(Files::from([(database::FILE.into(), candidate)]));
    }
    let old = validate_files(&files)?;
    let snapshot = Snapshot {
        schema: 1,
        archive_id: old.archive_id,
        revision: old.revision.checked_add(1).ok_or("revision overflow")?,
        parent: Some(head_from_files(&files)?),
        state,
    };
    let bytes = serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?;
    let head = crate::store::Head {
        schema: 1,
        archive_id: snapshot.archive_id.clone(),
        revision: snapshot.revision,
        digest: digest(&bytes),
    };
    files.insert(
        format!("revisions/{}-{}.json", head.revision, head.digest),
        bytes,
    );
    files.insert(
        "HEAD.json".into(),
        serde_json::to_vec(&head).map_err(|e| e.to_string())?,
    );
    validate_files(&files)?;
    Ok(files)
}
pub fn upload(s: &Store) -> Result<Value> {
    Engine::new(s, &SystemProcessRunner)?.upload()
}
pub fn download(s: &Store) -> Result<Value> {
    Engine::new(s, &SystemProcessRunner)?.download()
}
pub fn reconcile(s: &Store) -> Result<Value> {
    Engine::new(s, &SystemProcessRunner)?.reconcile()
}
pub fn cancel_pending(s: &Store, source: &str) -> Result<Value> {
    Engine::new(s, &SystemProcessRunner)?.cancel_pending_release(source)
}
pub fn fence_pending(s: &Store, source: &str) -> Result<Value> {
    Engine::new(s, &SystemProcessRunner)?.fence_pending(source)
}
