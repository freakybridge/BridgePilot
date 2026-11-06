---
delivery_layout: flat
---

# BridgePilot Documents

- 已验收：[develop 新增工程清理阶段](1_delivery/forge-simplification/acceptance_2026-11-06_develop-engineering-cleanup.md)（2026-11-06，新增 D3 工程清理，验证、原生版本收口和交付顺延至 D4–D6。阶段引用和分发清单检查通过，已授权正式发布）。

- 已验收：[BP 同版本源码重建复用修复](1_delivery/bridgepilot-update/plan_2026-11-04_self-first-update.md#同版本重建复用修复验收2026-11-05)（2026-11-05；仅 PE 时间戳／PDB GUID 不同则保留现有程序，完整哈希与实际内容差异保护保留；35 项单元测试、21 项源码更新检查、真实 EXE 验证及两份故障 2.2.0 重放通过，独立复核无阻断。本轮已授权正式发布，结果以 CLI 收据为准；未替换用户级安装或升级真实下游）。

- 已验收：[Forge 维护转为内部流程](1_delivery/forge-maintenance/acceptance_2026-11-04_internal-workflow.md)（2026-11-04；移除 bridgeforge-codex 菜单入口，保留 BP 内部维护；11 项迁移验证、现有事务回归、独立复核、本机 BP 2.2.0 安装和原生发现通过，仓库三项产物 current。BF 18.0.0／install 0.10.0 候选；本轮已授权正式发布，结果以 CLI 收据为准，未执行真实下游升级）。

- 已验收：[bridgepilot-git-sync 受管安装与菜单发现](1_delivery/bridgepilot-git-sync/acceptance_2026-11-03_release-mode.md#受管安装与菜单发现验收2026-11-04)（2026-11-04；十五项包资产、四入口受管安装及恢复校验，46 项定向测试、真实 2.0.2 旧包升级、本机 2.1.1 安装与 Codex 原生发现通过；配置与档案保持。旧工厂工具阻点已按明确授权刷新解除，三项产物及零写入检查通过，本轮当前 BP 正式发布以 CLI 收据为准；未执行下游同步或发布）。

- 已验收：[git-sync 统一使用 BF 用户级入口](1_delivery/git-sync-scope/acceptance_2026-11-04_bf-shared-entry.md)（2026-11-04；移除 BP 四个项目镜像及生成登记，BF 源与用户安装字节保持，定向测试、受管工具检查及原生技能发现通过；factory 0.12.5，本轮已授权正式发布，结果以 CLI 收据为准）。

- 开发阶段已验收、正式传播进行中：[BP 统一升级入口改名与自身优先更新](1_delivery/bridgepilot-update/plan_2026-11-04_self-first-update.md#用户验收与发布授权2026-11-04)（2026-11-04；用户验收开发及隔离验证并授权 BP 2.0.0 release。29 项安装测试、6 项集成回归、17 项源码更新检查及真实 0.10.0／1.3.0 包隔离迁移通过；新入口先更新本机 BP，再默认逐个升级登记下游，默认不发布。用户安装和菜单发现尚未完成，正式发布以 CLI 收据为准）。

- 开发阶段已验收、真实交付进行中：[发布入口部署、GitLab 支持与高效工作流修复](1_delivery/native-release/requirements_2026-11-04_install-release-entry.md#用户验收与发布授权2026-11-04)（TODO-055；2026-11-04 用户验收修复及本地工程验证并授权 BP／Forge release。Forge 17.3.0 候选、独立复核、实际隔离安装及驱动产物验证通过；真实四仓新版安装／发布和 GitLab 上传尚未完成，TODO 保持进行中，正式发布以 CLI 收据为准）。

- 已验收：[bridgepilot-git-sync 版本报告格式增量](1_delivery/bridgepilot-git-sync/acceptance_2026-11-03_release-mode.md#版本报告格式增量2026-11-04)（2026-11-04；固定主程序及变化子包的旧／新版本两表，明确复用、缺基线和失败规则；源码与当前用户级副本一致，格式校验和离线样例核对通过。用户已授权当前 BP 正式发布，结果以 CLI 收据为准；本轮未重跑下游发布，历史验收及限制保留）。

- 已验收：[旧测试链路激进退役](1_delivery/test-retirement/acceptance_2026-11-03_aggressive-test-retirement.md#用户验收2026-11-03)（2026-11-03；删除29文件，测试源码净减14,590行、总文本净减16,760行；121项定向用例分批通过，两套编译、三项一致性检查及独立复核通过。保留当前功能与安全回归，未授权整体删除测试目录；完整发布结果以本轮受管CLI收据为准）。

- 已验收：[单入口发布与无代码不升版增量](1_delivery/native-release/requirements_2026-11-03_single-entry-release.md#无代码变动不升版修复验收2026-11-03)（TODO-054 已完成；本轮37项定向验证、独立复核及三项受管产物核验通过，正式发布复用保留原标签和资产、区分同步与发布提交；BF17.2.1 候选。本轮发布以CLI收据为准，既有 BP1.2.0／1.2.1 历史保留；真实下游及用户环境安装未验证）。

- 已验收：[Forge 分发清单归属迁移](1_delivery/manifest-location/acceptance_2026-11-03_forge-manifest-migration.md)（2026-11-03；50 项 Rust 定向测试、安装事务、受管构建、869 文件零写检查及独立复核通过；源码已提交推送 4674c68。当时将完整 release 缩减为源码发布是 Agent 错误，用户未排除安装包；原记录已更正，发布链路由 TODO-054 接续）。

- 已验收：[BP、BF 与 orchestrator 三产品版本治理](1_delivery/plan_2026-11-03_three-product-versions.md#用户验收与发布授权2026-11-03)（2026-11-03用户验收BP1.0.0／BF17.0.0／orchestrator0.14.0并授权源码发布；构建、定向验证、旧真实产物隔离迁移、dogfood及独立复核通过。发布结果以受管Git收据为准；未更新用户级安装或真实下游）。

本仓库采用 Forge 六层文档体系。`1_delivery/` 采用扁平布局：每个 topic 直接位于该目录下；若未来交付规模需要里程碑，可改为 `milestone` 并迁入 `M1/<topic>/`。

## 从这里开始

- 用户已验收：[四个下游 Template-only JSON 退役](1_delivery/template-retirement/requirements_2026-11-02_four-projects.md#用户验收2026-11-03)（2026-11-03；四真实项目递归均仅 hooks.json，23份 JSON＋4戳清理，固定工具基线／重复更新及独立终审通过；私有 Hook、Skill、业务差异与 Git 状态保持。本项目发布另行执行，不含四下游 Git 操作）。

- 实施完成、待用户验收：[BP 源码根迁移与公共目录规则补强](1_delivery/source-workspace-refactor/plan_2026-11-02_bp-source-root-migration.md)（2026-11-02；完整仓库已迁至本机正式开发根，Git 及既有改动保持；公共指令区回归、工具身份、安装绑定及独立复核通过，旧副本冻结保留；未提交、推送或发布，下游规则未传播）。

- 已验收：[BP 六包职责拆分](1_delivery/bridgepilot-package-split/plan_2026-11-02_package-split.md#用户验收与发布授权2026-11-02)（2026-11-02；13 项最终候选检查及受管收口通过；BP 0.9.0、五个新包 0.1.0、lifecycle 0.8.5；用户已授权 release，memory 的共享 workspace 构建覆盖已按明确授权补齐且保持 0.1.1，正继续受管发布，实际结果以收据为准）。

- 已验收：[BP 第九阶段：status 六列与 SQLite 唯一状态库](1_delivery/bridgepilot-evolution/plan_2026-10-30_bridgepilot-evolution.md#bp-status-sqlite-acceptance)（2026-11-01用户验收TODO-050内部阶段并授权release；BP／用户级Skill **0.7.2**，真实档案及私有GitHub SQLite切换、旧86份历史等价审计和隔离恢复通过。超4MiB来源观察受限、物理双机未验证等边界保留，heartbeat仍暂停；发布结果以实际收据为准）。

- 发布计划与执行记录：[四个下游骨架升级与 Release Plan](1_delivery/forge-downstream-upgrade/plan_2026-11-01_four-projects.md)（DR 编号、最新执行结果、发布边界和继续条件以该记录为准）。BP私有 [bridgeforge-update 独立入口](1_delivery/forge-downstream-upgrade/plan_2026-11-01_four-projects.md#独立入口交付与用户验收2026-11-02)已于2026-11-02验收，BP0.10.0／install0.2.0收口、本机双入口安装及原生发现通过；不进入Forge共享分发，已授权本轮release，正式结果以收据为准。此前额外BF能力与升版撤回的历史保持。

- 待用户验收：[发布范围判定修复与四个下游骨架升级结果](1_delivery/package-version-lifecycle/requirements_2026-10-31_release-scope-and-four-projects.md#10-正式发布与四项目安装结果2026-10-31)（TODO-051、TODO-052；13.0.3 已由 475b04f 发布推送，lifecycle 0.8.1→0.8.2，12 项最终候选检查与最终工具核验通过；四个下游已逐一安装 13.0.3，保护摘要一致、未提交下游。Stratus 业务发布仍有独立的全局输入与证据缺口；安装后记录尚未另行提交）。

- 已验收：[旧测试目录项目报告保留](0_architecture/design/codex-project-sync.md#用户验收旧测试目录项目报告保留2026-10-31)（core 10.0.2；7 项检查 80 个用例及独立退役 1 项通过，独立复核与最终工具核验通过。HoldemTrainer 只读升级预览已解除原报告阻塞，8 个记录文件摘要不变，未执行下游升级；已授权 Forge release）。

- 已验收：[旧工具目录保留与 StratusAgent 骨架升级](0_architecture/design/codex-project-sync.md#用户验收旧工具目录保留与-stratusagent-升级2026-10-31)（core 10.0.1、workflow 0.9.1；8 项检查 184 个用例及独立退役 1 项通过，真实 StratusAgent 9.0.0→13.0.0，151 项现有改动、15 个保护文件和项目专区原样保留；公共工具自检及独立复核通过，已授权 Forge release。项目自有 Hook 业务运行未验证）。

- 进行中：[BridgePilot 产品演进与跨项目总管开发计划](1_delivery/bridgepilot-evolution/plan_2026-10-30_bridgepilot-evolution.md)（TODO-050；第八阶段统一Skill已于2026-11-01验收，历史发布尝试及边界保留原章节；第九阶段工程结果、B实际切换与未验证项见15.18，整体需求尚未关闭）。

- 已验收：[统一产品清单与安装管理边界](0_architecture/design/codex-project-sync.md#用户验收与发布授权2026-10-30)（三个旧源清单合并至根 bridgeforge-manifest.json；资产历史、忽略内容和退役测试拆分一并验收；受管收口、143 项最终候选测试及最终工具核验通过，已授权 release；真实下游和用户级安装未执行）。

- 已验收：[三任务 autopilot 交付](1_delivery/artifact-layout/requirements_2026-10-30_three-directories.md#用户验收2026-10-30)（2026-10-30 用户验收 TODO-048、TODO-049、release 构建缓存优化及撤回识别修复，并授权受管 release；发布是否成功以实际收据为准）。

- 已验收：[release 中间编译结果复用与缓存锁优化方案](1_delivery/git-sync-performance/plan_2026-10-30_build-cache.md)（持久构建目录与分组锁已实现，固定候选对照及受管收口完成；重复候选明显提速，未宣称冷构建加速）。

- 已验收：[清理 Skill 最新规则与真实清理](1_delivery/artifact-cleanup/requirements_2026-10-30_cleanup-skill.md#用户验收当前成品保留规则2026-10-30)（TODO-049；build 仅保留当前成品及对应收据，其余全部清理；81 项定向测试、独立复核及受管子包收口完成，真实 Forge cleanup completed、剩余 0；runtime/deps 保护范围不变，下游和用户级安装未验证）。

- 已验收：[严格三目录布局需求与实施计划](1_delivery/artifact-layout/requirements_2026-10-30_three-directories.md)（TODO-048；三目录、默认检查、Forge 工具迁移、隔离验证及收口完成；用户指定旧目录已删除，最终目录检查零错误，真实下游和用户级安装未更新）。

- 9.1.0 已决定撤回：[临时升级工具的历史验收及 9.0.0 恢复](1_delivery/legacy-seven-upgrade/requirements_2026-10-29_four-projects.md#910-撤回与-900-恢复2026-10-29)（TODO-047 历史验收保留；三个由 Agent 负责的正式下游已迁入 9.0.0，保护文件、基线和重复执行检查通过，CausisRiskSuite 由用户报告完成。工厂产品内容已恢复至原始 9.0.0；远端同步与用户级安装结果以该记录及受管收据为准）。

- 待用户验收：[develop 共用工作流实施计划](1_delivery/forge-simplification/plan_2026-10-28_develop-common-path.md)（TODO-046；D1–D5、S/M/L 差异及 F01–F12 已实施，23 项定向验证、独立审查、3 项最终候选检查及受管收口通过；factory 0.4.0、lifecycle 0.5.0，workflow 0.2.0 仅刷新覆盖，最终工具产物已核对。未提交推送发布、未用户级安装，真实下游及自然调用未验证。前期动机见[收口要求](1_delivery/forge-simplification/notes_2026-10-28_develop-closure.md)）。

- 最近验收：[summary 与 git-sync 共用流程精简](1_delivery/forge-simplification/plan_2026-10-28_command-paths.md#16-用户验收与发布授权2026-10-29)（2026-10-29 用户已验收并独立授权 release；59 项受管定向检查、三个子包收口、最终运行时及 3 项实际 CLI 测试通过；工程证据见第 15 节，实际发布以受管收据为准；用户级安装与真实下游未执行）。

- 待用户验收：[核心能力保留与非核心、过期实现退役](1_delivery/core-only-cleanup/requirements_2026-10-28_core-only-cleanup.md)（TODO-045；C01–C03 已退役，保留 C04/C05，仅支持骨架 8.0.0 及以后；实质代码净减 2291 行，选定验证、受管子包收口及最终产物核验通过；含实际文件清单与未验证边界。未提交推送发布）。

- 已验收：[Forge 骨架职责与版本工作流简化](1_delivery/forge-simplification/requirements_2026-10-27_workflow-and-versioning.md#用户验收2026-10-28)（TODO-044；P1–P3、112 项相关验证及七包必要检查收口完成，8.0.0 已以 c333caaf 推送；旧重型默认流程永久退役，临时方案 A 经单独授权删除，公共模板与 Hook 未改。验收记录与 AGENTS 清理已随 8.0.1 提交 64ee6ba 保存。真实下游、双机、用户级安装、自然 Hook 及端到端提速仍未验证）。

- 历史阶段与独立待续：[develop 按本次任务收口与验证复用](1_delivery/develop-demand-discovery/requirements_2026-10-27_task-closure.md)（TODO-043；保留第 14 节四对话诊断、当时的方案 A 和 F1–F5 方案；该卡当时的未收口状态与临时规则不代表当前现状，最新简化、正式收口及方案 A 删除见 TODO-044 验收。TODO-043 的完整原需求未由本轮自动结算）。

- 当前开发：[跨电脑验证证据复用、发布检查范围与发布提速](1_delivery/package-version-lifecycle/requirements_2026-10-27_cross-machine-release.md)（TODO-042；已补当前进度快照、经验和接续步骤；源码及投影已更新、末批定向验证通过，完整收口失败回滚，最终运行时未就绪，开发未完成）。

- 已验收：[LF/CRLF 开发证据误判修复](1_delivery/source-workspace-refactor/requirements_2026-10-26_eol-evidence.md#用户验收与发布授权2026-10-27)（两项目零写入预览、七包受管收口及最终产物核验；已授权 Forge release，实际发布以收据为准。TODO-035 诊断问题保留，TODO-041 跟踪预览开销）。

已验收：[跨对话开发证据与发布预览](1_delivery/package-version-lifecycle/requirements_2026-10-26_portable-closure-evidence.md#用户验收与发布授权2026-10-26)（TODO-038；Forge 修复和受管收口、Stratus 八包补验及两轮零写入 release dry-run 已通过，用户已授权 Forge release，实际发布以受管收据为准）。[TODO-039：跨目录解释器解析](1_delivery/package-version-lifecycle/requirements_2026-10-26_portable-closure-evidence.md#todo-039-windows-跨目录相对检查程序解析)与[TODO-040：绝对分析路径](1_delivery/package-version-lifecycle/requirements_2026-10-26_portable-closure-evidence.md#todo-040-windows-项目内绝对分析路径归一化)单独登记为待确认，未在本轮修复。

已验收：[develop 完成状态与受管收口](1_delivery/package-version-lifecycle/requirements_2026-10-26_develop-completion-state.md#用户验收与发布授权2026-10-26)（TODO-037；七项程序 fixture、factory 受管重验及最终 nothing-to-close 通过，子包未升版；自然 Agent 对话效果未验证；已授权 Forge release，实际发布结果以受管收据为准）。

已验收：[骨架升级后的开发收口记录兼容](1_delivery/source-workspace-refactor/requirements_2026-10-25_closure-upgrade-compatibility.md#用户验收与发布授权2026-10-25)（TODO-036；七项回归及五组必需检查通过，lifecycle 0.1.1→0.1.2 已受管收口；Stratus 只读 release 预览由十包阻断缩小为两包真实变化，现场未变；已授权当前仓库 release，实际发布结果以受管收据为准）。

已验收：[骨架路径迁移后的发布分类](1_delivery/source-workspace-refactor/requirements_2026-10-25_migration-release-classification.md#用户验收2026-10-25)（TODO-034；2026-10-25 用户验收，隔离下游回归及五组必需检查通过，lifecycle 0.1.0→0.1.1 已受管收口；主程序仍为 7.0.0，未提交推送发布，未修改真实下游或用户级安装）。[TODO-035：开发收口失败诊断](1_delivery/source-workspace-refactor/requirements_2026-10-25_migration-release-classification.md#验收核对新增事项-todo-035) 单独登记为待确认，尚未实施。

- [Rust 源码与公共投影结构](0_architecture/design/source-workspace.md)

已验收：[源码子包、Skills 与分发结构重构](1_delivery/source-workspace-refactor/requirements_2026-10-24_source-workspace.md#用户验收2026-10-25)（TODO-032；2026-10-25 用户验收 `source/` 九包、版本收口、Skills 迁移、公共投影与 dogfood，以及忽略模板路径调整；未提交推送发布、未修改真实下游。TODO-033 单独跟踪非阻断的 TODO 参考导航旧路径，待确认）。

已验收：[子包开发收口与累计版本公告](1_delivery/package-version-lifecycle/requirements_2026-10-24_package-version-lifecycle.md)（TODO-025；develop 升子包、程序自动判断 1.0.0、release 只升主程序并公告累计变化；已授权 release，实际发布结果以受管收据为准；未修改真实下游）。

已验收：[工厂运行时准备与 Hook 正确性、性能](1_delivery/runtime-hook-hardening/requirements_2026-10-24_runtime-hook-hardening.md#用户验收与发布授权2026-10-24)（TODO-023、TODO-024；定向验证、实际产物及独立审计完成，普通编辑基准中位数降低 79.11%；2026-10-24 用户验收并授权 release，实际发布以受管收据为准；真实下游和自然生命周期全链路未验证）。

最近验收：[累计骨架差异归属修复](1_delivery/git-sync-release-rewrite/requirements_2026-10-23_single-release-model.md#累计骨架差异修复与验收2026-10-23)（TODO-022 增量；9 项定向验证、受管构建及工厂一致性通过；用户已验收并授权 release，真实下游 release 尚未复验）。

最近验收：[唯一发布模型与历史分界](1_delivery/git-sync-release-rewrite/requirements_2026-10-23_single-release-model.md#用户验收与发布授权2026-10-23)（TODO-022 已完成；15 项定向检查、独立审查、受管构建和 Forge 自身接续通过，约 30 分钟收口；2026-10-23 用户验收并授权 release，实际发布以受管收据为准；真实下游未修改、未验证）。

最近验收：[git-sync 历史分析性能](1_delivery/git-sync-performance/requirements_2026-10-23_history-reuse.md#经验沉淀与用户验收2026-10-23)（15 项定向检查、独立审查与真实规模隔离对照通过；2026-10-23 用户验收并授权 release；自然对话与真实远端对照仍未验证，TODO-009 保留待验证）。

最近验收：[历史共享源码归属补正](1_delivery/git-sync-release-rewrite/implementation_2026-10-23_historical-shared-ownership.md#用户验收与发布授权2026-10-23)（TODO-017 增量；11 个定向场景、独立审查、受管构建及基线一致性通过；2026-10-23 用户验收并授权 release，实际发布以受管收据为准；下游未修改、未验证）。

实施与验收：[artifact-layout Hook 合并](1_delivery/artifact-layout/requirements_2026-10-23_artifact-layout-merge.md)及[公共默认规则增量](1_delivery/artifact-layout/requirements_2026-10-23_artifact-layout-defaults.md#用户验收2026-10-23)（TODO-020）已验收；后者按用户确认的定向验证范围收口，全量中止与下游未验证边界保留。2026-10-23 已授权 release，实际发布以受管收据为准。

当前验收：[git-sync 执行环境一致性与单次确认](1_delivery/git-sync-release-rewrite/requirements_2026-10-22_git-sync-context-and-confirmation.md#用户验收2026-10-22)（2026-10-22 用户按增量结果验收，TODO-019 已完成：核心 234 项、发布场景 18 项、五项基础检查与独立审计通过，隔离 3.0.1 构建成功，受管流程约 29 分 44 秒；原全量配置已恢复，最新全量及真实对话/下游未实测。TODO-018 正式发布保留待执行，用户明确先不发布；VERSION 仍 3.0.0，本轮未提交或推送）。

最近验收：[单次升级与历史 Cargo 归属补正](1_delivery/git-sync-release-rewrite/requirements_2026-10-22_upgrade-and-ownership.md#用户验收与发布授权2026-10-22)（2026-10-22 用户验收，TODO-016、TODO-017 已完成；workspace 456、fixture 96 项、七项工程检查与独立审计通过；发布预览因 analysis stale 中断，后续 TODO-018；未提交推送，用户级安装与真实下游升级未执行）。

下游维护优化：[R04、R02、R01 耗时与构建复用](1_delivery/project-sync-performance/requirements_2026-10-20_build-reuse.md#用户验收与发布授权2026-10-20)（2026-10-20 工程验收，TODO-013 已完成；真实下游提速未验证；R05/R03 暂缓；本次受管 release 已获授权，实际发布以最终收据为准）。

同步流程优化：[检查恢复与发布分析复用](1_delivery/git-sync-performance/requirements_2026-10-19_workflow-reuse.md)（2026-10-19 用户验收；428 通过、14 跳过；真实重复准备七项全复用约 13.7 秒；TODO-009 的端到端对照仍待验证；本次受管 release 已获授权，实际版本与同步状态以最终收据为准）。

首期交付：[通用发布核心与 Rust 独立包版本](1_delivery/git-sync-independent-packages/requirements_2026-10-16_rust-release.md)（schema 2 显式启用；旧项目保持原行为；已由扩展交付覆盖验收，TODO-010）。

扩展交付：[Rust / Python 包生命周期](1_delivery/git-sync-independent-packages/requirements_2026-10-19_polyglot-lifecycle.md)（通用核心、语言适配器、历史与 lock；2026-10-19 用户验收，TODO-010 已完成；本次受管 release 已获授权，实际发布以最终收据为准）；[协作记录](1_delivery/git-sync-independent-packages/collabs_2026-10-19_polyglot-lifecycle.md)。

新增包发布修复：[Rust 新增包首次纳入规则与现场恢复](1_delivery/git-sync-independent-packages/requirements_2026-10-19_package-additions.md)（2.5.2 历史修复，已由扩展交付覆盖验收；真实下游补归属配置及实际发布仍未验证；沿用 TODO-010）。

最近验收：[git-sync 首次配置引导与 Cargo 隐式成员修复](1_delivery/git-sync-first-use/requirements_2026-10-16_first-use-and-rust-members.md)（2026-10-17 工程验收通过，含历史锁迁移兼容修复；TODO-011、TODO-012 已完成；本次受管 release 已获授权，实际发布以同步收据为准，真实下游未复验）。此前调查记录见[历史锁迁移换机交接](1_delivery/git-sync-first-use/handoff_2026-10-17_historical-lock-migration.md)。

项目后续事项见 [待办总表](0_architecture/TODO-INDEX.md)。

| 你要做什么 | 先读这里 |
|---|---|
| 当前架构 | [`0_architecture/`](0_architecture/)；项目必须在该目录索引当前有效的架构资料 |
| 当前交付 | [`1_delivery/`](1_delivery/)；从 `lifecycle: active` 的交付包继续 |
| 开放 Bug | [`2_bugs/`](2_bugs/)；只把尚未归档的问题作为当前问题 |
| 项目操作 | 根 [`AGENTS.md`](../AGENTS.md) 和 [`3_reference/`](3_reference/) |
| 历史记录 | [`4_archive/`](4_archive/)；历史材料不作为当前运行合同 |
| 项目知识 | [`5_project_knowledgebase/`](5_project_knowledgebase/)；项目自有话题与长期资料 |
| 本工厂当前架构 | [`design-rationale.md`](0_architecture/design/design-rationale.md) 和 [`codex-native-instruction-architecture.md`](0_architecture/design/codex-native-instruction-architecture.md) |
| 本工厂当前重构 | [`BUG-agents-ia`](2_bugs/BUG-agents-ia/README.md) |
| 本工厂操作手册 | [`INSTALL.md`](../INSTALL.md) 和 [`codex-project-operating-guide.md`](3_reference/codex-project-operating-guide.md) |

本节只维护第一阅读路径；每个事项的详细状态仍由目标文档单独维护。

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

## 索引

| 目录 | 作用 | 当前内容 |
|---|---|---|
| `0_architecture/` | 架构与设计依据 | `design/` |
| `1_delivery/` | 需求确认、计划、验收、协作与专题讨论 | 见下方 topic 索引 |
| `2_bugs/` | 已知故障及其修复记录 | 34 条故障记录 |
| `3_reference/` | 外部资料与可复用参考实现 | `examples/antifab-deny-hook.rs` |
| `4_archive/` | 已完成或已失效的历史材料 | 既有历史档案；后续按 `delivery/`、`bugs/` 分类归档 |
| `5_project_knowledgebase/` | 项目自有知识话题与长期资料 | 当前为空 |

## 架构

- [BridgePilot 运行协议](0_architecture/design/bridgepilot-runtime-protocol.md)：Rust 核心、私有档案、有限窗口调度、原生交接与项目成员归类、heartbeat、开发发布证据核验及暂停／未知恢复；明确桌面渲染、真实发布和跨机未验收边界。

- [子包开发版本与主程序发布](0_architecture/design/package-version-lifecycle.md)：开发与正式发布的两个基线、规则事实源及新旧衔接。

- [artifact-layout 布局检查合同](0_architecture/design/artifact-layout.md)：公共默认规则、项目差异配置、A 模式、pre-commit 与 git-sync/release 的分工及性能验收；本批实施见公共默认规则增量卡。

- 工厂专属[开发收口清单](0_architecture/design/development-completion.md)：按影响范围分层验证、证据复用与独立可选的发布候选准备；[流程精简验收](1_delivery/git-sync-performance/requirements_2026-10-23_scoped-development.md#用户验收2026-10-23)（TODO-021，2026-10-23 已验收）。
- 工厂[独立构建产物缓存](0_architecture/design/factory-artifact-cache.md)：编译身份、缓存所有权、有限清理和中断恢复；当前 R04 的结果见交付卡。
- 设计资料：[`0_architecture/design/`](0_architecture/design/)，包括 [`codex-project-sync.md`](0_architecture/design/codex-project-sync.md)、[`codex-native-instruction-architecture.md`](0_architecture/design/codex-native-instruction-architecture.md)、[`codex-native-memory-sync.md`](0_architecture/design/codex-native-memory-sync.md)、[`user-facing-result-contract.md`](0_architecture/design/user-facing-result-contract.md)、`design-rationale.md` 与上游同步 playbook。
- 操作参考：[`codex-project-operating-guide.md`](3_reference/codex-project-operating-guide.md)、[项目自有 Rust Hook](3_reference/project-rust-hooks.md) 与主动澄清参考 [`codex-hook-signals.md`](3_reference/codex-hook-signals.md)；自动 Clarify / Focus Hook 已退役。
- `$bridgeforge-codex` 的运行手册属于产品源码，位于 [`skills/bridgeforge-codex/references/`](../source/forge/templates/skills/bridgeforge-codex/references/)，不纳入 `doc/`。

## Delivery topic

- [旧 Rule／项目 Memory 语义迁移退役](1_delivery/legacy-asset-migration-retirement/requirements_2026-10-24_retirement.md)：承接 TODO-026，本地实现与工程验证完成、尚未用户验收；2026-10-24 按用户要求冻结，进度与恢复边界已保存。

update part I–V：[完整更新流程与项目 Hook 解耦](1_delivery/project-sync-performance/requirements_2026-10-21_update-part-i.md#用户验收与发布授权2026-10-21)（2026-10-21 用户验收，TODO-014 已完成；七项检查、workspace 449 项、fixture 96 项及独立审计通过；已授权本轮受管 release，发布结果以最终收据为准；暂不升级用户级安装和真实下游）。

同步编排优化：[确认卡与计时基线](1_delivery/git-sync-performance/requirements_2026-10-12_orchestration.md#用户验收与发布授权2026-10-20)（R10/R11/R08 工程已验收：七项检查、workspace 435 项及 fixture 93 项通过；本地 CLI 中位数减少 3.8%，波动较大，不代表真实端到端；TODO-009 保持待验证。已授权本次受管 release，尚未分发到用户级或真实下游）。

R08：[严格 JSON 解析器职责归位](1_delivery/shared-json/requirements_2026-10-11_R08.md)（2026-10-11 用户已验收，TODO-007 已完成；工程检查与独立增量审计通过；正式 release 因 fetch TLS 错误未完成，后续 TODO-008）。

R07：[TODO 管理与归档收口一致性](1_delivery/todo-lifecycle/requirements_2026-10-11_R07.md)（实现、7 项工程检查与独立审计已通过；等待用户验收，TODO-006 未结算）。

本轮 R06：[Memory 快照加载基线](1_delivery/memory-snapshot-baseline/requirements_2026-10-11_R06.md)（2026-10-11 已验收；基线与工程检查通过，采纳暂缓优化结论，生产实现未改）。

本轮 R05：[项目同步内容合并职责拆分](1_delivery/project-sync-content/requirements_2026-10-11_R05.md)（2026-10-11 已验收；工程验证与独立审计通过，保持内容结果与安全事务行为）。

本轮 R04：[自动索引刷新性能核验](1_delivery/project-map-refresh/requirements_2026-10-11_R04.md)（2026-10-11 已验收；保持索引及时性，区别于历史独立产物缓存 R04；含本轮 R01–R04 累计验收记录）。

本轮 R03：[Memory 上传收尾归一](1_delivery/memory-publication-tail/requirements_2026-10-11_R03.md)（2026-10-11 已验收；区别于历史 R03 Memory CLI）。

本轮 R01：[受管 Markdown 基础解析收拢](1_delivery/managed-markdown-parser/requirements_2026-10-10_R01.md)（2026-10-11 已验收；区别于下表历史扫描中的 R01 清理）。

用户级指令分发：[确认卡与验证记录](1_delivery/user-agents-distribution/requirements_2026-09-27_user-agents.md)。

本轮 R02：[共享构建能力独立](1_delivery/shared-generated-build/requirements_2026-10-11_R02.md)（2026-10-11 已验收；区别于历史 R02 公共持久化）。

| Topic | 主要记录 |
|---|---|
| `package-version-lifecycle` | [跨电脑证据与快速发布](1_delivery/package-version-lifecycle/requirements_2026-10-27_cross-machine-release.md)：TODO-042；[需求与实施验证记录](1_delivery/package-version-lifecycle/requirements_2026-10-24_package-version-lifecycle.md)：TODO-025，开发收口升子包、自动稳定判定及主程序发布累计公告 |
| `git-sync-release-rewrite` | [schema4 两种策略确认卡与工程结果](1_delivery/git-sync-release-rewrite/requirements_2026-10-21_schema4.md)、[用户验收：TODO-015](1_delivery/git-sync-release-rewrite/requirements_2026-10-21_schema4.md#用户验收2026-10-22)、[接入合同](1_delivery/git-sync-release-rewrite/schema4-contract.md)、[autopilot 交接](1_delivery/git-sync-release-rewrite/handoff_2026-10-22_schema4.md)：2026-10-22 用户已验收，TODO-015 已完成；两种策略共用 Rust/Python、复用 Stratus；7/7 检查、538 tests passed、33 项 CLI 补验及目标 3.0.0 隔离准备通过；条件项与真实下游未验证，未提交推送发布 |
| `refactor-scan` | [Skill 开发验收](1_delivery/refactor-scan/requirements_2026-10-09_refactor-scan.md)已完成；[工厂重构候选与经验](1_delivery/refactor-scan/scan_2026-10-09_factory-refactor.md)记录扫描时事实；[R01 清理](1_delivery/refactor-scan/requirements_2026-10-09_r01-cleanup.md)、[R02 公共持久化](1_delivery/refactor-scan/requirements_2026-10-09_r02-persistence.md)、[R03 Memory CLI](1_delivery/refactor-scan/requirements_2026-10-09_r03-memory-cli.md)及 [R04 独立缓存](1_delivery/refactor-scan/requirements_2026-10-10_r04-artifact-cache.md)均工程验证通过并于 2026-10-10 获用户验收；累计收据与验收边界见 R04；[扫描换机交接](1_delivery/refactor-scan/handoff_2026-10-09_factory-refactor.md)为历史记录；真实项目试用见 TODO-004 |
| `summary-workflow` | [Summary 三模式与会话状态快照退役](1_delivery/summary-workflow/requirements_2026-10-08_summary-workflow.md)；[本轮经验教训](1_delivery/summary-workflow/lessons_2026-10-08.md) |
| `git-sync-performance` | [普通同步与发布构建耗时优化](1_delivery/git-sync-performance/requirements_2026-10-08_git-sync-performance.md)；[开发收口与轻量发布](1_delivery/git-sync-performance/requirements_2026-10-08_prepared-release.md)：检查前移、发布准备记录和端到端验收 |
| `git-sync-explicit-release` | [独立同步与用户可选升版](1_delivery/git-sync-explicit-release/requirements_2026-10-09_independent-git-sync.md)：当前需求；[旧项目接入诊断历史](1_delivery/git-sync-explicit-release/requirements_2026-10-09_release-onboarding.md)；[普通同步不升版](1_delivery/git-sync-explicit-release/requirements_2026-10-08_release-only.md)；[原显式发布交付](1_delivery/git-sync-explicit-release/requirements_2026-10-08_git-sync-explicit-release.md) |
| `weekly-quota-budget` | [token 与周额度双数字预算](1_delivery/weekly-quota-budget/requirements_2026-10-02_weekly-quota-budget.md)：临时系数、分配置估算与有效样本校准 |
| `autopilot` | [单项目任务托管需求与验证](1_delivery/autopilot/requirements_2026-10-02_autopilot.md)：自然语言/多对话链接接管、整包预算、Skill/Hook 衔接与当前对话验收 |
| `gpt6-skeleton-upgrade` | [初审](1_delivery/gpt6-skeleton-upgrade/audit-01_2026-09-05.md)；[一](1_delivery/gpt6-skeleton-upgrade/requirements_2026-09-05_block1.md)、[二](1_delivery/gpt6-skeleton-upgrade/requirements_2026-09-05_block2.md)、[三](1_delivery/gpt6-skeleton-upgrade/requirements_2026-09-05_block3.md)已交付；[四及课题一至五实施记录](1_delivery/gpt6-skeleton-upgrade/requirements_2026-09-05_block4.md)：课题一至五已确认问题完成源码修复和定向核验；模型行为、真实自动触发以及预算和 summary 候选待验证（2026-09-06）；[高耗能提醒 Hook 实施与验收](1_delivery/gpt6-skeleton-upgrade/requirements_2026-09-07_high-cost-reminder.md)（2026-09-07） |
| `project-map-autogeneration` | 两份项目 Map 的确定性生成、静默生命周期维护、旧手写内容完全重建与 Skill fallback；见[确认卡](1_delivery/project-map-autogeneration/requirements_2026-09-04_project-map-autogeneration.md)（2026-09-04） |
| `project-rust-hooks` | [项目自有 Rust Hook 构建与 Assist 迁移](1_delivery/project-rust-hooks/requirements_2026-09-03_project-rust-hooks.md)；[Hook 登记兼容性修复](1_delivery/project-rust-hooks/requirements_2026-09-03_hook-registry-compatibility.md)；[临时构建路径失效](1_delivery/project-rust-hooks/requirements_2026-09-03_build-directory-failure.md)；[项目独立依赖](1_delivery/project-rust-hooks/requirements_2026-09-08_project-dependencies.md) |
| `project-knowledgebase-layer` | 固定第六层项目知识库、项目内容保留与逐源迁移支持，见[确认卡](1_delivery/project-knowledgebase-layer/requirements_2026-09-03_project-knowledgebase-layer.md)（2026-09-03） |
| `rust-only-bridgeforge` | BridgeForge 工厂与下发骨架整体退役 Python，以 Rust CLI 迁移检查、项目同步、Git、批处理、Native Memory 和测试，并最终删除全部 `.py` 与 `.venv` 依赖；含[确认卡](1_delivery/rust-only-bridgeforge/requirements_2026-09-01_rust-only-bridgeforge.md)与[协作记录](1_delivery/rust-only-bridgeforge/collabs_2026-09-01_rust-only-bridgeforge.md)（2026-09-01） |
| `rust-hook-runtime` | 将项目骨架 6 个 Hook 的完整触发链迁移到 Rust，安装/升级时由 Cargo 构建，运行时零 Python，并保持跨平台、无窗口与失败语义等价；含[确认卡](1_delivery/rust-hook-runtime/requirements_2026-09-01_rust-hook-runtime.md)与[协作记录](1_delivery/rust-hook-runtime/collabs_2026-09-01_rust-hook-runtime.md)（2026-09-01） |
| `agents-md-simplification-review` | AGENTS.md 优化草案的确认卡、11 版双 agent 辩论、V11 中断现场、有限验收边界及“根级目录路由 + 工厂嵌套指令 + 硬闸”的后续决定（2026-08-27、2026-08-28） |
| `hook-utf8-memory-loading` | Windows GBK 环境下由公共 Hook dispatcher 强制 UTF-8，恢复项目 memory 的 SessionStart 加载，并以四个真实下游隔离 worktree 验证（2026-08-26） |
| `bridgeforge-codex-batch` | BridgeForgeCodex 仓库专属的下游骨架批量升级、Git 同步、异常隔离与全量重跑；新增 1.5.6 Git 环境污染和 Batch 状态机死锁的修复、真实恢复需求（2026-08-21、2026-08-27） |
| `bridgeforge-home-layout` | 用户级目录演进 |
| `bridgeforge-codex-naming-contract` | 用户命令、菜单、聊天显示、GitHub identity 与内部技术标识的分层命名契约（2026-08-17） |
| `bridgeforge-latency-optimization` | 用户级 sparse canonical fast path、项目 planner 去重、并行终态验证与阶段计时收据（2026-08-15） |
| `bridgeforge-actionable-readiness` | 双状态更新结果、可执行完善清单、程序推荐与用户自定义部分确认，并保持整体 0/1 次确认（2026-08-15） |
| `bridgeforge-upstream-absorption-modes` | A 激进吸收、B 温和自定义、C 保守停止的单卡上游受管区块吸收契约（2026-08-15） |
| `bridgeforge-keyed-index-merge` | 规则与目录索引按稳定键合并，防止 A 激进吸收删除下游独有条目（2026-08-16） |
| `bridgeforge-managed-rule-safety` | 高定制 Rule 的 project-owned/preserve ownership、缺失标题 fail-closed、fenced Markdown 解析与事务回滚（2026-08-16） |
| `bridgeforge-update-strategy` | 旧 AGENTS 分类、清理、精确去重与项目专区合并，以及 `$git-sync` 合法 no-op 分类（2026-08-18） |
| `target-cleanup-retirement` | 核心 Rust/Cargo `target_cleanup.py` 退役、官方旧副本受控删除与修改版项目所有权保留（2026-08-18） |
| `native-memory-durable-consent` | Native Memory 首次长期授权、双向自动同步、本地 hook 自维护与远端 reconcile 职责解耦（2026-08-18） |
| `project-sync-release-preflight` | 项目同步与 `$git-sync` 共用 transition proof，并在写骨架版本戳前完成只读 release preflight（2026-08-19） |
| `project-sync-schema-v1-lineage` | 可信 schema v1 HEAD 先验证历史 hash 与旧戳，再确定性映射到 schema v2 稳定资产（2026-08-19） |
| `project-sync-single-release-standard` | Planner、Apply 与 `$git-sync` 的骨架 transition 直接调用同一验收函数，消除 plan `ready` / apply `blocked` 双标准（2026-08-19） |
| `project-sync-versioned-region-lineage` | pre-commit region 退役历史 marker/hash，只保留当前规则并要求旧项目显式适配（2026-08-19） |
| `project-sync-agents-zones-single-rule` | 根 AGENTS 退役旧标题 projection，只保留 agents_zones 并要求旧项目显式适配（2026-08-19） |
| `project-sync-hooks-zones-single-rule` | 用户级与项目级 hooks 统一为 managed/external 逻辑分区，受管 handler 唯一、canonical、独占 group（2026-08-19） |
| `project-venv-hook-runtime-single-rule` | 历史 CPython/.venv 方案，已由 `rust-only-bridgeforge` 完整替代；原需求和验证记录保留，不作为当前操作入口（2026-09-02） |
| `project-sync-explicit-adaptation-transaction` | 精确选择 G 项并以 fingerprint 和一次性本地收据贯通 project-sync Apply 与后续 `$git-sync`（2026-08-19） |
| `project-sync-four-project-zero-blocker-rollout` | 闭合 #1～#9 后按共享 Git 与 Native Memory 串行约束推进四项目骨架更新，并记录 M2/Causis 现场补出的 1.4.24～1.4.26 三态凭证与全程收据（2026-08-20） |
| `cba-bridgeforge-1-4-26-clean-reinstall` | ClaudeBridgeAssist 以项目保留清单保存 vault hooks、skills、memory、映射和 AGENTS 项目区，放弃旧骨架谱系并一次性干净安装 BridgeForge 1.4.26（2026-08-20） |
| `bridgeforge-1-4-28-clean-baseline` | 全仓删除仅服务 1.4.28 以前的历史兼容层，以 `PreservationManifest` 破坏性重建和 1.4.28+ current-only 本地基线建立长期恒定体积的干净骨架（2026-08-20） |
| `bridgeforge-1-4-29-cleanliness-audit` | 完成 1.4.29 洁净审计与架构 debate，以及 1.4.30 两期核心安全修复和 current-only 整体洁净清理（2026-08-21） |
| `bridgeforge-1-4-31-clean-baseline` | 将最低清洁基线提升到 1.4.31：旧项目通过一次性 `PreservationManifest` 破坏性重装，1.4.31+ 保持 schema 3 current-only 更新（2026-08-21） |
| `user-skill-compatibility-retirement` | 彻底退役旧 `$bridgeforge` 用户级兼容 manifest、过渡入口与旧资产迁移链，只保留正式 active Skill 清单（2026-08-19） |
| `bridgeforge-single-confirmation` | `init`、`adopt`、`update`、`switch` 的零确认安全路径、单次风险确认与 Codex 窄权限规则需求（2026-08-15） |
| `bridgeforge-repository-structure-governance` | 文档五层结构机器检查、BridgeForge 测试迁入 `scripts/tests/**`，以及下游旧测试目录只报告不自动迁移（2026-08-16） |
| `bridgeforgecodex-codex-only-rebrand` | BridgeForgeCodex 1.0.0：彻底退役 Claude、删除 switch/parity、全技术标识改名、`0.86.0+` Codex 直升与最终仓库改名（2026-08-16） |
| `template-root-flattening` | Codex-only 模板从 `templates/codex/**` 提升为 `templates/**`，保留 `0.86.0+` 历史 lineage 并同步同步器、schema、dogfood 与测试（2026-08-16） |
| `factory-template-dogfood-contract` | Template 公共 AGENTS/Rules 单一事实源、bridgeforge-codex 项目定制区与工厂 Overlay，以及编辑/提交/发布防漂移硬闸（2026-08-16） |
| `codex-rule-runtime-simplification` | 退役未实际加载的 Markdown path rule，将有效红线无损迁入原生 AGENTS、hook、skill 与文档，并安全保留下游定制（2026-08-16） |
| `current-baseline-project-asset-migration-and-native-memory-sync` | 下游旧 Rule/项目 Memory 逐文件确认迁移、动态最新 current-only 基线，以及原生 Memory 无感双机同步、自愈和可验证 Git 历史；含[确认卡](1_delivery/current-baseline-project-asset-migration-and-native-memory-sync/requirements_2026-08-31_asset-migration-sync.md)、[自动同步可靠性闭环](1_delivery/current-baseline-project-asset-migration-and-native-memory-sync/requirements_2026-09-08_auto-sync-reliability.md)、[协作记录](1_delivery/current-baseline-project-asset-migration-and-native-memory-sync/collabs_2026-08-31_asset-migration-sync.md)与[Hook 自动触发辩论](1_delivery/current-baseline-project-asset-migration-and-native-memory-sync/debates/2026-09-04_native-memory-hook-dispatch.md)（2026-09-08） |
| `header-upgrade` | 旧版 current-only rebuild 安全升级 Markdown 受管表头、补齐缺失受管标题并保留项目自有内容；含[确认卡](1_delivery/header-upgrade/requirements_2026-09-01_header-upgrade.md)、[计划](1_delivery/header-upgrade/plan.md)与[验收](1_delivery/header-upgrade/acceptance.md)（2026-09-01） |
| `fixed-upgrade-baseline` | 固定 1.8.6 升级基线、低版本重建与基线内兼容更新；两条路径保留旧 Rule/Memory 逐文件确认，见[确认卡](1_delivery/fixed-upgrade-baseline/requirements_2026-09-02_fixed-baseline.md)（2026-09-02） |
| `codex-agents-structure-reorganization` | Codex AGENTS 信息架构重组、历史标题安全迁移、项目必填区双状态硬闸与 `ctx-budget` 完整退役（2026-08-16） |
| `shared-skill-model-inheritance` | 删除 shared skill 的 Claude 专用 `model:` 覆盖，双宿主统一继承当前会话模型，并合并精简 Codex 模板的模型选择与执行分工说明（2026-08-16） |
| `confirm-workflow`、`develop-demand-discovery`、`explain-skill` | 需求确认与通用 skill 演进；[develop 验证与收口重构](1_delivery/develop-demand-discovery/requirements_2026-10-09_validation-refactor.md)处理 Q1–Q5，后两者含 `research/` |
| `cross-project-write-guard`、`non-ascii-shell-guard` | 安全防护；后者新增 [memory writer stdin 编码旁路报告](1_delivery/non-ascii-shell-guard/research/2026-08-04_memory-writer-stdin-encoding-bypass-report.md) |
| `ctx-management` | 上下文治理；Stall Warning 已裁定从双宿主骨架及下游更新中移除（2026-07-30） |
| `doc-unification`、`document-lifecycle` | 文档体系演进 |
| `git-sync-latency-optimization` | `$git-sync` 单脚本直跑、失败前置、重复重建消除与完整同步收据（2026-08-01） |
| `git-sync-version-automation` | BridgeForge 与下游项目双版本域的 `$git-sync` 自动 bump、原生字段同步与统一 CHANGELOG 需求；新增 [下游项目 Rule 被误判为受管骨架的所有权缺口报告](1_delivery/git-sync-version-automation/research/2026-08-12_downstream-rule-managed-skeleton-boundary-gap.md)（2026-08-12） |
| `memory-rule-organization` | 双 Memory 交付历史；项目 `.codex/memory/` 部分已被 `project-memory-retirement` 替代，当前只保留原生 Memory 同步事实并指向 `codex-native-memory-sync.md`（2026-08-30） |
| `project-memory-retirement` | 退役项目 `.codex/memory/`，为候选工程与未来下游建立程序扫描、Agent 语义审核、用户逐项确认、受控迁移及独立清理授权（2026-08-30） |
| `shared-skill-distribution` | 用户级 shared skill 分发 |
| `skill-runtime-efficiency` | 非根 skill 的确定性 fast path、重复 agent/索引消除与高频 Git 单进程优化（2026-08-15） |

每个 topic 内以 `requirements_*.md` 保存确认卡；实现计划、验收方案、协作记录和正式讨论分别与该确认卡同域保存。仅 topic 内路径可作为该事项的工作上下文。

## 当前 Bug records

- [`BUG-schema3-historical-product-input-attribution.md`](2_bugs/BUG-schema3-historical-product-input-attribution.md)（schema 3 历史未登记项目文件的精确补认；工程检查与独立审计通过，等待正式发布及下游验证）
- [`BUG-native-memory-windows-long-path-sync.md`](2_bugs/BUG-native-memory-windows-long-path-sync.md)（Windows 长路径导致同步失败、后台测试并行干扰及真实冲突恢复；1.17.5 正式安装与自动生命周期验证完成，已验收）
- [`BUG-native-memory-snapshot-order-compatibility.md`](2_bugs/BUG-native-memory-snapshot-order-compatibility.md)（旧快照兼容与后台句柄继承已修复；1.14.7 真实启动、回复结束及关闭后同步验证通过，等待用户验收）
- [`BUG-managed-markdown-commented-table-ambiguity.md`](2_bugs/BUG-managed-markdown-commented-table-ambiguity.md)（项目实表与注释示例表共存时被误判为歧义，六类验证与独立审计已通过，等待发布与下游 Apply 验收）
- [`BUG-clean-factory-git-sync-skips-runtime-repair.md`](2_bugs/BUG-clean-factory-git-sync-skips-runtime-repair.md)（干净工厂同步跳过生成资产修复，源码与真实 fixture 已修复，等待发布、四项目 batch 与 runtime 验收）
- [`BUG-native-memory-hook-path-separator-drift.md`](2_bugs/BUG-native-memory-hook-path-separator-drift.md)（Windows 等价路径引起用户级 Memory Hook 漂移误报，修复与验证进行中）
- [`BUG-agents-ia/`](2_bugs/BUG-agents-ia/README.md)（当前状态与验证收据见该总账；IA-14 用户与 runtime 验收见 [`IA-14-user-runtime-acceptance.md`](2_bugs/BUG-agents-ia/IA-14-user-runtime-acceptance.md)）
- [`BUG-codex-backend-unexpected-control-exit.md`](2_bugs/BUG-codex-backend-unexpected-control-exit.md)
- [`BUG-windows-codex-hooks-open-visible-terminal.md`](2_bugs/BUG-windows-codex-hooks-open-visible-terminal.md)
- [`BUG-legacy-head-contract-missing-release-blocks-rebuild.md`](2_bugs/BUG-legacy-head-contract-missing-release-blocks-rebuild.md)
- [`BUG-rebuild-drops-project-doc-index.md`](2_bugs/BUG-rebuild-drops-project-doc-index.md)
- [`BUG-rebuild-blocks-required-project-maps.md`](2_bugs/BUG-rebuild-blocks-required-project-maps.md)
- [`BUG-project-sync-schema-v1-baseline-and-native-memory-hook-race.md`](2_bugs/BUG-project-sync-schema-v1-baseline-and-native-memory-hook-race.md)
- [`BUG-native-memory-maintain-misclassified-safe-data-egress.md`](2_bugs/BUG-native-memory-maintain-misclassified-safe-data-egress.md)
- [`BUG-target-cleanup-core-skeleton-ownership.md`](2_bugs/BUG-target-cleanup-core-skeleton-ownership.md)
- [`BUG-aggressive-absorption-drops-downstream-rule-semantics.md`](2_bugs/BUG-aggressive-absorption-drops-downstream-rule-semantics.md)
- [`BUG-git-sync-sandbox-permission.md`](2_bugs/BUG-git-sync-sandbox-permission.md)
- [`BUG-project-hook-registry-native-schema.md`](2_bugs/BUG-project-hook-registry-native-schema.md)（修复项目 Rust Hook 独立登记与原生配置兼容性，验收证据见关联确认卡）
- [`BUG-generated-build-directory-disappears.md`](2_bugs/BUG-generated-build-directory-disappears.md)（临时构建路径失效，根因与修复验证进行中）
- [`BUG-git-sync-contract-transition-classification.md`](2_bugs/BUG-git-sync-contract-transition-classification.md)（1.5.5 修复已通过工厂验证与独立审计，等待 Batch restart 的真实下游复验）
- [`BUG-current-baseline-gitattributes-hook-reinitializes-real-repository.md`](2_bugs/BUG-current-baseline-gitattributes-hook-reinitializes-real-repository.md)（源码与回归已修复，等待发布和 StratusAgent 真实恢复）
- [`BUG-batch-pending-drift-deadlock.md`](2_bugs/BUG-batch-pending-drift-deadlock.md)（状态机与修复见证已修复，等待发布后重启原批次）
- [`BUG-bridgeforge-codex-145-end-to-end-acceptance-gaps.md`](2_bugs/BUG-bridgeforge-codex-145-end-to-end-acceptance-gaps.md)
- [`BUG-hooks-template-stale-context-budget-comment-blocks-downstream-upgrade.md`](2_bugs/BUG-hooks-template-stale-context-budget-comment-blocks-downstream-upgrade.md)
- [`BUG-shared-skill-manifest-line-endings.md`](2_bugs/BUG-shared-skill-manifest-line-endings.md)
- [`BUG-migration-drops-project-pre-commit-extension.md`](2_bugs/BUG-migration-drops-project-pre-commit-extension.md)
- [`BUG-bridgeforge-references-omitted-from-user-skill.md`](2_bugs/BUG-bridgeforge-references-omitted-from-user-skill.md)（真实安装与回归已通过，等待用户验收）
- [`BUG-codex-native-memory-empty-snapshot-reconcile.md`](2_bugs/BUG-codex-native-memory-empty-snapshot-reconcile.md)
- [`BUG-create-worktree-sandbox-half-created.md`](2_bugs/BUG-create-worktree-sandbox-half-created.md)

## 归档与参考

- [骨架社区对标与问题清单（2026-10-24）](3_reference/skeleton-benchmark_2026-10-24.md)：关联 TODO-026～TODO-031；第 1 项退役已完成本地实现和工程验证，其余五项待讨论。当前已冻结，恢复位置为 TODO-027，详见文首进度记录。

`4_archive/` 内现有文件为迁移前历史档案，继续保持可追溯性；Codex harness parity 设计与交付材料已归档到 [`codex-harness-parity-design.md`](4_archive/codex-harness-parity-design.md) 和 [`codex-harness-parity-delivery/`](4_archive/codex-harness-parity-delivery/)，旧运行手册打包结构见 [`bridgeforge-doc-runtime-packaging`](4_archive/delivery/bridgeforge-doc-runtime-packaging/summary.md)。新归档按 `4_archive/delivery/<topic>/` 或 `4_archive/bugs/` 落位。外部资料与仅供参考的实现放入 `3_reference/`，不作为运行时资产。

| 归档日期 | 类型 | 记录 |
|---|---|---|
| 2026-08-30 | Delivery | [`bridgeforge-command-clarity`](4_archive/delivery/bridgeforge-command-clarity/) |
| 2026-08-30 | Delivery | [`bridgeforge-switch-direct-sync`](4_archive/delivery/bridgeforge-switch-direct-sync/) |
| 2026-08-30 | Delivery | [`bridgeforge-switch-semantic-migration`](4_archive/delivery/bridgeforge-switch-semantic-migration/) |
| 2026-08-30 | Delivery | [`codex-cost-routing`](4_archive/delivery/codex-cost-routing/) |
| 2026-08-30 | Delivery | [`codex-model-routing`](4_archive/delivery/codex-model-routing/) |
| 2026-08-30 | Delivery | [`codex-model-routing-56`](4_archive/delivery/codex-model-routing-56/) |
| 2026-08-30 | Delivery | [`codex-project-zone-ownership`](4_archive/delivery/codex-project-zone-ownership/) |
| 2026-08-30 | Delivery | [`codex-skeleton-refactor`](4_archive/delivery/codex-skeleton-refactor/) |
| 2026-08-30 | Delivery | [`codex-skill-routing-dispatch`](4_archive/delivery/codex-skill-routing-dispatch/) |
| 2026-08-30 | Delivery | [`codex-subscription-routing`](4_archive/delivery/codex-subscription-routing/) |
| 2026-08-30 | Delivery | [`create-worktree-skill`](4_archive/delivery/create-worktree-skill/) |
| 2026-08-30 | Delivery | [`memory-lifecycle-governance`](4_archive/delivery/memory-lifecycle-governance/) |
| 2026-08-30 | Delivery | [`token-context-optimization`](4_archive/delivery/token-context-optimization/) |
| 2026-08-30 | Bug | [`BUG-downstream-business-version-rule-without-enforcement.md`](4_archive/bugs/BUG-downstream-business-version-rule-without-enforcement.md) |
