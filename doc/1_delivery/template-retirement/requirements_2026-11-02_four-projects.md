---
lifecycle: completed
validation_status: verified
---

# 四个下游 Template-only JSON 退役

## 最新授权：完整退役阶段（2026-11-03，以本节为准）

用户明确要求“只有 template/hooks.json 应保留，其余 JSON 都应退役；四个下游一并清理”，审阅完整计划后再次明确“开始吧”。本节替代上一阶段“必要文件暂留”的终态；功能变化和实施授权已确认，不再重复询问。

唯一完成标准：BridgeLibrary、CausisRiskSuite、HoldemTrainer、StratusAgent 的 `.codex` **递归** JSON 相对路径集合均严格等于 `["hooks.json"]`，并且正常命令不再读取、初始化或重建其他项目 JSON。不建立 `.bridgeforge/`，不把项目旧配置或账本转成另一格式、塞进 hooks.json 或搬到其他在线载体续用。

最新实盘共有 27 份 JSON：BridgeLibrary 6、其余各 7。保留四份 hooks.json，退役剩余 23 份：版本配置 4、分发清单 4、收口账本 3、目录覆盖 1、Hook 旁文件 3、嵌套 TODO 静态 JSON 4、嵌套旧工具指纹 JSON 4。前阶段已删九份不重复计入。关联骨架身份戳随旧机制一并处理；普通业务 JSON、账户资料、用户级配置不在删除范围。

完整计划包含退出 Forge 配置化版本登记、开发收口账本及相应旧发布门禁；项目原生版本文件、构建、测试、普通 Git 和有效项目 Hook/Skill 保留。旧接口应明确退役，不能缺文件就跳过验证并返回成功。新公共基线依据通用模板规则及工具来源证明，不继续要求项目侧合同副本。静态 TODO 规则由工具原生实现承担，不保存项目私有数据；旧指纹 JSON 及对应旧识别机制退役。需要适配规则、Skill、工厂自举、构建及源码投影。

原预算余额不足以承载当前架构退役，且原两轮验证已用。用户已明确回复“采用该累计预算，完整实施”：累计上限16小时、240k新增token估算、复用现有一个独立复核Agent、累计四轮验证，**包含此前已用部分，不重置**。正在建立新保护基线和架构复核，不再重复确认目标/功能取舍。token未实测，不编造周额度比例。

接续顺序：冻结新保护基线 → 旧机制与接口退出 → 无项目旁文件的新基线及构建适配 → 隔离退役/恢复/不重建验证 → 独立复核 → 同一固定候选清理四项目23份JSON → 递归集合断言、Hook/Git/业务保护及重复更新核验。只在全部完成后报告完整达成，不再用部分清理代替。

## 当前交付结果（2026-11-03）

**完整实施、独立终审及用户验收通过。四个真实项目的 `.codex` 递归 JSON 名单均严格等于 `hooks.json`。** 不把整个 `.codex` 的非 JSON 私有文件称为相同；有效业务 Hook 与 Skill 仍各自保留。

## 用户验收（2026-11-03）

用户显式调用 `$summary accept`，确认本次交付符合需求。验收范围为四项目 JSON 名单统一、23 份 JSON 及 4 个旧版本戳退役、正常 Forge CLI 不再依赖／重建旧配置、私有 Hook／Skill 和业务／Git 现场保护；依据为下文真实四仓、原生回归及独立终审记录。实际业务 GUI、真实 Vault 复制和用户级安装仍不属于已验证范围。

本主题未建立单独 TODO 条目，不结算其他主题。用户同时明确授权当前 BridgePilot 项目的 `$git-sync release`；本段仅记录验收与发布授权，不能作为发布成功证据，发布结果另行追加。四个下游的提交、推送及发布不包含在本次 Git 授权内。

| 项目 | 本阶段删除 JSON | 关联旧版本戳 | 当前递归 JSON | 基线／重复计划 |
|---|---:|---:|---|---|
| BridgeLibrary | 5 | 1 | 仅 hooks.json | clean／零动作 |
| CausisRiskSuite | 6 | 1 | 仅 hooks.json | clean／零动作 |
| HoldemTrainer | 6 | 1 | 仅 hooks.json | clean／零动作 |
| StratusAgent | 6 | 1 | 仅 hooks.json | clean／零动作 |

