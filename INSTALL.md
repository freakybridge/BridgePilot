# BridgePilot 安装与迁移

当前本地候选采用 Template-only，同步器不再解释旧项目合同、版本配置、账本或版本戳。下游 `.codex` 递归只允许 hooks.json；未知额外 JSON 阻断，已知退役项经精确计划、备份与事务清理。业务原生版本和数据不由骨架旁文件接管。

项目安装／升级同时交付可用的原生发布入口。首次缺入口时，主对话依据项目 INSTALL、原生版本文件、已有构建命令、实际推送目标及明确产物范围整理安装规则；同一 `project-sync` 事务登记到根 Cargo metadata，并把复用公共 CLI 的 Rust 驱动部署到 `.build/project-release/bin/`。原生业务版本仍只在原文件维护。规则、程序、来源或目标不完整时，安装明确报告未就绪；不等到 release 才补安装，也不覆盖已有自定义入口。

发布仍只使用 `$git-sync release`。GitHub 和明确声明实例的 GitLab 共用现有协调流程；GitLab 使用项目 Generic Package 与 Release 资产链接，并核对固定提交、文件大小和摘要。API 认证采用作用域匹配的 `GITLAB_ORIGIN` / `GITLAB_TOKEN`，或项目被忽略的 `.runtime/config/bridgeforge-gitlab-auth.json`（仅 `origin`、`token` 两字段）。令牌不能进入 Git、命令参数或发布收据。安装不创建令牌、启动业务或执行真实发布来探测权限；API 不可用时保留现场并报告。

批次只准备一次公共产物，各项目复用；无变化不编译、不重复改写。发布阶段复用仍匹配的原生产物和固定提交，未知上传先核销再接续，不盲目重复写入。源码归档按项目显式规则选择，保留现有目录和启动方式，不表示提供独立安装包或已验证新机运行。

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

## Forge 项目安装和升级

1. 在目标项目确认 Git 根、原生版本和发布入口。通过 `$bridgepilot-update` 固定本批官方源码；独立引导从 `https://github.com/freakybridge/BridgePilot.git` 获取干净 `main`，以已获取的 `origin/main` 提交为准。
2. 从固定源码阅读 [Forge 项目维护手册](source/forge/templates/maintenance/bridgeforge-codex/MAINTENANCE.md)。按手册调用 `prepare-project.ps1` 准备一次公共 CLI 和 Hook，然后用该收据生成目标项目的零写入计划。
3. 按计划和指纹应用。项目 Skill 安装在 `.agents/skills/`，内部维护手册在 `.codex/maintenance/bridgeforge-codex/`，公共 Hook 注册在 `.codex/hooks.json`，公共工具及构建收据在 `.deps/bridgeforge/bin/`。应用保护项目自有 Skill、Hook、AGENTS 项目区和 pre-commit 私有扩展。用新 Codex 会话核对实际 Skill 路径。

项目同步采用统一的 Template-only 规则。旧项目合同、版本配置、账本、目录覆盖及 Hook 旁文件进入精确退役；未知额外 JSON 保留并阻断。Apply 核对指纹、公共工具编译来源和 `.codex` 递归仅 hooks.json，失败恢复自身改动并保留外部变化。详见 [项目同步设计](doc/0_architecture/design/codex-project-sync.md)。

## Forge 用户级迁移

完成各项目骨架迁移和实际 Skill 发现核对后，按单独授权从同一官方来源执行 [用户级迁移手册](source/forge/templates/maintenance/bridgeforge-codex/references/user-skill-maintenance.md)。用户级保留统一 `~/.codex/AGENTS.md` 和原生 Memory Hook 所需的专用执行器。用户指令有不同内容时备份原始字节。项目维护与用户级迁移分别核验。

存在依赖项目时，传入完整的已迁移项目根列表。确认没有依赖项目的机器可显式使用 `-NoDependentProjects`。旧产品检出和通用 CLI 的恢复副本由迁移收据列出。

## BridgePilot 本地试用与档案恢复

### 产品版本与安装格式

BP 主产品从 `1.0.0` 起，以根 VERSION 为准。BF 以 `source/forge/cli/Cargo.toml`、orchestrator 以 `source/orchestrator/cli/Cargo.toml` 为版本事实源，内部包版本仅作构建身份。`bp versions`／`self-test` 输出 BP 版本和组件版本；Forge 的来源引用不是实际安装回执，查询外部 BF 必须读取实际工具。

