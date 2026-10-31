---
name: git-sync
description: 审阅实际变化并通过受管入口提交、推送和核验；release 额外分析累计变化、更新主程序版本并公告子包累计版本。用户明确调用 /git-sync 或 $git-sync 时使用。
user_invocable: true
argument: 可选：release
---

# git-sync — 共用同步与显式发布

用户显式调用授权本轮指定范围的同步。复用已有决定，普通模式不升版；release 只升主程序，不替子包补做 develop。两模式沿 S1–S5 执行一次，不自动串联 summary、验收或业务测试。

## S1 一次预检与整理

使用当前项目已安装的受管入口；工厂直接使用根 AGENTS 指定的 factory 入口。日常同步不执行前置工具准备、Cargo 引导或独立 self-test。工具缺失、不匹配或不支持本流程时报告维护缺口，不隐式安装升级，不退回手工 Git或解释器包装器。

执行 `git-sync --preflight`，一次读取工具身份/能力、配置、分支、HEAD、工作区和实际推送目标。此时 remote_state=not_fetched 不能证明远端一致。普通同步不因缺少发布配置而要求迁移；release 读取[版本合同](references/schema4.md)，新协议须有 `git-sync-remote-context-v1`、`git-sync-development-closure-v1`、`version-coverage-v1`，从预检 tool 字段核对，不另跑 self-test。

审阅授权范围内的真实 diff；已有仍有效的内容结论直接复用，clean 不展开无关源码。生成简体中文提交消息 `<类型>: <描述>`（feat/fix/refactor/perf/docs/chore），固化最多三条实际行为效果；不能用文件列表代替效果或猜测未知语义。多行消息用项目 .runtime 中的 message-file，正文不经 shell 字符串中转。

平台权限按当前明确限制确定；仅在真实缺口时读取[权限与异常处理](references/runtime-context.md)。缺配置、历史归属不明等实际问题才读取[配置引导](references/first-use.md)，不在正常路径预演异常。

## S2 接入远端

普通模式直接运行受管 `git-sync --message-file <文件>`，由同一次调用完成 S2–S5。

release 先运行 `git-sync --sync-remote`：受管 fetch、必要快进及本地改动保护/恢复，返回 remote-ready；该操作会写 Git 状态，不是只读预览，不提交、不推送、不升版。实际推送目标有未接入变化或发生分叉/冲突时停止，保留现场。

远端接入后再确定最终发布范围，避免分析完才正常拉入新代码。不要拆成 Agent 手工 fetch、stash、pull 命令。

## S3 确定最终操作

普通模式跳过发布分析。release 在远端接入后运行一次零写入 `--release-preview`，审阅累计差异和 product_constraints：最低主程序级别、累计 breaking 包/迁移及接受依据要求。该下限来自已记录影响，不能代替本次主程序真实兼容性判断。

生成完整 product 分析，绑定本次 fingerprint；子包版本和累计理由来自已收口账本，不重新判级升包。已明确且仍适用的不兼容决定直接复用，未知或未接受影响不得猜测通过。显示目标与理由不再次索取已有发布授权。

```text
.deps/bridgeforge/bin/bridgeforge git-sync --sync-remote
.deps/bridgeforge/bin/bridgeforge git-sync --release-preview --message-file .runtime/message.txt
.deps/bridgeforge/bin/bridgeforge git-sync --release --skip-fetch --release-analysis-file .runtime/release-analysis.json --message-file .runtime/message.txt
```

分析文件由 Agent 在预览后生成，不是上例自动产生。实际 release 内部构建并校验最终计划，然后立即进入写入和提交；通常省略外层带分析预览，仅需要独立只读审阅完整计划时选用。无需同一宿主进程；实际输入及必要环境一致即可使用分析，真实变化仍须重新核验，不只替换旧 fingerprint。

## S4 必要写入与提交

受管入口只做真实输入需要的产物处理和既有提交检查；主程序版本、原生载体、发布记录及必要产物按事务处理。G1 的前置准备退役不取消发布结果必需的构建，但不能以必要构建为由恢复每次固定刷新工具。

release 核验实际受影响子包的版本覆盖、历史连续性及明确交付物；缺口则说明并停止，不补做 develop、不降级普通同步。普通同步不执行上述发布门禁。无实际差异不制造空提交或新版本，clean 仍可能有累计未发布变化。

预览的 `version_coverage` 引用已认可的收口记录，`applicable_required_checks` 列实际适用的独立发布证明范围；不把全部 development 检查清单当作发布待办。缺记录或旧记录不兼容时，先核对已有受管验证与收口衔接，不直接判定未测试；内容、依赖/构建或交付物变化则列出具体包、分项或文件。只报告无法复用的缺口，不自动补测、扩成全包验证或按全部登记检查估算发布耗时。

Git pre-commit 保持现行轻量检查，不使用 --no-verify。待提交内容须符合审阅范围及计划内自动写入；commit 后使用固定提交对象，不为证明同一提交反复扫描工作区。Forge 不再提供同仓库并行 git-sync 防护；构建/develop 的锁和 Git 自身锁不因此删除。

## S5 推送、核验并结束

由受管入口推送；随后一次独立核对工作区、HEAD、实际 push 目标及 ahead/behind，与收据一致即结束，矛盾才追查。不以 upstream 一致替代 push 目标一致。

返回实际提交消息/commit、推送或 no-op、工作区和 stash 状态、真实效果摘要。release 另列主程序及自上次正式发布以来全部变化子包的旧新版本与原因，新增/退役单列，未变包可汇总；失败不把计划版本称为已发布。

复用收据的产物复用/重建数量及主要 timings_ms；sync_elapsed_ms 是工具内部时间，不是对话端到端时间，嵌套阶段不能相加。没有可靠时间戳不编造耗时，不为收尾扫描会话日志。

## 共用授权与失败边界

正常路径零追加业务确认；真实未决策略、范围、不兼容影响及需人工批准的平台权限合并为至多一次主动确认，已有授权直接复用。平台自动批准不计新增用户确认；平台强制审批不可绕过，不能承诺任意现场都只有一次弹窗。确认后出现实质新风险且预算用尽则保留现场停止，不默认接受。

分叉、冲突、缺 upstream、Hook/版本/产物失败、网络或鉴权错误按实际阶段报告；保留用户修改、冲突和 stash。提交前自动写入失败按事务边界恢复，无法安全恢复则保留现场；提交成功而 push 失败保留提交，不强推、不删除提交或反复重跑完整同步。提交内容偏离计划时不继续推送，复用内容一致性保护，不另建 Hook 自动修复流程。

$ARGUMENTS
