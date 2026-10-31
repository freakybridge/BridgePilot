# 骨架社区对标与问题清单

日期：2026-10-24。本文是本轮只读调研的证据与建议快照，不是开发需求卡、运行时规则或验收记录。

## 当前进度与冻结点（2026-10-24）

用户要求“暂时冻结。先存档当前工作进度”，并调用普通 `$summary`。本次保存进度，暂停继续讨论与实施；不表示用户验收、TODO 结算、Git 保存或将文档移动到归档目录。下文调研授权与建议保留为历史快照，后续决定以本节及关联实施记录为准。

| 事项 | 冻结时实际进度 |
|---|---|
| 整体骨架对标 | 已完成调研及六项问题落盘；保留总体架构的判断不变 |
| TODO-026：迁移恢复冲突 | 用户已选择退役历史迁移功能；本地实现、模板/dogfood/分发登记和工程验证完成，用户尚未验收，详见[退役记录](../1_delivery/legacy-asset-migration-retirement/requirements_2026-10-24_retirement.md) |
| TODO-027：公共 AGENTS 分层与精简 | 尚未逐项展开、未采纳或实施 |
| TODO-028：用户级覆盖与个人定制 | 尚未决定产品受众及策略、未实施 |
| TODO-029：confirm 预算依赖 | 尚未采纳调整、未修改合同 |
| TODO-030：Hook 保证边界 | 尚未推进梳理；保留已有交付和自然生命周期验证边界 |
| TODO-031：模型行为对照 | 尚未启动；静态用例检查不等于模型行为验证 |

冻结时位于 `main`，本轮代码和文档仍保留在未提交工作区，新增记录尚未纳入 Git；未提交、推送、发布、更新用户级安装或写入真实下游。原 TODO 总表状态未随普通总结改动，最新决定在本文与退役记录中保存。

恢复须由用户明确提出。届时先核对工作区与已有记录是否变化，复用仍有效的验证证据；按用户原先“一项项给我看”的顺序，下一项是 TODO-027。恢复讨论本身不授权修改 AGENTS，也不自动授权验收或发布。

本次仅做文档保存和轻量核对，不重跑测试、构建或 smoke。原生 Memory 索引按本主题关键词检索无相关命中，未修改 Memory；没有新增 Rule／Hook／AGENTS 候选。本主题尚未用户验收，无归档候选。

## 用户目标与授权

用户希望判断按自身经验编写的用户级与项目级 AGENTS 是否合适，并参考 GitHub 类似骨架。范围已经扩展为 AGENTS、Skills、Hooks、安装分发和升级治理，同时考虑 Agent、版本发布及 Memory 的边界。

用户先选择“只在对话里完成调研，不写盘”，随后明确要求“所有问题汇总落盘。然后一项项给我看”。本次授权仅覆盖调研记录、待办登记和索引；不表示采纳建议、授权修改骨架、安装第三方项目、更新用户级配置或真实下游、提交、推送、发布。

本轮传播四问：属于元文档；记录的是当前工厂与通用产品的对比，不分发为公共约束；不写版本和 CHANGELOG；不需要同步 dogfood。后续实施时须按每项实际改动重新确定传播范围。

## 结论与证据边界

建议保留总体架构，优先处理说明冲突、规则分层、流程负担及行为验证。没有证据支持整体替换为某个社区骨架。

已完成：读取两份 AGENTS、相关 Skills、安装和架构说明；抽查 Hook 调度器、用户指令资产合同和 project-sync 指纹复核代码；阅读社区文档及部分指令、脚本源码。

未执行：第三方安装、功能测试、性能对照、真实下游写入、自然 Hook 生命周期验证、独立模型行为 A/B。社区功能是已阅读资料中的声明；源码抽查不等于端到端保证。外部链接指向调研时的活动页面，未固定 commit，后续采纳前须核对是否变化。

调研预算沿用本轮 M 级：累计上限 45 分钟、估算 20,000 token、0 个子 Agent；token 未实测，缺少可靠周额度换算记录。资料核验不等同于实现后的验收轮次；不报告虚构的实际 token 消耗。

## 外部参照