BP 1.0.0 使用 package／installation schema 2，历史包为十三项资产；具备 `bp-git-sync-skill-v1` 的新包为十五项，必须包含 `bridgepilot-git-sync-skill.md` 和 `bridgepilot-git-sync-list-projects.py`。新旧精确资产表由实际 core 能力核验，不能删除新包登记来伪装旧包。launcher protocol 2 支持读取 installation schema 1 与 2；包格式和 launcher 协议分别校验。旧 schema 1 仍按原 0.x 格式读取，其 version 表示 orchestrator。迁移先核对新 orchestrator 不低于旧版本，再检查实际档案读兼容、文件所有权及组件／二进制身份；未知额外字段和漂移保持阻断。

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

无参数 `$bridgepilot` 仅更新 BP 自身；统一升级本机 BP 和下游使用 `$bridgepilot-update`。

### 统一升级入口与旧名称迁移

`$bridgepilot-update` 由 BP 独立管理，先复用下述 BP 源码安装入口，取得实际程序安装与专用 Skill 成功收据后，再用新版 BP 的 `project-list` 读取全部登记下游，逐项目检查暂停、控制权和实际绑定，执行 Forge 骨架计划与升级。默认不提交、推送或 release；只在当前批次有明确且匹配的发布授权时接续。显式只读／dry-run 不运行安装脚本、不构建或写入，缺工具则报告预览缺口。

BP 2.0.0 将 `bridgeforge-update` 改为 `bridgepilot-update`，安装包对应资产使用 `payload/bridgepilot-update-skill.md`，home 来源为 `source/orchestrator/skills/bridgepilot-update/SKILL.md`，稳定资产 ID 保留。后文历史包的旧文件名仅用于识别和恢复，不是新包写入目标。旧本机绑定和配置原位复用；新入口验证成功后才按精确归属退役旧入口，未知或用户编辑的同名文件保持并阻断。新安装不会改 Forge 公共模板或共享 Skill 登记。

程序和 Skill 是两个受管事务，只有两阶段均核验成功才进入下游写入。已有同来源收据且当前程序、全部受管文件与 Skill 均一致时，返回 `mode=reused`、`build_invocations=0`，不重建或改写成功收据。来源变化或文件失配时重新准备，不能仅按版本号宣称已安装最新。

缺少或过期的源码更新收据会先触发构建补验。同版本、组件和全部配套资产一致时，安装器允许 core／launcher 仅有合法 PE 时间戳和 CodeView PDB GUID 差异，保留原程序、完整 SHA256、旧包和回退记录，不用新构建覆盖。该次源码更新返回 `mode=reused_after_build`、`build_invocations=1` 并保存本次成功收据；后续同提交调用可零构建复用。非元数据字节、组件或包资产不同，受管文件被修改、结构无法安全解析或存在 pending 时仍阻断。

迁移使用程序安装 journal schema 2 和 Skill journal schema 2，新程序仍能恢复旧 schema 1 的原文件布局。稳定 launcher 在内容变化时随安装事务刷新；若迁移日志写入后、启动器尚未切换就中断，旧启动器可能无法读取新日志，须使用失败收据 `recovery_core` 指向的本次固定候选程序执行 `install-recover` 或 `skill-recover`，不能删除日志或另下一个版本覆盖。已安装新入口后，不兼容的旧程序回退会被拒绝。

首次从 0.10.0 等没有源码更新脚本的安装升级时，沿官方源码引导调用下述脚本，并显式复用原 home 和 Skills 绑定；不手工复制 Skill，不重新初始化私有档案。无参数 `$bridgepilot` 保留只更新 BP 自身的行为，不递归调用完整下游升级。

### BP 自身的源码安装与更新

已安装用户级入口后，在 Codex 调用 **`$bridgepilot`**，无需参数。入口从 `https://github.com/freakybridge/BridgePilot.git` 的 `main` 获取源码，固定本次提交，使用锁定的 `Cargo.lock` 构建 `bp` 和 `bp-launcher`，再由 Rust 安装器预览、安装并更新全部受管用户级 Skill。重复调用会更新同一安装。`init`、`status`、项目登记和交接模式保留原行为，不隐式更新程序。

Windows 源码构建需要 Git、Rust 1.88 或更高版本及其 Windows C++ 构建工具；可复用本机已有环境。缺依赖时先报告具体缺口，按用户允许的安装范围准备，不自动提权或脚本化登录。构建可按 Cargo.lock 下载依赖，构建失败不切换当前安装。

