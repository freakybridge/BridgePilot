---
lifecycle: completed
validation_status: verified
---

# summary 与 git-sync 共用流程精简计划

日期：2026-10-28。范围：`summary`、`summary accept`、`summary switch`、`git-sync`、`git-sync release`。

**当前状态：2026-10-29 用户明确调用 summary accept，已验收本计划，并独立调用 git-sync release 授权受管提交、推送和主程序发布。工程证据见第 15.3 节，验收边界见第 16 节；实际发布结果以受管收据为准。**

## 1. 决定、授权与计划边界

用户先确认普通 summary 的精简方向，要求“先落盘，暂不实施”；随后要求按建议精简三者的额外流程，并重新生成 plan，说明现行流程、共用部分与共用流程的精简方案，可复用的附加要求移入共用流程。

本版按两组工作流统一整理：summary 三模式的取舍见第 4 节 A01–A12；git-sync 两模式的最新决定见第 9 节 G1–G12，后者取代对话中此前尚未收敛的建议。前期仅获准落盘方案；用户随后确认整体预算并明确“开始吧”，已授权本轮实现、必要验证、投影及受管子包收口，进度以第 13 节为准。不提交、推送、主程序发布、用户级安装、真实下游写入或修改 Memory；不改变 TODO-044 的既有验收。

独立 accept 入口仍未决定，本计划不新增命令。summary 与 git-sync 各自保持独立授权和结束条件，不自动串联。用户已明确要求 G1 退役同步前置工具准备、G2 退役日常同步独立 self-test、G5 保留 SKILL.md 的省略额外预览约束、G10 收敛为计划与提交内容一致性、G11 保留必要失败处理并退役并行同步防护；G3/G4/G6/G7/G8/G9/G12 按前述精简建议纳入本版。

## 2. 三者现在如何工作

现状依据为本轮读取的 `templates/skills/summary/SKILL.md` 与其四份 references；下表描述现行指令要求，不代表每项成本已单独实测。

| 环节 | summary | summary accept | summary switch |
|---|---|---|---|
| 入口 | 根据当前对话确定总结主题 | 根据当前对话确定验收交付 | 根据当前对话确定交接任务 |
| 普通总结：三者必读 ordinary-mode | 提取经验、查索引与相关文档、去重、核对已有证据、更新 doc | 同左，现行 accept reference 要求在完成普通经验沉淀后进入验收 | 同左，现行 switch reference 要求在完成普通经验沉淀后进入交接 |
| 附加流程：三者必读 deep-steps | 有界检索 Memory，命中后读正文并按需追溯；结合当前约束核对；评估规则建议和归档候选 | 同左 | 同左 |
| 模式专项 | 无 | 对照验收条件和收据，记录用户决定及真实状态，维护相关 TODO；必要时补最小正式决定记录或记录已明确发现的新独立问题 | 记录目标、决定、授权、进度、证据、阻塞、下一步、Git 基准、未提交改动及必要环境；autopilot 时补任务与预算等最后已知状态 |
| 发布准备耦合 | 项目启用发布准备时核对记录类文档与 prepared 复用影响 | 普通流程之外，验收 reference 再要求写完后核对发布准备记录 | 继承普通流程，并记录跨电脑不随 Git 携带的本机依赖、产物与缓存缺口 |
| 收尾 | 返回经验文档和限制；deep-steps 还要求说明无建议或无归档候选 | 返回经验、验收及 TODO 变化和限制 | 读回交接完整性，返回链接、接续提示及尚未同步说明 |

现有流程已经禁止为总结、验收重新测试、构建、审计或 smoke，也不授权自动 Git 同步；这些保护继续保留。现行材料存在职责重复，不等于每次 Agent 都实际重复执行全部动作。

## 3. 目标：一条共用主流程，模式只补必要内容

### 3.1 共用执行路径

原五项职责“明确范围、读取资料、核对事实、更新记录、检查并返回”均保留，合并为三个执行阶段。三者一次整理、合并写入、一次收尾，不先完整跑 summary 再重新开始 accept/switch。

| 阶段 | 三者共用动作 | summary 的必要内容 | accept 的专项增量 | switch 的专项增量 |
|---|---|---|---|---|
| C1 整理 | 复用当前对话、已读文档和有效证据，一次确定范围、定位文档、去重并核对关键事实；仅缺口、矛盾或适用指令触发补查 | 有信息增量的经验及其依据 | 本次验收条件、用户决定、相关 TODO；合同已读且未变则复用 | 本次接续必需的状态、Git 基准、未提交改动、必要环境缺口及已有授权 |
| C2 落盘 | 按模式合并更新既有主文档，只补增量；必要时才新建文档和更新索引；区分事实、推断及未验证项 | 触发条件、已确认原因、有效方法、失败尝试、适用边界 | 验收范围、完成依据、保留限制和相关 TODO 状态及链接 | 已完成/进行中、已确认决定与授权、阻塞及恢复条件、可执行下一步；引用已有需求和证据 |
| C3 检查并结束 | 检查本次内容、差异及受影响的链接和索引，返回实际结果及必要限制后结束 | 返回文档链接；无增量时引用原文并说明 | TODO 实际变化时做一次必要一致性检查，返回实际变动及影响验收的保留项 | 确认交接能独立读懂，返回接续提示和真实同步边界 |

共用的是执行方法与必要保护，不是把三种模式的字段取并集。普通 summary 不因此读取整套验收资料或采集 Git/环境状态；accept 不因此生成交接清单；switch 不维护 TODO 验收状态。

### 3.2 复用与停止规则

- 已有经验直接引用，没有新经验时不强制重写经验文档；accept/switch 仍可完成本身有增量的验收或交接记录。
- 已有相关文档和仍适用证据直接复用，实际缺口才补查；不因为切换模式、reference 或阶段重复读取。Git 状态等易变化事实在需要时读取当前快照，不能用过时状态冒充当前事实。
- 有信息增量优先补原文，不强制按经验、验收、交接分别新建三份文档；已有独立交接文档可继续更新，不机械合并或移动历史文档。
- 仅对本次记录及其直接影响做轻量检查；TODO 更新保留项目要求的一次只读一致性检查，不绕过现有 Hook，不把必要检查当作重新开展工程验证。
- 无增量且已有记录准确完整时，不写文件，返回已有链接；验收或交接有实际缺口时说明缺口，不把不完整记为完成，也不自动补做开发或发布。
- 本次必要记录及检查完成即结束；不追加仓库审计、长期规则治理或发布准备。

## 4. Forge 附加要求的统一取舍与共用位置

必要或有条件复用的项目移入 C1–C3，只维护一份规则；模式 reference 仅保留专项差异。移入共用流程不代表三种模式每次都要执行。