合计本阶段退役 23 份 JSON＋4 个戳；前阶段已删 9 份不重复计入。真实更新、已安装 CLI／Hook 自检、严格 config-health、完整工作区基线、旧接口 exit2 不重建、重复计划全零均已实际执行。暂存树正例／额外 JSON 反例只在隔离仓验证；真实暂存区保持，未用暂存或提交伪造通过。

### 工具与功能边界

- 四仓使用同一固定公共候选：CLI SHA-256 `73e8236f5d9ae113bca3b587f2696d3acf373f4cef0e8ac71942e16432802611`；Hook `83eb7bccffcdb91e372d7af2ebf445029f8d143dcdd9f1b2bfcc7f201af8566a`；compiled source `sha256:ef0504dcb71a3fdcc3ba7204b7fd41a1850dc23c59b3ee46329c8d17735cc467`。
- 正常 Forge CLI 不再解释、初始化或重建旧项目 JSON。精确退役名单仅用于一次性原字节备份／清理；未知额外 JSON 阻断。没有 `.bridgeforge/`，各 hooks.json 顶层仍只有 description／hooks，没有转存旧 registry、版本或账本。
- 旧配置化 develop、release/setup/migration、项目本地公共工具构建及依赖旧 keep 的 cleanup 明确退役；普通 Git 同步、原生构建测试、现有业务 Hook／Skill 保留。BP 旧账本自动认证源码明确拒绝，不将任务状态伪装为 Released。
- 全部非 JSON 公共 Rust 源码已按固定源投影更新，原 core 依赖可锁定冷构建；TODO 通用规则为原生 Rust，旧指纹资源及对应自动识别退出。
- 四个必要精确适配均有 before／after 哈希和备份：BL 原生测试从 Cargo 当前工件消息选择依赖，`-SkipBuild` 明确拒绝；SA 测试断言旧旁文件缺失；CRS／SA INSTALL 改为原生发布指引。没有更改业务版本。

### 验证与保护

- 第三轮分批回归 39 项通过；第四轮工厂镜像／路径边界定向 9 项通过。这些包含复验，不相加为不同用例总数。
- 四个隔离副本的完整清理、暂存树正反例、重复更新和旧接口不重建通过；共享事务的回滚／并发保护用例通过。官方 updater 组件在隔离 profile 成功安装来源匹配的 prepared CLI，没有修改真实用户安装。
- 私有 Hook 在清理后的隔离及真实工作区均完成同组原生验证：BL 31 passed、4 原有 ignored；SA 50 passed。BL WinPS5 的预期 stderr 中断、SA 缺测试父目录／WinError206 的先前失败日志保留，最终采用当前 pwsh7 和合法短临时目录，没有删除或放宽断言。
- 主对话及同一个独立只读复核 Agent 重算确认：27 个退役对象的恢复字节与原始 zip／SHA 一致，16 个实际工具／收据与 prepared pair 一致；私有 Hook／Skill 16 文件、AGENTS 项目区、pre-commit 扩展、native handlers 保持。283 条既有业务文件差异逐项核验，四项适配仅含已审阅增量。
- 四仓 HEAD、逻辑 index、refs、stash、Git 配置保持，hooksPath 仍为 `.githooks`；5 个已有成品／私有 Hook 文件哈希不变。普通业务资料和依赖环境未迁移。
- 工厂自身 manifest、factory-template 基线及三工具只读检查通过；工厂专有文档索引保持。工厂自有历史记录不纳入四下游删除范围，不能据此放宽下游 JSON 唯一标准。

实际业务 GUI、真实 Vault 复制、真实提交／推送／发布未执行；4 个 BL ignored 用例未冒充通过。用户级共享安装未升级；本次是 16.1.0 的本地源码候选，不是正式发布。没有自动创建任务、聊天、heartbeat、TODO 或用户验收记录。

### 证据与恢复

