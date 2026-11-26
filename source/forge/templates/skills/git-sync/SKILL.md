---
name: git-sync
description: 普通模式提交推送；release 使用项目当前版本构建、推送和正式发布。
user_invocable: true
argument: 可选 release
---

# git-sync

1. 核对授权范围内已暂存、未暂存和未跟踪的改动，使用项目现有受管 CLI。普通同步执行一次 `git-sync --preflight`，审阅差异后用 `git-sync --message-file <路径>` 提交推送。说明按项目约定写入 `.runtime/`。
2. 用户要求完整发布时，调用一次 `git-sync release --publication-required`，可附提交说明。项目提前准备业务版本前三段与发布说明。CLI 在提交前按已安装驱动收据中的 BP 整体版本写回第四段，构建并确认源码推送后创建标签和 Release。通用下游驱动发布说明，源码由平台按标签提供，额外附件为零。同一组合重复执行使用同一号码。旧收据缺少来源时，先通过受管项目更新补齐；预检保持只读。
3. 根据收据报告。成功时列出版本、提交、标签和 Release 地址，注明发布命令成功、远端内容未复核。操作失败时说明失败阶段和已经完成的操作；远端可能已收到写入时，保留现场并说明结果未确认。
4. Git 作者身份、访问权限或其他异常见[运行边界](references/runtime-context.md)，首次接入见[首次使用](references/first-use.md)，旧参数见[退役边界](references/schema4.md)。发布授权仅限本轮指定项目。
