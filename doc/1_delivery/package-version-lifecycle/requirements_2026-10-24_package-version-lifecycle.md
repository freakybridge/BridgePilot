---
lifecycle: completed
validation_status: verified
---

# 子包开发收口与累计版本公告

关联 TODO：TODO-025（原需求）。布局：flat。设计原因见[架构说明](../../0_architecture/design/package-version-lifecycle.md)，详细判级与证据规则以[版本生命周期合同](../../../source/forge/templates/skills/git-sync/references/schema4.md)为唯一事实源。

## 原始需求与文档阶段授权

用户要求 release 公告所有变化包，并为公共骨架补充子包版本配置、格式及升级规范；参考 StratusAgent，不复制其业务配置或版本数值。

已确认决定：

1. 子包登记采用规则约束，不新增漏登记包的自动扫描硬闸，保留现有归属检查。
2. 0.x 不兼容变化升 minor；程序依据接口基线、非空核心/集成检查、当前有效通过证据和本次兼容性自动决定进入 1.0.0，不逐包请求人工批准。
3. 用户原话：“采用这个模型：develop 完成且验证通过后升级受影响的子包版本，并自动判断是否进入 1.0.0；release 只升级主程序版本，公告自上次发布以来所有子包的版本变化。”
4. 用户原话：“选 A，未完成 develop 收口的子包阻止 release，提示完成验证和升版后再发布。”不在 release 自动补跑开发。
5. 用户要求精简 AGENTS 并检查冲突，已展示五条精简红线。
6. 本轮授权原话：“先改所有的文档吧”。本仓库现行文档、Skill、自身公共区与已有镜像、必要文档分发元数据在范围内；代码实施、真实下游、提交推送发布未获本轮授权。

此前“主程序和子包都在 release 升版”及“用户决定进入 1.0.0”的方案已被替代。历史验收记录保留原貌，不沿用其通过结论证明新行为。

## 文档阶段范围与传播四问

| 项目 | 约定 |
|---|---|
| 目标 | develop 管子包版本，release 管主程序版本及累计公告 |
| 层级 | 通用产品层，进入 templates 与共享 skills |
| 工厂专属 | 自身收口与验证要求，不下沉工厂测试 |
| 版本与 CHANGELOG | 正式发布时按产品变化处理；本轮不升 VERSION、不写已发布 CHANGELOG |
| dogfood | 同步公共区与已存在的 git-sync 镜像，不安装用户级 Skills |
| 不做 | Rust/CLI/测试代码、机器 schema 实现、业务版本/锁、真实迁移、下游写入、提交推送发布 |

## 规模与预算

完整功能为 L 级，跨开发入口、发布事务、证据和历史迁移。此前 4 小时 / 120,000 token / 1 名 review-auditor / 2 轮验证只是候选实施预算，尚未获代码实施授权。

本轮文档阶段按 M 级控制：45 分钟、约 20,000 新增 token、最多 1 名只读 review-auditor、最多 2 轮文档验收（含修正复核）。成本按本需求累计记录，不在后续实施时清零；token 未实测。缺少匹配当前模型/速度的周额度系数，不用异配置历史样本虚构占比，不启动额外校准任务。预计超预算或需要改代码、跨项目写入时停止相关动作。

## 文档阶段开始时的事实

- 当前公共条款笼统把业务版本归根 VERSION；旧 Skills 声称只有 release 升版且开发记录不阻断 release，须同步调整。
- 当前代码在 release 同时写主程序及子包版本，breaking 一律要求 major；新的子包开发收口、自动稳定及 release 硬闸均未实现。
- StratusAgent 当前登记 50 个包，抽查 types 与 strategy-sdk 存在自身原生版本；仅作只读参考，不宣称全部包或业务验证通过。
- 本仓库已有其他任务的运行时/Hook 及文档改动，本轮保留，不归为本轮成果。

## 业务规则与数据映射

| 输入/对象 | 输出/职责 |
|---|---|
| 稳定 id、唯一版本来源、实际包输入 | 子包身份与判级依据 |
| 上次有效包收口、当前代码/环境/证据 | 本次版本与有效收口；无变化或重复收口不重升 |
| 接口基线、稳定检查、本次兼容性 | 自动 1.0.0 或继续 0.x 的理由 |
| 最后正式发布、当前已收口包版本 | 主程序累计判级与全部变化包的旧新版本公告 |
| 收口后新改动或失效证据 | release 停止并说明需完成的验证和升版 |

