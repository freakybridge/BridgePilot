---
name: bridgepilot
description: BP个人跨项目统一入口；无参数自动拉取主分支源码、构建安装或更新；也用于初始化存档库、项目登记、换机、状态与已授权任务。
user_invocable: true
argument: 无参数安装或更新；init、project add <项目目录>、handoff prepare all、handoff restore、status，或明确项目任务
---

# BridgePilot

当前源码候选已退役 Forge 项目版本 JSON、开发账本及相应 verify-development／verify-publication 自动认证；后文依赖旧账本的核验步骤仅作历史参考，不执行或恢复旧配置。项目构建、测试、版本及显式发布按原生入口核对；旧认证不可用时保持真实任务状态并报告能力缺口，不以 completed 或普通 Git 同步伪称 Released。项目登记、状态、原生归类、调度与换机边界继续遵循本入口。

用户只需给目标；复用已确认决定、范围和预算。安装与恢复的唯一完整说明是产品根 INSTALL.md。本 Skill 由 BP 管理，源位于 source/orchestrator/skills/bridgepilot/，不属于 Forge 通用 Skill 分发。

## 先定位本机入口

安装到用户级 Skills 后，读取本 Skill 目录的 bridgepilot-local.json，以其实际 home 定位稳定入口 .build/bridgepilot/bin/bp.exe，通过 skill-status --home <home> --skills-root <实际根> 验证 bridgepilot、bridgeforge-update、bridgepilot-cleanup 的所有权与完整文件。各入口均由 BP 的 skill-install 管理，不进入 Forge 共享分发。以 home 的 .runtime/config/bridgepilot.json 定位私有 workspace；不把发行包或他人存档库当个人配置。

源码／安装 home 中的候选入口没有该本机绑定文件：先核对当前项目根及实际安装，不猜机器 ID 或编造路径。源码产物为 .build/bridgepilot/cargo/release/bp.exe。先使用 self-test／help 核验真实版本和能力；Skill 未安装／过旧／中断时沿 INSTALL 处理，不用旧程序冒充新命令。全新电脑的源码安装按根 INSTALL.md 的源码入口引导。

## 无参数：安装或更新

仅明确安装／更新或无附加限制的无参数调用进入本模式；咨询、status 和带参数模式不触发更新。读取已绑定 home 的 INSTALL.md（源码引导则读取源码根 INSTALL.md），执行本 Skill 的 `scripts/bridgepilot_update.ps1`：

```powershell
& powershell -NoProfile -ExecutionPolicy Bypass -File <本技能目录>/scripts/bridgepilot_update.ps1
```

脚本复用本机绑定；默认独立源码为 `~/.bridgepilot-source`，固定官方 `freakybridge/BridgePilot` 的 `main`，仅快进、锁定 Cargo.lock 构建，并调用 BP 的 bundle、install-preview/install、skill-install-preview/skill-install 和最终状态核验。不得用本地开发副本替代上游、强制重置或覆盖未提交改动。缺少 Git、Rust／Windows 构建工具时按 INSTALL 报告和准备，不切换二进制或语言绕过构建。

只有 `BRIDGEPILOT_SOURCE_UPDATE_RECEIPT` 的 `source_update_completed` 表示本次安装和全部入口核验完成；报告版本、固定提交及实际 home。失败保留源码、旧完整包和日志，按失败阶段报告程序／Skill 的实际安装状态；pending 先沿 install-recover 或 skill-recover 核销。首次只安装程序和 Skill，不创建私有库或空任务档案，不上传、接管、执行或恢复任务；已有配置由安装器原位保留。客户端发现新 Skill 仍须实际核验。

## 按请求进入模式

版本查询使用 `bp versions`，主版本来自 BP 根 VERSION，组件分别来自 BF／orchestrator 的原生 CLI manifest；Forge 的 source-reference 不代表本机已安装。安装格式、launcher 协议及旧 0.x 迁移统一从根 INSTALL.md“产品版本与安装格式”进入。