统一证据根：`.runtime/tests/template-only-retirement/20261103-01/`。

- `real-verification.json`、`real-runs/<项目>/report.json` 及其 attempt：真实计划、apply、基线、接口拒绝和零动作重复计划。
- `public-artifacts-final.json`、`fixed-candidate-input-r2.json`、`final-candidate-build-r2.log`：固定源及公共 pair；原失败／首候选日志另存。
- `protection-before.json`／`protection-after.json`、`current-business-outputs-before.json`／`after.json`：Git、业务差异、私有区域和成品保护。
- 各项目第二阶段 `protected-before.zip` 与 baseline.json、原生适配两份 before.zip、27 项 apply 的 recovery 路径：可核验恢复，备份只用于恢复，不作为在线配置。
- `round3-*.log`、`round4-factory-boundary.log`、`round4-updater-component.log`、`round4-*-native*.log`：真实执行与环境失败记录。

未提交、推送或发布。原始恢复资料保留，未经另行授权不删除。以下实施过程和前阶段结论均为历史，不代表当前仍有 23 份待清理。

### 完整退役阶段实施进展（过程历史）

- 已建立第二阶段五仓保护基线、冻结旧工具，仅用于恢复；四下游本阶段尚未删除 JSON。
- 新增独立 Template-only 同步与基线：公共规则编译入工具，不解释项目旧合同；已知退役项原字节备份，未知额外 JSON 阻断。事务删除后验证，再复核写集及退役缺失；并发变化不被回滚覆盖。
- 公共非 JSON Rust 源码保留并完整刷新，保护 BridgeLibrary 的原生 core 依赖；两份源码 JSON 的消费者分别改为通用原生 TODO 规则与旧指纹迁移明确退役。未扩大为删除整个公共源码树或迁移私有 Hook。
- 新增编译期 source/lock 来源证明；计划重算公共源码身份，绑定目标原字节、预期写入及工具原字节。既有工具需有 schema2/id/platform/二进制摘要归属，未知或修改目标拒绝覆盖。公共产物准备验证真实 self-test 来源；官方 updater 的全仓 Cargo bootstrap 仅作 helper，实际安装来自公共源投影的准备产物。
- 普通 Git 同步退出旧版本配置依赖，保留 Git 保护；旧 develop、配置化 release/setup/migration 和依赖旧 keep 配置的 cleanup 明确退役。BP 旧账本自动交付证明同样明确拒绝，不将任务改成 Released；原生构建、测试、版本和人工审阅仍是项目职责。
- AGENTS、共享 Skill 和项目 Rust Hook 说明已按新规则修改，尚在完成自身镜像及固定候选验证。
- 第三轮稳定化验证当前通过：7 项迁移/工具归属/退役清理回归，1 项 BP 状态不推进回归。工作区编译在恢复 Native Memory 共用参数辅助函数后通过；早先测试目录层级及 Windows 只读测试对象收尾失败已修正。一次无效候选构建在安装前终止，保留现场；不计为成功构建或真实迁移。

工厂自有历史开发记录不纳入四下游删除集合；新正常入口不把它们作为在线配置或重建。工厂使用单独的 factory-template 基线，不能据此放宽任何下游的递归 JSON 唯一标准。尚需：稳定候选构建、真实工具隔离应用及重复核验、两处原生测试适配、最终独立复核、四项目精确清理及保护对账。本节进展不表示完整达成。

第四轮已进入真实候选验证：首个候选三工具受管构建成功，但新工厂完整镜像暴露自身 `doc/README.md` 与下游索引结构不同，manifest 在写前阻断，原索引保留。已将工厂专属索引及 seed 排除出工厂镜像，生成和基线共用同一筛选；下游仍校验完整 Template。Windows 注册路径转换同样收敛为生成/校验共用函数，9 项定向回归通过，独立静态复核无阻断。修正后的原生工厂已真实重建镜像并断言索引字节不变，第二个候选正在受管重建。首个成功构建及后续镜像失败各自保留，不作为最终通过证据。

