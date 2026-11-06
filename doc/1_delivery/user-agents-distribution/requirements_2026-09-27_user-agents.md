---
lifecycle: completed
validation_status: verified
---

# 用户级 AGENTS 分发

## 已确认范围

统一标准放在 BridgeForge GitHub 仓库，面向所有安装者。每次获准维护时，用户级 `~/.codex/AGENTS.md` 相同则不写；不同则先备份再覆盖，不双向合并。初始内容采用本轮已确认的精简用户偏好。项目 AGENTS 继续更新公共区并逐字保留项目专区。

| 项目 | 决定 |
|---|---|
| 授权 | 用户已确认实施；不提交、不推送、不升级真实下游 |
| 规模和预算 | L；90 分钟、40k 新增 token 为估算；最多 1 个独立复核 Agent、2 轮验证 |
| 产品传播 | 通用能力；用户模板、受管 Rust 工具及现有分发入口；同步工厂 dogfood |
| 版本 | 新增能力，1.18.0；CHANGELOG 标记 product |
| 用户模板 | templates/user/AGENTS.md；与项目 templates/AGENTS.md 分开 |
| 机器合同 | user-agents-manifest.json；固定资产 ID、目标、覆盖策略和内容哈希 |
| 事务 | 加入既有 Home、CLI、Skills、ledger 事务；保留成功备份，失败回滚 |
| 不做 | 个人配置分支、多版本选择、双向合并、个人履历、自动发布 |

## 验收

1. 缺失时创建，相同时不改目标或新增备份。
2. 不同或本地编辑过的文件均备份原字节后覆盖。
3. 源哈希不符、目标非普通文件、重解析点、计划后漂移时阻断。
4. 整批失败或中断可回滚；成功清理保留用户文件备份。
5. 旧薄入口刷新后能够进入新增用户指令分发，不能误报已经覆盖。
6. 项目 AGENTS 公共区和专区行为不变；工厂检查、测试、独立复核有证据。

## 风险和边界

- 用户级模板修改将影响安装者后续会话；维护说明必须明确本地编辑会被替换及备份位置。
- 标准文件不含个人履历或隐私。只维护默认 USERPROFILE/.codex；不删除 AGENTS.override.md，不保证当前已开启会话重载。
- 真实下游、GitHub 发布和真实用户安装不在本轮验证范围。

## 结果

实施阶段完成时版本为 1.18.0，尚未提交、推送或安装到真实用户目录。项目根和项目模板 AGENTS 正文未改动，本轮新增用户模板与分发能力。后续受管发布版本以根 VERSION 和 Git 同步收据为准。

| 验证 | 证据与结果 |
|---|---|
| 受管构建 | `bridgeforge build-assets --project-root .` 成功；Hook / CLI 均返回 schema 2 构建收据，source_tree_sha256 为 `eb078ce04b7d88e797ebbee069cc14ee09d2d89da5535a125fe5a72e7d3d1575` |
| Rust workspace | `cargo test --locked --config scripts/tests/factory-cargo.toml --manifest-path .codex/hooks/Cargo.toml --workspace`：193 passed、5 ignored、0 failed |
| 用户指令断言 | 首次暂存、换行等价零写入、原字节见证、源哈希与目标类型阻断、事务路径冲突、override 保留、junction 阻断、预览参数零写入均通过 |
| 发布断言 | 用户模板漂移阻断；提交被拒后恢复原清单、版本及二进制，并保留用户模板编辑，通过 |
| 共享事务 | 临时 profile 中验证创建、no-op、原始字节回滚、中断恢复、并发编辑保护、备份冲突、成功保留备份及占用 CLI 延迟清理；中文 profile + CP437 解码通过 |
| 工厂完整回归 | `cargo test --locked --manifest-path scripts/tests/Cargo.toml`：87 passed、3 ignored、0 failed；包含真实临时仓库中的完整初始化、自动发布和运行时修复，433.46 秒完成 |
| 工厂检查 | baseline 为 clean；factory-version healthy；skill-metadata 无问题；project-structure 无错误；manifest --check changed=false；git diff --check 通过 |
| 独立复核 | 同一 review-auditor 发现的编码与 fixture 问题已修正；有限二次复核通过，发布清单输入见证与回滚无遗漏 |

首轮的两个旧工厂 fixture 缺少新用户模板，已补齐；一项长构建期间源码更新触发输入漂移保护，不能算通过，因此第二轮固定源码后重跑。Windows 正在运行的首轮测试镜像曾阻止第二轮链接，退出后重试同一受管命令，未使用替代入口。

GitHub 真实旧入口升级、真实用户安装、真实下游与当前会话重载未验证。只读检查不等于部署；用户验收独立判断。

最终两套测试合计 280 passed、8 ignored、0 failed；跳过项沿用既有显式环境或子进程辅助条件。本轮使用 1 个独立复核 Agent、2 轮验证；未执行真实仓库提交、推送或下游维护。

## 用户验收

2026-09-27，用户明确调用 `$summary 同意验收`，验收当前交付；上述真实部署未验证项继续保留，不视作已部署。验收复用本轮工具返回的成功退出与测试统计；自动 test_receipts 中 exit_code=null 的记录不单独作为通过证据。无未解决的交付 blocker。

用户同时显式调用 `$git-sync`，授权提交、推送本轮实现与验收记录；该授权取代实施阶段的暂不提交限制，不授权真实用户安装或下游维护。提交、版本及推送结果以随后受管 git-sync 收据为准。

已检索并阅读 BridgeForge 受管同步历史，现有 Skill 已覆盖入口、冲突和同步收据要求，本次不新增 Rule、Hook 或 AGENTS 条款。当前 topic 为归档候选，未执行归档；如需归档，另行调用 `$archive-scan`。

## 回复表格编号增量验收（2026-11-06）

一张监控影响对照表缺少编号列。核对后确认，旧条款只要求给多步操作编号，表格行编号需要单独表述。用户要求在现有条款原行修改，并通过 `$summary accept` 验收本次交付。[用户级 AGENTS 模板](../../../source/forge/templates/user/AGENTS.md#回复方式)

第 3 条现在要求回复中的表格以「编号」为首列，数据行从 1 开始顺序编号。分发清单的用户指令哈希已同步。`git diff --check` 通过，受管 `manifest --root . --check` 返回 `status=ok`、`changed=false`，`check factory-version --root .` 返回 `healthy=true`。

本次验收覆盖模板及分发清单。本机用户级副本尚未更新，新会话的实际输出未验证。用户另行调用 `$git-sync release` 授权正式发布，发布结果以受管 CLI 收据为准。