默认独立源码目录为 `$env:USERPROFILE/.bridgepilot-source`；编译和包进入其 `.build/bridgepilot/`，收据进入 `.runtime/bridgepilot-source-update/`。源码必须为官方 origin、干净的 main；只允许快进，不 reset、stash 或合并本地开发。已有 home 和 CODEX skills 位置由 `bridgepilot-local.json` 复用；全新安装 home 默认为系统 Documents 下 `Codex/BridgePilot`，skills 默认为实际 CODEX_HOME 下 `skills`（未设则 `~/.codex/skills`）。源码、home 和 skills 必须分离；未知同名目录或用户修改均保留。

没有 Skill 的新机先由 Codex 核验官方仓库并 clone main 到上述独立源码目录，读取本文件，然后执行 `source/orchestrator/skills/bridgepilot/scripts/bridgepilot_update.ps1`。该脚本完成获取、构建与受管安装，不要求先有 BP 可执行程序。不要通过 Forge 共享分发安装此私有入口。

安装更新不创建／恢复私有任务档案，不创建或同步私人仓库，不授予控制权，也不恢复任务或 heartbeat。首次后续运行 `$bridgepilot init` 配置档案；已有工作区由安装器检查读兼容并保留原配置。`source_update_completed` 收据包含实际源码提交、程序版本、home 及程序／Skill 的真实回执；安装或 Skill 事务中断先使用对应 recover，不删除 pending。程序安装和 Skill 投影为两个事务，后者失败时须如实说明程序可能已更新，核对后仅继续未完成阶段。

### 批量同步下游入口

`bridgepilot-git-sync` 同样由 BP 受管安装：包内正文和名单脚本进入 home 的 `source/orchestrator/skills/bridgepilot-git-sync/`，再通过 `skill-install-preview`／`skill-install` 投影到本机 Skills 目录。`skill-status` 验证正文、脚本和本机绑定；回退程序须具备 `bp-git-sync-skill-v1`。该安装不执行项目同步或发布，只有显式调用此入口才授权对应范围；菜单发现与文件安装分别核验。

### 批量清理下游入口

单项目 `$cleanup` 使用当前项目受管工具的 `explicit-cleanup-v1` 能力：固定保留配置、下载和依赖，依据当前原生构建、启动及发布文件核对成品，用重复 `--keep` 指定精确保留项、重复 `--evidence` 固定依据，再以 `--products-reviewed` 执行；`--dry-run` 零写入。运行必需但仍位于 `.runtime/` 的依赖同样显式保留。工厂自动保护自身三工具及收据，原生发布驱动和收据由其所属实现保护。旧项目旁文件不恢复；完整调用规则见项目 [cleanup Skill](.agents/skills/cleanup/SKILL.md)。

BP 0.12.0 起，便携包新增 `payload/bridgepilot-cleanup-skill.md`，固定资产共十三项。程序和 `skill-install` 同时管理 `bridgepilot`、`bridgeforge-update`、`bridgepilot-cleanup` 三个私有入口；已有安装原位补齐，回退程序必须支持已安装的清理入口。新入口的安装、恢复、来源摘要及用户编辑保护与已有入口使用同一事务，不进入 Forge 共享分发。

`$bridgepilot-cleanup` 默认遍历全部登记下游，逐项目执行各自实际安装的 cleanup；可追加精确项目名称或 ID，`preview` 将各项目调用限制为只读预览。具体清理规则、成品及依赖保留、Junction 和权限项处理均由项目 cleanup 负责，BP 只编排、继续其余独立项目并在结尾统一汇总。项目枚举使用 `bp project-list --workspace <实际目录>`；非 preview 指定子集可用 `--input <JSON文件>` 传 `{"projects":["<名称或ID>"]}`，preview 在内存中匹配，不写范围文件或记录。未知／歧义范围不回退为全部。缺工具或保护依据只阻断相应项目，不自行升级、恢复旧 JSON、改权限或改用 shell 删除。完整调用、授权、控制权及安全边界见 [bridgepilot-cleanup](source/orchestrator/skills/bridgepilot-cleanup/SKILL.md)；安装或修复入口本身不执行删除。

### 从 GitHub 在新电脑安装并恢复

在新电脑的 Codex 对话中输入这一句即可启动引导；不要求预先安装 BP Skill、复制旧电脑便携包或编译 Rust：

> 请按 https://github.com/freakybridge/BridgePilot 最新正式 Release 的安装说明安装 BP，并从我的私有仓库 freakybridge/BridgePilotData 恢复档案；只恢复和核对，暂不接管或执行项目任务。

下列流程由 Codex 执行。GitHub 登录、平台要求的权限确认仍由本人完成；账号登录不能从旧电脑复制。程序从公开产品仓库取得，个人档案只从用户明确指定的私有库读取。

