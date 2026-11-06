# Forge 用户级迁移

先用同一份已准备 Forge 来源升级所有仍依赖用户级 Forge 的项目，并从新 Codex 会话核对项目 Skill 的实际路径。用户级迁移需要明确授权，执行前确认传入所有已切换项目根。

从已验证官方源码调用 `scripts/bridgeforge_codex_shared_update.ps1 -MigrateUserScope -SourceRepositoryRoot <官方源码根> -PreparedSourceCommit <固定提交> -MigratedProjectRoot <项目根数组>`。入口先逐仓验证当前骨架及公共工具，再准备用户级变更。

- 将 Forge 登记且哈希一致的旧用户级 Skill 退出活动路径。用户级 CLI 和产品检出必须通过原归属核对，事务完成后保留其恢复副本。
- 用户级 `~/.codex/AGENTS.md` 有不同内容时先按原始字节备份再更新。同样的文本复用现有文件。
- 已有 Forge Memory Hook 时，从同一锁定来源准备专用执行器并校验来源，再切换旧注册。保留启用状态、Memory 配置、授权及第三方 Hook。首次显式安装 Memory 执行器时可加 `-InstallMemoryRuntime`，后续由用户决定是否启用同步。
- 归属、内容或恢复记录有冲突时保留现场。已有其他团队或个人的 Skill、Hook 和用户配置由原归属管理。

结果以 `user_scope_migrated` 收据为准。`cleanup_pending` 表示有待核销的事务收尾。Memory 的后续运行证据见 [Native Memory 状态处理](native-memory.md)。
