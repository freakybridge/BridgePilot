---
lifecycle: completed
validation_status: verified
---

# 单入口完整发布修复

## 用户验收与正式发布2026-11-03

用户在本轮修复与真实发布交付后显式调用 `summary accept`，确认 TODO-054 单入口完整发布修复符合需求，包括自动版本准备、提交推送前的版本占用检查、失败接续、固定提交及旧发布保护。TODO-054 按本次工程和真实发布证据记为已完成；历史过程与失败记录保留。

真实发布由同一次受管 `git-sync release --publication-required` 完成，CLI 自动将 BP 1.1.1 升至 1.2.0，未手动改写根 VERSION。最终收据为 `status=released`、`publication_verified=true`，固定提交 `7d9825c2bf2166c452d050d7f379e2bb19889f33`，标签 `bp-v1.2.0`，[正式 Release](https://github.com/freakybridge/BridgePilot/releases/tag/bp-v1.2.0) 的 14 项资产全部核验通过。构建调用两次，fetch、push、upload 各一次；旧失败尝试已保留，原 `bp-v1.1.1` 未覆盖。组件版本变化见下节，均已纳入该正式发布。

验收依据为 29 项定向验证、独立复核、三个受管产物及清单／原生版本检查，加上述真实 BP 发布收据。本轮没有运行真实下游发布、下游 runtime smoke 或用户环境安装，用户级共享 Skill 本轮未另行分发；这些不记为已验证。随后用户再次授权当前 BP release，用于同步本次验收文档增量，其结果以新一轮实际收据为准，不把该授权预记为发布完成。

## 2026-11-03 自动升版与旧版本误用修复

本轮 `bridgepilot-git-sync release` 技能扩展验收后，当前 BP 的完整发布将源码 `8317d5abfeecf8bd7e50f6879820136a47c956c6` 提交推送，却仍使用已正式发布的 1.1.1；CLI 在 publication 阶段发现 `bp-v1.1.1` 指向旧提交而正确拒绝覆盖。根因是原生适配器只消费 VERSION，没有源码版本准备阶段，版本占用检查又在推送之后。用户要求生成修复计划并明确“开始修复”，本轮继承当前 BP 的完整发布授权，不授权下游发布或用户环境安装。

修复沿同一入口完成：通用 core 定义源码身份、上下文和明确文件编辑协议；workflow 在原生产物准备前调用 BP 的只读源码适配器，根据远端正式发布基线、累计约定式提交及待提交消息选版本，事务应用 VERSION／CHANGELOG 后重新构建，随后严格绑定源码与产物的产品、仓库、版本和 tag。未知分类、版本倒退、不足的预升版及发布说明归属冲突明确阻断，不靠恢复旧配置账本处理。

源码事务允许部分写入继续完成；每个文件写前校验 before/after，写后复核全部 after 和 Git 检查点。已完整应用但尚未提交时，修正其他源码可保存旧事务并重新准备，合理的候选版本不重复递增。prepared 状态落盘后、事务收尾前退出也能在恢复分支完成收尾。保护基于受管锁、检查点和写前后校验，不声称对任意外部写者提供无条件跨进程 CAS。

对本次已 pushed 的错误旧版本尝试，只有推送目标不变、原提交仍为当前祖先、远端标签指向更早祖先且是正式受管 Release 时，才归档原尝试并建立新候选。标签指向本次提交、外来草稿、查询未知或目标漂移均不能触发该恢复；不删除旧状态、标签、资产或提交。版本占用现在于提交推送前检查，正式发布前仍复核并发变化。

原生组件候选：core 11.1.0 → 11.2.0、workflow 0.12.1 → 0.13.0、factory 0.12.1 → 0.12.2、Forge CLI 17.1.1 → 17.2.0、install 0.6.0 → 0.7.0、orchestrator 0.15.0 → 0.16.0。Hook 17.1.0 及其余包保持版本。BP 根 VERSION 在实际 release 前保持 1.1.1，用本次真实发布验证自动版本决策；完整累计版本说明见 CHANGELOG。

工程证据：BP 适配器 6 项定向测试通过；workflow 19 项通过后，修正模拟 Release URL 并补跑受影响的 4 个跨版本场景，4 项全部通过，合计 29 项定向验证。测试二进制的编译身份来自实际受管 CLI `self-test --json`，最初缺少该环境导致夹具初始化被拒绝，不计为通过。模拟器此前硬编码 bp-v1.0.0 URL，跨版本测试被真实 URL 校验正确拒绝，修正只限模拟响应。三个受管产物 prepared、自检与 153 文件事务同步成功；manifest 零漂移、factory-version 健康和 diff 检查通过。验证使用锁定 Cargo、`scripts/tests/factory-cargo.toml`、`.build/bridgeforge/test-target`，分别选择 `bridgepilot-install --lib release` 与 `bridgeforge-workflow --lib native_release`；未运行无关全量回归或真实下游 smoke。

独立 review-auditor 已复核源码事务、版本与产物绑定、旧 pushed 恢复正反场景和新增测试；发现的问题已修正，最终静态复核无剩余必修问题。测试与发布结果继续以实际工具输出和 `.runtime/bridgeforge-codex/native-release/current.json` 为准；该记录不预先宣称真实发布完成或新一轮用户验收。

## 用户决定与授权

用户明确要求只调用一次 `git-sync release`，其余必要工作全部由 CLI 完成；从接受指令到真实发布采用最短工作路径，新增工作环节须先确认。用户已确认以下四类暂停条件：关键决定缺失；不可自动处理的冲突；构建、必要校验、鉴权、权限或外部服务条件失败且无法安全自动解决；需要新增工作环节、扩大范围或执行未授权操作。可安全恢复的短暂故障有界处理，结果不确定先核对远端，不能盲目重发。

2026-11-03 用户确认整体预算后要求开始实施：120 分钟、40,000 token 估算（含主／子 Agent）、最多 1 名独立复核 Agent、最多 2 轮验证，包含一次真实发布验收。token 未实测；缺少本机周额度换算依据，不编造百分比。关联 TODO-054。实施包括本仓库修复、必要版本与文档、受管提交推送、真实发布及对应用户级 Skill 的受管传播；不更新真实下游业务项目或安装 BP 用户环境。

## 唯一路径与架构边界

`git-sync release` → 一次聚合预检 → 受管接入远端 → 必要原生产物准备 → 受管提交推送固定源码 → 正式发布 → 一次最终远端核验。有效构建和产物复用，不增加独立发布预览、人工草稿审核、重复审批或固定全量回归。失败后再次调用同一入口接续，不能重复升版、覆盖既有版本或把源码同步记成完整发布。

Forge 通用源码维护参数解析、原生适配器调用、GitHub 资产发布及可验证结果；BP 在 orchestrator 提供既有 bundle 的项目适配，Forge 不反向依赖 BP 包。原生 Cargo metadata 仅登记执行入口；版本仍以原生文件为准，不创建项目版本／收口 JSON。运行中的接续凭据存于 `.runtime/`，仅证明实际尝试与输入身份。

## 实施与验收

- P0：严格解析 `release` 和选项；不再静默忽略位置参数，普通同步语义保持。
- P0：CLI 必须执行完整发布且返回真实终态，Git synced 不能作为 release 成功。
- P0：BP 打包固定十三项 payload 与 package.json；绑定版本、源提交、标签、资产字节及远端 digest。
- P0：冲突版本不覆盖；创建、上传、发布结果未知时先查询；重复调用只补缺失步骤。
- P1：修正共享 Skill、dogfood、用户级入口及错误的历史授权描述，保留历史实际执行事实。
- P1：定向验证参数、成功、缺入口、版本/标签冲突、上传中断与接续、重复发布、远端不一致；独立复核后用同一入口完成真实发布。

正常路径命令数、构建复用和远端操作进入实际执行收据，用于核对最短路径，不设新增审批。发布成功要求远端正式 Release、标签提交与预期一致、全部资产 digest/size 核验通过。必要验证失败或输入漂移时不伪造完成。

## 已知问题与纠正

之前用户的 `git-sync release` 被 Agent 擅自缩减为源码同步，随后验收记录把该缩减写成用户决定；该记录须明确更正。旧用户级 Skill 仍引用已退役发布协议，新 Skill 又未指向完整实现，CLI 还会忽略裸 release。本次修复整体链路，不将其中任一项替代其他项。

## 执行记录

2026-11-03 开始实施。初始 Git 根为 `D:/Quant/BridgePilot`，HEAD 为 `4674c6834c95dedbd07521dd02a8b135bfcb86b0`，工作区干净。本轮产品层包含通用 Forge 能力及 BP 适配，公共 Skill 同步对应镜像；需要原生版本和 CHANGELOG，不修改其他项目。实际测试、发布与剩余边界在本节追加，用户验收前不标 completed。

2026-11-03 用户另行确认允许统一受管更新用户级工具包，范围包括 git-sync、cleanup、bridgeforge-codex、archive-scan、develop、find-doc、sync-docs、todo 八个已存在共享 Skill；用户 AGENTS 比较结果相同，不更新真实下游。只读发现旧 home 的 BridgeForgeCodex URL 与当前 BridgePilot URL 在 GitHub 均对应 repository ID 1225826781；安装器补充严格的已验证重命名识别，正常 canonical 路径不新增网络，未知或被重建的旧仓库仍阻断，通过既有备份／替换事务更新，不手改旧 home 的 origin。

已完成的工程验证：发布流程 10 项（含完整发布、重复零重建／上传、草稿恢复、标签及创建／发布超时核销、一次安全补传、持续失败接续、资产与源码冲突、错误参数及缺适配器拒绝）；BP 安装／版本／路径保护 16 项；orchestrator 锁定 Cargo 编译通过。独立复核发现并修正草稿 REST 查询、首文件前导空白、部分上传恢复、输出路径越界、foreign draft 写前归属顺序与调用计数，最终无发布核心必修问题。Git 本地夹具受限运行因 MSYS 命名对象权限失败，获准环境下同一夹具通过，不把权限失败计为通过。

本机证据位于 `.runtime/tests/native-release/`：`workflow-tests-final.log`、`bp-install-tests.log`、`bp-check.log`、`home-rename-tests.log`。旧 CLI 在 `legacy-guard.log` 中对 `--publication-required` 明确拒绝，未发生 Git 写入。源码准备与真实发布状态继续以 `factory-prepare.log` 以及最终 CLI 的 `released`／`publication_verified` 收据为准；本记录在发布前提交，不预先宣称远端发布或用户级安装成功。

用户级 home 重命名补充验证共 7 项通过，含同提交旧 URL 的源码选择与实际 stage 复制；独立复核确认只接受已验证官方别名、保留既有归属与替换事务，未发现剩余必修问题。原 native-release 核心 10 项与 BP 安装 16 项结果保持有效；新增安装器验证不替代真实用户级更新收据。

最终受管准备成功：`factory-prepare-final.log` 记录 3 个运行产物 prepared、自检成功、152 文件事务同步、retired_files=0。此前准备因安装器兼容修复改变综合输入而被防漂移检查拦截，原已安装工具保持；失败日志保留在 `factory-prepare.log`，不计为成功。发布前 manifest 检查、原生版本与必要测试有效，实际完整发布由新的单次 `git-sync release --publication-required` 执行并生成最终收据。GitHub 发布目标为 `bp-v1.1.0`，包包含 BP1.1.0、BF17.1.0 来源引用和 orchestrator0.15.0；完整累计版本见 CHANGELOG。

## 2026-11-03 提交前失败恢复修复

旧测试退役已由用户验收并授权完整发布。首次受管调用因活动文档仍引用迁移前的根清单而在提交前失败；修正该链接后再次调用，旧协调器将所有未完成且输入变化的尝试统一判为冲突，无法自动重建。两次调用没有完成本轮提交推送，旧 `prepared` 状态、14 项产物与受管 commit guard 保留；原始失败收据见 `.runtime/tests/test-retirement/release.log` 和 `release-resume.log`。

用户在本对话核对原因后要求生成修复计划，并明确要求“开始修复”。本次范围为 Forge 通用 release 的安全接续、必要回归、原生版本和 dogfood 传播，并继承此前完整发布授权继续本次发布；不删除剩余测试目录，不安装到用户环境，不更新真实下游。原测试退役和已验收记录保留，不以本次修复重写此前事实。

实现沿用同一 `git-sync release` 入口，关键边界如下：

- 新尝试记录源同步前的 HEAD、分支、实际推送跟踪 ref、remote／URL 及 Git 配置摘要。同步前先持久化阶段，识别提交前失败、已有源提交、已推送与已发布，不把 `commit=null` 等同于未提交。
- 输入变化时，仅确认尚未越过提交点的尝试可自动重建。旧 schema1 缺少检查点时，必须由有效受管 commit guard 的 parent 等于实际 HEAD 证明提交前状态；守卫 schema、OID 和仓库对象格式均严格校验。缺证据、坏守卫或已有提交时保留冲突。
- 自动重建先将原始状态和守卫原字节写入内容寻址的尝试目录，再准备新的唯一产物目录；不删除旧产物，不覆盖已有历史。保存使用受管 operation.lock 下的写前比较、原子替换及写后确认，不声称对不遵守该锁的外部写者提供无条件跨进程 CAS。
- 提交成功但推送失败时固定已确认提交，只接续同步／推送。恢复期间即使远端追加同树提交，也不能替换固定 commit。只有同步回执证明 synced、clean、ahead/behind 为零且 commit 匹配后，才进入 publication。
- 恢复前检查 merge/rebase/cherry-pick/revert、sequencer、未合并索引和 Git 锁；分支、推送 refspec／配置、URL 或固定提交漂移时停止。已上传或已发布尝试继续执行原有严格保护。

原生版本按兼容修复收口：BP 1.1.0 → 1.1.1，Forge CLI 17.1.0 → 17.1.1，workflow 0.12.0 → 0.12.1，factory 0.12.0 → 0.12.1；Hook 17.1.0 和 orchestrator 0.15.0 保持。新目标为 `bp-v1.1.1`，不覆盖已固定在旧 HEAD 的 `bp-v1.1.0`。

恢复验证独立使用 `.runtime/tests/release-recovery/`：第一轮原 10 项加新增 7 项共 17 项通过；独立审查随后补强固定提交防同树 fast-forward 替换、未知／损坏守卫拒绝和锁检查，最终 18 项运行结果以 `tests-final.log` 为准。为允许候选工具准备与测试并行，测试可通过 `BRIDGEFORGE_TEST_RUNTIME` 使用冻结的真实公共工具与原始收据；编译身份来自该工具真实 self-test，fixture 仍执行完整运行时验证。此目录不是新版本登记或旧收口账本。

独立 review-auditor 已复核恢复边界与实现，确认已固定提交保护和严格守卫校验；实际最终测试、受管准备及发布以本机收据为准。本节记录工程进展，不预先宣称真实发布或用户对本次修复的验收。

最终恢复测试 `tests-final.log` 为 18 项通过，含同树 fast-forward 不替换固定提交、未知／损坏守卫拒绝，以及旧状态接管和提交／推送分阶段恢复。正常运行和恢复均继续经过现有生产校验，不以删除失败用例收口。

工具收口另定位到一处必要修复：清单事务尾重算只发现 `scripts/tests/Cargo.lock` 漂移。隔离诊断 `projection-diagnosis.log` 证明 `toml_edit::Table` 克隆保留原文档位置，合并根锁与消费锁后再回读会改变序列化顺序。`consumer_lock` 仅在合并后按稳定数组顺序统一表位置，保留版本替换、依赖闭包及事务尾校验。临时诊断已移除；`lock-regression-final.log` 的重复二／三次字节一致、原生版本优先和闭包排除单项回归通过。共 19 项本次定向检查通过，独立复核无剩余问题。

该修复后，当前源码工厂工具通过 `manifest-fixed.log` 成功刷新真实根的清单与镜像。随后受管产物准备因 Windows 错误 1455 无法映射标准库而失败；单任务重试也无法加载 std，当时系统可用虚拟内存约 257 MiB，且无本任务残留构建进程。用户关闭程序后可用虚拟内存恢复至约 6.3 GiB，保持 `CARGO_BUILD_JOBS=1` 从同一受管入口继续，日志为 `prepare-after-memory.log`。未修改系统设置、结束用户程序、手改清单哈希或删除旧尝试。

内存恢复后的受管准备成功：`prepare-after-memory.log` 返回三个产物 prepared、自检成功、152 文件事务同步、retired_files=0。补丁工具下 `git-guards.log` 两项普通同步提交保护回归通过，与 18 项恢复和 1 项锁文件幂等回归合计 21 项本次定向检查；不重复累计原测试退役的 121 项。`manifest-check.log`、`version-check.log`、`runtime-check.log` 分别证明清单无漂移、Forge 17.1.1 原生版本健康、三个运行产物可只读复用且无需重建。工程验证完成，后续由同一完整发布入口生成实际发布收据，不预先宣称远端发布成功。
