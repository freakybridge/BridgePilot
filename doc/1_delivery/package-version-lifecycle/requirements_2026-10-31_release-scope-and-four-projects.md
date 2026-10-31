---
lifecycle: active
validation_status: awaiting_user_acceptance
---

# 发布范围判定修复与四个下游骨架升级工作计划

日期：2026-10-31。关联原需求：**TODO-051、TODO-052**，见[项目待办总表](../../0_architecture/TODO-INDEX.md)。本文件是本任务唯一的需求与工作计划，后续实施、验证及验收增量在此维护。

## 1. 用户目标与本轮授权

用户要求：

1. 修正 BridgeForge 骨架的发布范围判定问题。
2. 修复后，逐一对四个下游项目安装最新骨架。
3. 通过 `$confirm` 生成新的工作 plan。

用户随后明确确认四个下游为 StratusAgent、HoldemTrainer、causis_risk_suite、BridgeLibrary；第四个不是历史记录中的 BridgePersonalAssist。

本轮授权为需求核验、计划落盘、同步文档索引及新增关联 TODO。此前“先不要修改”与本轮“生成新的工作 plan”共同确定当前边界：本轮不修改可执行源码，不构建，不安装，不升级下游，不提交、推送或发布。上述两项是后续实施目标，不把计划确认等同于立即开工授权。

未来开工时继承现有决定，不重复访谈；实施、工厂提交推送发布、用户级分发和真实下游写入分别核对届时有效授权。四项目安装不包含下游业务代码修复、业务版本收口、业务发布或完整业务回归。

## 2. 已核实事实与未核实部分

- BridgeForge 本轮检查位置为 `D:\Quant\BridgeForge`，分支 `main`，HEAD `2708d71`，根 `VERSION` 为 `13.0.2`。写计划前工作树干净。此版本是本地现场信息，没有据此宣称它是官方最新版本。
- `source/lifecycle/src/release/schema4/development.rs` 的 `release_scope` 从第 1148 行开始；约第 1207–1225 行在逐包循环中比较整个 `shared_inputs` 字段。任意局部共享映射变化都会使每个尚未纳入范围的现存包满足配置变化条件，问题仍存在。
- release gate 使用这一范围检查版本覆盖；develop 预览约第 1634 行也复用它。因此修复须同时验证两个调用入口。
- 该源码与 `.codex/hooks/source/lifecycle/src/release/schema4/development.rs` 的 SHA-256 一致，不能只改镜像绕过上游源。
- `uses_build` 对显式全局 `build_inputs` 返回所有包使用；这与 `shared_inputs` 的误扩范围是不同机制，本次不取消全局输入合同。
- 现有范围还包括实际源码变化、生命周期端点、未发布收口记录、声明的 release artifacts 和依赖消费者；这些保护不能因修复误扩范围而丢失。

下游只读取了版本戳、目录信息及可访问的安装说明，未执行 project-sync 或业务验证：

| 执行顺序 | 项目根目录 | 当前可读骨架戳 | 安装入口现状 |
|---|---|---|---|
| 1 | `D:\Quant\StratusAgent` | `13.0.1` | 根 `INSTALL.md` 存在，说明受管骨架维护及项目保护边界 |
| 2 | `D:\Quant\HoldemTrainer` | `9.0.0` | 根 `INSTALL.md` 存在；业务环境步骤不作为骨架更新前置执行 |
| 3 | `D:\Quant\causis_risk_suite` | `13.0.1` | 根 `INSTALL.md` 存在；业务配置、知识库及凭证维持项目所有权 |
| 4 | `D:\Quant\BridgeLibrary` | `9.0.0` | 根 `INSTALL.md` 缺失，须在安装前根据真实入口补齐最小说明 |

四者未发现旧版双戳。当前权限下，下游 Git 状态读取失败，不能把空输出计为工作树干净；分支、现有改动、占用及可写性必须在实际安装前重新核验。BridgeLibrary 的 `vault` 是链接，列为保留对象；不遍历、迁移或修复链接目标，若同步器认为其影响安全则停止该项目。

