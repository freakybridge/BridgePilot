---
lifecycle: active
validation_status: in_progress
---

# Git 作者身份修正后的发布接续：问题交接

## 本轮修复进展（2026-11-11）

**追加的真实链路发现：** BP 2.5.4 已正式发布（`bp-v2.5.4`，`1f4f7ab791d5bd2ea0490001a4b81ae7a20222e2`，16 项资产核验，publication_verified=true），并通过官方 main fetch 核对后完成本机 BP／四 Skills 更新。随后 Holdem 的普通 project-sync 计划在零写入阶段拒绝 active commit guard（`holdem-update-plan-stderr.txt`）；不能安装恢复工具，故真实 Holdem 发布仍未接续。此前验证覆盖原生 release 恢复，遗漏了安装入口对同一 guard 的无条件阻断。

已补显式 `project-sync --recover-author-identity`：复用正向恢复证明，初始要求源码指纹等于旧候选；计划和双锁 apply 绑定 state/source/guard 原字节摘要及写集外源码，拒绝 retirement、配方请求、根 Cargo.toml 和全部 SourceIdentity.paths 写入。事务末尾复核且保留记录，由更新后的实装 CLI 真正执行 release 恢复。追加候选为 BF 19.0.4／workflow 0.15.4／core 11.4.4；独立复核未发现新增安全问题，受管构建和针对性验证进行中。下一次正式发布前先用候选公共 CLI 对真实旧 Holdem 场景生成零写入计划；该计划不是已安装或正式发布证据。

**追加验证已通过：** 19.0.4 已受管准备三工具并写入 20 项 dogfood，manifest 无漂移、factory-version healthy=true；`template_sync` 13 项通过，受影响 `identity_recovery` 6 项通过（361.49 秒）。真实 Holdem 候选零写入计划 `holdem-candidate-recovery-plan.json` 成功：1 safe＋19 risk，retired/gaps/blockers 均为空；全部 11 个原生版本归属路径和根 Cargo 不在写集中。证明精确重建的追加项恰为 Causis 专属 generic provider、user.name、user.email。独立复核核对了三个记录摘要与现场字节，以及 20 项写集和 excluded 的一致性，无遗留问题。正在发布补齐版本；正式 apply 和实装 CLI 接续仍待后续收据。

已在 Forge 维护源实现 `git-sync release --recover-author-identity`。旧 schema 1 记录不要求补写配置快照：在内存中从当前有效配置移除受限追加项，精确重建原 `config_sha256`，其余配置原字节顺序、重复项与摘要保护保持。允许项仅为非空 `user.name/user.email` 追加，以及不同服务主机的精确 HTTP(S) origin 下 `credential.provider=generic` 追加；不接受旧值替换／删除、通配或路径 origin、同主机、其他 provider/helper、Hook、签名、remote 等无法证明的变化。

恢复仅接纳 `blocked-before-commit`、无固定 commit/result、有原 anchor、HEAD／分支／实际 push 目标相同、driver 相同、产物摘要有效、版本事务已完成、有 before-commit guard 且 index 对 HEAD 为空的现场。工具归档原 state 和 guard，复核并发变化后原子保存新 anchor 与恢复证据；源码变化仍走既有 reprepare／版本适配器，不直接复用失效产物或删除记录。

真实配置分别核实：Holdem 本地有确认的姓名和邮箱；全局 `.gitconfig` 只有 Causis 服务的 generic provider。最终是否恰好为这三项追加，以实际 CLI 的旧哈希精确重建证据为准，不从本机配置备份推定安全。

工程收口候选：BF 19.0.2 → 19.0.3、workflow 0.15.2 → 0.15.3、core 11.4.2 → 11.4.3（内置 Skill 说明）。受管 build-assets 已成功准备三工具并写入 19 项；dogfood／manifest 检查无漂移，factory-version healthy=true。独立复核最终无遗留问题，归档并发和原生候选测试缺口已落实；原生发布针对性套件 **39 passed / 0 failed**，包含 GitHub／GitLab、legacy 恢复、配置安全矩阵、固定／未知提交拒绝、并发中断和候选复用（547.88 秒）。正在进入 BP 正式发布及 Holdem 更新接续；在真实收据返回前不宣称发布完成。

验证产物根：`.runtime/tests/author-identity-recovery/`。首轮 22 通过／17 fixture 初始化失败，原因是本轮漏传测试文档要求的编译身份，未进入对应发布逻辑；随后按 `scripts/tests/README.md` 从真实公共 CLI self-test 读取身份重编并重跑，日志为 `native-release-tests-verified.log`。未修改测试门禁或伪造身份。两份原有升级报告及索引已临时保存在该目录的 `preserved-docs/`，不混入修复发布，发布后原样恢复。

