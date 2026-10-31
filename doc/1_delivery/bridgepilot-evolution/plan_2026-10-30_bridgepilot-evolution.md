---
lifecycle: active
validation_status: in_progress
---

# BridgePilot 产品演进与跨项目总管开发计划

关联原需求：**TODO-050**，见[项目待办总表](../../0_architecture/TODO-INDEX.md)。

日期：2026-10-30。规模：**L**，涉及架构扩展、源码与分发路径迁移、持久状态、执行控制及产品身份迁移。

本文件是本主题唯一的需求与开发计划，后续实施、验证、偏差和验收增量在此维护，不另建一份重复需求卡。

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
| [产品分发清单](../../../bridgeforge-manifest.json) | 源路径、技能登记、目标、稳定资产 ID 与哈希需要一致维护 |
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
