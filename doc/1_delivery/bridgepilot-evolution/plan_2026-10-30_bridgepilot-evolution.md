---
lifecycle: active
validation_status: in_progress
---

# BridgePilot 产品演进与跨项目总管开发计划

关联原需求：**TODO-050**，见[项目待办总表](../../0_architecture/TODO-INDEX.md)。

日期：2026-10-30。规模：**L**，涉及架构扩展、源码与分发路径迁移、持久状态、执行控制及产品身份迁移。

本文件是本主题唯一的需求与开发计划，后续实施、验证、偏差和验收增量在此维护，不另建一份重复需求卡。

最新工程交付：[第九阶段：status 统一输出、执行事件与 SQLite 唯一状态库迁移](#bp-status-sqlite-plan)，最终结果见[15.18](#bp-status-sqlite-delivery)。用户后续明确授权 autopilot 实施及 B 范围，并将同一 Goal 上限调至10000000；本机 BP／Skill 已更新0.7.2，真实档案及私有 GitHub 已切为 SQLite，独立恢复核验通过。2026-11-01用户已验收本阶段并授权release，见[15.19](#bp-status-sqlite-acceptance)；发布实际结果另记，heartbeat保持暂停。下文早期“只计划／待执行”为各自时点历史，不是当前权限或状态。

## 1. 原始目标、来源与授权边界

用户希望通过一个个人 Agent 总管并行推进多个项目，减少项目切换、任务分派、追问进度、上下文传递和发布协调所消耗的精力。用户保留阶段目标、重要方向与审美判断。

历史讨论入口：[评估总管 Agent 项目管理模式](codex://threads/01a18cc3-ae03-7c80-9cb2-7e71b49d117b)。本轮确认来源为当前对话中的用户决定；历史对话中的 Agent 建议不自动成为授权。

用户明确提出并确认：

- 产品名称为 **BridgePilot（BP）**。
- 在现有 BridgeForgeCodex 仓库中扩展并演进产品，不另建独立 BP 产品。
- 用户给阶段目标，总管在确认范围内自主拆解、分派和持续推进，以正式发布作为开发任务的最终交付目标。
- 简单任务直接执行；复杂任务必须先经过 `confirm`；推进时一般采用项目内 `autopilot`。
- 重要方向或审美变化先提供方案／原型，用户确定后深入实现。
- 暂停必须明确全部或指定项目；范围不明时拒绝执行暂停指令，不根据当前话题猜测。
- 换机时允许未完成代码提交、推送至远端工作分支，并保存任务交接记录。
- BP 不负责预算分配，预算由用户关注。
- 只管理今后明确交给 BP 的任务；用户自行了结原有开发聊天。
- 首版登记五个项目，选择两个完成真实并行试运行，验证发布、指定范围暂停和换机续接。
- 原工程模块归入与 `orchestrator` 平行的 `forge`；`orchestrator` 不增加 `src/` 一层。

计划阶段的授权原文为“生成完整的开发计划吧，先不要实施”和“开发计划落盘至doc”，当时仅保存计划、索引及原需求 TODO。随后用户明确“可以开始了”，并以“以三小时为界，开始吧”授权首批实施；本批时间与执行边界见第 14 节。

首批实施覆盖能力探测、目录迁移及必要工程验证与受管子包收口。产品未来的自动发布职责不构成本批提交、推送、发布、真实下游修改、配置常驻自动化或用户级安装更新授权。隔离能力探测只使用本批自己的测试执行对象。既有决定直接继承，不重做访谈。

用户已明确首批三小时硬时间上限，未另给 token 数值额度，不启动原生 Goal 或自动续期；token 未实测。源码迁移按项目要求安排独立只读复核，主对话负责受管操作与整合。此实施约定与“BP 产品不负责预算分配”的职责边界分别判断，不擅自增加产品预算管理能力。

## 2. 范围与首版完成标准

| 事项 | 要求 |
|---|---|
| 使用者与入口 | 个人专用，只通过 Codex 与 BP 沟通，不新增独立管理界面 |
| 初始项目 | BP 自身（原 BridgeForgeCodex）、StratusAgent、CausisRiskSuite、HoldemTrainer、BridgeLibrary |
| 扩展性 | 通过项目登记接入更多项目，不把五个项目写死在调度代码中 |
| 阶段驱动 | 用户明确目标和范围；BP 自主组织执行与日常技术取舍 |
| 决策入口 | BP 在总管聊天统一进行必要的 `confirm`，下游复用已有决定 |
| 持续推进 | 运行条件允许时持续推进；关键阻塞、重要判断与成果集中汇报 |
| 工程规范 | 总管与工程模块共同遵守工程规范；下游保留自身规则 |
| 交付 | 实现、必要验证、受管收口、提交、推送、合并和发布；按项目实际交付模型执行 |
| 调查任务 | 可以完成于结论与证据交付，不为了发布而制造代码变更 |
| 暂停与恢复 | 显式范围、可核验停止、持久检查点、换机后核对再恢复 |
| 既有聊天 | 可作为历史资料，不自动恢复其中旧任务或扩大授权 |
| 首版验收 | 五项目登记，两个真实项目并行完成任务，验证一次重要判断、暂停及换机续接 |

首版不扩展为多用户平台、商业化产品、成本与预算管理系统，也不重写现有开发、版本及发布流程。具体试点任务尚未选择；BlackSwan 信号校对仅为交互示例，未因此成为已启动任务。

## 3. 现状依据与现场边界

计划基线的维护源码是九包 Rust workspace：`core / skeleton / checks / workflow / lifecycle / memory / factory / cli / hook`。公共八包与工厂专属包已有分发边界；2026-10-31 首批已将维护位置归组至 `source/forge/`，下表保留基线问题及现行文件链接。

| 已核对载体 | 对计划的影响 |
|---|---|
| [根 workspace](../../../Cargo.toml) | 新增总管包；迁移九包路径；保留一个 workspace 与锁文件 |
| [源码投影器](../../../source/forge/core/src/source_projection.rs) | 基线仅识别 `source/core/Cargo.toml` 与两层成员；首批已适配维护源与逻辑投影两种层级 |
| [工厂清单生成](../../../source/forge/factory/src/manifest.rs) | 基线存在 `templates/hooks` 与 `source/factory` 等约定；首批已同步模板和物理来源映射 |
| [产品分发清单](../../../source/forge/bridgeforge-manifest.json) | 源路径、技能登记、目标、稳定资产 ID 与哈希需要一致维护 |
| [版本合同](../../../.codex/bridgeforge-version.json) | 包输入、检查命令、共享输入与收口范围需要更新 |
| [共享更新器](../../../scripts/bridgeforge_codex_shared_update.ps1) | 现有远端、安装 home、日志及收据包含产品身份 |
| [cleanup 合同](../../../source/forge/templates/skills/cleanup/SKILL.md) | `.runtime/` 除 config/download 外可清理，不能承载唯一长期任务事实 |
| [文档索引](../../README.md) | `delivery_layout: flat`，本主题落在 `doc/1_delivery/bridgepilot-evolution/` |

前期项目外只读调查中，Git status 曾报工作树错误，未确认根因，也未改 Git 配置。2026-10-30 当前项目环境中的落盘核对已能执行 `git status --short`，现场存在大量既有未提交源码、投影、清单、安装器和测试改动。此前“无法读取状态”不能继续作为当前结论；当前也不是干净工作树。

计划落盘阶段仅追加本主题文档、索引和 TODO。首批开工已重新读取差异并划清既有改动与本需求边界，具体基线见第 14 节；后续接续仍须保护现场，不重置、不覆盖、不把既有成果归功于本任务。

源码检查与工具声明仅证明结构或接口存在。首批 stdio 隔离探测结果见第 14 节；Codex 桌面接管、持续唤醒、跨项目真实任务与换机完整链路仍未验证。

## 4. 目标代码组织

```text
BridgePilot/
├─ Cargo.toml / Cargo.lock           # 唯一 workspace
├─ VERSION / CHANGELOG.md
├─ AGENTS.md / README.md / INSTALL.md
├─ source/
│  ├─ orchestrator/                 # 新增：跨项目总管
│  │  ├─ Cargo.toml
│  │  ├─ main.rs
│  │  ├─ lib.rs
│  │  ├─ domain/
│  │  │  ├─ project.rs
│  │  │  ├─ task.rs
│  │  │  ├─ decision.rs
│  │  │  ├─ run.rs
│  │  │  ├─ operation.rs
│  │  │  └─ evidence.rs
│  │  ├─ application/
│  │  │  ├─ intake.rs
│  │  │  ├─ dispatch.rs
│  │  │  ├─ reconcile.rs
│  │  │  ├─ pause.rs
│  │  │  ├─ handoff.rs
│  │  │  └─ delivery.rs
│  │  ├─ ports/
│  │  ├─ adapters/
│  │  ├─ cli/
│  │  ├─ schemas/
│  │  └─ skills/bridgepilot/SKILL.md
│  └─ forge/                        # 原工程能力整体归组
│     ├─ core/
│     ├─ skeleton/
│     ├─ checks/
│     ├─ workflow/
│     ├─ lifecycle/
│     ├─ memory/
│     ├─ factory/
│     ├─ cli/
│     ├─ hook/
│     └─ templates/
│        ├─ AGENTS.md
│        ├─ user/
│        ├─ hooks/
│        └─ skills/
│           ├─ bridgeforge-codex/
│           ├─ confirm/
│           ├─ autopilot/
│           ├─ develop/
│           └─ …
├─ scripts/
│  ├─ 安装与分发入口
│  └─ tests/
│     ├─ forge/
│     ├─ orchestrator/
│     └─ support/
├─ doc/                             # 既有六层文档结构
├─ .codex/                          # 自身受管配置、登记与自用镜像
├─ .runtime/                        # 本机配置、日志、可重建索引与临时数据
├─ .deps/
└─ .build/
```

`forge/` 是职责目录，内部原有九个包保留各自边界和版本，不在本任务中统一扁平化。`orchestrator` 是一个新增包，可同时提供库和 `bp` 二进制；在 Cargo 清单显式设置 `lib.rs`、`main.rs` 路径，无 `src/` 层，不新增嵌套 workspace。

测试目录归组须同步引用；不得为目录外观改写无关测试或制造额外子包。总管专属代码、档案和依赖不进入下游公共投影。

## 5. 职责与依赖边界

| 组件 | 职责 |
|---|---|
| BP 总管 Agent 与 Skill | 自然语言理解、资料调查、复杂度判断、方案建议、confirm、协调和汇报 |
| domain | 项目、任务、决定、运行、操作、证据及合法状态规则 |
| application | 接收、派发、核对、暂停、交接、交付用例 |
| ports | 持久存储、执行能力、仓库查询和证据读取接口 |
| adapters | 持久化、Git 查询与具体 Codex 接入实现 |
| cli | 结构化输入输出；清楚表达成功、受阻、结果未知和所需动作 |
| forge | 骨架、检查、开发收口、版本、发布、安装和分发能力 |
| 项目执行聊天 | 在正确项目环境中实施与验证，通常采用单项目 autopilot |

依赖方向为入口与适配器依赖核心接口，总管按明确接口使用工程能力；forge 独立完成工程操作，不依赖总管在线。复用 core 中已存在的文件、路径、锁、进程和收据能力，只有真实共享需求才下沉新代码。

复杂度判断由 Agent 根据调查作出；程序保存判断及依据，并阻止被判定为复杂而未完成必要确认的任务进入实施。重要业务规则、架构或审美取舍不得包装成普通技术选择绕过用户判断。

公共工程规范不吸收下游业务语义。BP 项目定制区需区分“工厂验证仅在获准测试 worktree 写入下游”与“总管依据明确授权协调业务项目”，不能为了总管能力笼统取消现有工厂保护。

## 6. 任务、运行与持久化合同

### 6.1 对象与状态

| 对象 | 核心信息 |
|---|---|
| Project | 稳定 ID、仓库身份、约束入口；机器路径与执行绑定分开保存 |
| Task | 目标、非目标、范围、验收、依赖、复杂度和当前状态 |
| Decision | 用户原始来源、重要决定、授权范围与变更记录 |
| Run | 单次执行的机器、聊天、工作区、分支、代码身份及结果 |
| Operation | 待执行动作、关联任务、修订号、执行身份、结果与重试依据 |
| Evidence | 验证与发布证据、覆盖范围、对应代码身份及未验证项 |
| Checkpoint | 可移植状态快照、格式版本、校验信息与恢复条件 |

一个任务可有多次运行，聊天更换、换机和重试不复制任务。正常状态链为：

```text
已接收 → 待确认／就绪 → 执行中 → 验证中 → 待发布 → 已发布
```

另设受阻、暂停中、已暂停、结果未知、已取消，以及调查类任务的报告交付终点。运行结束、验证通过、发布成功和用户验收分别记录。交付不自动等于用户 `summary accept`。

### 6.2 长期档案与本机数据

本节下述 SQLite 仅作索引的选型属于首版历史方案。后续用户已选择 SQLite 唯一在线任务状态库，目标合同以[第九阶段](#bp-status-sqlite-plan)为准；当前代码尚未迁移，不能将新目标当作现状。

前期“SQLite 位于 .runtime 且作为唯一长期状态”的建议因 cleanup 合同而修正。本计划建议长期结构化档案作为事实来源，SQLite 仅作可重建查询索引；首版数据量较小时可不启用 SQLite。具体存储格式及事务实现由 P0-03 确定，不把该技术建议冒充已由用户单独确认的实现事实。

- 长期档案属于用户资料，保存项目、任务、决定、未完成操作、结果及交接，不视作可清理运行产物。
- 建议路径为用户管理工作区的 `doc/5_project_knowledgebase/bridgepilot/`；安装目录与用户工作区必须显式区分。
- 可重建索引、日志和临时数据进入 `.runtime/`；本机配置固定于 `.runtime/config/`；测试输出固定于 `.runtime/tests/<任务>/`。
- 产品更新不得覆盖用户档案；个人档案不进入公共模板、骨架投影或产品分发清单。
- 用户档案同步目标在换机接入时确定；不默认随产品源码公开分发，不记录凭据与登录材料。
- 文档索引指向档案说明入口，不逐项重复维护机器生成的任务数据；人类需求正文与机器状态各保留唯一事实源。

持久化实现须具备原子写入、修订号校验、单写入者控制、损坏检测及可恢复事务。多个对象变更不能假定多个文件写入天然原子；采用明确事务或日志协议，并验证断电／进程中断边界。

### 6.3 外部操作一致性

先可靠保存操作意图，再执行外部动作，最后记录实际结果。消息已发送但结果未保存时，恢复后先检查目标聊天和执行证据，不盲目重发。无法判断时保留结果未知。

数据库事务或文件锁只能保证自身状态的一致性，不承诺跨聊天、Git 和发布操作天然恰好执行一次。发布结果未知时核对远端状态与收据；禁止通过重发发布指令猜测恢复。

## 7. Codex 与技能接入

首选路径：核心生成结构化动作，BP Agent 调用当前客户端实际提供的 Codex 工具，再将实际结果交回核心保存。Rust 程序不能假定可直接调用桌面聊天内部工具。

P0-02 验证跨聊天派发、进展查询、执行中调整、暂停确认、总管再次唤醒和恢复。只验证一种首版执行实现，避免同时建设两套系统。官方 App Server 可作为替代路线，但是否接入桌面已有聊天、加载一致技能和权限须实测；能力不足时先说明具体分叉，不静默改为额外后台服务。

接入时保留直接来自用户的派发与新执行聊天创建授权；接收方 Agent 的请求或自称获准不能替代人类授权。执行绑定按稳定项目身份管理，不另建重复角色路由表。

已有 `confirm / autopilot / develop / summary / git-sync` 按实际能力复用，不复制实现。BP 统一向用户提问，项目聊天提供技术事实。复杂任务的确认结果和交付权限随同一任务包传递；新步骤、Skill 或聊天不重置授权，也不要求用户重述。

autopilot 维持单项目边界：BP 跨项目协调，各项目内部托管。用户已有“推进至发布”约定覆盖其默认本地未提交交付边界；真正的目标、范围或权限缺口才重新确认。预算由用户管理，BP 不做分配或自动追加；平台限制与实际不可用状态仍须如实处理。

## 8. 用户工作流、暂停与换机

### 8.1 日常工作

```text
用户在 BP 下达项目目标
  → BP 定位项目，读取已有资料与决定
  → 简单明确任务直接执行；复杂任务先 confirm
  → 形成任务包并交给项目执行聊天
  → 项目内通常使用 autopilot
  → BP 跟踪、核对、协调依赖
  → 必要验证、受管收口与发布
  → BP 汇报成果、证据、版本与剩余限制
```

示例：“StratusAgent：校对 BlackSwan 的交易信号，修复实现偏差，验证后发布。策略规则有歧义时先找我。”

BP 优先读取现有基准和决定，不凭“校对”二字猜测比较对象、数据区间或策略语义。已有资料足够则自行推进；缺失或冲突实质影响结论时再向用户确认。方向或原型评审先于深入实现。

正常执行不频繁推送无变化状态；重要判断、实质阻塞和阶段成果集中呈现。一个项目等待用户不拖停其他独立项目。同项目重叠写入需排序或获准隔离，发布协调串行化。

### 8.2 暂停

合法范围仅为全部项目或指定的非空项目集合。范围缺失时拒绝该暂停指令，要求用户明确；已有任务继续按原授权推进。

```text
校验范围 → 停止该范围新派发 → 请求在途任务停止
  → 核实运行及相关进程状态 → 保存检查点 → 标记已暂停
```

“已请求暂停”与“已安全停止”分开记录。对已开始的提交、推送、发布先核对实际结果；暂停不等于自动回滚外部操作。路线偏离时整理偏差与调整建议；疑似 Bug 或效率异常时保留现场、分析证据，不直接盲修或扩大范围。

### 8.3 换机

```text
原机器停止相关执行 → 下游代码形成工作分支检查点并推送
  → BP 保存任务档案和交接清单 → 核实同步成功
  → 新机器核对仓库、提交、环境与执行归属 → 恢复并重新核对现场
```

交接记录至少包含任务 ID、目标与决定、分支和提交、未完成事项、验证状态、执行绑定、恢复步骤、未同步材料及阻塞。忽略文件、大样本和本机依赖需明确记录，不以代码推送代替环境转移。

采用显式交接和执行归属控制，不仅凭心跳消失接管。旧执行是否仍在写入无法确认时，阻止新机器重复启动受影响任务。聊天 ID 不保证跨机可用；必要时从档案建立获准的新执行绑定，不能伪称完整聊天环境已经迁移。

## 9. 实施任务与依赖

下表是工作分解及完成标准；首批实际进度见第 14 节。任务编号属于 TODO-050 内部工作分解，不创建重复 TODO。

| 编号 | 工作与主要影响范围 | 验证与完成标准 |
|---|---|---|
| P0-01 | 核验 checkout、既有差异、写入者、工具、收据及当前输入 | 能区分原有改动与本任务改动；现场不明不移动目录 |
| P0-02 | 验证 Codex 派发、查询、控制、唤醒、恢复与授权传递 | 获准隔离任务取得真实结果，列明可用、不可用、未验证 |
| P0-03 | 固定对象、状态、持久化、清理、接入与动作合同 | 异常可解释、可恢复；选择唯一首版执行接入方案 |
| P1-01 | 修订项目架构规则、目录地图和分发边界 | 区分工厂验证与获准业务执行，不污染公共工程规则 |
| P1-02 | 九包移入 source/forge，模板移入 source/forge/templates | workspace、投影、模板读取、构建和工程行为一致 |
| P1-03 | 新增 orchestrator 包、CLI、模型与长期存储 | 状态转换、持久化、重启与清理后重建验证通过 |
| P1-04 | 实现 intake 与 BP Skill，衔接 confirm | 简单任务直行，复杂任务正确等待关键决定，下游不重复访谈 |
| P1-05 | 实现 dispatch、执行适配、reconcile 和依赖协调 | 重复唤醒不重复派发；结果未记账可核对；局部阻塞不拖停全局 |
| P1-06 | 对接项目内 autopilot、develop 与失败升级 | 目标、决定与授权完整继承，维持单项目托管边界 |
| P1-07 | 实现 delivery、验证身份与发布结果核对 | 过期证据不能用于当前交付；未知发布结果不盲目重试 |
| P1-08 | 实现暂停、检查点和换机恢复 | 模糊暂停零动作；指定项目准确停止；恢复不丢改动、不重复执行 |
| P1-09 | 登记五项目，选择两个真实并行试运行 | 两条真实任务完成发布，验证重要判断、暂停与换机续接 |
| P2-01 | 产品、入口、仓库地址与安装身份迁移 | 旧安装可升级，新安装可用，中断可恢复，历史识别完整 |
| P2-02 | 文档同步、开发收口与正式交付 | 主程序与子包记录匹配；验收、发布和未验证边界清楚 |

主要依赖：P0-01/02 → P0-03 → P1-01/02 → P1-03/04 → P1-05/06/07/08 → P1-09 → P2-01/02。

独立取证可并行；依赖不满足、接口或现场漂移时不强行推进。实施与独立复核安排需遵守届时有效的执行约定，本计划不自动启动子 Agent。

## 10. 迁移与分发细节

| 迁移基线区域 | 同步修改目标 |
|---|---|
| 根 Cargo.toml 与各包清单 | 成员、依赖路径、默认成员、公共成员及新包登记 |
| source/core/src/source_projection.rs | 源码发现、成员层级、依赖路径转换及投影选择 |
| source/factory/src/manifest.rs、factory_runtime.rs | 模板根、工厂包位置、生成资产及构建来源 |
| bridgeforge-manifest.json | 源路径、技能登记、稳定资产 ID、目标、所有权与哈希 |
| .codex/bridgeforge-version.json | 包输入、验证命令、共享输入、收口与版本范围 |
| .codex/managed-skeleton.json 与相关生成资产 | 通过受管机制刷新，不手工伪造合同或收据 |
| scripts/tests/** | 相对路径、测试工程、样本与源码内测试引用 |
| 安装器与共享更新器 | 模板／技能查找、安装和更新来源 |
| AGENTS、README、INSTALL 与架构资料 | 目录地图、事实源、安装与恢复入口、索引 |

先保持下游目标路径和稳定资产身份，避免内部目录重组扩散为无必要的下游迁移。映射使用显式源与目标，不用全局字符串替换猜测归属，也不用 glob ownership。

公共工程运行时、工厂运行时与总管运行时分别明确构建成员。新增总管后，不能继续假定整个 workspace 都属于工厂运行时。总管专属依赖不进入下游工具；共享 Cargo.lock、内容指纹和证据失效范围按实际合同核验，不能通过忽略真实依赖变化规避检查。

迁移分阶段留可恢复边界，保留既有产品版本与收据，不直接回滚用户未提交内容。投影、生成清单和自身镜像通过受管入口生成；零写入检查保持零写入。开发收口与发布准备分别处理，复用仍有效的证据，不以 release 为由默认重跑整套业务回归。

BP 自身开发与在用版本分离，在安全边界切换。换机档案、未完成操作和用户配置不因产品升级被覆盖或清理。

## 11. 更名与版本策略

总管能力及两个项目试运行通过后，再进行统一产品更名和安装迁移。外部产品名为 BridgePilot；内部 crate 名、稳定资产 ID、历史收据字段、版本标记仅在必要时迁移，不为外观统一制造无意义断裂。

迁移清单包含远端仓库与更新地址、安装 home、用户级技能入口、日志／收据识别、骨架来源和文档。具体新标识由迁移设计统一定义，不提前分散硬编码。

必须覆盖旧安装升级、新安装、旧新入口冲突、中断恢复与失败回退，明确兼容终点，不长期维护两套实现。根 VERSION 继续是产品主版本，子包按既有生命周期独立收口；不擅自预设发布版本号。

总管更新不自动触发所有下游业务验证，受影响范围由实际资产、依赖及指纹决定。源码迁移等价性和自身镜像须独立复核；真实下游安装结果与工厂自测分别报告。

## 12. 验证矩阵与首版验收

验证分层：状态及恢复规则测试、工程迁移集成测试、Codex 隔离链路验证、两个真实项目验收。计划阶段不执行这些测试；文档链接检查不等于工程验证。

| 场景 | 通过标准 |
|---|---|
| 简单任务 | 无多余确认，直接完成明确范围 |
| 复杂任务 | 先调查和 confirm，重要决定完整传递 |
| 两项目并行 | 可持续跟进，任务、路径、代码和结果不串项目 |
| 方向／审美变化 | 深入实现前评审，其他独立工作继续 |
| 模糊暂停 | 不暂停任何项目，要求明确范围 |
| 指定暂停 | 只作用于指定项目，取得停止结果 |
| 重复消息／唤醒 | 不重复派发、提交或发布 |
| 外部操作后未记账 | 恢复先核对，保留未知结果，不盲目重试 |
| 测试失败／连续修复失败 | 如实记录并遵守失败升级规则，不换 Agent 清零 |
| 过期证据 | 不能证明当前候选已验证 |
| 持久化中断／损坏 | 不丢已确认操作，不把半写入记录当有效状态 |
| cleanup | 长期任务、决定、未完成操作与唯一证据保留，本机索引可重建 |
| 换机 | 代码与任务对应，旧新机器不同时执行，未同步材料明确 |
| BP 自身升级 | 在途状态保留，版本切换可追踪 |
| 骨架与安装回归 | 工程能力可独立使用，下游分发不含总管专属依赖 |
| 两个真实项目交付 | 完成真实任务、发布及暂停恢复验证，成果与证据可核验 |

独立测试放 scripts/tests，输出放 .runtime/tests；原生内联测试可保留模块内。只为关键规则、真实故障边界及迁移合同编写必要测试，不用实现镜像测试堆砌数量。完整回归须有具体风险依据；版本、共享输入或环境变化后重核受影响证据。

首版验收应同时提供：五项目登记结果、两个真实任务的目标与最终结果、验证与发布身份、重要判断记录、暂停结果、双机交接与恢复证据，以及未验证项。任务发布完成不自动代表用户已验收 BP 产品。

## 13. 风险、停止点与实施输入

| 风险或缺口 | 处理与停止点 |
|---|---|
| 已有大量重叠未提交改动 | 先核对归属与在途写入者，不覆盖、不重置 |
| 原生工具不能完成持续唤醒或在途控制 | 阻止依赖能力的实施，说明替代方案与对使用方式的影响 |
| 模块层级迁移破坏投影和安装 | 同步合同、映射和测试；不以仓库可编译代替分发通过 |
| 共享锁文件／版本合同扩大验证范围 | 核验实际受影响包及证据，不承诺天然完全隔离 |
| 长期任务误放可清理目录 | P0 明确持久与临时边界，cleanup 恢复场景必须通过 |
| 双机同时执行 | 显式交接与归属检查，旧运行不明则阻止重复启动 |
| 产品更新覆盖档案或改变在途规则 | 分离安装代码与用户档案，安全边界切换运行版本 |
| 安装与身份迁移损坏旧入口 | 分阶段迁移，先验证升级／中断恢复，不盲目删除旧材料 |
| 权限、登录、网络、工具链或额度不可用 | 如实记录阻塞，只停受影响工作；不绕过平台限制 |

实施前需核实：可用 checkout；实际 Codex 执行路径；五项目路径与执行绑定；两个试点任务；目标换机环境及档案同步目标；有效实施授权与执行约定。能通过工具核验的由 Agent 调查，只有改变使用方式、授权或验收的真实分叉才再次 confirm。

## 14. 文档、进展与交接

本文件保留需求、任务表、实施记录及验收；长期架构进入 doc/0_architecture，使用方法进入 README，安装恢复进入 INSTALL，并同步唯一 doc/README 索引。实施阶段不维护另一套重复正文。

### 2026-10-31 首批实施启动

- 用户先授权“可以开始了”，随后明确“以三小时为界，开始吧”。本批硬时间窗口为新加坡时间 **2026-10-31 10:58:12–13:58:12**，13:43:12 起保留十五分钟收尾，不自动延长。
- 本批优先完成 P0 能力核验、forge 目录归组及必要回归；两个真实项目发布、双机验收和最终更名不作为三小时内已保证完成的成果。不从实施授权推导本批提交、推送、发布或真实下游修改授权。
- 原 main 基线为 `475b04f`，主程序版本 13.0.3。只读提权 Git 查询确认原有未提交改动为 `doc/README.md` 和另一交付文档；保留其内容。沙箱内 Git status 报错不能作为实际工作树状态。
- 已验证：Codex CLI 0.166.0-alpha.5.1 的独立 stdio App Server 初始化、临时任务启动、读取、取消终态，以及持久测试线程在服务重启后的 resume。取消与自然完成有竞态，分别按真实状态记录。测试仅使用隔离任务，成功恢复的持久测试线程已归档。
- 桌面默认 proxy 连接失败，桌面已有聊天接管、自动唤醒和跨机器完整恢复仍未验证；不能将 stdio 控制能力等同于这些能力通过。首版接入接口继续隔离，不新增常驻服务。
- 当前已将九包及模板移动到 `source/forge/`，补充显式 factory-members；保留对外 logical `source/<包>` 投影目标及资产身份，迁移当前注册路径，不改写 baseline、adoption、withdrawal 与既有 closure 历史。
- 已执行 `cargo check --locked --offline --workspace --target-dir .build/bridgeforge/test-target`，九包离线锁定编译通过；尚需投影、清单、安装入口和独立复核，不能据此宣布本批完成。
- 本批能力及验证产物位于 `.runtime/tests/bridgepilot-p0/`、`.runtime/tests/bridgepilot-stage1/`，属于可清理证据副本；本文件保留结果摘要和继续条件。

### 首批迁移设计与证据边界

- 维护源使用 `source/forge/<包>`，安装及独立运行时仍使用 logical `source/<包>`；模板解析统一通过显式路径解析器，兼容既有独立快照和旧 fixture。不创建目录链接，不要求下游仅为内部归组迁移其路径。
- workspace 分别登记 public-members 和 factory-members，两个投影都按所选成员裁剪路径依赖与锁文件闭包，并拒绝逻辑路径碰撞。隔离测试包含模拟 orchestrator 成员及其专属依赖，确认它们不进入工程投影；这不代表真实 orchestrator 已实现。
- 包身份保持不变：七条新增 move 声明绑定正式基线 `475b04f691e6789e1c5169ae2a4f3e2f795cd621`，当前路径进入新目录，原 baseline、adoption、withdrawal 与 closure 历史保留。独立复核确认既有 asset_history 不变、既有安装 target 无变更。
- 模板和自身镜像由受管 manifest materializer 刷新。编译期根清单／锁路径改由包 build.rs 从所属 workspace 提供，避免维护源加深一层后固定相对 include 路径失效。
- 独立只读复核确认 CLI Skill 扫描、新旧 Hook 模板路径、项目地图模板排除、真实仓库 fixture 读取、日志相对路径、维护 Skill 入口及投影边界均已同步；未发现剩余迁移阻断。主对话另核对 190 个已跟踪迁移文件，新位置全部存在。
- 已通过 core/hook 串行回归：core 88 项通过、8 项忽略；hook 32 项通过、2 项忽略。core 中实际公共及工厂源码快照均完成 `cargo check --locked --offline`。独立集成工程已通过锁定离线编译；具体执行覆盖仍以最终候选收据为准。
- 首轮 workspace 验证曾发现过时 fixture 路径、Git fixture 误向父仓库发现等问题，已修正；并行运行出现过保守进程检查失败，后续 core/hook 串行回归通过。原 broad continuation 在 lifecycle 阶段被本批主动中断，不能报告完整 workspace 一次全绿；失败和中断日志保留在 stage1 目录。
- 首次受管 apply 因分析指纹过期在写版本前拒绝，冻结输入后重取预览。后一次 apply 在 factory-distribution 暴露旧宿主 CLI：`shared_transaction.ps1` 实际调用 `.build/bridgeforge/bin/bridgeforge.exe`，旧二进制仍读取已迁走的 `templates/user/AGENTS.md`，返回 os error 3。事务自动回滚且 journal 已清除；独立复核确认调用链。应先通过官方 `build-assets --factory` 刷新宿主工具，再重新取预览、分析与执行检查；不能只编译测试工程或修改 fixture 来掩盖旧工具。子包候选改变后还须再次核对最终工具收据。
- 刷新后 prepare 实际生成三个工具并执行 self-test；原失败分发场景独立运行 1 项通过，旧工具退役保护独立 1 项通过，CLI 25 项及 Hook 32 项回归通过。随后全套 lifecycle 检查达到本批分析设定的 1800 秒上限，退出 -1、timeout=true，受管事务自动回滚且 journal 清除；原完整登记仍保留 7200 秒，未被修改。部分通过不能当作整套通过，完整超时日志保存为 `lifecycle-full-timeout.log`，当时分析保存在 `full-suite-analysis.json`。
- 独立复核逐文件确认 lifecycle 生产算法未变，9 个源码差异全部是外置测试 path 增加一级父目录。后续通过已有 `lifecycle-unit` 允许的单任务过滤器 `release::rust::tests::` 验证 38 项 Rust discovery、workspace/lock、ownership、历史与版本渲染；其他包仍按已选登记检查执行。不是重绑旧日志，也不更改全量检查或稳定条件。最终候选还须独立补跑 6 项 identity、两个 schema4 move 单例及工厂退役保护；以末尾实际结果判定本批完成。
- 下一轮在 skeleton 的真实模板升级测试发现单行旧路径：`core_project_sync_content.rs` 仍读取根 `templates/doc/README.md`。改为既有 `template_path` 后，skeleton 26 项独立回归通过；原断言全部保留。该文件仅编入 skeleton 的 cfg(test)，不属于 factory-unit 输入或测试编译边界，独立复核确认前轮相同候选上通过的 factory 39 项仍有效；最终受管选择 factory-distribution，不将这 39 项伪装成本轮 native 重新执行或缓存命中。
- workflow 的独立全 101 项因耗时被本批主动中断，仅停止本批启动的测试进程，没有版本事务。独立逐文件复核确认 workflow 生产逻辑未变，只有 4 处测试 path 增加父目录；最终通过原登记的 `workflow-unit` 加 `project_sync::tests::` 单任务过滤器，以及 settings-preservation、hook-independence 两项集成验证安装、ownership、源退役与回滚。未完成 workflow 全量 Git 事务及 cleanup 回归，不宣称全 101 通过；原检查登记不变。
- workflow 专项随后发现一个夹具按新清单 source 复制文件时未创建嵌套父目录；仅补齐该目录创建，原断言不变，49 项项目同步检查独立通过。settings 集成则是旧安装夹具的清单保留旧 source，而此前路径适配误将写入套用新模板解析器；恢复按旧清单写入后，settings 保留场景独立 1 项通过。两处均不修改产品逻辑；前述失败 apply 均由受管事务回滚。
- 隔离能力探测使用 CLI `0.166.0-alpha.5.1`。`app-server-probe.json` 记录临时任务取消终态，`restart-probe.json` 记录持久任务自然完成、重启 resume 及归档；取消和完成的竞态不被伪装成同一条成功链路。默认桌面 proxy 连接失败，不采用它作为已验证执行入口。
- 上述重启 resume 证明的是已完成持久对话可以重新加载，不证明正在执行的开发任务在进程中断后可以无损续跑；后者仍需执行协议及实际验证。
- 后续只发送 initialize 的诊断明确记录默认控制 socket 连接失败，退出 1、os error 10050，见 `proxy-initialize-diagnostic.json`。未创建任务或操作其他聊天；该结论限当前执行环境，系统原因未确定，不宣称产品原理上不支持，也不擅自修复原生服务。
- 桌面 `read_thread` 已成功读取前述 stdio 持久测试聊天及 completed 终态，证明这类聊天可以由桌面工具读取；实时侧栏显示、项目归类和桌面继续操作仍未验证。后台控制与桌面可见性不应被误作排他选择。需要显示执行对话时，设计归入对应下游项目，BP 项目保留总管及 BP 自身任务；用户尚未确认必须显示或接受不显示。
- 一次提前终止服务的失败探测遗留空原生 rollout，无法通过原生归档入口清理，见 `cleanup-probe.json`；未直接改写原生数据库或会话文件。后续执行协议须先验证持久元数据可读，再交接；外部结果不明时保留未知态，不能依靠原生聊天文件替代 BP 长期档案。
- 已保存 [运行协议草案](../../0_architecture/design/bridgepilot-runtime-protocol.md)，说明实际能力矩阵、不可变修订加原子 HEAD 的档案方案、操作意图、结果未知核对及同机／跨机归属边界；未创建档案或后台服务。唯一执行适配器、实际唤醒及用户档案工作区仍待收敛，P0-03 未闭合。
- `orchestrator`、长期档案实现、持续唤醒、桌面聊天接管、真实下游任务、两项目发布及双机交接均未交付。下一阶段从 P0-03 固定执行／唤醒和持久化合同后进入 P1-03；不能凭源码迁移完成宣称 BP 已可持续托管。

### 首批受管收口（2026-10-31）

`develop --apply --packages core,skeleton,checks,workflow,lifecycle,memory,factory --analysis-file .runtime/bridgepilot-stage1-analysis.json --root .` 返回 `development-closed`、退出 0，使用最终输入指纹 `sha256:82f4e0b982e623b0a75424febb710149bcf0c6f1cd5006b51d71193fb4a07e4a`。实际执行 9 项最终候选检查、296 个用例通过，native reused=0；无 Git 写入，事务 journal 已清除。

| 检查 | 实际通过数 |
|---|---:|
| checks-unit | 46 |
| core-unit | 88 |
| factory-distribution | 4 |
| lifecycle-unit（Rust 子集） | 38 |
| memory-unit | 43 |
| skeleton-unit | 26 |
| workflow-unit（project_sync 子集） | 49 |
| workflow-settings-preservation | 1 |
| workflow-hook-independence | 1 |

前轮相同生产代码、候选版本及该检查输入上通过的 factory 39 项作为仍有效的工程证据保留，见 `factory-unit-valid-candidate.log`；不计入上述 native 9 项或 reused。完整多语言及完整 workflow Git 事务回归没有完成。本收据的 readiness_scope=task、outside_task_evidence_checked=false，不代表全仓 release 准备完成。

| 包 | 原版本 | 新版本 | 原因 |
|---|---|---|---|
| core | 10.0.2 | 10.1.0 | 新增物理源／逻辑投影解析与显式成员隔离，保留安装接口 |
| checks | 0.2.1 | 0.2.2 | 目录归组及读取／测试路径适配 |
| factory | 0.10.0 | 0.10.1 | 目录、模板、快照及分发源定位适配 |
| lifecycle | 0.8.2 | 0.8.3 | 目录归组及测试引用适配，生产算法未变 |
| memory | 0.1.0 | 0.1.1 | 目录归组及测试引用适配 |
| skeleton | 0.5.1 | 0.5.2 | 目录归组及模板测试适配 |
| workflow | 0.9.1 | 0.9.2 | 目录归组及测试引用适配，生产算法未变 |

原生 `.codex/package-closures.json` 的末七条 before/after 与上表一致。主程序 VERSION 及 CLI/Hook 载体仍为 13.0.3；未配置稳定验收合同的 0.x 包保持开发阶段，未人工转为 1.0.0。最终产物、独立补验与交接状态见本节后续结果，不以版本数字代替这些检查。

最终工具通过官方 `cargo run --locked --offline --release … --bin bridgeforge-factory -- build-assets --factory --project-root .` 刷新，返回 prepared=3、reused=0、self_test_executed=true，见 `prepare-final.log`。对最终版本补跑工厂退役保护 1 项、生命周期 identity 6 项及两个 schema4 move 单例各 1 项，全部通过；日志为 `retirement-final`、`identity-final`、`cargo-move-final`、`history-final` 的 `.log` 与 `.stdout.log`。

最终零写入核验：manifest --check 返回 changed=false；factory-version 为 healthy=true，版本与合同均为 13.0.3；build-assets --factory --check 返回 current/read_only=true、reused=3、built=0，未再运行 self-test。见 `manifest-final-check.log`、`factory-version-final.log`、`runtime-final-check.log`。

本批工程迁移及适用子包收口完成，整体 BP 需求仍为 active/in_progress。交付的是可继续开发的工程底座、能力调查结果与运行协议草案；未实现 orchestrator、BP Skill、任务持久化或实际跨项目托管。下一阶段先完成 P0-02/03 的执行、显示、权限／技能与唤醒验证，再进入 P1-03；源码不再反复重新归组。

本批没有提交、推送、主程序发布、真实下游修改、用户级共享安装或常驻自动化；既有未提交资料保留。普通 TODO 状态不由本轮开发自动结算，TODO-050 的实际进度以本计划及索引为准，未记录用户验收。三小时硬截止不变；停止新增功能，最后仅做文档与交接检查，token 未实测。

收尾核验已通过：最终 CLI baseline 为 clean，instruction-source 退出 0，skill-metadata 零 issue，project-structure 零 error；文档落盘后的 manifest --check 仍 changed=false。新增本地链接检查 9 个、缺失 0，git diff --check 通过（仅配置文件已有 CRLF 规范化提示）。HEAD 保持 `475b04f691e6789e1c5169ae2a4f3e2f795cd621`；另一原有未提交交付文档 SHA-256 仍为 `C87CBFAC05B73341457D9382345C1D4601E7C09ABEC11B73E5434670D07CCAD2`，未被本批改变。

最终独立只读核验无新增阻断：七份 Cargo.toml、根 Cargo.lock 与末七条 ledger 的版本／历史链一致；九份原生日志的 296 通过与收口收据一致，筛选范围被原生记录保留；独立九用例及最终工具收据一致；三个夹具修正没有删除断言。约在新加坡时间 2026-10-31 13:40 收在本批工程迁移完成点，用时约 2 小时 42 分，未动用三小时以后的时间，不再启动下一阶段。

### 2026-10-30 文档落盘

- 完成：根据当前对话保存完整开发计划，关联 TODO-050，状态为待执行；同步文档索引。
- 当前 Git 状态可读取，存在大量原有未提交改动；本主题不处理其实施、提交或发布。
- 产品能力、Codex 运行链路、目录迁移、数据库／档案协议、发布与双机验收均未开始。
- 生命周期为 active，验证状态为 not_started；文档校验结果仅在实际执行后报告，不将其当成产品验证。
- 文档检查：9 个本地链接均存在，索引和 TODO 回链已建立；`git diff --check` 对本次索引／TODO 变更无格式错误；现有受管 `bridgeforge.exe check todo --root .` 返回零错误，识别 TODO-050 为待执行。以上仅为文档检查，不改变产品 validation_status。
- 下次接续从 P0-01 开始，先核对变化和授权，再进入已约定的流程；不因本文件存在自动开工。

### 第二阶段：总管核心与受控执行（2026-10-31）

本阶段按用户“继续推进 BP 总管本体，下一阶段仍以三小时为界；后台方案自行决定，独立执行对话归下游项目，重要方向再确认”接续，不重问既有架构决定。开始时间为 2026-10-31 14:24:56、新加坡时间硬截止 17:24:56；17:09:56 起只做收尾。三小时是本阶段上限，不代表完整 BP 的交付工期；未授权本轮提交、推送、主程序发布或真实下游业务写入。

传播四问：新增 orchestrator 是产品层的 BP 私有能力，不进入公共模板／八包投影；公开既有 core ProcessTree API 是兼容通用能力，须同步 dogfood。根配置与本地 Skill 导航是自身配置，架构／安装／交付说明是元文档。子包按受管 develop 记账，主 VERSION 与正式 CHANGELOG 留给另行授权的 release；不改用户级安装或正式产品名称。BP 业务任务授权与工厂测试 worktree 授权在项目专区区分，公共 AGENTS 与 Hook 合同保持原内容。

本阶段实现：

- `source/orchestrator/` 与 forge 平行，使用 lib/main/cli/domain/store/codex 六个 Rust 文件，没有额外 src 层；复用已有 core 持久化、FileLock 和无窗口 owned ProcessTree。
- 项目登记、机器绑定、任务目标／非目标／验收／依赖／逐项授权；简单任务直接 Ready，复杂任务等待当前修订 confirm。路线修改使旧确认失效，任务和执行代次分开。
- 私有档案采用不可变完整修订加原子 HEAD，完整父链校验、严格 JSON、CAS 和本机独占锁；异常保留孤立修订，损坏时停止，不自动回滚或改写原生 Memory。
- 原生客户端动作 outbox 与唯一 stdio 执行端：先在正确项目创建并等空闲，保存真实 threadId/projectId/cwd 回执，再 resume；缺回执不能偷偷创建隐藏执行对话。CLI 不覆盖 model/effort，记录实际解析值、根 AGENTS 来源和摘要。
- 明确范围暂停、同修订保留对话恢复、Unknown 禁重发、原 owner/server 停止后严格匹配原生派发历史。恢复提交绑定读前 revision，不能用过期观察核销新 owner。
- 项目本地 BP Skill、Codex 使用示例与 INSTALL 唯一恢复入口；没有新增共享 Skill 分发项或第二份 Agent 路由表。该 Skill 通过根 AGENTS 项目专区登记并显式读取。

后台方案选用“原生客户端创建与归类 + BP 自管 stdio 执行”。实测旧 stdio 测试对话无法安全移动到项目，默认 proxy 仍连接失败，因此不采用创建后强行移动或原生数据库改写。隔离旧对话已经重新归档。原生客户端创建工具仍须遵守其用户明确新建对话要求；本轮没有创建新的业务执行对话。

独立验证发现并修正四类风险：同工作树项目别名绕过串行、错误或缺失原生空闲回执、助手引用标识被误认任务结果、恢复读取期间新 owner 被过期观察覆盖。复核确认原问题闭合；仅审读及核对真实日志，没有扩大到真实模型任务或下游代码。

#### 实际验证与版本收口

`develop --apply --packages core,orchestrator --analysis-file .runtime/bridgepilot-stage2-analysis.json --root .` 返回 `development-closed`，退出 0。预览指纹 `sha256:d5904b3ddad6da00cbbd2eb109fdb501ebaad2558b1b29c171bf390f4749ddde`；实际有效范围为 core、orchestrator 与六个依赖／构建覆盖关联包。最终候选执行十项必要检查，reused=0，git_written=false。日志为 `.runtime/tests/bridgepilot-stage2/bridgepilot-stage2-development-apply.log`，真实账本为 `.codex/package-closures.json` 末八条。本阶段其他日志同置于 `.runtime/tests/bridgepilot-stage2/`。

| 检查 | 实际通过数／范围 |
|---|---:|
| checks-unit | 46 |
| core-unit | 88 |
| factory-distribution | 4 |
| lifecycle-unit | 38，保留已登记 Rust 子集 |
| memory-unit | 43 |
| orchestrator-unit-integration | 22 单元 + 9 子进程集成 |
| skeleton-unit | 26 |
| workflow-unit | 49，保留已登记 project_sync 子集 |
| workflow-settings-preservation | 1 |
| workflow-hook-independence | 1 |

现有九项日志在 `.runtime/tests/development/`；BP 的独立定向日志为 `.runtime/tests/bridgepilot-stage2/bridgepilot-stage2-reconcile-cas-tests.log`。合计十项检查 327 个用例；不是完整 workspace、真实下游或自然 runtime 回归。批准请求拒绝、丢响应精确核对、错误线程／缺历史／未知状态、标识引用与错误 operation、暂停和并发归属漂移均有夹具证据。

| 包 | 本阶段变化 | 原因 |
|---|---|---|
| core | 10.1.0 → 10.2.0 | 公开既有 owned ProcessTree API，兼容新增 |
| orchestrator | 首次登记 → 0.1.0 | 新包首次开发收口；无既有有效基线，不手工制造额外升版 |
| checks/factory/lifecycle/memory/skeleton/workflow | 版本不变 | 源码行为未变，受管刷新真实依赖与构建覆盖 |

BP 专用 release 程序已用锁定 Cargo 构建：`.build/bridgepilot/cargo/release/bp.exe`，初批为 0.1.0，日志 `bridgepilot-stage2-bp-build.log`；最终时间修正后的 0.1.1 见本节末尾。根 workspace 默认成员只含 Forge，实际命令显式 `-p bridgepilot-orchestrator --bin bp`；安装说明已按验证修正。当前程序纳入 `.codex/artifact-layout.json` 的 keep，未保留整个编译缓存。

最终 BP 程序实际连接本机 Codex `0.166.0-alpha.5.1` 完成只读 ephemeral 协议探测，返回 D:/Quant/BridgeForge、readOnly/networkAccess=false、gpt-6.1-sol/high 和用户／项目 AGENTS 路径，见 `bridgepilot-stage2-real-probe.json`。未发送模型回合，不证明真实原生项目交接或业务执行。

#### 私有工作区与后续边界

本机工作区为 `C:/Users/bridg/Documents/Codex/BridgePilotWorkspace`，由产品 `.runtime/config/bridgepilot.json` 绑定。实际 CLI 创建 archive 并逐一登记 BridgePilot、StratusAgent、CausisRiskSuite、HoldemTrainer、BridgeLibrary；原生 projectId 来自实际 list_projects，仓库身份来自各仓库只读 origin 核对。最终 revision=5、五个 Project／五个本机绑定，Task／Run／Operation 均空；actions 没有待派发动作。证据为 `bridgepilot-stage2-private-workspace.log`。私人路径和仓库清单未写入公共模板，未配置远端同步。

P1-03 的核心模型／存储与 P1-04 本地入口已实现；P1-05、P1-08 已有受控执行和同机暂停／恢复部分，仍不能宣称完整完成。P1-06 目前只在任务包传递既有确认与项目 autopilot/develop 指令，真实技能执行未验证。P1-09 仅完成五项目登记，未完成两项目真实发布。

尚未实现／验收：持续 scheduler／heartbeat、真实原生项目创建归类到 stdio 的端到端业务任务、完整 Evidence／交付和发布认证、代码检查点与跨机交接。正常 completed 只到 Reviewing，不标 Released；没有安装后台服务、创建常驻自动化或修改真实下游。整体需求保持 active/in_progress，未记录用户验收，不自动结算 TODO-050。

下一阶段从 P0-02/03 剩余的真实项目交接与唤醒验证进入 P1-05/06，再实现 P1-07 交付证据；保留当前稳定核心和真实子包收据，不重做目录归组或追改主版本。

#### 时间约束修正与最终交付候选

收尾发现 execute 的旧计时从 turn/start 响应后开始，握手和停止确认可能额外延长，故在同一阶段内修正：从 execute 入口使用单一 Instant 上限，含握手、发送任务包、执行及停止确认；提前留最多十秒请求 interrupt，到上限只终止自有 ProcessTree Job。定时器持有 Weak，不延长树寿命，正常退出取消等待；不会按外部 PID 终止其他任务。接收、pending 事件、RPC 返回和终态事务均复核截止时间，迟到或缺失结果保留 Unknown。BP 本地 Skill 同步使用本阶段剩余已授权时间，不通过重试重新计时；不把操作系统调度与档案 I/O 声称为实时保证。

新增故障场景覆盖握手挂起、turn/start 不返回、interrupt 不确认，以及服务端不读 stdin／大任务包写入阻塞。使用两秒和四秒时限，带合理调度余量核验返回，实际检查自有夹具 PID 消失，任务保持 Unknown、不误记 Completed／Paused；输入阻塞还验证 start 意图已尝试且服务端未消费。独立复核确认定时器归属／释放与迟到终态边界闭合。

最终 `develop --apply --packages core,orchestrator --analysis-file .runtime/bridgepilot-stage2-time-bound-analysis.json --root .` 返回 `development-closed`，退出 0，指纹 `sha256:c7be127185070bfadecdab3490a03e7df1dee1d68ec877e265002207af7b8d69`。只有 orchestrator 受影响，实际执行其完整包检查一项（22 单元＋10 集成），reused=0；其余 Forge 输入未变，沿用已完成记录，没有重跑首批九项。orchestrator **0.1.0→0.1.1**，core 保持 **10.2.0**，主 VERSION 保持 **13.0.3**。收据为 `time-bound-apply.log`，账本最后一条与 Cargo.toml／Cargo.lock 一致。此处 32 个 BP 用例包含首批 31 个的重验，不与前表机械相加冒充独立用例数。

最终程序按 INSTALL 命令构建成功，`bp-final-build.log`、`bp-final-self-test.json` 对应 0.1.1；`bp-0.1.1-real-probe.json` 为最终程序真实只读协议探测，仍无持久任务或模型回合。原有 Forge 三个程序经受管准备刷新并各自 self-test（prepared=3、reused=0、retired=0）；BP 私有补丁后零写入核验仍 current/read_only=true、reused=3、built=0，没有重复构建。

最终工程检查：manifest changed=false、factory-version healthy=true／13.0.3、baseline clean、instruction-source 通过，共享与 BP 本地 Skill 元数据无 issue，项目结构零 error，Rust 格式检查及 Git diff --check 通过。七份相关说明的 219 个本地链接均存在；registry 包集合仍为 105，无新增或移除外部包。

结构检查曾发现本轮 22 份日志误落在 `.runtime/tests/` 根，已经按任务移入 `.runtime/tests/bridgepilot-stage2/`，逐个 SHA-256 校验内容未变，映射保存在 `log-relocation.json`；原检查结果保留，没有改造为通过记录。文档引用已同步，修正后结构检查通过。之后新增日志均在任务目录。

HEAD 保持 `475b04f691e6789e1c5169ae2a4f3e2f795cd621`，原有另一未提交交付文档摘要保持 `C87CBFAC05B73341457D9382345C1D4601E7C09ABEC11B73E5434670D07CCAD2`。本阶段交付为稳定可接续的总管核心与真实档案登记；持续调度、真实下游交接、发布认证、跨机接管仍未完成。尚未提交、推送、发布、改变用户级共享安装或结算用户验收；三小时上限不因补修或步骤切换重置。

最终任务预览为 `task-development-current`，affected=[]、pending={}，见 `final-development-status.json`；只证明本阶段选择范围有效，不代表全仓 release 就绪。最终 BP 0.1.1 读取私有档案仍为 revision=5、五项目、零 Task／Run／Operation；公共八包投影未出现 orchestrator，原有 Git HEAD 与保护摘要一致。16:03 左右在本阶段总管核心可交接节点提前收口，实际约 1 小时 38 分，未耗尽三小时；不是完整 BP 开发完成。可试用首版剩余工期粗估 6–9 小时，持续调度与真实执行闭环、交付／发布核对分阶段推进；真实业务发布及双机验收的不确定性另行评估，这一估算不构成延长本阶段或提前发布的授权。

### 第三阶段：常驻调度与真实只读执行（2026-10-31）

沿用用户“继续推进 BP，下一阶段三小时为界，优先持续调度和真实下游执行闭环，技术方案自行决定”的授权。本阶段开始于新加坡时间 **16:04:40**，硬截止 **19:04:40**，18:49:40 起只做收尾，不因重试重置。用户另外明确答复“允许新建这两个只读验证对话”（确认项 `call_ec4e7377ec7c4ab4a499f18f5f2f1181`）：仅 StratusAgent、HoldemTrainer，读取 VERSION／AGENTS／INSTALL 和 Git 状态；没有下游业务写入、运行项目／测试、提交、推送或发布授权。

传播四问：本阶段 Rust 总管、监督器、证据核验和私有 BP Skill 均是产品层的 BP 私有能力，不进入公共模板或八包投影；检查登记是自身配置，安装／协议／进展是元文档。需要 orchestrator 子包开发收口，主 VERSION 与正式 CHANGELOG 留给另行授权的 release。Forge core 实现本阶段未改，不需要同步公共 dogfood；不修改 AGENTS 公共区、Hook 或用户级安装。

实际新增：

- `scheduler.rs` 在明确项目集合、并行数和单调期限内持续观察新任务／实际回执；跨项目并行、同项目串行，局部 Unknown 不阻止其他项目，重启不重发未知任务。持久记 launch_worker 后才创建 worker，worker 校验确切 PID 和 Windows 创建身份，不能被竞争 CLI 抢认。
- `owned.rs` 提取同一自有 ProcessTree／期限设施；超时只回收本监督器的 Job。Windows 新记录保存 owner/server 创建身份，解决实测旧 PID 被 rustc 复用而误阻断；旧记录保留保守检查，没有伪补身份或杀外部进程。
- 原生交接改为 `idle → 原生 archive 确认 → stdio thread/unarchive 数据恢复且 notLoaded → resume`；prepare_execution_chat 的真实恢复回执是直接 execute 和监督器共用门禁。重复恢复、身份错误、暂停漂移不放行。单纯 idle 并不释放桌面 writer。
- `review.rs` 核对当前任务／结果摘要、真实命令、项目目录和逐文件摘要；允许明确选用恢复后的成功命令，保留失败、乱码、截断历史。原生项目归属未读回时仅记 report_reviewed、保持 Reviewing，不标完整交付或发布。

#### 两项真实任务与已知边界

| 项目 | 原生执行聊天 | 实际完成 Run | 结果 |
|---|---|---|---|
| StratusAgent | `01a19116-e379-7631-9303-5cc986055c48` | `run-e593c652a5729665279c0478` | 主版本 1.56.4，HEAD 8eefef3369e354dbdf9ade87d02c534265239858；受限 worker 观察 629 条，实际命令、规则和报告已核对 |
| HoldemTrainer | `01a19117-09d0-7360-a8bd-ad3c8fd7279a` | `run-9519da33305f1e00a4ac5853` | 主版本 0.47.2，HEAD fa6b6579705138144c50356eafa4cc676169f818；受限 worker 观察 162 条，实际命令、规则和报告已核对 |

本阶段日志均在 `.runtime/tests/bridgepilot-stage3/`。`stratus-restored-handoff.log`、`holdem-restored-handoff.log` 是实际监督／模型执行；`stratus-stdio-restore.json`、`holdem-stdio-restore.json` 是数据恢复响应；两个 `*-report-review-receipt.json` 返回 report_reviewed，私有档案 revision=64、两任务仍 Reviewing。完整初始接入和后续原生回执保存在私有操作资料中，不写入公共模板。

两项任务的真实只读 sandbox、cwd、根 AGENTS 来源和终态均有回执；不是伪造夹具或只发 setup 回复。早期桌面 writer／archived 阻断没有发送任务回合，核对后才明确恢复，没有重复已完成任务。真实报告各保留 PowerShell 受限语言／中文回显诊断；Git 全局 ignore 权限警告也保留。宿主前后存在其他项目工作，不能把 worker 条目数与宿主计数当成不变断言，或把差异归因于 BP；`downstream-after-comparison.json` 显示两项目受保护的 VERSION／AGENTS／INSTALL 摘要均未变化，其他 Git 条目变化如实保留。

**桌面归类仍未验证。** 创建时使用各自下游项目，stdio 恢复／执行的 cwd 一直正确；随后两次原生 move 接口分别确认目标项目，但 recent 和同 host/source history 仍返回 projectId=null。不反复 move/unarchive、不改原生数据库、不把移动确认写成读回通过。actions 会列待核对归类；两项报告保持 Reviewing。第二阶段对“stdio 创建后不能移入项目”的判断也已收窄：当时测试 cwd 是工厂临时目录，只能证明该测试来源不安全，不能外推全部 stdio 能力。

独立只读复核发现并闭合一个新风险：直接 execute 可绕过监督器的交接检查，已在入口和认领事务共同检查 chat_ready，并验证缺失／Attempted／Unknown 状态零写入、零 spawn。期限归属、外部独立树存活、错误恢复身份、暂停竞态、命令选择与历史保留、PID 复用保守分支均有定向用例；不存在实际强制复用系统 PID 的测试。没有因包收口扩成全仓业务回归。

整体 BP 保持 active/in_progress，TODO-050 未验收。进程退出后的持续唤醒、稳定桌面归类、真实开发／发布认证、代码检查点和跨机接管尚未交付，不把本批可用的有限时长调度称为完整持续托管。

#### 第三阶段受管收口与最终检查

`develop --apply --packages orchestrator --analysis-file .runtime/bridgepilot-stage3-analysis.json --root .` 最终返回 **development-closed**、退出 0、executed=1／reused=0／git_written=false。分析指纹 `sha256:04b617e674205d999fa987b7a6f9dd77dcc5845de8c093e09fc3d1ad8c163513`，证据为 `development-apply.log` 与真实 `.codex/package-closures.json` 末条（hash `sha256:25694699d4a2e69e3a14280f0c61b9879b699dc5cd52af6a65f93b6df390a4ad`）。唯一升版为 **orchestrator 0.1.1 → 0.2.0**：兼容新增监督调度、原生交接和只读报告核验；没有配置稳定验收合同，保持 0.x。core 保持 10.2.0，主 VERSION 保持 13.0.3，其余包只是 context-only，没有追加升版。

完整登记的 BP 检查包含 **23 项单元＋23 项集成**；`native-check-diagnostic.log` 保存完整 46 项通过，最终候选原生收口也执行同一未删减检查。跨项目并发与局部阻断是受控子进程夹具验证；两个真实模型任务分别成功执行，不宣称已验证真实业务并行发布。独立审查核对代码、真实收据、最终账本／版本和产物，没有替代未完成的桌面或发布验收。

保留一个未查明的验证异常：首次原生检查 exit101 并自动回滚版本，见 `development-apply-failed-01.log`；工厂错误只携带 Cargo stderr，没有失败用例的 stdout，因此无法确认失败位置。完全相同登记命令单独运行时 46 项通过；随后用 `RUST_TEST_NOCAPTURE=1` 取诊断，旧分析被指纹门禁拒绝，见 `development-apply-stale-analysis.log`，按实际环境重新 preview／生成分析后原生检查通过。没有放宽断言、删测试、手写版本或绕过门禁。不能据这次通过认定最初原因已修复；两秒期限和数字 PID 的测试易失性只是独立审读提出的候选，尚未复现，不当作确定根因，也未因此改动断言。

最终 `.build/bridgepilot/cargo/release/bp.exe` 由锁定 Cargo 构建成功，`bp-final-build.log`、`bp-final-self-test.json` 均对应 **0.2.0**。最终真实 ephemeral 协议探测 `bp-0.2.0-real-probe.json` 返回正确 cwd、readOnly、networkAccess=false、实际 model/effort 和两级 AGENTS 来源；没有新增持久业务聊天或模型回合。Forge `runtime-final-check.json` 仍 current/read_only=true、reused=3、built=0，不为 BP 私有更新重复构建 Forge。

最终零写入工程检查均通过：manifest changed=false、factory-version healthy、baseline clean、instruction-source、项目结构及 BP 本地 Skill 元数据。Rust 格式及 Git diff --check 通过；仅有既有配置 CRLF 规范化提示。最终任务预览 `final-development-status.json` 为 **task-development-current**，affected=[]／pending={}，仅对本阶段选择范围有效，不代表全仓 release 就绪。保护的另一交付文档 SHA-256 仍为 `C87CBFAC05B73341457D9382345C1D4601E7C09ABEC11B73E5434670D07CCAD2`，HEAD 保持 `475b04f691e6789e1c5169ae2a4f3e2f795cd621`。

最终 BP 0.2.0 读取私有档案 revision=64；两 Task 为 Reviewing，两次真实任务 Run 为 Completed，早期三个未发起任务的 Run 已核对为 Interrupted，无 Released 或在途执行。actions 明确保留两个只读归类核对项。没有常驻服务、后台业务进程、提交、推送、主版本发布、用户级共享安装或用户验收。17:40 左右在本批可交接状态提前收口，约用 1 小时 35 分，未耗尽三小时；不是完整 BP 开发完成。下一阶段优先稳定归类与持续唤醒，再接开发／发布证据；可日常使用的发布导向首版剩余粗估 3–6 小时，原生归类接口的不确定性可能改变估算，双机验收另计。该估算不授权延长当前阶段或启动业务发布。

### 第四阶段：原生归类、持续唤醒与开发发布核验（2026-10-31）

用户明确授权继续推进，技术方案自行决定；**本阶段不设整体时间上限，每五分钟报告本阶段三个目标的大致完成百分比**，替代此前三小时限制。开始于新加坡时间 17:46:14。重要方向仍交用户判断；不扩大两项真实只读任务的权限，不新建聊天、不修改下游业务、不执行真实提交／推送／发布。百分比不是完整 BP 全计划验收比例。

传播四问：新增 grouping、wakeup、delivery 及私有 Skill 为 BP 产品能力，留在 source/orchestrator；检查登记为自身配置，协议／安装／进展为元文档。需要 orchestrator 兼容新增的 minor 开发收口，主 VERSION 与正式 CHANGELOG 不变；Forge 与公共模板未改，无公共 dogfood 传播。保留所有既有未提交内容和公共 AGENTS／Hook。

#### 本阶段实现与实际观察

- 归类以本机生成 schema 和真实 App Server 为依据：桌面旧项目 ID 与 server ID 不同，按唯一 canonical 单根目录匹配，核对 project/read、thread/read，再核对按项目过滤的 thread/list(useStateDbOnly=true) 成员。先保存 Attempted 再调用 metadata/update；响应丢失只读恢复，不重复未知写入，内部执行另查 owner 进程创建身份。两项真实聊天都读回正确项目成员，没有发送新模型回合或写原生数据库。
- 桌面工具摘要仍 projectId=null；可见侧栏渲染未证明与 server 数据一致，不能从底层归属外推 UI 已刷新。现有原生配置的 legacy-to-server 映射与显式成员归属只读核对过；不把缓存／过滤假设当作已查明根因，不重启影响其他项目的桌面应用。
- Stratus 原有文件证据未变，消费本 Run 的 Observed 归类证明后成为 ReportDelivered。Holdem 原始 VERSION 被其他工作改变，report-review 按“project evidence changed since report review”拒绝；保留原摘要和 Reviewing，不重跑模型或改摘要冒认旧报告仍有效。
- 原生 heartbeat 附着当前 BP 对话，五分钟周期，首次先登记 Paused。wake-plan／attempt／bind 保存来源、精确目标及提示词，并只读验证实际 automation.toml；每次 wake-check 重新核对，旧 Active 不能覆盖原生暂停／漂移。重复调用不给第二份创建动作；范围更新先暂停，再使用相同 native ID。机器与控制对话固定，不替用户授予执行／发布权限。
- development-probe／verify-development 消费已有受管任务范围收口，两次原生预览之间核对 Git／合同／账本／工具摘要。verify-publication 先查本任务发布权限，读取同一份真实收据字节，再核对固定提交、版本、包覆盖、账本及实际 push URL 的 live 远端。fetch 与 push URL 不同也按实际推送端点核验；保留无关脏文件。证据摘要不构成加密身份认证。

#### 验证范围与收尾状态

阶段日志统一在 `.runtime/tests/bridgepilot-stage4/`。最终候选初次完整 BP 检查 **23 单元＋32 集成，共 55 项通过**；后续发布端点修正的 2 项、项目成员读回的 4 项均是同一完整集合的受影响子集复验，不相加冒称 61 项。覆盖归类歧义／丢响应／暂停／进程身份，心跳登记重放／原生配置漂移／暂停后同 ID 更新，以及只读任务禁止发布、双 bare 仓库真实 push 端点区别。发布收据生产器与 develop 输出在这些场景使用受控样本，不等于真实业务已发布。

独立只读审查指出并已闭合：PID 复用身份、重复心跳创建动作、范围更新同 ID、收据校验解析双读、fetch URL 冒充 push URL 等风险。审查未代替真实触发、桌面显示或业务发布验证。受管开发收口、最终产物、BP 自身实际原生开发探测及心跳激活／首个触发将在取得真实结果后续记；目前不能据 Paused 配置称持续唤醒通过。

整体话题保持 active/in_progress、TODO-050 未验收。跨机检查点和接管协议、双项目真实业务发布仍未实现／验收。保护的无关交付文档和 Git HEAD 继续按阶段基线核对，不记录用户验收或自动发布。

#### 第四阶段受管收口、产物与原生接续检查点

实际 `develop --apply --packages orchestrator --analysis-file .runtime/bridgepilot-stage4-analysis.json --root .` 返回 **development-closed**、退出 0、executed=1／reused=0／git_written=false，见 `development-apply.log`。本次只升级 **orchestrator 0.2.0 → 0.3.0**，原生检查运行完整未删减的 55 项登记集合；Forge 包均 context-only，core 仍 10.2.0，主 VERSION 仍 13.0.3。未登记稳定验收合同，保持 0.x，不称稳定版。分析指纹 `sha256:719e46439ae818315d9c9b886278b24815da85d5e923b317b3623985e9c6b2c6`，真实账本末条 hash `sha256:1a20106838ae7bbd999c3fbc1912377bd74c189b0c3ef4b16033a0e09a1b1af8`，不手改收据或版本。

最终 `.build/bridgepilot/cargo/release/bp.exe` 锁定离线 release 构建成功，`bp-final-build.log` 和 `bp-final-self-test.json` 对应 **0.3.0**。实际 `development-probe` 使用本仓库已有工厂工具连续两次原生预览，返回 **development_verified**；当前包版本 0.3.0、task-development-current、affected=[]／pending={}，见 `bp-native-development-probe.json`。该实测验证开发探测能力，不把当前控制对话冒充下游写任务或发布成功。

最终 `final-checks.json` 九项退出码均 0：Forge runtime current/read_only=true/reused=3/built=0、manifest changed=false、factory-version healthy、baseline、instruction-source、project-structure、BP 本地 Skill 元数据、BP 包格式和 Git diff --check。检查没有修复或清理其他既有脏文件。额外全 workspace 格式检查观察到 Forge 历史未提交文件的格式差异；它不是 BP 包格式检查失败，也未获本轮改写授权，不将其记录为通过。

新加坡时间约 **18:53:55**，已通过原生 automation_update 把既有 **bridgepilot** heartbeat 从 Paused 改为 Active，未重复创建、未新建对话。`wakeup-native-active-binding.json` 是实际客户端回执绑定结果；`wakeup-active-check.json` 只读核对真实配置、五分钟周期、控制对话 `01a18d4c-bd2a-7d83-8cdf-89b322580637` 和本机，档案 revision=78。原生配置摘要 `42bfdeef8f32fa09effd98c60f16244c381dcc6a7b4de35d0bf32e38d0288244`。尚无独立的实际定时送达证据，**不能用 Active 或手动 wake-check 冒充首个触发通过**。

接续只需等待同一原生心跳送达本控制对话，先运行 wake-check，再记录真实触发来源和状态。当前两项真实任务仍是 Stratus ReportDelivered／Holdem Reviewing，无 Unknown 或在途执行，无 Released；Holdem 旧报告因文件漂移不自动重派发。其他项目无已登记 Ready 工作。桌面侧栏可见缓存刷新、真实业务发布和跨机接管仍未验证，不能为了补证擅自启动业务任务、重启桌面或写原生存储。目标完成后心跳保留持续托管，空闲安静；开发未完成时继续按五分钟报告粗进度。

#### 首次原生心跳实际送达（2026-10-31 18:59）

本控制对话实际收到系统生成的 heartbeat 消息，automation_id=bridgepilot，触发时间 **2026-10-31T10:59:06.595Z**（新加坡时间 18:59:06.595）。它在上一回合结束后自动接续，没有用户发送“继续”。控制 Agent 按提示读取当前 AGENTS／私有 BP Skill，并用最终 0.3.0 程序执行 wake-check；实际返回 wakeup_checked、原生 ACTIVE、同一配置摘要／机器／控制对话，档案仍 revision=78，无 Unknown 或在途执行。`first-heartbeat-wake-check.json` 保存本次实际命令输出；触发来源是本对话收到的系统 heartbeat，不把 Agent 自写观察描述当作服务端签名收据。

持续唤醒的首次实际送达与门禁接续已通过；仅证明电脑和当前桌面应用在线时的这次触发，不外推关闭应用、重启、双机或繁忙回合的调度保证。没有再次执行模型任务、修改业务文件、发布或重复登记自动化。

本次桌面对照查询仍返回两项验证聊天 projectId=null；同项目现有活跃普通聊天“梳理项目与Forge红线重复项”也返回 null，而另一些普通聊天返回旧桌面项目 ID，见 `first-heartbeat-desktop-list.json`。因此该摘要字段存在不同表现，不能单靠 null 证明底层归类失效；也不能据此断言 UI 已正确显示。已有底层项目／线程／项目成员三类真实读回继续有效。当前约 **98%**，剩余是桌面可见归类的一致性核查；暂无已验证的无干扰刷新接口，不通过重复 move、改原生存储或重启桌面强行补证。继续保留本对话心跳承担已授权持续托管与后续状态观察，不将此处写作用户验收或 BP 全计划完成。

#### 第二次心跳：客户端归档隐藏状态与实际侧栏修复

系统再次于 **2026-10-31T11:04:06.643Z** 唤醒，wake-check 仍通过。只读核对本机 Codex **26.1027.3384.0** 的相关已安装客户端代码，确认 shouldSurfaceThreadSummary 会排除 isConversationSuppressed；后者读取客户端 archiveStateByConversationId。客户端 unarchiveConversation 路径在 thread/unarchive 成功后还执行恢复确认、加载和广播；BP stdio 的持久记录恢复未经过该客户端路径。没有修改安装文件、原生数据库或用户设置。

在两个 Run 已 Completed、owner/server 已退出、无活跃／Unknown 归属的条件下，先对 Stratus 执行原生客户端 unarchive，实际报 **no archived rollout found**：持久记录此前已经恢复，不能直接再次 unarchive。保留失败，不重复调用。随后使用原生客户端对该已结束聊天进行一次明确的 archive(true)→archive(false)，两次返回匹配 thread ID 和预期 archived 状态。Holdem 同样完成一次客户端周期；全程没有新建聊天或发送模型回合。实际客户端回执统一保存于 `client-return-native-receipts.json`。

修复后原生 list_threads 的两项 projectId 分别为 Stratus `7dcda9a8-eb9c-477e-aa4e-1f865299bf0c`、Holdem `3abb1daf-5273-45ef-9734-cc4672204dab`，cwd／thread ID 均匹配。只读截图实际看到 **BP 只读验证 · StratusAgent** 和 **BP 只读验证 · HoldemTrainer** 位于各自项目侧栏；不是仅凭移动确认宣称 UI 通过。无障碍文本当时仍返回旧树，故视觉结论以实际截图为证。之前“尚未验证”属于历史检查点，本段补齐这一缺口。

传播判断：只修正私有 BP Skill 的任务结束后客户端交接流程，及本仓库对应说明，不改 Forge／公共模板／Hook；属于 BP 产品层兼容修复，计划由受管 develop 将 orchestrator 0.3.0 升 patch。增加明确的终态、进程退出、无其他在途／Unknown、先查成功回执、未知不重放门禁；显示交接由当前 Codex Agent 使用原生客户端处理，没有另建 GUI 执行器。两项真实聊天已验证同一流程，无需增加镜像实现的单元测试；保留登记的完整 BP 检查用于最终候选收口。当前约 **99%**，待该补丁受管收口、最终产物与记录核对。业务发布和跨机验证不因该修复而补做或冒认通过。

#### 收口阻断诊断与调度稳定性修复

客户端工作流补丁首次收口失败，`client-return-development-apply.log` 保存原始 exit101 和自动版本回滚。具体是 scheduler_cases::supervisor_runs_two_projects_and_rejects_a_second_controller 在八秒内未同时观察到两个 Running。没有删测试或放宽时限；单独运行相同精确用例时 1.35 秒通过，见 `client-return-supervisor-diagnostic.log`，该通过不证明原失败原因已排除。

随后独立只读审查确认两个实际缺口：

1. grouping 在原生 RPC 前对共享档案用单次旧 revision CAS；另一个合法项目提交或短暂占锁时，该步骤直接失败，执行器会把当前 Run 置 Unknown。修复仅让本地归类意图使用既有 Store.update 有界重试，闭包每次重核当前身份与暂停并复用既有操作；不重试已发送原生动作，不改变 reconcile 的严格终态 CAS。
2. scheduler 每 200ms 等待既有 prepare_execution_chat 时仍追加无变化修订。改为只有该操作不存在才创建，其余 tick 只读等待，减少档案增长及无关争用。

新增受控屏障测试让两个项目共享同一观察修订后并发提交意图；修复前实际出现 writer lock held／一方失败，修复后双方成功并保留各自唯一操作，随后暂停仍零写入拒绝。另一个回归等待已有交接操作一秒，修复前 revision 从 2 增到 7，修复后保持 2。两项修复前／后日志分别是 `concurrent-intent-regression-before.log` 和 `concurrent-intent-regression-after.log`。这证明缺口和修复，不冒认已经还原最初失败的唯一具体交错。

最终新增两项后，完整 BP 登记集合为 **24 单元＋33 集成，共 57 项**；是原 55 项加两项回归，不累计重复执行次数。`stabilized-development-apply.log` 返回 development-closed、executed=1／reused=0／git_written=false，受管将 orchestrator **0.3.0 → 0.3.1**，账本 hash `sha256:dba78b9c32c2e70a0f553729f98bb663340f63723805a0d847b7d843db4af27e`。独立审查确认两项缺口闭合，没有改变未知操作门禁。

随后目录核对发现本次新增内联回归测试使用 current_dir 建临时目录；Cargo 的实际测试工作目录是包目录，因此误留下 source/orchestrator/.runtime 下的空目录。已改用 CARGO_MANIFEST_DIR 定位仓库根 .runtime/tests/bp-native-intent，检查无额外内容和 reparse 后逐层非递归移除本次三个空目录；未删除任何已有资料或测试日志。该测试源码变化按原生预览确认为待收口，不手改旧记录或复用旧指纹；最终收口和程序结果接下文。

#### 第四阶段最终交付（2026-10-31 19:31）

本轮“桌面归类、持续唤醒、开发发布核验”三个约定目标已完成，**本轮进度 100%**。这不是 BP 全计划完成、用户验收或真实业务发布授权；TODO-050 与整体话题仍保持 active/in_progress。后续原生心跳保留既有持续托管用途，先核对门禁，只推进已登记且授权的工作；没有可执行工作或状态变化时安静，不反复重查已解决问题、不自动开始跨机等下一开发阶段。

最终 `develop --apply --packages orchestrator --analysis-file .runtime/bridgepilot-stage4-finalized-analysis.json --root .` 返回 **development-closed**、退出 0、executed=1／reused=0／git_written=false；完整 **24 单元＋33 集成，57 项检查**通过。证据 `finalized-development-apply.log`，分析指纹 `sha256:653dedbe44a76c475c10c8e32f2d13bb9520703db0260b93f63165e9bcd6fc94`，最终账本 hash `sha256:587f65cbbf78f15c4362159c31f8cbd7d8429f23390310491a7cbb0355303f25`。原始失败、回滚及回归前后证据全部保留，不将多次运行相加为更多独立用例。

| 包／程序 | 本阶段起点 → 最终 | 原因 |
|---|---|---|
| orchestrator | 0.2.0 → **0.3.2** | 0.3.0 兼容新增归类／心跳／证据核验；0.3.1 修复客户端显示交接和并发／空等待；0.3.2 修正回归测试产物定位 |
| core | 10.2.0 → 10.2.0 | 未修改 Forge 实现 |
| 主程序 VERSION | 13.0.3 → 13.0.3 | 本轮没有 release |

最终 `.build/bridgepilot/cargo/release/bp.exe` 由锁定 Cargo 离线 release 构建成功，`bp-0.3.2-build.log`、`bp-0.3.2-self-test.json` 对应 **0.3.2**。该最终程序的真实 `development-probe` 连续两次原生预览后返回 **development_verified**，package_versions.orchestrator=0.3.2、task-development-current、affected=[]／pending={}，见 `bp-0.3.2-native-development-probe.json`。没有用旧版探测代替，也没有把控制对话伪装成下游写任务。

`completed-final-checks.json` 九项工程检查均退出 0，Forge runtime 仍 current/read_only=true/reused=3/built=0，没有重复准备共享工具。项目结构通过，新增回归产物实际进入根 .runtime/tests，源码目录 .runtime 不存在。独立只读复核确认归类本地意图重试边界、只读等待、客户端真实回执、最终版本／账本／产物与路径一致；未替代真实业务发布验证。

最终真实环境证据：两验证聊天在各自下游项目侧栏可见，客户端列表 projectId、底层 server 项目和成员均匹配；本对话至少两次实际定时唤醒及 wake-check 接续通过。BP 私有档案 revision=78，Stratus ReportDelivered，Holdem 因原始 VERSION 变化保持 Reviewing；无 Unknown、在途执行或 Released。后台状态核验不能授予新业务或发布权限。

本阶段未提交、推送、发布、变更主版本、公共模板／Hook、用户级共享安装或原生 Memory。HEAD 继续为 `475b04f691e6789e1c5169ae2a4f3e2f795cd621`，另一交付文档保护摘要仍为阶段基线。真实业务发布、关闭应用／重启后的唤醒、真实开发暂停／恢复和跨机接管仍不在本轮已验证范围，后续按相应任务推进。

### 第五阶段：换机交接、私有档案同步与安装升级（2026-10-31）

用户明确要求继续做完三项，并回答：**“选 A，新建独立的 GitHub 私有仓库 BridgePilotData。继续推进，其他技术细节你来决定，重要方向再找我确认。”** 因此采用 freakybridge/BridgePilotData 作为独立个人档案远端；产品代码仍在现有 BridgeForgeCodex，项目源码仍使用各自仓库。本阶段包含该私有数据仓库的创建、必要初始化和档案同步，不包含 BP 产品正式发布、产品源码提交／推送或任意下游混杂工作树提交。延续用户无整体时间硬上限、预算由用户关注的约定；token 未实测。五分钟定时报告及 heartbeat 继续 Paused，不自行恢复。

规模 L，主对话负责实现、集成、受管操作和交付；跨机迁移及高风险一致性需要独立只读审查。唯一任务记录继续复用本文。开始于新加坡时间约 20:11；基线 orchestrator 0.3.2、私有档案 revision=79、五项目、两个只读任务，无在途／Unknown；Git dirty=294、HEAD 475b04f691e6789e1c5169ae2a4f3e2f795cd621，保护文档摘要与第四阶段一致。原生预览为 task-development-current，证据 .runtime/tests/bridgepilot-stage5/baseline-development-preview.json。

传播四问：同步、换机和安装属于 BP 私有产品能力，进入 source/orchestrator，不进入 Forge 公共八包投影／模板；版本检查登记属于自身配置，协议／安装／交付记录属于元文档。按实际受影响包完成受管开发收口，主程序及正式 CHANGELOG 不预写；现有 Forge 安装及共享入口保留，原生 Memory 不解析、不改写。

#### 实施分段与完成条件

1. **私有同步与归属**：只同步经过完整链校验的任务档案及必要说明，排除本机运行／依赖／构建目录和原生会话。使用明确私有远端与 ref；远端分叉拒绝，不 force、不自动合并。同步记录真实提交／远端读回，丢响应先查询不重推。兼容旧档案，新增单活动控制机器的归属门禁。
2. **显式交接**：旧机停止相关执行、确认无活跃／Unknown、核对代码检查点和远端提交，再冻结并提交交接。新机先核对完整档案及实际代码，成功取得远端归属后才允许本地激活。旧机不能凭心跳消失或超时抢回；新机不继承旧本机配置、进程和聊天绑定，不自动执行任务或恢复暂停的心跳。
3. **安装升级**：BP 自有包包含实际受管二进制、私有入口及说明，支持离线准备／安装／升级／失败恢复；独立安装位置按三目录管理，核对实际版本和内容摘要，保留可用旧版。现有 Forge 入口继续兼容，不重命名远端或覆盖旧配置；产品运行不依赖测试工程或临时诊断脚本。
4. **验证与交付**：使用真实隔离 Git bare 远端覆盖同步、交接、分叉、丢响应、旧机拒绝与恢复；使用两个本机隔离工作区模拟两机器，区分它与物理双机验证。真实 GitHub 私有库至少完成首次受管同步和只读取回校验。隔离安装验证首次／升级／配置保留及失败恢复；必要独立审查、受管收口、最终程序和文档检查完成后交付，不冒称物理第二台电脑已验证。

实际写入仅限当前 BP 产品代码、本机明确私有工作区、自己的安装测试目录及已授权 BridgePilotData 远端。真实下游代码检查点与发布不以该阶段代替项目授权；不为了测试而提交当前294项混杂改动。需要实际换机时，代码检查点由项目受管流程生成，本轮先实现并验证交接合同。

#### 本阶段实际交付

三项能力已经实现并完成本机交付：私有档案同步、明确控制权交接、独立安装／升级／恢复。核心分别为 `source/orchestrator/sync.rs`、`handoff.rs`、`install.rs`，`launcher.rs` 提供稳定 Rust 入口；只依赖现有 Forge core，未进入公共八包投影或共享 Skill。操作的唯一说明仍为根 [INSTALL.md](../../../INSTALL.md#bridgepilot-本地试用与档案恢复)，本地 Skill 同时支持源码入口和受核验安装 home。

同步同时检查 Git 与应用档案祖先；只允许 HEAD 和完整不可变修订，拒绝分叉、回退、未知文件及非普通文件。publish 先保存 pending，未知结果只查询；显式 fence 使用兄弟 Git 提交，保留 superseded 候选以核销迟到的原 push。取消候选准备完成前不删除唯一恢复记录。accept 先远端确认归属再激活本地；接受不自动恢复项目／heartbeat。新机按实际代码检查点和原生项目重新绑定，接续保留旧 Run、结果、确认及阶段，新 Run 使用本机与新聊天。

独立安装包包含真实 core、稳定 launcher、私有 Skill、控制 AGENTS／README／INSTALL 六项固定资产，登记 id、target、hash。版本目录不可变，最后切换 current；持锁后及覆盖前复核旧字节，不覆盖用户编辑。失败恢复日志保留原字节，回退先由旧程序读取真实档案，不回滚数据伪装兼容。安装不修改 PATH、Forge 既有安装、公共模板、共享 Skills 或登录凭据。

#### 异常验证与真实旧记录

独立迁移复核发现并修正：取消 pending 的崩溃窗口、旧终态缺 birth 的兼容问题、安装读取旧字节后发生外部编辑的覆盖窗口，以及检查点查询远端期间可能发生代码漂移。修正后增加前后／发布前复核；独立 Agent 确认严重项闭合，未代替主对话重跑测试。

真实首次 enrol 被旧进程核验阻止时，档案仍为 revision=79，没有上传或更改旧 Run。旧 `run-722247cfaeabbde1653628fb` 已在任务派发前通过真实原生历史核销，owner 39148 不存在，server PID 27772 却被 `nvcontainer.exe` 复用。没有杀外部进程、清档案或手补 birth。原始 Codex 启动调用从本控制对话的实际 tool input 精确提取，UTF-8 SHA256=`725f134a3a1a4cd90cb8a631d33c0d261dd4ae2386f7e6ed814c7033e2391e56`，并关联当时 supervisor 的 Run／owner 和原始核销收据。

`legacy-server-probe` 使用原生 Toolhelp 快照读取受保护进程的 ImageName，核对原 owner 不存在、原 Run 已核销且未派发任务、原始启动和核销来源、同一原生聊天全部终态。proof 只保存在本机 .runtime，绑定机器／archive／Run／resume 操作／server PID 与证据摘要；消费时重核，不豁免 Unknown、owner 或已派发任务。真实核验输出 `actual-legacy-server-proof.json` 证明当前镜像为 nvcontainer.exe、档案仍79、未启动模型。新 reconcile 同时保留既有进程身份。首次 enrol 与真正 release 分开：前者登记备份控制机，只要求无活跃／Unknown；后者仍执行严格停止核验。

#### 版本、检查与安装证据

| 项目 | 实际结果 |
|---|---|
| orchestrator | **0.3.2 → 0.4.0 → 0.4.1**；minor 新增三项能力，patch 修正真实旧记录兼容和保留身份 |
| 最终受管收口 | `legacy-development-apply.log` 为 development-closed，executed=1／reused=0／git_written=false；实际完整包集合 **25 单元＋43 集成，共68项** |
| 最终分析指纹／账本 | `sha256:aa1e4ef8889f1e08418c8866d023a8e5f65c8a3bfcbdd40fee25f5ccc48cefe7`／末条 `sha256:029e01631204540c6320808bac346851539e5cedb6f6e98de2dfcc7df2021e39` |
| 真实隔离 Git | `portable-reviewed-tests.log` 九项通过，最终 `legacy-boundary-tests.log` 十项通过；是同一集合增补，不累计重复次数 |
| 异常覆盖 | 应用分叉、公共库拒绝、竞争接管、丢响应只查询、未发送首次同步的 fence、取消中断、迟到旧 push 拒绝、脏／未推送检查点、查询期间编辑、旧进程退出与有来源的 PID 复用、用户编辑保护 |
| 安装恢复 | 内联 recovery 测试验证部分安装与外部编辑保留；真实 0.3.2↔0.4.0 升级／兼容回退及新档案拒绝旧 reader，通过 `actual-install/summary.json` 记录 |
| 最终升级 | `final-existing-home-upgrade-summary.json`：0.4.0→0.4.1 成功，旧稳定 launcher 字节不变且激活新 core，读取实际 custody 档案 |
| 最终构建／包 | `bp-0.4.1-build.log` 锁定 Cargo 离线 release 成功；`final-bundle.json` 为0.4.1、六资产、archive_included=false；当前包 `.build/bridgepilot/portable/current` |
| 本机正式安装位置 | `C:\Users\bridg\Documents\Codex\BridgePilot`；`personal-install-summary.json` 核对稳定 launcher、0.4.1、原工作区与控制机均匹配 |
| 主程序／Forge | VERSION **13.0.3**、core **10.2.0** 保持；没有产品源码提交／推送／正式发布；稳定验收合同尚未配置，不声称1.0 |

最终 BP 的原生 development-probe 核对 task-development-current、orchestrator=0.4.1、affected=[]／pending={}，原始输出 `final-native-development-probe.json`。所有本阶段日志均在 `.runtime/tests/bridgepilot-stage5/`；工程边界／链接／保护基线检查另由该目录 final-checks 与 final-records 原始结果记录。不从任务范围 current 推导全仓 release 就绪。

#### 已配置的真实私有数据与使用状态

GitHub [freakybridge/BridgePilotData](https://github.com/freakybridge/BridgePilotData) 已实际创建，API 读回 private=true、main。最终 BP 首次同步提交为 **`32182ddae6eb966fe480588dec91c3991e6ff74f`**，`actual-sync-upload.json` 记录 remote_verified=true。独立 GitHub tree API 核对 **82 个普通归档文件、零其他文件**，没有 .runtime、本机配置、源码、凭据或原生 Memory；证据 `github-independent-summary.json`。

实际档案 `archive-df1b25cda65f9bf5a0ff8c35` 从79仅新增 enrol 修订到 **80**，owner 仍为 `machine-b8aeb244a33d208bdbfdcfe4`。在独立新机器身份工作区从真实 GitHub 下载完整历史，HEAD 字节 SHA256=`BC64671DEB8096430687B28E7ECFE973BEBCFE578DABE27513ED808FD888D030` 与源相同；观察工作区尝试上传被 owner 门禁拒绝，没有激活、执行或复制本机身份。记录 `actual-github-roundtrip-summary.json`。五项目、两项原有只读任务及旧 Run 保持；Stratus 为 ReportDelivered，Holdem 仍因原 VERSION 漂移保持 Reviewing，没有冒认重新验收。

日常仍在 Codex 给项目目标；空闲边界由 BP 同步私有档案。需要真正换机时，先明确全部暂停和各项目代码保存权限，再按 INSTALL 的检查点／release／accept／continue 流程执行。用户要求暂停的五分钟 heartbeat 及报告保持 **PAUSED**；没有暂停项目任务，也没有偷偷恢复唤醒。新机不从档案复制旧机 heartbeat 或进程。

本阶段功能和本机交付完成；**两台物理电脑、真实业务代码的换机保存／接续、关闭应用或重启后的唤醒、真实双项目发布仍未验证**。它们需要相应设备与实际任务授权，不作为本轮隔离 Git／GitHub 往返／本机安装已经通过的替代说法。唯一总计划及 TODO-050 仍为整体进行中，未执行用户验收或正式 release。

### 第六阶段：完整项目更名（2026-11-01）

用户经 confirm 明确采用 **BridgePilot**，授权产品名称、GitHub 仓库、本地目录与 Codex 项目一起更名；保留既有改动，下游适配之后逐一处理，本轮不提交、推送或发布产品代码。内部 Forge 工具、包和稳定资产标识保留其子系统职责，不批量改写历史记录。

GitHub 已由 `freakybridge/BridgeForgeCodex` 更名为 [freakybridge/BridgePilot](https://github.com/freakybridge/BridgePilot)，不变仓库 ID 为 `1225826781`，public／main 保持。origin 更新为新 URL；本地源码迁至 `D:\Quant\BridgePilot`，原 Git HEAD `475b04f691e6789e1c5169ae2a4f3e2f795cd621` 保持。迁移使用不覆盖的移动并保存日志，所有文件已迁出旧根；修改前294项脏记录保留，新增差异另由本轮更名产生。没有对迁移前未跟踪源码生成完整逐文件摘要，不把移动日志冒充该项证据。

Codex 项目编辑器已将唯一源文件夹切换到新目录；实际 `list_projects` 读回项目 ID `9976788d-3e4d-4323-b8b8-dd562afeb76c`、名称 BridgePilot、路径 `D:\Quant\BridgePilot`。真实 stdio `project/list` 同时读回原 server 项目 ID `01a1358c-a35c-7c00-b613-6c2ce9563955` 及新名称／新根，没有新建替代项目或聊天、改写原生数据库或 Memory。旧空根 `D:\Quant\BridgeForge` 的非递归删除因进程占用失败，保持为空；不强杀进程、不创建链接或兼容副本。

#### 更名绑定与本机交付

原 `project-add` 禁止任何 repository URL 变化，本轮新增真正更名的核验：仅接受同 GitHub owner，显式指定 `--hostname github.com`，读取旧／新 URL 的实际 API 响应，要求同一非零不变 ID 且 canonical name 一致；仍以档案 CAS、活跃／Unknown 门禁保护绑定，历史 Run 不重写。相同 URL 的普通登记不增加网络调用，其他仓库替换仍拒绝。

| 项目 | 实际结果与证据 |
|---|---|
| 总管版本与受管收口 | orchestrator **0.4.1 → 0.5.0**，兼容新增更名核验；`development-apply.log` 为 `development-closed`，登记完整总管检查 executed=1／reused=0／git_written=false；账本末条 hash `sha256:90e8741943a6b43aa953d2ab857b90bc896b1840641d8c40a4aba61ec1a0828f` |
| Forge 与主程序 | 实际预览仅总管内容待处理；factory 保持 **0.10.1**，主程序保持 **13.0.3**，没有重复升版或 release |
| 分发与工程检查 | `distribution-check.log` 的4项 shared 分发回归通过；`final-checks.json` 的工具准备只读检查、manifest、factory-version、baseline、instruction-source、project-structure、Rust格式、Git差异空白检查均退出0 |
| 最终构建与安装 | 锁定 Cargo 离线 release 构建通过；原 home `C:\Users\bridg\Documents\Codex\BridgePilot` 经 preview／install 升级至0.5.0，旧0.4.1及稳定 launcher 保留；配置与私有工作区不变 |
| 安装包 | 当前 `.build/bridgepilot/portable/current` 为0.5.0，旧包保存在 `.build/bridgepilot/portable/preserved-0.4.1`；不包含私有档案 |
| BP 档案 | 真正 `project-add` 返回同 GitHub ID核验，revision80→81，项目根／仓库更新；原项目、两任务、Run与确认保留 |
| 原生心跳 | `wake-revise`／`wake-plan`／`wake-attempt` 后由实际 `automation_update` 更新原 `bridgepilot`，再 `wake-bind` 至 revision85；新提示词引用新根，仍为 **PAUSED**，不恢复五分钟报告 |
| 私有数据同步 | `sync-upload.json`：revision85，`remote_verified=true`，BridgePilotData 提交 `697776d22f805abbccc3faf7cb04678fd016f3d3`；仅同步私有档案，不是产品代码推送 |
| 原生开发核验 | `development-probe.json` 为 `development_verified`，原生受管预览 `task-development-current`，factory／orchestrator 范围 affected=[]、pending={}；不据此推导全仓 release 就绪 |

本阶段原始执行记录统一位于 `.runtime/tests/bridgepilot-rename/`。README、INSTALL、文档索引与自身项目级说明已更新产品名称；公共安装源由受管 manifest 同步至新仓库地址。公共模板仍为 Forge 子系统规则，本轮未写真实下游项目、未更新用户级共享安装。更名后的下游入口修复、实际业务发布及前阶段物理双机／自然唤醒验证继续保持各自边界；本阶段不自动进行用户验收。

#### 用户验收与保留项（2026-11-01）

用户在本控制对话明确调用 `$summary accept`，接受本轮已交付的完整项目更名成果，同时调用 `$git-sync release` 授权当前 BP 累计开发成果的受管提交、推送和正式发布。实际发布结果以随后的受管回执为准，本段不预先宣称已发布。

本次验收依据为第六阶段记录的同一仓库身份、原生项目与新源目录、BP0.5.0实际安装、revision85私有档案同步、受管开发收口、工程检查及独立复核。经验补充：Codex 项目源目录更改后，既有聊天的 `cwd` 不一定随之更新；实际 `read_thread` 仍返回本控制聊天 `D:\Quant\BridgeForge`。本轮命令均显式指定新根执行，未改写原生历史；当前聊天上下文迁移与被占用的旧空目录仍未收尾。后续需使用受支持的聊天目录设置处理，不以改数据库、历史文件、链接或强杀进程绕过。

**TODO-050 继续为进行中**：本次验收仅确认已有交付，不把当前聊天目录收尾、原计划要求的真实双项目发布／物理换机续接与自然唤醒等缺失证据标为完成；下游更名适配按用户决定另外逐一处理。既有阶段事实与未验证边界保留，整体 `lifecycle: active`／`validation_status: in_progress` 不变。

### 第七阶段：GitHub 新机引导（2026-11-01）

用户要求新电脑只在 Codex 说一句话，即可从 GitHub 下载 BP、完成安装并恢复私有档案，免去从旧机复制便携包。核验后确认现有0.5.0的 init／sync-configure／sync-download／install 已覆盖内核行为；本次补齐正式发布资产与 Codex 冷启动说明，不新增后台执行器、预算管理、自动接管或新的归档格式。

安装与恢复的唯一维护源仍为根 [INSTALL.md](../../../INSTALL.md#从-github-在新电脑安装并恢复)，README 仅导航。新机无需已有 BP Skill 或 Rust，Codex 从产品仓库的正式 Release 说明开始；必要账号登录与平台权限仍由本人完成。暂存位于私有工作区 `.runtime/download/bridgepilot/<release-id>/`，保持安装 home 的首次归属检查成立。固定正式 tag／提交、真实资产 id／size／digest 后，先校验 manifest 与六项固定 payload，再运行候选 core；通过私有恢复后才 preview／install。重复安装、已有配置、pending 与控制权仍沿已有受管边界处理。

已为正式发布的13.1.0提交 `b447e6954e59244d08e37cdc9d396ca8718767fd` 补齐 [GitHub Release v13.1.0](https://github.com/freakybridge/BridgePilot/releases/tag/v13.1.0)，实际 Release ID=`430059590`。tag API 解析回同一固定提交；非 draft／非 prerelease，latest 可读。安装说明自动取自 INSTALL 的引导段。公开资产恰为 package.json 与六个 payload，均来自此前已验证的0.5.0便携包；没有上传个人档案、本机配置、日志、凭据或原生会话／Memory，也没有提交或推送本轮新增的源码文档。

#### 实际验证与边界

- 本机现有包的受管预览为 `task-development-current`、affected=[]，核心源码和版本未变，不再次升版或重跑整包开发检查。
- 先用同一包在隔离身份下安装并从真实 BridgePilotData 下载，确认既有组合入口成立；随后重新从真实 GitHub latest Release 下载全部七资产，逐个核对 API digest／size、manifest SHA256和源制品摘要，再用这批网络下载文件执行全流程。
- 网络全流程的 init、sync-configure、sync-download、install-preview、install、installed self-test、install-status 和档案 status 均成功，实际 core=0.5.0、archive=`archive-df1b25cda65f9bf5a0ff8c35`、revision=85、项目数5；新测试机器身份与原 owner 不同，未取得控制权，未上传数据或执行任务。
- 原始收据位于 `.runtime/tests/bridgepilot-github-bootstrap/`，其中 `github-release.json`／`github-tag.json` 为实际原生响应，`github-verification.json` 汇总实际网络安装恢复，逐阶段输出分别保留。一次测试命令因 PowerShell 只读 HOME 变量命名冲突在下载／安装前停止，修正为独立任务变量后继续，没有更改系统变量或产品实现。
- 上述证明本机隔离身份下的真实网络引导；**物理第二台电脑、缺依赖时的实际安装及交互登录仍未验证**。下载档案不授权接管、恢复 heartbeat、创建聊天或推进项目；真正换机继续按既有全部暂停、代码检查点与 release／accept／continue 流程。既有 TODO-050 状态与第六阶段未完成边界保持。

### 第八阶段需求确认：扩展 BP 统一 Skill（2026-11-01）

关联原需求继续为 **TODO-050**，本节接续已有换机交接与 GitHub 新机引导，不另建重复需求卡。用户于2026-11-01明确“开始开发吧”，本阶段已获准实施、必要隔离验证与受管开发收口；不因此操作真实项目交接或提交／推送／发布新产品代码。整体计划及TODO不因开工而收口。

#### 用户目标与已确认决定

用户最初要求用归属于 BP 的 Skill 完成旧电脑暂停及上传进度、新电脑同步恢复；Codex 视为已安装，Git／GitHub CLI 不满足要求时支持静默安装，并接入第七阶段已有 GitHub 一句话引导。随后补充首次初始化／存档库绑定及下游项目登记，并明确选择 A：扩展现有 `bridgepilot` Skill 作为统一入口，不另建独立管理或换机 Skill。原 bp-handoff 仅为前期拟议名称，尚未实施，无已安装入口需要兼容迁移。

1. **旧机选择 A。** 显式调用拟议的 `$bp-handoff prepare all` 表示全部暂停，并授权核对实际改动范围后，将已确认开发任务的进度保存、推送至专用工作分支；不发布正式版。遇到冲突、范围不明或权限缺口才找用户，不默认提交其他未确认改动。仅“我要换机”等模糊表达仍不能推导全部暂停。
2. **新机选择 A。** 调用拟议的 `$bp-handoff restore` 时，核对旧机已释放、远端交接记录、准确代码检查点及本机项目绑定后自动接管控制权；任务与定时唤醒仍保持暂停，不自动开始开发。先前“独立安装只恢复、不接管”的边界继续保留；本次显式 restore 才包含上述接管授权。
3. **首次存档库选择 A。** 仅对没有任何 BP 配置的全新用户，init 首次向本人确认一次私有存档库：推荐当前登录 GitHub 账号下的 BridgePilotData，也允许指定已有库；确认后验证并保存绑定，后续自动复用。推荐位置不是创建或覆盖授权，确认前不创建远端库或上传数据；已有配置不重新询问、不静默更换目标。已有远端档案先按实际身份与控制权核对，不把它作为空白首次档案。

以上两项已确认行为继续有效，统一入口下分别对应拟议的 `$bridgepilot handoff prepare all` 与 `$bridgepilot handoff restore`；只改变入口组织，不重新解释或扩大操作授权。首次使用提示词已按用户明确要求加入 README 的快速开始，尚未提交或推送。

#### 拟议入口与职责

| 入口 | 行为 |
|---|---|
| `$bridgepilot init` | 首次初始化个人工作区；仅无配置的全新用户确认一次本人私有存档库，默认推荐当前GitHub账号下的BridgePilotData，也可指定已有库，验证后保存绑定。已有配置直接复用并核对，不从公开产品包推导用户私有库，不把已有远端档案当空白库 |
| `$bridgepilot project add <本机目录>` | 核对实际仓库、项目规则、安装入口与原生项目身份，登记到本人 BP 档案；缺少关键信息时补问。登记不授权骨架改造、业务开发、提交或发布，不直接修改下游文件 |
| `$bridgepilot handoff prepare all` | 核对控制机与范围，全部暂停并确认停止，保存当前开发上下文，沿项目受管 Git 入口保存并推送进度，验证代码检查点，同步档案并释放旧机控制权 |
| `$bridgepilot handoff restore` | 检测必要依赖，必要时静默安装，复用 GitHub 正式包下载校验，恢复档案与所有登记项目的准确代码检查点及本机绑定；经受管 handoff-accept 成功后报告，任务继续暂停 |
| `$bridgepilot status` | 只读查看已登记项目、控制机、交接阶段、运行停止状态、已保存进度及具体阻点，不安装、推送、接管或恢复任务 |

统一 Skill 的唯一源为现有 `source/orchestrator/skills/bridgepilot/`，沿用日常任务与调度能力，增加上述管理模式，随 BP 独立包管理及升级；为支持任意 Codex 对话调用，安装时提供受管的用户级 Skill。源及所有权属于 BP，不进入 `source/forge/templates/skills/` 或 Forge 共享 Skill 分发，不改写用户自有同名技能。新机尚无此 Skill 时，由现有 GitHub 引导完成依赖、BP 与 Skill 的首次安装；全新用户初始化自己的档案，换机用户恢复自己的已有档案。底层现有 init／configure／sync-configure／project-add 应优先复用，Skill 用法不冒充已增加的 CLI 子命令。

Skill 负责明确模式、授权及流程协调，Rust 内核继续负责停止、持久状态、传输、安装和控制权核验，不另外制造脚本执行器或第二套交接档案。安装与恢复的完整说明继续以根 INSTALL.md 为唯一入口；Skill 仅保留必要决策、步骤和停止条件，较长模式细节按需引用。

#### 安装与数据边界

- 静默安装仅覆盖 Git 与 GitHub CLI，优先复用符合要求的已有版本，从官方来源取得所需安装程序，优先当前用户范围；不默认升级其他工具、不安装 Codex或项目业务依赖、不重置 Git 配置。凭据登录、SSO及操作系统强制权限确认仍由本人完成，不复制旧机登录状态、不绕过确认。
- 代码进入各项目自己的受管远端工作分支，任务与交接事实进入本人已确认并绑定的私有档案库（本机现有绑定为 BridgePilotData）；当前控制聊天的已确认目标、决定、未完成工作及证据链接写入项目接续说明，不声称私有档案包含整段原生聊天、凭据或业务数据。
- 已有本机代码或配置发生冲突时保留现场，不强制覆盖；未明确归属的改动保持不动并报告依赖阻点。暂停、Unknown、CAS、网络丢回执、单控制机及原生聊天授权规则继续适用，失败从可核验阶段恢复，不重发未知操作。
- 不复制旧机项目绝对路径作为新机事实。先发现并核对已有仓库，再选择实际目录和原生项目绑定；缺少代码或环境时明确说明。独立只读恢复不等于接管，接管不等于恢复任务或 heartbeat。

#### 实施范围与验收安排

预期改动：扩展上述现有 BP Skill及必要模式引用；更新 `source/orchestrator/install.rs` 的私有资产打包、受管用户级 Skill 安装／升级／恢复及相关 CLI 入口；依实际需要在 orchestrator 内补必要的依赖检测／安装能力；同步 `source/orchestrator/skills/bridgepilot/SKILL.md`、INSTALL、README、GitHub 新版安装引导及明确资产清单；定向测试放在 `scripts/tests/orchestrator/`，实际包输入与所选检查登记按真实改动维护。既有0.5.0正式资产不可覆盖，新包与版本只能经受管开发收口产生。

验收覆盖旧机范围与进度保存、新机恢复及自动接管后仍暂停、Skill归属与可发现性、已有安装保护、依赖已存在／缺失／安装失败及需人工登录分支、中断恢复与未知操作拒绝。使用隔离身份和测试远端验证，不实际暂停当前五项目、转移当前控制权、提交下游代码或卸载本机 Git／gh 来造缺失环境。真实缺依赖系统与物理双机缺少可用环境时保留未验证项，不能以模拟结果代替。

规模暂定 **M**，复用 BP 整体已约定的时间与预算边界，不增加预算访谈或原生 Goal；默认主对话推进，按实际风险选择必要验证。正式开工确认只授权本阶段实现、必要隔离验证与受管开发收口，不自动授权新产品代码提交、推送、正式发布或实际换机。用户后续明确调用 prepare／restore，才触发对应已确认的操作授权。

#### 方案保存决定与后续开工授权（2026-11-01）

用户在本阶段开工确认中明确选择 **B：只保存方案，暂不实施**。上述需求、统一Skill命名及已确认行为保持有效，但本阶段尚无实施授权；不实施或安装本阶段的统一 bridgepilot Skill 扩展、不修改核心实现、不进行依赖安装、隔离执行或开发收口，也不触发实际项目暂停、进度上传或控制权接管。此决定仅限制本阶段实施，不改变其他项目的暂停状态，不撤回既有已发布成果。后续收到明确开工授权时，从本节接续，不重复确认已记录的需求。相关 TODO-050 及整体计划状态保持，不标完成。

后续用户明确“开始开发吧”，已接续本阶段实施并解除上述暂不实施限制；原已确认需求与本轮无真实交接／无新产品代码发布的边界不变。传播四问：本阶段为BP私有产品能力及自身入口说明，非Forge共享能力；orchestrator经受管流程升兼容minor，主程序及正式CHANGELOG留待另行release；公共模板／八包投影不传播，仅同步本仓库项目级AGENTS导航、BP独立包与用户级投影。

#### 本阶段实际交付

已扩展现有 `source/orchestrator/skills/bridgepilot/SKILL.md`，五种管理模式按需读取 init／projects／handoff 引用，原日常调度与执行规则移入 execution 引用并保留。私有包使用十项稳定资产登记，新增四模式引用；用户级投影仍归 BP，不使用 Forge 共享分发。底层原有 init／sync-configure／project-add／handoff 内核继续承担已有职责，不把 Skill 的空格参数伪装为 CLI 子命令。

`user_skill.rs` 提供 skill-install-preview／skill-install／skill-status／skill-recover：从实际安装home的受管来源取得内容，显式固定五文件与本机绑定；不采纳未知同名Skill、用户编辑或额外文件。跨目录写入前保存journal，逐项核验旧字节，入口与绑定最后提交；回滚前逐项比较实际观察字节。与程序安装共用home锁并互查pending，持锁后复核home到skills根的绑定，提交前核验marker，拒绝跨profile竞争造成第二份失管投影。已投影用户Skill时，回退core必须具备维护该Skill的能力；旧六资产包仍可读取，不用改写历史包冒充新包。

`environment.rs` 提供 environment-check与environment-install，Git最低2.28、GitHub CLI最低2.0；只针对明确的Git.Git／GitHub.cli、scope与授权来源，通过受管无窗口进程调用WinGet。检测可指定现有context，安装仅接受真实已安装home，状态与锁固定在此home，不能换项目目录重发Unknown。安装意图在启动前持久化，失败／超时／丢结果保留真实回执；成功但PATH滞后不重复安装，不自动提权、改scope、登录或升级其他软件。冷启动先通过系统HTTPS读取正式包，校验后无需Git／gh安装BP程序，再准备依赖，避免要求缺失的工具先下载自身入口。

独立只读复核定位并修正了四项边界：恢复扫描后的用户编辑、程序与Skill安装交错、依赖安装状态随context分散、同一home被不同Skills根竞争绑定。修正由屏障／交错用例覆盖，复核结论为无剩余阻断；未将静态复核冒充真实系统安装。

| 证据 | 实际结果 |
|---|---|
| 受管收口 | `development-apply.log`：development-closed，orchestrator **0.5.0→0.6.0**，最终完整登记检查executed=1／reused=0／git_written=false；其他包未升版，主程序仍13.1.0 |
| 账本 | 末条hash `sha256:3b5feeef854d9558bc42f1a303cd146c43da015ae506ba92db5cee3b317c7872`，完整orchestrator-unit-integration为passed |
| 新增边界用例 | 三项恢复单元用例与七项setup集成用例，覆盖外部编辑、部分回滚／提交恢复、两类pending、跨profile竞争、未知安装拒绝、成功后PATH滞后、用户文件及安装来源保护；最终候选由完整登记检查覆盖 |
| 最终构建与包 | `final-build.log`锁定离线release成功；`final-bundle.json`为0.6.0／十资产；`.build/bridgepilot/portable/current`已切换0.6.0，旧0.5.0保存在preserved-0.5.0 |
| 隔离真实安装 | `isolated-summary.json`：程序及用户级Skill安装成功，两次preview零写入，本机Git／gh检测ready；空档案revision0、未启动任务 |
| 独立Skill使用 | 独立Agent按隔离投影中的真实本机绑定执行status所需只读命令，实际报告安装0.6.0、Skill current、零项目／任务、revision0、未配置存档库和custody，无写入或联网 |
| 本机交付 | `personal-install.json`／`personal-skill-status.json`：既有home升级至0.6.0；用户Skill安装在 `C:\Users\bridg\.codex\skills\bridgepilot`，source仍在BP安装home |
| 原生发现 | `native-skill-probe.json`：真实skills/list读回bridgepilot、scope=user、enabled=true、准确用户Skill路径；task_turn_started=false，没有创建或恢复模型回合 |
| 私有档案保护 | `personal-archive-head-before.json`与实际HEAD字节相同；档案仍revision85、原owner，未暂停真实项目、上传新私有修订或接管其他机器 |

原始输出统一位于 `.runtime/tests/bridgepilot-unified-skill/`。本阶段开发与本机试用交付完成，用户验收另行记录；TODO-050整体状态不因此关闭。**Git／gh真实缺失机器上的静默安装、SSO／管理员／重启分支和两台物理电脑的完整换机尚未验证**，这些分支有隔离状态机用例与明确停止行为，不能称为真实冷机验证。GitHub正式资产仍为已发布13.1.0对应的core0.5.0；新core0.6.0及文档未提交、推送或正式发布，新电脑下载安装新版统一入口须等待后续授权发布。

#### 统一入口用户验收（2026-11-01）

用户明确调用 `$summary accept`，验收本阶段统一 bridgepilot Skill 的开发与本机交付，并另以 `$git-sync release` 授权正式发布。本次验收覆盖 init、project add、handoff prepare all、handoff restore、status 五种模式、BP 所有的用户级 Skill 安装与恢复、依赖准备边界及已有 GitHub 引导衔接；依据为上表最终候选收口、完整登记检查、独立复核、隔离安装、本机安装及原生发现回执。最后只读 development-probe 返回 development_verified／task-development-current，目录、指令来源、Skill 元数据、清单及版本检查通过，私有档案 HEAD 原始字节未变。

验收不把模拟用例视为真实缺依赖系统安装，不把隔离恢复视为物理双机接力；真实冷机安装、物理双机、真实双项目业务发布及既有唤醒／旧 cwd 收尾仍保留原验证边界。关联 **TODO-050** 更新为本阶段已验收，整体仍为进行中，文档 lifecycle=active／validation_status=in_progress 不变。上段“尚未发布”为开发交付时状态；本次另获发布授权，正式结果以受管 Git 同步收据和实际 GitHub Release 为准，不能用授权代替成功事实。本次未操作真实项目交接或恢复定时唤醒。

#### 本次发布停止记录

已完成一次受管 preflight、sync-remote 和 release-preview：实际推送目标 origin/main，远端接入后 ahead=0／behind=0，主程序最低兼容新增级别为13.2.0；累计仅orchestrator 0.5.0→0.6.0，版本覆盖引用本阶段schema6收口记录，独立required_checks为空，未补做develop或业务测试。正式调用使用绑定预览的产品分析和项目内message-file。

实际release返回 **blocked**：`automatic write planning failed before apply: generated asset build failed: codex.hooks`，stderr仅显示新版bridgeforge-hook及依赖编译进度，没有Rust诊断或链接错误。实现对timeout与非零退出使用同一错误格式，当前回执不足以确认具体原因，不能直接称为代码编译错误。原始证据位于 `.runtime/bridgepilot-unified-release/release.json`、`release.stderr.log`、`remote.json`、`preview.json`。

失败后独立核对根VERSION仍 **13.1.0**，本地HEAD与实际远端origin/main均为 `b447e6954e59244d08e37cdc9d396ca8718767fd`，暂存区及stash为空，原开发改动和验收文档保留，未提交、推送或创建新版GitHub Release。待发core0.6.0便携包十资产加manifest的SHA256已核对，但未上传。遵循git-sync失败边界停止外层自动重跑；下一步应针对Hook产物构建的真实退出／超时原因处理，再在有效发布分析下重试，不跳过产物闸或以旧二进制代替。

<a id="bp-status-sqlite-plan"></a>

## 15. 第九阶段：status 统一输出、执行事件与 SQLite 唯一状态库迁移（2026-11-01）

### 15.1 已确认需求、范围与本轮权限

本阶段继续关联 **TODO-050**，是该原需求下的产品架构演进，不新增第二份需求卡或重复 TODO。15.1～15.13 保存最初规划合同；当时状态为待执行、未验证，后续授权、执行及工程交付见15.14～15.18。本文件整体仍 `active/in_progress`，未收到本阶段用户验收，不结算TODO。

确认来源为[当前控制对话](codex://threads/01a19535-bab4-7c10-b0ac-1c8f9c263b4f)。用户已明确：

1. `status` 表头及顺序固定为 **序号／项目／任务／阶段／进展／阻点**，由 CLI 统一生成。
2. 用户查询入口只有 `status`（通过统一 Skill 使用时为 `$bridgepilot status`）；内部路径、格式参数和 JSON 协议不作为用户需要填写的接口。
3. 在任务管理和状态查询链路中，Agent 只承担“理解目标与授权”和“展示结果并回答追问”。登记、固定编号、记录执行事件与回执、状态转换、查询及渲染由 CLI／执行器和 SQLite 完成。业务开发本身使用模型执行不改变这个管理职责边界。
4. SQLite 是本机**唯一在线任务状态库**；不保留 JSON 和 SQLite 两套可写任务档案。JSON 可继续作为内部请求／响应或外部原始回执的编码，不等于第二套持久状态。
5. GitHub 私有存档库保存一致的 SQLite 状态快照，新机下载、校验并恢复 SQLite；不采用“GitHub 保存 JSON、本机转换 SQLite”的常态路径。
6. 用户选择 **B：完成实际切换**。最终验收包括现有真实个人档案转换、GitHub 私有存档格式切换、本机 BP 与用户级 Skill 更新，以及备份恢复验证，而非只交付迁移工具和测试数据。
7. 用户规划阶段原文为“选B，按这个范围生成完整开发计划并落盘，先不要实施”。该时点只授权计划；随后已明确批准 autopilot 开工及整体预算，并授权调额后继续，见15.14～15.18。产品源码 Git 提交／推送／正式发布仍未授权，私有档案快照上传属于已批准B范围。

首版其余已确认规则继续有效：不自动接管其他机器，不恢复 Unknown 操作，不推导新业务授权，不修改 Codex 原生数据库、会话或 Memory；不从本阶段范围推导四个下游业务写入、收口、提交或发布权限。

### 15.2 现状依据与需要替换的链路

本轮核对的是当前工作树源码；包版本为 orchestrator `0.6.0`。此前只读核对的真实档案为 revision 85、五个项目、两项只读任务，用户级 BP／Skill 为 0.6.0。这些是迁移调查起点，执行前须重新核验，不把旧 revision、owner、指纹或远端 HEAD 写死。

| 已核实入口 | 当前行为 | 本阶段影响 |
|---|---|---|
| [domain.rs](../../../source/orchestrator/tasks/domain.rs) | 已有任务随机 ID、spec hash、确认、Run、Operation、暂停／Unknown／交付状态；没有固定人类任务序号及统一进展视图 | 保留任务身份与状态机，增加持久序号、事件和来源关联 |
| [store.rs](../../../source/orchestrator/storage/store.rs) | `HEAD.json` 选择不可变完整 JSON 修订；读取校验父链；文件锁＋expected revision 提交；索引仍为 `index/tasks.json` | 替换为 SQLite 事务和可查询历史，迁移旧链，不只把完整 State JSON 塞进数据库 |
| [cli.rs](../../../source/orchestrator/cli/cli.rs)、[main.rs](../../../source/orchestrator/cli/main.rs) | `status` 返回整份状态，main 固定序列化 JSON | 新增同一数据库快照产生的六列视图与确定性展示，兼容并逐步调整机器消费者 |
| [codex.rs](../../../source/orchestrator/execution/codex.rs) | 已关联 thread／turn 与命令事件；命令回执主要先积累在内存，回合结束才随结果入档 | 将相关事件在接收时可靠记录；崩溃后仍能展示最后已提交进展 |
| [scheduler.rs](../../../source/orchestrator/execution/scheduler.rs)、[review.rs](../../../source/orchestrator/execution/review.rs)、[delivery.rs](../../../source/orchestrator/execution/delivery.rs) | 调度、报告复核、开发及发布认证分别推进状态 | 统一通过新 Store 事务保存，保留完成门禁，不能由任意进展更新冒充认证 |
| [sync.rs](../../../source/orchestrator/transfer/sync.rs)、[handoff.rs](../../../source/orchestrator/transfer/handoff.rs) | 同步显式列举 HEAD／revisions JSON，核对 Git 和档案双重祖先；pending、取消／栅栏及独占接管已有语义 | 改为 SQLite 快照与逻辑修订祖先，保留未知网络结果核销和控制权保护 |
| [install.rs](../../../source/orchestrator/install/install.rs)、[user_skill.rs](../../../source/orchestrator/install/user_skill.rs) | 安装／回退读取实际档案验证兼容；Skill 五文件由 BP 固定资产表管理 | 增加 SQLite 读写能力门禁、旧程序拒绝和本机更新顺序，不绕过所有权 |
| [BP Skill](../../../source/orchestrator/skills/bridgepilot/SKILL.md) | 本对话曾局部增加由 Agent 查询聊天后汇总的候选流程，尚未完成收口／安装 | 明确替换该临时方案，不能继续让 status 依赖 Agent 临场拼表 |

当前工作树已有 BP 私有源码、配置、账本与文档改动。本阶段执行时先对照既有已验收 0.6.0 收口证据确认同包基线；不 reset、不覆写，不以本次局部修改认证来源不明的整包。另一对话的 Forge 14.0.0 发布与下游升级结果以[其唯一记录](../forge-downstream-upgrade/plan_2026-11-01_four-projects.md)为准，不从本文件旧阶段的发布停止记录推断当前 Forge 状态。

### 15.3 产品流程与职责

```text
用户给目标
  → Agent 理解范围、必要人类决定及授权，形成结构化任务输入
  → CLI 校验／去重／登记 → SQLite 事务分配固定序号与写入任务
  → CLI／执行器关联实际执行 → 接收事件／回执 → SQLite 事务更新
  → 用户查询 status → CLI 只读同一数据库快照 → 生成固定六列结果
  → Agent 原样展示 CLI 的任务表，并回答用户追问
```

职责约束：

- 第三至第五步不依赖 Agent 每次阅读聊天推断状态、填写百分比或手写“最近进展”。阶段来自状态机与已验证里程碑；进展来自已入库的执行事件，阻点来自确认缺口、依赖、实际错误或用户明确的暂停／暂缓决定。
- 用户改变目标、暂停范围或发布决定时，仍属于输入端的“理解目标与授权”，经受管 CLI 写入；不是让 Agent 获得任意改状态的入口。
- 所有新 BP 任务先登记再派发。登记与派发使用来源幂等键，重试不能产生第二个任务或第二个执行；不得只更新计划文档后跳过登记。
- BP 自管执行与“关联已有对话观察”分别建模。已有对话可以绑定同一任务并由适配器获取真实执行元数据；不能为显示 Running 伪造 BP Run，也不能被调度器重复派发。原生接口无法可靠提供事件时明确显示观察受限／最后确认状态，不推测正在执行。
- 只处理明确交给 BP 的任务。一次性补录限当前对话及用户已指明的[核查 BridgePilot 状态](codex://threads/01a19487-ef12-7a80-8a5a-a35f4387bf07)等确认来源；不是自动接管所有历史聊天。补录保留“历史回填”来源，不重发已经完成的业务动作。
- Agent 的业务报告原文可作为证据存档，但不直接触发 Released 或 ReportDelivered。现有报告／开发／发布核验入口仍是正式终态的唯一推进路径。

### 15.4 status 的固定输出合同

| 列名 | 生成规则 |
|---|---|
| 序号 | 档案内持久整数任务号；登记事务分配，取消、过滤、排序、重试、换机均不重排、不复用。原 task ID 保留为机器身份。序号可不连续，不是每次输出的临时行号 |
| 项目 | 已登记项目显示名称；任务必须关联真实项目，跨项目目标按实际子任务／依赖登记，不能由展示层随意合并或拆分 |
| 任务 | 已确认任务标题；展示层不改写目标，长文本只按统一规则转义与截断 |
| 阶段 | 状态机中文映射及有证据的子阶段，例如“执行中”“开发验证·受阻”“待发布·暂缓”；内部状态单独保存，不额外增加状态列 |
| 进展 | 最近已提交、属于本任务当前修订／执行的事件摘要；可展示已完成的登记检查计数，不从命令数量推算整体完成百分比 |
| 阻点 | 明确用户决定、未满足依赖、待确认请求、失败／Unknown 等结构化原因；没有阻点为“—”，缺证据时说明“待核实”而非报成功 |

默认显示尚未交付的任务，包括待确认、待执行、执行中、Reviewing、待发布、受阻、Unknown、暂停／暂缓；Released、ReportDelivered、Cancelled 默认不占当前清单。阶段分组优先级固定，组内按持久任务号排序，不由 Agent 自行排序。没有当前任务时由 CLI 明确输出空清单结果。

运行完成不等于任务交付；发布暂缓不等于项目已执行暂停。项目暂停、任务暂停、仅发布动作暂缓分别存储及展示，不能因为四下游暂缓 release 就声称其全部进程已停。

查询在一个只读事务中读取任务、当前有效 Run、事件与阻点，并附档案身份、逻辑修订及查询时间。来源链接、核对时间和安装／控制机异常放在表外，固定表格始终只有六列。跨状态字段矛盾时输出诊断，不拼接互相矛盾的多次查询结果。

内部响应保留结构化行与 CLI 生成的展示文本，二者来自同一视图；Agent 只提取并展示，不重新编号或补写正式任务行。内部 JSON／展示编码不对用户形成第二套查询命令，现有安装器等机器消费者按能力门禁适配。

`status` 零写入：不创建数据库／索引、迁移 schema、补登记、刷新回执、重建缓存、联网推送或自动核销。缺库、旧格式未迁移、损坏、版本不兼容和忙超时均给出可区分诊断。不能用只读查询失败作为自动初始化空库的理由，也不能靠扫描聊天掩盖登记链路缺失。

### 15.5 SQLite 数据模型与事务合同

唯一在线库的目标位置为 `<私有工作区>/doc/5_project_knowledgebase/bridgepilot/bridgepilot.sqlite3`，属于用户长期资料，不能放进可清理的 `.runtime/`。本机完整备份也属于受保护资料；构建缓存进入 `.build/`，测试与可丢弃暂存进入 `.runtime/tests/<任务>/`，安装配置继续使用已规定的 `.runtime/config/`。不得为本阶段修改三目录合同或将用户档案纳入产品分发。

采用项目自有 Rust SQLite 绑定，锁定 Cargo.lock，通过受管 Rust 入口构建；不依赖用户手工安装 sqlite3、Python 或额外数据库服务。具体依赖版本在开工时核对工具链、许可证和 Windows 构建兼容，不在计划中编造已经引入的依赖。

| 数据组 | 主要内容与约束 |
|---|---|
| 档案元数据与 schema 版本 | archive ID、逻辑 revision、schema version、持久序号分配器、内容／祖先摘要、启用的 reader／writer feature |
| 项目与本机绑定 | 项目身份、仓库、暂停、机器／路径／原生项目绑定；保留原跨项目别名冲突检查 |
| 任务、修订与依赖 | 原 task ID、固定 task number、规格修订与 spec hash、人类确认和授权来源、执行模式；外键、唯一约束及依赖无环校验 |
| Run、Operation 与事件 | 当前／历史执行、thread／turn／item 关联、Attempted／Observed／Unknown、事件唯一键、执行器来源、真实回执和错误 |
| 状态、进展及决定 | 受管状态机、阶段里程碑、最近事件、结构化阻点、暂停／仅发布暂缓等决定及其作用范围 |
| 交付与交接 | 报告／开发／发布核验身份、控制机／epoch、代码检查点、换机 transfer 与 continuation、heartbeat 关联 |
| 历史及迁移凭据 | 全部旧修订的可查询历史、原摘要及父链映射、迁移来源与完整性证明；不能只导入最新状态而丢弃历史 |
| 本机恢复与同步 outbox | 迁移激活阶段、网络候选身份、预期远端、已尝试／Unknown、必要恢复输入；本机专属字段不得随共享快照外泄 |

规范化表是当前状态的事实源，历史以追加事件和必要历史表表达；不要另存一个完整可写 State JSON blob 当第二事实源。现有 State 可以暂留为内存领域模型／兼容适配器，写入必须落在同一个 SQLite 事务里，`status` 直接查询视图所需表，避免每次重读所有历史。

写事务验证 owner、项目／任务授权、expected revision、schema 能力、状态转换和未决外部操作；事件、状态、序号和 outbox 意图一起提交，成功后才允许触发外部动作。保留 CAS 语义和有界忙等待，不将网络或长工具执行放在 SQLite 写事务内。`PRAGMA foreign_keys` 每个写连接显式启用并核验，不依赖默认值；迁移与恢复使用 integrity／foreign-key 检查和领域不变量复核。

共享任务的逻辑修订／祖先与本机 outbox、安装激活等恢复元数据的事务版本分别管理：本机核销进度不能冒充一次可同步的业务修订，也不能因剔除本机专属表而破坏远端逻辑摘要。共享摘要使用显式表／列的稳定编码；不以 SQLite 页布局或整个文件字节作为领域身份，不另存一份可写 JSON 状态。

本阶段优先采用 rollback journal 与短事务保证严格只读查询和简单单文件离线快照；数据库及 journal 均在受保护数据根内。若实测并发要求采用 WAL，必须先验证只读连接可能涉及的 sidecar 创建、备份和恢复行为，不用 `immutable` 绕过活库一致性；不能未经证据改变 status 零写入合同。进程锁仅辅助迁移／切换，跨机器独占仍靠原 owner／epoch 和远端栅栏，SQLite 本地事务不替代它们。

### 15.6 事件采集、状态归约与异常恢复

1. 登记和来源绑定提交后才启动执行。事件主键绑定 task/spec revision、run、thread、turn 与原生 item；重复或乱序事件按协议核对，旧执行事件不更新新修订任务的当前进展。
2. 执行器接收启动、命令完成、审批请求、超时、停止、回合终态和受管验证／发布回执时，分批进行短事务持久化；最新已提交事件构成进展，不等整个模型回合结束后才可查询。
3. 阶段由明确的开始／结束事件和既有核验器推进。普通命令 exit=0 只证明该命令结果；模型 turn completed 仍只进入 Reviewing；缺乏结构化里程碑时显示“执行中”及实际事件，不调用模型猜阶段。
4. 命令及证据体积沿当前边界保护，截断／缺原文标记必须保留。摘要不能替代核验所需完整证据；失败时不能把截断输出记为通过。外部原始 JSON 回执可作为 SQLite 中的不透明证据载荷保存，不另维护可写 JSON 任务状态。
5. 意图先入库，外部调用后记录实际回执；进程在中间退出时保留 Attempted／Unknown。重启后只读核销原 native／Git 结果，不以 SQLite 回滚为理由重新创建聊天、执行命令或发布。
6. 观察已有对话的适配器必须保存实际绑定、核对游标和观察时间；禁止修改原生存储。无法取得可归属事件的任务保留最后证据和观察限制，不把客户端 active／idle 当业务认证。

### 15.7 JSON 退役、历史迁移与回退

退役范围是 BP 在线任务档案 `HEAD.json`、`revisions/*.json`、派生 `index/tasks.json` 及其在 GitHub 当前存档树中的常态传输。BP 关键任务／控制权／同步恢复状态不再以可丢弃 JSON 文件作为唯一来源。本机安装配置、产品机器合同、原生 API 通信、下游业务文件不因扩展名 JSON 被扫描删除或迁移。

采用单向受管迁移，不双写、不自动降级：

1. **只读预检**：核对真实 home、workspace、archive、owner、现有程序能力、全部旧链、未决同步、Run／进程和文件权限，产生转换清单与计数；`status` 不触发本步骤。
2. **冻结切换窗口**：只有另行开工且到达真实切换阶段，才按已授权范围停止写入；活动／Unknown 未核销、其他机器控制或未决网络结果存在时停止，不能自动清空 pending。单纯迁移本机存储不冒充已完成换机释放。
3. **保存可恢复原件**：逐字节保存当前 JSON 全链和配置身份、权限及摘要于受保护备份位置；记录迁移恢复输入，不因 cleanup 或程序重启丢失。此原件只用于取证／回退，不能与新库同时服务在线写入。
4. **构建离线候选**：验证每一旧修订并导入项目、任务、授权、确认、Run、Operation、wakeups、custody、checkpoint 与历史。保留全部旧机器 ID、task/run/op ID、状态、来源和历史摘要，不修饰 Reviewing／Unknown 等未完成状态。按“首次出现在已验证修订中的顺序，同修订内原 ID 排序”一次性分配旧任务序号；不伪造旧创建时间，记录导入时间及来源修订。
5. **等价性核对**：逐修订或逐历史事件验证领域内容、计数、依赖、确认、状态和证据摘要；新库物理字节不同不构成业务不等价。原始 JSON 字节可作为只读迁移证据纳入库内历史载荷，或保留于一次性旧格式备份；不能把不可解析旧字段静默丢弃，也不能把这种载荷当新状态存储。
6. **激活**：安装兼容的新 reader／writer 后，在持锁且旧写入停止的窗口原子选择唯一后端，记录切换凭据；中断按凭据继续或恢复，不能凭“看到一个 .sqlite3 文件”猜迁移成功。所有开库入口先识别后端能力，旧程序必须在写入前拒绝已迁移档案。
7. **切换后回退**：新库尚无新写入及外部动作时，允许按受管恢复流程还原原件；已有新事件／网络结果后禁止直接恢复旧 JSON，以免丢失进度或重复副作用。此时先保存新库、冻结依赖动作，再通过受控 SQLite 恢复／修复或另获批准的数据迁移处理，不能静默退回旧程序。
8. **退役**：等价性、本机运行、远端快照读回和恢复验证全部通过后，停用旧 JSON writer 和缓存入口。旧文件的实际移除只按实施阶段明确的迁移清单处理，保护原件备份；不改写或清除 Git 旧历史，不新增常驻 JSON 导出任务。

迁移期间新旧软件兼容以能力检测与显式格式版本控制；不允许旧软件把 SQLite 数据当作空档案重新 init。`rebuild-index` 等旧入口要么改为基于 SQL 的受管维护，要么给出明确退役诊断，不能继续写 JSON 或返回虚假成功。旧格式导入器保留可审计的只读兼容，正常新机恢复不反复 JSON→SQLite 转换。

必须用实际旧版程序验证拒绝行为，不能假定已发布旧 reader 会识别新 capability。旧在线 HEAD 位置应保留不含任务数据、令旧读取／init 明确失败的退役保护哨兵，或采用经验证等效的旧入口阻断；它不是第二套 JSON 档案。新入口以 SQLite 格式门禁识别，安装回退和远端旧格式消费者也须 fail closed。不能只移走 HEAD 和 revisions 后留下能被旧版 init 重建的“空档案”现场。

### 15.8 GitHub SQLite 快照、换机与恢复

保留本人既有私有仓库及 main 分支绑定，不另建仓库，不 force push，不按数据库二进制做自动合并。数据库物理 hash 用于传输验真，逻辑 archive／revision／parent／digest 用于祖先和回滚检查，不能把“sqlite 文件不同”直接等同于业务分叉。

本机完整备份可使用 SQLite Backup API 得到一致数据库副本；跨机快照必须按显式表／字段合同只包含可同步任务资料、必要历史和交接元数据。若在线库含本机恢复／同步 outbox 等专属数据，不能直接上传整个在线库：在一致读视图中构造新的 SQLite 快照，只复制白名单数据并复核计数／逻辑摘要；不能复制后 DELETE 敏感行就假定自由页已清除。本机配置、凭据、当前进程所有权材料及 Codex 原生状态不进入远端。

Git 当前树拟仅维护 `doc/5_project_knowledgebase/bridgepilot/bridgepilot.sqlite3` 这一任务快照，schema、archive、revision、祖先、快照来源及需要的 reader 能力存于库内。校验摘要可使用 Git 对象身份／固定格式摘要字段；不再另生成 HEAD／revisions JSON。保留 Git 历史作为历史传输版本，文件名与实际所有权清单由同步模块固定管理，拒绝未知归属路径。

库内字段保存逻辑摘要及来源身份；整个快照文件的物理 SHA-256 放在传输／提交回执中，不能试图把数据库自身的完整文件摘要回写进同一个数据库形成自引用。远端读回同时核对 Git commit、文件摘要和库内逻辑修订；恢复所需退役保护哨兵由受管激活流程在本机创建，不作为另一份远端任务数据。

旧机 prepare all 的顺序仍是：明确全部暂停 → 实际核验停止和无 Unknown → 按各项目既有授权保存并核对代码检查点 → 生成释放控制权的 SQLite 候选快照 → 保存上传意图与准确候选身份 → 条件推送 → 远端读回确认 → 激活本地冻结／已释放结果。不得直接把正在写入的 .db 主文件上传，也不得先释放控制权再猜推送成功。

新机 restore：确认本人 Git／gh／Codex 环境 → 下载完整 Git 历史及准确快照 → 校验 schema、摘要、逻辑祖先与完整性 → 恢复到候选数据库 → 核对真实各项目代码检查点／绑定 → 按既有独占协议远端确认接管 → 激活本机。旧 Run 和历史不改写，任务及 heartbeat 仍暂停，继续执行须遵守原授权和明确 resume 范围。

SQLite 事务不能跨本机与 GitHub 原子提交：同步 outbox 要持久保存预期远端、候选 commit／快照摘要、阶段及核销所需输入。网络期间不长期占用 SQLite 写锁；响应丢失只 reconcile，不能重新上传未知候选。现有 pending 栅栏、取消、竞争接管、分叉拒绝和迟到旧提交保护均须迁移，不能为简化数据库改造删除这些路径。

GitHub 切换使用新格式首次提交承接既有已验证 JSON 远端祖先，旧历史保留。首次 SQLite 下载读取和安装必须具备 reader 能力，新旧格式混合／不支持版本时阻断，不把旧 reader 的失败改成“新空库”。本机可清理缓存不是唯一恢复依据；必要候选身份及恢复数据保存在数据库的本机专属持久表中，缓存丢失后仍能安全核销。

### 15.9 影响文件与传播四问

| 范围 | 已定位文件或拟新增模块 | 修改边界 |
|---|---|---|
| 数据库与迁移 | `source/orchestrator/store.rs`；拟新增 `database.rs`／`migration.rs` 及 schema 资源 | 新建 SQLite 存储／历史／事务接口和旧链只读导入器；按职责拆分，不继续扩大单个 Store 文件 |
| 领域与事件 | `domain.rs`、`codex.rs`、`scheduler.rs`、`owned.rs`、`grouping.rs`、`review.rs`、`delivery.rs`、`wakeup.rs`、`lib.rs`；拟新增事件归约模块 | 保持授权、进程身份、原生项目、暂停、Unknown 和交付核验语义；SQL 事务连接传递与外部调用边界明确 |
| 查询与入口 | `cli.rs`、`main.rs`；拟新增 `status.rs` | 六列视图、固定序号、阶段映射、统一转义排序、只读错误；既有机器响应按协议升级 |
| 同步与换机 | `sync.rs`、`handoff.rs`；必要时独立快照模块 | 从 JSON 文件集换为一致 SQLite 传输，保留双祖先与网络未知核销，不能越权接管 |
| 依赖／安装 | `source/orchestrator/Cargo.toml`、根 `Cargo.lock`、`install.rs`、`user_skill.rs`、`launcher.rs`、本项目原有包登记与 artifact layout | 锁定 SQLite 构建；能力检测、迁移／激活和回退保护、安装资产显式登记；不自动改上游公共模板 |
| 本地 Skill 与说明 | `source/orchestrator/skills/bridgepilot/SKILL.md` 及四个既有 reference、README、INSTALL、`doc/0_architecture/design/bridgepilot-runtime-protocol.md`、本计划与索引 | 本阶段仅计划；实施后才同步“当前架构”。替换 Agent 拼表候选，沿 BP 自有打包／投影更新用户级副本 |
| 验证 | `scripts/tests/orchestrator/domain.rs`、`store.rs`、`protocol.rs`、`execute.rs`、`rpc_fixture.rs`、`scheduler.rs`、`delivery.rs`、`portable.rs`、`setup.rs`、`wakeup.rs`；拟新增 SQLite／status／migration 专项用例 | 独立测试仍留在规定目录，报表在 `.runtime/tests/<任务>/`；不让生产运行依赖测试 fixture |

传播四问结论：本次为 **BP 产品层私有能力**，非 Forge 通用能力；需要 orchestrator 子包受管收口及最终产品发布时的版本／CHANGELOG 分析，但本轮不写版本或发布说明。新格式对旧程序不兼容，具体版本判级按真实兼容门禁与现行版本合同分析，不预写目标版本。公共八包投影和 Forge 共享 Skills 不传播；自身 BP 包、用户级 BP Skill、本地安装与产品说明需要同步。Forge 自身源码只在发现确有必要的构建／发布集成缺口后另说明，不借本次计划修改公共模板、AGENTS 或下游项目。

### 15.10 分步任务、依赖与完成标准

下列 BP9 编号是 TODO-050 内部工作分解，不能冒充新库的真实任务号。

| 编号 | 优先级／依赖 | 实施内容 | 验证／完成标准 |
|---|---|---|---|
| BP9-01 | P0；首项 | 重验工作树、已验收包基线、home／档案／远端、活动执行及未决操作；冻结六列、状态映射、数据与同步合同 | 明确本阶段差异、受保护输入、唯一后端、旧格式边界和切换检查点；不把旧三小时预算套到新阶段 |
| BP9-02 | P0；01 | 引入锁定 SQLite 依赖，建立 schema／连接／事务、revision CAS、能力门禁与持久任务序号 | 事务失败零部分提交，序号唯一不复用，外键／依赖校验有效，多进程冲突及有界忙等待可核验 |
| BP9-03 | P0；02 | 建立旧档案只读预检、全历史导入、等价性核对、激活与恢复协议 | 有效／损坏／重复字段／断链／孤立修订／Unknown fixture 可区分；重复迁移不重复导入，旧 reader 不能写新库 |
| BP9-04 | P1；02 | 将任务登记、现有 Store 消费者与执行事件持久化迁至 SQL，接入已有对话观察模式 | 中途退出保留已提交进展；重复／乱序事件不重复推进；已绑定对话不被调度器再次派发 |
| BP9-05 | P1；02、04 | CLI 实现统一 status 视图／渲染，Skill 只调用并展示；删除候选的 Agent 拼表依赖 | 表头恰好六列且固定顺序，同 revision 输出一致；筛选／重启／换机后任务号不变；全文件零写入 |
| BP9-06 | P1；02、03 | 改造 SQLite 备份、白名单跨机快照、Git 传输、祖先与 outbox 栅栏；接入 release／accept／continue | 传输同源可读回，分叉／过期候选拒绝，网络未知只核销，所有旧故障分支有对应新格式覆盖 |
| BP9-07 | P1；03～06 | 更新安装／launcher／reader 特性、包资产、Skill 投影及 INSTALL 恢复入口 | 干净安装、旧 JSON 安装升级、未决 journal、旧程序回退拒绝和用户编辑保护均通过；不先坏掉原安装再依赖其迁移 |
| BP9-08 | P1；03～07 | 稳定候选，执行预定验收集与独立迁移／等价性复核，按影响完成受管子包收口 | 关键迁移缺陷与本轮回归关闭；必要独立证据齐全；收口只覆盖真实受影响包，不触发下游业务回归 |
| BP9-09 | P2；08及后续明确实施授权 | 执行 B 范围：保护原件、冻结真实切换窗口，迁移本机档案和指定遗漏任务，更新真实 BP／Skill | 保留全部已有身份／状态／历史、控制权和暂停偏好；真实 status 包含当前明确 BP 任务，编号固定，原件可恢复 |
| BP9-10 | P2；09 | 将既有私有 GitHub 存档的当前格式切为 SQLite，实际读回并做独立目录恢复核验 | 远端确认准确 commit／快照，恢复前后逻辑身份及业务数据等价；隔离恢复不接管真实 owner，不启动业务或 heartbeat |
| BP9-11 | P2；09、10 | 停用旧在线 JSON／索引，验证本机正常查询与恢复流程，更新唯一文档和证据 | 正常路径不再读写在线任务 JSON；无第二可写事实源；备份及 Git 历史保留；工程交付／用户验收／正式发布分别记录 |

关键依赖为 01→02→03／04，04→05，03＋02→06，03～06→07→08→09→10→11。可以复用有效证据和缓存；不得在迁移／快照规则未稳定时先切真实档案。独立 agent 的使用只在后续明确实施约定及项目迁移独立验证要求下安排，本轮不启动。

### 15.11 验证矩阵与 B 范围验收

| 编号 | 场景 | 必须取得的证据 |
|---|---|---|
| V01 | 六列及真实查询 | 固定表头／顺序、中文／换行／竖线转义、长内容、空清单、暂停／受阻／Unknown、已完成默认隐藏；原生入口读回与 CLI 一致 |
| V02 | 固定序号及事件 | 同时登记唯一、重复提交幂等、任务修订／过滤／取消不改号；重复／乱序／旧 Run 事件不会污染当前任务 |
| V03 | SQL 事务与只读 | 多进程 CAS 冲突、失败回滚、忙超时、缺库／损坏／未知 schema；status 前后全部受测文件摘要和目录成员不变，不建立 journal／索引／配置 |
| V04 | JSON 全历史等价迁移 | 原 HEAD 父链逐项验证、全部对象及证据映射相等，历史 task ID 不变；断链／孤立／重复 key／非法字段／截断输入失败不激活 |
| V05 | 崩溃与回退 | 候选生成、事务提交、后端激活、安装切换、事件入库各阶段故障注入；既有新写入后禁止恢复过时 JSON；权限／链接／占用冲突保护 |
| V06 | 执行与交付门禁 | 原生进程 owner 与项目绑定、暂停／Unknown、completed→Reviewing、开发／发布核验、观察已有对话不重复执行；原规则不退化 |
| V07 | 快照隐私与恢复 | 本机完整备份、白名单远端快照、完整性／外键／领域校验；配置／凭据／本机 outbox 不泄漏，快照丢失／篡改／截断拒绝 |
| V08 | 同步与独占交接 | 快进、回滚／分叉、首次格式切换、网络成功后丢响应、竞争 accept、迟到 push、cancel／fence、未决候选缓存被清理；未确认不释放或接管 |
| V09 | 安装和冷恢复 | 旧 JSON 档案上安装新 reader、激活迁移、Skill 归属／完整性／原生发现、旧版本回退保护；不把包存在当作入口可用 |
| V10 | 真实 B 切换 | 本机当前档案逐对象核对、指定任务回填、真实 status、GitHub 私有库准确快照读回、独立目录恢复、暂停和 owner 保持；无真实业务写入／发布副作用 |

必须做迁移与等价性的独立验证，不能只依赖实现者自测。上述验证是待执行安排，不是本轮通过记录。受管检查优先使用现有 orchestrator 的完整登记检查，按实际新增专项补充；数据库／同步广泛改造需要覆盖该包相关集成，但没有依据就不扩成整个 Forge workspace 或下游业务全量回归。

真实性分层：逻辑双机／本机隔离身份验证、真实 GitHub 往返、本机实际安装、两台物理电脑交接分别记录。B 的最低交付包括真实本机迁移／安装、真实远端 SQLite 快照及独立目录恢复；没有第二台物理电脑时不宣称物理双机通过，保留该项环境缺口，不能用模拟证据冒充。实际新机接管或业务 resume 仍需届时对应的用户指令。

性能只作为改造前后同口径检查：记录 status 延迟、事件提交和按任务数／历史数增长的读取成本，区分缓存命中与冷启动。不预设“SQLite 已更快”，不为这轮架构迁移另组织无边界优化或基准框架。

### 15.12 预算、风险与停止点

规模为 **L**：涉及存储架构、全历史迁移、外部副作用恢复、跨机独占与真实交付。本轮仅规划，不沿用初批三小时授权，也不据历史 release 授权实施本阶段。实施前按 confirm 一次约定时间、token、agent 与验证轮次整体预算；当前不新增预算额度、不启动原生 Goal、不自动并行分派。本计划已纳入迁移独立复核成本，不能为预算省略必要验证。

| 风险／停止条件 | 处理原则 |
|---|---|
| 旧链损坏、归属不明、活动／Unknown、未决同步或其他机器控制 | 停止激活和真实切换，保留实际状态；先核销已有操作，不重复执行 |
| 缺失 task 授权／来源、多个任务归属冲突 | 只阻断该项补录或后续执行；不让 Agent 自动认领所有旧聊天 |
| SQL 与现有领域状态机不等价 | 不部署真实库；先恢复兼容语义，不能删除保护来使测试通过 |
| 只读连接产生文件、schema 自动升级或活库备份不一致 | 视为验收失败，调整连接／journal／快照实现，不能以可恢复为由放宽只读 |
| Git 物理快照差异被误当逻辑分叉，或 outbox 恢复输入不可持久保存 | 停止推送／接管，补齐逻辑修订／祖先与持久恢复协议 |
| 快照包含本机配置／凭据或未授权资料 | 停止上传，修正显式同步白名单；不以“私有库”作为放宽理由 |
| 网络拒绝／结果未知、用户登录或必要设备不可用 | 仅停止依赖该条件的步骤，精确记录已完成部分；不改远端、提权或模拟成功 |
| 新库已写入却要求回退旧程序 | 保护最新 SQLite 及外部结果，拒绝丢数据的旧 JSON 恢复；另定受控恢复路径 |
| 当前候选同包混杂、预算不足或连续实质修复失败 | 按现有 develop／confirm／escalate 规则处理，不扩大范围或无限重测 |

确认结果：三项产品目标、唯一存储及 B 范围已经确认，没有阻断计划落盘的产品问题。后续开工前只补实际实施预算、设备条件和必要权限，不重问 SQLite／JSON 选型、六列表头、用户查询接口或 B 的含义。B 不自动授权产品源码提交／发布、下游业务写入或真正转移到另一台机器的控制权。

### 15.13 文档交付与接续

本轮只增补本唯一计划及 `doc/README.md` 导航，引用既有 TODO-050；不改其原始内容、整体状态或既有验收，不新增重复 TODO。计划内 BP9-01～11 均待执行，未运行迁移测试、构建、业务回归、真实档案改写、GitHub 上传或安装更新。

后续获得明确开工与预算约定后，从 BP9-01 开始，按既有 develop 完成实现、预定验证、必要独立复核和受管子包收口，再完成 B 范围真实交付。各步证据、偏差和恢复输入继续在本文件增量记录，原始测试及收据进入 `.runtime/tests/bridgepilot-status-sqlite/`；关键恢复状态不能只存在可清理日志中。开发完成、工程验证、用户验收和 Git 正式发布仍分别判断。

SQLite 技术依据仅用于实现约束，不代替 BP 验收：[事务语义](https://sqlite.org/lang_transaction.html)、[外键启用及约束](https://sqlite.org/foreignkeys.html)、[一致备份 API](https://sqlite.org/backup.html)。

### 15.14 autopilot 启动准备（2026-11-01，待整体约定确认）

用户后续明确调用 `$autopilot 按照这个计划开始工作吧`。继承 BP9-01～11、固定六列、SQLite 唯一在线状态库及 B 范围，不重做产品需求访谈。前文“只计划”是计划落盘时的历史权限；本次进入启动准备，尚未建立 Goal 或开始业务实施，等待下述整体预算与启动约定一次确认。

- 本次实测：原生 `get_goal` 返回空；工作区为 `D:/Quant/BridgePilot`、分支 main、HEAD `b447e6954e59244d08e37cdc9d396ca8718767fd`，既有脏改动仍保留。Rust/Cargo 1.94.1、Git 2.47.0、gh 2.92.0 可用；受管工厂入口和既有 orchestrator 0.6.0 收口收据存在。实际构建及 SQLite 依赖兼容尚未验证。
- 真实档案再次只读读回 revision 85、五项目、两任务、零 active／Unknown Run，owner 仍为本机，transport pending 文件不存在。已定位个人 home／workspace／Skill 绑定，未转换或更新它们。
- `gh auth status` 在受限沙箱中曾报 invalid；经只读系统凭据环境核验为 GitHub 账号 freakybridge 已登录、keyring 可用。未因此要求重新登录或改凭据；具体私有仓库远端及 SQLite 依赖下载仍需在实际步骤读回核验。
- Hook 注册已读取；入口存在和过去收据不作为本轮自然生命周期 Hook 已触发的证明。当前未手工重放 Hook、安装依赖或运行回归。
- 拟定整包预算 **200000 token**，包含准备调查、主对话、至多两名子 Agent 的工作、实施、独立复核、最多三轮完整验证、合理修复及收尾。预计 6～10 小时，仅估算不设时间硬上限；阶段不重置预算，不创建定时续跑。子 Agent 保持当前模型／effort，不擅自切换。原生 Goal 的子 Agent 统计覆盖尚未验证，须分别说明统计边界，不将主线程读数冒充整包实测。
- 本项目 `.runtime/task-budget/weekly-quota.json` 已不存在；指定的旧产品／安装目录也未找到可复用记录。现仅有已读历史需求文档中用户接受的 `40000 token ≈ 0.6%` 参考（见[周额度需求记录](../weekly-quota-budget/requirements_2026-10-02_weekly-quota-budget.md)）；它没有足够字段证明与当前模型／速度同配置。拟经本次用户明确同意后，把它作为当前任务的 provisional 参考，折算整包约 3.0%，不是实测或官方额度承诺。未获同意前不静默套用、不保存为 calibrated 系数。
- 拟定委派：至多一名按明确文件边界实施的 worker，以及一名独立迁移／等价性 reviewer，主对话整合并执行全部受管操作；不为预检启动子 Agent。正常验证覆盖 V01～V10；三轮按 confirm 的实际闭环口径累计，同一问题连续实质修复失败按现有规则停止盲修。
- 待批准的启动范围包括源码与文档改造、必要构建／隔离验证／子包收口，以及 B 所需的真实个人档案迁移、本机 BP／Skill 更新、既有 GitHub 私有存档库 SQLite 快照推送及恢复验证。后者是用户档案备份切换，不包含本产品源码的 Git 提交／推送／正式发布；不修改下游业务，不接管第二台机器，不恢复 heartbeat。平台对实际写入路径和网络动作仍按工具权限处理，不绕过拒绝。
- 用户批准后建立一个原生 Goal，覆盖同一任务包和整体预算，从 BP9-01 顺序接续；不覆盖其他 Goal、不逐阶段另建目标，不把预算阈值当作逐 token 精确截断。当前没有批准的本阶段 token 额度，也没有启动正式 Goal。

### 15.15 第九阶段首次执行与预算限制交接（2026-11-01）

用户随后明确“同意，按这个约定启动，沿用历史参考估算本轮周额度，开始吧”。本阶段已取得 BP9-01～11 实施及 B 范围授权，整体预算 200000 token，历史参考折算约 3.0%，至多两名子 Agent、最多三轮完整验证；不因此取得产品代码 Git 提交／推送／正式发布、下游业务改动或恢复 heartbeat 授权。15.14 中“待确认／未建立 Goal”为启动准备时历史状态。

原生 Goal 已在本控制对话建立，目标引用本节唯一计划及同一任务包。执行约 13 分钟时平台主动报告 budget_limited，触发时读数 697368／200000；最后一次 `get_goal` 实际读回 **status=budgetLimited、tokensUsed=774367、remainingTokens=0、timeUsedSeconds=810**。这是平台原生计数，不是费用账单；目前无法完整验证其主／子用量归集口径。停止新增实质工作，不新建 Goal、不自行追加预算、不把未完成目标标为 complete 或 paused。

#### 已完成的准备与代码草稿

- 原生 Goal、Rust/Cargo/Git/gh、真实档案 revision 85／无活跃 Run／无 pending、本机登录及旧包收口基线均按 15.14 核验。改动前主要文件与真实档案 HEAD 摘要保存在 `.runtime/tests/bridgepilot-status-sqlite/baseline-hashes.json`；该文件是在添加 rusqlite manifest 行后保存的辅助快照，不冒充全部文件的修改前副本。
- 经用户已批准的协作安排，派出一名 `implementation-worker`，任务名 `/root/sqlite_store`，仅负责 `store.rs`、新增 `database.rs`／`database/`／`migration.rs` 与存储专项测试；主对话负责其余接口整合。预算触发后已发停止消息并调用 interrupt，最后状态为已中断，不继续委派或要求其修复。独立 reviewer 尚未启动。主对话 Cargo 调用已返回退出结果；恢复前仍需核对子 Agent 可能遗留的进程，不认领或终止来源不明进程。
- SQLite 依赖已通过原生 Cargo 下载并锁定为 `rusqlite = 0.38.0`，features 为 bundled／backup／serialize；最初候选 0.41.1 声明 Rust 1.89，发现与项目 MSRV 1.88 不符后已替换，没有提高项目工具链合同。0.38 源码已核对 deserialize_read_exact 等接口，实际 bundled SQLite 与 rusqlite 编译成功；Rust 1.88 本身尚未实测。
- worker 已落 SQLite 规范化实体／历史 delta／持久任务号／本机 local_records 表、只读连接、事务与 CAS、候选快照、旧 JSON 导入／激活／恢复主体草稿。State 仍为内存领域模型，新增空值跳过的 tracking 字段；旧 State 序列化需由后续等价性用例证明未漂移。
- 主对话新增 `tracking.rs`：任务来源／观察模式／暂停与发布暂缓结构、事件载荷摘要及每任务链、注册幂等键、事件与状态同事务接口；新增 `status.rs`：六列确定性视图、固定号、阶段排序／过滤、来源元数据和 CLI 生成的展示文本。`cli.rs` 接入 status、storage-preview／migrate／recover、task-hold 与新能力标签；任务登记改为结构化来源登记；`domain.rs`／`scheduler.rs` 禁止观察模式任务重复派发；`install.rs` 增加按 SQLite reader feature 选择只读迁移预检的候选逻辑。

以上是代码草稿，不是开发完成或功能验证通过。第九阶段源码未执行受管子包收口、升版或产品 Git 同步。

#### 首次编译的实际结果与未完成部分

已执行一次：`cargo check --locked --offline --manifest-path Cargo.toml --target-dir .build/bridgepilot/test-target -p bridgepilot-orchestrator --features test-fixtures`。SQLite C 依赖及 Rust 绑定编译通过，BP 库编译失败（退出码 1），约 50 个诊断主要集中于：

1. rusqlite 0.38 的 `ToSql`／`FromSql` 不直接支持 u64；`database.rs`、`database/entities.rs`／`history.rs` 及主对话 `tracking.rs`／`status.rs` 的 revision、task_no、时间字段需进行**有溢出检查的 i64 转换**。不能用无检查 `as` 或放宽数字校验掩盖问题。
2. `database/history.rs` 的 `put!` 宏在表达式位置展开尾分号，被现有工具链拒绝；需在所属文件修正。
3. 完整诊断包含其他接口／类型错误及未使用 mut 警告，恢复后应将编译输出保存到上述任务报告目录再逐项处理，不能只凭本摘要声称所有根因已知。

尚未实质修复后重测；预定 V01～V10 完整验收轮次尚未开始，不能把 cargo check 当作完整第一轮通过。以下均未完成：

- `codex.rs` 的实时事件持久化尚未接线；报告／开发／发布核验事件及 publication hold 的发布门禁也尚未整合。
- `task-observe` 只出现在 help 清单中，尚无参数路由／原生只读观察适配器，不能对外宣称可用。正式阶段信息不能靠人工填写或模型猜测。
- `sync.rs`／`handoff.rs` 尚保留旧 JSON 文件集协议，**与新 Store 的单 SQLite 导出不兼容**，必须完成快照、旧远端祖先衔接、local_records outbox 与未知结果核销迁移后才允许真实上传。
- `status.rs` 声明的 `scripts/tests/orchestrator/status.rs` 尚未创建；旧存储／portable 等测试仍需按真实新协议更新与新增专项。未执行单元／集成、故障注入、只读零写入、全历史等价性或独立复核。
- Skill 中之前 Agent 拼表的临时候选尚未替换；README／INSTALL／当前架构尚未同步新的已实现行为。安装器兼容性代码只有草稿，未做安装回退保护验证。

#### 文件边界与下一次接续

本次新增／直接修改位于 `source/orchestrator/{Cargo.toml,lib.rs,domain.rs,store.rs,cli.rs,install.rs,scheduler.rs,tracking.rs,status.rs,database.rs,database/,migration.rs}` 与根 Cargo.lock；worker 的存储测试编辑是否已完成以恢复时实际 diff 为准。原先已有的 codex／environment／user_skill、AGENTS、项目配置／账本及其他文档改动保留，不能都计作本阶段成果，也不能 reset。当前分支仍 main，最近保存的基准 HEAD 为 `b447e6954e59244d08e37cdc9d396ca8718767fd`，恢复时重新核验，不把它当作未来提交。

1. 先处理原生 Goal 的 budgetLimited：由用户在受支持入口明确调整／追加预算并恢复，保留同一任务包及累计观察；不要创建新 Goal 清零或未经批准沿用本轮 200000 token。
2. 读取本节，核对实际文件与记录差异、Agent／进程和真实档案／远端／本机安装基线。确认旧 0.6.0 稳定入口仍可用，不执行迁移探针来冒充验收。
3. 先修复首次编译问题，补齐 status 专项测试，再完成事件、观察与 sync／handoff／安装协议整合；SQLite 快照验证应使用 `database::inspect_shared_bytes`，不能允许远端带入 local_records。
4. 按 BP9-08 及 V01～V10 做完整预定验证与独立迁移复核，通过后才做 BP9-09～11 的 B 真实切换及交付。只选择真实受影响包收口，不复用任意旧日志认证当前草稿。

**真实个人档案、GitHub 存档结构及个人 BP／Skill 安装均未切换；没有新上传、提交、推送、正式发布、下游业务改动或 heartbeat 恢复。** 线上仍是既有 JSON／0.6.0 行为。本阶段未完成、未验收；本次仅保存交接，不结算 TODO-050。

#### 用户授权调额至 1000 万及接口阻断（2026-11-01）

用户明确要求“通过app server将token上限修改至1000万然后继续”，指同一 Goal 的总上限 **10000000**，不是新建目标或清零累计。只处理预算管理，预算恢复生效前不恢复代码实现。

已通过本机 Codex 生成的实验协议 schema 核对 `thread/goal/get`／`thread/goal/set`，参数含 threadId、tokenBudget、status 与 origin。使用原生 App Server stdio，以 origin=user、tokenBudget=10000000、status=active 请求修改本控制对话目标，服务返回 `failed to record goal instruction: thread ... already has an active writer`。随即原生 get_goal 读回仍为 budgetLimited／200000，未发生成功调额。

随后尝试官方 `app-server proxy` 连接既有控制入口；只读 `daemon version` 与 proxy 诊断均显示无法连接 `C:/Users/bridg/.codex/app-server-control/app-server-control.sock`，Windows error 10050。没有启动／重启 daemon、抢写原生会话、修改原生数据库或使用自动来源冒充用户调额。最后原生读回仍 budgetLimited，tokenBudget=200000、tokensUsed=800753。接口 schema 位于 `.runtime/tests/bridgepilot-status-sqlite/app-server-schema/`。

继续条件：通过当前持有对话的客户端／可用的受支持控制接口落实用户已批准的 10000000 总上限并恢复同一 Goal，实际读回后接续；不能把用户授权本身当作平台限制已经解除。尚未验证 Goal 面板在本客户端的具体控件位置，不编造界面路径。

### 15.16 调额成功后的实施进展（2026-11-01，进行中）

用户继续明确要求代为修改并接续。原生 App Server 的官方 `thread/goal/set` 请求省略可选 origin 后成功；同一 threadId／createdAt 保留，上限为 10000000、status=active，累计 tokensUsed 未清零。实际 `get_goal` 已独立读回；辅助收据位于 `.runtime/tests/bridgepilot-status-sqlite/goal-budget-update.json`。15.15 的接口失败及暂停为历史状态，已解除，不再要求用户重复调额。

BP9-01～07 已形成候选：SQLite 规范化实体／修订 delta／稳定序号、全旧链原字节导入与恢复、执行事件、固定六列只读 status、绑定聊天的只读观察、共享表白名单快照、SQLite local_records outbox／候选及安装 reader 门禁。Skill 只展示 CLI 的 view.markdown；INSTALL 与当前架构已同步。SQLite 0.38 保持项目 MSRV 合同，未实际运行 Rust 1.88。status 当前仍读取当前实体中的全部 Run／事件来核对摘要，尚未实现仅取最新 Run 列的专用 SQL 投影；不宣称索引查询或性能提升。

已复用一名 worker 完成存储与 portable 专项，并启动一名独立 reviewer。初审发现的暂停决定互相覆盖、Prepared 停止语义、恢复候选合并超限、观察失联展示、大终态载荷及同步读回缺口均已修候选：任务暂停与发布暂缓独立保存；scope 错配拒绝；无尝试 Prepared 可取消，有未知外部尝试保持 Unknown；覆盖前核验完整合并字节；失联保留最后实际观察并显示受限；事件引用同事务完整回执的摘要；发布后实际 fetch 并核对 commit／物理 SHA-256／逻辑 Head。最终复核及整体验证尚未完成。

专项实际结果：此前存储13／迁移5／status5／portable12通过；补充后存储14项中新增超限保护已通过，status8项通过，观察适配器2项通过。一次新增测试错误地在已持 writer 锁的事务中再读取 machine，修正测试取值位置后通过，未改变生产锁保护。首次49项库检查为48通过／1个该测试失败，不能列为全库通过；完整受管检查尚待执行。portable 的沙箱首测受 MSYS 命名对象权限影响，系统环境12项通过，未修改代码绕过权限。

真实个人档案仍为原 revision 85／旧 JSON，BP 与用户级 Skill 仍为原安装，私有远端尚未切换；待最终审查、必要检查及受管收口成功后才执行 B。未提交／推送产品源码、未发布、未改下游业务、未恢复 heartbeat。

### 15.17 首次收口与真实切换窗口（2026-11-01）

受管 `develop --apply --packages orchestrator` 首次返回 development-closed，2组最终候选检查执行通过，orchestrator 0.6.0→0.7.0（旧 JSON 在线协议改为 SQLite 的 0.x breaking minor）；core 因共享锁文件构建覆盖变化仅重验并刷新证据，10.2.0 不变。收据 `development-apply.json`，登记 stdout／stderr 摘要见收口账本；一次验证前因文档漂移拒绝不计通过或完整验收轮次。锁定 Release 构建成功，未提交／推送产品源码。

B 已执行本机 BP 0.7.0 受管安装、真实档案 revision85 的86份旧链迁移、用户级 Skill 更新及原生 skills/list enabled 核验。独立 reviewer 运行只读审计辅助 `scripts/tests/bridgepilot_real_audit.rs`：86份旧文件摘要、完整逻辑 Snapshot、SQL 内保存的原始字节逐项一致，5项目／原2任务／5Run／18Operation、原owner／epoch0／handoff空及wakeup paused保留；数据库字节未变化，见 `independent-live-audit.json`。隔离副本实测旧0.6的status与init均退出2且不改SQLite／不复活revisions；真实install-rollback也拒绝旧reader，当前仍0.7.0。

固定号1、2保留，补登记本BP任务为3，以及来源对话最新重新授权的四下游发布为4～7。原“统一暂缓”已由用户后续“四个下游一起release”及允许Stratus已提交变更的决定覆盖，未重新施加旧hold；均绑定既有控制对话为Observed、不派发或接管。登记／确认来源及耗时见 `real-task-registration.json`，真实在线库推进到revision96；已有任务和历史未改写。

真实观察发现 stdio thread/read 返回 notLoaded，却读到历史 interrupted，当前桌面仍在执行。候选已修：只把实际active的回合当当前状态；其他情况下只读原生Goal，可靠active目标显示“托管目标进行中·观察受限”，无目标显示“观察受限·当前状态未核实”。保留历史回合与计数，不认证停止。新增隔离真实探针已读回本Goal active和历史interrupted，未改真实库；3项观察专项通过。此为兼容修复，待第二轮受管收口及更新安装，尚未上传远端，不能将0.7.0首次检查冒充最终修复验证。

同revision85、同稳定launcher完整进程口径的4次status：旧JSON 963.49／190.95／190.03／222.29ms，新SQLite 54.58／44.51／45.05／47.10ms，数据库逐字节未变。首样本仅进程冷启动，非OS缓存冷启动；本机小档案对比不外推大库性能。5次实际登记（含事件与修订）完整CLI 85.68～107.16ms；后续任务／历史增长后的查询另记，不宣称专用SQL投影优化已完成。

第二轮受管检查及收口通过，候选0.7.1，尚未部署。独立复核进一步指出 active 加历史终态仍不能作为停止证明，以及 Goal active→非active→active 的往返不能被旧事件幂等跳过。最终修正仅允许 active+inProgress；其他保持unknown。观察只对最新事件内容去重，重现旧证据用本次revision生成新事件。3项观察专项已覆盖上述往返、失联恢复后重复零写入与暂停拒绝，最终日志 `observation-final-tests.log`；一轮专项曾因恢复标记字段变化多增一次修订失败，删除该冗余字段后通过。等待独立复核及第三轮最终受管检查后构建最终补丁包，0.7.1仅为中间收口版本，不重复迁移真实档案。

<a id="bp-status-sqlite-delivery"></a>

### 15.18 第九阶段工程交付与接续（2026-11-01，待用户验收）

BP9-01～11 的工程交付及 B 最低范围已完成。原生 Goal 仍沿用本控制对话 ID、createdAt及累计用量，不创建第二目标。共有一名复用 worker 和一名独立 reviewer，未另开用户聊天；三轮受管完整检查均成功，最终候选审查通过。开发完成、用户验收及产品源码发布分别判断，TODO-050及lifecycle不因工程完成自动关闭。

| 范围 | 实际交付与证据 |
|---|---|
| BP9-01～07 | SQLite唯一在线库、固定六列及稳定号、事件同事务、全旧链迁移／恢复、共享快照、持久本机outbox、旧reader门禁和Skill单入口均落地。受管生成的锁及dogfood哈希同步保留，Forge公共Rust源码及共享Skill未增加BP依赖。 |
| BP9-08 | `development-apply.json`、`development-apply-v2.json`、`development-apply-v3.json` 依次 closed：orchestrator 0.6.0→0.7.0→0.7.1→**0.7.2**；core10.2.0仅首轮刷新构建覆盖，版本不变。最终锁定Release构建通过，未组织下游业务回归。 |
| BP9-09 | 真实home及用户级Skill均0.7.2；原生skills/list enabled。86份旧链、原2任务／5Run／18Operation及控制权／暂停偏好完整保留；新增5项仅Observed。固定任务号1～7，已交付1默认隐藏，实际6行。旧0.6 status/init与真实旧reader回退门禁均拒绝。 |
| BP9-10～11 | 真实私有库 `freakybridge/BridgePilotData` main 当前树切为唯一SQLite。独立B审计快照为revision101、commit `85f27c881af93ae7b1bc4aa5a2e18a656bdce84b`；完成状态收尾后最终revision102、commit `da393a172b33e467aabc1c130495f898fd26f6cc`、物理SHA-256 `edc2644323224fc307817fc0302eea6fa7d1341d8aafd11ef27a14e4b833e78a`。两批均实际读回及隔离恢复，没有接管、业务执行或heartbeat恢复。旧原件及Git历史保留，在线HEAD仅退役哨兵。 |

独立最终审计 live／restore 的全部86历史逻辑及原始字节一致，当前state SHA-256均 `048f2f7e658adde7218190d5eefc4acdf8540511ac19e362fcf4a7a7490cc455`，task_numbers／custody／wakeups一致。恢复库 local_records=0，本机2行仅为迁移激活和最后同步收据；真实Git blob、恢复库及上传／恢复收据物理摘要一致。两边审计前后数据库字节未变，Run／Operation仍为原5／18，没有重复派发。owner仍 `machine-b8aeb244a33d208bdbfdcfe4`、epoch0，wakeup paused。

最终增长后revision101／7任务的同口径4次status为49.65／42.00／57.47／40.94ms，SQLite字节不变；六列表头／顺序仍固定。这个小档案结果不代表大库或物理换机性能保证。

#### 实际能力边界

- 四项目来源聊天的原生完整历史超过现有4MiB消息上限，CLI已持久记录 `oversized App Server message` 和观察受限，没有由Agent猜写进展。控制对话可核实Goal active，但独立服务仍notLoaded，表中明确当前回合观察受限。status是SQL中最后已提交事实的只读视图，不扫描聊天、不现场补登记或自动刷新观察。
- 未执行两台物理电脑换机、掉电／网络盘验证或Rust1.88实际工具链测试；隔离新身份恢复与真实GitHub往返不能冒充上述环境。
- status目前读取当前实体中的历史Run／事件进行完整性核对；未交付仅最新列的专用SQL投影，不宣称该优化或全规模性能提升。当前兼容版本仍0.x，未认证稳定1.0。

#### 证据、原件与接续

原件路径由真实迁移收据确定：个人workspace的 `doc/5_project_knowledgebase/bridgepilot/legacy-json-backup/85-53d776df89070598/`。其HEAD／revisions保存原始字节，原自动README也已原字节备份；仅精确匹配旧自动说明时更新当前README为SQLite导航。不得用旧JSON覆盖已新增事实的SQLite，不删除退役哨兵或在途记录来绕过门禁。

本机证据统一在 `.runtime/tests/bridgepilot-status-sqlite/`：三轮受管收口、final构建／安装／Skill发现、real-migration、old-reader-guard、real-task-registration、observe-final、real-sync-upload、restore-verification、independent-live-final-audit及independent-restore-audit。审计工具源码位于 `scripts/tests/bridgepilot_real_audit.rs`，仅RO连接与SELECT；其产物属于可重建测试工具，不是生产依赖。隔离恢复目录没有复制真实机器身份或配置凭据。

保存时分支main、HEAD `b447e6954e59244d08e37cdc9d396ca8718767fd`，原有混杂改动保留；本轮没有产品源码提交／推送／正式发布，没有下游业务写入、原生数据库／Memory修改或heartbeat恢复。其他对话的Forge14发布及下游工作不并入本轮成果；本checkout未擅自合并它们。新增版本、收口与必要锁／manifest／dogfood哈希由受管事务写入，不能归为手工升版。

后续用户可直接 `$bridgepilot status` 试用六列；用户验收留在当前对话，明确验收后才记录验收／TODO。若需正式产品Git同步或发布，仍需对应明确指令。接续本主题时先读本节，核对当前代码与记录差异，在已有授权内继续；本机证据缺失时如实重建必要部分，不把本节HEAD当作未来同步的最终提交。

#### 原生完成标记与最后元数据

工程目标完成后，原生 `update_goal` 实际返回 **status=complete、tokensUsed=1380927／tokenBudget=10000000、timeUsedSeconds=4971**（约1小时23分）；这是该原生Goal的读数，不把未核实的归集口径改称账单。createdAt仍1793523365，未清零累计或新建目标。

随后仅做完成状态的归档：CLI再次只读原生来源，Goal不再返回active（实际proof的goal_status为null），将先前active观察替换为当前执行未核实，没有将Goal结束认证为业务Released。该一条观察使revision101→102，已条件上传并在既有隔离目录下载核对。最终两边 state SHA-256均 `eb8685eb623dba149b66fd15f322fd9fcd73ac5b38c5265093a5285d0e20a99f`，logical head `578a74e9f1a32760b689548c1d40f85365797ab800038c2c1cb5360d10f21a2f`；86旧修订、全部任务号、原5Run／18Operation、owner／暂停偏好继续一致，恢复local_records仍0。收据为 `goal-completion-observed.json`、`completion-sync-upload.json`、`completion-restore-verification.json` 及两份completion审计；前述独立审计对应101，最后元数据由同一已审RO工具再次核验，没有再改源码、部署或启动业务。

<a id="bp-status-sqlite-acceptance"></a>

### 15.19 第九阶段用户验收与发布授权（2026-11-01）

用户在确认本轮开发和B范围工程验证已完成、已告知观察受限及物理双机边界后，明确调用 `$summary accept`，并调用 `$git-sync rrelease`；结合显式release Skill内容，后者按release处理。用户验收本阶段BP9-01～11及三项改造：CLI固定六列、Agent／CLI／SQLite管理分工、SQLite唯一在线状态库，以及真实本机／私有存档／隔离恢复切换。依据为15.18所列受管检查、独立迁移审计、0.7.2真实安装和最终revision102的恢复比对；本次记录不补跑开发验证、不把验收等同于发布成功。

关联 **TODO-050**：本阶段已验收，更新该唯一条目的进展及本验收链接；原总管需求仍含真实缺依赖冷机、双项目业务发布、物理双机、自然唤醒及旧cwd／旧空根收尾等未完成条件，整体保持进行中，文档lifecycle／validation_status继续active／in_progress。未把原范围缺口另拆事项来强行关闭总需求。

保留实际边界：超4MiB原生完整聊天仍显示观察受限；status读取最后已提交事实；物理双机、掉电／网络盘及Rust1.88未实测，专用最新列SQL投影未交付。用户验收不产生新增验证证据，也不自动恢复heartbeat或授权下游业务操作。

本次已明确授权当前项目受管release，包含接入已发布远端变化、核对累计版本覆盖、必要主程序升版／产物、轻量提交检查和提交推送。原有改动保留，不使用手工Git替代受管事务，不在release中补做develop；真实远端接入及最终发布范围以本次受管收据为准。

### 15.20 本轮release接入远端后停止（2026-11-01）

受管预检确认origin/main推送目标为本人产品仓库 `https://github.com/freakybridge/BridgePilot.git`，已具备所需remote-context／development-closure／version-coverage能力。随后执行工厂 `git-sync --sync-remote`，未手工fetch或stash；工具保存本地改动后快进至远端既有Forge14.0.0提交 `45a51bcb1e0ad95397d1936c597b0568d43443e4`，再恢复自动stash时失败，状态 `autostash-retained`、退出2、push_performed=false。该14.0.0属于其他对话已经发布的远端成果，不是本轮新发布。

只读核对发现4个未解决冲突：`.codex/managed-skeleton.json`、`.codex/package-closures.json`、`bridgeforge-manifest.json`、`doc/README.md`；另有原未跟踪的 `doc/1_delivery/forge-downstream-upgrade/plan_2026-11-01_four-projects.md` 与远端已跟踪同路径文档相撞，两个正文不相同，不能直接选一份覆盖。受管自动stash保留为 `stash@{0}`，固定对象 `1e5f5ef40642d9bf6091d2b1a4ab03d2f501550f`；原未跟踪版本仍在其第三父对象中，其余已恢复文件及未跟踪源码保留。当前HEAD与实际origin/main为0／0，但工作区／暂存区未合并，不是发布成功。

按本次指定git-sync Skill的冲突停止规则保留现场，不自动合并、选边、删除stash或重跑同步。未进入release-preview／主程序升版／新提交／推送阶段；当前VERSION14.0.0来自远端。此前0.7.2开发收口基于旧checkout，远端接入后的最终覆盖尚未核验，不能直接复用旧fingerprint做新release。

验收主记录与TODO-050进展已保存，公共CLI `.build/bridgeforge/bin/bridgeforge.exe check todo --root .` 实际退出0且errors为空；工厂check子命令仅支持factory-version，首次误用已纠正，未将其报错当TODO检查通过。索引doc/README.md仍处于本次恢复冲突，summary的索引收口受此阻断。收据位于 `.runtime/tests/bridgepilot-status-sqlite/release-preflight.json`、`release-remote.json`、`release-remote.stderr.log` 及 `accept-todo-check.json`。

继续条件：先明确处理上述冲突和同路径文档的恢复，完整保留两侧账本历史、远端公共变化及本地BP变更；解决后重新核验受管工具与当前覆盖、生成最终release预览和分析。不得在当前未合并状态直接发布，不以当前上游一致冒充本轮已提交推送。

## 当前源码拆包接续（2026-11-02）

初期单包设计和各阶段描述保留为历史记录。当前已授权的六包拆分及实际验证、收口进展统一见 [BP 六包职责拆分](../bridgepilot-package-split/plan_2026-11-02_package-split.md)；本文件源码导航已指向迁移后的职责包。该重构不变更公共骨架分发范围。
