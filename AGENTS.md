<!-- BRIDGEFORGE:PUBLIC:BEGIN -->
## BridgeForge 公共区

> 本区由 bridgeforge-codex 管理。项目定制写入文末“项目级专区”或项目自有的嵌套 AGENTS.md，不直接修改公共区。

### 1. 授权与沟通

- **只做已授权的事。** 确认方案不等于授权实施，授权实施不等于允许提交、推送或发布。提交和推送须有本次明确授权，且仅限指定改动。
- **已确认的事不重复问。** 换步骤、Skill 或 Agent 后，继续遵守已确认的目标、决定、授权和预算。缺少正式记录时，从已有对话中整理，不让用户重复回答。
- **只问关键问题。** 未定事项会实质改变范围、行为、验收要求或风险时，只问一个关键问题；只暂停依赖答案的工作，其余已授权工作继续。
- **只读就不修改。** 可以查看资料、做必要的小范围核验，但不写文件、不安装或修复、不跑完整回归，也不做其他未授权操作。
- **暂停要说明原因。** 说清哪些工作受影响、什么条件下能继续。因 Skill 暂停或改变流程时，链接实际读取的 SKILL.md，引用条款并说明原因。条件满足后从暂停处继续。
- **不擅自改其他项目。** 只获准修改当前项目时，不修改其他项目或上游模板。需要修改上游时，先说明影响、收益和风险，再取得授权。
- **用户要求优先于 Skill 默认流程。** 遵守用户明确要求和已有有效授权，同时遵守平台权限与安全限制。

### 2. 调查与修改

- **修改前先了解代码、查清影响。** 涉及两个以上陌生模块时，先了解相关代码。修改前核实调用方、配置入口、数据流和失败路径，调查范围与改动影响相称。不凭文件名或一条报错猜根因。
- **保留未获准改变的行为。** 重写、移植或替换时，保留原有功能、配置、数据链路和错误处理含义。不用 TODO、临时绕过或把逻辑塞进上层来掩盖缺失能力。
- **查资料要有范围。** 使用当前可用的工具，限定查找路径和输出量，优先只读，避免无差别扫描。
- **结论要有依据，证据足够就收口。** 不编造文件、接口、字段、配置或执行结果。不知道或证据不足时，明确说明，不把猜测当作事实；需要提出假设时，标明是假设，并说明尚未确认的部分。有效证据可以复用；情况变化、证据矛盾或工具输出异常时，再针对性核验。能够回答当前问题后及时给出结论，不追查无关细节。
- **修 Bug 先分清事实和猜测。** 核实数据源、用户操作路径、边界条件及外部影响。根因未确认时，说明判断的把握程度，不先加兜底或连带修改其他层。
- **体验问题先取证。** 用户反映“慢、难用、不清楚”但无法稳定复现时，先找可观察的证据和触发条件，再决定如何验证，不凭猜测修改。
- **发现错误及时纠正，连续失败停止盲修。** 发现自己的判断或操作有误时，明确说明并核验受影响部分。同一问题连续两次实质修复失败后，整理证据并进入 escalate 或 debate。权限重试、输出故障和未改代码的重测不计次数；用户明确要求外援时，立即响应。

### 3. 验证与交付

- **验证通过要有证据。** 交付和高风险结论必须有实际依据。报告验证通过时，说明命令或收据、检查标准和覆盖场景；没有证据的部分标为“未验证”。
- **审查先说问题。** 按严重程度列出问题，说明文件位置、行为风险和依据。提出架构建议时，说明推荐方案、主要取舍和适用条件。
- **交付只报实际成果。** 说明本轮完成了什么、验证结果和剩余风险，不把之前已有的改动算作本轮成果。
- **性能优化要前后对比。** 优化前记录基线，优化后用同一方法复测，完成后清理临时诊断工具。
- **重写或移植不能只靠自测。** 等价性验收、重写或移植完成后，要做独立验证，不能只依赖实现者自测。
- **审计自己的改动时引入独立 Agent。** 用户要求审计、复核需求或找遗漏，且范围包含本轮 Agent 自己的改动时，由独立 Agent 再审一次。普通解释和轻量自查不强制。
- **汇报、写文档和验收要分清。** 用户要求将成果写入项目文档或记录验收时，使用 summary Skill 的相应模式。普通进度汇报不自动写文档，也不等于验收。总结、验收和采纳规则分别判断，不因此自动修改 AGENTS、Rule、Hook 或 Memory。

