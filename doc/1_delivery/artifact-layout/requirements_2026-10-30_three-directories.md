---
lifecycle: completed
validation_status: verified
---

# 严格三目录布局：需求与实施计划

## 用户验收（2026-10-30）

用户在当前对话显式调用 `$summary accept`，确认本次三项交付符合需求，同时调用 `$git-sync release` 授权本轮提交、推送和主程序发布。验收覆盖 TODO-048 三目录合同与受管迁移、release 持久构建缓存与分组锁优化、TODO-049 cleanup 最终范围，以及解除开发阻断所需的撤回识别修复。

依据为下文工程终态：七组受管检查共 216 项通过、六个直接变化子包完成升版、memory 无升版刷新覆盖、原账本 59 条保持并追加 7 条；最终任务预览 current，工厂工具/manifest/目录核验通过，三个原生探针及最终 EXE cleanup 隔离补验通过，独立审查无新增阻断。缓存收益限于已记录的固定候选单轮对照。TODO-048、TODO-049 据此结算为已完成，不自动归档文档。

验收沿用已明确的边界：cleanup 的 build 仅删除旧版本产物及编译缓存，保留当前程序和对应收据；不使用外部清理器。真实下游、用户级安装、自然 Codex 生命周期及真实项目全量 cleanup 未执行；一次 memory 检查失败未复现、原因未定位，失败证据保留。验收不将这些项目改记为通过。发布尚待本轮受管事务成功，不能将授权写成已发布；历史阶段中的“待验收、禁止发布”由本次用户决定更新，其余事实保留。

### 发布过程中发现的基线入口遗漏

首次受管 10.0.0 发布在 pre-commit 的 `check baseline` 被阻断，报 `post-edit` Hook payload drift；事务自动恢复发布写入和 Git 暂存区，HEAD 仍为 47bc190、VERSION 仍为 9.0.0，无提交或推送。只读核验确认实际 Hook hash 与 `.codex/managed-skeleton.json` 完全匹配，问题是公共 CLI 在工厂内仍改选下游 `templates/managed-skeleton.json`：其 `.deps` Hook 合同不适用于已经迁到 `.build` 的工厂。

沿原三目录开发授权补正这一入口遗漏：CLI 与 index/工作流统一验证已安装合同，不再因存在模板而改用下游合同，未跳过或降低基线验证。新增定向测试覆盖工厂与下游正常、Hook 漂移拒绝、安装合同缺失不回退模板，1 项通过；独立 Agent 复核无新增阻断。修正仅涉及产品 CLI 及测试，后续受管刷新与预览负责重新核对版本覆盖，不据此手动升包或改写旧收口证据。首轮失败和后续事务收据保存在 `.runtime/bridgeforge-codex/release-20261030/`；发布成功与否以最终受管结果及独立远端核验为准。

## autopilot 整包执行约定（2026-10-30）

用户要求接管三个来源对话，审阅整包范围后明确授权：“预算提升至1000万，开始吧”。本节统一维护调度和累计预算，需求正文仍由各原计划维护。

| 任务 | 来源（local，当前仓库） | 需求及文件边界 | 依赖与状态 |
|---|---|---|---|
| A / TODO-048 | `01a1884d-7755-7731-98ba-23b0f09a40f4` | 本文 T1–T7；布局、检查器、工具、受管迁移、测试和 dogfood | 已验收 |
| B / P1-1、P1-2 | `01a18881-3a68-70f2-bedf-eeba218fbbde` | [缓存计划](../git-sync-performance/plan_2026-10-30_build-cache.md)；core 构建复用和 factory 编排 | 已验收，保留固定候选单轮测量边界 |
| C / TODO-049 | `01a18874-003f-76b1-b7aa-3d1d77232d0d` | [cleanup 计划](../artifact-cleanup/requirements_2026-10-30_cleanup-skill.md)；受管执行、Skill 分发及隔离测试 | 已按用户最终缩小的 build 范围验收 |

授权覆盖本仓库源码、模板、配置、dogfood、必要构建、隔离迁移/清理样本、性能对照、文档及实际受影响子包受管收口。主对话顺序实施并整合共享文件，允许 1 名独立审计 Agent 复核迁移、误删、缓存并发及必要修复。交付本地未提交改动；不暂存、提交、推送、发布，不清理真实项目，不修改真实下游/用户级安装，不创建定时任务。

整包原生 Goal token 停止预算 **10,000,000**，替代此前未批准估算，包含调研、主/子 Agent、实现、整合、验证、返工和收尾。原生计量是否包含子 Agent 未验证，不将主对话读数伪称已核实整包消耗；在途调用和收尾可能超限。时间初估约 3 小时，无时间硬限制。本机无适用周额度系数，不伪造百分比。每个高成本阶段先检查剩余额度，同包累计不因阶段或上下文恢复清零。

接管基线 `main@47bc1903fd91e26d60011417863d59ef87027941`；三个来源均 idle，工作区仅三份计划及 TODO/索引变化。Rust/Cargo 1.94.1 可用；受管 `build-assets --factory --project-root . --check` 返回 `current/read_only=true/reused=3/built=0`。Hook 注册已静态核验，自然生命周期及迁移后行为未验证。原生 Goal 无冲突；只读预检不保证后续平台写入或网络权限。

按三份计划矩阵运行定向检查、必要相关集成和独立审计；性能使用同方法前后对照，修复后只补受影响检查，不固定全量回归。旧文件归属不明、外部编辑、指纹漂移、工具/锁文件缺口、权限拒绝或预算不足时保留证据，停止受影响部分；连续两次实质修复失败遵守升级规则。人工验收另行处理，保持 lifecycle: active。下文“仅落盘”等是接管前历史状态，以本节新授权及后续证据为准。

关联原需求：[TODO-048](../../0_architecture/TODO-INDEX.md)。本文件是本次需求、计划及后续实施记录的唯一载体。历史 TODO-020 已验收，不重开或改写其结论；现行实现见[布局检查合同](../../0_architecture/design/artifact-layout.md)。

### 阶段交接：布局、迁移与缓存实现中

保存时 HEAD 仍为 `47bc1903fd91e26d60011417863d59ef87027941`，分支 `main`。本轮未暂存、提交、推送或发布；源码、模板和测试存在未提交改动，公共投影及实际工具尚未切换。