## 目标与当前决定

主任务 A 是对 BP 登记的四个下游执行正式 release。阻塞 B 是 HoldemTrainer 首次发布因缺少 Git 作者身份失败，按用户要求配置署名后，协调器又因完整 Git 配置哈希变化拒绝接续。

用户已明确要求四仓本地统一使用 `bridgexue <bridgexue2021@gmail.com>`，已经设置并核验。随后用户同意修复 B，包含必要验证、BP 修复版正式发布及受管更新 Holdem 工具，再继续 Holdem 发布；之后要求本对话“先不修”，将 B 拆到新对话。本对话没有修改修复源码。新对话先核对本记录与实际现场，再按用户粘贴提示词及已有范围继续，不重复询问已确认署名。

修复维护源是本仓 `source/forge/`，不能直接修改下游 `.codex/hooks/source/` 镜像。只解决 B，不扩展业务、删除接续记录、绕过 Git 安全检查或重发其他仓发布。

## 保存时的真实现场

- 正式开发根：`D:\Quant\BridgePilot`，分支 `main`，HEAD 为 `e00683bca1356316f04667d43ad9a12186379888`（BP 2.5.3／Forge 19.0.2）。交接时原有未提交内容为 `doc/README.md` 与两份未跟踪的下游升级报告；本次再增加本交接及索引，不把既有报告混入修复发布。
- Holdem：`D:\Quant\HoldemTrainer`，master，HEAD 仍为 `fff116838fc156f6130a6113cfa75d7e7c864067`；正式旧版由本次 source-input 确认为 0.49.3。当前 VERSION 与 CHANGELOG 保留了 **0.49.4 候选**，未提交、推送或发布，不可当作已发布版本。当前有原 10 项骨架差异及 2 项版本／日志准备差异，暂存区和 stash 均为空。
- BP 本机绑定仍由用户级 Skills 的 `bridgepilot-local.json` 定位；实际安装 home 为 `C:\Users\bridg\Documents\Codex\BridgePilot`，私有 workspace 为其配置指向的 `C:\Users\bridg\Documents\Codex\BridgePilotWorkspace`。不直接改 SQLite 或伪造 Run。

