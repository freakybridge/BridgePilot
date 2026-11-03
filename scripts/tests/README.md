# BridgePilot 回归测试

本目录包含 Forge 工厂与 BP 总管的私有测试。Git 集成检查只操作隔离临时仓库和本地 remote；真实用户安装、项目、档案和 Memory 远端不属于默认测试目标。

## 运行入口

从仓库根执行，所有编译输出显式进入 `.build/`。根 workspace 的 Forge 私有单元测试由 `factory-cargo.toml` 启用 `bridgeforge_factory_tests`；BP 测试也在根 workspace。`src/` 是独立集成测试 crate，通过自己的 manifest 运行。

```powershell
# 编译测试，检查跨包注册与辅助模块引用。
cargo test --locked --config scripts/tests/factory-cargo.toml --manifest-path Cargo.toml --target-dir .build/bridgeforge/test-target --workspace --no-run
cargo test --locked --manifest-path scripts/tests/Cargo.toml --target-dir .build/bridgeforge/integration-target --no-run

# 先列出实际用例，再按改动选择 package 与测试名称运行。
cargo test --locked --config scripts/tests/factory-cargo.toml --manifest-path Cargo.toml --target-dir .build/bridgeforge/test-target -p bridgeforge-workflow -- --list
cargo test --locked --manifest-path scripts/tests/Cargo.toml --target-dir .build/bridgeforge/integration-target -- --list
```

完整 workspace 回归不是每轮开发的默认要求。按[工厂验证说明](../../doc/0_architecture/design/development-completion.md)选择必要检查；筛选为零、编译成功和实际测试通过分别报告。下游公共源码构建不需要本目录，也不分发测试实体。

普通同步和真实 CLI 布局 fixture 验证完整的当前模板与运行时合同。运行前须完成根 AGENTS／INSTALL 中的受管准备；测试原样复制准备后的公共 CLI、Hook 及真实收据到隔离项目。编译这类测试时，从公共 CLI 的 `self-test --json` 取真实身份，仅向本次 Cargo 进程传入官方构建同名变量；不使用工厂身份或自行构造哈希／收据。

```powershell
$identity = & .build/bridgeforge/bin/bridgeforge.exe self-test --json | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or $identity.status -ne 'ok') { throw 'Public runtime identity unavailable' }
$env:BRIDGEFORGE_BUILD_SOURCE_SHA = $identity.build_identity.source_tree_sha256
$env:BRIDGEFORGE_BUILD_LOCK_SHA = $identity.build_identity.lockfile_sha256
# 在此进程中执行选定的 cargo test；上述变量不写入用户或机器环境。
```

## 保留范围

- 当前模板更新、来源证明、清单位置、分发与 dogfood 一致性。
- 普通 Git 同步、preflight 零写入、冲突和 stash 保留、提交与推送失败、并发及索引保护。
- 原生 `git-sync release` 的适配器边界、产物与远端发布状态核验；相关测试位于 `unit/native_release.rs` 和 `orchestrator/native_release.rs`，不执行真实发布。
- 当前工厂运行时准备、公共产物缓存、输入漂移、安装事务和用户文件保护。
- Memory 授权与私有性、排队并发、冲突恢复，以及进程、Hook、文件锁和路径安全。
- BP 任务、存储、调度、换机、安装和版本身份，以及必要的原生格式解析和输入所有权检查。

配置化 develop/release、旧收口账本、旧 cleanup 成功流程及其独占 fixture 和性能脚本已退役。保留旧入口拒绝、未完成事务保护与历史文件不丢失检查；历史资料中的旧测试命令不作为当前入口。旧参数 `--release` 与当前子命令 `git-sync release` 分别处理。

`#[ignore]` 中既有显式基准，也有父测试启动的子进程辅助项。按用途和调用方判断，禁止一次运行全部 ignored 项；父测试负责传入子进程的必要环境。真实 Cargo 基准仅在有明确性能核验需求时单独执行。
