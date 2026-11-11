---
lifecycle: active
validation_status: awaiting_user_acceptance
---

# Forge 项目 Skill 迁回 .codex/skills：实施与批量发布

## 目标、授权与预算

用户在当前对话明确批准：迁移 Forge 自带项目 Skill 至 `.codex/skills/`，同步模板与 dogfood，完成 Forge/BP 正式发布、四个登记下游受管更新及正式发布，保留自定义内容和现有改动，全部完成后统一汇总。随后显式启动 autopilot，原生 Goal 总预算 **10,000,000 token**；阶段与 Agent 切换不重置。时间原估3–5小时，不设时间硬停。原生读数不是精确费用，子Agent归集口径未单独实测，不伪报周额度。

任务从2026-11-11开始，2026-11-12完成工程与正式发布链路；用户验收尚未代替。维护源仍为 `source/forge/templates/skills/`。传播属于通用产品安装接口及配套文档，需要版本／CHANGELOG和自身dogfood。本轮 Forge 是由 BP 组合发布交付，不宣称存在另一个独立 Forge Release。

## 最终完成状态

| 阶段 | 实际结果 |
|---|---|
| P0 归属与迁移 | 20个Skill／35个文件，稳定asset id、正式历史哈希；缺失旧别名也进入指纹，精确备份、退役、回滚；自定义和未知内容保留 |
| P1 模板与dogfood | 三工具准备成功，主仓写入56、退役35、retained_directories=[]；旧.agents移除，新根20目录；模板、BP调用与现行设计一致 |
| P1 验证 | 本机Codex实际20个enabled repo Skill唯一发现；隔离新装／旧35迁移／自定义／冲突／漂移／回滚通过；独立复核无遗留 |
| P2 上游发布 | BP2.6.0／Forge20.0.0正式发布，16资产、publication_verified=true；本机BP及四私有Skill安装成功，固定官方b7f429f |
| P3 四仓升级 | 全部成功，每仓退役35；Library/CRS/Stratus写60、Holdem写54，runtime_verified=true、driver ready、无遗留目录 |
| P4 四仓正式发布 | 四仓全部released且publication_verified=true，各3资产；版本、全部变化子包、stash和自定义内容独立核对通过 |

保留19.0.4原生身份恢复、双锁、指纹和旧记录保护；包含退役的目录迁移不使用身份恢复flag绕过pending release guard。没有操作交易、数据库、风控阈值或活库快照。

## 正式主版本与发布凭证

比较口径均为本轮CLI确认的正式旧基线到最终固定提交，未用候选或本地标签推定。

