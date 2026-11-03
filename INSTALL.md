# BridgePilot 安装与迁移

当前本地候选采用 Template-only，同步器不再解释旧项目合同、版本配置、账本或版本戳。下游 `.codex` 递归只允许 hooks.json；未知额外 JSON 阻断，已知退役项经精确计划、备份与事务清理。业务原生版本和数据不由骨架旁文件接管。

BridgePilot 当前支持 Windows 和 Codex。源码开发及 Forge 工程维护需要 Rust/Cargo 1.88+；Cargo 在安装、升级及工厂运行时准备时按需构建
受管 Rust 工具；日常 Hook 不调用 Cargo 或脚本解释器。请按自己的情况直接进入对应章节：

- 第一次使用：阅读“首次安装”。
- 使用过旧 `$bridgeforge`：阅读“旧 BridgeForge 用户重新安装”。
- 项目更新被阻断：阅读“项目升级与异常诊断”。
- 维护产品或同步器：阅读“维护者协议”。

## 工厂本地运行时准备

工厂源码可由 Git 更新，但 `.build/bridgeforge/bin` 是被忽略的本地产物。需要显式刷新工具或完成开发必要构建时执行下列入口；普通 `$git-sync` 使用已安装工具，不将准备或独立 self-test 作为日常前置：

```powershell
cargo run --locked --release --manifest-path Cargo.toml --target-dir .build/bridgeforge/cargo/local --bin bridgeforge-factory -- build-assets --factory --project-root .
```

该入口不依赖已安装的 CLI，不提交或发布，不安装下游骨架。
仓库不设置根 `.cargo` 目录。所有构建、检查和测试入口显式传入 `--target-dir`，输出必须位于本仓库 `.build/`；从仓库根目录执行本文命令。不要省略该参数：直接运行未指定输出目录的原生 Cargo 命令仍会使用其默认 `target`，受管入口不能替任意手写命令改变默认行为。
它依据工具侧产品清单及当前原生源码生成候选 dogfood、manifest 和入口，不读取旧项目合同或版本配置；同时核对 CLI、Hook、工厂工具的源码、锁文件、配方、产物
哈希、自检与收据；健康产物不改写，失效产物复用经验证的缓存或执行锁定构建。
候选投影、入口、三种产物及收据共用安装回滚事务。旧 `.codex/bin/` 指纹自动迁移已退役，遇到该目录保留并阻断，独立核销恢复现场。维护锁仍保护必要构建；git-sync 不再持有该锁或支持同仓库并行同步。输入漂移、构建失败、
文件占用或安装校验失败均停止；并发外部修改保留并报告，不覆盖其他进程的文件。

Cargo 引导可能产生增量构建及缓存写入，只用于已授权维护。引导产物未建立完整输入
身份时不冒充正式产物复用；首次准备可能仍需隔离构建，实际耗时以执行结果为准。
当前产品源可以生成新的候选投影；已有受管镜像与当前产品源不一致时，只读 `--check` 阻断，明确授权的准备入口可在同一恢复事务中修复已声明的镜像，不改写产品源。未知旧文件、归属不明及准备期间的并发变化继续保留并阻断。单独 `manifest` 不得在新工具尚未就绪时切换运行入口。

项目目录遵守三目录合同：工厂生成工具及编译缓存放在 `.build/bridgeforge/`，消费项目的已安装公共工具放在 `.deps/bridgeforge/bin/`；用户级 `$env:USERPROFILE/.codex/bin/` 属于独立安装作用域，保持不变。项目本机配置、下载和测试结果分别使用 `.runtime/config/`、`.runtime/download/`、`.runtime/tests/<任务>/`。三个根全部由项目 `.gitignore` 忽略，受版本管理的配置继续留在原所有权位置。

只读诊断使用已支持 `factory-runtime-prepare-v1` 的工具：

```powershell
.build/bridgeforge/bin/bridgeforge-factory.exe build-assets --factory --project-root . --check
```

