---
lifecycle: completed
validation_status: verified
---

# BP 六包职责拆分

## 目标与授权

2026-11-02 用户授权按已确认方案实施 tasks、storage、execution、transfer、install、cli 六包拆分，完成必要验证和受管子包收口；不提交、推送、发布，不修改真实档案、用户级安装或下游。无新增整体预算约定。

BP 私有产品实现，不传播公共 Forge 模板或源码投影；包登记为自身配置，说明为元文档。开发仅升级适用子包，根 VERSION 与正式 CHANGELOG 保留。

## 包边界

- tasks：任务、授权、状态、控制权、唤醒与事件类型及内存校验。
- storage：SQLite、事务、历史、迁移、事件与同步本机记录存储。
- execution：Codex 通信、进程、调度、观察、状态视图及开发发布核验。
- transfer：档案同步与换机交接；依赖 execution 的必要只读查询。
- install：便携安装、恢复、回退、私有 Skill 与依赖准备。
- cli：参数、服务调用与输出；保留 bridgepilot-orchestrator 原生名称和 orchestrator 稳定身份。

依赖：storage → tasks；execution → storage/tasks；transfer → execution/storage/tasks；install → storage/tasks；cli 为组合入口。Forge 不依赖 BP，BP 对 Forge 仅依赖 core。无嵌套 workspace 或 src 中间层。Skill 源路径和安装目标保留。

## 阶段及完成条件

| 阶段 | 目标 | 验证 |
|---|---|---|
| S01 P0 | 工具与包生命周期核实 | 修改前 develop --preview --packages orchestrator 返回 task-development-current；必要七项能力具备 |
| S02 P0 | 提取共享模型与规则 | 状态、序列化、授权规则等价 |
| S03 P0 | storage 及 SQL 收拢 | 历史、事件、CAS、迁移、快照与 pending 事务 |
| S04 P1 | execution | RPC、超时、暂停、Unknown、状态、证据门禁 |
| S05 P1 | transfer/install/cli | 交接、未知结果、升级回退和中断恢复 |
| S06 P1 | 构建与登记 | 锁定构建、包图、BP 相关完整集及 Forge 公共投影排除 |
| S07 P1 | 独立验证与收口 | 独立审查、最终候选检查、development-closed、重复收口 |

## 风险和停止点

保持 State 序列化、SQLite schema、摘要、事务、命令、资产 ID 与安装目标。类型和 SQL 拆分不能削弱授权、所有权、暂停、Unknown 或发布认证。工具缺生命周期能力时停止相关版本写入，不手改旧账本。无关 Forge 全仓回归不纳入。

## 实施记录

- 修改前工作树干净；主程序 15.0.0，orchestrator 0.8.0。受管预览无待收口，core 为 context-only。
- 初期状态：源码提取中，尚未验证或收口；最终状态见文末。

- 六包源码、清单、主要存储接口和命令服务已提取；Cargo check 已通过一次全部 BP 包与测试目标。原有第三方依赖版本未变化。
- 独立静态复核未发现确定性回归；程序版本传递新增专项 fixture 断言，待实际测试。
- 受管配置预览被 Forge migration 原生 DTO 未过滤 release_withdrawals 阻断，尚未写入新包登记；最小修复需用户另行确认，原有历史保留。BP 包登记为私有版本管理，不进入公共八包分发。

## 阶段验证与原阻断（已解除）

- 六包锁定编译检查通过；独立静态复核未发现确定性功能回归。
- BP 相关工程用例共 105 项通过：execution 13、install 4、CLI 1、集成 56、storage 19、tasks 12。集成测试首次因沙箱拒绝 MSYS 系统对象及进程限制失败，较宽权限下 56 项全部通过；存储根定位用例遗漏已修正并重验。原始日志位于 .runtime/tests/bp-package-split/bp-engineering-tests-unsandboxed.log 和 storage-tasks-tests.log。
- 根定位遗漏仅影响本轮测试：误生成的 source/doc、source/.runtime、source/.gitignore 已核实为测试脚手架并移入 .runtime/tests/bp-package-split/misplaced-fixture 保留，source 根恢复为 forge 与 orchestrator。
- SQLite 仅在 BP storage 与 CLI 测试中声明，公共 workspace 不新增 BP 专用第三方依赖。实际受管 manifest --root . --check 返回 status=ok、changed=false，说明当前公共生成资产及 dogfood 投影无需变化，BP 六包未进入公共分发。
- 受管配置提案保存在 .runtime/tests/bp-package-split/config-proposal.json；migration-plan 被 release_withdrawals 未过滤阻断。Forge 源码、受管版本配置、旧账本及既有版本未改；新五包仅有初始清单版本 0.1.0。尚未执行正式构建和子包收口，不能报告开发完成。
- 用户就“为何登记、是否下发、是否为 Forge Bug”提出澄清；已说明登记仅用于本仓库私有版本管理，修复为配置迁移原生 DTO 字段过滤。不将询问理解为新增 Forge 修改授权。
- 继续条件：用户明确允许最小 Forge 配置入口修复后，补回归测试、同步 dogfood、准备匹配工具，受管登记六包并对最终候选验证和收口；不提交、推送或发布。