| 编号 | 当前项目 | 取舍 | 精简后的承载位置与触发条件 |
|---|---|---|---|
| A01 | Memory 检索与历史追溯 | 保留按需补查，取消默认独立研究任务 | 共用 C1：本次关键事实、历史决定或约束存在缺口时补查，复用已有阅读；会话/平台明确要求仍须遵守，证据足够即停，不改写 Memory |
| A02 | 核对 AGENTS、Rule、Hook 等约束 | 保留与本次相关的核对，取消全面审查 | 共用 C1：只核对当前记录实际涉及或存在冲突的约束，不借总结扫描整套规范 |
| A03 | Rule/Hook/AGENTS 候选评估 | 移出默认任务，不主动搜集候选 | 共用 C3 的可选提示：整理过程中已有明确、稳定、重复的问题时才简短建议，不另做历史研究，不强制完整候选模板，不自动写规则 |
| A04 | 归档候选扫描 | 移出默认任务，扫描与移动交给 archive-scan | 共用 C3 的可选提示：本轮偶然发现明确候选时可以提示，无候选无需报告；不自动调用归档流程 |
| A05 | prepared/发布准备及缓存复用核验 | 取消作为三者前置与完成条件 | 不另设共用检查；本轮已知且影响理解的状态可随 C2 如实记录，归相应流程处理。switch 仅保留实际接续所需的依赖或产物缺口，不建立 prepared 硬门槛 |
| A06 | 事实、推断、验证状态区分 | 必要，保留 | 共用 C1/C2：核对已有证据并准确表述；用户接受与工程验证分别记录，不补造测试结论 |
| A07 | 文档定位、去重、索引、链接 | 必要，保留 | 共用 C1/C2/C3：一次定位、增量写入、检查本次变化，模式分支不重复实现 |
| A08 | 验收、TODO 与正式决定记录 | accept 必要或条件必要 | 复用 C1/C2/C3 的定位、证据、写入和检查方法；accept 只定义验收条件、合法状态及关联编号。必要时才补最小决定记录或去重记录已明确发现的新独立问题，不主动找新问题 |
| A09 | Git 基准、未提交改动、接续环境与授权 | switch 必要 | 复用 C1/C2/C3；switch 只定义需采集的接续字段及相对路径要求，不重复文档定位和核对流程。读取状态不等于同步或修复环境 |
| A10 | autopilot 任务、预算、Goal、进程/Agent 状态 | switch 条件必要 | 仅交接此类任务时纳入 C1/C2，保存最后已知事实和恢复边界，不重建 Goal、重启进程或重置预算 |
| A11 | 先完整 summary 再进入专项 | 取消重复串行执行 | 三者直接进入同一 C1–C3，按模式加必要内容，经验已覆盖则引用 |
| A12 | 固定空结果、无关 TODO 未变等汇报 | 取消 | 共用 C3：仅报告实际文档/TODO 变化、影响结论的限制及必要接续信息；不机械输出“无建议、无归档候选”等 |

## 5. summary 后续实施任务、影响文件与传播

本节定义本轮 summary 实施边界，按已确认授权执行。

| 任务 | 优先级 | 修改目标 | 拟影响文件 |
|---|---|---|---|
| P1 共用入口与主流程 | P0 | Skill 入口只路由模式并链接唯一共用流程；沿用已有文件，避免为三个模式新增独立框架 | `templates/skills/summary/SKILL.md`、`references/ordinary-mode.md`；ordinary-mode 改承载三模式共用 C1–C3，保留路径以减少迁移 |
| P2 专项只保留差异 | P0 | accept/switch 删除重复定位、经验沉淀、通用核对及收尾规定，只保留必要字段、专项条件和停止点 | `references/acceptance-mode.md`、`references/switch-mode.md` |
| P3 附加职责归位 | P0 | 按 A01–A12 移入共用路径的条件分支；取消 deep-steps 三模式必读及重复事实源 | `references/deep-steps.md`；先核对消费者，已无独立用途且引用可安全迁移时删除，不留平行主流程 |
| P4 合同与分发一致性 | P1 | 核对 TODO 操作合同实际要求，保留必要一致性保护；检查引用和分发登记，同步实际存在的镜像 | `templates/skills/todo/references/contract.md` 仅真实冲突时调整；实际资产登记和引用按发现结果处理，不假定存在项目级 summary 镜像 |
| P5 有界验证与交付 | P1 | 验证最终流程符合第 6 节，报告未验证边界；依实际产品改动完成受管收口 | 实际适用的既有文档、Skill metadata、链接和分发检查；不为 Skill 文案机械运行 Rust 全量 |

传播四问：本次 plan 与索引属于元文档，仅记录方案，无版本、CHANGELOG 或 dogfood 刷新。未来 Skill 调整属于通用产品能力，事实源在 templates，不下沉工厂专属事实；依实际改动与受管合同处理版本、CHANGELOG、分发及存在的 dogfood。用户级共享安装、其他项目写入或正式发布不包含在本次授权中。

风险与停止点：移除 deep-steps 前检查其全部消费者；不可把验收变成只改 TODO 状态，不可把换机文档当作环境或进程已恢复，不可用共用流程增加三模式各自不需要的采集工作。本节 summary 改造不包含 AGENTS/Hook 行为修改；超出本节范围的扩展须单列影响，不顺手实施。git-sync 的 G1 已明确涉及工厂 AGENTS 的同步前置准备条款，按第 11 节限定调整，不因该授权改动其他 AGENTS/Hook 规则。

## 6. summary 验证方案与完成标准

后续以实际流程覆盖而非减少步骤文字作为验收依据，优先复用适用证据，不新建长期证书、缓存或必填状态框架。

| 场景 | 预期行为 |
|---|---|
| 普通总结有新经验 | 一次 C1–C3，更新原文或必要新文档及索引，不读取验收或交接专属资料 |
| 无新增经验、重复调用 | 引用准确已有记录，不强制写文件或重跑历史研究 |
| 总结后再 accept | 复用经验和证据，补验收/TODO 增量；必要一致性检查通过，不重复完整 summary |
| 验收有 blocker 或必要证据缺失 | 保存真实决定与缺口，未满足项不标完成，不自动重测或补做 develop |
| switch 与 autopilot 交接 | 保存足够接续的信息、真实 Git 基准及必要状态；不验收 TODO、不同步、不假定进程/预算跨机恢复 |
| 历史事实缺口或会话要求 Memory 检索 | 遵守要求并有界补查，结果适用才引用，不因取消 Forge 默认研究而跳过原生要求 |
| 已有明确规则问题或归档候选 | 仅按需简短提示，不开启专项扫描、规则写入或归档 |
| 文档无关输入变化、已知发布状态影响 | 不组织发布准备；准确记录相关限制，必要工作交回对应流程 |

完成标准：三模式使用同一共用路径，专项仅保留必要差异；附加要求按表归位；没有重复经验写入、默认治理扫描或发布准备前置；必要记录、TODO 及交接保护仍有效。性能改进以同场景端到端复测为准，不把计划或文字精简宣称为实测提速。

## 7. git-sync 两模式的目标与现行流程

普通 `git-sync` 将授权范围内的变化提交并同步到实际推送目标，不主动升版；`git-sync release` 在同步基础上，按累计未发布变化生成主程序版本及正式发布记录，核验子包版本覆盖并公告累计变化，不再次升子包。无实际变化时不制造提交或版本；工作区干净不等于没有累计未发布变化。

现状依据：`templates/skills/git-sync/SKILL.md`、`references/runtime-context.md`、`references/schema4.md`、`source/workflow/src/git_command.rs`、`source/workflow/src/git_sync.rs` 与 `source/lifecycle/src/release/schema4/`。下表区分 Agent 外层要求与受管入口内部动作；各条件分支并非每次全部执行。