`--check`（以及同义 `--dry-run`）不创建锁、缓存或收据，不启动 Cargo 或产物自检进程；
它只证明当前源码/镜像与产物哈希、收据一致。缺失或旧工厂工具不支持此入口时报告检查不可用，
不得改用 `cargo run` 声称零写入。直接绕过准备入口运行旧 EXE 仍可能使用旧程序。
不新增拉取后构建 Hook，pre-commit 保持校验职责。下游继续使用原有受管安装升级流程。

公共 `.gitattributes` 默认文本使用 LF，`.bat`、`.cmd` 使用 CRLF；PowerShell 与 VBScript 不统一强制 CRLF。更新保留项目附加规则，显式冲突阻断而不覆盖。严格只读检查支持可证明的简单规则和路径通配；宏、带引号路径等不支持的语法报告无法证明，不创建临时 Git 仓库兜底。普通基线检查保留完整 Git 属性语义核验，并同时检查公共换行例外。

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

项目旧 `.codex/settings.json`、`project-hook-policy.json` 和 `project-hook-migrations.json` 不再分发。同步器只对可识别的历史默认 settings 及明确所有权、结构和状态的旧策略／迁移状态生成退役动作；未知或项目修改的内容保留并阻断，不能按名称盲删。当前版本登记仍引用待退役文件时，先经受管配置入口解除当前引用，历史基线与账本保持。dry-run 不写文件，apply 纳入同一指纹与回滚事务；生命周期 Hook 不创建这些旧文件或备份，用户级 settings 不在退役范围。

骨架 Hook 与项目自有 Hook 独立维护，边界固定在代码中。安装和升级仅维护公共源码、产物与公共注册，保留项目 Hook 源码、程序、收据、注册及扩展字段；项目 Hook 的构建、安装、路径迁移和业务验证由项目自己的开发流程负责。

## 统一清单格式的入口刷新

产品根清单已统一为 `bridgeforge-manifest.json`，包含 `user_instructions`、`shared_skills` 和 `project_skeleton`。三个旧源清单不再保留；该清单仅作为工具侧事实源，下游不再生成 `.codex/managed-skeleton.json`。

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

咨询、诊断或只读预览不刷新安装；明确维护才按 bridgeforge-codex 刷新已验证产品与公共产物。项目 `project-sync --project-root <根> --template-root <产品根>` 生成零写计划，`init/adopt/update/auto` 共用通用规则，不按旧版本戳分流。

旧项目合同、版本配置、账本、目录覆盖及 Hook 旁文件进入精确退役；未知额外 JSON 保留并阻断。AGENTS 项目区、pre-commit 私有扩展、原生 Hook 和 Skill 保持项目所有。Apply 用同一指纹和 prepared 公共产物，先备份后安装／删除，再校验模板、编译来源和递归仅 hooks.json；失败恢复自身改动、保留外部变化。不写新版本戳、PreservationManifest 或替代项目配置。

版本、构建、测试与发布按项目原生入口执行；Forge 配置化 develop/release/setup/migration 以及依赖旧 keep 声明的 cleanup 已退役。原生 Memory 的用户级同步不属于项目 JSON 清理范围。详细规则见[项目同步设计](doc/0_architecture/design/codex-project-sync.md)。

## BridgePilot 本地试用与档案恢复

### 产品版本与安装格式

BP 主产品从 `1.0.0` 起，以根 VERSION 为准。BF 以 `source/forge/cli/Cargo.toml`、orchestrator 以 `source/orchestrator/cli/Cargo.toml` 为版本事实源，内部包版本仅作构建身份。`bp versions`／`self-test` 输出 BP 版本和组件版本；Forge 的来源引用不是实际安装回执，查询外部 BF 必须读取实际工具。

