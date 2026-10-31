# {{PROJECT_NAME}} 文档索引

---
delivery_layout: flat # flat | milestone；初始化时确认，已有项目不得静默切换
---

> 唯一索引 — 任何 `doc/*.md` 文件的新增 / 删除 / 重命名都要同步本文件。

---

## 从这里开始

| 你要做什么 | 先读这里 |
|---|---|
| 当前架构 | [`0_architecture/`](0_architecture/)；项目必须在该目录索引当前有效的架构资料 |
| 当前交付 | [`1_delivery/`](1_delivery/)；从 `lifecycle: active` 的交付包继续 |
| 开放 Bug | [`2_bugs/`](2_bugs/)；只把尚未归档的问题作为当前问题 |
| 项目操作 | 根 [`AGENTS.md`](../AGENTS.md) 和 [`3_reference/`](3_reference/) |
| 历史记录 | [`4_archive/`](4_archive/)；历史材料不作为当前运行合同 |
| 项目知识 | [`5_project_knowledgebase/`](5_project_knowledgebase/)；项目自有话题与长期资料 |

项目可以在本表追加自己的当前主线；详细状态只在目标文档维护，不在多个入口重复抄写。

---

## 文档生命周期

需求卡和 Bug 使用 `lifecycle` 表示是否仍属当前工作，使用 `validation_status` 表示验证进度：

| 字段 | 允许值 | 含义 |
|---|---|---|
| `lifecycle` | `active` / `completed` / `superseded` / `archived` | 当前推进、已完成、已被替代、已移入 `4_archive/` |
| `validation_status` | `not_started` / `in_progress` / `awaiting_validation` / `awaiting_user_acceptance` / `verified` | 验证阶段；具体缺口写在正文，不扩展状态词 |

- 新需求卡必须以 `lifecycle: active`、`validation_status: not_started` 开始。
- `completed` 必须已经满足验收条件并取得用户验收；`superseded` 必须同时写 `superseded_by`。
- 只有 `$archive-scan` 在用户确认移动后才能写 `archived`；只有 `active` 事项进入当前交付导航。
- 迁移前的 `status` 或正文状态只作为历史证据；缺少 `lifecycle` 的事项视为 `unclassified`，禁止自动当作 active 或 completed。

### 项目知识库

[`5_project_knowledgebase/`](5_project_knowledgebase/) 用于项目自有的长期知识话题、研究笔记与资料，可以为空；不是软件交付包，也不是 Agent 指令源。全局红线仍进 AGENTS，操作流程进 Skill，机器硬闸进 Hook。

话题目录和正文由项目所有，必须在本索引登记；骨架只提供空目录占位，不接管或覆盖内容。知识资料不要求填写需求验收状态，也不会因日期或完成状态自动进入归档候选；需要整理历史副本时由用户确认具体路径与内容。

已有外部知识库时必须明确唯一事实源；这里可以登记链接或用户确认的历史快照，禁止自动建立第二份持续维护副本。

---

## 目录职责

| 目录 | 放什么 | 状态 |
|------|--------|------|
| [`0_architecture/`](0_architecture/) | 系统当前架构、关键接口、数据流与 ADR | 长期，慎改 |
| [`1_delivery/`](1_delivery/) | 需求从确认、计划到验收的完整交付包 | 活跃 |
| [`2_bugs/`](2_bugs/) | 尚未归档的 Bug：现象、根因、修复和验证 | 活跃 |
| [`3_reference/`](3_reference/) | 外部资料与 BridgeForge 公共参考 | 只读 |
| [`4_archive/`](4_archive/) | 已完成归档 | 只读 |
| [`5_project_knowledgebase/`](5_project_knowledgebase/) | 项目自有知识话题、研究笔记与资料 | 长期维护，可为空 |

完整边界见根 `AGENTS.md` §2.3；长 SOP 由项目自行放入 `doc/3_reference/`。

---

## 0_architecture/

项目待办使用 `0_architecture/TODO-INDEX.md`；确有获准事项时按 todo 合同创建并登记入口。confirm 正式落盘记账，summary accept 更新状态/收口，查询只读，不支持删除条目。字段与状态合同随受管 Rust 源码分发，可用 `bridgeforge check todo --contract` 查看；初始化不创建空总表。

<!-- TODO: 列入 PRD / Roadmap / 核心需求文档
| 文件 | 说明 |
|------|------|
| `system-overview.md` | 系统边界、核心模块与运行方式 |
| `data-flow.md` | 关键数据流 / 控制流 |
| `interfaces.md` | 关键接口与数据契约 |
| `adr/ADR-*.md` | 跨模块、替代成本高的架构决策 |
-->

## 1_delivery/

<!-- TODO: 每个 topic 是完整交付包。布局由本文件 frontmatter 决定：
| 文件 / 子目录 | 说明 |
|------|------|
| [`feature_x/`](1_delivery/feature_x/) | flat：中小项目的需求包 |
| [`M1/feature_x/`](1_delivery/M1/feature_x/) | milestone：按里程碑组织的需求包 |

每个需求包至少包含确认卡 `requirements_*.md`、`plan.md`、`acceptance.md`；正式 debate 放入 `debates/`。`M1/README.md` 只汇总状态和链接，不维护第二套验收勾选。
-->

## 2_bugs/

<!-- TODO: 每个 Bug 记录发现、复现、根因、修复、验证和回归，例：
| 文件 | 说明 |
|------|------|
| `BUG-001_<topic>.md` | 轻量 Bug 记录 |
| `BUG-002_<topic>/` | 需要正式 debate 或附加证据的 Bug 包 |
-->

## 3_reference/

| 文件 | 说明 |
|---|---|
| [`codex-hook-signals.md`](3_reference/codex-hook-signals.md) | Agent 原生主动澄清的响应边界、例外和调试方法；自动 Clarify / Focus Hook 已退役 |
| [`project-rust-hooks.md`](3_reference/project-rust-hooks.md) | 项目自有 Rust Hook 的源码、注册、锁定构建和事务边界 |

<!-- 外部资料必须记录来源、获取日期和适用范围。 -->

## 4_archive/

<!-- TODO: 已完成的 delivery 保持原 milestone/topic 层级归档；已解决 Bug 归档至 bugs/。 -->

## 5_project_knowledgebase/

<!-- 项目在此登记自己的话题链接；本话题索引不是骨架受管区。 -->
