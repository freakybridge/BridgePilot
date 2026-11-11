---
name: bridgepilot-cleanup
description: 对 BridgePilot 已登记下游逐个执行各自项目的 cleanup，默认全部，支持指定项目和只读 preview，最后统一汇总。
user_invocable: true
argument: 可选 preview，以及项目名称或 ID 列表
---

# bridgepilot-cleanup

由当前控制对话编排，默认遍历 BP 已登记下游，逐项目执行该项目实际安装的 `.agents/skills/cleanup/SKILL.md`。本入口不维护第二套删除规则或项目清单，不新增 BP cleanup CLI 子命令，不代替项目执行器拼接删除命令。

显式调用本 Skill 即授权所选项目各自 cleanup 约定范围的本轮清理，不逐项目重复询问。`preview` 仅预览，不随后自动删除；咨询、查看名单、修复或安装本 Skill 本身不授权清理。项目清理规则和保护范围仍须逐一核实，不从项目登记推导其他业务、安装、提交或发布授权。

| 调用 | 行为 |
|---|---|
| `$bridgepilot-cleanup` | 全部已登记下游，各自执行 cleanup |
| `$bridgepilot-cleanup preview` | 同一项目范围，各自只读 preview |
| `$bridgepilot-cleanup [preview] <精确项目名称或ID列表>` | 只处理指定子集，未指定项目时才默认全部 |

1. 读取本 Skill 的 `bridgepilot-local.json`，按真实 home 定位 `.build/bridgepilot/bin/bp.exe`，用 `skill-status --home <home> --skills-root <实际根>` 核验本入口 current=true；由 home 的 `.runtime/config/bridgepilot.json` 取得私有 workspace。源码候选无绑定时沿 [BP 入口](../bridgepilot/SKILL.md) 核对，不自动安装或迁移。
2. 用只读 `bp project-list --workspace <实际workspace>` 取得本轮项目快照。正常执行指定子集时，可在安全控制目录 `.runtime/` 写 `{"projects":["<精确名称或ID>"]}` 并传 `--input <绝对路径>`；preview 从只读快照在内存中匹配，不写范围文件或记录。显式空名单、未知、歧义或无法解析的名称停止范围解析，不能退回全部；后续新增登记项目不自动加入本批次。
3. 初始项目 ID 范围固定；每个项目执行前只读刷新 BP 的 project-list/status，重新核对其实际 Git 根、唯一当前机器且 host=local 的绑定、控制权及空闲状态，不能沿用整批开始时的旧状态。暂停、在途／Unknown、外部持有或绑定失效的项目不执行，记录原因并继续其余独立项目；已终态的历史记录不当作在途。不能为清理恢复任务、接管、结束进程或修改目录权限。
4. 在每个合格项目读取其 AGENTS、INSTALL 和本项目 cleanup Skill，按该 Skill 核对当前成品、原生依据及运行依赖，再调用其当前受管执行器。将本轮批量调用视为该项目 cleanup 的显式授权；preview 必须传递为该项目只读预览。Junction、权限跳过、保留清单、路径／占用／漂移保护均沿项目 cleanup 处理，不复制成 BP 私有清理规则。缺 Skill、工具或可靠保护依据时阻断该项目，不自动升级、恢复旧 JSON 或改用 shell 递归删除。
5. 逐项目等待真实结果再进入下一项目。单项目 blocked、partial 或失败只阻断该项目，保留实际结果并继续其他独立项目，不重跑已完成项目。程序安装和项目 Skill 执行分别核验，不把只有入口可发现或模型 completed 当成实际清理完成。
6. 批次结尾统一按项目汇总 completed／partial／blocked／未执行、实际或预览数量、保留范围及失败原因；无权限项只在最后按项目合并完整路径和数量，一次性提示用户手动删除，不在各项目间重复提示。有跳过或失败不能宣称全部清理完成；preview 明确未删除。逻辑删除字节不等于实际磁盘释放量。

非 preview 的必要收据只保存在不属于任一目标待删范围的控制目录，不能在刚清空的目录中重建记录。preview 全程只读，不写目标或控制目录。批次不自动提交、推送、发布、安装、接管，不创建聊天、Run 或后台任务。

$ARGUMENTS
