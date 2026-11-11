---
lifecycle: active
validation_status: in_progress
---

# Forge 项目 Skill 迁回 .codex/skills：实施与批量发布

## 目标、授权与范围

用户在当前对话明确批准：完成 Forge 自带项目 Skill 从 `.agents/skills/` 到 `.codex/skills/` 的受管迁移，正式发布 Forge/BP，受管更新四个登记下游，再分别完成正式发布，全部完成后统一汇总。随后显式启动 autopilot，整体原生 Goal 预算为 **10,000,000 token**，包含实施、必要独立复核、验证、合理返工与收尾；不因阶段或 Agent 切换重置。时间估计3–5小时，不设时间硬停。原生预算是否归集全部子 Agent 用量尚未实测；不把主线程读数冒充已验证的整包计量。当前无适用周额度校准记录，不虚构周额度百分比；该信息不阻断已经明确授权的执行。

范围：20 个 Forge 项目 Skill、35 个文件及其分发映射、历史归属、事务迁移、模板、dogfood、BP 私有调用路径、必要测试和现行文档。维护源仍为 `source/forge/templates/skills/`。只迁移可证明归属 Forge 的精确文件；保留自定义 Skill、额外文件、本地修改、其他现有改动与 stash。无关 code-reclaimer 实验及历史未提交文档不得混入发布。

现有正式基线：BP 2.5.5／Forge 19.0.4，源码 `e2fb7c0b7a2fe07a4b98ca9d2d5580c5414d117b`。四仓为 CausisRiskSuite 3.8.0、BridgeLibrary 0.74.5、StratusAgent 7.0.0、HoldemTrainer 0.49.4；本轮正式旧基线仍由各仓原生 CLI 在发布时核实，不能仅据当前版本推定。

传播四问：产品层及配套文档；通用能力；需要原生版本与 CHANGELOG；必须同步自身 dogfood。发布沿已确认的 BP 组合发布交付 Forge，再固定官方来源给下游受管安装；各产品版本分别报告。

## 实施与验收

| 阶段 | 完成条件 | 当前状态 |
|---|---|---|
| P0 归属与迁移 | 稳定 asset id；旧、新路径均进入精确计划、指纹、备份与回滚；冲突阻断且不覆盖自定义内容 | 已实现；35条精确迁移、36条真实Git哈希证明 |
| P1 模板、dogfood、入口 | 新默认路径一致；已知旧 Forge 文件受管退役，额外内容保留；公共源码投影与现行调用同步 | 已完成：三工具准备、写入56、退役35，旧.agents已移除 |
| P1 验证 | 旧／新／两处并存／本地修改／漂移／回滚测试；本机 Codex 实际发现 20 项 repo Skill 且无重复；独立复核无遗留 | 实际20项repo发现、隔离安装升级、受影响包与Hook验证通过；最终复核中 |
| P2 Forge/BP 正式发布 | 原生版本、锁、CHANGELOG 一致；released 且 publication_verified=true | 准备隔离发布文件集 |
| P3 四仓升级 | 官方固定来源与 prepared 收据；计划/apply 成功，业务差异及 stash 保留，路径发现核验 | 未开始 |
| P4 四仓发布与汇总 | 各仓实装受管 release 成功；完整正式旧新版本、全部变化子包、提交、标签、资产、保留项 | 未开始 |

保留 Forge 19.0.4 的身份恢复、原生发布双锁、指纹及旧记录保护。本次目录迁移包含旧资产退役，不得借身份恢复 flag 绕过 pending release 保护。

## 现场与验证记录

启动时正式 Git 根为 `D:/Quant/BridgePilot`、main。现有未提交内容包括 TODO-INDEX、native-release 最终交接补记、doc 索引、code-reclaimer 文档与实验脚本、两份下游升级报告；均不属于本轮迁移源码。发布前隔离并核对原字节，若有并发写入则使用合适隔离副本，不移动或覆盖正在使用的其他主题文件。

已建立1000万token原生Goal。历史证明逐项核对正式 e2fb7c0／e00683b 的真实Git blob与当时manifest，35个稳定ID、36个不同哈希（包含前三仓旧git-sync正文）。源码和生成规则已实现迁移；15项template_sync测试通过、工厂自定义目标保护测试通过。独立复核指出的工厂新目标归属缺口和19项旧常量已修正并复核，无新增明确风险；旧legacy最终验证与客户端静默检查保留。

候选版本：Forge20.0.0（默认项目安装路径变更）、core11.5.0、skeleton0.9.0、workflow0.16.0、factory0.14.0、BP install0.10.2；BP主版本由正式原生适配器选择。受管三工具及dogfood准备成功：written56、retired35、retained_directories=[]，.agents已不存在、.codex/skills有20目录。manifest无漂移、factory-version20.0.0 healthy，read-only build-assets current/reused3/built0。

验证：本机Codex app-server的skills/list实测20项Forge Skill均enabled/repo且新路径唯一，记录codex-discovery-factory.log及其receipt路径；隔离native_release_install.ps1完成新装、35旧文件迁移、额外文件与personal Skill保留、无变化不重写、未知同名阻断、指纹漂移零写入，native-install-receipt.json为passed。其外层采集曾误带最后一个预期拒绝码2；已核对输出及落盘result.json一致，未将真实脚本失败算通过。

受影响包：工厂33通过/3既有ignored，skeleton33通过；工作流全包154通过/3过期断言失败，正式e2源码已证明自动.codex/bin迁移早已退役。本轮只把3项旧测试改为当前阻断、空重定位写集及原字节保留合同，精确legacy组6项重测通过，生产实现不变；其余154项证据复用，合计工作流157项均有通过证据。Hook新路径用例1项通过。原始日志owned-package-tests.log、legacy-contract-tests-final.log、hook-skill-test.log保留。

四仓升级前审计：各35个旧文件均为已知哈希，新目标冲突0，工作区均干净；原stash为Library4/CRS7/Holdem0/Stratus9，完整条目SHA保存在downstream-before.json。尚未写入真实下游或正式发布本轮版本。

发布隔离：复用现有干净官方main副本 `C:/Users/bridg/.bridgepilot-source`，已从e00683b只快进至e2fb7c0。后续只转移并核验本轮指定改动，在该副本完成正式发布；主仓其他进行中文档、实验及原有改动不提交或移动。原始验证产物统一在 `.runtime/tests/skill-layout/`。