此前 Stratus UI 对话中的“41 个缺记录、10 个内容变化”仅是问题来源，不是本轮重新核验的项目结果。不能承诺本次修复消除全部阻断。

## 3. 规模、预算及传播四问

暂定 **M**：逻辑修复集中在现有发布范围函数，四个下游使用现有同步事务，不新增迁移架构或批量升级器。沿用 confirm 的 M 默认预算：45 分钟、20k 新增 token 估算、最多 1 个子 Agent、最多 2 轮验证；预算按整个后续任务累计，token 未实测，不因切换 Skill 或项目重置。当前不启动子 Agent。实际开工先评估必要受管构建和四项目维护成本；预计超出预算或出现未知数据迁移时，先核准调整或缩小范围，不静默追加。

| 传播问题 | 本次决定 |
|---|---|
| 产品层、自身配置层还是元文档 | 范围算法属于产品层；本轮需求卡、索引与 TODO 为元文档 |
| 通用能力还是工厂专属 | 通用发布能力，不能加入四个项目名、UI 包名或路径白名单 |
| 是否涉及版本与 CHANGELOG | 修复完成后受管 develop 收口受影响子包；获授权的 release 统一处理主程序版本与 `[product]` CHANGELOG，不手工预写版本 |
| 是否同步自身 dogfood | 是，源码、公共投影、受管清单与本仓库运行时均须对应最终候选；禁止对本工厂执行下游 adopt/apply |

## 4. 分步任务与完成标准

### P0 / TODO-051：修复局部共享映射导致的全包范围扩大

1. 开工时复核当前源码、调用入口和已有改动，保留本轮仍有效的调查结论。
2. 在 `source/lifecycle/src/release/schema4/development.rs` 中，将 `shared_inputs` 从整字段全局判定中单独取出，按变化的具体映射计算受影响身份；比较规范化后的路径与所有者集合，纯数组顺序变化不扩大范围。
3. 覆盖映射新增、删除、路径变化和所有者转移；核对发布边界与当前快照的有效归属，包括取消或新增显式映射时的源码目录自然归属，避免漏掉失去文件归属的旧包。
4. 保留现有依赖消费者传播及生命周期处理。其他配置字段暂不扩大为全套重构；无法可靠读取旧配置时保留保守行为，不能把未知解释为无影响。
5. 使用既有受管准备入口同步 dogfood、生成资产及必要工具；修改涉及的主要文件为上述源码、对应 `.codex/hooks/source/` 镜像、现有测试和受管生成的清单/覆盖记录，不创建第二份规则来源。

### P1 / TODO-051：定向验证与受管开发收口

主要测试载体为 `scripts/tests/unit/core_package_development.rs`；生命周期既有回归位于 `scripts/tests/unit/core_release_schema4.rs`。沿用现有 fixture、runner 和断言方式。

| 必须覆盖的场景 | 验收标准 |
|---|---|
| 仅 A 的共享登记变化，无关 B 缺版本记录 | B 不因这次局部登记变化进入 release/develop 范围 |
| 映射新增、删除、换路径、转移所有者 | 实际新旧所有者都进入必要范围，包含自然归属回退 |
| 只重排映射或所有者数组 | 不制造额外包影响 |
| D 依赖受影响 A | D 仍被覆盖，无关独立包保持范围外 |
| 新 UI 合并、旧包退役 | 生命周期两端仍被识别，不要求为退役包重复执行独立业务测试 |
| 真实共享源码或显式全局构建输入变化 | 原有保护与消费者传播不被削弱 |
| 声明 release artifacts，或存在未发布收口记录 | 原有必要范围仍保留 |
| release 仍有实际证据缺口 | 正确停止且不启动 develop 检查、不写版本或账本 |
| 只读预览与候选漂移 | 预览零写入，已有事务漂移保护保持 |

