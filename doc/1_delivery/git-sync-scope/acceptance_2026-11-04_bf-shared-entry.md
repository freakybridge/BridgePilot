---
lifecycle: completed
validation_status: passed
---

# git-sync 统一使用 BF 用户级入口

## 用户验收2026-11-04

用户要求 `git-sync` 只由 BF 提供，不在 BP 安装项目级重复副本；完成修复后明确调用 `summary accept`、`git-sync release`，验收本次改动并授权当前仓库正式发布。此次只处理普通 `git-sync` 的重复入口，不改变独立的下游批量同步功能。

本主题没有先前登记的独立 TODO，本次直接记录实际验收，不补造编号或改写其他主题状态。工程验证、用户验收和正式发布分别判断；发布结果以随后受管 CLI 收据为准。

## 实施与依据

- 原因：工厂私有 `source/forge/factory/src/dogfood.json` 显式登记了四个 `git-sync` 项目镜像文件，和 BF 用户级共享入口同时被 Codex 发现。
- 移除这四项生成登记及 `.codex/skills/git-sync/` 下对应文件。删除前逐文件验证与 BF 模板源完全一致，并保存原字节到 `.runtime/tests/git-sync-scope/before/`。不递归删除未知内容。
- BF 模板源、共享分发登记和用户级 `git-sync` 的四个文件哈希均保持不变。工厂实际 Git 程序入口仍为 `.build/bridgeforge/bin/bridgeforge-factory.exe git-sync`；根 AGENTS 项目专区同步其 Skill 来源说明。
- factory 0.12.4 → 0.12.5，根锁和派生测试锁一致；主产品版本由本次原生发布流程判定。未修改 BP install、orchestrator 或 Forge 公共 CLI 的功能。

## 验证结果

1. 定向测试 `factory_skill_mirrors_are_explicit_private_and_readonly` 通过：工厂输出不再包含项目级 `git-sync`，BF 共享分发仍包含它。新增断言初次读取了项目合同而非分发合同，修正测试取值来源后通过；未放松生产校验。
2. 受管 `build-assets --factory` 成功：复用两个公共工具，仅准备一个工厂工具，写入三个实际变化文件。随后 `--check` 返回 `status=current`、`read_only=true`、`built=0`。
3. 最终逐文件哈希核验确认 BF 模板和用户级安装保持原字节，BP 项目镜像未被重新生成；`git diff --check` 通过。
4. Codex 原生 `skills/list` 强制刷新后，仅发现一个启用的 `git-sync`，scope 为 `user`，路径为 `C:/Users/bridg/.codex/skills/git-sync/SKILL.md`。证据在 `.runtime/tests/git-sync-scope/native-skills.json`。未另行打开桌面菜单做视觉复验。

## 边界

本次修复及本地工厂工具刷新已完成；不将其推定为其他电脑已更新。本轮发布授权不包含额外用户环境安装、下游升级、清理或重新发布既有下游。