- 已实现三目录共享路径合同、默认检查、项目忽略检查和固定测试任务前缀；布局定向测试 34 项通过。独立审计发现的忽略反转、旧配置剪枝、源码误报及合法删除误阻断已针对性修正。
- 工厂与下游工具路径分流、入口渲染、基于历史收据的旧工具退役及安装回滚已接入。工厂准备定向测试 9 项通过，包含失败保护、旧工具验证后退役、外部编辑保留和退役回滚；其后新增的悬空链接与登记 Hook 迁移补丁仍需补验，不能用这 9 项结果覆盖后续修改。
- 独立审计已检查入口提前切换、旧项目 Hook 归属、旧目录清单漂移及 Unix 回滚执行权限；已补实现，最终审计与专项验证待完成。Windows 结果不能代替 Unix 实测。
- 缓存阶段已实现顺序分组持锁及固定源码/target 目录。持久输入 4 项测试、现有缓存 18 项测试通过；发布编排回归正在处理旧 fixture 路径/忽略假设。未报告性能改善。
- 有效性能基线在隔离候选执行：冷构建 `226858 ms`、3 次 Cargo；相同候选第二次 `110760 ms`、复用 2 个产物并重建 factory 1 次。原流程的同锁并存问题已复现。版本/模块阶段及改造后对照尚未完成。本机原始记录在 `.runtime/tests/release-cache/baseline-corrected.json`，不加入 Git、不假定跨机存在。
- 测量中发现仅含 `build.target-dir` 的 Cargo 配置被原缓存环境检查整体拒绝；已按受管命令显式指定 target 的既有行为修正。此前无缓存的试跑中断，其冷构建约 230 秒不作为最终有效对照。
- 必需受管 develop 预览仍被既有 `release history decreases VERSION` 阻断。历史中 9.1.0 曾撤回到 9.0.0；本轮没有修改发布历史或手工升版。此项影响正式子包收口，不阻止独立实现与测试。
- cleanup 尚未实施，依赖 A 完成。工厂 Windows 自身执行器与全量 `.build` 清理的方案仍需用受管能力证明，不能扩大到系统临时目录绕过。

已运行的主要验证入口为 `cargo check --locked --manifest-path Cargo.toml --workspace`，以及带 `--locked --config scripts/tests/factory-cargo.toml --manifest-path Cargo.toml` 的包级定向测试（`bridgeforge-checks artifact_layout`、`bridgeforge-factory factory_runtime`、`bridgeforge-core build_inputs`、`bridgeforge-core artifact_cache`）。测试中的 Git 仓库、远端与删除样本均为隔离 fixture；真实下游、用户级安装、自然生命周期及完整 runtime 尚未验证。

接续顺序：完成迁移与发布编排回归 → 完成真实缓存前后对照及独立审计 → 同步模板/dogfood/入口文档并由受管工厂准备事务迁移本项目已知产物 → 实施并验证 cleanup → 处理受管收口阻断及整包交付。继续保持单一 Goal、单一任务包和一个只读审计 Agent，不重置预算。此时原生计量 `685774 / 10000000 token`，剩余 `9314226`；子 Agent 是否计入仍未验证。进程和本机测试文件易变，接续时核对现状，不能重启任务清零。

## 1. 目标、决定与授权

用户原始目标：将四个下游项目的运行产物约束吸收进入 Forge 公共骨架，并让 `artifact_layout` 执行对应检查。讨论后用户明确选择三个根目录，要求“严格三个目录”，并选择 A，将 Forge 自身产物及相关工具入口迁移纳入范围。

2026-10-30 补充决定：固定使用 `.runtime/config/` 存放本机运行配置、`.runtime/download/` 存放下载文件。用户所称 `runtime/config`、`runtime/download` 沿用本任务已确认的根目录名 `.runtime/`，不新建无点号的 `runtime/`。

同日追加决定：测试报告、测试日志、覆盖率结果和测试运行临时数据必须固定存放到 `.runtime/tests/<测试任务>/`；该路径由建议提升为强制合同。

同日补充红线：项目根目录 `.runtime/`、`.deps/`、`.build/` 必须全部纳入项目 `.gitignore`，覆盖目录内的全部内容。三个固定运行子目录的细节与该忽略要求一并进入公共骨架红线、检查和验收计划。

需求来源为 StratusAgent、causis_risk_suite、HoldemTrainer 和 BridgePersonalAssist。四者采用 `.runtime/` 的事实不等于四者已有完全相同的红线：Causis 明确约束缓存、输出、下载及日志；其他项目主要通过架构、配置、忽略规则及运行入口落实。本次只上移通用约束，不复制业务、凭据或数据存储政策。

2026-10-30 用户授权“生成计划并落盘”。本轮交付仅为本计划、索引和待办登记，不修改代码、运行配置或本机产物。此前“开始吧”后已执行只读范围核验，未执行实现、构建或迁移。本记录不把需求选择、估算或文档落盘视为开发验收、提交、推送或发布授权。

四个真实下游及真实用户级安装不直接修改；后续工程验证先使用项目内隔离样本。真实下游迁移需要对应授权，不能用隔离验证冒充真实运行结果。

## 2. 已确认的目录职责

| 项目根目录 | 必须归入的内容 | 保护边界 |
|---|---|---|
| `.runtime/` | 应用和测试的本机配置、状态、日志、运行缓存、临时文件、运行下载、输出和测试报告 | 配置、凭据及恢复状态可能不可重建；不能把整棵目录视为可删除缓存 |
| `.deps/` | 项目依赖环境、托管解释器、第三方运行库、浏览器环境、解压后的 SDK 及安装的工具 | 下游使用的 Forge 公共工具属于依赖；依赖清单和锁文件仍留在各语言要求的源码位置 |
| `.build/` | 编译与打包的中间和最终产物、构建工作树及可复用的编译产物缓存 | 工厂从源码构建的 Forge 工具、项目自有 Hook 的编译产物属于构建结果 |

`.runtime/` 内以下三个子目录为固定合同，可在其下继续按用途分层，不能另换位置或名称：

| 固定子目录 | 用途 | 边界 |
|---|---|---|
| `.runtime/config/` | 本机运行配置，包括本机设置和配置形式的凭据文件 | 受版本管理的配置、模板、依赖清单和锁文件保留源码位置；配置读取、保存及初始化入口必须同步使用此目录 |
| `.runtime/download/` | 下载保存的原始文件、压缩包、安装包及下载过程的临时文件 | 下载完成后安装或解压得到的依赖环境归 `.deps/`；本项目编译打包生成的安装包归 `.build/` |
| `.runtime/tests/<测试任务>/` | 测试报告、测试日志、覆盖率结果和测试运行临时数据 | 必须具有具体的测试任务子目录，不直接写入 `.runtime/tests/` 根；可在任务下按运行批次继续分层 |

