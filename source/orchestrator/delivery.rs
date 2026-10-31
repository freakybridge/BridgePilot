//! Consume actual managed development and Git publication evidence. This module
//! only queries tools/Git; it never commits, fetches, pushes or changes versions.
use crate::{
    Result, digest,
    domain::{Run, RunState, State, TaskState},
    store::{Store, ordinary},
    valid_id,
};
use bridgeforge_core::{ProcessRequest, ProcessRunner, SemVer, SystemProcessRunner};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

fn command(root: &Path, program: &Path, args: &[String]) -> Result<Vec<u8>> {
    let mut request = ProcessRequest::new(program.as_os_str(), root);
    request.args = args.iter().map(Into::into).collect();
    request.timeout = Duration::from_secs(60);
    let output = SystemProcessRunner
        .run(&request)
        .map_err(|e| e.to_string())?;
    if output.timed_out || output.code != 0 {
        return Err(format!(
            "read-only evidence command failed (exit {}, timeout {}): {}",
            output.code,
            output.timed_out,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if output.stdout.len() > 16 * 1024 * 1024 {
        return Err("oversized evidence command output".into());
    }
    Ok(output.stdout)
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let mut all = vec!["--no-optional-locks".into()];
    all.extend(args.iter().map(|s| (*s).into()));
    String::from_utf8(command(root, Path::new("git"), &all)?)
        .map_err(|e| e.to_string())
        .map(|s| s.trim_end().to_owned())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CodeIdentity {
    pub root: PathBuf,
    pub head: String,
    pub branch: String,
    pub status_sha256: String,
    pub working_tree: String,
}
fn identity(root: &Path) -> Result<CodeIdentity> {
    let actual = git(root, &["rev-parse", "--show-toplevel"])?;
    if fs::canonicalize(actual).map_err(|e| e.to_string())? != root {
        return Err("evidence root is not the exact Git project root".into());
    }
    let status = git(root, &["status", "--porcelain=v1", "--untracked-files=all"])?;
    Ok(CodeIdentity {
        root: root.into(),
        head: git(root, &["rev-parse", "--verify", "HEAD"])?,
        branch: git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?,
        status_sha256: digest(status.as_bytes()),
        working_tree: if status.is_empty() { "clean" } else { "dirty" }.into(),
    })
}
fn file_hash(root: &Path, relative: &str) -> Result<String> {
    let path = root.join(relative);
    ordinary(&path)?;
    let metadata =
        fs::metadata(&path).map_err(|e| format!("required evidence input {relative}: {e}"))?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err("evidence input is not a bounded file".into());
    }
    Ok(digest(&fs::read(path).map_err(|e| e.to_string())?))
}
fn files(root: &Path) -> Result<BTreeMap<String, String>> {
    [
        "VERSION",
        "AGENTS.md",
        ".codex/bridgeforge-version.json",
        ".codex/package-closures.json",
    ]
    .into_iter()
    .map(|p| Ok((p.into(), file_hash(root, p)?)))
    .collect()
}
fn packages(ids: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    if ids.is_empty() || ids.len() > 64 || ids.iter().any(|id| !valid_id(id) || !seen.insert(id)) {
        return Err("explicit unique registered package scope required".into());
    }
    Ok(())
}
fn managed_tool(root: &Path, tool: &Path) -> Result<PathBuf> {
    ordinary(tool)?;
    let actual = fs::canonicalize(tool).map_err(|e| e.to_string())?;
    let allowed = [
        ".build/bridgeforge/bin/bridgeforge-factory.exe",
        ".build/bridgeforge/bin/bridgeforge.exe",
        ".deps/bridgeforge/bin/bridgeforge.exe",
        ".deps/bridgeforge/bin/bridgeforge",
        ".codex/hooks/bin/bridgeforge.exe",
    ];
    if !actual.is_file()
        || !allowed
            .iter()
            .any(|p| fs::canonicalize(root.join(p)).ok().as_ref() == Some(&actual))
    {
        return Err("verification requires an existing project-local managed tool entry".into());
    }
    Ok(actual)
}
fn preview(root: &Path, tool: &Path, ids: &[String]) -> Result<(Value, String)> {
    let raw = command(
        root,
        tool,
        &[
            "develop".into(),
            "--preview".into(),
            "--packages".into(),
            ids.join(","),
            "--root".into(),
            root.to_string_lossy().into(),
        ],
    )?;
    let value = bridgeforge_core::json::parse_unique_json(&raw, "managed development preview")?;
    if value["status"] != "task-development-current"
        || value["readiness_scope"] != "task"
        || value["affected_packages"]
            .as_array()
            .is_none_or(|p| !p.is_empty())
        || value["pending_reasons"]
            .as_object()
            .is_none_or(|p| !p.is_empty())
    {
        return Err(format!(
            "managed development is not current for this task: {value}"
        ));
    }
    for capability in [
        "package-development-v1",
        "version-coverage-v1",
        "task-scoped-development-v1",
    ] {
        if value["capabilities"]
            .as_array()
            .is_none_or(|c| !c.iter().any(|v| v == capability))
        {
            return Err(format!("managed evidence capability missing: {capability}"));
        }
    }
    for id in ids {
        if value["packages"][id]["version"]
            .as_str()
            .is_none_or(|s| s.is_empty())
        {
            return Err("preview omitted a requested package version".into());
        }
    }
    if value["fingerprint"]
        .as_str()
        .is_none_or(|s| !s.starts_with("sha256:"))
    {
        return Err("managed preview fingerprint missing".into());
    }
    Ok((value, digest(&raw)))
}
/// Read-only diagnostic, also used by the task-bound verifier. Two previews
/// bracket the observed inputs; no arbitrary historic passed log is imported.
pub fn development_probe(root: &Path, tool: &Path, ids: &[String]) -> Result<Value> {
    packages(ids)?;
    ordinary(root)?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let tool = managed_tool(&root, tool)?;
    let tool_hash = digest(&fs::read(&tool).map_err(|e| e.to_string())?);
    let before = identity(&root)?;
    let original_files = files(&root)?;
    let (first, _) = preview(&root, &tool, ids)?;
    let (last, stdout_hash) = preview(&root, &tool, ids)?;
    let after = identity(&root)?;
    if before != after
        || original_files != files(&root)?
        || first["fingerprint"] != last["fingerprint"]
        || tool_hash != digest(&fs::read(&tool).map_err(|e| e.to_string())?)
    {
        return Err("development evidence changed during observation".into());
    }
    let package_versions: BTreeMap<_, _> = ids
        .iter()
        .map(|id| (id.clone(), last["packages"][id]["version"].clone()))
        .collect();
    Ok(
        json!({"schema":1,"status":"development_verified","code":after,"files":original_files,"managed_tool":tool,"managed_tool_sha256":tool_hash,"preview_stdout_sha256":stdout_hash,"preview_fingerprint":last["fingerprint"],"package_versions":package_versions,"managed_preview":last,"publication_verified":false}),
    )
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentReview {
    pub run: String,
    pub result_sha256: String,
    pub source: String,
    pub note: String,
    pub tool: PathBuf,
    pub packages: Vec<String>,
}
fn run_context<'a>(s: &'a State, id: &str, machine: &str, result_hash: &str) -> Result<&'a Run> {
    let r = s.runs.get(id).ok_or("unknown delivery run")?;
    let task = &s.tasks[&r.task];
    let binding = s
        .bindings
        .get(&format!("{machine}:{}", task.input.project))
        .ok_or("missing local project binding")?;
    if r.machine != machine
        || r.state != RunState::Completed
        || r.spec_hash != task.spec_hash
        || !task.input.authorization.allow_write
        || !matches!(
            task.state,
            TaskState::Reviewing | TaskState::AwaitingRelease
        )
        || s.projects[&task.input.project].paused
        || binding.root != r.binding.root
        || binding.desktop_project_id != r.binding.desktop_project_id
        || s.runs.values().any(|other| {
            other.id != id
                && other.state.active()
                && s.tasks[&other.task].input.project == task.input.project
        })
        || digest(
            &serde_json::to_vec(r.result.as_ref().ok_or("missing model result")?)
                .map_err(|e| e.to_string())?,
        ) != result_hash
    {
        return Err("delivery requires the current completed authorized local run and unchanged result, without another project writer".into());
    }
    Ok(r)
}
pub fn review_development(store: &Store, expected: u64, input: DevelopmentReview) -> Result<Value> {
    if input.source.trim().is_empty() || input.note.trim().is_empty() {
        return Err("actual development review source and note required".into());
    }
    let snapshot = store.read()?;
    if snapshot.revision != expected {
        return Err("archive changed before development review".into());
    }
    let machine = store.machine()?;
    let r = run_context(&snapshot.state, &input.run, &machine, &input.result_sha256)?;
    if crate::grouping::observed_proof(&snapshot.state, &input.run).is_none() {
        return Err("actual native project grouping required before development delivery".into());
    }
    let mut proof = development_probe(&r.binding.root, &input.tool, &input.packages)?;
    proof["source"] = json!(input.source);
    proof["note"] = json!(input.note);
    proof["task_spec"] = json!(r.spec_hash);
    proof["result_sha256"] = json!(input.result_sha256);
    store.update(|s| {
        let r = run_context(s, &input.run, &machine, &input.result_sha256)?;
        let task = r.task.clone();
        s.runs.get_mut(&input.run).unwrap().result.as_mut().unwrap()["development_review"] =
            proof.clone();
        s.tasks.get_mut(&task).unwrap().state = TaskState::AwaitingRelease;
        Ok(())
    })?;
    Ok(
        json!({"schema":1,"status":"development_verified","run":input.run,"next":"awaiting_explicitly_authorized_managed_release","proof":proof,"publication_verified":false}),
    )
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationReview {
    pub run: String,
    pub result_sha256: String,
    pub source: String,
    pub receipt_path: PathBuf,
    pub receipt_sha256: String,
    pub remote: String,
    pub remote_ref: String,
}
fn repository_identity(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .replace('\\', "/")
}

fn actual_push_ref(root: &Path, branch: &str, remote: &str) -> Result<String> {
    let branch_ref = format!("refs/heads/{branch}");
    let actual_remote = git(
        root,
        &["for-each-ref", "--format=%(push:remotename)", &branch_ref],
    )?;
    if actual_remote != remote {
        return Err(
            "requested publication remote differs from actual Git push configuration".into(),
        );
    }
    let tracking = git(root, &["rev-parse", "--symbolic-full-name", "@{push}"])?;
    let specs = git(
        root,
        &["config", "--get-all", &format!("remote.{remote}.fetch")],
    )?;
    let mut sources = BTreeSet::new();
    for spec in specs.lines() {
        let spec = spec.strip_prefix('+').unwrap_or(spec);
        if spec.starts_with('^') {
            continue;
        }
        let Some((source, destination)) = spec.split_once(':') else {
            continue;
        };
        if !source.contains('*') && !destination.contains('*') {
            if destination == tracking {
                sources.insert(source.to_owned());
            }
        } else if source.matches('*').count() == 1 && destination.matches('*').count() == 1 {
            let (prefix, suffix) = destination.split_once('*').unwrap();
            if let Some(middle) = tracking
                .strip_prefix(prefix)
                .and_then(|v| v.strip_suffix(suffix))
            {
                sources.insert(source.replacen('*', middle, 1));
            }
        }
    }
    if sources.len() != 1 {
        return Err("actual push tracking ref does not resolve to one remote branch".into());
    }
    Ok(sources.into_iter().next().unwrap())
}
pub fn review_publication(store: &Store, expected: u64, input: PublicationReview) -> Result<Value> {
    if input.source.trim().is_empty()
        || input.remote.starts_with('-')
        || input.remote.is_empty()
        || !input.remote_ref.starts_with("refs/heads/")
    {
        return Err("explicit actual publication source and push target required".into());
    }
    let snapshot = store.read()?;
    if snapshot.revision != expected {
        return Err("archive changed before publication review".into());
    }
    let machine = store.machine()?;
    let r = run_context(&snapshot.state, &input.run, &machine, &input.result_sha256)?;
    let task = &snapshot.state.tasks[&r.task];
    if !task.input.authorization.allow_publish {
        return Err("publication verification cannot grant missing publication authority".into());
    }
    let development = &r.result.as_ref().unwrap()["development_review"];
    if development["status"] != "development_verified" || development["task_spec"] != r.spec_hash {
        return Err(
            "current task-bound development verification required before publication".into(),
        );
    }
    let root = fs::canonicalize(&r.binding.root).map_err(|e| e.to_string())?;
    ordinary(&input.receipt_path)?;
    let receipt_path = fs::canonicalize(&input.receipt_path).map_err(|e| e.to_string())?;
    if !receipt_path.starts_with(root.join(".runtime")) || !receipt_path.is_file() {
        return Err(
            "actual managed publication receipt must be inside this project's runtime".into(),
        );
    }
    let receipt_bytes = fs::read(&receipt_path).map_err(|e| e.to_string())?;
    if receipt_bytes.len() > 4 * 1024 * 1024 || digest(&receipt_bytes) != input.receipt_sha256 {
        return Err("publication receipt digest changed".into());
    }
    let receipt =
        bridgeforge_core::json::parse_unique_json(&receipt_bytes, "managed publication receipt")?;
    let code = identity(&root)?;
    if receipt["schema"] != 1
        || !matches!(
            receipt["status"].as_str(),
            Some("synced" | "remaining-state")
        )
        || receipt["release_requested"] != true
        || receipt["push_performed"] != true
        || receipt["version_bumped"] != true
        || receipt["ahead"] != 0
        || receipt["behind"] != 0
        || receipt["commit"] != code.head
        || receipt["release_details"]["schema_version"] != 4
    {
        return Err(
            "receipt does not establish an actual managed release and successful push".into(),
        );
    }
    let push_target = git(
        &root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{push}",
        ],
    )?;
    if receipt["push_target"] != push_target {
        return Err("actual Git push target differs from the managed receipt".into());
    }
    if actual_push_ref(&root, &code.branch, &input.remote)? != input.remote_ref {
        return Err(
            "requested publication remote/ref does not match the actual push configuration".into(),
        );
    }
    let url = git(
        &root,
        &["remote", "get-url", "--push", "--all", &input.remote],
    )?;
    if url.lines().count() != 1 {
        return Err("publication requires one explicitly registered push repository".into());
    }
    if repository_identity(&url)
        != repository_identity(&snapshot.state.projects[&task.input.project].repository)
    {
        return Err("actual push repository does not match the registered project identity".into());
    }
    let committed_version = git(&root, &["show", &format!("{}:VERSION", code.head)])?;
    let before: SemVer = receipt["version_before"]
        .as_str()
        .ok_or("missing release old version")?
        .parse()
        .map_err(|e: String| e)?;
    let after: SemVer = committed_version.parse().map_err(|e: String| e)?;
    if after <= before
        || receipt["version_after"] != committed_version
        || receipt["release_details"]["version_after"] != committed_version
    {
        return Err(
            "published product version does not match the committed managed release".into(),
        );
    }
    let old_head = development["code"]["head"]
        .as_str()
        .ok_or("development code identity missing")?;
    git(
        &root,
        &["merge-base", "--is-ancestor", old_head, &code.head],
    )?;
    let committed_ledger = command(
        &root,
        Path::new("git"),
        &[
            "--no-optional-locks".into(),
            "show".into(),
            format!("{}:.codex/package-closures.json", code.head),
        ],
    )?;
    if development["files"][".codex/package-closures.json"] != digest(&committed_ledger) {
        return Err("release commit changed the verified development closure ledger".into());
    }
    let package_list = receipt["release_details"]["packages"]
        .as_array()
        .ok_or("release package summary missing")?;
    for (id, version) in development["package_versions"]
        .as_object()
        .ok_or("development package scope missing")?
    {
        if !package_list
            .iter()
            .any(|p| p["id"] == *id && p["version_after"] == *version)
        {
            return Err("published package versions do not cover the development task".into());
        }
    }
    let remote = git(
        &root,
        &[
            "ls-remote",
            "--exit-code",
            "--refs",
            "--",
            &url,
            &input.remote_ref,
        ],
    )?;
    let lines: Vec<_> = remote.lines().collect();
    let expected_remote = format!("{}\t{}", code.head, input.remote_ref);
    if lines != [expected_remote.as_str()] {
        return Err("live push target does not match the actual release commit; publication remains unverified".into());
    }
    if identity(&root)? != code
        || git(
            &root,
            &["remote", "get-url", "--push", "--all", &input.remote],
        )? != url
        || git(
            &root,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{push}",
            ],
        )? != push_target
        || actual_push_ref(&root, &code.branch, &input.remote)? != input.remote_ref
        || digest(&fs::read(&receipt_path).map_err(|e| e.to_string())?) != input.receipt_sha256
    {
        return Err("publication identity drifted during verification".into());
    }
    let proof = json!({"schema":1,"status":"publication_verified","source":input.source,"task_spec":r.spec_hash,"result_sha256":input.result_sha256,"code":code,"product_version":committed_version,"receipt_path":receipt_path,"receipt_sha256":input.receipt_sha256,"remote":input.remote,"remote_ref":input.remote_ref,"live_remote_commit":code.head,"packages":package_list,"managed_receipt":receipt,"publication_verified":true});
    store.update(|s| {
        let r = run_context(s, &input.run, &machine, &input.result_sha256)?;
        let task = r.task.clone();
        s.runs.get_mut(&input.run).unwrap().result.as_mut().unwrap()["publication_review"] =
            proof.clone();
        s.tasks.get_mut(&task).unwrap().state = TaskState::Released;
        Ok(())
    })?;
    Ok(
        json!({"schema":1,"status":"publication_verified","run":input.run,"proof":proof,"publication_verified":true}),
    )
}