先取得失败复现，再在相同场景验证修复。通过 root workspace 的受管测试配置、锁定 Cargo 和 `.build/` 输出执行所选检查；核对实际用例数，不把 0 项算通过。测试输出进入 `.runtime/tests/release-scope/`。只对与本次改动有关的现有回归扩展验证，不因发布准备重跑全仓或四项目业务测试。

开发收口遵循[工厂开发收口清单](../../0_architecture/design/development-completion.md)；只运行本次必要检查并复用仍有效证据。需要独立审计时按届时有效指令执行，不把计划轻量自查当作独立审计。最终记录源码、产品传播、dogfood、fixture、真实下游与 runtime 六类证据，缺失项标为未验证。完成开发不自动发布。

### P1 / TODO-052 前置：让修复成为可分发的正式骨架

官方 updater 从官方来源取得产品，不能把尚未发布的本地改动称为“最新骨架”。因此正式下游安装前必须核对：修复已完成受管开发收口，且已通过获授权的工厂 Git/release 流程进入官方可分发来源。

本轮没有提交、推送或发布授权，计划不代为执行。后续没有匹配授权时只暂停正式分发阶段，已获准的开发工作可继续。release 复用有效开发证据，不自动补做 develop；若官方候选缺少本次修复，停止安装，不能通过手工复制源码或改戳冒充安装成功。

### P2 / TODO-052：逐一安装四项目的最新骨架

在上述正式来源就绪且获实施授权后，由主对话使用当时实际可用的 `bridgeforge-codex` Skill，按表中顺序一个项目完成后再进入下一个，不调用旧批量迁移工具。

1. 进入项目后读取其 AGENTS 与根 `INSTALL.md`，重新核对身份、版本戳、工作树和可写性；记录现有改动及保护项。BridgeLibrary 先核实真实安装入口并补齐根 `INSTALL.md`，只建立导航及骨架维护说明，不恢复业务环境。
2. 使用官方受管 updater 获取来源收据与公共产物，核对包含本次修复的版本/提交。每个项目都记录实际 source commit、目标版本和收据；若过程中官方来源更新，先核对变化，不静默把未评估候选用于剩余项目。
3. 明示并核验用户级共享 Skills、工具及用户级 AGENTS 更新范围；AGENTS 差异遵守安装器备份行为。已取得的有效授权不重复询问。
4. 只通过受管 `project-sync --mode update` 生成零写入计划。根据真实 safe/risk/gap/blocker 结果操作；safe-only 在已有安装授权内直接继续，风险选择按单项目合并一次确认。阻断时停止该项目，不跳过保护或手工补资产。
5. apply 使用该计划指纹与匹配公共产物收据，依赖既有事务的漂移复核、备份回滚与末尾写戳合同；不手动更新版本戳、不复制公共源码，不构建项目自有 Hook，不改项目业务检查清单或历史收口账本。
6. 以实际安装收据核对完成状态、版本、产物及保护项；利用同步器已有完整验证收口，只有证据异常时再做针对性检查。记录新增骨架改动与原有改动的区别。若本来已是包含修复的正式最新版本，允许真实 no-op 收口。
7. 对 StratusAgent 仅追加必要的只读范围/发布预览，观察实际剩余阻断是否还来自全局输入或旧证据缺口；不自动调整业务归属配置、补验或发布。其他项目不因骨架安装而启动业务验证。

真实安装是用户计划的目标，不属于任意真实项目测试授权；如开发期间需要复现夹具，使用工厂隔离 fixture。下游写入若受平台权限限制，申请对应范围，拒绝时保留现场并报告，不寻找绕过入口。

## 5. 验收、风险与停止点

