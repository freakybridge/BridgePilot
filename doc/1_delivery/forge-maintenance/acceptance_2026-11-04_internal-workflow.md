---
lifecycle: completed
validation_status: verified
---

# Forge 维护转为内部流程：验收记录

## 用户决定与范围

2026-11-04，用户要求斜杠菜单中不再显示 `bridgeforge-codex`，并选择将其正式改为内部维护流程、保留 BP 骨架升级能力。随后用户明确“开始吧”，授权实现及本机更新。交付后用户调用 `summary accept`，确认本次交付符合需求，并调用当前 BP 仓库的 `git-sync release` 授权正式发布。

本次范围覆盖源码、后续分发、旧安装受管迁移、BP 调用路径和本机更新。安装或本次发布不授权真实下游升级、下游提交／发布、恢复任务、修改原生 Memory 或清理资料。

## 实际交付

- 维护正文和九份引用资料从 `source/forge/templates/skills/bridgeforge-codex/` 移至 `source/forge/templates/maintenance/bridgeforge-codex/`，正文改名为 `MAINTENANCE.md`，移除 Skill frontmatter 和 `agents/openai.yaml`。
- 保留原分发身份及用户目录 `.codex/skills/bridgeforge-codex/`，由同一目录 stage／swap 和账本事务安装内部手册、资料及 updater；目录不再含 `SKILL.md` 或菜单元数据。此目录是内部文件载体，不是可发现的 Skill。
- `bridgepilot-update` 按本机 Skills 绑定读取内部手册；首次只有 BP 时，允许从同一成功源码更新收据的 `source_root` 读取手册并引导同源 updater。维护授权、来源核验、规划、apply、保护及恢复边界保留。
- 本机候选通过定向受管入口迁移，仅替换该目录及对应账本记录，保留其他共享 Skills 记录、用户指令和原配置；BP 程序及专用 Skills 经其原生安装事务更新。
- 为迁移补齐目录恢复目标、账本读写与恢复阶段的字节见证。独立复核指出的目录外部修改、非目标账本漂移、首次 BP-only 引导及账本切换后恢复窗口均已处理。

实现入口：[内部手册](../../../source/forge/templates/maintenance/bridgeforge-codex/MAINTENANCE.md)、[BP 升级入口](../../../source/orchestrator/skills/bridgepilot-update/SKILL.md)、[共享分发器](../../../scripts/bridgeforge_codex_shared_update.ps1)。README、INSTALL、项目目录地图、精确分发清单及适用 dogfood 同步更新。

## 验证证据

| 命令或证据 | 实际结果与覆盖 |
|---|---|
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/internal_maintenance.ps1 -RepositoryRoot D:/Quant/BridgePilot -Base D:/Quant/BridgePilot/.runtime/tests/internal-maintenance-final` | 11 项通过；旧入口退役、重复安装、回滚、未知目录、用户修改、规划后漂移、首次及已有安装的恢复外部修改、旧日志保守拒绝、账本恢复漂移和首次引导路径 |
| `scripts/tests/shared_transaction.ps1`，Base 为 `.runtime/tests/internal-maintenance-transactions-final` | 完整现有分发事务、原始字节回滚、已提交延迟清理、用户指令与相关 Skill 打包回归通过；运行中镜像造成的 cleanup warning 为既定注入场景，最终恢复完成 |
| `scripts/tests/shared_manifest.ps1`，Base 为 `.runtime/tests/internal-maintenance-manifest-final` | 六种分区漂移／回滚场景及旧入口恢复导航通过 |
| 独立 Agent 只读复核 | 最终写入见证顺序及恢复保护复核通过；不以实现者自查替代该结果 |
| 锁定 Cargo Release 构建及 BP 原生打包／安装 | BP 2.2.0 候选构建、十五项 payload 加 package.json 打包、本机程序和四个专用 Skills 安装通过 |
| Codex 原生 `skills/list` | `bridgeforge-codex` 返回 0 项，`bridgepilot-update` 仍 enabled，无加载错误；未启动任务回合 |
| 本机内部文件及档案 | 手册和全部相对引用存在；四个 BP Skills 为 current；档案 revision 102、6 条任务不变 |
| 受管工厂产物准备及零写入检查 | 3 项产物准备成功；随后 `build-assets --factory --project-root . --check` 返回 current、read_only=true、reused=3、built=0；`manifest --root . --check` 返回 ok、changed=false |

本机综合收据：`.runtime/tests/internal-maintenance-final/local-verification.json`。受管准备首次因 BF 18.0.0 缺独立 CHANGELOG 条目停止，补齐说明后继续同一入口成功；不将先前停止视为成功，也未借此扩大业务验证。

## 版本与经验

相对上次正式 BP 2.1.1：BP 2.1.1 → 2.2.0，接入内部维护流程；BF 17.3.0 → 18.0.0，移除公开可调用入口属于用户已接受的不兼容变化；install 0.9.1 → 0.10.0，交付新的 BP 调用内容。orchestrator 0.18.0 和其余子包版本不变。编译时记录的 BF source-reference 不等于外部用户级 BF 已安装；本机仓库工具的就绪状态另以受管产物检查证明。

菜单退役需要同时核验分发注册、旧目录替换、可发现文件移除及原生发现结果；只修改描述或自动触发开关不能作为“从菜单消失”的证据。受管目录即使位于 Skills 根下，也可作为不含 `SKILL.md` 的内部载体，现有所有权和恢复事务继续适用。

旧 schema 1 恢复日志若缺本次写入指纹，且已有新目录／账本需要移除，必须保留目标、备份和日志并报告，不能声称可自动恢复所有历史日志。恢复原始 SHA256 字段使用裸摘要格式；曾误按带 `sha256:` 前缀检查，终轮修正后完整回归通过。

## 验收及发布边界

本次用户验收覆盖上述实现、定向验证、独立复核和本机原生发现。未执行真实下游升级、业务 runtime smoke 或全仓回归，不将静态内部调用校验等同于一次真实下游升级。

本主题未发现唯一匹配的独立 TODO，不补造已完成待办，不结算 TODO-016、TODO-050、TODO-055 等其他事项。正式发布结果以本轮受管 CLI 的 `status=released`、`publication_verified=true`、固定提交和完整资产核验为准；本机安装或用户验收本身不等于正式发布完成。