第三轮最终为 39 项通过：模板/工具归属/退役入口 8、迁移保护 5、CLI（含 Native Memory）25、BP 状态保护 1；Windows 沙箱拒绝隔离夹具目录联接/只读 Git 对象收尾后，经相同用例提升权限通过。四仓迁移前实盘对账通过，283 条既有业务文件差异、HEAD、逻辑暂存、refs、stash、私有区和现有配置均保持。四份原生测试/安装说明的精确适配已暂存及备份，真实下游本阶段仍未改写或删除。

以下为上一阶段历史，保留原决定、证据与实际结果，不代表最新终态。

**2026-11-03 本轮交付：四项目九份已确认失效配置处理完成；必要配置、身份和账本按用户后续选择保留。完整 Template-only 仍为 0/4，未宣称实现。**

## 用户决定与授权

2026-11-02 用户已确认实施：BridgeLibrary、CausisRiskSuite、HoldemTrainer、StratusAgent 以现有 Forge Template 及既定分发映射为公共文件名单依据；`.codex` 顶层 JSON 只保留 `hooks.json`，保留 `.githooks/pre-commit`。模板外文件原则上退役，并收掉对应旧读写、保留、生成和验证机制，不新建 `.bridgeforge/`，不换路径或格式继续使用旧配置。

用户已明确“开始吧”。实施目标和实施授权不再重复确认；不提交、推送、release，不擅自取消开发发布或项目业务能力，不丢失原有工作区、暂存内容、业务资料、当前成品和历史。

### 上一阶段范围调整（历史）

预算确认后，主对话及独立复核实际确认：分发合同、版本配置、账本和身份戳仍是现行功能必需；artifact-layout 仍保护 Holdem 当前成品。用户随后明确选择“保留现有受管开发发布能力；先清理已失效项，必要文件暂留”。因此本轮仅退役四份 settings、三份旧策略、两份旧迁移状态，共九份遗留配置；必要的版本配置、账本、身份、目录规则、有效 Hook 与项目 Skill 保留。完整 Template-only 名单是后续需重新设计的方向，不作为本轮已达到或需强行删除的验收条件。不执行任何迁移安置方案。

这是跨模块退役及数据清理，L 级。用户已明确回复“采用该预算，继续完整实施”：最多 6 小时、120k 新增 token 估算（含调查、主 Agent、一个独立复核 Agent、验证、返工与收尾）、最多 1 个独立复核 Agent、2 轮验证。token 未实测；本机 `.runtime/task-budget/weekly-quota.json` 不存在，无可用周额度换算系数，不编造消耗百分比。

## 已核实事实

| 项目 | 顶层 JSON | 明确遗留/可选覆盖退役目标 | 仍有现行依赖的目标 |
|---|---:|---|---|
| BridgeLibrary | 7 | settings、旧策略、旧迁移状态 | 分发清单、版本配置、Hook 登记 |
| CausisRiskSuite | 7 | settings、旧策略 | 分发清单、版本配置、收口账本、空 Hook 登记 |
| HoldemTrainer | 6 | settings、artifact-layout | 分发清单、版本配置、收口账本 |
| StratusAgent | 8 | settings、旧策略、旧迁移状态 | 分发清单、版本配置、收口账本、空 Hook 登记 |

共 24 个模板外顶层 JSON 纳入退役处理范围。另核对四项目版本戳、空 `.codex/scripts`、项目 Hook 及专属 Skill。当前公共 Rust 源码投影中的 JSON 属于现有分发合同，不能仅按名称含 legacy 判为垃圾。

当前代码事实：

- `source/forge/workflow/src/project_sync.rs` 对 settings、旧策略及旧迁移状态仍显式 required-preserve；需要真正退役该特例。
- `source/forge/checks/src/artifact_layout.rs` 仍读取可选 `.codex/artifact-layout.json`，缺失时使用默认规则；Holdem 当前文件包含成品与 Cargo 等声明，删除前须证明现行保护安全。
- `source/forge/lifecycle/src/release/schema4/development.rs` 在线账本固定为 `.codex/package-closures.json`；Git 同步/发布仍读取 `.codex/bridgeforge-version.json`。
- `source/forge/skeleton/src/baseline.rs`、`project_sync` 当前仍依赖 `.codex/managed-skeleton.json` 和骨架版本戳。
- BridgeLibrary 有 Vault Hook 源码及四个项目 Skill，StratusAgent 有独立 Rust Hook 及 acceptance Skill。不能将它们当作失效配置混删，也不能执行已被用户撤回的迁移安置方案。
- Forge 工作区有本轮之前已存在的 BP 安装、源码更新、清理入口及源码根迁移改动，必须保持来源与内容，不通过全仓 reset/stash 清理现场。