共同约束：

1. 根目录和源码目录不得散落上述产物。`node_modules/`、`.venv/`、`target/`、`dist/` 等不保留原位例外；需要使用这些名称时，必须位于对应分类根之下。不能满足布局的工具链明确阻断，不静默降级或在原位增加链接绕过。
2. 受版本管理的配置、依赖清单、锁文件、源码、测试源文件、文档及用户输入资料不因本规则被当作运行产物迁走。`.codex/` 保留规则、配置与受管源码；独立测试继续放在 `scripts/tests/`。
3. 按用途分类，不仅按扩展名或“缓存”字样分类：编译缓存进入 `.build/`，应用缓存进入 `.runtime/`；原始下载文件进入 `.runtime/download/`，安装后的工具和依赖环境进入 `.deps/`。
4. 同一个 EXE 在生产它的工厂属于构建产物，在消费它的下游属于工具依赖。构建/安装收据与其产物关联保存；运行事务日志及锁属于 `.runtime/`。
5. 项目配置只能细化三个根内的路径和识别规则，不能开放根外目录作为豁免。旧差异配置若与严格规则冲突，明确报告，不忽略配置或继续按旧宽松规则执行。
6. 本机运行配置和下载文件必须分别处于 `.runtime/config/`、`.runtime/download/`；即使位于 `.runtime/` 内，放在根部或其他子目录也不合规。配置迁移保留原始内容和现有权限语义，目标冲突或归属不明时停止；不得因目录调整清空持久配置。
7. 测试报告、测试日志、覆盖率结果和测试运行临时数据必须进入 `.runtime/tests/<测试任务>/`。`<测试任务>` 是实际任务目录名，不是字面占位符；不同任务分别存放，同一任务可通过批次子目录隔离并发运行。测试源码和受版本管理的固定样本仍在 `scripts/tests/`，测试编译产物进入 `.build/`，测试依赖进入 `.deps/`，本机运行配置和下载文件继续遵守各自固定目录。
8. **三类本机目录必须由 Git 忽略。** 项目根 `.gitignore` 必须忽略 `.runtime/`、`.deps/`、`.build/` 及其全部子目录和文件，不允许通过反向规则重新纳入其中的配置、下载、测试产物、依赖或构建结果。该要求必须写入公共 AGENTS 红线，不能仅作为本仓库配置约定。被 Git 忽略不代表可以删除本机数据，也不替代布局检查。

项目根 `.gitignore` 的标准规则为：

```gitignore
/.runtime/
/.deps/
/.build/
```

已有文件若已被 Git 跟踪，添加忽略规则不会自动解除跟踪。实施时单独核验这三个根内的跟踪项；迁移或解除跟踪应明确列出精确路径并保留本机文件，不自动删除文件或改写历史。本次仅更新计划，不立即修改 `.gitignore` 或 Git 索引。

推荐的实现子路径为工厂 `.build/bridgeforge/bin/`、下游 `.deps/bridgeforge/bin/`，具体子目录由统一路径合同维护；它们是本计划的技术方案，不是用户另行指定的路径名称。

## 3. 已核实的现状与影响入口

只读调查基于 `47bc190`，调查时工作树干净。开始实施前应复核现场，不把本快照当作永久事实。

| 入口或文件 | 已核实事实 | 修改目的 |
|---|---|---|
| [公共 AGENTS](../../../source/forge/templates/AGENTS.md) | 仅要求产物按项目配置存放 | 明确三目录的职责、严格约束及持久数据保护 |
| [根 .gitignore](../../../.gitignore) | 当前已忽略 `.runtime/`，尚未添加 `.deps/`、`.build/` | 将三根完整忽略要求写入公共红线并同步工厂及下游受管安装配置，保留项目其他忽略规则 |
| [artifact_layout.rs](../../../source/forge/checks/src/artifact_layout.rs) | Python 产物检查默认关闭；工作区遍历会跳过 `.venv`、`node_modules`、`target` 和根 `.runtime` 内部 | 默认检查分类位置；先检查目录自身，再按边界剪枝；保留暂存影响分析 |
| [manifest.rs](../../../source/forge/factory/src/manifest.rs) | 公共产物目标与工厂私有工具均生成到 `.codex/bin/` | 按工厂与消费项目身份生成正确目标和收据路径 |
| [runtime.rs](../../../source/forge/core/src/runtime.rs) | 工具定位及 Windows 运行镜像替换保护使用 `.codex/bin/` | 统一路径解析，维持身份检查、无窗口运行与文件占用处理 |
| [Hook util.rs](../../../source/forge/hook/src/util.rs) | Hook 从 EXE 向上三层推导项目根 | 迁移后可靠定位项目根，保留显式根路径支持，避免依赖新的固定层数 |
| [CLI lib.rs](../../../source/forge/cli/src/lib.rs) | 公共 CLI 转发工厂工具时使用旧 EXE 和收据路径 | 同步工厂调用入口和构建身份核验 |
| [generated_assets.rs](../../../source/forge/core/src/generated_assets.rs)、[artifact_cache.rs](../../../source/forge/core/src/artifact_cache.rs) | 隔离编译使用系统临时树；编译产物缓存位于 `.runtime/bridgeforge-codex/build-artifact-cache` | 将项目拥有的构建工作树与编译缓存归入 `.build/`，保留输入身份和缓存复用条件 |
| [project_sync.rs](../../../source/forge/workflow/src/project_sync.rs)、[factory_runtime.rs](../../../source/forge/factory/src/factory_runtime.rs) | 已有安装事务、目标快照、外部修改检测及失败回滚；不能仅改新目标就视为旧文件已退役 | 复用事务，补旧工具与收据的归属核验、旧路径退役、失败恢复 |
| [project_hooks.rs](../../../source/forge/skeleton/src/project_hooks.rs)、[project_hook_migration.rs](../../../source/workflow/src/project_hook_migration.rs) | 项目 Hook 有自身二进制路径、注册与迁移合同 | 区分公共工具依赖和项目自有 Hook 构建产物，同步注册与来源证明 |
| [hooks.json](../../../source/forge/templates/hooks.json)、[pre-commit](../../../source/forge/templates/.githooks/pre-commit) | 编辑、生命周期及提交入口均引用旧工具路径 | 按目标身份渲染入口；变更入口和可用工具必须成套落地 |
| [public_artifacts.rs](../../../source/forge/workflow/src/public_artifacts.rs)、[共享更新器](../../../scripts/bridgeforge_codex_shared_update.ps1) | 存在共享构建缓存、用户级 bootstrap 和安装作用域校验 | 核对项目级迁移与共享安装边界，保留引导身份及事务协议 |
| [发布包发现](../../../source/forge/lifecycle/src/release/polyglot.rs)、[Python 包发现](../../../source/forge/lifecycle/src/release/python.rs) | 排除规则仍识别旧产物目录 | 防止 `.deps/` 和 `.build/` 内的包、源码快照被计入业务包或发布输入 |
| [工厂测试配置](../../../scripts/tests/factory-cargo.toml)、根 Cargo workspace 与独立测试 workspace | 当前测试配置未统一声明新的构建目标根 | 同步普通构建、测试和受管构建的输出位置及相关实际环境合同 |