BP 1.0.0 使用 package／installation schema 2，固定十三项资产，launcher protocol 2 支持读取 installation schema 1 与 2；包格式和 launcher 协议分别校验，不按 BP 产品版本推断资产。旧 schema 1 仍按原 0.x 格式读取，其 version 表示 orchestrator。迁移先核对新 orchestrator 不低于旧版本，再检查实际档案读兼容、文件所有权及组件／二进制身份；未知额外字段和漂移保持阻断。

使用新包 core 直接执行 install-preview／install，稳定入口的旧 launcher 升级与 home claim 在同一恢复事务内完成；占用、漂移或失败时不切换安装身份，保留现场并沿 install-recover 恢复。不要通过正在运行的旧稳定 launcher 自行覆盖它。新格式禁止直接回退到无法读取新安装身份的旧 core；同格式回退仍要求精确匹配已保留版本及档案读兼容。安装和预览不迁移私有档案，不更新外部 BF 或接管任务。

BP 源码在 `source/orchestrator/`，按 tasks、storage、execution、transfer、install、cli 六包分工，与 Forge 使用同一个锁定的 Rust workspace；cli 保留原生包名 `bridgepilot-orchestrator` 及 `bp`、`bp-launcher` 入口。也可用独立便携包安装，无需新机编译 Rust。它不替换 Forge 既有安装、公共骨架名称或共享 Skill。普通用户只通过 Codex 对话下达目标，以下命令由当前 Agent 按真实路径准备和恢复。

### 开发仓库、安装目录与档案的边界

| 目录角色 | 用途与约束 |
|---|---|
| 正式开发仓库 | 根目录包含 `.git`、`Cargo.toml`、`source/forge/` 与 `source/orchestrator/`，保存人工开发及未提交改动；两子系统共用同一个 Git 仓库和锁定 workspace，不放入任何项目的 `.runtime/`、`.deps/` 或 `.build/` |
| 源码更新脚本的独立检出目录 | 默认为 `~/.bridgepilot-source`，用于获取官方干净 `main` 并构建安装包；不是有本地开发改动时的自动更新入口 |
| 安装 home | 已安装程序、BP 私有 Skill 投影和本机安装配置；从用户级 Skill 的 `bridgepilot-local.json` 解析，不以开发仓库路径替代 |
| 私人档案工作区 | 从 home 的 `.runtime/config/bridgepilot.json` 解析，保存 SQLite、项目绑定和任务档案；与产品源码及安装 home 分离 |

开始源码开发先核对当前真实 Git 根、origin 和两子系统源码路径。聊天的工作目录、某个 `source/` 目录或便携安装 home 本身均不足以证明是完整开发仓库。临时下载和测试副本不得直接登记为正式项目路径，也不得持续在其中开发。

已有开发仓库放错位置时，先停止相关写入，记录 HEAD、refs、索引、工作区修改和未跟踪文件，为源码及 Git 元数据建立可校验的恢复归档。迁移须包含隐藏文件、空目录、目录属性和必要权限；目标已有内容逐项比对，同名差异不覆盖。运行资料按对应三目录合并，历史收据保留原路径，实际入口改用新根；绝对路径使当前产物身份失效时按原受管流程重建，不改写旧收据或版本账本。纯迁移核验通过后才实施规则修改，分别记录两类差异。

迁移不重新初始化 Git，不自动运行源码更新脚本，不执行安装升级、档案接管或下游同步。冻结恢复副本只用于回退，不继续开发；删除前说明精确目标、影响和恢复方式并取得明确授权。本机迁移记录从 [BP 源码根迁移计划与结果](doc/1_delivery/source-workspace-refactor/plan_2026-11-02_bp-source-root-migration.md)进入。

### 无参数源码安装与更新

已安装用户级入口后，在 Codex 调用 **`$bridgepilot`**，无需参数。入口从 `https://github.com/freakybridge/BridgePilot.git` 的 `main` 获取源码，固定本次提交，使用锁定的 `Cargo.lock` 构建 `bp` 和 `bp-launcher`，再由 Rust 安装器预览、安装并更新全部受管用户级 Skill。重复调用会更新同一安装。`init`、`status`、项目登记和交接模式保留原行为，不隐式更新程序。