| 用法 | 执行规则 |
|---|---|
| `$bridgepilot`（无参数） | 按上节自动获取主分支源码、构建并安装；已有安装则更新 |
| `$bridgepilot init` | 读取[首次初始化](references/init.md)：准备必要环境、首次确认一次本人存档库、保存绑定；已有配置复用，不自动接管旧机 |
| `$bridgepilot project add <本机目录>` | 读取[项目登记](references/projects.md)：核对并登记，不因此修改下游或启动开发 |
| `$bridgepilot handoff prepare all` | 读取[旧机交出](references/handoff.md#prepare-all)：明确全部暂停并核验停止，保存已确认进度到工作分支，远端确认后释放控制权；不正式发布 |
| `$bridgepilot handoff restore` | 读取[新机接管](references/handoff.md#restore)：恢复并核验准确检查点，自动完成独占接管；任务和 heartbeat 仍暂停 |
| `$bridgepilot status` | 调用 CLI 只读 SQLite 状态视图，按下节原样展示固定六列；不扫描聊天拼表，不安装、迁移、上传、接管或推进 |
| `bridgepilot-cleanup` | 读取实际 home 下私有 [清理入口](../bridgepilot-cleanup/SKILL.md)，核对登记下游或指定子集；旧配置化清理已退役，缺少可靠原生成品保护时阻断删除 |
| 单项目或多项目骨架升级、`bridgeforge-update` | 读取实际 BP home 下 `source/orchestrator/skills/bridgeforge-update/SKILL.md`，由本控制对话编排整轮升级及本轮已有授权的发布；不从项目登记推导维护权限 |
| 自然语言任务、明确范围暂停／恢复、原生唤醒 | 读取[日常执行](references/execution.md)，保留现有 confirm、调度、客户端交接及开发／发布证据要求 |

以上是 Skill 参数，由 Codex 协调底层命令，不把空格模式当作已存在的 bp CLI 子命令。只有匹配的请求才读取对应长引用；复杂任务复用有效 confirm，重要方向与审美判断交用户。

## status：当前在做什么

用户只使用 status。路径及内部 JSON 请求由本机绑定解析，不要求用户输入 CLI 格式参数。任务登记、持久序号、执行事件、阶段归约和展示文本均由 CLI／执行器与 SQLite 完成；Agent 只理解目标／授权，展示结果并回答追问。

1. 核验实际入口具备 `bp-sqlite-status-v1`，调用 CLI status，原样展示返回的 `view.markdown`。表头固定为 **序号／项目／任务／阶段／进展／阻点**；不重排、重新编号、改阶段或补写任务行。序号是稳定任务号，可不连续；历史已交付任务默认不占清单。
2. 来源对话、核对时间与安装／控制机异常可在表外解释；仅能依据 CLI 元数据说明。独立 App Server 的 notLoaded 只证明其未加载当前桌面回合：CLI 可展示原生 Goal 及末次历史，但必须标注观察受限，不把历史终态当作当前任务停止。观察结束不等于业务交付，发布暂缓不等于整个项目已暂停；不得从模型 completed 推定 Released。
3. status 查询不扫描其他聊天、不刷新观察、不补登记，不创建数据库、迁移 schema 或核销未知操作。缺库、旧 JSON 尚未迁移、兼容性或损坏诊断如实报告，不能初始化空库掩盖缺失。档案转换仅按 INSTALL 的显式迁移流程。
4. 查询发现已授权任务未入库时，明确登记缺口，沿[日常执行](references/execution.md)的登记／来源绑定修复，不在本次正式任务表里临时制造记录。SQL 是唯一在线任务事实源；内部 JSON 编码不是第二套任务档案。

档案写入使用当前 revision 的 CAS，结构化输入放项目 .runtime 后传 --input。范围不明的暂停拒绝猜测；Unknown 先核销，不重发未知操作、不抢其他机器。安装、登记与接管不等于业务授权；新聊天、提交和发布分别沿已有明确授权。账号登录与系统强制确认由本人完成，不复制凭据或修改原生数据库／Memory。
