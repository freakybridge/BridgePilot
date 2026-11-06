# Rust runtime preflight

仅在诊断涉及运行时，或维护时 CLI / workspace 验证失败时读取。

- 诊断检查当前已准备的官方来源，维护检查本轮固定的来源提交。
- 先核对下述固定路径及 home 根目录的 `Cargo.toml`、`Cargo.lock`，以及 `source/forge/core/Cargo.toml`、`source/forge/cli/Cargo.toml`、`source/forge/hook/Cargo.toml`；这些是产品维护源路径，下游公共投影仍使用 `source/<包>`。缺失只报告，不调用缺失入口或补文件。
- Cargo / rustc 必须满足 workspace 下限，产品与 CLI 版本一致；锁或自检失败阻断维护，不阻断只读说明。
- 禁止使用 Python、旧脚本、其他 clone 或 PATH 中同名非受管二进制兜底。

只读验证命令：

```powershell
& $prepared.cli doctor `
  --product-root $prepared.source_root --json
```

doctor 检查工具链版本、`cargo metadata --locked --offline --no-deps` 和实际受管 CLI 的自检。收据须为 `schema=1`、`status=ok`，且 manifest/lockfile 位于当前产品 home。

诊断记录证据和缺口。准备或修复需沿项目维护入口执行，使用同一来源的构建、自检和公共产物收据。