Windows 源码构建需要 Git、Rust 1.88 或更高版本及其 Windows C++ 构建工具；可复用本机已有环境。缺依赖时先报告具体缺口，按用户允许的安装范围准备，不自动提权或脚本化登录。构建可按 Cargo.lock 下载依赖，构建失败不切换当前安装。

默认独立源码目录为 `$env:USERPROFILE/.bridgepilot-source`；编译和包进入其 `.build/bridgepilot/`，收据进入 `.runtime/bridgepilot-source-update/`。源码必须为官方 origin、干净的 main；只允许快进，不 reset、stash 或合并本地开发。已有 home 和 CODEX skills 位置由 `bridgepilot-local.json` 复用；全新安装 home 默认为系统 Documents 下 `Codex/BridgePilot`，skills 默认为实际 CODEX_HOME 下 `skills`（未设则 `~/.codex/skills`）。源码、home 和 skills 必须分离；未知同名目录或用户修改均保留。

没有 Skill 的新机先由 Codex 核验官方仓库并 clone main 到上述独立源码目录，读取本文件，然后执行 `source/orchestrator/skills/bridgepilot/scripts/bridgepilot_update.ps1`。该脚本完成获取、构建与受管安装，不要求先有 BP 可执行程序。不要通过 Forge 共享分发安装此私有入口。

安装更新不创建／恢复私有任务档案，不创建或同步私人仓库，不授予控制权，也不恢复任务或 heartbeat。首次后续运行 `$bridgepilot init` 配置档案；已有工作区由安装器检查读兼容并保留原配置。`source_update_completed` 收据包含实际源码提交、程序版本、home 及程序／Skill 的真实回执；安装或 Skill 事务中断先使用对应 recover，不删除 pending。程序安装和 Skill 投影为两个事务，后者失败时须如实说明程序可能已更新，核对后仅继续未完成阶段。

### 并行清理下游入口

BP 0.12.0 起，便携包新增 `payload/bridgepilot-cleanup-skill.md`，固定资产共十三项。程序和 `skill-install` 同时管理 `bridgepilot`、`bridgeforge-update`、`bridgepilot-cleanup` 三个私有入口；已有安装原位补齐，回退程序必须支持已安装的清理入口。新入口的安装、恢复、来源摘要及用户编辑保护与已有入口使用同一事务，不进入 Forge 共享分发。

`$bridgepilot-cleanup` 核对登记下游的清理入口，可追加项目名称或 ID；`preview` 全程只读。项目枚举使用 `bp project-list --workspace <实际目录>`，非 preview 指定子集可用 `--input <JSON文件>` 传 `{"projects":["<名称或ID>"]}`，preview 在内存中匹配项目名单。该查询只读 SQLite 与已有机器身份，缺档案或身份直接报错。旧配置化 cleanup 已退役，缺少可靠原生成品保护时报告 blocked，不执行删除；原生流程的范围、控制权及保护规则见 [bridgepilot-cleanup](source/orchestrator/skills/bridgepilot-cleanup/SKILL.md)。安装入口不执行删除。

### 从 GitHub 在新电脑安装并恢复

在新电脑的 Codex 对话中输入这一句即可启动引导；不要求预先安装 BP Skill、复制旧电脑便携包或编译 Rust：

> 请按 https://github.com/freakybridge/BridgePilot 最新正式 Release 的安装说明安装 BP，并从我的私有仓库 freakybridge/BridgePilotData 恢复档案；只恢复和核对，暂不接管或执行项目任务。

下列流程由 Codex 执行。GitHub 登录、平台要求的权限确认仍由本人完成；账号登录不能从旧电脑复制。程序从公开产品仓库取得，个人档案只从用户明确指定的私有库读取。

