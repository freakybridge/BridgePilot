---
name: cleanup
description: 说明旧配置化产物清理入口的退役状态；未建立可靠当前成品保护前不执行删除。
user_invocable: true
argument: 可选 preview
---

# cleanup

旧自动清理依赖已退役的 artifact-layout、项目 Hook 登记和分发旁文件。当前工具明确阻断此入口，防止删除仍在使用的业务成品、DLL、资源或恢复副本。

读取当前项目 AGENTS / INSTALL；若项目有独立、已核实的原生产物清理流程，按该流程及用户明确范围处理。没有可靠成品保护规则时说明缺口，不运行旧 cleanup，不改用 shell 全量递归删除，不恢复旧 JSON 续用机制。只读请求不写入。
