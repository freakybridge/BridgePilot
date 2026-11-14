---
lifecycle: completed
validation_status: passed
---

# 旧测试链路激进退役验收

本文件分别保留 2026-11-03 的选择性退役与 [2026-11-15 全部测试清理验收](#all-tests-retired-2026-11-15)；两次授权和统计独立，后一次明确授权覆盖本仓库剩余测试代码。

## 用户验收2026-11-03

2026-11-03，用户要求分析 `scripts/tests/` 中可永久退役的测试，随后明确采用“清理更激进，非必要不保留”的原则，并在清单迁移任务结束后授权实施。交付后用户显式调用 `summary accept`，确认本轮交付符合需求；同时调用 `git-sync release`，另行授权本仓库本轮指定改动的完整发布。

本节 2026-11-03 的验收范围为旧配置化 develop/release、开发收口账本、旧 cleanup 成功流程及其独占辅助代码、fixture 和基准退役，保留当前功能与安全回归。当时关于整体删除测试目录的问答仅解释可行性和代价，没有授权删除剩余目录或停用全部自动化测试；2026-11-15 的新授权与实际删除另见文末。

本主题此前没有正式需求卡或唯一匹配的 TODO；本次直接保存实际验收决定，不补造需求访谈、编号，不结算其它主题的 TODO。发布状态与工程验证、用户验收分别管理，最终发布结果以本轮受管 CLI 收据为准。

## 实施范围与数量

实施基线为 `2b2da1c`，开工时工作区干净。`scripts/tests/` 实际删除 29 个文件、修改 10 个文件，新增 1 个必要的当前模板初始化辅助文件；总文本删除 17,059 行、增加 299 行，净减少 16,760 行。其中 Rust／PowerShell 源码净减少 14,590 行，行数包括注释和空行。上述数字只计算测试目录，不混入本次验收文档、源码条件注册、dogfood 和清单哈希传播。

- 整组移除旧 schema4／开发账本测试及其子用例、旧配置化 CLI 发布测试和旧工厂同步套件。
- 删除无活动引用的旧 Hook 场景、旧 cleanup 成功流程、旧产品版本映射 fixture、三个旧发布性能脚本及其空目录。
- 删除旧 schema4 Agent／迁移初始化和发布计划辅助文件；从混合测试中移除旧发布分支和只约束历史文档措辞的断言。
- 保留普通同步、原生 `git-sync release`、Memory、布局、安装、缓存、事务与用户文件保护检查，以及仍有用途的原生格式解析和输入所有权检查。
- 新增 `scripts/tests/support/current_template.rs`，使用当前模板和真实公共运行时初始化隔离项目，不重建旧版本配置或收口账本。
- 公共生产 Rust 源码仅解除条件测试注册；同步五个 lifecycle dogfood 镜像及对应来源哈希，不修改生产行为。未调整产品版本，未修改用户级安装或真实下游。

## 验证与证据

本机日志和结构化统计位于 `.runtime/tests/test-retirement/`，属于本机测试证据，不是版本登记、开发账本或发布证明。总表为 `result.json`。共 121 项唯一的定向用例分批通过，修复后的重测按同一用例计数，不重复累计：

| 范围 | 通过数 | 证据与复用边界 |
|---|---:|---|
| Git 同步、原生发布替身与模板事务 | 53 | `workflow-verified.log`；整组最终通过 |
| CLI 与相关 Memory 命令 | 25 | `cli-tests-authorized.log`；显式导入及当前工厂转发 fixture 适配后通过 |
| 生命周期、解析与输入所有权 | 22 | `lifecycle-tests.log`；保留的检查通过 |
| 集成、版本治理、清单与投影 | 7 | `integration-verified.log` 中五项未受后续修改影响的通过结果，加 `layout-final.log` 中两项布局最终通过结果 |
| 工厂准备、缓存与安装保护 | 14 | `factory-verified.log` 中十二项未受后续修改影响的通过结果，加 `factory-corrections.log` 中两项最终通过结果 |

两套测试入口已编译成功：

```powershell
cargo test --locked --config scripts/tests/factory-cargo.toml --manifest-path Cargo.toml --target-dir .build/bridgeforge/test-target --workspace --no-run
cargo test --locked --manifest-path scripts/tests/Cargo.toml --target-dir .build/bridgeforge/integration-target --no-run
```

受管准备成功重建 CLI、Hook 和工厂三个本机产物并同步 dogfood。下列检查分别通过，收据为 `manifest-check.log`、`version-check.log`、`runtime-check.log`；最后一项返回 `read_only=true`、`status=current`、`reused=3`、`built=0`：

```powershell
.build/bridgeforge/bin/bridgeforge-factory.exe manifest --root . --check
.build/bridgeforge/bin/bridgeforge-factory.exe check factory-version --root .
.build/bridgeforge/bin/bridgeforge-factory.exe build-assets --factory --project-root . --check
```

独立 review-auditor 复核了删除范围、当前原生发布覆盖、公共条件注册、辅助模块引用、活动文档引用，以及后续 fixture 和替身适配；最终无新增问题。独立复核为只读，不替代主对话的实际构建和运行结果。

## 已确认的经验与失败边界

1. 退役旧参数 `--release` 与保留当前子命令 `git-sync release` 必须分别核对。旧路径测试不能因为文件名包含 release 就与现行发布回归一起删除。
2. 本机沙箱会阻断 Git for Windows 的 MSYS 命名对象，出现 `NtCreateDirectoryObject ... 0xC0000022`。同一代码在获准的沙箱外隔离仓库重跑后可继续验证；不能据此修改业务逻辑。
3. 当前普通提交会校验模板和运行时，旧 `managed-skeleton.json` fixture 不再是有效项目。编译这类库测试时，从受管公共 CLI 的真实 self-test 身份取得官方构建同名环境变量；fixture 原样复制真实 CLI、Hook 和两份收据，并在复制前后核验。不能使用工厂投影身份代替公共身份，也不能伪造收据或跳过生产校验。
4. 布局测试原有 setup 生命周期和自造文档不再适用。先后暴露无实际变更的空提交、覆盖受管文档导致缺少生命周期标题等问题，进入独立盲点复核后改为保留完整模板正文，仅由 fixture 明确选择 `delivery_layout: flat`，删除无关 demo，增加有效 baseline／项目结构前置检查。未修改生产模板或放宽校验。
5. 旧迁移保护用例改为验证当前“自动旧二进制指纹迁移已退役”的拒绝原因，继续检查回滚、原 ignore 字节、私人文件和新公共文件不落地。缓存替身从产品传给 Cargo 的 fixture 构建环境捕获身份并返回模拟 self-test，保留构建次数、命中、漂移和零写入断言。

## 保留限制

完整发布首次调用在 `source-push` 阶段被既有活动文档的失效清单链接阻断，提交未完成，Git 索引由受管入口回滚。仅将总管开发计划中的清单链接从旧根路径修正为实际的 `source/forge/bridgeforge-manifest.json`，保留其历史正文；首次失败收据为 `.runtime/tests/test-retirement/release.log`，使用同一 `git-sync release` 入口接续，不拆分提交、打包或发布。

修正链接后第二次从同一入口接续，CLI 在 `preflight` 阶段返回 `pause_category=conflict`：未完成尝试的输入指纹已改变，原产物与尝试保留。收据为 `.runtime/tests/test-retirement/release-resume.log`。原生状态仍为 `prepared`、`commit=null`，包含 package.json 与十三项 payload，共十四项资产；本次两次调用均未完成源码提交、推送或正式发布。候选仍为 BP 1.1.0，而本地 `bp-v1.1.0` 已固定指向基线 `2b2da1c`，不能覆盖该标签发布新内容。未手动改写发布状态、删除失败现场、复用已发布版本或绕过受管入口；发布接续与新 patch 版本处理需另行明确恢复范围。

其后用户已明确授权修复 CLI release 接续并继续原发布，按 BP 1.1.1 准备新候选；修复过程和验证独立记录于[原生发布主题](../native-release/requirements_2026-11-03_single-entry-release.md#2026-11-03-提交前失败恢复修复)。本验收仍对应上文测试退役范围，不将后续 CLI 修复计入原始删减统计。

未执行完整 workspace 运行回归、真实下游 smoke、用户级安装或物理跨机验证；不将当前定向结果扩张为这些场景已通过。历史失败日志与恢复现场保留在本机测试目录。没有因为用户验收而自动修改规则、AGENTS、Hook、Memory 或归档历史资料。

<a id="all-tests-retired-2026-11-15"></a>

## 全部测试清理验收（2026-11-15）

### 用户决定与授权范围

用户明确要求“scripts/tests 我需要全部删除”“source 内有无测试代码，我也需要全部删除”，完成交付后再次确认：“本次测试代码清理验收通过，执行 summary accept，然后 git-sync release。”本节记录对全部测试清理的用户验收；提交、推送和正式发布由随后独立授权的受管发布入口处理。

本次为当前仓库的一次性清理，覆盖长期回归测试，不将此前 `summary accept` 默认保留长期回归测试的通用规则改成自动删除全部测试。产品运行时 `self-test`、测试收据处理及下游测试布局检查仍属于生产能力，保留其行为。本次不授权真实下游写入或用户环境安装。

### 实际成果及已有证据

- `scripts/tests/` 整个目录已删除，含 122 个受跟踪文件；删除前核对真实 Git 根及绝对路径为 `D:/Quant/BridgePilot/scripts/tests`，确认没有链接条目后在同一 PowerShell 内执行删除。
- 以 Rust 语法树定位并删除 69 个源码文件内 93 处测试条件块、测试模块、测试函数、夹具入口与专用辅助代码，语法删除阶段共 59,877 字节。另清理失效注册、Cargo 测试/夹具 target、`test-fixtures` feature、仅用于测试的编码观察回调和安装锁前回调、夹具可执行文件白名单，以及工厂锁生命周期对照分支。现有生产路径和运行时自检保留。
- 同步移除工厂针对独立测试 workspace 的锁文件投影、构建输入登记，以及根 AGENTS 项目区失效测试命令。公共 AGENTS 模板和 summary 通用规则未改。临时清理程序源码及可执行文件已移除。
- 清理交付时相对基线 `e217e15c8e321e2a2dcad7b8396159c14cdfe6ce` 的总差异为 272 文件、增加 131 行、删除 39,478 行，净减少 39,347 行；含源码、测试、版本、锁文件、dogfood 和清单，不含本次随后新增的验收正文及索引。
- `cargo check --locked --offline --manifest-path Cargo.toml --target-dir .build/bridgeforge/test-retirement --workspace --all-targets` 通过；Cargo metadata 显示 15 个包、0 个显式测试/夹具 target。源码语法复查不再发现待删测试条件块；`source/` 和 `.codex/hooks/source/` 的 `#[test]`、测试 cfg、`test-fixtures`、工厂测试 cfg 及夹具入口标记复查为 0。`scripts/tests` 不存在，`git diff --check` 通过。
- 锁定 Cargo 的受管准备返回 `status=prepared`、`prepared=3`、`self_test_executed=true`、`written_files=67`，完成 CLI、Hook、factory 构建、自检和自身镜像同步；本机入口收据为 `.build/bridgeforge/public-ready.json`。
- 最终零写入 `build-assets --factory --project-root . --check` 返回 `status=current`、`reused=3`、`built=0`；`manifest --root . --check` 返回 `status=ok`、`changed=false`；`template-standard --root . --check` 返回 `status=current`；`check factory-version --root .` 返回 `healthy=true`，原生和合同版本均为 `20.1.1`。

验证复用上述开发阶段结果，本次验收不补跑构建或测试。编译仍有改动前已存在的 Hook `codex_root` 未使用告警；增量缓存硬链接失败时 Cargo 复制文件继续，检查退出码为 0。真实下游、用户环境安装及业务运行未验证；删除全部回归测试后不存在可据此宣称通过的回归测试集。

### 原生版本与发布边界

本次适用包均按测试退役维护变更升 patch：Forge 20.1.0 → 20.1.1，core 11.5.2 → 11.5.3、checks 0.3.0 → 0.3.1、skeleton 0.9.0 → 0.9.1、lifecycle 0.8.7 → 0.8.8、workflow 0.16.0 → 0.16.1、memory 0.2.0 → 0.2.1、hook 17.2.0 → 17.2.1、factory 0.14.0 → 0.14.1；BP 子包 tasks 0.1.0 → 0.1.1、storage 0.1.1 → 0.1.2、execution 0.2.0 → 0.2.1、transfer 0.1.0 → 0.1.1、install 0.10.2 → 0.10.3、orchestrator 0.18.0 → 0.18.1。BP 主版本在开发交付时为 2.7.0，正式组合发布版本由原生适配器确定，不在验收中预报发布成功。

TODO 总表没有与本次全部测试清理唯一对应的条目，不补造编号、不结算无关 TODO；旧选择性退役记录保持历史语义。未额外发现需要登记的独立问题。正式发布必须取得受管 CLI 的 `status=released` 且 `publication_verified=true` 收据，提交、tag、Release URL 及资产核验结果以该收据为准。

### 本次发布尝试与接续状态

同一轮受管 `git-sync release --publication-required` 已由原生适配器确定 BP 2.7.1 并完成发布前准备，但在 `source-push` 阶段提交前失败，收据为 `status=incomplete`、`pause_category=required_condition_failed`。具体为活动文档 `../gpt6-skeleton-upgrade/requirements_2026-09-05_block4.md` 两处 Markdown 链接指向已删除的 `scripts/tests/fixtures/gpt6-behavior-cases.json`，项目结构检查以 `dead-doc-reference` 阻断。

CLI 已回滚自动提交阶段和暂存区，没有产生本次提交、推送或正式 Release。原生接续状态 `.runtime/bridgeforge-codex/native-release/current.json` 为 `blocked-before-commit`，候选版本 2.7.1 及恢复现场保留。已将两处失效链接改为历史路径和清理前 Git 版本说明，不恢复测试、不修改用户数据、不删除或手改发布状态。修复后 `check project-structure --root . --pre-commit` 退出码为 0、errors 为 0；`git diff --check` 通过，暂存区为空，测试目录仍不存在。

按 git-sync 运行边界，外层不自动重启整个同步；等待用户明确重试后以同一受管入口接续，复用合理候选，不再次升版或覆盖已发布资产。此次验收成立，正式发布尚未完成。
