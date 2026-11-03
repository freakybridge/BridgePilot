---
lifecycle: active
validation_status: in_progress
---

# 单入口完整发布修复

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
