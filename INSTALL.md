# BridgePilot 安装与迁移

当前候选工具仅支持骨架 8.0.0 及以后。更早项目的升级/格式迁移已退役，须在项目写入前拒绝；新项目初始化、8.x 当前配置与恢复保持。业务 VERSION 和独立子包版本不受该数字下限限制。

BridgePilot 当前支持 Windows 和 Codex。源码开发及 Forge 工程维护需要 Rust/Cargo 1.88+；Cargo 在安装、升级及工厂运行时准备时按需构建
受管 Rust 工具；日常 Hook 不调用 Cargo 或脚本解释器。请按自己的情况直接进入对应章节：

- 第一次使用：阅读“首次安装”。
- 使用过旧 `$bridgeforge`：阅读“旧 BridgeForge 用户重新安装”。
- 项目更新被阻断：阅读“项目升级与异常诊断”。
- 维护产品或同步器：阅读“维护者协议”。

## 工厂本地运行时准备

工厂源码可由 Git 更新，但 `.build/bridgeforge/bin` 是被忽略的本地产物。需要显式刷新工具或完成开发必要构建时执行下列入口；普通 `$git-sync` 和 `$git-sync release` 使用已安装工具，不将准备或独立 self-test 作为日常前置：

```powershell
cargo run --locked --release --manifest-path Cargo.toml --target-dir .build/bridgeforge/cargo/local --bin bridgeforge-factory -- build-assets --factory --project-root .
```

该入口不依赖已安装的 CLI，不提交或发布，不安装下游骨架。
仓库不设置根 `.cargo` 目录。所有构建、检查和测试入口显式传入 `--target-dir`，输出必须位于本仓库 `.build/`；从仓库根目录执行本文命令。不要省略该参数：直接运行未指定输出目录的原生 Cargo 命令仍会使用其默认 `target`，受管入口不能替任意手写命令改变默认行为。
它先核验已安装基线，再从当前产品源生成候选 dogfood、manifest 和入口，并核对 CLI、Hook、工厂工具的源码、锁文件、配方、产物
哈希、自检与收据；健康产物不改写，失效产物复用经验证的缓存或执行锁定构建。
候选投影、入口、三种产物及收据共用安装回滚事务。旧 `.codex/bin/` 文件仅在历史收据及实际字节可证明归属时退役；未知文件保留并阻断。维护锁仍协调构建与 develop；git-sync 不再持有该锁或支持同仓库并行同步。输入漂移、构建失败、
文件占用或安装校验失败均停止；并发外部修改保留并报告，不覆盖其他进程的文件。

Cargo 引导可能产生增量构建及缓存写入，只用于已授权维护。引导产物未建立完整输入
身份时不冒充正式产物复用；首次准备可能仍需隔离构建，实际耗时以执行结果为准。
当前产品源可以生成新的候选投影；已有镜像被外部改坏或旧文件归属不明时阻断，不自动改写产品源。单独 `manifest` 不得在新工具尚未就绪时切换运行入口。

项目目录遵守三目录合同：工厂生成工具及编译缓存放在 `.build/bridgeforge/`，消费项目的已安装公共工具放在 `.deps/bridgeforge/bin/`；用户级 `$env:USERPROFILE/.codex/bin/` 属于独立安装作用域，保持不变。项目本机配置、下载和测试结果分别使用 `.runtime/config/`、`.runtime/download/`、`.runtime/tests/<任务>/`。三个根全部由项目 `.gitignore` 忽略，受版本管理的配置继续留在原所有权位置。

只读诊断使用已支持 `factory-runtime-prepare-v1` 的工具：

```powershell
.build/bridgeforge/bin/bridgeforge-factory.exe build-assets --factory --project-root . --check
```

`--check`（以及同义 `--dry-run`）不创建锁、缓存或收据，不启动 Cargo 或产物自检进程；
它只证明当前源码/镜像与产物哈希、收据一致。缺失或旧工厂工具不支持此入口时报告检查不可用，
不得改用 `cargo run` 声称零写入。直接绕过准备入口运行旧 EXE 仍可能使用旧程序。
不新增拉取后构建 Hook，pre-commit 保持校验职责。下游继续使用原有受管安装升级流程。

