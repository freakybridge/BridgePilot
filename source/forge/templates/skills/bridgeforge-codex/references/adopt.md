# Codex 旧骨架受控接入

仅在根 skill 判定为 `adopt` 后读取。

1. 只运行 `bridgeforge project-sync --mode adopt` 生成 latest current-only rebuild 计划。
2. 仅允许“无合法版本戳但已有骨架资产”的项目进入；双戳、非法戳必须零写阻断，禁止猜测或自动补戳。
3. 主对话根据 planner 的项目资产盘点生成 `PreservationManifest`；旧 Markdown Rule／项目 Memory 同样按普通项目文件处理，不执行语义迁移。
4. 所有需要用户决定的项目资产逐项展示，与风险动作汇总为一次确认；确认前零写入。最终 apply 传入已确认的 `PreservationManifest` 与 `--confirmed-preservation-manifest`，不得在既有确认后追加清理确认。
5. 散落 Hook、非 canonical 命令或无法闭合的目录必须零写阻断。主对话只能把整理显式分派给 `implementation-worker`；它只能先在临时副本或受控前置步骤中整理为 `.codex/hooks/project_XXXX/entrypoint.rs` 自包含目录并闭合 `.codex/hooks.json`，然后重新规划和确认。
6. 准备 apply 时返回根入口并读取 `references/transaction.md`。

发现 `.claude/` 或 `CLAUDE.md` 时仅提示“Claude 骨架已停止支持”，不得读取、迁移、删除。旧项目 Rule／Memory 仅参与普通资产归属盘点，不作为指令加载或迁移输入。