机器入口和数据格式现由[版本生命周期合同](../../../source/forge/templates/skills/git-sync/references/schema4.md)维护。复用 Rust/Python 适配边界；特殊原生版本不强制转换。版本、相关依赖/锁、收口记录及最终产物须一致。

## 拟修改与顺序

1. 精简 templates/AGENTS.md 并同步根公共区，保留项目专区和既有变更。
2. 修改 develop、git-sync、骨架接入说明；稳定规则只保留一个事实源，明确文档先行与能力缺失边界。
3. 同步 README、工厂收口、操作指南及用户结果合同；旧 schema4 接入文档加历史适用边界。
4. 新增架构说明与本卡，登记 TODO-025 和索引；同步已有 git-sync 镜像及受管派生清单。
5. 文档一致性核验与独立冲突复核，记录实际结果。

## 验收与风险

本轮文档验收：公共区/镜像一致、相关现行规则无无条件冲突、引用可解析、索引和 TODO 合法；不新增虚构接口或运行时通过声明，不覆盖既有改动。

代码验收范围（已获下述实施授权，结果另记）：正常/失败/重复收口、0.x breaking/minor、自动 1.0.0 及缺失/过期证据、未收口阻断 release、多次 develop 累计公告、release 不二次升包、依赖/锁与产物一致、历史衔接、身份生命周期和事务回滚。真实下游与自然 runtime 独立取证。

迁移建议保留可信的最后正式发布及已有版本，未发布改动需真实收口，不能以当前 HEAD 跳过变化；具体方案在代码阶段以 plan 和夹具证明，本轮不执行。程序判定达到已配置标准，不证明任意代码兼容或绝无缺陷；0.x minor 可能 breaking，仍保留不兼容授权。

稳定检查不满足可继续 0.x，但普通开发必需验证失败不得收口。新收口不等于 prepared，不恢复全量回归、用户验收或开发 JSON 的普遍门槛。能力不足时不手工改版本或伪造收据。

## 实施与验证记录

### 代码实施授权（2026-10-24）

用户已明确授权开始 TODO-025 开发并验证，预算为 4 小时、120,000 token、1 名独立审计 Agent、2 轮验证。以下历史文档阶段的“未授权”仅描述当时状态。当前实施包括受管 Rust/CLI、机器合同、必要测试、Skill 和 dogfood 同步；不提交、推送、发布或修改真实下游。主对话实施，独立 Agent 审计；保留开始时已有的运行时、Hook 和文档改动。当前实现与工程验证已完成，等待用户验收。

- 2026-10-24：本轮只获文档授权；代码、新机器合同、真实下游和发布未实施。文档核验结果随后补充，不将整项功能记为完成。

### 文档阶段结果（2026-10-24）

本轮更新 23 个 Markdown 文件（含自身镜像与两份新增文档），由受管 manifest 刷新 3 份发生内容变化的派生 JSON；未修改业务 VERSION、CHANGELOG、配置 schema、锁文件或 Rust/测试代码。与本阶段开始的文件哈希快照比较，范围外文件内容未变；已有其他任务改动保留。

| 核验 | 实际结果 |
|---|---|
| `bridgeforge manifest --root . --check` | 通过，changed=false |
| `bridgeforge check todo --root .` | 通过，无 errors；新增 TODO-025，未改写已有条目 |
| `bridgeforge check skill-metadata --root .` | 修正后通过，无 issues；git-sync 入口 119 行，满足 120 行上限 |
| `bridgeforge check project-structure --root .` | 通过，无 errors；存量归档及排除目录 advisory 不在本轮处理 |
| `bridgeforge check baseline --root . --skip-generated-runtime` | 通过，state=clean；明确跳过生成运行时，不表示运行时或业务功能通过 |
| Markdown 链接及镜像核对 | 258 个本地文件链接目标存在；公共 AGENTS 与四份 git-sync 镜像一致；未认证所有历史链接的章节锚点 |
| `git diff --check` | 通过，无空白错误 |
| 独立复核 | review-auditor 未发现 P1/P2 文档冲突或授权扩张；确认新旧边界、0.x 规则、普通同步豁免及失败语义一致 |