严格只读检查对 `.gitattributes` 只接受能够直接证明的默认 `* text=auto eol=lf` 规则及注释；复杂属性语法报告无法证明，不创建临时 Git 仓库兜底。普通基线核验保留完整 Git 属性语义检查。

## 首次安装

1. 克隆产品仓库：

   ```powershell
   git clone https://github.com/freakybridge/BridgePilot.git "$env:USERPROFILE\tools\BridgePilot"
   Set-Location "$env:USERPROFILE\tools\BridgePilot"
   ```

2. 用锁定的 Cargo workspace 构建工厂 dogfood 工具，再安装用户级入口、通用 Skills 和统一用户指令：

   ```powershell
   cargo build --locked --release --manifest-path .\Cargo.toml --target-dir .build/bridgeforge/cargo/local --bin bridgeforge
   & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install-shared-skills.ps1
   ```

3. 新开 Codex 会话，在目标项目根目录运行：

   ```text
   $bridgeforge-codex
   ```

安装器会把完整产品仓库原子安装到 `~/.bridgeforge-codex`，只在
`~/.codex/skills/bridgeforge-codex` 保留供 Codex 发现的薄入口，并把其他受管 Skills
写入 `~/.codex/skills/`。`~/.codex/bridgeforge-codex-managed.json` 记录哪些用户级
Skills 由产品管理，避免覆盖来源不明或被人工修改的 Skill 文件。

安装和获准维护同时分发 `source/forge/templates/user/AGENTS.md` 到默认 `~/.codex/AGENTS.md`。
文本相同则不写；本地有不同内容（包括个人编辑）时，原始文件先保存为
`~/.codex/.AGENTS.bridgeforge-backup-<operation_id>.md`，再覆盖为统一标准版。
备份成功后保留，收据给出具体路径；需要恢复时可用对应备份替换 AGENTS.md，但下次维护仍会恢复标准版。
这不改变项目专区保留规则，也不删除 `AGENTS.override.md`；现有会话不保证重载，请新开会话。
自定义 `CODEX_HOME` 不在本功能管理范围。旧薄入口首次刷新缺少用户指令收据时，按新版 Skill 补跑一次入口完成过渡。

项目侧运行面包括 `AGENTS.md`、`.codex/` 和 `.githooks/pre-commit`。项目同步器会先给出
计划，需要决定的项目会停下来确认；无法安全判断的冲突直接停止，应用失败时回滚。项目
指令由根或嵌套 `AGENTS.md` 原生加载，机器可判约束由 Hook 或 pre-commit 执行，操作流程
由 Skill 执行，长说明放在 `doc/`。

项目 `.codex/settings.json` 由项目自行管理。BridgeForge 初始化、接管、升级和生命周期

骨架 Hook 与项目自有 Hook 独立维护，边界固定在代码中。安装和升级仅维护公共源码、产物与公共注册，保留项目 Hook 源码、程序、收据、注册及扩展字段。旧 project-hook-policy.json 和迁移状态文件保留并忽略；项目 Hook 的构建、安装、路径迁移和业务验证由项目自己的开发流程负责。
Hook 不创建、修改或删除该文件，也不生成或覆盖其备份；旧版安装留下的内容原样保留。

## 统一清单格式的入口刷新

产品根清单已统一为 `bridgeforge-manifest.json`，包含 `user_instructions`、`shared_skills` 和 `project_skeleton`。三个旧源清单不再保留；下游项目的 `.codex/managed-skeleton.json` 继续由项目分区自动生成。

