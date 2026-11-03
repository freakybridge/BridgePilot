# BP、BF 与 orchestrator 三产品版本治理

状态：2026-11-03 用户验收通过（lifecycle: completed；validation_status: passed）。用户通过 `summary accept` 确认本次交付符合需求，并通过 `git-sync release` 重新授权本仓库源码发布；发布实际结果以受管 Git 收据为准。用户级安装和真实下游更新不在本次范围。

## 决定与事实源

| 对象 | 唯一版本来源 | 本轮候选 |
|---|---|---|
| BP 主产品 | 根 VERSION | 新产品版本线 1.0.0 |
| BF | source/forge/cli/Cargo.toml | 继续 17.0.0 未发布候选，已发布的 16.x 历史不改写 |
| orchestrator | source/orchestrator/cli/Cargo.toml | 前序 0.13.0 候选 → 0.14.0，BP 版本输出与安装身份不兼容调整 |
| 内部安装包 | source/orchestrator/install/Cargo.toml | 前序 0.4.0 候选 → 0.5.0，安装协议变化；不增加第四条对外产品版本线 |

其他内部 Rust 包保留原生版本。移除根 workspace.package.version；Forge CLI 与 Hook 显式拥有包版本。Cargo.lock、生成的公共 workspace、分发清单及运行收据是派生物，不是第二份版本登记。BP 的 components.forge 为 source-reference，仅记录源码组合；当前 BP 便携包不携带或安装 BF。

## 任务与验收

| 优先级 | 内容 | 验证 |
|---|---|---|
| P0-1 | 拆开版本事实源、原生版本及锁文件 | 锁定 Cargo 检查；BP/orchestrator 候选变化不影响 Forge 两种源码投影 |
| P0-2 | bp versions/self-test 输出主版本及组件；factory 自检区分 BF 产品和内部包版本 | 真实二进制输出核对；help 登记新命令 |
| P0-3 | package/installation schema 2，十三项资产，组件与实际 core 身份一致 | 新包预览零写、重复安装、篡改阻断；schema1→schema2 旧包迁移 |
| P0-4 | 公共与工厂验证器分别匹配来源；工厂核验公共运行产物使用公共投影 | 正确来源通过、混合指纹失败；受管构建与零写工厂检查 |
| P1-1 | launcher protocol 2 读取两种 installation schema；旧 launcher 与 home claim 纳入恢复事务 | 真实旧0.13包升级后稳定入口 self-test；失败恢复及外部编辑保护 |
| P1-2 | BP／BF／orchestrator 发布范围和版本比较各归其主 | 文档、原生 manifest、安装包及命令输出一致；本轮不执行发布 |
| P2-1 | 同步安装文档、项目规则、CHANGELOG 和索引 | 独立复核及 Git diff 检查 |

## 版本与发布规则

- BF 独立变化只判 BF；orchestrator 独立变化只判 orchestrator。正式交付新组件组合、统一入口或安装行为时再判 BP 升版。
- 发布分析分别以上次该产品正式发布为基线，报告累计差异；Git 提交可同时包含三个组件，但不能因此视为三者均已发布。
- BP 产品发布记录精确 BP/BF/orchestrator 组合与来源。普通同步不升版，重复构建不升版，任何发布另须明确授权。
- 包格式、launcher 协议、私有档案 schema、能力和源码指纹独立校验，不用产品版本相等替代兼容性证明。

## 安装与恢复边界

旧 schema1 的 version 是 orchestrator 版本，迁移只与新 components.orchestrator 比较。新 schema2 的 version 是 BP，必须带可核实的组件身份与 protocol2 launcher。未来支持不同资产表时升级包格式，不再通过 BP 版本阈值猜结构。读取旧格式的版本阈值保留在 legacy 分支，不用于新格式。

保留旧 launcher 字节与 home claim，修改归属均纳入现有 Journal.changes；占用、外部编辑或提交前校验失败不得切换安装指针。恢复不覆盖外部修改。现代格式之间回退须精确匹配 retained 身份及真实档案读兼容；跨回旧 core 明确拒绝，保留历史包用于人工恢复评估。

## 传播与验证状态

本次含产品实现、自身配置和元文档；通用 Forge 实现在 source/forge，BP 特有版本组合留在 source/orchestrator 和项目专区，不下沉污染公共 Template。需要原生版本、CHANGELOG 和自身公共源码镜像同步。用户级共享安装与真实下游不在授权范围。

未执行的真实下游、用户级安装、业务运行、发布均标为未验证；不以历史收据认证当前候选，不创建版本登记或开发账本。

## 实施与验证结果（2026-11-03）

P0-1 至 P2-1 已实现。BP 的 `versions`／`self-test`、便携包、安装状态与源码更新回执分别报告主版本及组件；Forge 的活跃版本读取不再使用 BP 的 VERSION。通用 Forge 源码已通过受管准备同步到自身公共八包镜像，未刷新用户级共享安装。