1. **核对本机和入口。** 当前包面向 Windows x64，Codex 已安装。检测 Git、GitHub CLI（`gh`），但缺失时不以它们作为下载 BP 的前提：可用系统 HTTPS 客户端（例如 PowerShell 的 Invoke-RestMethod／Invoke-WebRequest）读取 GitHub.com 的公开 API 并下载已核验的正式包；这一段仅用于安装引导，不替代 BP 的 Rust 执行内核。先安装 BP 程序，再由固定 home 的 environment-install 准备缺少的 Git／gh。鉴权由本人完成，不在聊天中粘贴令牌。`GH_HOST` 指向其他服务时停止核对，不把其他服务器当 GitHub.com。首次引导不要求已有 BP Skill。
2. **选择目录并保护已有数据。** 默认 home 为本机 `Documents/Codex/BridgePilot`，私有工作区为同级 `BridgePilotWorkspace`，均使用实际绝对路径。存在安装或档案时先读取已有配置、安装状态与同步记录；不重置、不换空目录掩盖冲突，不覆盖用户修改。不使用或复制旧机机器身份。下载暂存固定放在工作区 `.runtime/download/bridgepilot/<release-id>/`；首次安装的 home 保持为空，不能先把下载文件堆到 home 导致归属检查失败。
3. **固定发布身份并下载。** 使用 `gh api --hostname github.com repos/freakybridge/BridgePilot/releases/latest`，读取同一非 draft、非 prerelease 的正式 Release，保存真实 id、tag、资产 id／name／size／digest。正式 tag 须可解析至该发布的固定源码提交。找不到完整 BP 包时停止，不能改用 main 源码编译、旧机器文件或未经验证的第三方二进制。通过 `gh release download <实际 tag> --repo freakybridge/BridgePilot --pattern <精确资产名> --dir <下载目录>` 下载，禁止通配或覆盖冲突文件；中断后先核对已有文件再复用。
4. **执行前校验并组装包。** Release 必须包含 `package.json` 及包格式要求的全部 payload。先按 GitHub 实际资产 digest（`sha256:`）校验 manifest，再核对 `product=BridgePilot`：schema 2 必须为 BP 1.0.0 或更高、精确 components（orchestrator=embedded，forge=source-reference）及 launcher protocol 2；历史包十三项，具备 `bp-git-sync-skill-v1` 的包十五项，须包含 `bridgepilot-git-sync-skill.md` 和 `bridgepilot-git-sync-list-projects.py`，按实际 core 自检核对。旧 schema 1 的 0.x 包按原资产表读取：0.12.0起十三项，0.11.x十二项，0.8.x至0.10.x十一项，0.6.x至0.7.x十项，旧0.5.x六项。旧包不具备新版能力。源路径、资产 ID、目标必须匹配 BP 原生固定资产表；拒绝额外入口、绝对路径、`..`、链接与未知目标。模式引用进入 home 的 `source/orchestrator/skills/bridgepilot/references/`，源码更新脚本进入该 Skill 的 `scripts/`；各私有入口进入同名 Skill 目录，git-sync 名单脚本进入其 `scripts/list_projects.py`。历史升级入口名称与新名称按本页迁移合同识别，不另装 Forge 共享 Skill。逐文件校验大小、GitHub digest 和 manifest SHA256；缺失、漂移或身份不明时停止，不执行候选程序。组装到同一下载任务的 `bundle/`，再执行 core `self-test` 与 launcher `--launcher-self-test`，核对实际版本、能力、组件和协议；摘要及回执保存在本次下载目录。
5. **安装、准备依赖并恢复。** 对新工作区调用 core `init`，先 `install-preview --bundle <已核验包> --home <home> --workspace <工作区>`，通过后执行 `install`；这两步无需 Git／gh，也不接管其他机器。用稳定 launcher `environment-check --context <home>` 检查；需要时经明确授权执行 `environment-install --home <home> --scope user --source <来源>`，安装日志和 Unknown 意图固定在 home，不允许换目录重发。只安装 Git.Git／GitHub.cli，不脚本化登录或提权。用户级不受支持时报告，只有用户明确授权才可 machine scope；成功但 PATH未刷新时先核实再检查，不重复安装。winget缺失时报告系统依赖，不猜第三方下载入口。首次配置只确认一次本人私有库，验证后 `sync-configure`；已有库用 `sync-download`，必须读回私有属性、完整历史与准确远端提交。全新空库按已确认初始化授权 enrol／upload，已有控制权不覆盖。已有工作区沿原配置，pending／分叉／活跃／Unknown先核销。最后核对 install-status、self-test和status；中断用 install-recover，不手改记录。
6. **安装用户级入口并报告。** 按实际 CODEX_HOME（未设则本人 `~/.codex`）解析 skills 目录，使用 `skill-install-preview --home <home> --skills-root <目录>`，再 `skill-install`，用 `skill-status` 逐项验证。支持 `bp-update-first-v1` 的 core 同一事务安装 `bridgepilot`、`bridgepilot-update`、`bridgepilot-cleanup` 三个入口；已有安装可原位补齐。可直接选择 `/bridgepilot-update` 或输入 `$bridgepilot-update <项目与范围>`。其来源仍在 BP home，每个用户级入口只有受管投影和本机 home 绑定；不进入 Forge 共享分发，未知或用户修改的同名 Skill 拒绝覆盖。中断用 `skill-recover`；已安装独立升级入口时，回退 core 也必须支持该能力。返回 home、core版本、档案id／revision、项目、owner与本机身份、Skill安装结果和缺口。文件安装与客户端发现分别核验；未出现时刷新客户端或重启 Codex 后核对。只恢复档案的请求不接管、不上传、不恢复任务或heartbeat、不创建聊天；首次新库初始化另按用户确认的授权。显式 `$bridgepilot handoff restore` 才包含核验后的接管，成功后任务仍暂停。项目代码和业务环境不在私有档案中，恢复和执行分别按实际授权进行。

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