## 2026-11-02 确认后接续

- 用户明确确认允许最小 Forge 配置过滤修复、回归验证和自身投影同步，并继续完成原六包受管收口。仍不提交、推送或发布。
- migration.rs 仅在 adapter_value 副本的字段过滤清单加入 release_withdrawals；原候选配置保留历史。新增 configuration_update_preserves_verified_withdrawal_history 实际通过（1项，121.39秒），覆盖真实隔离发布与撤回历史下的配置 plan/apply、基线与账本保留和后续开发预览。
- 独立复核确认该最小修复与正向回归有效。另发现相邻既有缺口：配置提案本身删除或改写撤回历史时，入口尚未对候选执行完整 withdrawal audit，可能到后续 develop 才拒绝；本轮不扩大修改，实际提案已逐项核对撤回历史与当前配置完全一致。此项不作为已修复问题。

## 最终交付结果

六包源码、职责接口、独立身份与开发检查登记、显式 split 生命周期、测试入口、文档导航及受管开发收口均已完成，并经用户验收。CLI 仅负责命令组合，tasks 保存共享类型及授权规则，storage 维护 SQLite 与事务，execution 负责执行与证据核验，transfer 负责同步换机，install 负责安装恢复。私有 Skill 源与安装目标保留。

| 包 | 原版本 | 最终版本 | 原因 |
|---|---|---|---|
| bridgepilot-orchestrator（cli） | 0.8.0 | 0.9.0 | 延续 orchestrator 身份；原大包 Rust 导入拆至各职责包 |
| bridgepilot-tasks | 首次独立 | 0.1.0 | 任务、授权、状态及共享类型 |
| bridgepilot-storage | 首次独立 | 0.1.0 | SQLite、历史、迁移、事件及同步事务 |
| bridgepilot-execution | 首次独立 | 0.1.0 | Codex 执行、调度、观察与交付核验 |
| bridgepilot-transfer | 首次独立 | 0.1.0 | 档案同步及换机交接 |
| bridgepilot-install | 首次独立 | 0.1.0 | 便携安装、依赖准备、Skill、恢复与回退 |
| bridgeforge-lifecycle | 0.8.4 | 0.8.5 | 配置迁移原生 DTO 过滤 release_withdrawals，完整配置保留历史 |

core 10.2.0、checks 0.2.2、factory 0.10.1、skeleton 0.6.0、workflow 0.9.2 仅因共享 Cargo workspace 构建身份变化刷新覆盖，版本保持不变。根 VERSION 保持 15.0.0，正式 CHANGELOG 未改。

### 最终候选验证与收口证据

- 主入口：`.build/bridgeforge/bin/bridgeforge-factory.exe develop --apply --root . --packages orchestrator,bp-tasks,bp-storage,bp-execution,bp-transfer,bp-install,lifecycle --analysis-file .runtime/tests/bp-package-split/development-analysis.json`。同环境实际预览核对后执行；原权限环境预览与 apply 指纹不同导致的一次提前退出未写版本。
- 实际终态 `development-closed`，`validation_mode=task-selected-final-candidate`，`executed=13`、`reused=0`，`git_written=false`。收据：`.runtime/tests/bp-package-split/develop-apply.json`；正式追加记录：`.codex/package-closures.json`。
- 最终候选检查：tasks、storage、execution、install 单元；transfer 的 portable 集成；CLI 单元与其余集成及 Release 双入口构建；Forge 配置迁移回归；core 公共投影隔离、checks Cargo 路径合同、factory 私有 Skill 镜像边界、skeleton Hook 注册保留、workflow 用户资产分发边界各一项。13 项检查全部 passed，不执行 Forge 全仓回归。
- 新增程序版本身份回归实际核对 RPC initialize.clientInfo.version 与 CLI self-test.version，防止 execution 子包版本冒充程序版本。工程阶段的 105 项 BP 用例与新 Forge 配置回归结果保留为前置证据，最终交付以受管最终候选检查为准。
- 独立复核覆盖源码迁移等价性、测试路径修复、Forge 最小补丁、最终六包身份和 split、账本前缀、历史与版本、13 项 passed 记录及公共投影边界；未发现本轮新增确定性回归。相邻既有候选撤回历史校验缺口保留上节说明，不作为本轮已修复事项。
- 最终 Forge 工具受管准备成功：`.runtime/tests/bp-package-split/forge-final-prepare.log` 返回 prepared=3、written_files=137，产物及收据对应最终 lifecycle 0.8.5。
- 最终 BP Release 成品：`.build/bridgepilot/cargo/release/bp.exe`、`bp-launcher.exe`；实际自检分别报告 bridgepilot 0.9.0 与 launcher_protocol=1，原始回执位于 `.runtime/tests/bp-package-split/bp-final-self-test.json` 和 `launcher-final-self-test.json`。
- 最后核验：manifest --check 返回 status=ok、changed=false；build-assets --factory --project-root . --check 返回 status=current、read_only=true、reused=3、built=0。同范围 develop --preview 返回 task-development-current、affected_packages=[]，不重复升版。原始收据为 final-manifest-check.json、final-runtime-check.json、develop-final-preview.json。
- Git 与工作区边界：HEAD 仍为 972443d6c58f684e4ed6743a23375974e689169b，分支 main；暂存区无变化，VERSION/CHANGELOG 无差异，git diff --check 通过，develop transaction journal 已移除。source 根仅含 forge 和 orchestrator。