| 实际检查 | 结果与证据（仓库根相对路径） |
|---|---|
| 锁定 workspace 编译及正式模式 BP 构建 | 最终 workspace-check-final.log、bp-release-build.log；均在 `.runtime/tests/version-governance/`，Cargo --locked，BP/orchestrator 为1.0.0/0.14.0 |
| `cargo test --locked ... -p bridgepilot-install --lib` | 13项通过；install-tests-final.log。含命名空间迁移、版本复用／降级保护、精确保留回退、launcher/claim外部编辑和恢复 |
| 独立测试 workspace 的 `version_governance::` | 3项通过；forge-version-tests.log。BP/orchestrator候选不改变两种Forge投影；两种正确指纹通过、混合指纹拒绝 |
| orchestrator 包身份测试及既有档案安装测试 | 各1项通过；package-tests.log、archive-install-tests.log。组件与实际core比对、预览零写、重复安装、真实fixture SQLite配置保留及篡改拒绝 |
| PowerShell版本解析及源码更新脚本 | 5项版本检查、既有10项源码更新检查通过；scripts/tests/version_governance.ps1、source-updater-tests-unrestricted.log。源码更新原受限运行因MSYS命名对象权限失败，原样在获准环境通过，未修改断言 |
| 工厂准备与故障路径 | 5个不同场景通过；factory-runtime-healthy-r2.log中的1项、factory-runtime-tests-final.log中明确完成的legacy/mirror两项、factory-runtime-tests-resumed.log中的2项。中断前未结束的两项只补跑它们，不把中断整轮宣称成功 |
| 实际受管Forge准备 | 原生Cargo引导 `build-assets --factory --project-root .` 成功，3个产物prepared、149文件事务写入；factory-prepare.log |
| 最终工厂及公共运行检查 | factory-check.json=current/read_only/self_test_executed=false，manifest-check.json=changed:false，factory-version.json=17.0.0/healthy，baseline-final.json=clean/runtime_verified:true |

所有上述日志位于 `.runtime/tests/version-governance/`。旧失败、权限失败与一轮因测试程序仍运行而发生的LNK1104链接占用日志保留；占用解除后重新验证，不将它们记作通过。

真实治理前产物 `.build/bridgepilot/release17-20261103/bp-0.13.0/` 已用于隔离升级。执行 `scripts/tests/version_governance_native.ps1 -Core <新构建bp.exe> -LegacyPackage <上述旧包>`，`native-smoke-latest.json` 记录：旧stable入口先以0.13.0正常启动，再迁移为BP1.0.0／orchestrator0.14.0；旧launcher protocol1换为2且home claim哈希一致；预览零写、组件篡改拒绝、重复安装字节不变、跨回旧reader拒绝，未创建私人档案绑定。新包保存在收据记录的 `.build/bridgepilot/version-governance/` 路径，测试home仅在本任务 `.runtime/tests/` 下。

独立只读Agent `version_governance_review` 已复核迁移、写前并发保护、实际版本入口、安装文档与旧工厂测试的事实源修正；无剩余阻断。其静态结论不代替以上运行结果。

## 证据边界与交付状态

| 范围 | 状态 |
|---|---|
| 源码、版本与原生锁 | 已实现并经必要构建和定向测试验证 |
| 产品传播 | 仅本仓库当前产品源及受管构建已核对；正式分发未执行 |
| dogfood | manifest／来源／运行基线通过，公共镜像已同步 |
| fixture | 包、档案、故障、恢复和真实旧产物隔离升级通过 |
| 真实下游 | 未执行、未验证 |
| runtime | 新BP／旧真实二进制在隔离home的smoke通过；实际用户级安装及业务运行未执行、未验证 |
| Git与发布 | HEAD仍为da0a00a33e9e835ba1ab367ebd0f38982c729ead；暂存区空，工作区保留前序及本轮修改。只读预检ready，BP1.0.0／BF17.0.0，remote_state=not_fetched不证明当前远端一致；无提交、推送或发布 |

以上 Git 状态为工程交付时的历史快照。后续授权与验收以下节为准，不能由验收推断发布成功。

## 用户验收与发布授权（2026-11-03）

用户明确调用 `summary accept`，验收 P0-1 至 P2-1 的三产品版本治理、安装兼容和来源校验交付；既有构建、定向测试、隔离旧产物升级、自身基线及独立复核证据继续有效。用户同时调用 `git-sync release`，恢复本仓库源码发布授权，版本组合为 BP1.0.0、BF17.0.0、orchestrator0.14.0，不再次升版。

相关总表未发现此主题的独立 TODO 编号，本次不补造编号或结算其他事项。新格式跨回旧 reader 的阻断属于已说明的恢复边界；真实用户安装、下游传播和业务运行仍未执行。发布包含此前已验收且留在同一工作区的 Template-only 退役及源码根整理，不扩大至下游提交、推送或安装。