本批次控制证据根（本机绝对路径）：`D:\Quant\BridgePilot\.runtime\tests\downstream-release-ebf783d1383b4fccb471b70f70e5a155\`。下述文件不是旧历史分析，均来自本次实际调用：

- `holdemtrainer-release-identity-failed.json`：第一次在 source-push 阶段因 Author identity unknown 失败，无新提交／push／平台写入；CLI 报告暂存等自动操作回滚，但实际原生候选 VERSION/CHANGELOG 仍保留，不能据通用错误字符串假定全部版本编辑已撤销。
- `holdemtrainer-release.json`：配置身份后接续，preflight/conflict，理由 `release branch, push target or Git configuration changed`；last_completed_stage 为 blocked-before-commit。
- `identity-configured.json` 与各仓 `*-git-config-before`：用户确认后的本地身份及原本地配置备份。备份不是允许手工覆盖接续状态的凭据。

Holdem 实际接续文件：`D:\Quant\HoldemTrainer\.runtime\bridgeforge-codex\native-release\current.json`。核对时 status=`blocked-before-commit`、commit/result=null、候选 0.49.4，fingerprint=`a923a8bc3b13e6d86e917c06e16009b9fe14ed3511c15b3a45be1dfbccea8c91`。旧锚点 HEAD、分支、origin/master 和 push URL 均仍匹配，config_sha256 为 `sha256:bd44d25a06c7eb96e99b4c1f2a11a727c57ed72d16f6ebdb6000a6f509621b32`。这些值是定位证据，不能据此直接改写文件。

## 已确认原因与排除项

相关维护源：

- `source/forge/workflow/src/native_release.rs`：Anchor::same_target、anchor、fingerprint、require_repreparable 和 run_inner。
- `source/forge/workflow/src/native_release_source.rs`：recover、can_replace_obsolete_attempt 及源码版本事务。
- `source/forge/workflow/src/git_command.rs`：受管参数入口；现版本没有 abandon/reset/reanchor 恢复入口。
- 针对性回归入口：`scripts/tests/unit/native_release.rs`、`native_release_gitlab.rs`。

Anchor 对完整 `git config --null --list` 结果做哈希，并把该哈希作为 same_target 的一部分。作者姓名／邮箱加入后，仓库和实际推送目标虽然不变，完整哈希仍改变。源码 fingerprint 不含 Git 配置，所以旧接续被复用后再次拒绝；真正源码变动触发 reprepare 时，又要求完整旧 anchor 等于当前 anchor，同样无法恢复。现有工具没有可安全使用的受管路径，继续盲调入口无益。

还存在一项必须纳入真实场景的环境事实：并发 CRS 发布时，Git Credential Manager 于 2026-11-11 19:46:07（Asia/Shanghai）新建 `C:\Users\bridg\.gitconfig`，仅增加 Causis Git 服务的 `credential.provider=generic`。它也进入全部有效配置哈希。不能未经证明地认定 Holdem 当前与旧锚点仅有两个本地身份字段差异，更不能一概忽略其他 Git 配置。对“仅身份修正”的授权与全局凭据副作用要分别取证、限定恢复条件。

Library 因同一全局 provider 写入在只读适配阶段触发检查点漂移，但它当时没有 current/source 事务，源码清单、HEAD、index、stash 未变。环境稳定后同入口安全恢复已成功。该情况不代表 Holdem 已有旧锚点也可直接重试。

## 修复边界与验收条件

先调查实际旧 schema 状态及原生输入，不预设具体实现。需要的是可审计的原生恢复能力，不是取消配置保护：

1. 缺作者身份导致提交前失败，用户确认配置身份后，能够在不删除／手改 state 的情况下安全接续，覆盖本机现存 legacy 记录；不能只让新生成记录可恢复，而继续卡住真实旧记录。
2. 核实 HEAD、index、源码 fingerprint、driver、分支与真实 push 目标、commit guard，保留原 attempt 与候选版本，不重复升版、提交或上传。
3. 身份之外的远端、分支、Hook、签名等关键配置漂移继续阻断；已经固定提交／push 结果未知时不能借此重设锚点。旧记录若缺足够证据，不能默认认证安全。
4. 明确验证如何处理本次无关服务 provider 的追加，禁止为这台机器或该仓硬编码邮箱、主机、服务地址或名单。
5. 覆盖正常恢复、恶意／非身份配置变化、固定 commit 后拒绝、异常中断与候选复用；针对性测试及独立复核通过后，按原生版本规则、CHANGELOG、dogfood 和受管构建收口。

不能通过删除／移动接续记录、手改 anchor/hash、伪造源码差异或回退身份来骗过保护；不使用手工 Git 提交／push、`--no-verify`、强推或旧冻结工具替代受管流程。不顺带修改 Holdem 业务或调用数据库。

## 返回主任务的路径

已成功的仓不能重发：

| 项目 | 正式结果 | 固定提交 | 资产 |
|---|---|---|---|
| CausisRiskSuite | 3.7.1 → 3.8.0，v3.8.0，publication_verified=true | `9b7c17830e284f536bd22eb1bb422d3ad7703988` | 3 |
| BridgeLibrary | 0.74.4 → 0.74.5，v0.74.5，publication_verified=true，既有私库 | `dd31f88de365250e0b4bdfe0cd93410c0fb419e6` | 3 |
| StratusAgent | 6.0.1 → 7.0.0，v7.0.0，publication_verified=true | `03e35576b7635c8f9e542c2997715c03982fe38f` | 3 |

对应主控 `causisrisksuite-release.json`、`bridgepersonalassist-release.json` 均已保存。两仓工作区干净，原 7／4 个 stash 保留。完整子包比较必须基于本次 CLI 的正式基线与最终 commit，不能用本地标签或候选代替。

**最终读回交接时，Stratus 已成功结束。** 原生收据于本机 20:09 写入，`status=released`、`publication_verified=true`，版本 7.0.0、固定提交如上、3 个资产通过；不再存在本次需要等待或重启的发布。正式旧基线由 source-input 确认为 6.0.1／`3bc0b12f21a1acdfb92added36fc532c72b8a5c3`。完整子包明细尚需从正式旧、新 commit 提取，不能复用预审候选表；CLI 本次还调整了 Matching、Strategy SDK、TradingRoom 和六个策略模板。新对话不要重发 Stratus，只处理 B 及最后的批量汇总。

完成 B 后更新本交接；按已授权范围正式发布 BP 修复版，再以官方固定来源受管更新 **HoldemTrainer 子集**的必要工具，保留其业务差异、候选、stash 和恢复证据。不要顺便更新／发布其他项目。随后仍只使用 Holdem 实际安装的 `git-sync release --publication-required --message-file <本次消息>` 接续，以 released 且 publication_verified=true 作为成功标准。

返回主任务对话或新对话，读取本交接，核对当前代码与证据，再汇总四仓主程序及全部变化子包的正式旧／新版本、提交、标签、URL、资产核验和保留项。本次交接未提交推送，也未实施修复或创建新聊天。
