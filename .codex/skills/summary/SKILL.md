---
name: summary
description: 将当前对话的有效经验写入项目文档；accept 同时记录验收与相关 TODO，并清理有明确依据的临时测试，switch 同时保存接续信息。三模式复用同一整理、落盘和检查流程。
user_invocable: true
argument: 无参数、accept 或 switch
---

# summary — 经验、验收与交接记录

| 调用 | 本次记录 |
|---|---|
| `$summary` | 有增量的经验与必要索引 |
| `$summary accept` | 同上，加用户验收决定及相关 TODO；验收通过后清理有明确依据的临时测试 |
| `$summary switch` | 同上，加下一次接续所需信息 |

三模式直接沿[共用流程](references/ordinary-mode.md)执行一次；accept 加读[验收差异](references/acceptance-mode.md)，switch 加读[交接差异](references/switch-mode.md)。不先完整执行普通 summary 再启动另一套流程。

精确参数 accept 表示用户确认本次交付符合需求；必要证据缺失或已知 blocker 仍须如实保留。旧参数“同意验收”和其他参数只说明以上用法，不写入。用户要求只读时仅解释或预览。

仅保存本次授权的记录；accept 验收通过后可按验收差异清理本次相关临时测试及其专属引用，普通 summary 和 switch 不修改测试。除此之外不修改业务代码、规则、AGENTS、Hook、配置或 Memory，不自动归档、提交、推送、发布或补做 develop。经验、工程验证、用户验收和发布状态分别表述。

$ARGUMENTS
