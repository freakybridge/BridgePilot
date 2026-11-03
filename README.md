# BridgePilot

> 通过 Codex 管理个人多个项目，持续推进已确认的目标；Forge 子系统提供统一工程约束。

BridgePilot（BP）接收项目目标，保存确认、任务与执行历史，协调不同项目的开发和交付。
日常直接在 Codex 的 BP 对话中指定项目和目标；复杂任务先 confirm，重要方向再由你判断。
`source/forge/` 提供工程骨架与受管工具，`source/orchestrator/` 提供总管能力。
开发仓库、独立源码更新目录、安装 home 与私人档案工作区的区分见 [安装与源码维护说明](INSTALL.md#开发仓库安装目录与档案的边界)。
单项目或多项目骨架升级可直接选择 `/bridgeforge-update`（或输入 `$bridgeforge-update`），由 BP 私有 [bridgeforge-update](source/orchestrator/skills/bridgeforge-update/SKILL.md) 编排，覆盖 dry-run、问题处理、升级及本轮已有授权的发布。该独立入口仍由 BP 安装与维护，不进入 Forge 共享分发。
Forge 维护入口仍为 `$bridgeforge-codex`，现有下游入口的迁移另行处理。

Forge 工程子系统负责：

- 为项目安装或更新 `AGENTS.md`、`.codex/`、文档结构和提交前检查等协作设施。
- 为用户安装 `confirm`、`develop`、`summary`、`git-sync` 等通用 Skills。
- 每次以最新正式版为完整基线；项目自有资产按明确归属保留或确认处理，无法安全判断时停止，验证失败则不写入或回滚并报告未恢复项。

BridgePilot 当前支持 Windows 和 Codex；源码开发需要 Rust/Cargo 1.88+。Forge 的安装、同步、校验、Git、
Native Memory 与受管项目 Hook 全部由 Rust 承载，不要求 Python 或项目 `.venv`。

## 快速开始

### 首次使用

先安装并登录 Codex，再新开对话，复制以下提示词；不需要预先安装 BP Skill 或 Rust：

```text
请读取 https://github.com/freakybridge/BridgePilot 最新正式 Release 的安装说明，下载、校验并安装 BP，再检查并从官方来源安装缺少的 Git、GitHub CLI；没有这些依赖时，先沿说明中的系统 HTTPS 引导安装 BP 程序。
我是首次使用，请为我初始化独立的个人工作区。首次配置时请向我确认一次私有存档库：可推荐我当前 GitHub 账号下的 BridgePilotData，也允许我指定已有库；确认后保存绑定，以后直接复用。已有资料先核对，不读取或恢复其他人的档案。必要的账号登录由我完成。
完成后请报告实际安装和初始化结果，并引导我登记第一个需要管理的项目；暂不启动项目开发。
```

这是首次初始化入口；已有 BP 档案的用户按 [安装与恢复说明](INSTALL.md#从-github-在新电脑安装并恢复)恢复自己的档案。完整流程统一维护在 INSTALL.md。

### 已安装后的统一入口

以下入口需要 BP core 0.6.0及以上，以及已核验的用户级 Skill；正式可下载版本以 Release 为准。

| 用法 | 用途 |
|---|---|
| `$bridgepilot init` | 首次确认并保存本人存档库，已有配置复用 |
| `$bridgepilot project add <项目目录>` | 核对并登记项目，不自动开发 |
| `$bridgepilot handoff prepare all` | 全部暂停，保存并上传已确认进度，完成旧机交接 |
| `$bridgepilot handoff restore` | 新机恢复并核验接管，任务与定时唤醒仍暂停 |
| `$bridgepilot status` | CLI 从 SQLite 只读生成固定六列：序号／项目／任务／阶段／进展／阻点；Agent 原样展示，流程见 [BP Skill](source/orchestrator/skills/bridgepilot/SKILL.md#status当前在做什么) |

### 从源码维护 Forge

```powershell
git clone https://github.com/freakybridge/BridgePilot.git "$env:USERPROFILE\tools\BridgePilot"
Set-Location "$env:USERPROFILE\tools\BridgePilot"
cargo build --locked --release --manifest-path .\Cargo.toml --target-dir .build/bridgeforge/cargo/local --bin bridgeforge
& powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install-shared-skills.ps1
```

然后新开 Codex 会话，在需要安装或更新骨架的项目根目录运行：

```text
$bridgeforge-codex
```

首次安装、旧版迁移和异常诊断分别见[安装与迁移说明](INSTALL.md)。

## 文档入口

BP 主产品、BF 与 orchestrator 分别管理版本。BP 以根 `VERSION` 为准，首版为 `1.0.0`；BF 与 orchestrator 分别以各自 CLI Cargo manifest 为准，延续已有版本线。`bp versions` 展示 BP 与组件版本，BF 的 `source-reference` 仅表示源码组合，不代表本机已安装该版本。安装与旧版本迁移从 [INSTALL.md](INSTALL.md) 进入。

普通 `$git-sync` 只提交和同步，不主动升版。当前本地候选采用 Template-only：下游 `.codex` 递归 JSON 仅保留 `hooks.json`；项目分发旁文件、版本配置、开发账本及骨架版本戳退出，不换路径或格式续用。

通用 develop 继续组织调查、实现、必要验证和交付；Forge 配置化 `develop`、`git-sync --release/setup/migration` 及依赖旧 keep 配置的 cleanup 已明确退役。构建、版本与发布按项目原生入口处理，发布仍须明确授权。BP 旧账本自动交付认证同样退役，不将任务自动标为 Released。当前规则见[项目同步设计](doc/0_architecture/design/codex-project-sync.md)及[工厂原生开发交付](doc/0_architecture/design/development-completion.md)，本轮真实结果见[四项目退役记录](doc/1_delivery/template-retirement/requirements_2026-11-02_four-projects.md)。本地候选不代表已正式发布。

单项目长任务或多个已讨论任务需要托管时，在该项目的新对话中输入：

```text
$autopilot
对话链接1
对话链接2
```

也可在 `autopilot` 后直接描述需求。Agent 汇总已有决定、预检权限与环境，与你一次确认任务包和
整体 token 预算后，在新对话统一实施和验收。时间只作估算；原生预算达到阈值后停止，在途调用
可能超额。默认交付本地未提交改动。需要当前客户端的对话读取与 Goal 能力；详细流程见
[autopilot Skill](source/forge/templates/skills/autopilot/SKILL.md)。

重构已有项目时，使用 [refactor-scan Skill](source/forge/templates/skills/refactor-scan/SKILL.md)：

```text
$refactor-scan 扫描当前项目，允许现有定向测试和离线性能基准，先给重构清单。
```

清单按稳定编号给出职责调整、业务合并、代码质量和性能候选；你选择并授权条目后，
由 confirm 记录范围与验收，再交给 develop 实施。扫描覆盖范围与未验证项会单独说明。

需要了解当前架构、当前交付、开放 Bug 或历史资料时，从唯一完整导航
[文档索引](doc/README.md)开始；根 README 不复制这些动态状态。

## 它会修改什么

用户级位置：

- `~/.bridgeforge-codex`：完整、可信的产品仓库。
- `~/.codex/skills/bridgeforge-codex`：Codex 用来发现产品的薄入口。
- `~/.codex/skills/`：由产品维护的通用 Skills。
- `~/.codex/AGENTS.md`：仓库统一用户指令；相同不写，不同先备份再覆盖，详见 [安装说明](INSTALL.md)。
- `~/.codex/memories/`：Codex 原生 Memory；用户授权后可作为不透明目录跨电脑同步，
  同步器不解析正文语义；summary 可只读检索、阅读并提出建议，不改写正文。

目标项目：

- 根 `AGENTS.md`：公共区由产品更新；项目级专区逐字保留。
- `.codex/hooks/`、`.build/bridgeforge/bin/bridgeforge-hook.exe` 与 `.build/bridgeforge/bin/bridgeforge.exe`：受管 Rust
  源码与编译产物；`init`、`adopt`、`update` 按需构建，工厂先按 [运行时准备](INSTALL.md#工厂本地运行时准备)维护当前产物，再检查能力或执行获授权的 `$git-sync`。日常入口不启动脚本解释器。项目自有 Hook 继续使用
  `.codex/hooks/project_*/` 扩展目录，不与受管 Rust 源码混写。
- `.githooks/pre-commit`：执行提交前机器检查。
- `doc/`：以 `doc/README.md` 为索引的六层文档结构，包含项目自有知识库。
- 项目自己的嵌套 `AGENTS.md`、项目规则和明确保留的定制资产仍归项目所有。
- 旧项目的 `.codex/rules/*.md` 与 `.codex/memory/`：不再运行或语义迁移；兼容更新保留，重建时按普通项目自有资产确认保留或删除，不影响原生 Memory。

## 更新时怎样保护项目

- `init/adopt/update/auto` 使用同一通用模板规则，不读取旧版本戳或项目合同分流。
- Plan 零写，未知额外 JSON、链接、未结事务或工具归属不明时阻断；已知退役项和已有内容替换列出精确风险。
- Apply 复用已确认授权，锁内重算输入指纹，先校验恢复副本，再在同一事务安装和清理；删除后验证递归 JSON 唯一名单和工具来源。
- 保留项目私有区、原生 handler、Skill、业务资料和 Git 状态；回滚不覆盖并发外部变化。工厂自身文档索引和历史资料由工厂维护，不适用下游接管。

安装与恢复从[INSTALL](INSTALL.md)进入；内部合同见[项目同步设计](doc/0_architecture/design/codex-project-sync.md)。

## 仓库结构

```text
BridgePilot/
├── Cargo.toml / Cargo.lock  # Forge 九包与 BP 六包的受管 Rust workspace
├── source/
│   ├── forge/               # 九个工程包的唯一维护源
│   │   ├── bridgeforge-manifest.json # Forge 用户指令、共享 Skills 与项目骨架的唯一源清单
│   │   ├── core/skeleton/lifecycle/workflow/memory/checks/factory/cli/hook
│   │   └── templates/       # 分发资产与通用 Skills
│   └── orchestrator/        # 总管，与 forge 平行，无 src 中间层
│       ├── tasks/ storage/ execution/
│       ├── transfer/ install/ cli/
│       └── skills/bridgepilot/SKILL.md  # 本项目入口，不下发
├── .codex/                  # 本仓库 dogfood 配置、公共源码投影与生成资产
├── scripts/                 # PowerShell 用户级安装入口
├── doc/                     # 架构、交付、Bug、参考与归档
├── VERSION
└── CHANGELOG.md
```

`source/`、`source/forge/templates/` 属于产品层；`.codex/` 是本仓库的自用镜像；`doc/` 和 README
属于元文档。产品层变化需要同步版本、CHANGELOG、分发清单与自用镜像。

`source/forge/` 承载工程基础，`source/orchestrator/`
承载跨项目总管，二者平行。当前实现项目登记、任务确认、持久档案、客户端动作、有限时长调度、受控 stdio 执行、原生心跳、开发发布核验、私有档案同步、明确换机交接和独立安装升级。安装与恢复从 [INSTALL.md](INSTALL.md#bridgepilot-本地试用与档案恢复) 进入；真实物理双机及业务发布的验收范围以开发计划为准。
公共安装仍使用原来的 `.codex/hooks/source/<包>/` 源码目标及工具入口。
范围与进展见[BridgePilot 开发计划](doc/1_delivery/bridgepilot-evolution/plan_2026-10-30_bridgepilot-evolution.md)。

## 通过 Codex 使用 BP

在本项目对话中直接给出目标，例如：“让 StratusAgent 校对 BlackSwan 交易信号，先调查信号产生、落库和消费是否一致，发现需要改变交易语义时找我确认。”这是使用示例，不代表已启动该任务。BP 读取[本地入口](source/orchestrator/skills/bridgepilot/SKILL.md)，将需求登记为任务；简单明确任务直接推进，复杂任务先 confirm。你不需要自己操作 JSON 或 CLI。

独立执行对话由原生客户端在对应下游项目创建。桌面空闲仍可能持有写锁，BP 通过有记录的原生归档和仅恢复持久记录完成交接，再由自管 Codex stdio 进程执行。任务结束且执行进程退出后，再完成客户端显示交接。StratusAgent、HoldemTrainer 各一项真实只读任务已执行，两项聊天的原生成员、客户端列表与各自项目侧栏显示均已核验。Stratus 报告已交付；Holdem 的原始文件证据随后发生变化，保留核对状态。

持续接续使用本 BP 对话的原生 heartbeat，每次触发核对实际配置和当前任务；开发与发布核验消费真实受管证据，发布还核对固定提交及实际推送端点。不会把模型 completed 当作 Released。心跳激活／首个触发、真实业务发布及双机交接的验证状态见开发计划；不保证应用关闭后执行。

试用准备与档案恢复只在 [INSTALL.md](INSTALL.md#bridgepilot-本地试用与档案恢复) 维护；架构、状态与失败语义见[运行协议](doc/0_architecture/design/bridgepilot-runtime-protocol.md)。

## 兼容边界

旧 `$bridgeforge`、旧用户级受管记录、旧 `.bridgeforge` 产品目录和 Claude 用户级资产
不再受支持，也不会由当前产品读取、接管或删除。Claude 骨架、host switch 和双骨架
一致性机制已经退役；项目中发现 Claude 遗留文件时只报告存在，不读取或修改。

## License

MIT