用户级 Codex home 与项目根是不同作用域。本次严格目录规则针对项目；共享安装代码须验证兼容性，但不据此自动迁移系统 Rust/Cargo 安装、用户全局缓存、原生 Codex 数据或真实用户级安装。若发现必须改变这些范围才能实现目标，先报告依赖和影响，不暗中扩大。

## 4. artifact_layout 的目标检查合同

### 4.1 检测范围

- 无 `.codex/artifact-layout.json` 时也启用三目录基础规则。现有测试布局与 Cargo 注册检查继续保留。
- 核验项目 `.gitignore` 对三个根的忽略要求及其有效语义，不以用户全局忽略或 `.git/info/exclude` 代替可分发的项目规则；对相关暂存或工作区范围内已跟踪的产物单独报告，不能因为匹配忽略规则就视为未跟踪。提交入口使用暂存规则，规则变更按实际影响检查，保留无关历史不全扫的边界。
- 对确定可识别的 Python/pytest/coverage 过程产物、依赖环境目录、构建目录和受管 EXE/收据检查分类位置；项目可声明额外产物路径，声明必须落在相应根内。
- 对能够确定类型的本机运行配置、下载文件及项目声明的对应输出路径，进一步检查固定前缀 `.runtime/config/`、`.runtime/download/`。仅位于 `.runtime/` 内不足以通过；不能通过自定义配置开放其他位置。声明路径与可识别文件的检查须在相关派生树剪枝前完成，不因跳过 `.runtime/` 内部而漏掉已知配置或下载路径。
- 对能够确定为测试报告、测试日志、覆盖率结果或测试运行临时数据的路径，检查 `.runtime/tests/<测试任务>/` 前缀和必需的任务层级；错放到其他 `.runtime/` 子目录或直接写在 `.runtime/tests/` 根部均不合规。同步检查测试入口声明的输出路径，不能因跳过派生树内部而漏检已知测试输出。
- 普通 `.json`、`.log`、二进制文件或名为 `cache` 的业务源码，不能仅凭扩展名或模糊名称推定为产物。实施时在同一布局合同登记明确识别标准与限制，并用源码、输入资料、固定 fixture 验证误报边界。
- 规则检查路径与明确配置，不做任意业务代码的文件写入语义分析；不能宣称已证明所有程序绝不写到根外。未知类型的识别缺口应在交付说明中列明，不当作“全部产物合规”。
- 合法派生树内部不作为项目源码遍历；对错误位置的依赖、构建目录先报告违规再剪枝。链接、重解析点和必要读取失败不得被解释为检查通过。

### 4.2 调用时机与历史影响

| 调用场景 | 保留的执行语义 |
|---|---|
| `post-edit` | 检查本次编辑路径及直接相关输入，只警告；不搬文件、不撤销编辑、不全仓扫描 |
| Git `pre-commit` | 检查暂存改动及实际影响范围；新增违规、配置冲突或必要输入读取失败阻断提交；使用暂存内容而非工作区替代内容 |
| 手动 `check project-structure --root .` | 检查工作区中的可识别违规，包括被 Git 忽略的错误位置目录；报告排除范围 |

无关历史产物不因普通提交被全仓扫描；严格规则生效后仍以手动工作区审计核验完整可识别布局。项目规则的语义变更继续按现有影响分析扩大相关暂存检查。无新提交的同步、SessionStart 和发布预览不新增布局全扫。

## 5. 分步实施计划

| 编号 | 优先级 | 修改目标 | 验证与交付条件 |
|---|---|---|---|
| T1 | P0 | 更新公共 AGENTS 与布局合同，建立三目录、固定 config/download/tests 子目录及工厂/消费项目路径的单一事实源；红线明确三根及其全部内容必须被项目 `.gitignore` 忽略；明确配置兼容、识别清单及不变量 | 默认配置与显式配置一致；根外及固定子目录之外的豁免被拒绝；忽略红线与三个运行子目录细节完整；文档与机器合同无冲突 |
| T2 | P0 | 改造 `artifact_layout` 三个入口，启用默认检查、固定配置/下载前缀及测试任务目录检查，核验三根忽略规则与相关跟踪项，调整剪枝并保留增量检查边界 | 逐类覆盖正确位置、根外位置、错误分类根、`.runtime/` 内错放配置/下载/测试产物、缺少测试任务层级、忽略缺失/失效、已跟踪产物、源码误报、空目录、路径大小写/分隔符及读取失败 |
| T3 | P0 | 同步 manifest、运行时、CLI、公共/项目 Hook、pre-commit、相关 Skill 命令；区分 `.build/` 和 `.deps/` 工具身份 | 工厂与下游两套入口可用；从其他工作目录启动仍找到正确项目；旧路径不作为成功后的备用入口 |
| T4 | P0 | 将构建工作树、编译缓存和测试编译目标归位；将测试报告、日志、覆盖率结果和运行临时数据的输出入口统一到 `.runtime/tests/<测试任务>/`；同步包发现、构建输入和环境指纹排除规则 | 缓存命中条件仍可证明；变更环境或输入拒绝复用；测试任务输出正确隔离；派生树不被误登记为产品源码或业务包 |
| T5 | P0 | 在已有受管安装/升级及工厂准备事务内补齐旧布局迁移；同步本任务范围内本机配置的读写/初始化及下载输出入口 | 新工具与收据先验证，入口切换及已知旧文件退役纳入可恢复事务；配置内容保持、冲突不覆盖、下载与安装目录正确分离；未知文件保留并阻断相关迁移；重复执行无多余写入 |
| T6 | P1 | 同步公共投影、dogfood、manifest、INSTALL 和受影响命令文档；工厂与下游受管安装均落实 `.gitignore` 的三根完整忽略规则 | 工厂身份不被转成下游；不写 `.bridgeforge_codex_version`；三根及 config/download/tests 子目录均被有效忽略，其他项目忽略规则保留；相关 manifest/dogfood 检查通过 |
| T7 | P1 | 执行必要集成、独立审计及受影响子包的受管开发收口 | 验证覆盖下节矩阵；收据匹配最终输入；主程序版本和正式 CHANGELOG 留给另行授权的 release |