- TODO-051 完成标准：局部映射不再误扩至无关包，相关负向保护保持，必要定向验证和受管子包收口通过；用户验收与正式发布分别记录。
- TODO-052 完成标准：四个明确项目分别取得成功/no-op 的真实受管收据，来源包含本次修复；原有业务改动、配置、项目专区、知识资料和项目 Hook 保留；逐项报告旧/新骨架版本和剩余 gap。不把骨架安装当作业务测试、业务发布或自然 Hook 生命周期已通过。
- 最终补验多少包只能由修复后的实际预览确定；显式全局输入、workspace manifest 共享归属、旧证据衔接和 release artifacts 仍可能合理阻断。不承诺消除历史全部 51 项，也不要求全部重测。
- 出现无法证明归属、目标冲突、占用、输入漂移、来源不匹配、版本超出支持范围、工具能力缺失或回滚不完整时，停止相关阶段并明确原因。不得删除未知文件、绕过事务或手改历史账本。
- 一个项目阻断时记录该项目状态；其余项目的独立只读核验可继续，不未经授权改变串行安装顺序或悄悄跳过项目。
- 同一问题连续两次实质修复失败时按项目规则进入 escalate/debate；预算预计不足时停止新增高成本动作，保存进度。

## 6. 本轮实际结果与后续交接

本轮仅核验并保存计划、文档索引与 TODO-051/TODO-052。源码修复、定向复现、构建、受管开发收口、工厂发布、用户级维护及四项目安装均未执行；本卡保持 `lifecycle: active`、`validation_status: not_started`，两个 TODO 均为待执行。

计划文档检查：frontmatter 核对通过；`git diff --check` 退出 0；已安装工具 `.build/bridgeforge/bin/bridgeforge.exe check todo --root .` 退出 0，`errors=[]`，两个新增 TODO 的状态与需求链接均被识别。这些结果仅证明本轮记录检查通过，不代表修复或安装验证通过。

后续实施由 `develop` 消费本卡；上游受管 Git/release 由主对话执行 `git-sync`，正式下游升级由主对话执行 `bridgeforge-codex`。阶段切换不重置预算、不补造授权，也不要求用户重复已确认的四项目名单。

## 7. 实施启动（2026-10-31）

用户明确授权“按这个计划开始实施，先修复并验证 BridgeForge，完成后再逐一升级四个下游；需要发布时再告诉我”。该授权覆盖本计划开发、必要验证与受管开发收口，以及正式来源就绪后的四项目维护；工厂提交、推送和发布仍未授权，正式分发前告知用户。本节更新前面的计划阶段授权，不抹去原计划历史。

修改前实际 `develop --preview --packages lifecycle` 为 `task-development-current`，`affected_packages=[]`，有效上下文为 checks/core/factory/lifecycle/skeleton/workflow，具备任务选择、直接 apply 与版本覆盖能力。后续只能把真实变化包纳入收口，不将上下文包全部升版。当前开始隔离回归与实现，正式下游尚未写入。

## 8. BridgeForge 实施与验证记录（2026-10-31）

### 实际修改

- `source/lifecycle/src/release/schema4/development.rs` 新增共享映射差异的局部影响计算；路径与所有者集合规范化比较，消除数组重排的假影响。旧、新显式所有者均保留，新增或撤销覆盖时按各自快照的有效角色补入自然所有者；沿用原有依赖消费者传播。
- `source/lifecycle/src/release/polyglot.rs` 仅将已有 `natural_owners` 改为 release 模块内可见，复用既有最长目录根与外置源码归属规则，没有另写一套归属算法。
- 全局 build inputs、其他全局配置判断、release artifacts、未发布收口记录和生命周期端点的原有门禁保留。
- 公共源码镜像与受管清单由 `build-assets --factory` 和受管 develop 事务传播，不手工修改源版本或下游副本。

### 复现与定向检查

首次测试命令错误使用了短名配合 `--exact`，实际运行 0 项，未作为通过证据。去掉错误过滤后实际运行 1 项，原实现复现失败：应为 `[a,c]`，实际为 `[a,b,c]`；日志在 `.runtime/tests/release-scope/reproduction.log`。

修复后首个场景通过，随后 8 项范围场景通过；扩展到正式发布历史配置及发布产物声明保护。扩展阶段的 13 项检查为 12 通过、1 个产物夹具失败，原因是夹具未完整声明 portable environment、产物生产者和检查工具；修正的是测试配置，没有削弱生产校验。补验该夹具通过，最终受管候选重新执行全部 10 项新范围回归通过。