| 项目 | 正式旧版本 | 正式新版本 | 固定提交 | 标签 | 发布地址 | 核验资产 |
|---|---|---|---|---|---|---:|
| BridgePilot | 2.5.5 | 2.6.0 | `b7f429f972388f69f0057da347d944ae1076bf00` | `bp-v2.6.0` | [Release](https://github.com/freakybridge/BridgePilot/releases/tag/bp-v2.6.0) | 16 |
| BridgeLibrary | 0.74.5 | 0.74.6 | `ddd20824e20b92466c29f08386f2d9c5fa52ac0f` | `v0.74.6` | [Release](https://github.com/freakybridge/BridgeLibrary/releases/tag/v0.74.6) | 3 |
| CausisRiskSuite | 3.8.0 | 3.8.1 | `33d8e9a75714f27be7c558b6993426a351674e5d` | `v3.8.1` | [Release](http://git.causis.com.cn:8095/jinqiao.xue/causis_risk_suite/-/releases/v3.8.1) | 3 |
| HoldemTrainer | 0.49.4 | 0.49.5 | `f6b37a661de627afe7e07547aa39b3fb9d16bc35` | `v0.49.5` | [Release](https://github.com/freakybridge/HoldemTrainer/releases/tag/v0.49.5) | 3 |
| StratusAgent | 7.0.0 | 7.0.1 | `e92e90519f86d49c4d25dfedcf6f859a1cd58173` | `v7.0.1` | [Release](https://github.com/freakybridge/StratusAgent/releases/tag/v7.0.1) | 3 |

四个下游业务子包版本均无变化；主版本patch交付授权的骨架、Skill与公共工具输入。BP兼容增加受管迁移能力升minor；Forge默认项目Skill安装路径变更升major。

## 全部变化产品包

BP维护源中的六项变化如下；四项公开包的dogfood是同源投影，不重复算产品包。

| 包 | 正式旧版本 | 正式新版本 |
|---|---|---|
| `bridgeforge-cli` | 19.0.4 | 20.0.0 |
| `bridgeforge-core` | 11.4.4 | 11.5.0 |
| `bridgeforge-factory` | 0.13.1 | 0.14.0 |
| `bridgeforge-skeleton` | 0.8.0 | 0.9.0 |
| `bridgeforge-workflow` | 0.15.4 | 0.16.0 |
| `bridgepilot-install` | 0.10.1 | 0.10.2 |

CLI版本对应Forge安装接口；core对应编译模板与迁移登记；skeleton对应归属、历史和路径验证；workflow对应迁移事务；factory对应自身准备和dogfood迁移；BP install同步包内升级／清理Skill的读取路径。BP orchestrator及其他包未改变版本。

## 全部公共源码镜像包变化

| 项目 | 包 | 正式旧版本 | 正式新版本 |
|---|---|---|---|
| BridgePilot | `bridgeforge-cli` | 19.0.4 | 20.0.0 |
| BridgePilot | `bridgeforge-core` | 11.4.4 | 11.5.0 |
| BridgePilot | `bridgeforge-skeleton` | 0.8.0 | 0.9.0 |
| BridgePilot | `bridgeforge-workflow` | 0.15.4 | 0.16.0 |
| BridgeLibrary | `bridgeforge-cli` | 19.0.2 | 20.0.0 |
| BridgeLibrary | `bridgeforge-core` | 11.4.2 | 11.5.0 |
| BridgeLibrary | `bridgeforge-skeleton` | 0.8.0 | 0.9.0 |
| BridgeLibrary | `bridgeforge-workflow` | 0.15.2 | 0.16.0 |
| CausisRiskSuite | `bridgeforge-cli` | 19.0.2 | 20.0.0 |
| CausisRiskSuite | `bridgeforge-core` | 11.4.2 | 11.5.0 |
| CausisRiskSuite | `bridgeforge-skeleton` | 0.8.0 | 0.9.0 |
| CausisRiskSuite | `bridgeforge-workflow` | 0.15.2 | 0.16.0 |
| HoldemTrainer | `bridgeforge-cli` | 19.0.4 | 20.0.0 |
| HoldemTrainer | `bridgeforge-core` | 11.4.4 | 11.5.0 |
| HoldemTrainer | `bridgeforge-skeleton` | 0.8.0 | 0.9.0 |
| HoldemTrainer | `bridgeforge-workflow` | 0.15.4 | 0.16.0 |
| StratusAgent | `bridgeforge-cli` | 19.0.2 | 20.0.0 |
| StratusAgent | `bridgeforge-core` | 11.4.2 | 11.5.0 |
| StratusAgent | `bridgeforge-skeleton` | 0.8.0 | 0.9.0 |
| StratusAgent | `bridgeforge-workflow` | 0.15.2 | 0.16.0 |

版本取自两端固定提交的原生manifest；严格TOML无法整体解析的少量历史文件，只在其package/project区段读取显式name/version，不从其他字段推定。独立复核对版本表52个版本字面值逐项核对一致，业务manifest无变化。

## 工程、安装与运行证据

- 历史归属逐项核对正式 `e2fb7c0`／`e00683b` 的真实Git blob和当时manifest：35个稳定ID、36个不同哈希，覆盖四仓19.0.2/19.0.4旧正文；没有为本机或项目硬编码特殊名单。
- 受影响包：工厂33通过、3既有ignored；skeleton33通过；工作流原全包154通过、3失败来自正式e2早已退役的legacy `.codex/bin` 自动迁移断言。本轮只修正3项测试为现行阻断／空重定位写集及原字节保留，最终精确legacy组6项全过；其他154证据复用。不能表述为原157全包命令一次全绿。
- Hook新路径用例1项通过；15项template_sync事务先行验证也通过。manifest无漂移、factory-version20.0.0 healthy，read-only build-assets current/reused3/built0。
- 隔离安装fixture完成新装、35文件旧布局迁移、额外引用和personal Skill保留、无变化不重写、同名冲突和输入漂移零写入。最终输出及落盘result.json均passed；外层采集曾继承最后一个预期拒绝码2，已核对结构化收据，不把真实失败冒充通过。
- 主仓与四仓均用本机已安装Codex app-server的skills/list实测：每仓20个Forge Skill enabled/repo、路径唯一、旧根条目0、errors=[]。总repo Skill数为Library24／CRS20／Holdem20／Stratus21，包含既有自定义，不能误报总数均20。
- 所有正式计划以官方 `b7f429f972388f69f0057da347d944ae1076bf00` 和一次prepared产物生成；四仓gaps/blockers空，35个旧文件精准退役，写集不涉及业务源或原生版本文件。发布由各仓实际安装的CLI一次受管release完成。

独立复核修正了工厂新目标归属检查缺口及19项旧常量；最终实现、工程证据、119项隔离发布文件集和四仓实际发布验证均无遗留。

## 自定义、stash与工作区保留

四仓最终HEAD均等于上表正式提交、工作区干净。原stash逐项SHA与升级前完全一致：Library **4**、CRS **7**、Holdem **0**、Stratus **9**。正式旧新提交中，自定义Skill文件无差异：Library保留5文件／4个额外Skill，Stratus保留1文件／1个额外Skill；四仓旧 `.agents` 根均不存在。

**执行现场区别：** 正式BP发布来自干净官方副本 `C:/Users/bridg/.bridgepilot-source`。其118项文件SHA与已验证主仓候选一致，另一个精确索引hunk只增加本轮条目，119项实际集合与预期完全一致。主仓 `D:/Quant/BridgePilot` 仍基于e2fb7c0并保留未提交迁移、历史native-release最终补记、TODO-INDEX、code-reclaimer实验与文档、两份旧升级报告等现场；没有自动stash、覆盖、提交这些其他工作，也没有宣称主仓已快进发布提交。发布标签和正式资产保持指向上述代码提交。

本记录和唯一索引的最终补记属于本轮交付；普通文档同步不升版或重发产品。用户原有其他文档、实验及其索引hunk不混入本轮同步。

## 原始收据

统一根 `.runtime/tests/skill-layout/`：`relocations.json`、`scope.json`／`scope.patch`／`doc-index.patch`、`build-assets.log`、`publisher-build-assets.log`、`owned-package-tests.log`、`legacy-contract-tests-final.log`、`hook-skill-test.log`、`native-install-receipt.json`、`codex-discovery-factory.log`、`bp-release.json`、`bp-source-update.log`、`official-public-prepare.log`、`projects-official.json`、`downstream-before.json`、各仓 `*-plan.json`／`*-apply.json`／`*-discovery.log`／`*-release.json`、`after-update-checks.json`、`final-checks.json`、`version-summary.json`。Codex实际发现完整receipt路径在各discovery日志中。

记录依据为实际命令、固定提交及收据；不伪造SQLite任务或BP Run，不替代业务应用桌面／数据库人工验收。未自动归档或修改Memory。