依赖顺序：T1 → T2；T1 → T3/T4；T3/T4 → T5；T2–T5 稳定后完成 T6/T7。以上是逻辑阶段，不要求逐项向用户重复确认，也不因机械传播文件数量提高规模。

迁移复用现有安全机制：稳定 asset id、精确路径归属、原内容/收据 hash、apply 前指纹复核、外部修改保护、失败回滚与最后写入版本戳。不能用递归删除整个 `.codex/bin/` 或任意替换字符串代替迁移。工厂迁移经工厂入口，下游经受管 project-sync；不对本仓库执行下游 adopt/apply。

## 6. 验证计划与完成标准

验证优先复用 [布局单元测试](../../../scripts/tests/unit/core_artifact_layout.rs)、[布局 CLI/Git 场景](../../../scripts/tests/src/artifact_layout_runtime.rs)、[工厂准备测试](../../../scripts/tests/unit/core_factory_runtime.rs)、[项目同步测试](../../../scripts/tests/unit/core_project_sync.rs)、[构建来源测试](../../../scripts/tests/src/build_provenance.rs) 和现有 Hook/共享安装测试，避免创建平行测试框架。

| 验收面 | 必须覆盖的场景 |
|---|---|
| 严格布局 | 三根分类正确；原位 `.venv`、`node_modules`、`target` 等被识别阻断；Git 忽略不等于合法位置；未配置项目默认生效 |
| 固定配置与下载目录 | 本机配置进入 `.runtime/config/`、下载文件进入 `.runtime/download/`；`.runtime/` 根部或其他子目录的已知错放被识别；受版本管理的配置不误迁；配置迁移保真、冲突拒绝覆盖；下载归档与安装后的依赖分别归位 |
| 固定测试任务目录 | 报告、测试日志、覆盖率结果及运行临时数据进入 `.runtime/tests/<测试任务>/`；缺少任务层级、其他子目录错放被识别；不同任务及并发批次互不覆盖；测试源码、固定样本、编译产物和依赖保持各自分类 |
| Git 忽略红线 | 项目 `.gitignore` 有效忽略 `.runtime/`、`.deps/`、`.build/` 及全部内容，包括 config/download/tests；缺失或失效规则被识别；全局忽略不能代替项目规则；暂存与工作区规则差异按各入口处理；已跟踪产物不被忽略规则掩盖，处理时保留本机文件 |
| 检查范围 | 编辑只警告；新增暂存违规阻断；合法删除违规文件可通过；未涉及历史不扩大；规则变化按实际影响检查 |
| 无误伤 | 受管配置、锁文件、源码、测试源文件、输入样本和文档保留；`.runtime/` 持久配置不被清理 |
| 安装与迁移 | 空白项目安装；受支持旧布局更新；重复执行；工厂准备；无戳/非法身份按原合同拒绝；不复活已退役的低版本升级路径 |
| 事务失败 | 旧文件未知、目标被修改、输入指纹漂移、构建失败、文件占用、写入/验证失败；恢复本事务改动并保留外部修改 |
| 运行入口 | 工厂转发、公共 Hook、项目自有 Hook、pre-commit；跨工作目录；无控制台行为及退出码保持 |
| 构建与发布发现 | 构建目标根正确；缓存身份可复用或正确失效；新派生树不污染源码指纹、包发现或版本分析 |
| 零写入 | `--check` / `--dry-run` 不建目录、不加锁、不构建、不写缓存或收据 |
| 传播 | 产品源码、模板、dogfood、manifest 与使用说明一致；真实下游/自然生命周期的未验证范围明确 |

只选择能覆盖本次风险的定向单元、相关集成和实际 CLI smoke；跨模块迁移需要独立验证，但不预设每轮运行全部 workspace 与所有 fixture。若需扩大，先指出具体覆盖缺口。性能不另立优化目标；检查不得引入无条件全仓扫描，发现可归因开销再做同方法定向对照。

完成标准：T1–T7 具备对应实际证据，受影响子包按可用受管能力收口，列明剩余限制。代码完成、工程验证、用户验收、用户级安装、真实下游升级及发布分别报告，不互相替代。

## 7. 传播、规模、预算与停止点

传播四问：本次主要属于产品层的通用能力；工厂角色路径属于自身配置差异，不能硬编码进所有下游；实现变化需要按受管流程完成子包收口，根 VERSION 与正式 `[product]` CHANGELOG 在获授权 release 时处理；公共配置、源码投影、manifest 和自身 dogfood 必须同步。本轮仅记录计划，不执行任何版本写入。

规模从最初的 M 调整为 L，依据是纳入受管安装与升级路径迁移。只读核验后的工程估算约 2 小时，编译缓存及真实测试耗时尚未测量；它不是承诺或已批准的时间上限。此前提出的“3 小时 / 60k token / 1 名审计 Agent / 2 轮验证”未获确认，且其估算依据不足，不能作为已批准预算沿用；token 未实测。正式实施前按 confirm 的 L 级规则一次明确整体预算，复用本文件全部已确认需求，不再重新访谈目录选择。

停止点：

- 工具链、锁文件或受管构建能力不满足时停止相应构建，不换未受管入口。
- 旧文件归属不明、外部编辑或指纹漂移时保留现场，不能为完成迁移强删、覆盖或改写旧通过收据。
- 工具链无法在严格三根内运行时报告具体阻断，不恢复根外例外。
- 发现必须改真实下游、用户级安装或系统全局目录时先明确影响和授权，不能据本计划自动执行。
- 本轮落盘完成后不自动开始代码实现、构建、真实迁移或 Git 发布。

## 8. 当前进度

已按本文开头的整包约定实施，仍为 in_progress。严格布局、工具路径、受管精确旧文件迁移及构建缓存已进入验证；最新 factory runtime 10 项通过（3 项显式辅助/性能测试跳过），project_sync 46 个定向迁移场景通过，core 66 项通过（7 项跳过）。之后新增的旧测试收据迁移与 Windows 活动路径边界也已定向通过。真实工厂首次准备返回 prepared（3 个工具、146 个写入、21 个旧文件退役），随后只读检查返回 current、0 构建、未执行 self-test。后续源码改动尚需最终刷新；不等于真实下游或自然生命周期通过。