| 最终候选的受管检查 | 实际用例数 | 结果 |
|---|---:|---|
| lifecycle-unit，过滤 `shared_input_scope_` | 10 | passed |
| factory-unit，过滤 `git_sync_tests::factory_build_failure_and_input_drift_do_not_apply_release` | 1 | passed |
| workflow-unit，过滤 `release_declared_directory_drift_blocks_reuse_and_final_write` | 1 | passed |

最终候选的三个检查保留原登记命令，仅按合同追加 Cargo 测试名过滤；未执行 factory-unit 完整组或工厂运行时测试组。受管原始日志在 `.runtime/tests/development/{lifecycle,factory,workflow}-unit-tests.log`，完整选定命令在 `.runtime/tests/release-scope/execution-analysis.json`，成功收据为 `.runtime/tests/release-scope/apply-result.json`。

另复用同一最终实现、升版前已通过的三项相关回归：`schema4_retirement_survives_release_and_cannot_reuse_identity`、`task_development_dependency_identity_transition_includes_endpoints_and_consumers`、`package_development_check_inputs_do_not_replace_explicit_build_input_scope`；日志为 `.runtime/tests/release-scope/targeted-regression.log`。后续只有本包版本元数据变化，这三项的测试源码和隔离 fixture 行为未变；它们不冒充本次受管收据中的额外检查。

### 受管收口结果

修改后预览的真实待处理包只有 factory/lifecycle/workflow，其余包为上下文。首次 apply 因分析指纹失效拒绝执行，未写版本；随后在实际权限环境重新预览，核对包范围、版本、原因及所选检查不变后重新生成完整分析，没有直接放宽校验或强行重绑旧证据。

实际执行命令：

```powershell
.build/bridgeforge/bin/bridgeforge-factory.exe develop --apply --root . --packages lifecycle --analysis-file .runtime/tests/release-scope/execution-analysis.json
```

结果为 `development-closed`，`readiness_scope=task`、`checks.scope=final-candidate`、executed=3、reused=0、`git_written=false`。只证明本次任务范围完成，不等于全仓正式发布就绪。

| 包 | 收口前 | 收口后 | 原因 |
|---|---|---|---|
| lifecycle | 0.8.1 | 0.8.2 | 发布范围算法的兼容修复 |
| workflow | 0.9.1 | 0.9.1 | 自身内容未变，仅刷新依赖覆盖 |
| factory | 0.10.0 | 0.10.0 | 自身内容未变，仅刷新依赖覆盖 |

原账本 128 条记录的 hash 前缀全部保留，仅追加 3 条 schema 6 记录，三个检查均为 passed；事务 journal 已清除。根 `VERSION` 保持 `13.0.2`，未提交、推送或发布。

### 最终运行时与剩余阶段

升版前的受管准备实际成功：prepared=3、self_test_executed=true；当时零写入检查为 current。收口后 manifest `--check` 为 changed=false、factory-version 为 healthy，但运行时零写入检查报告 `generated asset receipt drifted: codex.hooks`，因此没有用旧产物宣称最终工具就绪。随后通过根 INSTALL 指定的锁定 Cargo 受管准备入口刷新最终版本，实际结果为 prepared=3、self_test_executed=true、written_files=135、retired_files=0；没有重新执行业务测试。收据为 `.runtime/tests/release-scope/final-prepare-result.json`。

最终只读检查全部退出 0，实际输出保存在 `.runtime/tests/release-scope/final-checks.json`：

- `build-assets --factory --project-root . --check`：current、read_only=true、reused=3、built=0。
- `manifest --root . --check`：changed=false。
- `check factory-version --root .`：13.0.2、healthy=true、issues=[]。
- `check todo --root .`：errors=[]；没有据此结算用户验收或自动改写既有 TODO 状态。
- 随后同一实际权限环境的 `develop --preview --packages lifecycle` 为 task-development-current、affected_packages=[]，收据在 `.runtime/tests/release-scope/final-task-preview.json`。该结果只覆盖本次任务，outside_task_evidence_checked=false。
- 两个修改源码与公共镜像分别做 SHA-256 对照，均一致；最终日志计数分别为 factory 1、lifecycle 10、workflow 1，通过且失败数为 0；`git diff --check` 通过。

