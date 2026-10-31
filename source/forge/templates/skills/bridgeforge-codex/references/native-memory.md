# Native Memory 状态处理

仅当根入口的只读 `bridgeforge memory-sync status` 结果不是 `declined`，也不是当前策略下完整健康的 no-op 时读取。

- `approved + enabled + hookInstalled + !hookRuntimeVerified`：只记用户级 runtime gap；禁止把“已安装”描述成“健康”，禁止重复 repair 或前台触发 reconcile，等待下一次真实生命周期事件登记 pending 并启动 worker。
- `approved + enabled + !hookInstalled`：把本地-only `repair-hook` 归为 safe。该 safe 来自已保存的长期授权，不是项目更新授权。
- `approved + disabled_by_user`：保留现状并记 gap，禁止擅自重开。
- `consent=null + disabled`：把首次 `setup`、private 仓库和用户 Hook 安装合并为本轮唯一 risk；拒绝后才运行 `decline --confirmed`，同意后才运行 `setup --confirmed-enable`。
- `consent=null + enabled`：授权状态损坏，保留现场并阻断；禁止猜测修复或补写授权。

首次 risk 卡必须披露：同步 `~/.codex/memories/**` 中除根 `MEMORY.md`、`memory_summary.md`、`raw_memories.md` 外的文件，这三个 Codex 自维护索引固定留在本机；不同路径修改自动合并、同路径双改停止并保全两份、生命周期 Hook 持续自动同步，并且目标必须是指定 private 仓库。确认后形成长期授权；目录、远端、可见性或协议未变化时，日常同步、自愈、重试、no-op 和 hook 修复不得重复询问。

`repair-hook/setup/decline` 必须传 `--project-root .`，并进入根入口的统一 safe/risk/gap accumulator；禁止提前执行或另问一次。`repair-hook` 只能修改用户 hooks 并核对受管 Rust 入口，禁止访问 GitHub、Git、读取 Memory 或调用 `reconcile`。

实际同步只由已授权的生命周期 Hook 触发：Hook 只登记 pending 并启动或复用单一隐藏 worker，worker 合并并发事件；每次同步前必须验证长期授权、远端身份和 private 状态。`gh` 登录失效时必须无感复用系统 Git 凭证完成 private API 校验，不得要求用户重复登录，也不得回显或持久化凭证。用户级 Hook 只能调用 updater 安装的用户级 `bridgeforge` Rust 二进制；禁止持久化任一项目的运行时路径。

- `pending` 或 `busy`：表示等待，禁止算作成功；状态不变时不得提示用户。
- `degraded` / `failed`：同一 `alertId` 只在下一次 SessionStart 或 `$bridgeforge-codex` 输出一次；随后保持静默，直到状态产生新的告警 ID。
- `conflicted`：展示 `activeConflict` 的路径与两份保全位置，一次只问当前冲突选择；用户决定后使用 `resolve --conflict-id <id> --choose <path>=local|remote`，每个可同步冲突路径必须恰好选择一次。旧冲突中的三个本机索引自动忽略，不要求选择，也不得从远端恢复。禁止按时间戳、本机或远端默认覆盖。若决议前远端变化，仅当新远端逐字节等于 captured local 时允许安全重放；否则重新取证。
- 死亡锁和受管临时目录由 worker 自动验证并自愈；超过五分钟仍未完成必须进入 degraded / failed，禁止无限 busy。
- 内容无变化必须 no-op 且不创建 commit；有变化必须以远端 HEAD 为父创建普通 commit，禁止 parentless force-push。