旧构建缓存按 Owner/Entry 协议核验精确文件集合与 hash，在原 cache.lock 持锁期间逐文件迁到 `.build/bridgeforge/legacy/`，纳入既有安装事务；原锁 inode 留在 runtime，避免旧进程重新创建另一把锁。无 owner 的退役目录只允许原锁及空的 64hex 条目目录；未知内容阻断。Windows 实测持有内部锁时整目录 rename 被拒绝，因此未采用释放锁后搬目录的有竞态方案。

旧根 `D:\Quant\BridgeForge\target` 的整包保留搬迁已得到用户明确同意，并在恢复后完成。3805 个文件、4360 个目录/文件条目、2360970403 字节，搬至 `.build/bridgeforge/legacy/root-target`，逐文件 SHA-256 与搬前一致；原路径已不存在。收据为 `.runtime/tests/artifact-migration/root-target-move.json`。Windows 不允许持有内部文件句柄时改名父目录，因此先持句柄核验、关闭句柄后再次核对清单/元数据，再同卷原子改名并复核；未删除文件。

仍待：最终工具刷新、集成与独立复核、其余旧目录处置、与最终输入一致的开发收口。现有 develop 预览因已撤回的 9.1.0 发布历史下降阻断，未手改版本或绕过校验。用户已将 cleanup 的 build 范围收窄为历史编译，否决外部执行器方案；旧的清空 build/删除工具后恢复测试已停止，不再作为验收条件，详见其唯一需求记录。无 Git 提交、推送、发布、用户级安装或真实下游写入。

后续只读布局检查还识别到 `.codex/hooks/target`、`scripts/tests/target`、`templates/hooks/target`、`.venv` 和三处历史 `__pycache__`（`.codex/scripts`、`scripts`、`scripts/tests`）。这些旧目录均保留；根 target 的询问不隐含授权处理其他目录，尤其虚拟环境不能仅改路径后就宣称可用。

### 本轮最终工具与隔离入口验证

受管准备命令 `cargo run --locked --release --manifest-path Cargo.toml --bin bridgeforge-factory -- build-assets --factory --project-root .` 再次成功：prepared=3、written_files=143、retired_files=6、self_test_executed=true；后续直接入口只读检查 current、reused=3、built=0、未执行 self-test；manifest 检查 changed=false，factory-version 为 healthy（根版本仍为 9.0.0）。未更新用户级安装。

独立集成工程 `scripts/tests/Cargo.toml` 的实际验证结果：build_provenance 12 通过，artifact_layout_runtime 的两个 CLI/Git 功能场景通过（性能场景显式跳过），Hook 入口矩阵 10 通过，prepared_public_artifacts_are_shared_and_update_preserves_project_hooks 1 通过（127.66 秒），independent_project_hook_completion_requires_own_build_and_rejects_external_inputs 1 通过。公共产物测试使用临时 profile 和两个临时项目，核验安装、更新、私有 Hook 保留、重复应用零写入/零构建及安装 CLI baseline；私有 Hook 测试实际锁定 Cargo 编译，并拒绝包外输入。不是正式下游升级或自然应用生命周期证据。

测试迁到项目 runtime 后，修复了辅助模块 import、旧收据父目录准备、仍要求根外快照的断言，以及非 Git 仓库 fixture 向上发现真实父仓库的问题；后者通过仅限测试子进程的 GIT_CEILING_DIRECTORIES 和清除外部 Git 定位变量隔离。初次失败保留为失败事实，只有修正后对应测试算通过。开发检查登记已补新增 cleanup/迁移/输入同步/进程测试及共享路径辅助文件，两份历史日志输出改为 `.runtime/tests/development/`；未改旧收口收据。

当前只读全工作区检查仍报告 49 个位置错误，均来自尚未授权搬迁的 7 个旧目录及其中 Python 字节码；`.runtime/test_receipts/` 只剩空的 payload_samples 目录。7 个旧目录合计约 24.2 GiB，无重解析点，拟分别原样移入 `.build/bridgeforge/legacy/` 和 `.deps/legacy/root-venv` 并保持 cleanup keep；目标均不存在。用户目前只询问这些操作涉及哪个项目，尚未同意，未执行。已向用户说明两项待定事项都仅涉及 `D:\Quant\BridgeForge`。

刷新后同范围 `develop --preview --packages core,checks,skeleton,workflow,lifecycle,cli,hook,factory` 仍返回 `release history decreases VERSION`。建议单独识别已批准的 9.1.0 撤回，但用户当前仅询问问题含义，尚未授权新增该能力；版本写入继续停止，主程序版本、CHANGELOG、Git 历史和发布状态不变。整包仍为 in_progress，不作完成或验收声明。

最终独立夹具复核另发现 baseline 收据来源测试未单独建立 `.codex` 父目录；已补建，`baseline_rejects_generated_receipt_from_different_source_tree` 定向 1 项通过。其他所审夹具修正保留原构建、漂移、回滚和保护断言，没有用降低检查替代迁移适配。原生 Goal 在用户明确恢复后仍显示 paused，最近可读累计为 1,761,328 / 10,000,000 tokens；此计数不包含暂停状态后的工作，不作为本轮精确总消耗，也未通过重建目标或改数据库重置预算。

### 用户改为删除旧目录后的执行结果

用户明确要求“旧目录全部删除，不保留”，覆盖此前保留搬迁的决定。本次仅删除已列明的 7 处旧目录及已搬迁的 `.build/bridgeforge/legacy/root-target`，共 8 处、21,253 个文件、28,363,547,502 字节逻辑大小（约 26.42 GiB）；未扩大到其他运行数据、当前编译目录、用户级安装或下游。执行前复核绝对路径均在本项目内、所有祖先及子项无重解析点、无活动进程路径引用，并逐文件检查占用；使用 PowerShell 原生 LiteralPath 删除。执行后 8 处均不存在，当前工具/收据、VERSION、Cargo.lock 的 SHA-256 保持。实际磁盘释放量未测量。

删除收据为 `.runtime/tests/artifact-migration/legacy-directories-deleted.json`，status=completed。删除已失效的 root-target keep 输出声明后，实际公共 CLI `check project-structure --root .` 错误数由 49 降至 0；工厂 `build-assets --factory --project-root . --check` 仍返回 current、reused=3、built=0、read_only=true。因此旧目录处置阻断已解除。