| 现行环节 | 普通 git-sync | git-sync release | 当前共用情况 |
|---|---|---|---|
| 工具前置 | 项目声明准备入口时先准备；工厂 AGENTS 明确要求当前源码准备，再独立 self-test 和 preflight | 同左，另核对发布能力 | 已共用，但准备和独立自检被放进日常同步路径；preflight 已可返回工具身份和能力 |
| 范围审阅 | 读取状态及真实 diff，生成提交消息和效果摘要 | 另审阅最后正式发布以来的累计差异、子包覆盖和主程序兼容性 | 两模式共用 Skill 文档，普通同步容易读入发布专项内容 |
| 发布预览 | 不执行 | 无分析预览、Agent 生成分析、可选带分析预览；runtime reference 与主 Skill 对是否必做带分析预览有冲突 | release 专属，但可能重复完整核验 |
| 内部执行起点 | 获取 Forge 同步锁，检查未完成事务及已有提交保护状态 | 同左，并在 fetch 前构建发布计划和采集输入状态 | 同步锁共用；release 有独立预先计算 |
| 远端接入 | fetch，检查分叉，必要时 stash、快进并恢复改动 | 同左；之后判断前面的发布计划是否还能复用，不可复用则重算 | 已共用，远端更新可能使早先分析失效 |
| 写入和提交 | 需要时准备受管产物、做 baseline 等检查，add、记录待提交 tree、commit；Git 自动执行 pre-commit | 加主程序版本、原生载体、发布记录的写入及必要产物处理 | 共用事务与提交路径，夹有多个状态/输入检查 |
| 提交后与推送 | 核对提交及仓库状态，工厂再次核对 baseline，必要时 push，生成收据 | 同左，加版本与累计子包公告 | 共用；提交的 tree 固定，工作区后续编辑不会改变既有提交 |
| Agent 收尾 | 独立核对一次工作区、HEAD、实际推送目标及同步状态 | 同左，追加版本变化结果 | 已共用，成功后不应继续扩展调查 |

预览只读取并记录内容摘要，不锁住源码。暂存区仍可变化；commit 成功后，该提交的文件内容才成为固定对象。现行“同一宿主进程”要求是执行约束，不能等同于锁住文件。

## 8. git-sync 共用主流程的精简方案

### 8.1 目标执行路径

由受管入口承接共用机械操作，主对话负责范围、消息与 release 的必要语义分析。不把下表实现为 Agent 手写的多条 Git 命令，也不要求每个阶段另开一次对话或额外预览。

| 阶段 | 两模式共用 | 普通 git-sync | release 专项增量 |
|---|---|---|---|
| S1 一次预检与整理 | 使用已安装的项目受管工具，聚合仓库、分支、目标、状态及工具信息；复用已有授权，审阅实际变化，准备消息与效果摘要 | 不读取子包收口及完整发布合同 | 识别需审阅的累计范围；只读取发布必要规则 |
| S2 接入远端更新 | fetch，判定分叉；需要时保存本地改动、快进并恢复；失败按第 10 节处理 | 更新完成后按最终差异继续 | 把会改变源码的远端接入安排在最终发布分析之前，避免分析完再正常拉入新代码 |
| S3 确定最终操作 | 核对最终待同步范围；无变化则不制造提交 | 跳过发布计划，直接进入提交 | 一次必要预览返回累计变化、版本覆盖及判级约束，完成主程序分析；实际 release 内部校验后立即进入必要写入，不默认再做外层带分析预览 |
| S4 写入并提交 | 做必要自动写入、产物处理和既有轻量提交检查；确认待提交内容符合审阅范围及计划内自动变化，生成 commit | 不主动修改业务版本 | 只更新主程序版本、对应原生载体及正式记录；必要构建按真实发布输入决定，子包不再次升版 |
| S5 推送、核验并结束 | 推送确定的提交；一次独立终态核验，对照收据报告提交、实际目标、工作区、ahead/behind 与 stash 状态；一致即结束 | 返回同步结果及效果摘要 | 同时完整公告主程序及累计子包版本变化，未变包可汇总 |

S2 提前及预览/执行衔接是待实施的受管编排调整，不是现有 CLI 已支持的承诺。后续实现须在既有受管职责内衔接，不能以退化为手工 Git、伪造指纹或新增长期证书/缓存框架替代。输入真实变化时只重新处理受影响计划；无变化时不因阶段、命令进程或宿主更换机械重跑完整分析。

### 8.2 共用内容与模式边界

- 共用 S1–S5 中的仓库状态读取、授权/消息处理、远端接入、提交、推送、必要失败处理和终态收据；共用的是同一实现和规则，不把 release 检查塞入普通同步。
- 将 G7 的输入读取与内容一致性、G9 的实际异常分支、G11 的必要失败处理、G12 的终态核验归入共用流程。G8 的子包覆盖与 G6 的累计判级保留为 release 专项，复用底层历史读取与解析能力。
- G1、G2 是退役项，不移动到另一个阶段或改名恢复。同步入口不再先准备/刷新工具，也不独立运行 self-test；工具缺失、损坏或不匹配时报告维护缺口，不在同步中自动安装升级。安装/升级流程本身的编译与自检不在退役范围。
- G1 不等于禁止发布本身因版本/源码变化而需要的产物构建。后者属于 S4 的发布结果与实际交付物一致性，不是为了启动 git-sync 而提前构建工具；不得借此重新引入所有调用固定准备。
- G10 以“分析结果 → 必要版本/记录写入 → 提交内容一致性确认 → commit → push”为短路径。保留足以证明提交内容与计划一致的校验，退役强制同一宿主进程及仅因换宿主整段重来的规则；实际源码、配置或必要环境差异影响计划时才重新核验。
- 不重新运行开发测试、全量回归、独立审计或用户验收；没有子包覆盖时报告并停止 release，不自动补做 develop 或退回普通同步。
- 一次核对提交对象与计划后，避免为了证明同一提交内容而反复扫描工作区；仍需确认实际推送对象和终态，不能把“提交内容固定”误解成 HEAD、分支或远端状态永不变化。

## 9. G1–G12 最终处理表

本表记录用户已授权实施的最新取舍，覆盖对话中此前较保守的建议。

| 编号 | 最终处理 | 共用位置或专项归属 |
|---|---|---|
| G1 同步前置工具准备 | 退役普通同步和 release 的固定准备/刷新步骤；工厂 AGENTS 对应前置条款与 Skill 同步修改。安装升级编译独立保留 | 从正常主流程删除；S1 使用已安装工具，不能自动改走 Cargo 引导 |
| G2 独立 self-test | 退役日常两模式的独立调用，复用 preflight 已有工具信息；安装升级或显式诊断的自检保留 | 共用 S1，不另设能力自检轮次 |
| G3 发布规则混入普通同步 | 拆出 release 与异常规则，普通同步只读共用必要部分 | 共用入口路由；release 专项及条件异常 reference 按需读取 |
| G4 多次完整预览 | 默认一次必要预览加实际 release 内部执行校验，取消外层重复带分析预览及无变化重算 | release 的 S3，复用捕获结果；只读查看完整计划的独立需求可用额外预览 |
| G5 主入口与 reference 冲突 | 以 SKILL.md“通常省略外层重复带分析预览”为准，修正 runtime-context 的必经要求 | 一份共用执行规则，其他文档链接或只写差异；同一宿主约束另按 G10 处理 |
| G6 版本限制晚返回 | 首次必要预览提供最低主程序级别、累计 breaking 原因、接受依据缺口，避免错误分析完成后才返工 | release 的 S3，复用已有累计账本与判级计算 |
| G7 多处全量核验 | 保留关键操作边界的必要一致性；相同输入的历史读取、解析、摘要计算复用；区分工作区、暂存区和提交对象，消除真正重复的检查 | 共用 S3/S4/S5；不新增永久认证层，不把所有异常假设重新放回主路径 |
| G8 子包版本核验 | 保留 release 必需的当前内容、依赖/构建覆盖、历史连续性及明确交付物核验；按影响范围执行 | release 的 S3；普通同步不执行，底层读取解析可共用 |
| G9 配置/归属/权限异常 | 改为实际遇到才进入的分支；配置补正仅在适用授权与已核实边界内处理，恢复后继续受影响步骤 | 共用异常分支；正常路径不重复配置、不预演所有恢复情况 |
| G10 长链与同一宿主约束 | 按第 8 节短路径精简，保留计划到提交的一致性；取消强制同一宿主及仅因换进程整段重来，内容真实变化才重算影响部分 | 共用 S4 的内容确认，release 绑定 S3 计划；避免引入新“代码锁定”框架 |
| G11 并行同步及失败防护 | 明确退役 git-sync 的并行同步防护，不把并发同步列为支持场景；保留第 10 节必要失败处理 | 共用执行与失败分支；不删除 Git 自身原生锁，不误删其他业务消费者需要的共享锁 |
| G12 过长收尾 | 一次独立终态核验，与收据一致即返回；矛盾才追查 | 共用 S5，release 公告是结果输出，不再扩大到仓库审计 |

