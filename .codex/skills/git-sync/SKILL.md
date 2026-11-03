---
name: git-sync
description: 审阅实际改动，通过现有受管入口执行普通提交、推送与核验；显式发布按项目原生流程处理。
user_invocable: true
argument: 可选 release，必须使用项目原生发布流程
---

# git-sync

显式调用只授权本轮指定范围。普通同步不升版；发布另须明确授权，按项目 AGENTS / INSTALL 的真实原生入口执行。Forge 配置化 release、版本登记和收口账本已退役，不创建替代旁文件。

1. 从项目 AGENTS 确定现有受管入口，运行 `git-sync --preflight` 获取分支、HEAD、实际推送目标和差异摘要；这是零写检查，remote_state=not_fetched 不证明远端一致。工具缺失时报告，不自动安装或重建。
2. 审阅授权范围实际 diff。消息采用 `<类型>: <简体中文实际效果>`；多行正文写项目 `.runtime/` 文件，经 `--message-file` 传入，不通过 shell 字符串中转。
3. 普通同步运行 `git-sync --message-file <路径>`，由同次调用完成远端接入、改动保护、轻量基线检查、提交和推送。保留 stash、冲突与失败现场，不使用 --no-verify。
4. 发布时使用项目已核实的原生构建、版本和发布入口；不向 Forge 传 --release、--setup-*、--migration-* 或旧 develop 参数，不伪造账本或把普通同步称为发布。
5. 一次核对收据、HEAD、实际 push 目标、ahead/behind、工作区和 stash。报告真实 commit、推送或 no-op、版本变化与未完成事项；本地提交不等于发布成功。

不从当前调用推导其他项目授权，不反复 self-test 或前置准备工具。输入或来源有实际矛盾时才补查；平台权限遵守当前限制，已有授权不重复询问。

首次连接有缺口时读[首次使用](references/first-use.md)，权限或运行异常时读[运行边界](references/runtime-context.md)；涉及旧发布参数时读[旧 schema4 退役边界](references/schema4.md)，不恢复该机制。