## 阶段和完成条件

| 阶段 | 任务 | 门槛 |
|---|---|---|
| P01 | 保护 Forge 与四下游 Git、文件、权限、配置和历史 | 可校验恢复；活动写入与事务已核实 |
| P02 | 逐文件消费者、功能影响及退役设计 | 不通过换路径、换格式延续旧配置；不静默移除未授权业务能力 |
| P03 | Forge 同步、检查、Hook、版本/身份机制收口 | 不再要求退役文件，不自动重建 |
| P04 | 隔离验证与自身 dogfood | 漂移、中断、回滚、重复执行和功能保护可证明 |
| P05 | 独立复核与适用受管收口 | 复核通过，不伪造旧版本账本或收据 |
| P06 | 固定工具来源的四项目 dry-run | 完整逐路径清单，无未知冲突 |
| P07 | 逐项目备份、清理、验证 | 每项目独立收口，不批量盲删 |
| P08 | 最终名单对比及重复预览 | 公共名单一致，无旧文件重建或隐式依赖 |

## 清理、回退及停止条件

各项目恢复资料位于 `.runtime/tests/template-retirement/<批次>/`，仅作本次处理证据和恢复副本，不作为在线配置。记录原字节、属性、必要权限、摘要和 Git 差异。dry-run 零写入；apply 前重核指纹；已退役且归属可证的文件进入事务；未知、被改写或仍有依赖的文件阻断，不静默保留后报告整体完成。

未核销事务、活动写入、占用、链接异常、路径越界、备份不完整、当前成品保护失效、历史损坏、新旧工具合同不匹配时，停止受影响操作并保留现场。回退不 reset、不覆盖用户变化；恢复副本最终删除另行授权。

## 当前进展

进度必须区分：本轮用户选择的“失效项阶段处理”和最初“完全 Template-only 收敛”。不能将某项目九类旧文件中的适用项处理完毕，报告成完整模板收敛。

- Forge 已实现精确旧配置退役、未知内容及当前输入引用阻断、缺失状态与指纹保护；项目级旧 settings 检查退出，用户级保留。
- 工厂 config.toml 镜像遗漏导致首轮工具安装检查回滚，已修复并经精确回归验证；最终候选工具及公共产物已准备。
- 受管四项检查实际通过 57 个用例；factory 0.10.1→0.10.2、workflow 0.9.2→0.10.0 收口完成，主 VERSION 16.1.0 不变。一次日志目录缺失的环境失败已回滚并补齐目录，源码与验收集未因此变化。
- 四项目均已执行本轮精确退役，最终 apply succeeded、baseline clean；重复预览 safe/risk/gaps/blockers 全部为 0，已安装公共工具及收据字节一致。必要文件、有效 Hook 和 Skill 保留。
- CRS 原旧测试日志有 847 条可读记录和 4 行损坏内容，完整原文件已原样备份并隔离在该项目本轮恢复区，未修补或导入为通过证据。
- CRS 既有 product_inputs 同 owner 的 scripts/ 与四个子路径冗余声明阻断了原配置入口。已修复 lifecycle 解析器：只在临时检查集合中过滤同属 product 的严格覆盖子路径，原字段不改；精确重复、非法路径、product/shared 和 shared/shared 冲突仍拒绝。3 项定向回归与 55 项消费者复验通过，第二轮共 58 项；lifecycle 0.8.5→0.8.6，factory/workflow 仅刷新依赖覆盖、不再升版。
- 新源码 CLI 对真实 CRS 受管 configuration-plan/apply 成功，只移除已被 scripts/ 覆盖的四个冗余子路径，覆盖范围、包、检查、baseline 和账本保持。另通过受管入口解除 Holdem 两条失效 product_inputs 及 Stratus 一条失效 record_inputs 引用；未手改历史或伪造账本。
- 独立复核已实际重算四项目安装工具、备份、账本、私有 Hook/Skill、项目专区、Git 与 283 条原有业务差异，无阻断问题。
- 失效项阶段 4/4、已删除 9/9；完整 Template-only 仍为 0/4。后者因用户确认保留现行受管能力而尚未实施，不能宣称已经达成。