| 证据层 | 当前结果 |
|---|---|
| 源码 | 局部映射影响修复已实现 |
| 产品传播 | 公共投影与 manifest 已更新，最终 check 无漂移 |
| dogfood | 源码镜像哈希一致，最终本机工具合同 current |
| fixture | 最终候选 12 项通过，另有 3 项相关既有回归的有效证据 |
| 真实下游 | 四项目尚未安装，等待包含修复的正式来源 |
| runtime | 最终 CLI/Hook/工厂工具受管准备及自检通过；自然 Codex Hook 生命周期和下游业务运行未验证 |

本次上游修复的开发、必要工程验证、受管子包收口和最终工具核验已完成；整体任务尚未完成，仍需正式发布与四项目安装。实施阶段约 30 分钟，未启用子 Agent，token 未实测；未执行全仓业务回归、工厂提交、推送、发布或用户级分发。

后续依赖为：告知用户需要工厂正式发布 → 获有效发布授权后执行受管 Git/release → 从包含修复的官方版本逐一升级四项目。当前不得用未发布本地候选替代正式骨架安装；四项目名单和已取得的后续安装授权继续有效，不重复索取安装授权。

## 9. 发布与下游安装启动（2026-10-31）

在上游修复、验证及最终工具核验完成后，用户对“是否授权执行 `$git-sync release`，发布后继续按原顺序升级四个下游”明确回复“授权”。本次授权覆盖当前修复及其文档、生成资产和版本记录的受管提交、推送和正式发布；发布成功后继承原有四项目安装授权，不包含下游业务提交、推送或发布。

发布复用第 8 节证据，不重跑 develop 或业务回归。具体正式版本、提交和各项目安装结果，以后续实际受管收据为准，不将授权记录当作操作成功。

## 10. 正式发布与四项目安装结果（2026-10-31）

### 发布与用户级准备

受管 `git-sync release` 已成功提交并推送 `origin/main`：`475b04f691e6789e1c5169ae2a4f3e2f795cd621`，提交消息为“fix: 修复共享输入配置变更误扩发布范围”。主程序 **13.0.2 → 13.0.3**，理由为兼容修复；累计子包版本变化只有 **lifecycle 0.8.1 → 0.8.2**。workflow 0.9.1、factory 0.10.0 仅刷新依赖覆盖，其余子包版本保持原值，无新增或退役包。

发布实际重建 3 项受管产物，未补做 develop 或业务测试；工具内部 sync_elapsed_ms=280383，其中 artifact_preparation=249782、commit_and_hooks=10527、push=2553，嵌套阶段不相加。发布后一次独立核对为 HEAD=origin/main=475b04f、ahead/behind=0/0、工作树干净、stash 数为 0。

官方 updater 随后取得同一正式提交，返回 completed；用户级 AGENTS 状态为 noop，没有覆盖或备份动作。公共 CLI/Hook 的 13.0.3 准备收据为：

`C:\Users\bridg\.codex\.cache\bridgeforge\public-ready\b26bcf824a6f584049dcf716b72e77df210c7b910614aba6a3c9d73011954a29.json`

该次 updater 总内部耗时 185117.3 ms；四项目复用这一份不可变公共产物，不在项目端重编公共程序。用户级维护成功后重新读取了正式产品 home 中的 Skill 与 update/transaction 合同。

### 逐项目执行结果

| 顺序 | 项目 | 实际安装前版本 | 当前版本 | 受管结论 | 本轮变更项 |
|---|---|---|---|---|---:|
| 1 | StratusAgent | 13.0.1 | 13.0.3 | 已完成 | 13 |
| 2 | HoldemTrainer | 13.0.2 | 13.0.3 | 已完成 | 11 |
| 3 | causis_risk_suite | 13.0.2 | 13.0.3 | 已完成 | 11 |
| 4 | BridgeLibrary | 9.0.0 | 13.0.3 | 已完成 | 72 |