## 10. G11 保留哪些失败处理

G11 不作为不可拆的“安全包”。提交前 Hook 校验是正常检查，不是失败恢复；当前用户没有授权整体删除现有 Hook 的业务检查，不通过绕过 Hook 实现提速。

| 编号 | 场景 | 处理边界 |
|---|---|---|
| H1 | 同仓库并行 git-sync | 退役 Forge 为此设置的互斥/等待及专属恢复处理，不再保证并行同步正确性；Git 自身 index 等原生锁不改 |
| H2 | fetch、鉴权或快进失败，出现分叉 | 报告具体错误，停止依赖动作，不自动 rebase/merge 或改变历史 |
| H3 | 保护本地改动后恢复发生冲突 | 保留 stash 和冲突现场，不自动选边覆盖或丢弃原有改动 |
| H4 | 版本、发布记录、必要产物写入或提交前检查失败 | 按本次自动写入边界恢复，避免半套发布状态；不能安全恢复时保留现场说明原因，不覆盖用户原有修改 |
| H5 | commit 成功但 push 失败 | 保留本地提交，报告尚未完成同步；不删除提交、不重做开发或自动回滚已提交版本 |
| H6 | 推送目标、提交或终态与收据矛盾 | 不报告完成，针对具体矛盾核查，不再盲目推送或强推 |
| H7 | 受管 Hook 报错或产生异常内容变化 | Hook 报错即停止提交；实际提交偏离计划时不得按成功推送。与 G10 共用最小内容一致性判断，不另做通用 Hook 改写恢复框架 |

不能把“退役并行同步防护”扩大为删除所有名字带 lock/guard 的实现。当前 `lock_factory_runtime` 还服务 factory_runtime 与 develop 的执行/恢复；实现时须拆分消费者，在同步路径退役对应保护，保持本轮未授权改变的构建与开发行为。共享 API 是否重命名或拆分由实际调用关系决定，不保留换名后的并行 git-sync 保护。

## 11. git-sync 后续实施任务与传播

本节是本轮 git-sync 实施计划。内容涉及工作流和发布事务，不能仅修改 Skill 就声称行为已改变。

| 任务 | 优先级 | 修改与影响范围 | 验证重点 |
|---|---|---|---|
| Q1 入口与文档统一 | P0 | `templates/skills/git-sync/SKILL.md`、references/runtime-context.md、schema4.md、first-use.md；工厂根 AGENTS 中与 G1 直接对应的准备前置句；INSTALL 等实际引用按发现结果同步 | 普通路径无准备与独立 self-test；G5 一致；无强制同一宿主或完整重启；不删除安装升级/显式工具刷新入口 |
| Q2 共用同步与异常职责 | P0 | `source/workflow/src/git_command.rs`、`git_sync.rs`、`git_sync_preflight.rs` 及实际写入计划/辅助模块 | 调整 S2/S3 顺序和受管接口衔接；两模式共享最短路径；缺工具报告而不自动准备；真实错误按 H2–H7 保留现场 |
| Q3 release 计划与核验复用 | P0 | `source/lifecycle/src/release/schema4/plan.rs`、analysis.rs、inputs.rs、development.rs 及 `source/lifecycle/src/release_history_cache.rs` 中实际热点 | 首次预览完整返回判级限制；输入未变时复用；实际变化只重算相关部分；版本覆盖、声明交付物和历史仍正确 |
| Q4 同步互斥退役与内容保护归并 | P0 | `source/workflow/src/git_sync.rs` 的 SyncLock、共享 lock_factory_runtime 消费者及相应 fixture | git-sync 并行防护及专属测试退役；构建/develop 共享消费者语义不受损；G10 与 H7 不重复设置恢复框架 |
| Q5 有界验证、投影与版本收口 | P1 | 相应 `scripts/tests/**`、公共源码投影 `.codex/hooks/source/**`、实际分发登记及文档 | 覆盖第 12 节；按实际改动受管收口，验证传播和 dogfood，不修改真实下游或安装共享副本 |

传播四问：当前仅改元文档和索引，不改版本或 CHANGELOG。未来 git-sync 行为与 Rust 优化属于通用产品层，源码在 source、Skill 在 templates，公共源码按真实合同投影；G1 在根 AGENTS 的工厂准备条款调整属于工厂自身配置，不能把工厂细节下沉公共模板。届时按受管合同处理受影响子包及发布说明；主程序发布、用户级安装、真实下游写入需相应授权，不包含在本次计划落盘中。

G1 对应条款调整已经由用户本次取舍及实施授权覆盖，不再次要求确认是否退役 G1。若发现必须删除未讨论的 Hook 检查、改变子包升版分工或扩展其他模块行为，单列实际缺口，不能借简化自动扩大范围。

## 12. git-sync 验证与本轮状态

| 验证场景 | 预期 |
|---|---|
| 普通同步有改动/无改动/仅有未推送提交/需要快进 | 正确提交或同步，允许 no-op，不升版；不执行 release 覆盖检查 |
| 工具正常、缺失或不匹配 | 正常路径无独立 self-test 和前置准备；缺口明确停止，不隐式安装构建 |
| release 包含累计 breaking 改动 | 首次预览给出主程序最低级别与原因，不再先接受错误 minor 分析后返工 |
| 工作区干净但有累计未发布变化 | 仍正确计算发布范围，不把 clean 当作 nothing-to-release |
| 远端有新提交 | 在最终分析前完成必要接入；后续若再次实际变化则正确停止或重算，不使用过期计划 |
| 输入和必要环境相同，仅命令宿主不同 | 不因此强制重做整段发布；真实影响计划的变化仍被识别，不把更换进程当作内容变化 |
| 版本写入/检查/提交失败，恢复冲突或 push 失败 | 分别符合 H2–H7，失败不冒充成功，用户改动和已生成提交得到正确保留 |
| 并行同步保护退役 | 检查专属入口与测试确实移除；不宣称仍支持并行同步，不破坏其他消费者或 Git 原生锁 |
| 确定提交后的推送 | 推送对象对应计划；不为证明同一提交内容反复扫描工作区；实际目标及终态仍核实 |
| 性能比较 | 使用同场景基线，分别记录 Agent 编排、发布预览、必要构建、commit/Hook、网络和总耗时；没有数据不承诺耗时上限 |

优先做对应定向验证与真实计时，公共事务改动仅在有具体跨模块风险时扩大测试；复用仍适用证据。既有源码有其他任务未提交改动，实施前检查其边界，不覆盖或把已有成果计入本计划。

此前阶段仅完成 plan 与索引。本轮用户确认整体预算并明确“开始吧”，已授权当前仓库实施、必要验证、传播与受管子包收口；仍不提交、推送、主程序发布、用户级安装或真实下游写入。

## 13. 本轮实施记录

开工：2026-10-28 20:17（Asia/Shanghai）。整体上限 4 小时、约 80k 新增 token（估算，含子 Agent）、最多 1 个独立审查 Agent、3 轮验证；token 未实测。需求、授权和预算已确认，不按阶段重开。

承接 develop 已交付候选：主程序 8.0.1、factory 0.4.0、lifecycle 0.5.0、workflow 0.2.0。保留核心清理/develop 的已有工作区修改；本轮只计新增差异。开工前使用已准备工具核对 workflow/lifecycle/factory 范围，不重复 self-test 或为读取状态重建工具。

