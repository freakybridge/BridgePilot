---
lifecycle: completed
validation_status: passed
---

# Forge 分发清单归属迁移验收

## 用户决定与范围

2026-11-03，用户要求将根目录 `bridgeforge-manifest.json` 迁入 `source/forge/`，使 BridgePilot 根目录承载整体产品信息。经 confirm 与迁移计划后，用户明确授权实施，并在交付后调用 `summary accept` 验收、调用 `git-sync release` 授权本仓库本次改动的源码发布。

唯一活动清单为 `source/forge/bridgeforge-manifest.json`。根目录不留副本、不新增 BP 替代清单；文件内源码路径仍相对仓库根解析。同步加载、生成、工厂识别、共享安装、Hook 触发、BP 档案与安装保护、测试、dogfood 和当前有效文档。历史交付记录保留原路径语义。

本主题此前未登记独立 TODO；已按总表去重，本次不补造编号或结算无关事项。用户级安装、真实下游更新与安装包 Release 不在本轮范围。

## 实施结果与关键经验

- 统一 Rust 清单路径入口；旧根文件名仅保留防覆盖识别和明确诊断，不作为读取回退。损坏的新清单不能静默转读旧文件。
- 工厂及 BP 仓库定位仍以仓库根为基准。新旧源码布局均禁止成为用户档案库或安装覆盖目标；仅嵌套清单的配置路径可正常绑定独立档案。
- 共享安装器与清单漂移检查使用新位置。旧 updater 可能因找不到根清单退出，应从已有 `install-shared-skills.ps1` 获取新源码入口恢复，不能复制清单回根目录绕过迁移。
- 清单迁入 Forge 目录不会进入按 Cargo 包收集的源码投影，不产生自哈希循环。138 项资产、20 项共享 Skill 的登记保持；资产 ID、source、target、strategy、资产历史、退役记录和用户指令保持原语义，仅 29 个派生哈希及版本字段变化。
- 历史迁移记录可以比其活动资产存续更久。既有测试错误地要求已退役的 `todo_contract.json` 仍在活动资产中；本次改为同时核验源码确已不存在、对应退役记录和全部历史哈希仍保留，未删除或重写历史。

## 已完成验证

所有本机日志和收据位于 `.runtime/tests/manifest-migration/`，不作为新的版本登记或开发账本。

| 检查 | 实际结果与证据 |
|---|---|
| 清单定位与旧布局保护 | 2 项通过，`core-tests.log` |
| BP 安装与存储 | 15＋20 项通过，`bp-tests.log`；含新旧布局拒绝覆盖、仅新清单配置成功及错误产品根拒绝 |
| 工厂清单生成与历史保护 | 10 项通过，`factory-manifest-tests.log` |
| Hook 新路径与运行工具替换 | 1＋2 项通过，`hook-tests.log`、`runtime-tests.log`；3 个 child fixture 入口标记 ignored，由父场景调用，不作为独立通过项计数 |
| 共享清单安装 | `shared-manifest.log`：分区隔离、旧入口恢复路由、六个漂移／回滚场景通过 |
| 共享安装事务 | `shared-transaction-final.log`：隔离打包、重复安装、回滚、提交后延迟清理及旧 Skill 退役场景通过；故意占用的备份清理告警属于预期测试路径 |
| BP 编译 | 原生 `cargo check --locked ... -p bridgepilot-orchestrator --bins` 通过，`bp-check.log` |
| 受管 Forge 构建 | `prepare.log`：3 个运行产物 prepared，自检执行成功，149 文件事务写入，retired_files=0 |
| 清单语义对比 | `manifest-comparison.json`：根副本不存在，资产与历史保持，仅派生字段变化 |
| 最终只读检查 | `final-check.json`：manifest changed=false、factory-version healthy；build-assets 的 --check 与 --dry-run 均 current、built=0；869 个受监测源码、文档、镜像与运行文件的内容及修改时间未变 |
| 独立复核 | `migration_review` 只读复核真实差异、安装恢复路径、资产边界与 9 个变动 Rust 镜像；覆盖补齐后无阻断问题 |

Rust 定向测试合计 50 项通过。原测试编译缺少导入已修正；镜像刷新最初被沙箱 `.codex/` 只读权限阻断，取得必要权限后同一受管入口成功。安装事务初次调用尚未刷新的旧 CLI 失败，完整运行工具准备后在新隔离目录复测通过。上述失败不计为通过。

## 验收与发布边界

用户验收覆盖本次源码、产品源、dogfood、fixture 与本机受管工具验证。真实下游、实际用户级安装和业务运行未执行、未验证，不从工程验收推导对应结果。

本次源码发布复用已验证版本，不重复升版：BP 1.0.0 → 1.0.1，BF 17.0.0 → 17.0.1，orchestrator 0.14.0 → 0.14.1。全部受影响内部包的前后版本与原因见根 CHANGELOG 的本次条目；其他包不变。

发布基线为 `640e6b01114660041859d1db2bb789602cb88263`，原生版本与发布说明收口后，通过 `.build/bridgeforge/bin/bridgeforge-factory.exe git-sync --message-file <本轮消息文件>` 提交并推送 `origin/main`。本记录随发布提交保存，实际提交、远端一致性和成功状态以本次受管 Git 收据为准；不调用退役的 `--release`，不把普通同步收据当作 GitHub 安装包 Release。GitHub 安装包发布、用户级安装及真实下游仍未执行。