变更项来自各自 human.changed_count，包含公共工具产物，不等于 Git 新增变更文件数，也不包含安装前已有改动。四项 human.pending 均为空，human.next_step 均为“需要保存到 GitHub 时运行 $git-sync”；实际执行均 succeeded、stamp_written_last=true、无回滚。原始安装收据及汇总保存在工厂 `.runtime/tests/release-scope/{Stratus,Holdem,Causis,BridgeLibrary}-apply-result.json` 和 `installation-final.json`。

HoldemTrainer、CausisRiskSuite 的实际现场已是 13.0.2，安装遵守现场版本，没有依照旧计划中的 9.0.0/13.0.1 戳回退。替换的风险项均为既有 whole-owned 公共资产，已继承用户“安装最新骨架”的明确决定；Stratus 的 7 项及 BridgeLibrary 的全部 47 项已与原安装合同 current_sha256 对照一致，没有自定义覆盖。各项目自身配置、业务改动和 Hook 不因此变成工厂资产。

BridgeLibrary 已按授权先补齐根 `INSTALL.md`，并在自身 `doc/README.md` 中新增一行导航。其第一轮安装因缺少 `--confirmed-preservation-manifest` 在骨架写入前停止；源码核对表明这是确认既有清单的布尔开关，无需另造清单。清单只有两项已被历史摘要证明归属的旧公共运行时代码退役，不是项目 vault Hook。用户随后明确将本任务总时间预算由 45 分钟延长至 60 分钟，实施范围与验证上限不变；补上该确认开关后，原计划指纹仍有效，安装成功。

### 保护核验与验证边界

四项目安装前后核对结果均为：业务 `VERSION`、所核对的项目 settings、版本管理配置、既有 package-closures 账本及 AGENTS 项目专区摘要一致，Git HEAD 不变。原有 INSTALL.md 的三个项目内容也保持一致；BridgeLibrary 的 INSTALL.md 为本轮明确计划的新建文件。证据在 `.runtime/tests/release-scope/downstream-before.json`、`downstream-after.json`、`downstream-protection-checks.json`。

安装使用受管事务的项目完整验证与产物身份核验，没有额外重复 doctor、baseline 全量检查或业务构建。未提交、推送或发布任何下游项目，未恢复业务环境，未维护项目自有 Hook，未修改活库或 vault-mirror。项目 Hook 的真实生命周期、业务启动和业务回归仍未验证，不因骨架安装成功宣称通过。

### StratusAgent 剩余发布缺口

安装后执行了一次零写入 `git-sync --release-preview`，仍被 **41 项缺版本覆盖记录、10 项内容变化**阻断，原始输出在 `.runtime/tests/release-scope/Stratus-release-preview.log`。本轮没有补测、补做 develop、改项目登记或重写账本。

当前项目仍将三个测试工作区锁文件作为全局 build_inputs，其中 `scripts/tests/market_loader/Cargo.lock`、`scripts/tests/pms/Cargo.lock` 相对 HEAD 有实际变化；`stratus/Cargo.toml` 仍登记给 36 个包。全局输入变化独立于本次修复的局部 shared_inputs 配置判定，仍会按合同扩大检查范围。因此，升级安装完成不等于 Stratus 业务发布已解除阻断，也不能把上述 51 项自动当作全量重测清单。

### 本任务终态

TODO-051/TODO-052 的本次工程与安装条件均已满足，记录保持 active / awaiting_user_acceptance，未代替用户验收或自动结算 TODO。预算扩展后完成剩余工作，累计主动执行粗估约 55 分钟（等待用户回复不计，未作逐阶段实测），未启用子 Agent，token 未实测。发布已经同步；本节及索引是安装后新增的本地记录，没有另行提交或再次发布。