第一轮发现 git-sync 入口超出行数上限，精简标题和重复说明后第二轮通过。受保护 .codex 写入及基线临时属性沙箱曾因权限失败，经相同范围提权重试完成，未修改 ACL 或更换校验规则。

文档阶段结束时，本卡为 lifecycle=active、validation_status=not_started；上述通过仅限当时文档，不代表 TODO-025 整体完成或用户已验收。当前状态以顶部字段及代码实施记录为准。

### 代码实现与工程验证（2026-10-24，已完成）

本阶段新增受管 `develop --preview/--apply/--recover`，在 schema 4 内登记逐包检查与稳定合同，维护追加式包收口历史。release 改为只写主程序及其原生载体，校验有效收口并累计公告全部变化包；原生 Rust/Python 适配器继续处理版本、依赖要求与锁。配置迁移保持原正式发布基线，不认证未发布改动。

事务使用共享同步锁、持久日志与受保护回滚；检查失败不留下已认证版本，外部并发修改保留。纯验证失效、共享原生锁文件变化和继承版本的机械物化只重新验证，不重复升版。中断事务须先恢复才可同步，正常未收口工作仍可普通同步。

验证按两阶段组织：第一阶段实现与定向检查，第二阶段审计修正、相关回归及最终 CLI 集成；失败仅针对对应原因修正复测，没有运行整个 workspace 或整个 factory fixture 集。1 名独立审计 Agent 复用至最终复核，没有另开 Agent。初次 schema4 回归为 46 通过 / 14 失败，旧用例迁移后，扩展发布回归为 201 通过 / 6 失败 / 4 忽略；6 个失败分别定位为夹具漏保留 release_model 与原生继承版本机械改写缺收口记录，修正后的结果继续记录如下。

| 检查 | 已取得的证据 |
|---|---|
| 开发收口定向集 | `cargo test --locked --config scripts/tests/factory-cargo.toml --manifest-path .codex/hooks/Cargo.toml -p bridgeforge-core package_development_ -- --test-threads=6 --nocapture`：22 通过、0 失败；覆盖重复/累计、自动稳定及失败降级、回滚/恢复、锁、源/输入/产物漂移、依赖传播、超过 3 包公告 |
| 最终关键回归 | 在上述受管 workspace 执行 `-p bridgeforge-core --lib -- package_development_ schema4_both_languages_lifecycle_changes_keep_identity_and_tombstones independent_release_rates_each_final_package_and_is_idempotent explicit_release_rejected_hook_restores_native_versions_log_and_index --test-threads=6 --nocapture`：25 通过、0 失败，581.42 秒；包含上述 22 项及跨语言包生命周期、独立版本幂等和发布 Hook 拒绝后的回滚 |
| CLI 能力与写入参数 | 同一受管 workspace 的 `-p bridgeforge-cli development_capabilities_and_mutation_flags_are_explicit`：1 通过；实际 CLI self-test 声明三个新增能力 |
| 最终真实 CLI 集成 | `cargo test --locked --manifest-path scripts/tests/Cargo.toml --lib -- real_cli_explicit_release_ real_cli_python_lock_preview_release_rollback_and_repeat --test-threads=3 --nocapture`：4 通过、0 失败，203.30 秒；覆盖既有策略、Rust 独立包与新增包、Python 原生锁、失败回滚和重复发布，使用隔离临时仓库及本地 bare 远端 |
| 最终受管构建 | 根 AGENTS 指定的 `cargo run --locked --release --manifest-path .codex/hooks/Cargo.toml --bin bridgeforge -- build-assets --factory --project-root .`：prepared=2、written_files=4、self_test_executed=true；随后的 `--check` 为 current、reused=2、built=0 |
| 工厂与派生一致性 | manifest `--check` changed=false；完整 baseline（含生成运行时）state=clean；factory-version healthy=true；TODO、Skill metadata、项目结构检查 exit=0 |
| 自身版本收口 | 实际 CLI `develop --preview --root .`：nothing-to-close，affected_packages=[]，writes=[]；工厂没有注册业务子包，根 VERSION 保持 5.0.1 |
| 独立审计 | 检出并修复原包级必需证据、额外检查输入指纹、真实接口变化根、跨稳定阶段判级、BREAKING 标记及中断事务同步保护；随后复核继承版本机械收口，未发现遗留实质问题。审计为源码/测试设计复核，不冒充执行测试 |

