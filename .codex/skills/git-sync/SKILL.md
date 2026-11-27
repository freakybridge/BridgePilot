---
name: git-sync
description: 普通模式提交推送；release 使用项目当前版本构建、推送和正式发布。
user_invocable: true
argument: 可选 release
---

# git-sync

用户显式调用 `$git-sync` 即授权对本轮指定仓库执行普通同步，包括获取远端更新、快进本地分支，以及按需提交授权范围内的改动并推送到配置的目标分支。显式调用 `$git-sync release` 即授权按该项目的正式发布流程构建、提交、推送、创建标签和 Release。沿用用户指定的文件、分支及只读等限制；仅询问用法或提及命令时不执行。

需要平台执行权限时，直接按[运行边界](references/runtime-context.md)为受管命令申请，说明用户的原始调用及其授权范围，无需先在对话中重复确认。

1. 核对授权范围内已暂存、未暂存和未跟踪的改动，使用项目现有受管 CLI。普通同步执行一次 `git-sync --preflight`，审阅差异后用 `git-sync --message-file <路径>` 提交推送。说明按项目约定写入 `.runtime/`。
2. 发布前用 `git-sync release --preflight` 获取实际推送目标、改动路径、待推送提交和 `review_id`，结合本轮任务审阅差异。目标唯一、范围清楚就直接执行。仅当有其他任务混入且范围未明确时，提交前列清差异，一次询问一起发还是只发选定部分。用户已说“全部一起发”就沿用该范围；文件数量不决定归属。
   调用一次 `git-sync release --publication-required --review-id <review_id>`。只发部分改动时附 `--paths-file <路径>`，用 UTF-8 JSON 数组列出选定的仓库相对文件路径，放在忽略的 `.runtime/` 中。文件选择针对工作区的整个文件；同一文件混有两项修改，或待推送提交含有排除的内容时，先厘清可独立发布的范围。CLI 核对内容、构建并确认源码推送后创建标签和 Release。源码由平台按标签提供，BP 及通用下游驱动的自定义附件为零。
3. 根据收据列出版本、提交、标签和 Release 地址。`publication_verified=false` 表示发布命令成功、远端内容未复核；为 true 时，说明已核对远端结果。失败时说明阶段、已完成的操作及仍未确认的远端状态。
4. Git 作者身份、访问权限或其他异常见[运行边界](references/runtime-context.md)，首次接入见[首次使用](references/first-use.md)，旧参数见[退役边界](references/schema4.md)。发布授权仅限本轮指定项目。