### 4. Agent 与 Skill

- **沿用当前模型设置。** Skill 和子 Agent 默认使用当前会话的 model / effort，不擅自固定模型或推理强度。
- **默认由主对话执行。** 只有用户或适用指令明确要求委派时，才交给已有的 Agent 角色。主对话负责沟通、确认授权、整合和交付；子 Agent 只做分配的任务，不替用户决定，不重复已有工作。
- **受管操作由主对话执行。** bridgeforge-codex、create-worktree 和 git-sync 始终由主对话执行；git-sync 使用当前项目的受管同步入口。
- **只使用当前可用的 Skill。** 以当前会话实际发现的 Skill 为准，不在根 AGENTS.md 维护 Skill 名单。
- **编写 Skill 时保持精简。** 简单 Skill 使用单文件；复杂 Skill 的入口只保留主要流程、需要选择的地方、停止条件，以及何时读取 reference，不在入口和 reference 中重复整套规则。

### 5. 工具与运行环境

- **工具与业务代码各按自己的规范运行。** BridgeForge 工具和 Hook 使用受管 Rust workspace；项目自有 Hook 可使用其目录内的独立 Cargo 工程。两者都使用锁定的 Cargo.lock 和受管构建入口；工具链缺失、版本不足、锁文件漂移或构建失败时停止，不换用 Python、脚本包装器或其他未受管入口。配置、Hook 和入口脚本的编码与注册须符合项目检查要求。业务代码遵守项目自己的语言、环境和依赖规范。
- **不要通过 shell 字符串中转正文。** 不通过 shell 字符串中转写入或动态执行中文等非 ASCII 正文。
- **Windows 非交互任务不弹控制台。** GUI、Hook 和后台任务启动的非交互命令，使用可验证的无控制台窗口入口，保留输入输出、退出码和超时行为。只有用户明确要求时，才打开可见交互窗口。

### 6. 文件与文档管理

只保留必要的常驻规则和入口。同一信息只维护一个主要来源，其他位置只放链接、自动生成内容或执行必需的短摘要。

| 内容 | 放置位置 |
|---|---|
| 全项目约束与必要入口 | 根目录 AGENTS.md |
| 目录专属约束 | 对应目录的嵌套 AGENTS.md |
| 可重复的操作流程 | 对应 Skill |
| 机器合同 | manifest、schema 或结构化配置 |
| 可自动检查的要求 | Hook、pre-commit 和测试 |
| 设计原因、迁移说明、案例与长流程 | doc/0_architecture/ 或 doc/3_reference/ |
| 项目知识与资料 | doc/5_project_knowledgebase/ |
| 产品用途和最短使用路径 | 根目录 README.md |
| 安装、恢复与迁移说明 | 根目录 INSTALL.md |
| 本次决定、结果和下一步 | 本轮交付说明 |

- **文档按固定目录组织，变动时更新索引。** `doc/` 下使用 `0_architecture`、`1_delivery`、`2_bugs`、`3_reference`、`4_archive`、`5_project_knowledgebase` 六层目录，不擅自增删、改名或合并。布局以 `doc/README.md` 为准，它也是唯一索引；交付路径由 `delivery_layout` 决定。新增、删除、移动或重命名文档时，同步更新索引。
- **安装与恢复统一从 INSTALL.md 进入。** 每个项目必须在根目录维护 `INSTALL.md`，作为安装、换机恢复、重装和环境迁移的唯一说明入口。`README.md`、`AGENTS.md` 中的安装与恢复相关内容只做导航或保留必要的快速命令，不重复维护完整安装流程。
- **保护项目资料，不自动归档。** 项目知识资料归项目所有，骨架升级不得覆盖或删除正文。不因日期或完成状态自动归档；需要归档已完成事项时，使用 archive-scan。
- **测试和工具放在规定位置。** 独立测试、基准及其专用辅助文件放在 `scripts/tests/**`；语言原生的内联单元测试可以留在被测模块中。独立工具放在 `scripts/`，生产运行不得依赖测试集。
- **产物严格按三目录存放。** 本机运行产物进入 `.runtime/`，依赖环境和安装工具进入 `.deps/`，编译、打包及缓存进入 `.build/`；根目录及源码目录不散落 target、dist、.venv、node_modules，也不以链接绕过。本机配置在 `.runtime/config/`，下载在 `.runtime/download/`，测试产物在 `.runtime/tests/<任务>/`。源码、原生 manifest、依赖锁和用户资料保留原职责。根 `.gitignore` 完整忽略三根及内容，忽略不等于删除授权。旧 artifact-layout 旁文件与依赖它的自动清理已退役，不凭默认值删除当前业务成品。迁移同步读写与调用链，归属不明或目标冲突时停止。
- **正式开发仓库与产物分离。** 本项目持续维护的源码和开发仓库必须位于正式源码工作区，不得放入 `.runtime/`、`.deps/` 或 `.build/`。临时下载的源码和测试仓库可分别放入 `.runtime/download/`、`.runtime/tests/`，但不得作为正式项目开发根、项目登记路径或持续维护的源码事实源；转为正式开发前必须迁出。开始修改前核对真实 Git 根与用户指定的项目目录，不一致时先说明并处理，不在产物目录中另建开发仓库继续工作。受控第三方依赖源码及冻结的迁移恢复副本仍按各自用途存放，不作为本项目开发入口。

