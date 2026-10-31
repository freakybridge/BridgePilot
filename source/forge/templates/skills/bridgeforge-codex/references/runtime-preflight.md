# Rust runtime preflight

仅在诊断涉及运行时，或维护时 CLI / workspace 验证失败时读取。

- 诊断检查已有产品 home，维护检查本轮 updater 刷新后的 home；禁止先更新再诊断。
- 先核对下述固定路径及 home 根目录的 `Cargo.toml`、`Cargo.lock`，以及 `source/forge/core/Cargo.toml`、`source/forge/cli/Cargo.toml`、`source/forge/hook/Cargo.toml`；这些是产品维护源路径，下游公共投影仍使用 `source/<包>`。缺失只报告，不调用缺失入口或补文件。
- Cargo / rustc 必须满足 workspace 下限，产品与 CLI 版本一致；锁或自检失败阻断维护，不阻断只读说明。
- 禁止使用 Python、旧脚本、其他 clone 或 PATH 中同名非受管二进制兜底。

只读验证命令：

```powershell
& (Join-Path $env:USERPROFILE ".codex/bin/bridgeforge.exe") doctor `
  --product-root (Join-Path $env:USERPROFILE ".bridgeforge-codex") --json
```

doctor 检查工具链版本、`cargo metadata --locked --offline --no-deps` 和实际受管 CLI 的自检。收据须为 `schema=1`、`status=ok`，且 manifest/lockfile 位于当前产品 home。

诊断只交付证据、缺口和修复建议，不安装工具链、构建或替换。修复另需授权；官方 updater 在仓库外构建、自检新产物并事务替换，失败回滚。禁止手工覆盖或用旧产物自检代替新产物自检。维护中的旧成功收据协议衔接仅按主 Skill §1 自动执行一次，不视为诊断授权或失败重试。