| 参照 | 已核实的做法 | 可借鉴内容与边界 |
|---|---|---|
| [Trail of Bits Codex Config](https://github.com/trailofbits/codex-config) | 全局原则、Skills、配置、执行规则和 Hooks 分开；已有全局 AGENTS 可选择覆盖、跳过或人工合并 | 最接近 Codex 配置骨架；不照搬其组织偏好、Bash Hook 或平台选择 |
| [Spec Kit 定制指南](https://github.github.io/spec-kit/guides/customization.html) | 区分集成、扩展、预设和项目覆盖；模板覆盖有明确优先级 | 参考默认与定制的边界，不引入整套插件系统作为前提 |
| [Spec Kit 升级指南](https://github.github.io/spec-kit/upgrade.html) | CLI 和项目文件分别更新；安装清单识别本地修改，普通更新保护项目资料 | 支持显式归属和定制保护的方向；不据文档认定其事务保证等同于 BridgeForge |
| [OpenSpec](https://github.com/Fission-AI/OpenSpec) | 按变更组织提案、需求场景、设计和任务，提供探索、实施、归档等入口 | 参考需求追踪和任务选择流程；不能替代安装升级治理 |
| [Ruler](https://github.com/intellectronica/ruler) | 单一指令源、目标分发、预览、备份及撤销 | 参考分发体验；备份撤销不自动等于失败原子回滚，多工具支持不是当前必需目标 |
| [官方 AGENTS 指南](https://learn.chatgpt.com/docs/agent-configuration/agents-md) | 全局和项目指令沿启动目录层级加载 | 保持原生机制；目录索引不能充当动态加载器 |
| [官方指令与 Skill 精简指导](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra) | 复核常驻指令必要性、按需读取资料、缩小 Skill 触发范围 | 为有界精简提供依据，不证明任何删减一定改善当前项目 |
| [官方 Hooks 文档](https://learn.chatgpt.com/docs/hooks) | 区分事件和结果语义；PostToolUse 不能撤销已发生副作用 | 保证必须对应具体触发阶段和运行环境 |

## 值得保留

| 范围 | 保留方向 | 本地依据 |
|---|---|---|
| 指令分层 | 用户偏好、项目公共约束、项目自有内容分开；公共区和项目区归属明确 | [原生指令架构](../0_architecture/design/codex-native-instruction-architecture.md)、[用户模板](../../source/forge/templates/user/AGENTS.md)、[项目模板](../../source/forge/templates/AGENTS.md) |
| Skills | 按需读取、小任务直接执行、复杂任务分支处理；复用已有授权和证据 | [develop](../../source/forge/templates/skills/develop/SKILL.md) |
| Hooks | 机器可判检查程序化，公共与项目 Hook 分开，事前检查和事后反馈分开 | [注册](../../source/forge/templates/hooks.json)、[调度器](../../templates/hooks/src/lib.rs) |
| 安装升级 | 显式归属、哈希、应用前指纹复核、定制保留、失败恢复 | [设计](../0_architecture/design/codex-project-sync.md)、[源码](../../templates/hooks/crates/bridgeforge-core/src/project_sync.rs) |
| 验证发布 | 区分实现、工程验证、用户验收和 Git 发布；区分主程序、子包与骨架版本 | [develop](../../source/forge/templates/skills/develop/SKILL.md)、[项目模板](../../source/forge/templates/AGENTS.md) |
| 资料和 Memory | 项目资料归项目所有；原生 Memory 不作语义改写 | [原生指令架构](../0_architecture/design/codex-native-instruction-architecture.md)、[项目模板](../../source/forge/templates/AGENTS.md) |
| 平台与 Agent | 保持 Windows/Codex 定位，默认主对话执行；无需求不扩展多平台或更多 Agent | [README](../../README.md)、[项目模板](../../source/forge/templates/AGENTS.md) |

## 问题总览

总表只维护摘要和办理状态，本文维护本轮证据与建议。以下六项均等待逐项讨论，不表示已经批准实施；未改动产品规则。

| 展示顺序 | 编号 | 分类 | 问题或候选 | 本轮判断 |
|---|---|---|---|---|
| 1 | TODO-026 | 明确问题 | 迁移中断恢复说明互相矛盾 | 优先统一事实源与说明 |
| 2 | TODO-027 | 调整建议 | 公共 AGENTS 常驻内容与操作细节的分层 | 有界精简，不按行数硬删 |
| 3 | TODO-028 | 条件性建议 | 用户级统一覆盖与个人定制的取舍 | 先明确产品受众，再决定是否改变 |
| 4 | TODO-029 | 调整建议 | confirm 的周额度估算形成额外前置依赖 | 控制预算，但不让校准阻断原任务 |
| 5 | TODO-030 | 保证边界与验证候选 | Hook 阻断、反馈、辅助维护的保证需分别说明 | 不据此直接增删 Hook |
| 6 | TODO-031 | 证据缺口 | 已有行为用例尚不能证明 AGENTS 的实际效果 | 复用现有用例取得模型对照证据 |

## TODO-026 迁移中断恢复说明冲突

2026-10-24 后续决定：用户明确要求直接退役此历史功能，不采纳仅统一说明的原建议。实施范围与验证由[退役需求记录](../1_delivery/legacy-asset-migration-retirement/requirements_2026-10-24_retirement.md)维护；下列内容保留为退役前调研快照，旧迁移手册链接不再代表可用入口。

- 退役前事实：原 `skills/bridgeforge-codex/references/project-asset-migration.md` 第 7 项允许在核实完整迁移包、授权、源 hash、目标原值和最新基线后，继承未变化的已确认决定；该文件已按后续授权移除。
- 冲突：[INSTALL](../../INSTALL.md) 的“项目升级与异常诊断”和[同步设计](../0_architecture/design/codex-project-sync.md) 的迁移说明仍写“中断后从第一个源重来”。
- 影响：不同入口可能指导 Agent 重复确认同一事项。已确认文档矛盾；未通过真实迁移复现用户行为。
- 建议：采用“核实后继承有效决定”的口径，流程以迁移 Skill 为主来源，安装与设计保留说明或链接。来源、授权、目标或基线变化时，仅重核受影响部分；不得从摘要推定删除授权。
- 拟涉及：INSTALL、同步设计及必要引用；是否涉及产品 Skill 由逐项决定后的范围核验确认。本次不修改这些文件。
- 后续核验：对照未变化恢复、授权撤回、源或共享目标变化、证据不完整四类情况；保持全部确认前零写入。
- 状态：已记录；建议等待用户采纳，未实施、未验收。

## TODO-027 公共 AGENTS 有界精简

- 事实：[项目模板](../../source/forge/templates/AGENTS.md) 调研时约 12 KB，覆盖授权、调查、验证、工具、文档、版本和 Memory。长度本身不能证明过重或导致性能问题。
- 建议承载边界：常驻 AGENTS 保留授权、行为保留、证据和项目定制保护等原则；具体发布能力检查、版本公告和操作步骤交对应 Skill/机器合同；安装恢复步骤交 INSTALL；可机判要求由配置和检查器拥有。
- 外部依据：官方精简指导及 Trail of Bits 的跨任务原则与专业流程分离做法。
- 风险：删减可能丢失必要边界，或造成 Skill 未触发时无规则可用；仅将内容搬到 reference 不保证成本降低。
- 后续核验：逐条说明防范的错误、触发任务、唯一 owner、迁移后入口；与 TODO-031 联动验证，不预设目标行数或 token 节省比例。
- 状态：等待用户采纳；未写入产品规则；未实现。不是已经证明的性能 Bug。

## TODO-028 用户级统一覆盖与个人定制

- 事实：[用户模板](../../source/forge/templates/user/AGENTS.md) 调研时约 1.6 KB，主要为语言和沟通偏好；[资产合同源码](../../templates/hooks/crates/bridgeforge-core/src/user_agents.rs) 使用 `backup_then_replace`；[INSTALL](../../INSTALL.md) 明示个人编辑会在后续维护中恢复为标准版。
- 判断：若主要服务用户本人跨电脑统一配置，当前设计可以保留。若面向其他用户或团队，应评估保留个人偏好或明确选择统一管理；备份只能支持恢复，不能让定制持续生效。
- 参照：[Trail of Bits 安装 Skill](https://github.com/trailofbits/codex-config/blob/main/.agents/skills/install-codex-config/SKILL.md) 对已有 AGENTS 提供覆盖、跳过或人工合并的选择。
- 未决：产品受众及希望采用的所有权策略；不得把社区差异直接定义为本地 Bug。
- 后续核验：首次安装、无变化更新、个人修改、显式接受统一覆盖、现有 override 等场景；任何策略变更另行确认传播及兼容影响。
- 状态：等待用户判断是否采纳；未改变安装策略。

## TODO-029 confirm 预算流程的非必要依赖

- 事实：[confirm](../../source/forge/templates/skills/confirm/SKILL.md) 与[周额度估算](../../source/forge/templates/skills/confirm/references/weekly-quota.md) 将预算输出关联到账户、模型、速度、计量口径和系数；无适用依据时要求补问参考。本轮缺少本机系数记录，未编造百分比，也没有实际阻断调研。
- 风险判断：估算前置可能将用户任务转为额度校准访谈；本轮证明依赖缺失，不证明所有任务都会被阻断。
- 建议：保留范围、时间、Agent 和验证预算；有可靠计量时报告实测，其他情况标记估算；周额度占比仅在可靠依据可用或用户确有需要时提供，缺少系数不阻断原任务。
- 参照：Spec Kit 的独立任务入口、OpenSpec 的探索/提案入口支持按任务选择流程，但并未提供适用于本账户的额度换算方案。
- 历史关系：[周额度既有需求](../1_delivery/weekly-quota-budget/requirements_2026-10-02_weekly-quota-budget.md) 仍是原决定来源；本条为重新评估候选，不能自动推翻已确认规则。
- 后续核验：简单咨询、明确只读调研、已授权开发、缺系数和已有系数的路径；避免重复确认、虚构消耗或静默扩大预算。
- 状态：等待用户采纳；未修改 confirm 或预算合同。

## TODO-030 Hook 保证边界与验证

- 事实：[调度器](../../templates/hooks/src/lib.rs) 将 pre-tool 边界检查、post-edit 检查、post-shell 收据及 Stop 辅助工作分开；[官方 Hooks 文档](https://learn.chatgpt.com/docs/hooks) 明确事后事件不能撤销既有副作用。
- 建议：分别说明事前阻断、事后反馈和辅助维护的保证；每项对应实际事件、失败语义、注册与工具覆盖。不要把程序内拒绝路径扩大为宿主一定触发、一定阻断的保证。
- 参照：[Trail of Bits Hook](https://github.com/trailofbits/codex-config/blob/main/hooks/block-dangerous-command.sh) 明确定位为护栏而非安全边界，不能因为代码更短就替换现有 Rust 检查。
- 历史关系：TODO-024 已有 Hook 正确性与性能交付，保留其验收；[原记录](../1_delivery/runtime-hook-hardening/requirements_2026-10-24_runtime-hook-hardening.md) 的自然生命周期缺口继续按原边界说明。本条只承接对外保证口径和覆盖梳理，不重复登记原工程开发。
- 后续核验：优先复用已有有效收据；另行授权后，针对具体触发、误报、漏报和开销补证据，不默认完整回归或真实下游写入。
- 状态：等待用户决定梳理范围；没有证据支持本轮直接增删任何 Hook。

## TODO-031 复用行为用例验证 AGENTS 效果

- 事实：[现有用例](../../scripts/tests/fixtures/gpt6-behavior-cases.json) 已定义 personal_only、skeleton_only、combined 组合与重复确认、无关读取、工具调用、验收和越界等指标。
- 证据限制：[对应 Rust 测试](../../scripts/tests/src/distribution_regressions.rs) 明确只校验用例结构、唯一 ID 和引用，不验证模型行为；既有[交付记录](../1_delivery/gpt6-skeleton-upgrade/requirements_2026-09-05_block4.md) 的模型行为边界应保留。
- 建议：复用现有用例，在独立现场与上下文中固定模型、effort、工具、任务和初始文件，记录加载证据，比较现版与候选版。当前长对话不能冒充隔离 A/B。
- 指标：重复确认和无关读取是否减少，授权任务完成率是否保持，越界和错误完成声明是否增加，同时观察工具调用、耗时及可获得的实际用量；单次成功不代表稳定效果。
- 历史关系：TODO-005 是 develop 安装后的 Q1–Q5 真实行为核验，范围不等同于本条的 AGENTS 指令组合对照；可复用相关有效证据，不改原条目状态。
- 风险与成本：真实模型运行有额外用量与环境要求，须另行确认范围和预算；不新建评估框架作为前提，不以静态测试代替模型实测。
- 状态：证据缺口已记录；未启动评测，未宣称现版或候选版更优。

## 逐项讨论与后续边界

展示顺序按 TODO-026 至 TODO-031，每次先说明一项的事实、影响和建议。用户对方向的认可与实施授权分别记录；没有明确授权不改骨架。

建议的处理先后是：统一说明冲突；建立/复用行为基线；按证据精简常驻指令；结合产品受众决定个人定制和流程可选性。展示顺序不等于已批准的实施顺序。

暂不建议整体更换骨架、替换 Rust 技术栈、扩展多平台/多工具、引入完整插件体系或增加常驻规则。没有本轮归档操作或归档候选。
