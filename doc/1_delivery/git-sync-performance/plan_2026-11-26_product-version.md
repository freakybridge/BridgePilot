---
lifecycle: active
validation_status: awaiting_user_acceptance
---

# 产品四段版本与 BP 组合来源

2026-11-26，基准 `301c289`。主对话实施，预算最多 2 小时、预计 8 万新增 token、最多两轮本地验收。范围覆盖 Forge 安装收据、产品发布写回、BP 安装升级／回退及其规则和镜像。其他会话的 Skill 变更保留。

1. 产品前三段由项目维护。第四段使用本项目实际采用的 BP 整体基础版本 `M.m.p`，按 `M×1000+m×100+p` 派生，`M≥1`、`0≤m≤9`、`0≤p≤99`。子包保持三段；组件换版纳入新组合时更新 BP 基础版本。
2. 下游升级时绑定固定产品来源到已有发布驱动收据。发布锁内、Git 暂存之前写回产品版本文件。预检零写入，重复执行保持同号。缺少来源或编码越界时在修改前停止。
3. 以 BridgeLibrary 业务版本 `0.75.1`、历史标签 `v0.75.1`、BP `4.0.0` 验收新标签 `v0.75.1.4000`。核对安装、旧包兼容、来源漂移、打包、提交号及两个平台的写操作，沿用 R1、R9。真实下游写入和正式发布另行授权。

## 修改落点与代码量

| 编号 | 源文件 | 职责 |
|---|---|---|
| 1 | `source/forge/core/src/native_release.rs` | 解析产品版本、编码 BP 来源、比较版本、选择发布说明 |
| 2 | `source/forge/workflow/src/template_sync.rs` | 读取本次采用的 BP 来源，在计划和实际应用时核对同一份 VERSION |
| 3 | `source/forge/workflow/src/project_release_install.rs` | 将来源写入已有驱动安装收据，兼容旧收据升级 |
| 4 | `source/forge/workflow/src/project_release.rs` | 从本项目的驱动收据取版本，计算预览、按需写回，生成对应发布说明 |
| 5 | `source/forge/workflow/src/native_release.rs` | 在发布锁内、暂存之前写回，核对提交树和包使用同一个版本 |
| 6 | `source/orchestrator/install/versions.rs` | 允许 BP 整体版本带第四段，子包继续按三段解析 |
| 7 | `source/orchestrator/install/install.rs` | 处理新旧包比较、组件组合身份和显式回退 |
| 8 | `source/orchestrator/install/release.rs` | BP 自身打包、标签与发布说明使用完整产品版本 |

按上表唯一维护源统计，生产代码预计净增 88–168 行，实测净增 172 行；内联测试预计净增 76–128 行，实测净增 88 行。统计包含源文件空行和注释，版本文件、清单、文档、生成镜像及临时验收脚本另计。

BP 基础版本从 `4.0.0` 调整到 `5.0.0`，原因是正式产品版本格式和旧发布驱动收据的升级要求发生变化。Forge CLI 从 `22.0.0` 调整到 `23.0.0`，core 从 `12.0.0` 调整到 `12.1.0`，workflow 从 `0.17.0` 调整到 `0.18.0`。BP install 从 `0.11.0` 调整到 `0.12.0`，orchestrator 从 `0.19.0` 调整到 `0.20.0`，用于交付支持四段版本的安装与回退入口。

## 本地验收

2026-11-26 完成两轮本地验证。最终收据在 `.runtime/tests/bpv/verification.json`，保存各场景的相对路径、测试发布提交号及工具哈希。

| 编号 | 最终验证 | 结果 |
|---|---|---|
| 1 | Cargo workspace 全目标检查、core／workflow／install 单元测试、工厂受管构建与镜像检查 | 编译通过，4 项单测通过，3 个受管工具就绪；版本、manifest 与 Template 均一致 |
| 2 | 本地 Git 配合模拟 GitHub、GitLab 发布接口 | 每个平台 10 个场景通过。保留原标签，从三段迁移、只升级 BP、只升级业务均生成预期版本。重复发布沿用同号；缺少来源、来源漂移、推送失败和平台失败落在预期阶段 |
| 3 | BP 自身打包并安装到隔离目录 | 产品包为 `5.0.0.5000`，组件为 Forge `23.0.0`、orchestrator `0.20.0`。从旧包 `4.0.0` 升级成功，普通降级被拒绝，显式回退恢复到 `4.0.0` |
| 4 | 四个登记项目现行配方的隔离预检 | BridgeLibrary、CausisRiskSuite、HoldemTrainer、StratusAgent 分别得到 `0.75.1.5000`、`3.14.2.5000`、`0.50.1.5000`、`10.11.0.5000`，预检前后原始 VERSION 字节相同 |

真实下游构建、宿主机安装和线上发布未验证。CausisRiskSuite 的旧手动辅助脚本 `scripts/bump_version.py` 仍只接受三段，当前登记的发布配方未调用它。
