//! Owned portable installation. Runtime activation, user configuration and
//! archive enrolment are separate transactions; failed upgrades never migrate
//! a private archive merely to make an older reader appear compatible.
use crate::{
    Result, digest,
    store::{Store, ordinary, read_json},
};
use bridgeforge_core::{
    ProcessRequest, ProcessRunner, SemVer, SystemProcessRunner, file_lock::FileLock, persistence,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

const RULES:&[u8]="# BridgePilot\n\n这是 BridgePilot 的安装控制入口。先读取 [BP 工作流](source/orchestrator/skills/bridgepilot/SKILL.md)，只推进当前用户已授权的任务。私有工作区以 .runtime/config/bridgepilot.json 为准；安装升级不授权业务提交或发布，不修改原生 Memory。\n".as_bytes();
const README:&[u8]="# BridgePilot\n\n在 Codex 中打开本目录并给出项目目标。由 BP 读取私有任务档案，确认必要方向后推进。\n\n- [安装、升级与换机](INSTALL.md)\n- [BP 工作流](source/orchestrator/skills/bridgepilot/SKILL.md)\n".as_bytes();
const GUIDE: &[u8] = r#"# BridgePilot 安装与恢复

这些是 Agent 的执行摘要；用户继续在 Codex 中给目标，无需手填 JSON。

包包含 payload/bp-core.exe、payload/bp-launcher.exe 和 package.json。新机准备 Git、GitHub CLI 与 Codex 登录，无需编译 Rust。先初始化独立私有工作区：`bp-core.exe init --workspace <绝对路径>`。用 `install-preview --bundle <包目录> --home <独立绝对目录> --workspace <工作区>` 零写入检查，再将命令换成 install。在 Codex 打开 home；稳定入口是 `.build/bridgepilot/bin/bp.exe`，从 home/.runtime/config/bridgepilot.json 定位档案。

升级用新包重复 preview/install，最后切换 current，配置和旧完整包保留。中断先 install-recover --home，回退用 install-rollback --home；旧程序不能读真实档案时拒绝。安装不迁移或回滚档案、不修改 PATH、旧 Forge、共享 Skill 或凭据。用户编辑过的受管文件不覆盖。

私有同步：先以 sync-configure --workspace --input 绑定 JSON `{"repository":"<owner/repo>","branch":"main","source":"<真实授权>"}`。原机首次 sync-enrol --expected <当前 revision> --source 登记控制权；空闲边界 sync-upload。只传 HEAD 与完整 revisions，不传 .runtime/.deps/.build、源码、原生会话或 Memory。新机 init 自己的身份、配置同一私有库、sync-download；下载不授予控制权。未知结果先 sync-reconcile，只查不重发。远端仍是原预期头时，明确 sync-fence-pending --source 可发布兄弟提交栅栏解决未发送的初始同步或放弃接管；不删除 pending，不 force 或自动合并。

交接前必须明确全部暂停、无活跃/Unknown，原进程确认已停。各项目由主对话按已有 git-sync 授权保存并推送代码，BP 不内部提交。每项目 checkpoint-probe --workspace --project --remote --ref refs/heads/<branch> --source，收集真实输出中的 checkpoint 对象。handoff-release --workspace --input 消费 `{"checkpoints":[<真实 checkpoint 对象>],"source":"..."}`，原机先冻结。handoff-status 显示 transfer；不要手写 ID。

新机检出各检查点的精确分支/HEAD，查询自己的原生项目成员，handoff-accept --workspace --input 消费 `{"transfer":"<实际 ID>","bindings":[{"project":"<id>","machine":"<本机 ID>","root":"<真实绝对 Git 根>","desktop_project_id":"<实际原生 ID>","host":"local"}],"source":"..."}`。全部登记项目核验后，远端先确认独占归属，才激活本机。接受不自动恢复项目或 heartbeat。

明确 resume 项目后，handoff-continue --workspace --expected --input 消费 `{"tasks":["<旧 task ID>"],"source":"<明确接续来源>"}`。旧 Run/结果/确认保留，新机 Run 使用新聊天和本机绑定；新聊天仍要满足原生工具的人类授权。未接管的释放可由原机 handoff-cancel --source；未确认释放用 sync-cancel-pending --source。另一机器已接管则不能覆盖。

早期派发前已核销的终态 Run 若缺 birth、server PID 被复用，只能按 BP Skill 用 legacy-server-probe，提供实际 --run、--codex、--historical-receipt、--launch-evidence、--supervisor-log 和 --source。程序核对原 owner 不存在、系统进程名与当时 Codex 不同、原生线程全部终态；本机证明不改 Run，不豁免 Unknown、owner 或已派发任务。缺少真实原始证据则停止依赖动作。

所有档案命令显式带 --workspace。status 给当前 revision，需 CAS 的写入使用 --expected；结构化正文通过 JSON 文件 --input 传递，不手改档案。运行与核对沿 [BP Skill](source/orchestrator/skills/bridgepilot/SKILL.md)。
"#.as_bytes();
fn config(home: &Path) -> PathBuf {
    home.join(".runtime/config/bridgepilot-installation.json")
}
fn journal(home: &Path) -> PathBuf {
    home.join(".runtime/bridgepilot-install/journal.json")
}
fn workspace_config(home: &Path) -> PathBuf {
    home.join(".runtime/config/bridgepilot.json")
}
fn home_claim(home: &Path) -> PathBuf {
    home.join(".runtime/config/bridgepilot-home.json")
}
fn claim(home: &Path) -> Result<Option<String>> {
    if !home_claim(home).exists() {
        return Ok(None);
    }
    let v: Value = read_json(&home_claim(home))?;
    if v["schema"] != 1 || v["product"] != "BridgePilot" {
        return Err("invalid installation home ownership".into());
    }
    Ok(Some(
        v["launcher_sha256"]
            .as_str()
            .ok_or("launcher claim missing")?
            .into(),
    ))
}
fn scaffold_only(home: &Path) -> Result<bool> {
    if !home.exists() {
        return Ok(true);
    }
    let mut queue = vec![home.to_path_buf()];
    while let Some(dir) = queue.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            ordinary(&entry.path())?;
            let meta = entry.metadata().map_err(|e| e.to_string())?;
            if meta.is_dir() {
                queue.push(entry.path());
            } else if entry.path() != home.join(".runtime/bridgepilot-install/locks/writer.lock")
                || meta.len() != 0
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub source: String,
    pub target: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub schema: u32,
    pub product: String,
    pub version: String,
    pub launcher_protocol: u32,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnedFile {
    pub id: String,
    pub target: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Version {
    pub version: String,
    pub executable: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Installation {
    pub schema: u32,
    pub product: String,
    pub current: Version,
    pub previous: Option<Version>,
    pub files: Vec<OwnedFile>,
    pub launcher_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    target: String,
    old: Option<Vec<u8>>,
    new_hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    old: Option<Installation>,
    new: Installation,
    changes: Vec<Change>,
}

fn bytes(path: &Path) -> Result<Vec<u8>> {
    ordinary(path)?;
    let m = fs::metadata(path).map_err(|e| e.to_string())?;
    if !m.is_file() || m.len() > 256 * 1024 * 1024 {
        return Err("installation asset is not an ordinary bounded file".into());
    }
    fs::read(path).map_err(|e| e.to_string())
}
fn digest_file(path: &Path) -> Result<String> {
    Ok(digest(&bytes(path)?))
}
fn matches_bytes(path: &Path, expected: Option<&[u8]>) -> Result<bool> {
    ordinary(path)?;
    match expected {
        Some(body) => Ok(path.exists() && bytes(path)? == body),
        None => Ok(!path.exists()),
    }
}
fn asset_bytes(root: &Path, asset: &Asset) -> Result<Vec<u8>> {
    let body = bytes(&owned_target(root, &asset.source)?)?;
    if digest(&body) != asset.sha256 {
        return Err("portable source changed after validation".into());
    }
    Ok(body)
}
fn owned_target(root: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || Path::new(name).is_absolute()
        || name
            .split(['/', '\\'])
            .any(|p| p.is_empty() || p == ".." || p == "." || p.contains(':'))
    {
        return Err("unsafe installation asset target".into());
    }
    let path = root.join(name);
    ordinary(&path)?;
    Ok(path)
}
fn probe(exe: &Path, args: &[String]) -> Result<Value> {
    ordinary(exe)?;
    let mut q = ProcessRequest::new(
        exe.as_os_str(),
        exe.parent().ok_or("binary parent missing")?,
    );
    q.args = args.iter().map(Into::into).collect();
    q.timeout = Duration::from_secs(30);
    let output = SystemProcessRunner.run(&q).map_err(|e| e.to_string())?;
    if output.timed_out || output.code != 0 {
        return Err(format!(
            "installed program probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}
fn assets() -> Vec<(
    &'static str,
    &'static str,
    &'static str,
    Option<&'static [u8]>,
)> {
    vec![
        ("bp.core", "payload/bp-core.exe", "", None),
        (
            "bp.launcher",
            "payload/bp-launcher.exe",
            ".build/bridgepilot/bin/bp.exe",
            None,
        ),
        (
            "bp.controller.rules",
            "payload/controller-rules.md",
            "AGENTS.md",
            Some(RULES),
        ),
        (
            "bp.controller.readme",
            "payload/readme.md",
            "README.md",
            Some(README),
        ),
        (
            "bp.controller.install",
            "payload/install.md",
            "INSTALL.md",
            Some(GUIDE),
        ),
        (
            "bp.controller.skill",
            "payload/bridgepilot-skill.md",
            "source/orchestrator/skills/bridgepilot/SKILL.md",
            Some(include_bytes!("skills/bridgepilot/SKILL.md")),
        ),
    ]
}
pub fn bundle(output: &Path, core: &Path, launcher: &Path) -> Result<Value> {
    ordinary(output)?;
    if !output.is_absolute() || output.exists() {
        return Err("new explicit portable package directory required".into());
    }
    let status = probe(core, &["self-test".into()])?;
    let protocol = probe(launcher, &["--launcher-self-test".into()])?;
    let version = status["version"]
        .as_str()
        .ok_or("core version missing")?
        .to_owned();
    SemVer::from_str(&version)?;
    if status["program"] != "bridgepilot" || protocol["launcher_protocol"] != 1 {
        return Err("portable binaries have incompatible identity".into());
    }
    let mut package = Package {
        schema: 1,
        product: "BridgePilot".into(),
        version,
        launcher_protocol: 1,
        assets: Vec::new(),
    };
    for (id, source, target, body) in assets() {
        let body = match body {
            Some(b) => b.to_vec(),
            None if id == "bp.core" => bytes(core)?,
            None => bytes(launcher)?,
        };
        let path = owned_target(output, source)?;
        persistence::atomic_write(&path, &body).map_err(|e| e.to_string())?;
        package.assets.push(Asset {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            sha256: digest(&body),
        });
    }
    persistence::atomic_write_json(&output.join("package.json"), &package)
        .map_err(|e| e.to_string())?;
    Ok(
        json!({"status":"portable_package_created","root":output,"version":package.version,"assets":package.assets.len(),"archive_included":false}),
    )
}
fn package(root: &Path) -> Result<Package> {
    ordinary(root)?;
    let p: Package = read_json(&root.join("package.json"))?;
    if p.schema != 1 || p.product != "BridgePilot" || p.launcher_protocol != 1 {
        return Err("unsupported BridgePilot package".into());
    }
    SemVer::from_str(&p.version)?;
    let expected = assets();
    if expected.len() != p.assets.len() {
        return Err("portable asset registry mismatch".into());
    }
    for (asset, (id, source, target, _)) in p.assets.iter().zip(expected) {
        if asset.id != id
            || asset.source != source
            || asset.target != target
            || digest_file(&owned_target(root, &asset.source)?)? != asset.sha256
        {
            return Err("portable asset identity or bytes changed".into());
        }
    }
    let status = probe(&root.join("payload/bp-core.exe"), &["self-test".into()])?;
    if status["program"] != "bridgepilot" || status["version"] != p.version {
        return Err("portable core version differs from its actual program".into());
    }
    if probe(
        &root.join("payload/bp-launcher.exe"),
        &["--launcher-self-test".into()],
    )?["launcher_protocol"]
        != 1
    {
        return Err("incompatible portable launcher".into());
    }
    Ok(p)
}
pub fn inspect(home: &Path) -> Result<Installation> {
    ordinary(home)?;
    let installation: Installation = read_json(&config(home))?;
    if installation.schema != 1 || installation.product != "BridgePilot" {
        return Err("invalid installed-home identity".into());
    }
    let executable = owned_target(home, &installation.current.executable)?;
    if digest_file(&executable)? != installation.current.sha256 {
        return Err("installed core digest mismatch".into());
    }
    if digest_file(&home.join(".build/bridgepilot/bin/bp.exe"))? != installation.launcher_sha256 {
        return Err("installed stable launcher changed".into());
    }
    Ok(installation)
}
fn read_old(home: &Path) -> Result<Option<Installation>> {
    if config(home).exists() {
        Ok(Some(inspect(home)?))
    } else {
        Ok(None)
    }
}
fn workspace(home: &Path, requested: Option<&Path>) -> Result<Option<PathBuf>> {
    let configured = if workspace_config(home).exists() {
        let v: Value = read_json(&workspace_config(home))?;
        Some(PathBuf::from(
            v["workspace"]
                .as_str()
                .ok_or("installed workspace configuration invalid")?,
        ))
    } else {
        None
    };
    if let (Some(old), Some(new)) = (&configured, requested) {
        if fs::canonicalize(old).map_err(|e| e.to_string())?
            != fs::canonicalize(new).map_err(|e| e.to_string())?
        {
            return Err("upgrade cannot replace the configured private workspace".into());
        }
    }
    let result = configured.or_else(|| requested.map(Path::to_path_buf));
    if let Some(path) = &result {
        Store::open(path)?.read()?;
    }
    Ok(result)
}
fn reader_check(core: &Path, workspace: Option<&Path>) -> Result<()> {
    if let Some(workspace) = workspace {
        let result = probe(
            core,
            &[
                "status".into(),
                "--workspace".into(),
                workspace.to_str().ok_or("invalid workspace path")?.into(),
            ],
        )?;
        if result["schema"] != 1 {
            return Err("candidate program cannot read the actual configured archive".into());
        }
    }
    Ok(())
}
pub fn install(
    package_root: &Path,
    home: &Path,
    requested_workspace: Option<&Path>,
    preview: bool,
) -> Result<Value> {
    install_inner(package_root, home, requested_workspace, preview, false)
}
fn install_inner(
    package_root: &Path,
    home: &Path,
    requested_workspace: Option<&Path>,
    preview: bool,
    rollback: bool,
) -> Result<Value> {
    if !home.is_absolute() {
        return Err("absolute installation home required".into());
    }
    ordinary(home)?;
    if home.join("bridgeforge-manifest.json").exists() {
        return Err("do not install over the source/factory checkout".into());
    }
    if journal(home).exists() {
        return Err("interrupted installation requires install-recover first".into());
    }
    let p = package(package_root)?;
    let old = read_old(home)?;
    let claimed_launcher = claim(home)?;
    if old.is_none() && claimed_launcher.is_none() && !scaffold_only(home)? {
        return Err("unowned nonempty installation home is preserved".into());
    }
    let workspace = workspace(home, requested_workspace)?;
    reader_check(
        &package_root.join("payload/bp-core.exe"),
        workspace.as_deref(),
    )?;
    let core = &p.assets[0];
    let rel = format!(
        ".build/bridgepilot/versions/{}-{}/bp-core.exe",
        p.version,
        &core.sha256[..16]
    );
    let current = Version {
        version: p.version.clone(),
        executable: rel,
        sha256: core.sha256.clone(),
    };
    if let Some(old) = &old {
        let before = SemVer::from_str(&old.current.version)?;
        let after = SemVer::from_str(&p.version)?;
        let approved_rollback = rollback
            && old
                .previous
                .as_ref()
                .is_some_and(|v| v.version == p.version && v.sha256 == core.sha256);
        if after < before && !approved_rollback
            || after == before && old.current.sha256 != core.sha256
        {
            return Err(
                "installation cannot downgrade or reuse a version for different program bytes"
                    .into(),
            );
        }
    }
    let mut changes = Vec::new();
    let mut owned = Vec::new();
    for asset in p.assets.iter().skip(2) {
        let target = owned_target(home, &asset.target)?;
        let actual = if target.exists() {
            Some(bytes(&target)?)
        } else {
            None
        };
        let previous = old
            .as_ref()
            .and_then(|i| i.files.iter().find(|f| f.id == asset.id));
        if let Some(content) = &actual {
            if previous.is_none_or(|f| f.target != asset.target || f.sha256 != digest(content)) {
                return Err(format!(
                    "owned installation asset has user edits or unknown ownership: {}",
                    asset.target
                ));
            }
        }
        changes.push(Change {
            target: asset.target.clone(),
            old: actual,
            new_hash: asset.sha256.clone(),
        });
        owned.push(OwnedFile {
            id: asset.id.clone(),
            target: asset.target.clone(),
            sha256: asset.sha256.clone(),
        });
    }
    let launcher_hash = if let Some(hash) = old
        .as_ref()
        .map(|i| i.launcher_sha256.clone())
        .or(claimed_launcher)
    {
        let path = home.join(".build/bridgepilot/bin/bp.exe");
        if path.exists()
            && (digest_file(&path)? != hash
                || probe(&path, &["--launcher-self-test".into()])?["launcher_protocol"] != 1)
        {
            return Err("stable launcher needs an explicit offline maintenance update".into());
        }
        hash
    } else {
        p.assets[1].sha256.clone()
    };
    let previous = old.as_ref().and_then(|i| {
        if i.current == current {
            i.previous.clone()
        } else {
            Some(i.current.clone())
        }
    });
    let new = Installation {
        schema: 1,
        product: "BridgePilot".into(),
        current: current.clone(),
        previous,
        files: owned,
        launcher_sha256: launcher_hash,
    };
    if preview {
        return Ok(
            json!({"status":"installation_planned","home":home,"version":p.version,"workspace":workspace,"writes":false,"legacy_forge_unchanged":true}),
        );
    }
    if let Some(path) = &workspace {
        let snapshot = Store::open(path)?.read()?;
        if snapshot.state.runs.values().any(|r| r.state.active()) {
            return Err("finish or explicitly pause/reconcile owned work before switching installed runtime".into());
        }
    }
    fs::create_dir_all(home.join(".runtime/bridgepilot-install/locks"))
        .map_err(|e| e.to_string())?;
    let _lock = FileLock::acquire(&home.join(".runtime/bridgepilot-install/locks/writer.lock"))?;
    if read_old(home)? != old || journal(home).exists() {
        return Err("installation changed after preview".into());
    }
    for change in &changes {
        if !matches_bytes(&owned_target(home, &change.target)?, change.old.as_deref())? {
            return Err(
                "owned installation asset changed after preview; preserve user edits".into(),
            );
        }
    }
    let executable = owned_target(home, &current.executable)?;
    if !home_claim(home).exists() {
        persistence::atomic_write_json(
            &home_claim(home),
            &json!({"schema":1,"product":"BridgePilot","launcher_sha256":new.launcher_sha256}),
        )
        .map_err(|e| e.to_string())?;
    }
    let mut new_workspace = None;
    if !workspace_config(home).exists() {
        if let Some(path) = &workspace {
            let body = serde_json::to_vec(&json!({"schema":1,"workspace":path}))
                .map_err(|e| e.to_string())?;
            changes.push(Change {
                target: ".runtime/config/bridgepilot.json".into(),
                old: None,
                new_hash: digest(&body),
            });
            new_workspace = Some(body);
        }
    }
    let transaction = Journal {
        schema: 1,
        old: old.clone(),
        new: new.clone(),
        changes,
    };
    persistence::atomic_write_json(&journal(home), &transaction).map_err(|e| e.to_string())?;
    let result = (|| -> Result<()> {
        if executable.exists() {
            if digest_file(&executable)? != current.sha256 {
                return Err("immutable installation version collision".into());
            }
        } else {
            persistence::atomic_write(&executable, &asset_bytes(package_root, core)?)
                .map_err(|e| e.to_string())?;
        }
        let retained = executable.parent().unwrap().join("package");
        for asset in &p.assets {
            let target = owned_target(&retained, &asset.source)?;
            if target.exists() {
                if digest_file(&target)? != asset.sha256 {
                    return Err("retained package collision".into());
                }
            } else {
                persistence::atomic_write(&target, &asset_bytes(package_root, asset)?)
                    .map_err(|e| e.to_string())?;
            }
        }
        persistence::atomic_write_json(&retained.join("package.json"), &p)
            .map_err(|e| e.to_string())?;
        let launcher = home.join(".build/bridgepilot/bin/bp.exe");
        if !launcher.exists() {
            let body = asset_bytes(package_root, &p.assets[1])?;
            if digest(&body) != new.launcher_sha256 {
                return Err("cannot silently replace the claimed launcher".into());
            }
            persistence::atomic_write(&launcher, &body).map_err(|e| e.to_string())?;
        }
        for asset in p.assets.iter().skip(2) {
            let change = transaction
                .changes
                .iter()
                .find(|c| c.target == asset.target)
                .ok_or("installation change missing")?;
            if !matches_bytes(&owned_target(home, &asset.target)?, change.old.as_deref())? {
                return Err(
                    "owned installation asset changed during installation; preserve user edits"
                        .into(),
                );
            }
            persistence::atomic_write(
                &owned_target(home, &asset.target)?,
                &asset_bytes(package_root, asset)?,
            )
            .map_err(|e| e.to_string())?;
        }
        if let Some(body) = new_workspace {
            if workspace_config(home).exists() {
                return Err(
                    "workspace configuration appeared during installation; preserve it".into(),
                );
            }
            persistence::atomic_write(&workspace_config(home), &body).map_err(|e| e.to_string())?;
        }
        if digest_file(&executable)? != new.current.sha256 {
            return Err("candidate runtime changed during install".into());
        }
        for file in &new.files {
            if digest_file(&owned_target(home, &file.target)?)? != file.sha256 {
                return Err("candidate owned asset changed during install".into());
            }
        }
        reader_check(&executable, workspace.as_deref())?;
        persistence::atomic_write_json(&config(home), &new).map_err(|e| e.to_string())?;
        fs::remove_file(journal(home)).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        return Err(format!(
            "installation incomplete; run install-recover, previous runtime retained: {error}"
        ));
    }
    Ok(
        json!({"status":"installation_current","home":home,"version":p.version,"launcher":home.join(".build/bridgepilot/bin/bp.exe"),"workspace":workspace,"archive_migrated":false,"legacy_forge_unchanged":true}),
    )
}
pub fn recover(home: &Path) -> Result<Value> {
    ordinary(home)?;
    if !journal(home).exists() {
        return Ok(json!({"status":"installation_recovery_not_needed"}));
    }
    let _lock = FileLock::acquire(&home.join(".runtime/bridgepilot-install/locks/writer.lock"))?;
    let j: Journal = read_json(&journal(home))?;
    if j.schema != 1 {
        return Err("unsupported installation journal".into());
    }
    let current: Option<Installation> = if config(home).exists() {
        Some(read_json(&config(home))?)
    } else {
        None
    };
    if current.as_ref() == Some(&j.new) {
        for change in &j.changes {
            if digest_file(&owned_target(home, &change.target)?)? != change.new_hash {
                return Err("committed installation asset changed; preserve for review".into());
            }
        }
        inspect(home)?;
        fs::remove_file(journal(home)).map_err(|e| e.to_string())?;
        return Ok(
            json!({"status":"installation_commit_recovered","version":j.new.current.version}),
        );
    }
    if current != j.old {
        return Err("installation pointer changed outside transaction; preserve journal".into());
    }
    let mut observed = Vec::new();
    for change in &j.changes {
        let path = owned_target(home, &change.target)?;
        if !path.exists() {
            if change.old.is_some() {
                return Err("installation asset disappeared outside transaction".into());
            }
            observed.push(None);
            continue;
        }
        let hash = digest_file(&path)?;
        let old_hash = change.old.as_ref().map(|v| digest(v));
        if hash != change.new_hash && old_hash.as_ref() != Some(&hash) {
            return Err("installation asset has external edits; preserve journal".into());
        }
        observed.push(Some(bytes(&path)?));
    }
    for (change, expected) in j.changes.iter().zip(observed) {
        let path = owned_target(home, &change.target)?;
        if !matches_bytes(&path, expected.as_deref())? {
            return Err("installation asset changed during recovery; preserve journal".into());
        }
        match &change.old {
            Some(old) => persistence::atomic_write(&path, old).map_err(|e| e.to_string())?,
            None if path.exists() => fs::remove_file(path).map_err(|e| e.to_string())?,
            None => {}
        }
    }
    fs::remove_file(journal(home)).map_err(|e| e.to_string())?;
    Ok(json!({"status":"installation_rolled_back","archive_rolled_back":false}))
}
pub fn rollback(home: &Path) -> Result<Value> {
    ordinary(home)?;
    if journal(home).exists() {
        return Err("recover the pending installation before rollback".into());
    }
    let before = inspect(home)?;
    let previous = before
        .previous
        .clone()
        .ok_or("no prior installed version")?;
    let executable = owned_target(home, &previous.executable)?;
    if digest_file(&executable)? != previous.sha256 {
        return Err("prior installed runtime changed".into());
    }
    let workspace = workspace(home, None)?;
    reader_check(&executable, workspace.as_deref())?;
    if let Some(workspace) = workspace {
        if Store::open(&workspace)?
            .read()?
            .state
            .runs
            .values()
            .any(|r| r.state.active())
        {
            return Err("cannot roll back during active or unknown execution".into());
        }
    }
    let retained = executable.parent().unwrap().join("package");
    install_inner(&retained, home, None, false, true)
}
pub fn launch() -> Result<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--launcher-self-test"] {
        println!(
            "{}",
            json!({"program":"bridgepilot-launcher","launcher_protocol":1})
        );
        return Ok(0);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let home = exe
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or("stable launcher must reside at <home>/.build/bridgepilot/bin/bp.exe")?;
    if journal(home).exists() {
        recover(home)?;
    }
    let installation = inspect(home)?;
    let core = owned_target(home, &installation.current.executable)?;
    use std::process::{Command, Stdio};
    let mut command = Command::new(core);
    command
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut tree = bridgeforge_core::process::process_tree::ProcessTree::spawn(&mut command)
        .map_err(|e| e.to_string())?;
    let status = tree.child.wait().map_err(|e| e.to_string())?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn interrupted_asset_recovery_preserves_external_edits_and_restores_owned_bytes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.runtime/tests/bridgepilot-stage5/install-recovery")
            .join(crate::new_id("case"));
        fs::create_dir_all(root.join(".runtime/bridgepilot-install")).unwrap();
        let asset = root.join("AGENTS.md");
        let before = b"owned original";
        let after = b"transaction candidate";
        let new = Installation {
            schema: 1,
            product: "BridgePilot".into(),
            current: Version {
                version: "0.4.0".into(),
                executable: ".build/bridgepilot/versions/test/bp-core.exe".into(),
                sha256: "fixture".into(),
            },
            previous: None,
            files: Vec::new(),
            launcher_sha256: "fixture".into(),
        };
        let j = Journal {
            schema: 1,
            old: None,
            new,
            changes: vec![Change {
                target: "AGENTS.md".into(),
                old: Some(before.to_vec()),
                new_hash: digest(after),
            }],
        };
        persistence::atomic_write_json(&journal(&root), &j).unwrap();
        fs::write(&asset, b"user edit after transaction began").unwrap();
        assert!(!matches_bytes(&asset, Some(before)).unwrap());
        assert!(recover(&root).is_err());
        assert!(journal(&root).exists());
        assert_eq!(
            fs::read(&asset).unwrap(),
            b"user edit after transaction began"
        );
        fs::write(&asset, after).unwrap();
        assert_eq!(
            recover(&root).unwrap()["status"],
            "installation_rolled_back"
        );
        assert_eq!(fs::read(&asset).unwrap(), before);
        assert!(!journal(&root).exists());
        fs::remove_dir_all(root).unwrap();
    }
}
