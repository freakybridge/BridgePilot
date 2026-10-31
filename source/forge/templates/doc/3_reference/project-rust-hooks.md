# 项目自有 Rust Hook

项目 Hook 的源码、依赖、入口、构建、安装和注册归项目所有。普通骨架 update 只保留它们，不借用骨架 workspace 编译，不检查项目开发产物是否最新。公共 Hook 由骨架维护，两者只共用 Codex 的事件配置，不共享业务实现或构建依赖。

## 独立工程

已有 project_demo 可在 .codex/hooks/project_demo/ 维护自己的 Cargo.toml、Cargo.lock 和 main.rs。以下 manifest 的 [workspace] 使工程独立于父级骨架 workspace：

    [package]
    name = "project_demo"
    version = "0.1.0"
    edition = "2024"

    [workspace]

    [[bin]]
    name = "project_demo"
    path = "main.rs"

项目自行声明第三方依赖，不使用 bridgeforge_core 或骨架目录中的源码。Windows main 使用 #![cfg_attr(windows, windows_subsystem = "windows")]；启动子进程也需保持隐藏窗口、stdin/stdout/stderr、退出码及超时/子进程树语义。迁移时不能只替换成缺少超时处理的 Command::output。

在项目开发流程中生成并保存锁文件，从项目根通过自己的入口构建，例如 `cargo build --locked --release --manifest-path .codex/hooks/project_demo/Cargo.toml --target-dir .build/project-hooks/demo/target`，验证原业务行为，再安装到 `.build/bridgeforge/bin/project_demo.exe`。不依赖根 `.cargo` 配置或下一次骨架升级来补构建。项目可采用自己的构建脚本，但输出仍须显式位于 `.build/`，并自行维护完整调用链。

## 注册与调用

项目直接维护 .codex/hooks.json 中自己的 native handler，例如：

    {
      "hooks": {
        "SessionStart": [
          {"hooks": [{
            "type": "command",
            "command": ".build/bridgeforge/bin/project_demo check",
            "commandWindows": ".build/bridgeforge/bin/project_demo.exe check",
            "timeout": 150
          }]}
        ]
      }
    }

将自己的条目合入已有配置，不覆盖公共条目。bridgeforgeCodexId 的 bridgeforge-codex.project-hook.v1: 前缀归骨架所有，项目不能冒用。不同项目各自注册并调用自己的程序，不在用户级重复登记公共项目 Hook。

旧 .codex/project-hooks.json、bridgeforgeProjectHookId 和 hooks.json 中的 bridgeforgeProjectHooks 登记原样保留，不自动改写或拆分。新工程由项目直接维护 native handler，不需要骨架生成注册。

## 已有项目的维护

独立维护边界固定在代码中，不依赖 project-hook-policy.json。骨架不再读取旧策略或迁移状态、不再生成迁移提示，也不提供 project-hook-migration status/verify；旧文件留在项目中并被忽略。

旧项目程序和收据不会随公共布局迁移搬移，注册不会自动改路径。用户授权项目维护后，由项目补齐独立 manifest/lock/main、去掉骨架私有依赖，按自己的锁定入口构建、安装和验证。新产物进入 .build/；历史 .codex/bin 路径需要项目自行完成迁移和调用链验证。公共迁移遇到归属不明的旧文件时保留并报告阻断，不猜测删除。

骨架基线只校验公共资产，项目源码、锁文件或业务收据的变化不触发骨架重编译。项目 Hook 是否仍能正确运行，由项目开发与实际业务测试证明；骨架更新成功不等于业务验收。