验证安排：summary 的相关模式、链接和分发检查；git-sync 的远端接入/分析顺序、一次计划构建、同输入跨宿主、判级提示、必要失败处理及共享锁消费者；受影响 package 的定向检查、独立审查、最终产物核对。性能以相同夹具的进程/计划调用数及耗时对照，真实下游和自然对话端到端未执行则标未验证。

### 当前实现与首轮验证

- summary 四份入口/reference 已沿 C1–C3 收敛，deep-steps 已退役并从显式分发登记移除；普通总结、验收和交接只补自身增量。
- 两种 Git 模式复用同步实现；新增受管 --sync-remote 仅 fetch、必要快进及恢复工作，不提交/推送/升版。release 在接入后分析并以 --skip-fetch 执行，内部只构建一次计划。同步锁已从 git-sync 路径移除，维护锁消费者保留。
- 首次预览新增 product_constraints，给出累计影响的最低版本级别及 breaking 来源/接受要求；一次历史 Reader 贯穿输入捕获、版本覆盖及计划，不缓存活动 HEAD 或跨调用复用旧计划。
- release 排除明确终端会话/追踪元数据，其他环境继续绑定；项目显式声明的变量由 release_gate 另行绑定。develop 的严格宿主合同保持。
- G1 工厂条款、INSTALL、git-sync Skill 与版本合同已统一；独立 self-test 和额外带分析预览不再是日常前置。受管 manifest 已重建，相关投影/镜像已同步，尚未安装用户级共享 Skill。
- 修改前同方法基线：既有 release_reuses_plan_only_when_fetch_preserves_all_planning_inputs 单线程测试 1 passed，测试主体 168.73 秒；含夹具准备及三个实际同步调用，不等于一次用户 release 耗时。最终相同场景复测待执行。
- 首次 cargo check（workflow/lifecycle/cli/factory --tests）通过。远端接入两个新增场景最终 2 passed；第一次只有换行断言失败，修正夹具本地 autocrlf，不改变生产内容处理。
- 同步模块首轮 37/40，通过覆盖包括 Hook 拒绝回滚、输入漂移、实际目标、push 失败；3 项失败为旧 Mock 多算已退役锁调用和旧 schema 错误文案，已适配。随后 42 项回归取得 31 passed / 11 failed；失败时出现系统内存与子进程启动错误，未视为通过，详见暂停记录。
- 不可变历史读取 4 项通过；跨进程宿主元数据与真实构建变量场景 2 项通过，子进程探针在父用例中实际执行，不把单独 ignored 探针算失败；显式声明同名终端变量用例实际执行时 Git 历史查询失败，仍待验证。累计 breaking 的 stable/0.x 两项已通过；完整定向组 5 passed / 1 failed，失败场景与资源故障时段重叠，需串行重跑。
- 独立 review-auditor 首轮发现无关宿主环境字段过度失效、旧行为夹具降级预期两项，均已修复；同一 Agent 最终增量复核无新增确定问题。静态复核不替代尚在运行的工程检查。

## 14. 用户暂停记录与恢复入口（2026-10-28 21:15）

### 14.1 暂停范围和现场

用户明确要求“暂停。把当前工作进度落盘至 doc”。已停止实施、测试、构建与收口推进，仅保存本记录和索引；未验收、不结算 TODO、不提交推送或发布。

- 仓库：D:/Quant/BridgeForgeCodex；分支 main；暂停时 HEAD 为 `64ee6ba9fa87ba1d3d5a2252eb5a768927585ffd`。工作区仍包含前轮核心清理/develop 及本轮修改，均保留，不能整体回退或将全部 dirty 计为本轮成果。
- 本轮开工约 20:17，暂停约 21:15，经过约 58 分钟。约定仍为 4 小时、约 80k 新增 token、最多 1 名独立审查 Agent、3 轮验证；已用 1 名审查 Agent，token 未实测。恢复沿用已用预算与有效证据，不重新从零计。
- 主程序 8.0.1、factory 0.4.0、lifecycle 0.5.0、workflow 0.2.0 保持前轮版本。本轮尚未执行 develop apply 或追加包版本记录。
- 暂停时 `.runtime/bridgeforge-codex/develop-transaction.json` 不存在。本轮未进行真实仓库的 commit、push、release、用户级安装或真实下游写入。
- 受管构建 exec 会话 50202 尚在运行时收到暂停。核对命令与创建时间后，仅终止该任务的 Cargo PID 16488（2026-10-28 20:56:19 启动）及其后代，包含 30352、13208 和当时子编译进程。会话已退出，exit=1；这是用户要求中断，不是构建成功。
- `.codex/bin/build-receipt-cli.json`、factory、hook 的最后写入时间仍为 20:11:18 左右，早于本轮开工。没有本轮 prepared 终态收据；不能将已编译的 bootstrap 或半途缓存称为最终运行时。保留缓存和临时产物，未清理或伪造收据。

### 14.2 已完成的实现

| 范围 | 本轮实际变动 |
|---|---|
| summary | SKILL.md 与 ordinary/acceptance/switch references 共用 C1–C3；删除 deep-steps，移除其分发登记；已有经验后的 accept/switch 只处理增量 |
| git-sync Skill | S1–S5、G1/G2/G5/G10/G11 已按决定调整；first-use、schema4 和 runtime-context 同步，保留 develop 已交付合同 |
| Git 受管实现 | git_command.rs 新增 --sync-remote；git_sync.rs 共用远端接入、删除同步专属互斥和 fetch 前计划/全仓快照复用层，实际 release 在接入后只构建一次计划；构建/develop 锁消费者保留 |
| 发布分析 | schema4/plan.rs 首次预览 product_constraints 与共用最低级别逻辑；单个历史 Reader 贯穿捕获/覆盖/计划，已知只读装饰可复用不可变对象 |
| 跨宿主 | inputs.rs 仅 release 排除明确终端追踪变量，其余环境保守绑定；显式项目变量仍由 release_gate 绑定，develop 宿主模型未改变 |
| 配套 | CLI/factory 能力声明、相关单元/行为夹具、根 AGENTS 的 G1 专属条款、INSTALL、分发 manifest 及公共源码/git-sync 镜像已更新 |

后续无需重做这些实现；先检查暂停后是否有新差异，再沿剩余验证推进。summary 尚未安装到用户级，不能宣称其他对话已经使用新共享副本。

### 14.3 验证结果及资源事故

| 检查 | 实际结果与边界 |
|---|---|
| 编译检查 | workflow/lifecycle/cli/factory 的 cargo check --tests 通过；不是正式 release 产物交付 |
| 新远端接入 | 2 项通过，覆盖脏/未跟踪文件保留、快进恢复及不 commit/push/升版；初次失败是隔离夹具 CRLF 预期，已固定夹具本地 autocrlf |
| 同步模块首轮 | 37/40；3 项为退役锁后 Mock 次数/顺序及旧 schema 错误文本，已修正 |
| 同步模块后续轮 | 31/42；11 项失败时出现 Windows 1455、无法创建进程、Git/sh 资源不可用等错误。不得算通过；待串行复跑名单见下 |
| 跨宿主 | 2 项通过：不同子进程终端元数据变化可复用，实际自定义构建变量变化拒绝；ignored 是由父用例实际调用的子进程探针 |
| 显式项目变量 | release_host_explicit_project_variable_still_invalidates_analysis 执行于资源故障时段，初始化阶段 Git 历史查询失败，未取得通过 |
| 累计 breaking | 6 项中 5 项通过，含 stable/0.x 最低级别断言；schema4_all_breaking_lifecycle_operations_require_explicit_acceptance 报 cannot inspect Git content normalization，需复跑区分资源影响与逻辑问题 |
| 不可变历史缓存 | 4 项通过，覆盖只读装饰复用、活动 HEAD 不缓存、替换历史、部分克隆/错误批次边界 |
| 文档与传播 | skill-metadata issues=[]/warnings=[]；project-structure errors=[]；本次两 Skill 本地引用存在；diff --check 通过；受管 manifest 重建后 --check 返回 changed=false |
| 独立审查 | 同一 review-auditor 两次只读审查；首轮 P2/P3 已修，最终增量复核未发现新的确定问题。不代替最终测试/构建 |
| 性能 | 修改前单线程同场景 168.73 秒；修改后并发测试已证明实际 release 的 plan.build 调用为 1，但受并发负载影响，尚无有效同条件耗时对照，不宣称端到端提速 |