恢复证据位于 `.runtime/tests/template-retirement/20261102-01/` 及各下游同名本机恢复目录。未提交、推送、发布，未新增 `.bridgeforge/` 或迁移续用旧配置。后续只在实际结果改变时更新状态，不自动新增 TODO、任务 Run 或正式验收记录。

## 本轮逐项目结果

| 项目 | 删除的失效配置 | 数量 | 保留的顶层 JSON 数 | 实际结果 |
|---|---|---:|---:|---|
| BridgeLibrary | settings、旧策略、旧迁移状态 | 3 | 4 | apply succeeded；baseline clean；重复计划零动作 |
| CausisRiskSuite | settings、旧策略 | 2 | 5 | 同上；登记冗余由受管入口修复 |
| HoldemTrainer | settings | 1 | 5 | 同上；artifact-layout 原字节保持 |
| StratusAgent | settings、旧策略、旧迁移状态 | 3 | 5 | 同上；私有 Hook 与 acceptance Skill 保持 |

`.githooks/pre-commit` 保留，四项目实际 core.hooksPath 均为 `.githooks`；私有扩展段保持。三个旧 `.codex/scripts` 仅含空 `__pycache__` 子目录，首次非递归检查正确阻止删除；逐级确认无文件、无链接后，非递归移除六个空目录，没有删除缓存文件或项目资料。

业务 VERSION、HEAD、逻辑暂存条目、refs、stash 和原有业务文件保持。四下游从原骨架 14.0.0 使用本轮 **16.1.0 本地候选**，并非本轮正式发布；上游主 VERSION 也保持 16.1.0。用户级 Skills／程序安装未升级，真实业务程序未启动。

## 验证和证据

- 第一轮：四项已登记检查执行 57 个 Rust 用例；第二轮：三项定向检查执行 58 个（含消费者复验），均通过。两轮共 115 次用例执行，不能表述为 115 个不同用例。
- 源码必要修复及最终交付由同一独立只读 Agent `/root/retirement_review` 复核，未由复核者修改文件或运行测试。
- 九份原配置、原工具、Git／文件保护基线及权限摘要已校验备份；14 份旧运行记录由原事务迁往 `.runtime/tests/legacy-test-receipts/`，逐项字节不变。
- CRS 损坏日志原始 SHA-256 为 `5c737ae8b48788bf69377f2ea06ee6ea94fcfbaedee11e2885e91f901e4eb1c2`，保留在 `.runtime/tests/template-retirement/20261102-01/quarantined-legacy/receipts.jsonl`，无证据导入。
- `verification-final.json`：四项目原有效配置、三份账本、私有 Hook/Skill、AGENTS／pre-commit 项目段、Git 与 283 条既有业务差异保护核验通过。
- `installed-tools-final.json`：四项目最终工具和收据哈希一致。
- 各项目 `project-final-applied-plan.json`、`project-final-apply.json`、`baseline-after.json`：最终真实应用及 clean 结果；当前 `project-plan.json` 为应用后的重复预览，零动作、零阻断。
- `development-apply-retry.json`、`ownership-development-apply-final.json`：两轮 development-closed、版本与执行范围；失败日志保留，不将日志目录缺失或过期分析视为验证通过。
- 上游最终工具与 manifest 检查通过，本轮 lifecycle/workflow/factory 范围最终预览为 task-development-current、affected_packages=[]，不据此宣称全仓 release 就绪。

本轮范围已交付，尚待用户验收；完整模板收敛、必要机制的进一步退役、正式分发与发布不在本轮完成声明中。