### 完整发布

用户只调用 `git-sync release`。本仓库已有工具的实际单次命令如下，不需要 Agent 再手工运行普通同步、打包或 GitHub 发布：

```powershell
.build/bridgeforge/bin/bridgeforge-factory.exe git-sync release
```

CLI 读取根 Cargo.toml 的 `workspace.metadata.bridgeforge.native-release` 执行入口，接入远端后，由原生源码适配器比较最近正式 BP Release 与当前源码的实际代码净差异，再按相关代码提交和本次待提交代码消息判定版本。纯验收文档、TODO、索引及普通 Markdown 说明不触发升版；source／.codex 中运行 Skill、模板及其他行为输入不按文件扩展名排除。有代码变化时，适配器提出 VERSION／CHANGELOG 修改，协调器在可恢复事务中应用；已有合理新版本直接复用，无法分类的改动明确暂停。引导构建与版本确定后的最终构建分开，后者使用锁定 Cargo 构建 BP core／launcher，再调用内部 `release-prepare` 复用本节 bundle 逻辑。适配器声明完整 package.json 与十三项 payload；它不包含私人档案、用户配置、凭据或 Memory。

没有代码变化时保持原生版本：尚未发布的既定版本继续完成正式发布；已经发布的版本只同步当前文档等源码增量，再核验原 Release、原标签提交、发布标记和完整资产摘要，复用原发布，不重新打包或替换远端资产。此时收据的 commit 是原发布提交，synchronization_commit 是本次同步提交，publication_reused 为 true；只有源码同步与远端核验都完成才返回 released。缺少本机旧状态或本地产物不影响从原生资产清单和正式发布标记进行核验；远端缺失或不一致明确失败，不以普通 synced 代替发布成功。

CLI 在提交推送前核对目标版本占用，再受管提交推送固定源码，并发布 `bp-v<BP版本>` 标签的正式 GitHub Release。标签采用 BP 独立命名空间，既有 v13.x 等历史不改写。上传中的 draft 仅是内部事务状态，不增加人工审批；只有完整资产及 digest/size 核验通过才转正式并返回 released。安装消费者继续按上文读取正式 Release。

有效 Cargo 输出会复用。中断后的固定源码、产物和已完成阶段只记录在 `.runtime/bridgeforge-codex/native-release/`，不是版本登记或开发认证；再次调用同一入口仅补缺失步骤。远端结果不确定先查询，不覆盖已有冲突标签、资产或版本。源码变化与未完成发布冲突时保留现场并报告，不能删除接续记录绕过保护。真实用户环境安装和下游部署不自动随发布执行。

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

升级重复 preview/install，保留旧完整包及配置，最后原子切换 current。用户改过的受管文件拒绝覆盖；同版本仅构建元数据不同的复用规则见上文“BP 自身的源码安装与更新”，其他程序／组件／包资产差异仍拒绝。失败用包内 core 的 `install-recover --home <home>`；稳定 launcher 也会先恢复中断事务。回退用 `install-rollback --home <home>`，必须保留旧包、没有活跃执行，且旧 reader 能读取当前实际档案；否则拒绝。安装不升级／回滚档案，启用 custody 后不能回退到不支持它的 reader。真实双物理机、掉电和网络盘环境仍须区分于隔离工作区测试。

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