用户同时说明本机将直接升级到 9.1.0 以上，并询问处理方式。当前建议：保留 Git 历史，严格识别已有批准的撤回后完成开发收口；下一次获授权发布选择高于 9.1.0 且符合改动性质的版本，再直接更新本机，无需中间安装 9.1.0。现有工具在生成新版本前即因历史下降阻断，仅修改本机版本号无法消除此问题。此条仍是方案说明，未实施撤回识别、未改版本或发布。

### 撤回识别修复授权与实施中状态

用户随后明确批准“修复撤回识别，继续完成剩余开发和子包收口，暂不提交、推送或发布”。授权覆盖通用历史识别修复及本工厂的明确撤回登记，不扩大到本机共享安装或其他项目。传播属于产品层通用 lifecycle 能力，工厂的三个提交 ID 仅留在自身配置；受管收口负责子包版本，主程序 VERSION/正式 CHANGELOG 不在本轮改动，公共源码与 Skill 合同需同步 dogfood。

核实撤回提交 `47bc1903fd91e26d60011417863d59ef87027941` 恢复正式 9.0.0 提交 `31d1451ca5f32a6c4ed129e439e9fb5f9f210070`，撤回正式 9.1.0 提交 `35b5ad7ad1829ceae8805d78abdb759065c6b645`。恢复树仅保留 CHANGELOG 及三个 doc 文件差异；原 59 条账本逐字恢复，撤回发布增加的 factory 0.6.0、workflow 0.4.0 仍保留为不可复用上限。实现正在进行，定向夹具和独立复核通过后再准备工具并执行真实受管收口；本段不是完成声明。

撤回识别已完成定向实现与独立静态复核：第一父链中的正式回退必须有精确声明，恢复树包含模式及 gitlink 比较，replacement/graft 无法隐藏历史；配置降级和已提交声明删除被拒绝。首次历史没有配置仍允许正常接入。已发布版本上限沿稳定包 ID 保留，同语言 move/rename 不被旧路径冻结，合法退休仍检查其间升级记录。普通检查输入仅约束本次执行，不被误当成长期产品内容；稳定性输入、合同、产物以及产品/构建/源码归属仍禁止文档豁免。

`cargo test --locked --config scripts/tests/factory-cargo.toml -p bridgeforge-lifecycle withdrawal_tests -- --nocapture` 的 5 组隔离测试通过（70.56 秒），另 `withdrawal_document_roles` 1 组通过。覆盖恢复后 0.3.1→0.3.3（跳过已发 0.3.2）、退休后继续发布 1.0.3（跳过已发 1.0.2）、伪造中间低版本拒绝、保留 CHANGELOG 测试输入、产品文档拒绝、首次缺配置、源码恢复不符、graft/空声明与配置删除。完整受管收口尚未执行；这些夹具中的提交/发布只发生在隔离临时仓库。

第一轮受管准备普通权限因 `.codex` 维护锁访问拒绝退出，按已有授权在提升权限下成功（prepared=3、written_files=138、retired_files=0、self-test=true）。其后真实 develop 预览暴露 CHANGELOG 纯检查输入被误阻断，已按上述输入角色修复；工具因此正在再次准备，第一轮产物不作为最终交付证明。最终收口将执行受影响包已登记的必要检查；lifecycle 的公共历史入口影响正常发布、首次配置、迁移与多语言版本连续性，需覆盖相关既有回归，不用单独六组新测试替代。

修复后的再次受管准备成功（prepared=3、written_files=138、retired_files=0、self-test=true）。真实任务预览已进入 development-analysis-required：直接选择 core/checks/skeleton/workflow/lifecycle/factory，工具自动纳入依赖覆盖变化的 memory。实际执行权限与初始预览之间指纹变化使首次 apply 在写入前拒绝；同权限新预览重核范围后开始第二次事务，候选 core=8.0.0、workflow=0.5.0、factory=0.7.0、memory 保持 0.1.0。core-unit 返回 101，随后回滚访问拒绝；已执行受管 develop --recover 成功，事务日志清除、core 恢复 7.1.0、根 VERSION/CHANGELOG 无 diff。不能把这些候选版本算作收口完成。

恢复后同参数 core 独立诊断为 71 通过、7 跳过（31.99 秒），尚未复现受管候选失败；已给登记的 core 检查增加 `.runtime/tests/development/core-unit-tests.log`，不改变测试范围或通过条件，下一次受管候选检查将保留具体失败用例。原失败没有被替换为通过记录，检查和恢复日志均保存在该任务目录。

第三次 apply 的日志定位到 `running_legacy_image_blocks_migration_until_process_exits`：子进程退出后，夹具错误地要求全机无外部 BridgeForge 进程，实际受管 develop 父进程仍在运行；生产的未知外部进程保守阻断符合原语义。该次事务自动回滚成功，无 journal。修复仅将原判定提取为私有快照函数，公共入口仍传完整系统枚举；隔离测试断言真实子 PID 和映像路径、错误中的子 PID、退出后新枚举中消失，并独立覆盖本项目新路径允许、旧路径/外部未知/其他项目新路径仍阻断。定向 2 项通过、1 个辅助入口按设计忽略（子进程由父测试实际调用），独立审查认可此隔离边界。该修复没有放宽生产进程检查。

第四次 apply 之前再次受管准备成功；候选下 core 72 项通过、7 项跳过，factory Git 同步 14 项通过。lifecycle 的 6 项基础身份检查与 3 组 Rust/Python 大型矩阵均通过，随后发现三个旧 fixture 准备错误：remove/rename-chain 用 schema3 默认配置覆盖了已迁移的 schema4，explicit-target 重复迁移。为保留失败证据并避免无效长跑，只终止已核对 PID/映像/命令行的本次 lifecycle 测试子进程，受管父进程自动回滚成功；未终止其他进程，未完成的整次 lifecycle 检查不算通过。

只修三个测试函数的准备步骤，保留全部退休、依赖、改名顺序和版本下限断言；`release::polyglot::tests` 11 项重测通过（26.48 秒）。生产实现和大型矩阵的公共 helper 未改变，独立审查确认可复用已完成矩阵。工具只读检查仍 current、reused=3、built=0，因此没有因纯测试改动重建工具。余下 lifecycle 回归按完整模块前缀排除上述已完成组后执行，线程数 4、独立夹具目录，未发现全局环境/工作目录修改；结果与最终候选 `withdrawal_tests` 收口检查分别记录，不把分组累计覆盖写成最后一次 apply 全量执行。