### 7. 版本与发布

版本以项目原生文件和项目自身发布流程为准。Forge 的项目版本登记、开发收口账本、配置迁移及相应发布门禁已经退役，不创建替代旁文件，也不把旧数据塞入 hooks.json。

- **各类版本分别管理。** 主程序版本以项目原生版本文件为准；子包以各自原生 manifest 为唯一版本来源。第三方依赖和骨架工具遵守各自的版本规则。
- **按改动性质升版。** 使用 `MAJOR.MINOR.PATCH`：兼容修复升 patch，兼容新增升 minor，不兼容变化升 major；`0.x` 的不兼容变化升 minor。特殊格式按语言适配合同处理。不得覆盖、复用或倒退已发布版本。
- **按项目原生流程收口。** 在授权范围内完成必要构建、测试、原生版本及依赖锁一致性核验；不调用已退役的 Forge develop 或配置化 release，不伪造旧账本。没有变化或重复执行不升版，普通 git-sync 不主动升版。
- **发布须有明确授权。** 使用项目真实发布入口，核对实际版本和交付物；不得从开发完成、测试通过或本地提交推断已经发布。
- **发布后完整报告版本变化。** release 成功后，在对话中列出主程序及自上次正式发布以来全部子包的版本变化，说明旧版本、新版本和原因。具体判级、稳定条件和证据要求由对应 Skill 与机器合同维护。

### 8. 骨架与 Memory

- **不假定目录指令已经加载。** AGENTS 的发现取决于启动目录和目录层级；目录索引只是查找线索，不代表处理某个文件时会自动加载对应指令。
- **公共文件以 Template 为准。** `.codex` 递归只允许 `hooks.json` 一份 JSON。公共基线由工具内置通用模板规则和编译来源证明校验，不保留项目侧分发合同、版本配置、收口账本或骨架版本戳；未知额外 JSON 阻断更新，已知退役项经计划、备份及事务清理。项目业务 Hook 直接在 hooks.json 注册，专属源码和 Skill 保持项目所有。
- **保护 Codex 原生 Memory。** 保留官方生成和注入机制，BridgeForge 不解析或改写 Memory 正文。跨电脑同步按受管合同处理原始字节，本机专属文件不参与同步。

<!-- BRIDGEFORGE:PUBLIC:END -->

<!-- BRIDGEFORGE:PROJECT:BEGIN -->
## 项目级专区

> 本区由项目完全所有。bridgeforge-codex 更新时必须逐字保留，不得覆盖、删除、吸收或重新格式化。

### 项目架构红线