### 传播及未验证边界

BP 六包仅登记为本仓库私有版本对象，不进入 public-members 或公共源码／锁文件投影，SQLite 私有依赖也未进入公共 workspace。Forge 修复及 lifecycle 版本已同步自身公共投影；未更新用户级共享安装或真实下游。

本轮只交付源码构建的 BP 成品，既有便携包与用户级 BP 安装未刷新；未改真实个人档案，未做物理跨机或真实桌面业务验收。不提交、不推送、不发布，用户验收与工程完成分别判断。

## 用户验收与发布授权（2026-11-02）

用户显式调用 `$summary accept`，确认本轮六包拆分与最小 Forge 配置入口修复符合需求；同时显式调用 `$git-sync release`，授权提交、推送和正式发布。原先“不提交、不推送、不发布”是开发阶段边界，至本次明确发布授权结束。发布是否成功以受管 release 的实际收据及固定提交／远端核对为准，不从本验收推断。

验收范围包括 tasks／storage／execution／transfer／install／cli 职责拆分、稳定身份与版本历史、事务及运行合同保留、程序版本传递、测试和文档迁移、六包私有分发边界、Forge release_withdrawals 原生 DTO 过滤、dogfood 与最终工具、13 项最终候选检查及受管收口。沿用现有独立复核和实际收据，不补做开发或回归。

关联 TODO-050 仅更新本次六包增量验收，整体产品演进仍为进行中；真实个人档案、用户级安装、真实下游、物理跨机和真实桌面业务等本次未覆盖边界保持。相邻候选提案删改撤回历史的既有静态疑点单独登记 TODO-053，待确认及负向夹具验证，未纳入本次修复，不作为已修复事实。

本轮经验：Cargo 清单迁移需同时核对 include_bytes、#[path]、CARGO_MANIFEST_DIR 及固定祖先层数；检查输入按当前入口使用显式文件清单；共享 workspace 构建指纹变化可由受管流程刷新未变包覆盖而不升版；受管预览与 apply 使用一致实际环境。上述触发、失败和有效处理证据见本记录各阶段。

## 本次 release 预览原阻断（2026-11-02，已补齐）

用户验收和发布授权已记录；受管 preflight 核实工具能力、main／HEAD 和实际推送目标 origin/main，随后 sync-remote 成功返回 remote-ready，HEAD 仍为 972443d6c58f684e4ed6743a23375974e689169b。

远端接入后的零写入 release-preview 拒绝继续，唯一列出的缺口为 memory：`package dependency/build coverage mismatch [coverage-mismatch]; missing components: []; changed components: [build]`。其源码、版本和依赖分项未变；共享 Cargo workspace 清单变化使构建覆盖指纹与旧记录不匹配。此前六包任务收口的 effective scope 不包含 memory，且 outside_task_evidence_checked=false，不能将任务范围完成推导为全仓 release 就绪。

release 入口明确报告 stopped without running checks。本次未执行 release 写入／提交／推送，主程序仍为 15.0.0；已完成的用户验收与六包开发收口有效。遵循 git-sync 的 release 不补做 develop 合同，停止相关发布，不自动重验或批量升级其他包。继续发布需另行明确授权仅对 memory 做必要的受管验证／覆盖刷新，再重新生成实际 release 预览和产品分析；预计内容未变时不升 memory 版本，最终以工具计划为准。

## memory 覆盖补齐及继续发布（2026-11-02）

用户明确授权仅补齐 memory 必要验证和受管覆盖，源码未变不升版，然后继续 release。实际 memory 预览 affected 仅 memory，core 为关联上下文。选择已登记 memory-unit 检查的 persistence_contract_tests 定向组，5 项通过；受管 apply 返回 development-closed、executed=1、reused=0，memory 0.1.1 → 0.1.1，仅追加覆盖记录，源码未改、其他包未处理。原始收据 memory-preview.json、memory-analysis.json、memory-apply.json 位于 .runtime/tests/bp-package-split/。

继续复用已核验工具和六包开发结果，远端接入后重新生成实际发布预览；本轮 release 不补做其他开发验证，产品升版、提交及推送由受管发布事务执行，实际结果以最终收据为准。