资源事故：并行 release 构建与 Git 测试时，系统提交内存约 40.3/41.5 GB；编译器报告 memory allocation failed，Git/Windows 报页面文件不足（1455）及进程创建失败。用户已保存并关闭 MATLAB，之后读取约 27.9/34.8 GB，余量约 6.9 GB。未修改系统页面文件、ACL 或关闭其他用户应用。后续执行安排改为构建与测试串行，构建限定 CARGO_BUILD_JOBS=1；暂停后恢复须重新核对资源，不假定余量仍相同。

失败同步用例（此前有些已通过，按最终源码与实际失败边界选择串行复跑，不把旧通过直接替代本次失败）：

```text
direct_release_ignores_missing_invalid_failed_or_stale_development_state
direct_release_uses_current_deletion_or_reappearance_without_development_gate
explicit_policy_syncs_twice_then_releases_clean_history_once
explicit_release_rejected_hook_restores_native_versions_log_and_index
linked_worktree_uses_its_own_index_and_common_identity
ordinary_sync_does_not_acquire_runtime_maintenance_lock
ordinary_sync_never_bumps_even_with_missing_or_legacy_config
phase_timing_retains_tls_timeout_and_launch_failures_without_retry
phase_timing_success_preserves_commit_hook_and_remote_receipt
preflight_blocks_clean_filters_before_running_project_helpers
preflight_rejects_truncated_attributes_and_submodule_helper_scope
```

测试受管入口使用根 Cargo.toml、scripts/tests/factory-cargo.toml、--locked --offline、对应 package --lib，复用目标目录 `.runtime/source-workspace-refactor/cargo`。恢复时单线程执行资源受影响项，保留实际结果；临时 .git 在沙箱不可写时使用已有明确证据窄提权，不改 ACL。未修改源码的环境重测不另计实质修复轮次，但仍计时间与成本。

### 14.4 下一步（仅用户明确恢复后执行）

1. 读取本节和当前 diff，核对其他对话是否又改了相同文件、包版本或合同；沿用已有决定与预算。确认没有遗留构建进程及 develop journal。
2. 核对可用内存，以单构建任务完成受管运行时准备并取得真实终态收据。被中断的命令为：

   ```powershell
   $env:CARGO_BUILD_JOBS='1'
   cargo run --locked --release --manifest-path Cargo.toml --bin bridgeforge-factory --jobs 1 -- build-assets --factory --project-root .
   ```

   这是已授权开发的必要构建，不是恢复已退役的日常 git-sync 前置；不换用未受管工具或绕过产物一致性检查。缓存可复用但先核对现场，不能把本次中断当 prepared。
3. 处理 factory 检查输入登记：当前 `.codex/bridgeforge-version.json` 的 factory-unit.inputs 仍包含已删除的 `templates/skills/summary/references/deep-steps.md`，会影响收口。已准备 `.runtime/sync-simplification/closure-config-proposal.json`，仅删除这一条，proposal 不含 baseline；**尚未 plan/apply，当前配置未由本轮改写**。恢复时重新对照当前配置，通过受管配置 plan/apply 核对只改这一条、保留版本/包登记/发布基线，不手改 baseline。若忽略目录文件丢失，可依当前配置重建这个单路径提案。
4. 串行重跑上述失败项、显式项目变量用例与 breaking 生命周期用例。新增 remote-only 非法 CLI 组合、分叉和 stash 冲突场景已在后续轮通过，可按输入未变复用。
5. 用已准备的新 CLI 做隔离仓库实际链路验证，覆盖预检 → --sync-remote → 零写入预览 → --release --skip-fetch 的提交/失败/重复行为；当前只有源码级/临时夹具调用证据，实际新 CLI 全链路尚未验证。真实下游与实际项目 Git 发布不在授权内。
6. 无其他并发负载时，用与修改前一致的单线程性能场景复测，区分夹具总耗时、工具内部与自然对话端到端。不为收尾追加无关全量回归。
7. 工程验证稳定后，使用新 develop 的任务范围/direct apply 完成实际受影响 workflow/lifecycle/factory 及必要连带包的受管收口。基线版本见 14.1，具体级别以实际差异和用户已接受的退役决定生成分析，不预写版本或旧证据指纹。
8. 完成最终投影/manifest、必要版本后产物与零写入一致性核对，更新本卡为待用户验收并交付。安装、真实下游、提交推送或主程序 release 仍需各自授权。

恢复提示：读取本文件第 14、15 节，先核对当前分支、代码与记录差异，沿用本轮已确认范围、取舍和剩余预算，从剩余受管构建及后续验证继续；未取得真实终态收据前不得宣称开发完成。

## 15. autopilot 接续（2026-10-28）

用户在当前对话明确授权沿本计划继续开发并采用 autopilot，随后指定接续预算为 10,000,000 token。原生 Goal 已建立；覆盖本次主对话、必要审查、验证、合理修复与交付，原生计量是否包含全部委派成本尚未实测，不声称精确计费或整包计量已验证。前轮 token 未实测，历史成本保留，不将其记为零；本次额度为用户重新明确授权的接续额度。没有可用的本机周额度换算记录，周额度百分比未估算。时间仅估算，不设时间硬上限；在途调用和收尾可能超过原生停止阈值。

本次保持原任务和文件边界：受管登记/投影与构建、资源故障项串行复测、隔离 CLI 全链路、同条件性能对照、workflow/lifecycle/factory 及真实必要连带包收口、同一文档交付。复用已有独立审查；新增实质修复确需复核时最多一名审查 Agent。失败升级规则保留；同一候选环境补跑不重计实质验证轮次。不提交、推送、主程序发布、用户级安装、真实下游写入或修改 Memory，不自动验收。

接续预检：

- 当前工作区为 `D:/Quant/BridgeForge`，`main`，HEAD `83969378f87720469805d07f254fd5ae1d6036c5`，开始时干净。暂停成果已由后续提交保存，不再按未提交修改处理；混合提交中的既有核心清理/develop 成果不计为本次成果。
- 当前无原生 Goal 冲突、无 develop journal。只读进程核对未发现 cargo/rustc/bridgeforge 构建进程；提权只读内存查询显示约 12 GiB 可用物理内存、15 GiB 可用虚拟内存。后续构建与测试串行，构建任务数为 1。
- `.codex/bin/bridgeforge-factory.exe build-assets --factory --project-root . --check` 返回 manifests stale；需先核对并受管重建派生内容，不能冒称 prepared。Hook 注册文件及本地产物存在；仅证明静态配置，未模拟原生事件。
- `.runtime/sync-simplification/` 和此前配置提案在本机不存在；factory-unit.inputs 仍引用已退役 deep-steps，按当前配置重新生成单路径提案并经受管 plan/apply，保留 baseline。