- BridgePilot 同时包含跨项目总管、Forge 协作骨架工厂与 dogfood 样板；公共 AGENTS 以 `source/forge/templates/AGENTS.md` 为单一事实源，工厂专属约束只留在本节。
- 任何改动落地前必须明确回答传播四问：属于产品层、自身配置层还是元文档；是通用能力还是工厂专属；是否需要版本与 CHANGELOG；是否需要同步自身 dogfood。
- 通用 Rust 实现进入 `source/`，公共配置与内容进入 `source/forge/templates/`，共享 Skill 进入 `source/forge/templates/skills/`，并同步对应自身镜像；工厂专属事实禁止下沉污染 Template。
- 上游到下游与下游反哺上游的边界分别以 `doc/0_architecture/design/sync-from-upstream-playbook.md` 和 `doc/0_architecture/design/reverse-sync-playbook.md` 为准。
- 禁止对本仓库执行下游 `project_sync adopt/apply`，禁止写入 `.bridgeforge_codex_version`；工厂合规只能由 dogfood 一致性硬闸证明。
- 本项目的模块职责、依赖方向、数据流和外部副作用边界必须与本文件“项目目录地图”一致。
- 三条产品版本独立管理：BP 主版本以根 `VERSION` 为准，从 `1.0.0` 起步；BF 以 `source/forge/cli/Cargo.toml` 的显式版本为准；orchestrator 以 `source/orchestrator/cli/Cargo.toml` 为准。其他 Rust 包以各自原生 manifest 为准，不混称产品版本。根 workspace 不统一继承产品版本。配置化版本登记、开发收口账本及相应 release 门禁已退役，不重建旁文件。普通 git-sync 不升版；组件发布和纳入 BP 的组合发布分别确定范围及授权，BP 安装包记录精确组件版本，不据此宣称已安装外部 BF。
- 受管资产必须使用显式 target、稳定 asset id、可验证历史 hash 和单一 ownership strategy；禁止 glob ownership。
- safe/risk/gap 计划必须在 apply 前重算 aggregate fingerprint；漂移时禁止写入。gap 必须原样保留并降级收据。
- Skill 或 Agent 增删改名必须同步分发登记、dogfood 与引用校验；禁止维护第二份角色路由表。
- `--check` / `--dry-run` 必须零写入；`init/adopt/update` 必须只经受管 `bridgeforge project-sync` apply。
- Bug 关闭必须分别记录源码、产品传播、dogfood、fixture、真实下游与 runtime 六类证据；缺失项必须标为未验证。
- 工厂开发按实际改动范围选择必要构建、测试、独立复核及原生产物核验，复用有效证据；完整回归须有具体风险依据。旧 develop 账本不再作为完成条件，历史记录只作恢复和历史证据。Git 同步与原生版本发布分别授权，保留轻量提交检查。
- 未执行真实下游或 runtime smoke 时，禁止宣称对应验证已通过。

### 项目业务与安全红线

- 工厂不得把下游业务约束吸收到公共 Template；工厂同步与验证的真实下游写入只允许发生在用户明确授权的测试 worktree。BP 的业务任务另以该任务的用户授权、项目绑定及下游 AGENTS 为边界；登记项目不代表授权执行，不从工厂验证权限推导业务写入或发布权限。

### 项目目录地图

- `source/forge/templates/**`：下沉到项目的 Codex 公共骨架与公共 `AGENTS.md` 源。
- `source/forge/**`：工厂 Rust workspace 的唯一维护源，公共八包投影到 `.codex/hooks/source/`，factory 只用于本仓库。
- `source/orchestrator/**`：BP 私有总管，含 tasks（任务模型与规则）、storage（SQLite 与事务）、execution（执行、调度与核验）、transfer（同步换机）、install（安装恢复）、cli（命令组合）六个独立包及本地交互 Skill；与 `source/forge/` 平行，无额外 `src/`。包依赖向 tasks/storage 收敛，transfer 可调用 execution 的必要查询；只依赖 Forge core，不进入公共源码投影或共享 Skill 分发。Forge 不依赖 orchestrator。
- `source/forge/templates/skills/**`：由用户级分发器安装的通用 Skill 与 bridgeforge-codex 入口。
- `scripts/**`：Windows 用户级安装与分发入口、独立辅助工具及 `scripts/tests/**` 自动测试；工厂 Rust 命令由 `source/forge/factory/` 实现。
- `.codex/**`：本仓库 dogfood 镜像与本仓库配置；legacy 项目 Memory 仅在迁移完成前保留，不再作为运行时设施。
- `doc/**`、`README.md`、`CHANGELOG.md`：架构、交付、Bug、参考资料与发布说明。

原生 Memory 跨电脑同步的工厂架构事实源为 `doc/0_architecture/design/codex-native-memory-sync.md`。

### 项目快速命令

BP 组织单项目或多项目骨架升级时，读取私有 [bridgeforge-update](source/orchestrator/skills/bridgeforge-update/SKILL.md)；该 Skill 随 BP 包管理，不进入 Forge 公共模板或共享分发。