1. **核对本机和入口。** 当前包面向 Windows x64，Codex 已安装。检测 Git、GitHub CLI（`gh`），但缺失时不以它们作为下载 BP 的前提：可用系统 HTTPS 客户端（例如 PowerShell 的 Invoke-RestMethod／Invoke-WebRequest）读取 GitHub.com 的公开 API 并下载已核验的正式包；这一段仅用于安装引导，不替代 BP 的 Rust 执行内核。先安装 BP 程序，再由固定 home 的 environment-install 准备缺少的 Git／gh。鉴权由本人完成，不在聊天中粘贴令牌。`GH_HOST` 指向其他服务时停止核对，不把其他服务器当 GitHub.com。首次引导不要求已有 BP Skill。
2. **选择目录并保护已有数据。** 默认 home 为本机 `Documents/Codex/BridgePilot`，私有工作区为同级 `BridgePilotWorkspace`，均使用实际绝对路径。存在安装或档案时先读取已有配置、安装状态与同步记录；不重置、不换空目录掩盖冲突，不覆盖用户修改。不使用或复制旧机机器身份。下载暂存固定放在工作区 `.runtime/download/bridgepilot/<release-id>/`；首次安装的 home 保持为空，不能先把下载文件堆到 home 导致归属检查失败。
3. **固定发布身份并下载。** 使用 `gh api --hostname github.com repos/freakybridge/BridgePilot/releases/latest`，读取同一非 draft、非 prerelease 的正式 Release，保存真实 id、tag、资产 id／name／size／digest。正式 tag 须可解析至该发布的固定源码提交。找不到完整 BP 包时停止，不能改用 main 源码编译、旧机器文件或未经验证的第三方二进制。通过 `gh release download <实际 tag> --repo freakybridge/BridgePilot --pattern <精确资产名> --dir <下载目录>` 下载，禁止通配或覆盖冲突文件；中断后先核对已有文件再复用。
4. **执行前校验并组装包。** Release 必须包含 `package.json`、`bp-core.exe`、`bp-launcher.exe`、`controller-rules.md`、`readme.md`、`install.md`、`bridgepilot-skill.md`；core 0.6.0起另含 `bridgepilot-init.md`、`bridgepilot-projects.md`、`bridgepilot-handoff.md`、`bridgepilot-execution.md` 四个模式引用；0.8.0起另含 `bridgeforge-update-skill.md`，0.11.0起另含 `bridgepilot-update.ps1`，0.12.0起另含 `bridgepilot-cleanup-skill.md`。先按 GitHub 实际资产 digest（`sha256:`）校验 manifest，再核对 `product=BridgePilot` 和包格式：schema 2 必须为 BP 1.0.0 或更高、十三项固定资产、精确 components（orchestrator=embedded，forge=source-reference）及 launcher protocol 2，并与实际 core 自检核对；只有旧 schema 1 的 0.x 包按下列历史资产登记读取：0.12.0起十三项，0.11.x十二项，0.8.x至0.10.x十一项，0.6.x至0.7.x十项，旧0.5.x为六项。旧包仍可走旧安装流程，但不具备新版能力。源路径必须为对应 `payload/<上述文件名>`，拒绝额外入口、绝对路径、`..`、链接与未知目标；目标须匹配 BP 固定资产表（模式引用进入 home 的 `source/orchestrator/skills/bridgepilot/references/`，源码更新脚本进入该 Skill 的 `scripts/`，独立入口分别进入 `source/orchestrator/skills/bridgeforge-update/SKILL.md` 与 `source/orchestrator/skills/bridgepilot-cleanup/SKILL.md`）。它由用户级 BP 入口通过实际 home 加载，不另装 Forge 共享 Skill。逐文件校验大小、GitHub digest 和 manifest SHA256；缺失、漂移或身份不明时停止，不执行候选程序。组装到同一下载任务的 `bundle/`。校验后再执行 core `self-test` 与 launcher `--launcher-self-test`，核对 core 版本、所需能力与 manifest、launcher protocol=1 (legacy schema 1) / 2 (schema 2)；摘要和回执保存在本次下载目录。
5. **安装、准备依赖并恢复。** 对新工作区调用 core `init`，先 `install-preview --bundle <已核验包> --home <home> --workspace <工作区>`，通过后执行 `install`；这两步无需 Git／gh，也不接管其他机器。用稳定 launcher `environment-check --context <home>` 检查；需要时经明确授权执行 `environment-install --home <home> --scope user --source <来源>`，安装日志和 Unknown 意图固定在 home，不允许换目录重发。只安装 Git.Git／GitHub.cli，不脚本化登录或提权。用户级不受支持时报告，只有用户明确授权才可 machine scope；成功但 PATH未刷新时先核实再检查，不重复安装。winget缺失时报告系统依赖，不猜第三方下载入口。首次配置只确认一次本人私有库，验证后 `sync-configure`；已有库用 `sync-download`，必须读回私有属性、完整历史与准确远端提交。全新空库按已确认初始化授权 enrol／upload，已有控制权不覆盖。已有工作区沿原配置，pending／分叉／活跃／Unknown先核销。最后核对 install-status、self-test和status；中断用 install-recover，不手改记录。
6. **安装用户级入口并报告。** 按实际 CODEX_HOME（未设则本人 `~/.codex`）解析 skills 目录，使用 `skill-install-preview --home <home> --skills-root <目录>`，再 `skill-install`，用 `skill-status` 逐项验证。支持 `bp-parallel-cleanup-skill-v1` 的 core 同一事务安装 `bridgepilot`、`bridgeforge-update`、`bridgepilot-cleanup` 三个入口；已有安装可原位补齐。可直接选择 `/bridgeforge-update` 或输入 `$bridgeforge-update <项目与范围>`。其来源仍在 BP home，每个用户级入口只有受管投影和本机 home 绑定；不进入 Forge 共享分发，未知或用户修改的同名 Skill 拒绝覆盖。中断用 `skill-recover`；已安装独立升级入口时，回退 core 也必须支持该能力。返回 home、core版本、档案id／revision、项目、owner与本机身份、Skill安装结果和缺口。文件安装与客户端发现分别核验；未出现时刷新客户端或重启 Codex 后核对。只恢复档案的请求不接管、不上传、不恢复任务或heartbeat、不创建聊天；首次新库初始化另按用户确认的授权。显式 `$bridgepilot handoff restore` 才包含核验后的接管，成功后任务仍暂停。项目代码和业务环境不在私有档案中，恢复和执行分别按实际授权进行。