传播四问沿用本计划：Rust/Skill 行为为通用产品层，配置登记为工厂自身配置，进度为元文档；公共源码和 Skill 按实际登记投影，工厂细节不下沉。适用子包版本由受管 develop 处理，根 VERSION/正式 CHANGELOG 留待独立 release 授权；不更改既有 AGENTS/Hook 行为。

### 15.1 已核实的修复与验证安排

- 旧本机工厂 EXE 的 manifest 生成遗漏 `source-layout.json.retirements`。从当前锁定源码执行受管 manifest 后，派生清单恢复且无净差异；当前 manifest 源码无需修复。首次受管运行时准备取得 `prepared / prepared=3 / written_files=6 / self_test_executed=true`，后续源码修复及子包升版仍需最终产物核对，不能复用该收据冒称最终内容已准备。
- 已完成受管配置 plan/apply，终态 `migrated`、`sync_performed=false`、`version_changes=false`。实际 diff 仅移除 deep-steps 输入、将 factory/lifecycle 检查线程数从 4 改 1（workflow 原已为 1）；检查集合、超时和 baseline 未变。第一次语义比较误将 JSON 字段顺序视为变化，核对字段后纠正；构建持锁时的 apply 拒绝零写入，待构建完成重算计划后成功。
- 本次独立审查找到 G11 遗漏：factory 脏同步/release 仍获取 ProjectLock。现只移除同步中的调用，保留维护消费者，并新增同时持有 RuntimeLock/ProjectLock 的普通同步及 release 回归。该单线程定向回归已通过 `1 passed / 0 failed`（测试内含两个模式），日志 `.runtime/sync-simplification/factory-lock-regression.log`。
- 增补实际 CLI 预检、sync-remote、不提交/推送、跨进程预览、输入漂移拒绝、skip-fetch 执行与重复行为断言。漂移前快照在合法子包收口后采集；独立复核两项发现已修正，增量无新增确定 P1/P2，实际 CLI 测试待执行。
- 当前任务预览仅 factory/lifecycle/workflow 为 affected；checks/core/skeleton 为 context-only，不自动升版。计划选择 workflow 的 git_sync 模块检查覆盖原 11 项失败及性能场景，factory 的 git_sync_tests 覆盖维护锁分离、构建/缓存/提交回滚，lifecycle 的 release_host 检查覆盖会话元数据与显式项目变量；另串行补跑原失败 breaking 生命周期用例。历史读取 4 项及此前累计判级通过项因相关源码未再修改而复用。
- 版本语义：workflow 以 minor/breaking 记录用户明确接受的同步互斥退役及新远端接入编排；lifecycle 以 minor/compatible 记录预览约束和同输入宿主元数据精简；factory 以 minor/compatible 记录新同步能力接入及相关集成行为。均从各自最新受管版本记录比较，不重复计入前轮核心清理/develop；最终数字和是否连带由受管计划决定。

### 15.2 受管收口与性能结果（2026-10-29）

受管命令 `develop --apply --packages workflow,lifecycle,factory --analysis-file .runtime/sync-simplification/develop-analysis.json` 已返回 `development-closed`、`validation_mode=task-selected-final-candidate`、`checks.executed=3`、`checks.reused=0`、`git_written=false`。原始终态输出保存在 `.runtime/sync-simplification/develop-apply.log`；版本、完整检查命令及摘要由工具追加到 `.codex/package-closures.json`。journal 已清除，manifest 零写入检查为 `changed=false`，工厂版本检查 healthy=true。

| 包 | 旧版本 | 新版本 | 实际检查与结果 |
|---|---|---|---|
| factory | 0.4.0 | 0.5.0 | 注册 factory-unit 加 git_sync_tests 过滤，单线程，14 项通过；覆盖构建/缓存、Hook 拒绝回滚及同步不获取维护锁 |
| lifecycle | 0.5.0 | 0.6.0 | 注册 lifecycle-unit 加 release_host 过滤，3 项通过；独立探针标 ignored，由跨进程父用例实际调用 |
| workflow | 0.2.0 | 0.3.0 | 注册 workflow-unit 加 git_sync::tests 过滤，同步模块 42 个用例全部通过，含原资源故障失败名单 |

主程序保持 8.0.1，正式 CHANGELOG 未修改；其他包没有升版。factory/lifecycle 未配置稳定合同，workflow 本次含已接受的不兼容退役，因此保持 0.x，不宣称达到自动 1.0.0 条件。上述只证明任务范围收口，`outside_task_evidence_checked=false`，不据此宣称全仓可发布。

另已串行补验 `schema4_all_breaking_lifecycle_operations_require_explicit_acceptance`：`1 passed / 0 failed`，内部含 Rust/Python 的 rename/remove/split/merge 共 8 个场景，耗时 511.35 秒；日志 `.runtime/sync-simplification/breaking-lifecycle-regression.log`。轻量检查 skill-metadata issues/warnings 为空、project-structure errors 为空、diff --check 通过；既有归档提示未处理。

性能使用同名、同单线程方法的 `release_reuses_plan_only_when_fetch_preserves_all_planning_inputs`，本次没有并发构建或其他测试；`1 passed / 0 failed`，主体 48.18 秒，对比原文基线 168.73 秒，观察值减少约 71.4%。原始日志 `.runtime/sync-simplification/release-performance.log`：正常/漂移两个 actual release 的 `release.plan.build.calls` 均为 1，所测 sync 内部耗时分别 15.247/11.564 秒。夹具总耗时还包含准备和重复调用，历史基线没有完整宿主元数据，因此不是严格宿主匹配的因果性能实验，也不代表自然对话端到端提速。

阶段快照：当时 HEAD 为 8396937，修改未提交；原生 Goal 为 active，预算观察为 191,521/10,000,000 token（原生口径，委派覆盖未另行核实）。受管收口已完成，最终 build-assets 和实际 CLI 链路尚在后续队列；该阶段未宣称整个开发完成。最终结果见下节。

### 15.3 最终交付与接续边界（2026-10-29）

本节为交付时快照：当时开发已完成、等待用户验收，未代替用户执行验收，也未更新 TODO-044 的既有验收记录。后续用户验收见第 16 节。

| 验证面 | 最终证据与结论 |
|---|---|
| 源码与独立复核 | G11 的 factory ProjectLock 遗漏已修复；维护锁消费者保留。复用前轮审查，本次同一独立审查 Agent 对实际增量及夹具修正完成复核，无剩余确定 P1/P2 |
| 受管开发 | 第 15.2 节的三个包、三组最终候选检查已收口；最终 `develop --preview --packages workflow,lifecycle,factory` 返回 `task-development-current`、affected 为空；没有 develop journal |
| 产物与 dogfood | 最终 `cargo run --locked --release --manifest-path Cargo.toml --bin bridgeforge-factory --jobs 1 -- build-assets --factory --project-root .` 返回 prepared=3、written_files=6、self_test_executed=true；最终零写入 build-assets --check 为 current/reused=3/built=0，manifest --check 为 changed=false |
| 实际 CLI | 最终二进制执行 `cargo test --locked --offline --manifest-path scripts/tests/Cargo.toml real_cli_explicit_release -- --test-threads=1 --nocapture`：3 passed / 0 failed，232.93 秒。覆盖缺配置/显式配置、Rust 独立包和新增包；预检 → sync-remote → 零写入预览 → 输入漂移拒绝并保留用户修改 → release --skip-fetch → 重复调用；仅测试本地 bare 远端 |
| 工厂入口 | 本仓库 `git-sync --preflight` 为 ready，tool.version=0.5.0、product_version=8.0.1，包含 git-sync-remote-context-v1；该只读预检没有执行同步或发布 |
| 真实下游与自然运行 | 未执行用户级共享 Skill 安装、真实下游写入或真实远端发布；自然对话端到端耗时、完整原生 Hook 生命周期仍未验证，不以本机隔离 CLI 结果替代 |