Windows 定向测试重链接曾因上一测试 EXE 仍在运行出现 LNK1104，等待其退出后同命令通过；受管构建曾因修复后 manifest 过期停止，同步清单后按原入口通过。均未变更 ACL、绕过受管入口或降低检查。构建仍有原有 release.rs 三个 dead_code warning。

4 项忽略测试为真实 uv/Poetry/PDM 提供器的可选原生锁验证，本轮未执行；Python CLI 夹具的锁字节一致性不等于这些第三方提供器的实际运行认证。真实下游、用户级 Skill 安装、正式发布和自然桌面 Hook 均未验证或执行。已有工作区修改保留，不计作本轮实现成果。

后续相关回归 68 项中 67 通过，原 6 项发布失败全部通过；剩余 1 项是旧回滚用例在 develop 之前取快照，错误要求 release 回滚已完成的子包升版，已将快照移至真实开发收口之后，并保留 manifest、锁、配置、CHANGELOG、index 与用户源码的恢复断言。真实 Python CLI 链路已通过；Rust 新增包场景另定位到历史归属进入内容指纹，导致首次正式发布后重复 release 错判失效。内容指纹改用当前归属，历史归属仍参与影响检测；独立审计复核无遗留问题，最终 25 项关键回归和 4 项真实 CLI 集成全部通过。相关源码未变的既有通过证据继续复用，不把多次运行的重叠用例累加为独立覆盖数。

本阶段约 90 分钟（03:22–04:52 UTC），在 4 小时额度内；使用 1 名审计 Agent、2 个验证阶段，阶段内包含上述失败修正复测。token 未实测，无法报告精确消耗或证明额度余量。测试后的清理仅移除临时失败夹具保留逻辑及指定失败夹具目录，恢复原清理行为，未改变产品源码或测试断言。

## 交接

工程验证完成，等待用户验收；TODO-025 保持进行中，未提交、推送、发布或修改真实下游。试用入口与配置示例见[版本生命周期合同](../../../source/forge/templates/skills/git-sync/references/schema4.md#6-机器入口与配置)。本仓库实际 `develop --preview --root .` 为无子包变化；逐包升版、稳定转换与发布公告已在隔离夹具验证，真实项目接入不在本次授权内。

## 用户验收与发布授权（2026-10-24）

用户明确调用 `$summary accept` 和 `$git-sync release`，确认 TODO-025 在约定范围内符合需求，并单独授权受管发布。TODO-025 结算为已完成；复用本卡最终 25 项关键回归、4 项真实 CLI 集成、受管构建及独立审计证据，不为验收重新运行工程测试。上文待验收和未获发布授权均为阶段记录，以本段为准；正式版本、提交与推送结果以受管 release 收据为准，不提前宣称成功。

本次经验补充：开发收口与正式发布必须保留两个比较基线；内容认证使用当前包归属，历史归属只参与影响检测，否则新增子包首次发布后会误判原包证据失效。共享锁变化、原生继承版本机械物化和纯证据失效应重新验证而不重复升版。release 回滚测试应在已完成 develop 后取快照，避免错误撤销此前独立完成的子包收口。这些边界已由本卡记录的回归用例覆盖。

真实下游、自然桌面 Hook、用户级 Skill 安装与可选 uv/Poetry/PDM 原生提供器仍未验证或执行，不因用户验收改为通过。无新发现的未解决独立问题，不新增 TODO；其余 TODO 保持原状。有界核对既有 schema4 发布经验后，现行 AGENTS、版本合同和新增事务检查已覆盖相关约束，不新增 Rule/Hook/AGENTS/Memory 建议。本主题可列归档候选，但不自动移动；需要整理时另行调用 `$archive-scan`。