余下 lifecycle 回归实际完成 281 通过、9 失败、9 忽略、20 过滤（819.42 秒）。其中四项暴露撤回审计调用旧边界校验时误用基线提交，现改为从实际 HEAD 验证并保留边界相等断言；旧配置入口恢复在 Git Reader 创建前拒绝。测试修正包括 Cargo 产物忽略目录、准确的提前拒绝消息，以及使用仅测试可用的无缓存 Reader 代替 graft 开关；真实历史和执行期间不变性检查仍然保留。另一项本地 clone 因 Windows 沙箱 MSYS 命名对象权限失败，改在已授权的提升权限环境定向重测，不改生产逻辑。九个失败项及六个撤回场景合并定向回归，结果单独保存于 `.runtime/tests/development/lifecycle-regressions.*`；未通过前不视为问题关闭。

该组定向重测最终为 15 通过、0 失败（283.41 秒），`cargo check --locked -p bridgeforge-lifecycle` 通过。独立 Agent 复核最终差异，确认普通边界的 HEAD 校验、配置提前拒绝、无缓存测试入口及三目录夹具修正均未削弱保护，无新增阻断。此前成功的 281 项、11 项 polyglot 单元组、6 项基础身份检查及 3 组多语言矩阵按实际范围复用；修复只影响已定向覆盖的历史边界和测试准备。最终候选收口仍实际运行登记的 `withdrawal_tests`，不把旧日志导入或伪装为最终事务全量执行。

### 受管子包收口结果及最后补验

第五次 apply 中 core、factory 和撤回检查通过，memory-unit 返回 101，事务自动回滚成功、无 journal。原失败只有 Cargo 通用错误，没有具体用例；同参数独立诊断为 43 通过、3 忽略（77.17 秒）。没有证据确定根因，未猜测修改 memory 生产实现。为剩余登记检查补齐逐项日志及 nocapture，保留检查范围和断言；第六次同范围受管执行 memory 通过，旧失败未重现，保留为未定位的历史失败。

第六次 `develop --apply --packages core,checks,skeleton,workflow,lifecycle,factory --analysis-file .runtime/tests/development/autopilot-analysis.json` 返回 `development-closed`，检查 scope=final-candidate、executed=7、reused=0、git_written=false。实际通过数为 checks 34、core 72、factory 14、lifecycle 6、memory 43、skeleton 41、workflow 6，共 216 项；core 7 个辅助/现场入口、memory 3 个性能入口、workflow 1 个需显式最终工具的入口在此命令中忽略，不能将其算为该命令通过。收据为 `.runtime/tests/development/autopilot-apply-6.stdout.json`。

| 子包 | 收口前 | 收口后 | 说明 |
|---|---:|---:|---|
| checks | 0.1.0 | 0.2.0 | 严格布局检查 |
| core | 7.1.0 | 8.0.0 | 三目录、迁移、缓存与清理合同 |
| factory | 0.5.0 | 0.7.0 | 工厂布局及缓存；跳过已发 0.6.0 |
| lifecycle | 0.6.0 | 0.7.0 | 可验证撤回及历史版本上限 |
| memory | 0.1.0 | 0.1.0 | 无内容变化，仅刷新依赖/构建覆盖 |
| skeleton | 0.2.0 | 0.3.0 | 新目录工具及 Hook 迁移 |
| workflow | 0.3.0 | 0.5.0 | 同步和 cleanup；跳过已发 0.4.0 |

账本由原 59 条增至 66 条，前 59 条逐项内容未变；无残留 journal。HEAD 仍为 `47bc1903fd91e26d60011417863d59ef87027941`，根 VERSION 仍为 9.0.0，VERSION/CHANGELOG 无 diff。0.x 包未自动转为 1.0.0：本次不兼容变化或未配置稳定合同，具体原因保留在实际收据中。任务范围收口不等于全仓 release 就绪。

最终复核另发现 memory 后台探针子测试 exact 名称过时，可能零测试退出成功；现从实际模块名生成 exact 名称，并强制断言子进程执行 1 项且通过。三个直接 rustc 探针（memory-background、memory-user-config、artifact-cache）编译输出统一改到 `.build/tests/<任务>/<唯一批次>/`，保留原特殊字符路径覆盖，运行数据仍在 `.runtime/tests/`。事务结束后才修改这些测试输入，避免候选执行期漂移；三个实际探针定向通过（core 1 项、memory 2 项），包括真实后台进程不继承额外句柄、参数/标准流/退出码及长路径缓存执行。日志为 `.runtime/tests/development/native-probe-final.*`。这些是收口后的测试补验，不改写旧检查收据；最终工具准备和任务当前性仍需核验。

### 本地工程交付终态

最终版本的受管准备成功（prepared=3、written_files=138、retired_files=0、self_test_executed=true）。随后只读检查返回 current、reused=3、built=0，manifest changed=false，factory-version healthy=true，project-structure errors=[]。收据统一为 `.runtime/tests/development/autopilot-final-{prepare,current,manifest,version,structure}.json`。最终同范围 develop 预览为 `task-development-current`、affected_packages=[]、pending_reasons={}；测试补验和工具传播没有重新打开本次任务包，未把任务当前状态扩大为全仓发布就绪。

最终 `.build/bridgeforge/bin/bridgeforge-factory.exe` 的隔离 cleanup 实测 1 项通过（2.47 秒），日志为 `.runtime/tests/development/cleanup-final-exe.*`；删除运行和旧编译样本后，当前程序、对应收据、config/download 保持，清理后 self-test 成功。该场景显式补齐前述受管检查中的 workflow ignored 入口。独立 Agent 最后复核探针修正、版本账本及证据边界，无新增必需阻断。`cargo fmt --all -- --check` 与 `git diff --check` 通过；只有现有 CRLF 转换提示，无差异格式错误。

整包授权内实施、必要工程验证、独立复核、最终产物和子包收口完成，交付本地未提交改动。用户验收尚未发生，lifecycle 保持 active，TODO 不代用户结算；文档前面的阶段记录保留失败与授权演进，不代表当前仍阻断。真实下游迁移、用户级共享安装、自然 Codex 生命周期和真实项目全量 cleanup 均未执行，不宣称通过。用户特别授权的 8 处历史目录删除已另有完成收据，不扩大该删除范围。

后续若获准发布：保留当前 Git 历史，受管发布依据改动性质选择严格高于 9.1.0 的主程序版本，再更新本机安装；本轮不预写主版本、不提交推送发布。若用户要求验收，可基于本记录核对三目录、缓存对照和 cleanup 保留行为，不重复已经有效的工程验证。交付前最后一次原生 Goal 检查仍显示 paused，累计读数冻结为 1,761,328 / 10,000,000 tokens，不能代表恢复后的准确总消耗；没有重建目标或改写原生数据库。
