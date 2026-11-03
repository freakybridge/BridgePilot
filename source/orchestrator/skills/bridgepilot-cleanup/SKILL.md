---
name: bridgepilot-cleanup
description: 核对 BridgePilot 登记下游的清理入口；旧配置化清理已退役，缺少可靠原生成品保护时阻断删除。
user_invocable: true
argument: 可选 preview，以及项目名称或 ID 列表
---

# bridgepilot-cleanup

旧 Forge cleanup 依赖的 artifact-layout、Hook 登记与分发旁文件已经退役。本入口保留安装、恢复和用户编辑保护，不再按旧登记删除运行产物、缓存或构建内容。

读取本 Skill 的 `bridgepilot-local.json`，以真实 home 定位 `.build/bridgepilot/bin/bp.exe`；通过 `skill-status --home <home> --skills-root <实际根>` 核对入口完整性，以 home 的 `.runtime/config/bridgepilot.json` 定位私有 workspace。源码候选无绑定时沿 [BP 入口](../bridgepilot/SKILL.md) 核对，不自动安装或迁移。

用只读 `bp project-list --workspace <实际workspace>` 获取登记项目；指定子集时，在控制目录 `.runtime/` 写入 `{"projects":["<精确名称或ID>"]}`，追加 `--input <绝对路径>`。显式空名单、未知或歧义名称停止范围解析，不退回全部。核对真实项目根、机器绑定、控制权及空闲状态；Unknown、运行中、暂停或外部持有的项目不执行。

逐项目读取 AGENTS、INSTALL 和当前 cleanup Skill，只报告已核实的原生清理流程及保护缺口。仅有旧 Forge cleanup 时报告 blocked，不调用该入口，不改用 shell 递归删除，不恢复旧 JSON，也不根据时间或目录名猜测当前成品。

`preview` 全程只读，不创建范围文件或记录；指定子集从只读项目快照在内存中精确匹配，不能随后自动执行删除。非 preview 也不从本次调用推导任意原生删除授权；存在可靠原生流程时，先说明精确目标、影响及可恢复性，按用户已有明确范围处理，缺失时仅暂停删除。

汇总项目、已核实入口和 blocked／未执行原因；不报告磁盘已释放，不自动提交、发布、接管或启动后台任务。安装入口可发现不等于真实清理已验证。
