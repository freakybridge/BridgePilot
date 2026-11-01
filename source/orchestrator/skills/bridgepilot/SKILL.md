---
name: bridgepilot
description: BP个人跨项目统一入口；首次初始化存档库、登记项目、旧机保存交接、新机恢复接管、查询状态及推进已授权任务时使用。
user_invocable: true
argument: init、project add <项目目录>、handoff prepare all、handoff restore、status，或明确项目任务
---

# BridgePilot

用户只需给目标；复用已确认决定、范围和预算。安装与恢复的唯一完整说明是产品根 INSTALL.md。本 Skill 由 BP 管理，源位于 source/orchestrator/skills/bridgepilot/，不属于 Forge 通用 Skill 分发。

## 先定位本机入口

安装到用户级 Skills 后，读取本 Skill 目录的 bridgepilot-local.json，以其实际 home 定位稳定入口 .build/bridgepilot/bin/bp.exe，通过 skill-status --home <home> --skills-root <实际根> 验证所有权与完整文件。以 home 的 .runtime/config/bridgepilot.json 定位私有 workspace；不把发行包或他人存档库当个人配置。

源码／安装 home 中的候选入口没有该本机绑定文件：先核对当前项目根及实际安装，不猜机器 ID 或编造路径。源码产物为 .build/bridgepilot/cargo/release/bp.exe。先使用 self-test／help 核验真实版本和能力；Skill 未安装／过旧／中断时沿 INSTALL 处理，不用旧程序冒充新命令。全新电脑尚无 BP 时，从用户指定产品仓库正式 Release 的引导开始。

## 按请求进入模式

| 用法 | 执行规则 |
|---|---|
| `$bridgepilot init` | 读取[首次初始化](references/init.md)：准备必要环境、首次确认一次本人存档库、保存绑定；已有配置复用，不自动接管旧机 |
| `$bridgepilot project add <本机目录>` | 读取[项目登记](references/projects.md)：核对并登记，不因此修改下游或启动开发 |
| `$bridgepilot handoff prepare all` | 读取[旧机交出](references/handoff.md#prepare-all)：明确全部暂停并核验停止，保存已确认进度到工作分支，远端确认后释放控制权；不正式发布 |
| `$bridgepilot handoff restore` | 读取[新机接管](references/handoff.md#restore)：恢复并核验准确检查点，自动完成独占接管；任务和 heartbeat 仍暂停 |
| `$bridgepilot status` | 调用 CLI 只读 SQLite 状态视图，按下节原样展示固定六列；不扫描聊天拼表，不安装、迁移、上传、接管或推进 |
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