公开 Release 的安装引导从本节生成，包资产从同一已验证便携包取得。公开产品资产不能包含个人档案、配置、凭据、日志或原生会话／Memory。恢复成功不等于已经完成物理换机接续；缺少旧机交接时，保留可查看档案的状态并报告。

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

产品或安装目录的 `.runtime/config/bridgepilot.json` 仅记录本机工作区位置；档案工作区的 `.runtime/config/bridgepilot-machine.json` 记录本机身份。任务、稳定序号、执行事件、修订历史及本机传输待办保存在 `<工作区>/doc/5_project_knowledgebase/bridgepilot/bridgepilot.sqlite3`。SQLite 是唯一在线任务状态库，内部 JSON 参数和证据载荷不构成第二套任务档案。不得手工编辑，也不能只备份 `.runtime/`。个人档案不进入产品仓库、分发清单或原生 Memory。

用户查询只需 `$bridgepilot status`，Skill 根据本机绑定调用 CLI，原样展示其生成的“序号／项目／任务／阶段／进展／阻点”。任务由执行入口登记，序号不随排序变化；查询本身不补登记、不刷新聊天、不创建库或迁移。

已绑定的既有聊天由执行入口观察。独立 App Server 无法读取桌面当前回合时，CLI 明示“观察受限”，只展示可核实的原生 Goal 和末次历史；不把旧回合结束误报为当前任务完成。来源和观察时间可在表外核对。

