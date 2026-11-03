---
lifecycle: completed
validation_status: passed
---

# 旧测试链路激进退役验收

## 用户验收2026-11-03

2026-11-03，用户要求分析 `scripts/tests/` 中可永久退役的测试，随后明确采用“清理更激进，非必要不保留”的原则，并在清单迁移任务结束后授权实施。交付后用户显式调用 `summary accept`，确认本轮交付符合需求；同时调用 `git-sync release`，另行授权本仓库本轮指定改动的完整发布。

验收范围为旧配置化 develop/release、开发收口账本、旧 cleanup 成功流程及其独占辅助代码、fixture 和基准退役，保留当前功能与安全回归。后续关于整体删除测试目录的问答仅解释可行性和代价，没有授权删除剩余目录或停用全部自动化测试。

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