用户要求 BP 初始化、项目登记、换机、分派、状态核对或明确范围的暂停时，先读取 [BridgePilot 统一入口](source/orchestrator/skills/bridgepilot/SKILL.md)。该入口由 BP 独立包管理，可受管安装到用户级 Skills，不进入 Forge 共享分发。有限窗口调度由自有进程执行，窗口外接续依赖本控制对话已核验的原生 heartbeat；每次唤醒先核对机器、控制对话与实际原生配置，不接管 Unknown。任务回合结束先进入 Reviewing；只读报告核对命令、文件及原生成员，写任务按项目原生验证与人工审阅处理。旧账本自动认证已退役，不能把 completed 自动标为 Released。工作区和恢复入口见 [INSTALL.md](INSTALL.md#bridgepilot-本地试用与档案恢复)，实际验证缺口见唯一开发计划。

本工厂开发及 dogfood 优先读取 `source/forge/templates/skills/develop/SKILL.md` 及其按需引用的 reference；用户级安装副本不同时，以仓库候选作为本工厂的执行依据，不调用已退役的配置化 develop/release。本入口不分发到公共 Template 或其他项目。

实际工厂命令使用受管准备后的 `.build/bridgeforge/bin/bridgeforge-factory.exe`；模板内置规则由 `template-standard` 从通用 Template 生成，再经 manifest 与受管构建校验。不把旧账本缺失当作原生验证失败，不以本地 dogfood 证明正式发布，也不自动更新用户级共享安装。

本工厂普通 Git 同步入口为 `.build/bridgeforge/bin/bridgeforge-factory.exe git-sync`，使用现有工具与一次聚合预检。工具缺失或不匹配时报告维护缺口，不自动安装升级。原生开发验证使用下列 Cargo 入口；旧 develop、配置迁移与配置化 release 参数明确报退役，不能转回冻结工具继续在线运行。

用户完整发布仍只调用 `git-sync release`；实际受管命令为 `.build/bridgeforge/bin/bridgeforge-factory.exe git-sync release`。CLI 通过原生 Cargo metadata 登记的 BP 适配器完成必要构建、打包、源码提交推送、正式 GitHub Release 和远端核验。只有 `status=released` 且 `publication_verified=true` 才是发布成功；普通 synced 不代替发布。最短路径、四类暂停与接续规则只维护在共享 git-sync Skill；原生入口和包边界见 INSTALL。Forge 不依赖 BP 包，不重建旧版本账本。

旧 cleanup 依赖的项目产物覆盖与 Hook 登记旁文件已退役。未建立可靠的原生成品保护前，该入口明确阻断；不恢复旧 JSON，不默认清空 `.build`，不删除当前成品或本次恢复证据。

工厂显式工具刷新及开发必要构建的受管准备入口为下列第一条命令，不再作为日常 Git 同步前置。它通过锁定 Cargo 启动当前源码工厂工具，按需准备 CLI、Hook 和工厂工具；准备失败停止，不对工厂执行下游 adopt/apply。只读检查直接使用已具备 `factory-runtime-prepare-v1` 的工厂工具的 `build-assets --factory --project-root . --check`；没有该能力时报告工具不可用，不用 Cargo 引导冒充零写入检查。完整说明见 INSTALL.md。

本工厂不新增根 `.cargo` 配置目录。下面的命令从仓库根执行，编译、检查和测试必须显式指定 `.build/` 下的输出目录；不依赖 Cargo 默认 `target` 或全局环境变量。

```powershell
cargo run --locked --release --manifest-path Cargo.toml --target-dir .build/bridgeforge/cargo/local --bin bridgeforge-factory -- build-assets --factory --project-root .
cargo build --locked --release --manifest-path Cargo.toml --target-dir .build/bridgeforge/cargo/local
cargo test --locked --config scripts/tests/factory-cargo.toml --manifest-path Cargo.toml --target-dir .build/bridgeforge/test-target --workspace
cargo test --locked --manifest-path scripts/tests/Cargo.toml --target-dir .build/bridgeforge/integration-target
.build\bridgeforge\bin\bridgeforge-factory.exe manifest --root . --check
.build\bridgeforge\bin\bridgeforge-factory.exe check factory-version --root .
```

### 目录级 AGENTS 索引

- 当前没有项目自有的嵌套 `AGENTS.md`；新增时必须登记其适用目录与职责。
<!-- BRIDGEFORGE:PROJECT:END -->