旧 JSON 档案必须显式转换：先准备具备 `bp-sqlite-status-v1` 的新程序，用 `storage-preview --workspace <工作区>` 只读核对原完整历史、revision、控制权与在途状态；无活跃／Unknown 和未核销传输时，用 `storage-migrate --workspace <工作区> --expected <实际revision> --source <迁移授权来源>`。程序保留原始字节备份，核验所有历史逻辑摘要、实体和稳定号后激活 SQLite，旧 HEAD 替换为退役哨兵以阻止旧写入器复活。中断用 `storage-recover --workspace <工作区> --source <来源>`，不能手工删除候选、备份或哨兵来绕过保护。转换不授予新机控制权，不启动任务或 heartbeat。

### 私有档案同步

1. 每机准备 Git、GitHub CLI（`gh auth status`）和 Codex 登录；不复制凭据。sync-configure 的输入为 `{"repository":"<已授权 owner/repository>","branch":"main","source":"<真实用户授权来源>"}`，配置只在本机 `.runtime/config/bridgepilot-sync.json`。用户本次选择的独立私有库是 `freakybridge/BridgePilotData`；示例不是其他用户的默认值。
2. 当前控制机首次用 status 的 revision 调用 `sync-enrol --expected <revision> --source <来源>`，加入可验证的控制权。旧程序不支持 custody，因此启用前确保新程序已就绪；此操作不自动启动任务或恢复心跳。
3. 每个空闲批次边界 `sync-upload --workspace <工作区>`，只上传经过完整性校验的单一 SQLite 一致快照，包含共享任务、事件与完整逻辑历史；快照显式排除 `local_records` 本机传输待办、配置、日志、源码、原生会话和 Memory。首次转换远端可验证旧 JSON 祖先后切换为 SQLite；新客户端不从旧远端自动迁移。每次抓取／发布都读回准确 GitHub 私有属性；归档图和 Git 图分别校验，不自动合并分叉。活跃／Unknown Run 先处理，不持有数据库写事务等待网络。未知 push 的候选快照及 outbox 位于在线库的本机表，清理运行缓存后仍可核销。
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

`status` 以只读连接读取一个 SQLite 事务快照，不创建锁文件或重建运行目录。缺库、未迁移旧库、摘要冲突或历史断裂时保留现场并报告，不能初始化空库掩盖问题。迁移后的档案拒绝回退到旧 JSON reader；程序安装和档案迁移仍是独立操作。当前入口的字段与能力以 `bp help`、`bp self-test` 为准。

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

开发与发布沿项目原生构建、版本、发布入口及本任务授权核对。BP 的旧账本 verify-development／verify-publication 自动认证已退役，调用时明确拒绝并保持任务状态，不恢复旧配置续用。模型 completed 和普通同步均不能自动标为 Released；原生归类仍须读取 chat-group 的实际项目／成员，摘要空值或 move 确认不能单独代替证据。

## 维护者协议

本页说明安装、迁移与阻断行为；内部合同见：

- [Codex 项目同步设计](doc/0_architecture/design/codex-project-sync.md)：通用模板、来源证明、规划、精确退役、事务和回滚合同。
- [设计依据](doc/0_architecture/design/design-rationale.md)：资产归属与安全取舍。
- [原生指令架构](doc/0_architecture/design/codex-native-instruction-architecture.md)：AGENTS、Hook、Skill 与文档的职责边界。

## 已退役能力

bridgeforge-codex 不再安装或维护 `CLAUDE.md`、`.claude/`、Claude 入口、host switch、
project finalizer、setup junction 或 harness parity。项目中发现 Claude 遗留时只报告存在，
不读取、不迁移、不删除。

Markdown `paths:` 不是 Codex 的指令加载机制，安装过程不会建立自研 Markdown Rule 加载器。
