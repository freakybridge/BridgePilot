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