此前已安装的薄入口仍查找 `bridgeforge-codex-manifest.json`，首次更新到统一清单版本可能报“Missing root distribution manifest”。这不是下游项目文件损坏。请从可信的产品仓库运行现有安装入口，完成一次用户级入口刷新：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/install-shared-skills.ps1
```

安装入口重新获取并验证官方 main，再运行新 checkout 内的 updater；它不依赖旧薄入口解析新清单。共享 Skills 的归属冲突校验、用户 AGENTS 原始字节备份及事务回滚保持有效，不手工复制或伪造旧清单来绕过校验。刷新后新开会话，再从目标项目执行 `$bridgeforge-codex`。若旧入口留下未完成事务，先按现有事务诊断恢复，不删除日志。

## 旧 BridgeForge 用户重新安装

旧 `$bridgeforge` 到 `$bridgeforge-codex` 的自动迁移已经退役。请直接执行上面的“首次安装”
流程。

当前安装器不会读取、接管或删除旧 Codex/Claude 受管记录、旧 `.bridgeforge` 产品目录或
旧用户级 Skill。如果正式 Skill 的目标路径已被旧安装占用，安装器会把它报告为“不属于
当前产品管理的冲突”并停止，不会根据旧记录猜测文件归属。

## 项目升级与异常诊断

项目骨架版本记录在 `.codex/.bridgeforge_codex_version`。咨询、诊断或只读预览不更新安装；明确维护（含无附加限制的 `$bridgeforge-codex`）才刷新官方产品 home，随后分流：

- 没有骨架身份和骨架资产的空白项目：直接初始化，不要求预先存在版本戳。
- 低于修复首版 `1.8.6`：盘点并确认项目资产后重建最新骨架，不读取旧 schema、旧 manifest 或逐版本兼容链。
- 等于或高于 `1.8.6`：兼容更新并保留项目定制；升级分界线固定，不随最新发布版本自动上移。比产品 home 更新的版本拒绝降级。
- 已有骨架资产但无戳：受控接入；双戳或非法戳在项目写入前停止。

旧 Markdown Rule／项目 Memory 的语义迁移已经退役。`.codex/rules/*.md` 和 `.codex/memory/**` 不再生成迁移包，也不会被固定删除；兼容更新保留这些文件，低基线重建或 adopt 时按普通项目自有资产的精确路径确认保留或删除。原生 `~/.codex/memories/` 不属于本操作范围。

可选项目资产通过一次性 `PreservationManifest` 明确保留或删除；确认前项目零写入，不长期保存该 manifest。资产处理、最新基线及验证属于同一项目事务；可捕获失败恢复本事务能够证明归属的改动，外部编辑保留并报告未恢复项。同步器最后才写当前版本戳，`.codex/managed-skeleton.json` 只保存当次最新 schema 4 的资产归属和内容哈希。

[子包开发收口与主程序发布](doc/0_architecture/design/package-version-lifecycle.md)要求 `package-development-v1`、`package-stability-v1`、`git-sync-development-closure-v1`；任务检查选择和版本覆盖还须核验 `task-selected-development-checks-v1`、`version-coverage-v1`。直接 apply 和阶段间复用分别核验 `direct-development-apply-v1`、`staged-development-validation-v1`，优先消费实际入口收据，不默认另跑 self-test。安装或骨架升级成功不等于项目已配置业务检查或完成版本收口。配置及实际入口只维护在[版本生命周期合同](source/forge/templates/skills/git-sync/references/schema4.md)，保留原发布基线和下游 Python 环境支持，不借安装说明自动收口或写入真实下游。

## BridgePilot 本地试用与档案恢复

BP 源码在 `source/orchestrator/`，与 Forge 使用同一个锁定的 Rust workspace；也可用独立便携包安装，无需新机编译 Rust。它不替换 Forge 既有安装、公共骨架名称或共享 Skill。普通用户只通过 Codex 对话下达目标，以下命令由当前 Agent 按真实路径准备和恢复。

从产品仓库根构建专用 CLI，编译产物只进入 `.build/`：

```powershell
cargo build --locked --offline --release --manifest-path Cargo.toml --target-dir .build/bridgepilot/cargo -p bridgepilot-orchestrator --bin bp --bin bp-launcher
```

使用产品仓库之外的明确私有目录保存个人档案。下例路径由本机选择，换机时重新选择，不能写成公共分发默认值：

```powershell
$bpWorkspace = Join-Path $env:USERPROFILE 'Documents/Codex/BridgePilotWorkspace'
& .build/bridgepilot/cargo/release/bp.exe init --workspace $bpWorkspace
& .build/bridgepilot/cargo/release/bp.exe configure --product-root $PWD.Path --workspace $bpWorkspace
& .build/bridgepilot/cargo/release/bp.exe status --workspace $bpWorkspace
```

产品或安装目录的 `.runtime/config/bridgepilot.json` 仅记录本机工作区位置；档案工作区的 `.runtime/config/bridgepilot-machine.json` 记录本机身份。实际任务及决定保存在 `<工作区>/doc/5_project_knowledgebase/bridgepilot/`：`HEAD.json` 选择不可变 `revisions/`。不得手工编辑，也不能只备份 `.runtime/`。个人档案不进入产品仓库、分发清单或原生 Memory。

### 私有档案同步

1. 每机准备 Git、GitHub CLI（`gh auth status`）和 Codex 登录；不复制凭据。sync-configure 的输入为 `{"repository":"<已授权 owner/repository>","branch":"main","source":"<真实用户授权来源>"}`，配置只在本机 `.runtime/config/bridgepilot-sync.json`。用户本次选择的独立私有库是 `freakybridge/BridgePilotData`；示例不是其他用户的默认值。
2. 当前控制机首次用 status 的 revision 调用 `sync-enrol --expected <revision> --source <来源>`，加入可验证的控制权。旧程序不支持 custody，因此启用前确保新程序已就绪；此操作不自动启动任务或恢复心跳。
3. 每个空闲批次边界 `sync-upload --workspace <工作区>`，只上传 HEAD 与完整历史；不传本机配置、日志、源码、原生会话和 Memory。每次抓取／发布都读回准确 GitHub 私有属性；归档图和 Git 图分别校验，不自动合并分叉。活跃／Unknown Run 先处理，不在网络锁内阻塞 worker。
4. 新机空工作区 init、sync-configure 后 `sync-download`，导入仍只可观察；旧机器身份不复制，不从同名仓库推导执行权。待交接完成才能写任务。pending 状态先 `sync-reconcile`，只读查询实际远端，不重发未知 push。远端仍为原预期头、确认需要放弃旧发布时，`sync-fence-pending --source <明确来源>` 以兄弟 Git 提交建立栅栏；首次同步的空远端也支持。它不会重发旧候选，放弃 accept 不激活新 owner，迟到旧 push 先赢时仍核销真实结果。原机取消尚未确认的释放也可用 `sync-cancel-pending`；禁止删除 pending 或回退 HEAD。

### 换机交接

早期、已经在派发前核销的终态 Run 若缺进程 birth，PID 又被其他程序复用，可用 `legacy-server-probe` 做本机只读核验：必须有原始调用正文、真实 supervisor 日志和原始核销收据，绑定当时 Codex 路径、工作区、Run／owner；当前原 owner 必须不存在，当前 OS 进程名须不同于原 Codex，原生同 ID 聊天必须全部终态。证明仅保存在本机 .runtime，不改写历史 Run、不终止进程；Unknown、仍存活 owner、已有任务派发或无法证明时仍拒绝。首次 sync-enrol 仅要求无活跃／Unknown，它登记本机备份控制权，不宣称全部历史进程已经核验；真正交接仍走严格 stopped 检查。

1. 用户须明确全部暂停。以实际终态证明无活跃／Unknown，核对原 owner/server 已停止；暂停请求成功不等于停止。各项目脏代码先由主对话按项目已授权的受管 git-sync 保存工作分支并推送；没有提交／推送授权时保留脏文件并报告依赖阻点，BP 不内部代做提交。
2. 每个登记项目调用 `checkpoint-probe --project <id> --remote <name> --ref refs/heads/<branch> --source <来源>`。它只消费实际干净工作树、Git HEAD／分支、登记推送地址与固定远端 ref，不伪造检查点。将真实 Checkpoint 列表写入 `{"checkpoints":[...],"source":"<明确交接来源>"}`，交 `handoff-release --input <JSON>`。本机先冻结，远端确认后形成 transfer；这是整个控制器交接，不能只停部分项目。
3. 新机安装 BP、初始化自己的工作区和机器身份、下载完整档案；在各项目检出检查点的准确分支／提交。查询实际原生 projectId、host、canonical root，以 `{"transfer":"<handoff-status 中的 ID>","bindings":[{"project":"...","machine":"<新机 ID>","root":"<真实绝对路径>","desktop_project_id":"...","host":"local"}],"source":"..."}` 调用 handoff-accept。所有登记项目都必须通过代码核验；先远端确认独占接管，再激活本地控制权，竞争失败不激活。
4. 接管不自动 resume 或执行。按明确项目恢复，再以 `handoff-continue --expected <revision> --input <JSON>` 指定 tasks/source。旧 Run、confirm 和结果保留；新 Run 使用新机身份、新聊天并带上旧阶段／结果上下文，证据需重新核实。不把模型完成当作发布，不绕过新聊天工具的人类授权。已交付任务不重复执行。
5. 原机取消已确认但尚未被接受的交接使用 handoff-cancel；新机已接受后拒绝覆盖。网络未知用 sync-reconcile，不盲重做。原机心跳配置不复制到新机，保持用户暂停偏好，新控制对话的唤醒须重新核验。

### 独立安装、升级与回退

```powershell
# 源码机打包；输出必须是新的绝对目录，包内没有私有档案。
& .build/bridgepilot/cargo/release/bp.exe bundle --output '<absolute-package-dir>' --core '<absolute-bp.exe>' --launcher '<absolute-bp-launcher.exe>'
# 新机只需包，先检查后安装；home 使用独立目录，不能覆盖 Forge 源码根。
& '<package>/payload/bp-core.exe' install-preview --bundle '<package>' --home '<absolute-home>' --workspace $bpWorkspace
& '<package>/payload/bp-core.exe' install --bundle '<package>' --home '<absolute-home>' --workspace $bpWorkspace
& '<home>/.build/bridgepilot/bin/bp.exe' self-test
```

在 Codex 打开安装 home，根 AGENTS 导航到随包的 BP 私有 Skill；它从本机配置定位档案。稳定入口是 `.build/bridgepilot/bin/bp.exe`，实际 core 在 `.build/bridgepilot/versions/<version>-<hash>/`。home 的 `.runtime/` 保存配置及事务日志，`.build/` 保存产品，`.deps/` 预留本机依赖；不改 PATH，不搬迁 Forge 或用户登录信息。

升级重复 preview/install，保留旧完整包及配置，最后原子切换 current。用户改过的受管文件拒绝覆盖；同版本不同程序字节拒绝复用。失败用包内 core 的 `install-recover --home <home>`；稳定 launcher 也会先恢复中断事务。回退用 `install-rollback --home <home>`，必须保留旧包、没有活跃执行，且旧 reader 能读取当前实际档案；否则拒绝。安装不升级／回滚档案，启用 custody 后不能回退到不支持它的 reader。真实双物理机、掉电和网络盘环境仍须区分于隔离工作区测试。

`status` 不创建锁或重建运行目录。HEAD 缺失、摘要冲突或历史断裂时停止写入，保留文件供诊断。当前入口的字段与能力以 `bp help`、`bp self-test` 为准。

已配置私有工作区后，由当前 Codex 对话准备监督请求 JSON，明确 `projects`、`max_parallel`（1–8）、`timeout_seconds`（1–86400，且不超过用户本阶段剩余时间）和 `stop_when_idle`。以下为入口形式，实际路径及 Codex 程序必须从本机核实：

```powershell
& .build/bridgepilot/cargo/release/bp.exe supervise --workspace $bpWorkspace --codex '<actual-absolute-codex.exe>' --input '<actual-supervision-request.json>'
```

`stop_when_idle=false` 在该进程和时限内持续观察新任务及实际客户端回执；`true` 在无可处理任务后退出。`actions` 列出创建、原生交接及归类核对动作，仍需当前 Codex 客户端处理。桌面 idle 不释放 writer：先保存原生归档确认，再 `chat-restore` 仅恢复持久记录，明确 `notLoaded` 后才允许执行；未知操作不自动重试。不要在 BP 持有执行时打开同一对话。任务 Completed、owner/server 已退出且项目无在途／Unknown 时，BP 按私有 Skill 核对客户端显示交接；必要时用原生客户端完成一次归档／恢复并保存回执，清除原先客户端的归档隐藏状态。已有成功回执不重复执行，不能把该步骤用于运行中聊天；仅后台恢复持久记录不代表侧栏已经显示。

监督进程只回收自己启动的进程树，未确认终态保持 Unknown。BP 不安装额外 OS 服务；持续唤醒使用本 BP 对话的原生 heartbeat，由 Codex 客户端登记并保持实际回执。重启先读状态，不接管外部／未知执行。Windows 新记录保存进程创建身份，旧记录仍保守查询，不按外部 PID 强制终止。完整协议见[运行协议](doc/0_architecture/design/bridgepilot-runtime-protocol.md)。

持续唤醒的准备和恢复：

1. 使用包含 wake-plan／wake-check 能力的新 BP 程序。早期 schema 1 档案可直接读取；新版本写入 wakeups 后不要再用旧版程序操作档案。先 status 核对现有注册，不重复创建。
2. 当前 Codex 对话以实际项目集合、controller_thread_id、controller_project、周期和用户来源调用 wake-plan；先 wake-attempt，再用原生 automation_update 创建附着此对话的 heartbeat，不指定新的 model/effort。客户端实际回执交 wake-bind，程序只读核对原生配置，不手改 automation.toml。
3. 每次实际唤醒先 wake-check，核对本机、当前控制对话和原生配置。原生配置缺失、暂停、周期／提示词／目标漂移时停止依赖动作，不从旧 Active 记录继续派发。保持电脑和桌面应用运行；关闭应用／关机后的执行不在保证范围。
4. 修改范围时先在原生接口暂停，绑定实际 Paused，再 wake-revise／wake-plan 获取同 native ID 的更新动作；更新后重新绑定。不用重新创建、删除历史或恢复旧配置来绕过未知状态。换机不复制旧机自动化／本机身份，也不自动接管旧机任务。

开发与发布核验由当前 BP Agent 操作，你无需手填命令。verify-development 需要当前 Completed Run、明确包集合和项目已有受管工具；它只核对现有收口，不补做 develop。发布前仍需本任务的明确发布权限及受管 git-sync release；把真实 JSON 收据保存到项目 .runtime，再 verify-publication。该命令只读核对固定提交和实际 push URL，不替用户提交、推送或升版。模型说已完成和普通同步都不会自动标为 Released。原生归类使用 chat-group 的实际项目／成员读回，摘要空值或 move 确认不能单独代替证据。

## 维护者协议

本页说明安装、迁移与阻断行为；内部合同见：

- [Codex 项目同步设计](doc/0_architecture/design/codex-project-sync.md)：版本路由、规划、确认、事务和回滚合同。
- [设计依据](doc/0_architecture/design/design-rationale.md)：资产归属与安全取舍。
- [原生指令架构](doc/0_architecture/design/codex-native-instruction-architecture.md)：AGENTS、Hook、Skill 与文档的职责边界。

## 已退役能力

bridgeforge-codex 不再安装或维护 `CLAUDE.md`、`.claude/`、Claude 入口、host switch、
project finalizer、setup junction 或 harness parity。项目中发现 Claude 遗留时只报告存在，
不读取、不迁移、不删除。

Markdown `paths:` 不是 Codex 的指令加载机制，安装过程不会建立自研 Markdown Rule 加载器。