实际 CLI 首轮发现两个测试夹具问题，均已修正，没有放宽生产门禁：

1. 新增 Rust 包原先只有 value 函数、没有测试，实际开发检查正确拒绝零执行并回滚版本。现为 7/8 两个阶段加入对应实际断言。
2. 根独立包拥有 business.txt，修改它会先触发子包版本覆盖检查。诊断重跑取得完整错误后，改用始终属于 product 且不计入包内容指纹的 CHANGELOG.md 注释变化来测分析 stale；严格断言分析拒绝、其他版本文件/HEAD/index/FETCH_HEAD 未写、用户注入内容仍保留，再恢复原字节验证成功发布。原业务文件场景保持。

失败日志保留在 `.runtime/sync-simplification/real-cli-chain.log`、`real-cli-rust-retest.log`，最终通过日志为 `real-cli-final.log`；最终构建日志为 `final-runtime-build.log`，入口与覆盖收据为 `final-preflight.json`、`final-development-status.json`。这些本机忽略文件不是跨电脑自动可用的证据；正式包版本记录保存在受管 `.codex/package-closures.json`，没有手工回填通过记录。

本轮实际新增修改：同步路径移除残余 ProjectLock、维护锁注释及回归；实际 CLI 接续/漂移/重复测试和新增包夹具；退役输入登记及串行检查配置；受管子包版本、锁、投影与记录；本计划和索引。前轮已保存的 summary C1–C3、git-sync S1–S5 及其他精简实现不重复算作本轮新增成果。

交付时仓库 `D:/Quant/BridgeForge`、分支 main、基准 HEAD 仍为 `83969378f87720469805d07f254fd5ae1d6036c5`，本轮修改未暂存、提交或推送；根 VERSION/正式 CHANGELOG 未改。独立审查已结束，本次构建和测试进程已取得终态。Goal 对应当前对话 `01a18321-9ece-7743-89e4-75817d627ad0`，最后一次交付前原生预算观察为 388,946/10,000,000 token、约 69 分钟；这是原生计量快照，前轮 token 和委派计量覆盖未独立实测，最终 Goal 终态与用量以对话报告为准。

交付时接续提示：先核对当前代码、HEAD 与交付记录，再消费后续用户授权。换机时通过 INSTALL.md 的受管入口恢复本地产物，不将本机日志、进程或 Goal 状态假定为已自动恢复，也不为缺少本机忽略文件重写版本历史。

## 16. 用户验收与发布授权（2026-10-29）

用户明确调用 `$summary accept`，确认本次 summary/git-sync 精简交付符合需求；同一条消息独立调用 `$git-sync release`，授权当前交付及正式发布以来累计变化的受管提交、推送和主程序发布。两项授权分别执行，发布不补做 develop 或重复升级子包。

验收覆盖本计划已确认的 summary 三模式共用流程、git-sync 两模式精简、G11 补修及既定失败保护。复用第 15 节真实证据：59 项受管定向检查、breaking 生命周期补验、3 项最终实际 CLI 测试、独立复核、三个子包收口、最终产物 current 与任务覆盖 current。没有因验收重跑构建、业务测试或审计。autopilot Goal 已完成，最终原生计量为 392,489/10,000,000 token、约 70 分钟。

保留边界：用户级共享 Skill 安装、真实下游写入/发布、自然对话端到端性能与完整原生 Hook 生命周期未验证；本次验收不把这些项目记为通过，不授权安装或真实下游写入。性能仅保留第 15.2 节限定的夹具对照结论。

TODO 总表没有与本计划唯一对应的既有条目，故本次不新增编号、不修改其他事项。TODO-044 保留原有验收；TODO-045、TODO-046 分属核心清理与 develop 共用流程，不由本次 summary/git-sync 验收结算；TODO-001、TODO-002 的用户级安装与换机验证仍保持未完成。验收记录及索引已更新，未新增规则、Hook、Memory 或归档操作。

发布使用已安装 factory 工具的聚合预检，接入远端后再按实际累计范围生成 product 分析。主程序与累计子包公告、提交及真实推送目标以最终受管收据为准，不将计划版本或已验收状态当作发布成功。

<a id="summary-accept-temporary-tests"></a>

## 17. summary accept 临时测试清理验收（2026-11-15）

用户先明确授权修改规则：“删除有明确依据的临时测试，保留长期回归测试，无法确定的保留并报告，覆盖 scripts/tests 和 source 中的相关测试”；本轮通过 `$summary accept` 确认交付符合需求，并独立调用 `$git-sync release` 授权本次变更的正式提交、推送和发布。该增量以本节为验收记录，不改写前述历史决定。

### 交付与验收范围

- 通用产品行为维护在 `source/forge/templates/skills/summary/SKILL.md` 和 `references/acceptance-mode.md`，同步自身 `.codex/skills/summary/` 镜像、内置 Template 规则、分发摘要及历史哈希。适用范围包括 `scripts/tests/**` 的独立测试和 `source/**` 的源码内联测试，仅处理本次已验收事项。
- 验收通过且未要求只读时，依据对话、记录、代码用途及调用关系删除明确的一次性临时测试；仅凭目录、文件名、本次新增或未接入 CI，不能判定可删。长期回归测试和共享辅助代码保留，用途或删除影响不明时保留并报告。
- 删除仅连同专属注册、引用及辅助文件处理；混合文件只删明确的临时测试部分，不改业务行为。普通 summary、switch、只读调用及未通过验收时不执行清理。仅做必要静态一致性检查，不把既有通过记录称为删除后重新验证通过。

### 已有工程证据

复用本轮开发完成后的实际结果，验收阶段不补跑构建、测试或 smoke：

- 锁定 Cargo 的工厂准备入口返回 `status=prepared`、`prepared=3`、`self_test_executed=true`、`written_files=15`，完成 CLI、Hook、factory 的受管构建及自检。本机产物入口为 `.build/bridgeforge/public-ready.json`。
- `bridgeforge-factory.exe build-assets --factory --project-root . --check` 返回 `status=current`、`read_only=true`、`reused=3`、`built=0`。
- `manifest --root . --check` 返回 `status=ok`、`changed=false`；`template-standard --root . --check` 返回 `status=current`；`check factory-version --root .` 返回 `healthy=true`，版本与合同均为 `20.1.0`。
- `bridgeforge.exe check skill-metadata --root .` 返回空 issues/warnings；summary 四份模板与镜像逐文件 SHA256 相同，`git diff --check` 通过。
- 原生候选 BF `20.0.1 → 20.1.0`，兼容新增验收清理能力；core `11.5.1 → 11.5.2`，更新内置规则。开发阶段 BP 维持 `2.6.1`，正式组合发布版本由原生发布适配器依据累计差异确定。

### 本次临时测试清理及限制

本次开发没有新增或修改测试实现；`scripts/tests/` 只有受管生成的 `Cargo.lock` 版本同步，`source/` 变更为共享 Skill、内置资产和原生版本。结合本轮对话和实际差异，没有识别出属于本次事项且有明确删除依据的临时测试，实际删除 0 项；现有回归测试保留，不扩大扫描或清理无关历史测试。

验收通过的是上述规则交付及本地工程验证。真实下游传播、实际验收删除流程和用户环境安装未验证，也不由本次发布授权推导相应写入权限。TODO 总表没有与本次增量唯一对应的条目，不新增编号或改动其他 TODO；TODO-001、TODO-002 不由此次验收结算。正式发布以受管 CLI 的 `status=released` 且 `publication_verified=true` 收据为准。


