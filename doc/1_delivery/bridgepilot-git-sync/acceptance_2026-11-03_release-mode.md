---
lifecycle: completed
validation_status: verified
---

# bridgepilot-git-sync release 模式验收

## 用户决定与范围

2026-11-03，用户要求为 `bridgepilot-git-sync` 增加类似 `git-sync release` 的批量发布模式，并选择：单项目失败保留现场，继续其余独立项目，最后统一汇总。用户随后授权实施，并在交付后显式调用 `summary accept`，确认本次技能交付符合需求。

本次验收范围是技能规则及现有用户级副本同步。默认全部已登记下游，也可用重复的 `--project` 指定子集；不带 release 保持普通同步。逐项目复用受管 `git-sync release --publication-required`，只有 `status=released` 且 `publication_verified=true` 才计为发布完成。未知结果先核销，接续仅针对受影响项目，不整批盲重发。受管命令由主对话执行，子 Agent 可分担只读核对。

实现事实源：[技能源码](../../../source/orchestrator/skills/bridgepilot-git-sync/SKILL.md)。本次变更属于 BP 私有本地扩展，未进入 Forge 公共模板、dogfood 或 BP 安装包；未修改 Rust 代码、项目名单脚本或程序版本。发布说明记录于根 CHANGELOG 的 Unreleased 段。

## 已有验证证据与限制

- `git diff --check` 通过；主对话核对两种模式、参数边界、失败继续策略、成功收据和接续要求。未执行独立 Agent 审计。
- 同步前确认用户级 SKILL.md 与原始源码一致，名单脚本两侧 SHA256 相同；保存旧技能备份后更新现有用户级副本。
- 更新后源码与用户级 SKILL.md SHA256 均为 `C25D3926E152FDE24D79837152AAEF6555DE848518BE370F59E5FBA0F7078C13`；用户级 git-sync、bridgepilot 及名单脚本引用路径检查通过。客户端随后提供了更新后的技能描述，已观察到原生技能发现更新。
- `python -X utf8 .../skill-creator/scripts/quick_validate.py source/orchestrator/skills/bridgepilot-git-sync` 因 `ModuleNotFoundError: No module named 'yaml'` 未能运行，不记为校验器通过；未安装依赖。
- 本次没有执行真实下游发布，故不证明各下游当前工具、鉴权、构建或发布链路可用；未做隔离端到端发布 fixture 或业务 runtime smoke。

用户验收不抹去上述自动校验及真实执行边界。检查现有 TODO 总表未发现本次独立技能扩展的唯一匹配条目；不将其他发布或 BP 演进任务据此结算，也不补造历史 TODO。

## 经验与发布边界

模式必须先于项目范围解析，不能将 release 传给项目名单脚本；普通同步的 preflight 和 no-op 规则不能套到完整发布。部分项目发布成功、源码已推送或工作区干净，均不能替代每个项目的正式发布核验。

用户同轮调用的是当前 BridgePilot 项目的 `git-sync release`，已授权该项目正式发布，不是执行 `bridgepilot-git-sync release`。本记录保存时发布尚未执行，最终结果以本轮受管 CLI 收据为准；不据用户验收或本地安装推定已发布。

## 版本报告格式增量2026-11-04

用户确认“两张表”的示例及修改计划后授权实施，随后于 2026-11-04 明确调用 `summary accept`，验收本次报告格式改动，并调用当前项目的 `git-sync release` 授权正式发布。本节补充此次验收，不改写前述历史决定。

- 主程序表固定为“项目／旧版本／新版本／发布结果／变化子包数”，每个目标项目均列出；子包明细固定为“所属项目／子包／旧版本／新版本”，列全已确认的版本变化并核对数量。
- 比较口径固定为上次正式发布至本次正式发布；不能用本地 VERSION、发布前 HEAD 或本轮自动编辑清单替代正式基线。复用已有 Release 显示相同旧、新版本和 0 个变化包，不重放首次发布时的升级明细。
- 缺基线或包范围不完整时保留未知，首次发布需明确证据；失败候选和未核验结果不混入成功发布统计。全部子包未变时用明确说明替代空占位行，发布凭证和实际失败阻点仍保留。
- 只修改此 Skill 的报告段，仓库源码和用户级副本同步；不改名单脚本、安装器、Rust 发布协议或任何项目版本。用户级旧文件同步前哈希与源码一致，备份位于 `.runtime/tests/bridgepilot-git-sync-report/before/SKILL.md`。
- Skill Creator 格式校验通过。依据已有四项目复用发布收据生成 `reuse-example.md`，并用 `mixed-example.md` 离线核对多个项目变化、全部复用、仅主版本变化、缺旧基线、部分失败的表格和计数；两份样例均位于 `.runtime/tests/bridgepilot-git-sync-report/`。未启动新的真实发布、构建或业务测试，未修改 Memory。

这仍是未纳入 BP 安装包的本地扩展，本次不扩大安装体系。当前用户级副本已更新，后续调用读取新报告规则。本次验收沿用格式校验、源码／用户副本一致性和离线样例核对结果，不将其冒充一次新的真实下游发布验证。

本主题没有新增独立 TODO，不改写其他事项状态。本轮仅发布当前 BP 仓库的报告格式改动；不因该发布调用重跑下游 release、升级环境或清理资料。主产品版本由原生流程判断，子包没有预先升版。正式发布完成须以本次 CLI 的 `status=released` 且 `publication_verified=true` 为依据。
