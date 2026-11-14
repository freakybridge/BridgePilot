---
lifecycle: active
validation_status: awaiting_validation
topic: gpt6-skeleton-upgrade
date: 2026-09-05
---

# 板块四：一致性与升级验证

验收收据（2026-09-05）：用户明确“同意验收”，接受本地测试补充；前三板块的未执行行为场景及完整模型 A/B 仍待验证，本板块保持 active，不把用例校验计为行为通过。用户另行授权 Git 同步；不包含安装传播，无整题归档候选。

用户已授权修改，继续控制文字膨胀。本轮只补工厂测试与验收材料，属 `[repo] / [meta]`；产品版本保持 `1.14.11`，不改运行代码、公共规则、Skill、角色或分发清单，不安装、不提交推送。

## 范围与方法

- 扩展既有真实合同测试，不复制框架：合成旧公共区和旧角色，使用现行完整合同升级；核对项目专区、Skill、文档行和 Hook 保留，旧角色退役、版本落盘及再次规划无动作。此夹具不是完整历史安装回放，也不测试用户级 Skill 分发。
- 当时的行为用例文件 `scripts/tests/fixtures/gpt6-behavior-cases.json` 是场景与预期的唯一事实源；Rust 只检查用例完整性和引用，不调用模型、不自动安装。该文件已在 2026-11-15 经用户明确授权随全部测试清理删除，历史内容可从清理前 Git 提交 `e217e15c8e321e2a2dcad7b8396159c14cdfe6ce` 读取，不再作为当前可运行入口。
- 对照运行时固定模型、effort、工具及初始现场，旧规则取用例中的已核实 Git 基线，新规则取候选工作树；仅替换被测指令，隔离上下文并保留加载证据。无法证明隔离时不得报告有效 A/B。
- 每例记录输入、工具动作、文件差异、提问及结果；Agent 主动动作与后台 Hook 分开记账。原始日志留 `.runtime/`，本文只记证据指针；比较行为而非措辞，有限样本不得宣称普遍改善。

## 验证与传播状态

使用 `cargo test --locked --manifest-path scripts/tests/Cargo.toml <过滤名> -- --test-threads=1`：

| 过滤名 | 实际结果 |
|---|---|
| `project_sync_combined_upgrade` | 1 项通过：受管公共区和四角色更新、退役旧角色、项目自有内容保留、再次规划无动作 |
| `gpt6_behavior_cases` | 1 项通过：6 个用例 ID 唯一、输入完整、指令路径可解析；不代表模型行为通过 |

去掉过滤名运行完整工厂回归：**83 通过、0 失败、2 ignored，319.17 秒**；跳过项为子进程辅助入口和需额外授权的 Assist 来源夹具。用例措辞收紧后再次单独运行用例校验，1 项通过。

受管 CLI 的 `manifest --root . --check` 为 `changed=false`，`check factory-version --root .` 为 `healthy=true / 1.14.11`，`check baseline --root .` 为 `clean`，`self-test --json` 为 `ok`；metadata 无问题，structure 仅既有归档提示，instruction-source、格式与 diff 检查退出 0。

本轮前后 158 个产品及镜像路径的 SHA-256 / 删除状态一致：运行规则文字增量为零。只扩展一项测试、增加一项用例校验，JSON 保存完整场景，本文不复制场景正文；没有新增测试调度器。未改产品代码，Template / dogfood 回归沿用板块三证据，不重复宣称本轮执行。

模型 A/B、用户级安装及真实下游升级均未执行；前三区块记录保留。这里只交付测试补充，不将板块四整体标为验收完成。

## 课题一：个人与骨架 AGENTS 精简（2026-09-05 后续实施）

本节记录后续独立授权的实施，不改变上文板块四原始范围与历史验收。用户接受两份完整 MD 候选稿，并在具体文件清单后明确“开始吧”；授权修改个人配置、工厂产品源码、dogfood、配套文档和定向验证，不包含用户级 Skill 安装、完整回归、真实下游、提交或推送。本轮由主对话完成，无子 Agent。沿用 M 级 45 分钟 / 20k 新增 token 估算预算，token 未实测；一次定向验收及发现旧断言后的修复重测，权限重试不另计验证轮次。

### 已实施与传播范围

- 个人 `C:/Users/bridg/.codex/AGENTS.md` 应用用户接受的偏好与最小安全边界稿；不由产品清单接管，也不把正文复制进仓库。旧文件备份在本机 `.runtime/agents-topic1/personal.before.md`，候选副本为同目录 `personal.after.md`，均不提交。
- 产品 `templates/AGENTS.md` 应用用户接受的候选稿，工厂根公共区同步，工厂项目专区从 marker 起到文件末尾的字节与修改前完全一致。
- `project-asset-migration.md` 明确恢复时核对可见的完整迁移包、授权、源 hash、目标原值及基线；仍有效的决定复用，变化部分及共享目标相关包重核，无法恢复时不从摘要推定删除授权。不增加持久恢复状态。Rust `asset_migration::validate_manifest` 已校验源身份、完整确认与目标 hash，本次不改迁移程序和事务语义。
- 修改指令架构、上游更新手册及澄清参考的旧章节引用；受管澄清参考同时修改模板源与工厂镜像。
- 定向检查发现 Hook 内重复写死旧公共标题。删除该标题副本，仍以受管公共区完整 hash 验证内容，保留标记、项目标题、UTF-8、围栏及工厂镜像检查；源码与 dogfood 同步。
- 产品版本更新为 `1.14.13`；三个 Cargo manifest 和 lock 仅同步本地包版本，外部依赖未升级。受管 CLI 生成模板/工厂清单与 Skill 分发 hash，并通过 `build-assets` 重建本项目 Hook/CLI 和实测收据。
- 本机 `confirm` Skill 仍是旧安装版；工厂源码已支持直接接受明确自由回答。本轮不修改或安装用户级 Skill，不将文件存在当作新会话已加载。

### 文本大小与核对结果

以 UTF-8 字节统计，不作为 token 或性能指标：

| 对象 | 修改前 | 修改后 | 变化 |
|---|---:|---:|---:|
| 个人 AGENTS | 4856 | 1816 | 减少 62.6% |
| 公共模板整文件 | 12124 | 8642 | 减少 28.7% |
| 根 AGENTS 公共区，含 marker、不含结束 marker 后换行 | 11145 | 7857 | 减少 29.5% |

个人旧文件 SHA-256 为 `f58af13ab8dc2daf48d1c1220353cd340b9b168bc15497b8f2a4fa79c108c3a1`，新文件为 `326726ecf47de831699cbc6e656c027528f13f603b1061d5dca09593785f9216`；复制后核对新文件与候选副本 hash 一致。工厂项目专区字节比较结果为 `True`。

### 实际定向验证

工厂测试公共命令前缀：`cargo test --locked --offline --manifest-path scripts/tests/Cargo.toml`；每项追加下列过滤名和 `-- --test-threads=1`。

| 过滤名 | 结果与断言 |
|---|---|
| `gpt6_behavior_cases` | 1 通过；11 个用例 ID 唯一、输入完整、引用可解析，原 6 场景保留；不验证模型行为 |
| `public_instruction_region` | 1 通过；两份公共区一致、四个 marker 各出现一次 |
| `migration` | 7 通过、1 ignored；组合迁移保留项目内容与完整最新公共区，失败回滚、非法/未确认目标阻断、Hook 注册一致；ignored 的 Assist 来源夹具未运行 |
| `rust_source_is_identical` | 1 通过；模板与 dogfood Rust 源文件一致 |

`cargo test --locked --offline --config scripts/tests/factory-cargo.toml --manifest-path .codex/hooks/Cargo.toml --lib instruction_source_uses_trusted_public_hash -- --test-threads=1`：1 通过；受信公共区的新章节可接受，正文篡改、项目标题缺失、重复 marker、未闭合围栏与缺失可信合同仍报错。该测试检查程序判定，不是模型行为对照。

本轮总计 **11 项定向测试通过，1 项需要额外来源授权的夹具跳过**。首次用例检查因旧测试只允许恰好 6 个 ID 失败，首次组合迁移检查因匹配旧句子失败；已改为保留原场景的结构检查、迁移后完整公共区对比，修复后通过。没有通过放宽迁移授权或丢弃回滚断言来通过测试。

构建和一致性证据：

- 官方 `manifest --root .` 生成清单；首次受保护目录写入权限失败后同命令补齐。`manifest --root . --check` 返回 `changed=false`。
- `build-assets --project-root D:/Quant/BridgeForge` 返回 `status=built`，包含 `codex.hooks` 与 `codex.bridgeforge-cli` 两份收据。为避免 Windows 正在运行的映像覆盖，驱动使用修改前本仓库受管 CLI 的 hash 相同副本；没有安装或使用用户级产品。
- 新 `.codex/bin/bridgeforge.exe self-test --json` 返回 `ok / 1.14.13`；`check factory-version --root .` 返回 `healthy=true / 1.14.13`；`check baseline --root .` 返回 `clean`，覆盖受管资产与新产物收据。
- `check instruction-source` 无诊断、退出 0；`check skill-metadata --root .` 无问题或警告；`check project-structure --root .` 无错误，仅有既存归档候选提示。
- 未暂存、未提交、未推送；工作树保留本轮改动。Git 的用户级 ignore 读取仍有既存权限警告，不据此宣称用户级 Git 配置已验证。

### 未验证项与下一步

当时的行为用例以 `scripts/tests/fixtures/gpt6-behavior-cases.json` 为唯一场景定义；该文件在 2026-11-15 随全部测试清理删除，历史读取方式见“范围与方法”。本节基线更新为修改前已核实的 `0edbd425773e9fff9c0de5473e43b1dc3ebb1818`；原基线 `500f5caeb670301917a14296009732a95c37fa3d` 仅用于此前板块记录。个人基线使用上述本地备份，不另建受管个人配置副本。

- 首轮在隔离上下文对照旧版/候选版的授权续接、只读自由回答、范围边界和 Bug 用户路径；按 `comparison` 明确个人单独、骨架单独和共同使用的加载组合，记录实际指令与 Skill 的 hash。
- 迁移恢复额外覆盖未变现场与共享目标变化；只用隔离夹具，不调用 updater 或真实下游。
- 实际模型 A/B、独立审计、完整回归、新会话个人指令加载、本机 Skill 安装、真实下游和真实生命周期 Hook smoke 均未执行。本轮主对话的分析或工具测试不能替代这些证据。
- 上游实现与定向验证已完成，但课题一仍等待行为验收，不关闭、不宣称协作性能或交付质量已经改善。用户级安装与下游传播单独安排，不作为本轮已完成项。
- 如需回退，个人文件用上述逐字备份恢复；仓库按本轮 diff 恢复相关源码、版本和清单，再通过受管入口重建匹配产物。不使用整仓 reset，不覆盖后续用户改动。

## 课题二：find-doc 检索链清理（2026-09-06）

用户在确认课题二含义及 find-doc 的具体证据后授权“开始吧”。本次按 S 级处理已有入口与 reference 的局部一致性问题，主对话实施，无子 Agent；不改多 Agent 分工、Map 生成器、其他 Skill 流程或课题一已接受的 AGENTS。保留课题一所有未提交改动，不安装用户级 Skill，不操作真实下游，不提交或推送。

### 实现

- `skills/find-doc/SKILL.md` 直接承载简短输出规则，删除 `references/output-format.md` 及其显式分发登记。没有新增 reference、索引或状态系统。
- 移除旧 `1_plan` 路径、已不存在的 Step 2 rules 字典和固定 Glob/Grep 工具名。Delivery 按 `doc/README.md` 的实际布局检索；可读索引明确指向的指令文件，不扩成源码或 Memory 调查。
- 输出先给最相关位置与来源，进展结论读取 `lifecycle`、`validation_status` 和必要正文；字段缺失或矛盾时说明未知，不从目录或文件名推定活跃、完成、验收。未命中的关联约束省略。
- 两文件合计由 5235 UTF-8 字节降为单文件 4773 字节，减少 8.8%；执行链少一次必读格式 reference。这是文件结构结果，不是 token、耗时或模型质量实测。
- 产品更新到 `1.14.14`，同步三组 Cargo 本地包版本；外部依赖未升级。现有受管 CLI 生成清单/hash，并通过 `build-assets` 重建本项目 Hook/CLI 和收据。Skill 不另复制到工厂 `.codex/skills`，用户级分发登记为唯一安装入口。

### 定向核验

以下前缀 `cargo test --locked --offline --manifest-path scripts/tests/Cargo.toml` 各追加过滤名和 `-- --test-threads=1`：

| 过滤名 | 实际结果 |
|---|---|
| `managed_manifests_are_current` | 1 通过；受管清单与实际源一致 |
| `active_skills_do_not_invoke_retired` | 1 通过；现有退役入口检查通过 |
| `distributed_roles_match_skill_references` | 1 通过；角色引用与 dogfood 保持一致 |

额外检查：

- 分发清单中 find-doc 恰有一个文件，源为 `skills/find-doc/SKILL.md`；被删除的 reference 不存在，活动入口/登记不再引用旧 reference、旧路径或字典。
- `check skill-metadata --root .` 无问题或警告；`manifest --root . --check` 为 `changed=false`；`check factory-version --root .` 为 `healthy=true / 1.14.14`。
- 新 CLI `self-test --json` 为 `ok / 1.14.14`；`build-assets` 为 `built` 且包含两份产物收据；`check baseline --root .` 为 `clean / 1.14.14`；`check instruction-source` 无诊断、退出 0。
- 主对话只读路径走查：限定架构目录的文件名搜索命中当前指令架构文档；README 命中本交付主题；读取状态字段和正文确认它仍是 `active / awaiting_validation`，没有因“实现完成”判为整体已验收；不存在主题返回无命中。没有调用 Map 刷新、updater 或额外 Agent。
- 上述走查证明检索路径可用，不是隔离模型 A/B。未对不同模型、重复运行、安装后实际 Skill 调用或性能收益作结论。

### 状态与回退

本次已确认的 find-doc 旧引用和单文件承接问题完成源码修复与定向核验。整个 Skill 体系的行为一致性、用户级安装、新会话加载和真实下游仍未验证，不将这一次修复写成整个课题二全部验收。

本轮开始前的工作树 diff 保存在本机 `.runtime/agents-topic2/preexisting.patch`；find-doc 原入口和原 reference 的备份分别是同目录 `find-doc.before.md`、`output-format.before.md`。回退时只恢复本轮增量及相应版本/登记，按受管入口重建匹配产物，不覆盖课题一改动；这些临时资产不提交。用户级个人 AGENTS 未再次修改。

## 课题三：快照与恢复可靠性（2026-09-06）

用户在只读排查、影响文件说明后授权“开始吧”。本轮由主对话实施，保留课题一、二的未提交改动；不安装用户级 Skill、不操作真实下游、不执行完整回归、不提交推送。未调用真实项目的 snapshot manual、Stop 或 PostCompact，未改写或清理现存交接。

### 证据与具体规则

- 原快照将 Git 查询失败变为空字符串并显示 clean；现在快照和启动状态复用一次 `git --no-optional-locks status --porcelain=v2 --branch`，区分查询未知、无上游、未创建首个提交和 detached HEAD，并记录 HEAD。文件保存成功不等于 Git 状态已验证。
- 原文件名只精确到秒并允许覆盖；现在使用独占创建，同名时递增后缀，不替换已有文件。写入失败返回错误，尽力删除本次未完成文件；清理失败也不伪报成功。
- 原 20 份配额混合手动与自动记录。现在只淘汰超出 20 份的、可识别的 Stop/PostCompact 自动状态；manual、有交接标题、未知或不可读的旧文件不自动删除，不迁移历史文件。代价是手动交接会逐渐积累，后续清理由用户显式授权，不新增索引或自动归档系统。
- 新增现有 Hook 的只读子命令 `snapshot latest` / `snapshot list`，与 SessionStart 提示共用选择函数。优先选有四个非空交接段的文件；没有完整交接时明确标记 `state-only / incomplete`。这个结构判定不证明交接内容在语义上完整，resume 仍需核对目标、授权和下一步。
- snapshot Skill 使用命令回传的确切路径，四段交接中保留任务目标、已有授权、禁止项和未验证项。resume 核对 HEAD 与下一步相关文件；同分支、同文件列表不证明 dirty 内容未变。缺 HEAD 或 unknown 的旧快照仅作为历史线索，不宣称一致，也不为补字段改写旧快照。
- 恢复遇到差异先检查影响，只有实质影响范围、授权、安全或验收的未决项才暂停依赖动作；没有目标或授权依据时不能从一份自动 Git 状态开始实施。

### 影响与传播

- 产品层：`templates/hooks/src/session.rs`、`templates/hooks/src/lib.rs`、`skills/snapshot/SKILL.md`、`skills/resume/SKILL.md`。`lib.rs` 的额外修改仅为接入共用的只读选择入口，避免在 Skill 和启动提示中维护两套选择算法。
- 自身配置层：同步 `.codex/hooks/src/session.rs` 和 `lib.rs`；个人 AGENTS 与公共 AGENTS 不变。
- 版本：`1.14.15`，同步三组 Cargo 本地包版本及锁文件；未升级外部依赖。清单和本仓库运行产物经现有受管入口生成，不手改派生 hash。
- 元资料与测试：复用本记录、doc/README、`scripts/tests/unit/hook.rs` 和既有行为用例 JSON，没有新增文档或状态系统。

### 定向验证

已执行 `cargo test --locked --offline --config scripts/tests/factory-cargo.toml --manifest-path .codex/hooks/Cargo.toml --lib`，分别追加下列过滤名及 `-- --test-threads=1`：

| 过滤名 | 实际结果与覆盖 |
|---|---|
| `snapshot` | 6 通过：既有生命周期去重及手动入口、真实非 Git 夹具查询失败、状态解析、同秒并发独占创建、交接选择与自动保留、只读入口和缺交接降级 |
| `lifecycle_write_failures` | 1 通过：写入失败可见且不输出成功收据 |

并发和保留测试只操作隔离临时目录；当前真实快照未用于写入实验。查询超时映射到 unknown 的路径由代码和缺失输出解析断言覆盖，未执行真实超时等待。测试并未证明模型遵循恢复规则。

另执行 `cargo test --locked --offline --manifest-path scripts/tests/Cargo.toml`，分别追加 `gpt6_behavior_cases`、`managed_manifests_are_current`、`rust_source_is_identical` 及 `-- --test-threads=1`，各 1 通过：场景 ID 与输入可解析、清单匹配、Template/dogfood Rust 源一致。连同上述 7 项，共 10 项定向测试通过；行为场景的结构检查不等于模型 A/B 已运行。

- 受管 `manifest --root .` 生成清单，`build-assets --project-root D:\Quant\BridgeForge` 返回 built 和两份生成收据。新 CLI `self-test --json` 返回 ok / 1.14.15；`manifest --root . --check` 为 changed=false；`check factory-version --root .` 为 healthy=true。
- `check baseline --root .` 为 clean / 1.14.15，fingerprint 为 `sha256:a124c9c33a5dd7a2c96fe6582479b774d28897e94cb9a338b6b05333a3f0d68c`；`check skill-metadata --root .` 无问题或警告，`check instruction-source` 无诊断，`check project-structure --root .` 无错误，仅有既存归档候选提示。
- 实际构建后的 Hook 只读执行 `snapshot latest` / `snapshot list`，通过 Windows 隐藏进程等待退出并捕获收据，均退出 0。latest 返回 `2026-09-01_073248.md`；list 返回该文件及 `2026-08-30_024020.md` 两份完整交接。现存 20 份快照的路径、SHA256 和修改时间在执行前后完全相同。输出收据保存在 `.runtime/agents-topic3/selector-*.out`，没有触发真实快照生成或淘汰。
- 仅对改动 Rust 文件执行 rustfmt 检查并通过；git diff --check 通过。全 workspace 格式化曾触及三份无关 Template 文件，已核对其原始 dogfood 与 HEAD 一致后恢复该格式化增量；最终未扩大源码改动范围。
- 初次 Cargo 测试因受保护构建目录拒绝写入而未启动，窄范围提权重试后通过。未暂存、提交或推送，HEAD 仍为 `0edbd425773e9fff9c0de5473e43b1dc3ebb1818`；Git 用户级 ignore 仍有既存读取权限警告。

### 最小行为对照与停止点

既有 `gpt6-behavior-cases.json` 增加四个场景：完整交接之后有自动记录、同分支 HEAD 与 dirty 内容变化、旧快照或 unknown、只有自动状态而没有任务。基线 Skill 取本轮前的备份；旧新组使用各自匹配的 Hook。固定模型、effort、工具和初始文件，分别记录读取文件数、工具调用数、重复确认次数、授权及未验证事项是否保留。

接续正确、零越权、零假完成是验收条件；流程更轻以对照结果判断，不从源码字符数或单元测试外推。缺目标、缺授权或相关差异无法核实就暂停依赖动作；不要求遇到任何 Git 差异都重做访谈。隔离模型 A/B、新会话实际调用、用户级 Skill 安装、独立审计、完整回归及真实下游仍未执行，本课题等待行为验收。

回退只恢复本轮增量和匹配版本/清单，再用受管入口重建产物。`.runtime/agents-topic3/preexisting.patch` 保存本轮前工作树，旁边保存 session/lib、两份 Skill 和 Hook 测试的原文；不使用整仓 reset、不覆盖课题一、二及后续改动。运行期新快照仍是 Markdown，回退程序前保留快照目录，因为旧版的混合淘汰规则会重新生效。

## 课题四：Hook 错误传播与批量编辑检查（2026-09-06）

用户在只读诊断和 8 个核心文件、预计 21 个受管文件的范围说明后授权“开始吧”。主对话实施四个已确定问题，保留既有改动；不修改启动修复机制、Hook 注册与 timeout/context 参数、个人 Hook、用户级 Skill 或真实下游，不提交推送。测试中的生命周期及写入只发生在隔离夹具，不触发真实项目生命周期。

### 实现与取舍

- 调度器将 Map 标脏、SessionStart 重建和 Stop 重建的 StepResult 纳入诊断与退出码。失败返回非零，说明 Map 不可作为最新证据并提示直接检索原文件；后续文件检查、启动检查或快照仍执行，已完成的工具编辑不回滚。一次标脏失败后不对剩余文件重复报同一错误；原有严格 Map 命令不变。
- 同一次 post-edit 事件的编码公共扫描与 instruction-source 各执行一次；乱码检查仍覆盖所有去重目标，requirements/Cargo/fallback 等专项仍逐文件运行。BOM 硬闸仍先行，失败后跳过依赖检查。未缩小原有全局扫描范围，未添加缓存或常驻监控。
- apply_patch 虚拟事件保留源/目标路径，另携带逐修改块的新侧内容和新增行号。fallback 提醒只匹配与新增行相交的片段，不检测删除内容，不把不同 hunk 拼成新构造，也不对未改动上下文中的旧坏味道重复提醒。Edit/Write/MultiEdit 原有内容仍被读取；该检查仍是软提醒，不提升为阻断规则。
- 跨项目 guard 仍按路径阻断；提示明确已有授权继承、需要在目标项目的受管任务中执行、当前入口不会因对话确认改变判定。没有新增授权字段、白名单或恢复状态，不允许改 Hook 根路径绕过边界。

产品源码为 `templates/hooks/src/lib.rs`、`post.rs`、`guards.rs`，同步同名 `.codex/hooks/src` dogfood。核心测试复用 `scripts/tests/unit/hook.rs` 与 `scripts/tests/src/hook_guards.rs`。产品版本 `1.14.16`，三组 Cargo manifest/lock 只更新本地包版本，外部依赖未升级。配套 VERSION、CHANGELOG、三份清单、本记录与 doc/README，共 21 个本阶段修改的受 Git 跟踪文件；另重建忽略的 Hook/CLI 与收据。公共及个人 AGENTS 不在本阶段增量中。

### 实际定向验证

以下每条 cargo test 均使用 `--locked --offline`，并在过滤名后追加 `-- --test-threads=1`：

| Manifest / 配置 | 过滤名 | 结果 |
|---|---|---|
| `.codex/hooks/Cargo.toml`，`--config scripts/tests/factory-cargo.toml --lib` | `hook_topic4` | 3 通过：1/10 文件公共扫描各一次、后续文件乱码/BOM不漏检、Map 失败传播和继续其他工作、Edit/patch 一致及移除/上下文/跨 hunk 负例 |
| 同上 | `lifecycle` | 3 通过：快照去重、生命周期写失败及注册入口覆盖 |
| 同上 | `post_edit_and_stop` | 1 通过：标脏到 Stop 重建的既有成功路径 |
| `scripts/tests/Cargo.toml` | `hook_guards::`，另加 `--nocapture` | 9 通过：全部既有工具别名、补丁 add/update/delete/move 两端边界、非法 payload、只读命令，以及本次原生二进制诊断/回退和批量探针 |
| 同上 | `managed_manifests_are_current` | 1 通过：清单与当前源码一致 |
| 同上 | `rust_source_is_identical` | 1 通过：Template/dogfood Rust 源一致 |

共 18 项不同的定向测试通过。公共扫描计数器仅在 factory-test 编译条件下存在，不进入发布产物，不写额外状态。原生 post-edit 夹具验证了 Map 失败退出 1、诊断和回退提示可见、后续 fallback 仍输出、已编辑文件正文保持；跨项目探针两次相同输入均退出 2，提示不再声称确认可改变结果。

### 小夹具前后对照

修改前先将本项目 `1.14.15` Hook 复制到 `.runtime/agents-topic4/bridgeforge-hook-before.exe` 并核对 hash。设置 `BRIDGEFORGE_TEST_HOOK` 指向该文件，执行 `cargo test --locked --offline --manifest-path scripts/tests/Cargo.toml hook_topic4_batch_probe -- --test-threads=1 --nocapture`。重建后清除该临时命令环境覆盖，使用默认 `1.14.16` 产物在同一测试中复测；均为隔离小目录，每组 3 次，计时包含子进程启动。

| 输入 | 旧公共指令诊断次数 | 新公共指令诊断次数 | 旧中位耗时 | 新中位耗时 |
|---|---:|---:|---:|---:|
| 1 个文件 | 1 | 1 | 52.806 ms | 60.536 ms |
| 10 个文件 | 10 | 1 | 65.650 ms | 54.981 ms |

夹具故意缺少 AGENTS，以稳定的 cannot-read 诊断计数证明公共检查调用次数；它不模拟大型真实仓库的完整 instruction-source 扫描。计数和后续文件断言证明重复工作减少，耗时仅描述本次样本，不能宣称普遍提速或模型交付质量提高。旧产物探针额外运行 1 次测试，不重复计入上述 18 项。

### 产物、未验证项与回退

- 受管 `manifest --root .` 和 `build-assets --project-root D:\Quant\BridgeForge` 完成；生成器返回 built 及 Hook/CLI 两份收据。新 CLI self-test 为 ok / 1.14.16，factory-version 为 healthy，manifest --check 为 changed=false；baseline 为 clean / 1.14.16，fingerprint 为 `sha256:6eee8993d2c3c58315667fb763bd1828cdef3a53bd26c097250b389b49e2d12c`。
- Skill metadata 无问题或警告，instruction-source 无诊断；限定改动 Rust 文件的 rustfmt 检查与 git diff --check 通过。
- 真实 Codex 自动触发、项目规模下的实际延迟、模型读取失败提示后的行为、新会话加载、用户级安装、真实下游、独立审计和完整回归均未验证。直接运行二进制和单元测试不能替代平台自动触发证据。
- 启动自修复、相对入口、超时/上下文上限、个人 `on_resolved.sh` 保持调研候选，不在本轮改动中。先在隔离入口验证需求与风险，再决定是否实施；不为补齐课题四擅自扩大授权。
- `.runtime/agents-topic4/preexisting.patch` 和核心文件原文备份保存本轮前现场。回退只恢复本阶段增量与匹配版本/清单，再经受管入口重建产物，不整仓 reset，不覆盖课题一至三或后续改动。未暂存、提交或推送。

## 课题五：流程投入与任务风险匹配（2026-09-06）

验收收据（2026-09-06）：用户明确调用 `$summary 同意验收`，接受本阶段三份 Skill 的源码修复、四项定向检查及配套记录；另行调用 `$git-sync` 授权保存并推送当前工作区的累计改动。实际模型 A/B 及本记录既有必要行为验证仍未完成，因此整份记录保持 `lifecycle: active`、`validation_status: awaiting_validation`，不将用户接受当前成果等同于行为验证通过。预算和 summary 仍为待取证候选，用户级安装与真实下游仍未传播；本次不归档、不新增规则或 Hook。

提交前修正（2026-09-06）：受管 git-sync 的 pre-commit 发现课题四测试源中的连续问号触发编码硬闸，提交未生成，自动版本和暂存已回滚。将 `scripts/tests/unit/hook.rs` 中故意构造的坏文本改为等价 Rust 十六进制转义，运行时输入保持相同，不豁免或绕过编码检查；随后仅复验该批量扫描用例。此项属于本次 Git 交付的必要修正，未扩展产品行为或降低验收要求。

修正验证：`cargo test --locked --offline --config scripts/tests/factory-cargo.toml --manifest-path .codex/hooks/Cargo.toml --lib hook_topic4_batch_scans_once_and_checks_later_files -- --test-threads=1`，1 通过、25 filtered out；仍能检测第十个文件的坏文本及 BOM，批量公共扫描断言通过。

### 范围与授权

用户在三份核心文件及配套验证、传播清单后明确“开始吧”。本轮仅实施已确认的固定委派和审计收据歧义修复，由主对话完成；不安装用户级 Skill、不操作真实下游、不运行完整回归、不提交或推送。按逻辑改动为 M 级，沿用默认 45 分钟 / 20k 新增 token 估算、最多一个子 Agent / 两轮验证的口径；本轮不委派，token 未实测。版本和机械生成文件不增加逻辑规模。

### 当前证据与修改

- `skills/develop/references/agent-execution.md` 原先固定要求 L 级顺序任务交给调研和实现 Agent，即使已有完整证据仍有交接义务。现在默认由主对话复用调研并实施，有独立子任务及相应授权、预算时才委派。L 级独立审计和并行路径复用审计收据保持。
- `skills/develop/SKILL.md` 原先通用收据要求独立 review，停止条件没有限定是否必需，与 M 级条件审计路径有歧义。现在只在需要时要求 review 收据；必需验证缺失仍须明确未完成及影响，无需 review 不作为交付缺口。
- `skills/sync-docs/SKILL.md` 原先无条件将文档定位交给 `light-explorer`。现在默认主对话定位并复用已核实结果，仅有独立定位任务且授权、预算允许时委派；只读预览、Map 回退和原文核对保持。
- 未修改个人或公共 AGENTS、Agent 角色、collab、confirm 预算和 summary 流程；预算造成无效暂停、普通总结额外阅读的实际收益仍需取证，不预先删除。

### 传播与验证范围

核心修改属于共享 Skill 产品层，版本升为 `1.14.17`，CHANGELOG 标记 `[product]`；行为用例与交付说明是工厂验证资产和元文档。三组 Cargo manifest/lock 仅同步本地包版本，分发清单和 dogfood 合同通过受管生成器更新，产物通过官方 build-assets 重建。三份共享 Skill 没有项目内的同名 dogfood 副本；用户级安装是独立传播步骤，本轮不写入。

共 17 个本阶段受 Git 跟踪文件：三份 Skill、既有行为用例 JSON、VERSION、CHANGELOG、三组 Cargo manifest/lock、三份清单、本记录和 doc/README。没有新增流程文件、索引或状态系统。

### 最小行为对照方案（尚未执行）

复用 `scripts/tests/fixtures/gpt6-behavior-cases.json` 的四个 `topic5_*` 用例。每例使用相同的隔离小项目、输入、授权、预算、工具和 GPT-6 Astra model/effort，各运行修改前后一次，共八次新上下文运行；先作行为筛查，有差异再对同例配对复测，不能用一次耗时宣称稳定提速。

为隔离课题五效果，两组均固定本轮其他指令和现场。旧组三份 Skill 从 `.runtime/agents-topic5/*.before` 提取，新组取当前文件；记录实际路径与 hash。不要直接用全局 baseline_ref 回退整个项目，否则会混入课题一至四差异。个人配置如加载也须两组相同。期望与禁止断言只交给结果评估，不喂给执行者。已有记录的静态检查不是该行为对照结果。

| 用例 | 用户路径与质量断言 | 流程投入断言 |
|---|---|---|
| `topic5_sync_docs_small_preview` | 正确定位已有文档及变化，文件树无写入，不声称已同步 | 简单定位由主对话完成，不刷新 Map 或固定委派 |
| `topic5_develop_l_sequential_reuse` | 依赖修改正确、成功与失败路径通过；独立审计实际读取需求和 diff | 复用已有调研，取消固定的调研和实现交接，保留审计 |
| `topic5_develop_m_no_review_required` | 完成约定成功、缺字段及错误输入验证，准确报告结果 | 无必要审计条件时不额外启动 Agent，不误报交付未完成 |
| `topic5_develop_required_review_unavailable` | 必需独立验证不可用时明确未完成及风险，不伪造收据或发布 | 如实交付已有成果，不用自查充当独立审计 |

每次记录额外确认次数、Agent 数与职责、重复读取和验证次数、工具调用数、耗时，以及需求遗漏、越权和假完成。必须先满足质量断言，再比较投入；必要审计被跳过、验收遗漏或越权时停止扩大简化并修复或回退。本轮不启动新任务或 Agent，因此八次模型运行全部标为未验证。

### 定向验证与交付状态

三份 Skill 修改及四个行为用例定义已完成。以下四项分别执行 `cargo test --locked --offline --manifest-path scripts/tests/Cargo.toml <过滤名> -- --test-threads=1`，各一项通过，共四项，未运行完整回归：

| 过滤名 | 实际验证断言 |
|---|---|
| `gpt6_behavior_cases_have_unique_ids_and_resolvable_inputs` | 用例 ID 唯一、输入非空、指令路径可解析，原用例保留；不代表模型行为通过 |
| `distributed_roles_match_skill_references_and_dogfood` | 已分发角色与 Skill 引用、dogfood 一致 |
| `managed_manifests_are_current_and_python_free` | 受管清单与当前源文件一致，无旧 Python 入口 |
| `rust_source_is_identical` | Template 与 dogfood Rust 源一致 |

受管驱动 `.runtime/agents-topic5/bridgeforge-build-driver.exe manifest --root .` 返回 changed=true，`build-assets --project-root D:\Quant\BridgeForge` 返回 built 和两份生成收据。新 CLI 执行下列检查：

- `self-test --json`：ok / 1.14.17。首次遗漏 `--json` 仅返回 usage / code 2，按源码要求补参数后成功；未修改实现。
- `manifest --root . --check`：changed=false。
- `check factory-version --root .`：healthy=true，VERSION、合同及 CHANGELOG 一致。
- `check baseline --root .`：clean / 1.14.17，fingerprint 为 `sha256:5654d2235cac66f7fbdbf81b5cd1a42940f377094c41f1f1dbe667b16ebc1d72`，覆盖生成产物与 dogfood。
- `check skill-metadata --root .`：issues=[]、warnings=[]。
- `check instruction-source --root .`：无诊断，退出 0。
- `check project-structure --root .`：errors=[]，仅有既存的其他交付归档建议。
- `git diff --check` 通过；相对本轮原文备份，17 个预定文件发生变化。暂存区为空，HEAD 仍为 `0edbd425773e9fff9c0de5473e43b1dc3ebb1818`。

Hook 二进制 hash 为 `ab8320147785d82fef7f50e88cabca1a786c0afeb2a66522157b2e3479e083ad`，CLI 为 `c606a65fd04315519e270fb1c18fe908c6a2360238de7675068eee9aaca2d871`。Cargo manifest/lock 的普通写入被 dogfood 目录权限阻断后，通过受控补丁完成；生成和构建经已授权的受管入口完成，没有升级外部依赖。

验证覆盖用例结构、角色引用分发、清单、版本、dogfood 和文档结构，没有增加匹配具体措辞的伪行为测试。实际模型 A/B、新会话加载、用户级安装、真实下游、独立审计和完整回归均未验证。课题五已确认的三项规则修复完成；预算与 summary 的实际流程负担仍为待取证候选，不能宣称整个课题已行为验收。

### 回退

`.runtime/agents-topic5/` 保存 17 份修改前文件、preexisting.patch 和 hash 核对一致的旧 CLI 驱动。仅撤销课题五增量并恢复匹配的版本与清单，再通过受管入口重建产物；如这些文件已有后续修改，按增量合并，不直接覆盖。保留前四个课题的全部工作，不使用整仓 reset。

## 待办：持续高耗能设置提醒 Hook（2026-09-07）

- [x] `TODO-HIGH-COST-REMINDER`：2026-09-07 已实现并安装本仓库，用户明确同意验收；范围、收据与未验证边界统一见[高耗能提醒需求卡](requirements_2026-09-07_high-cost-reminder.md)。下方保留实施前调查证据，不代表当前交付状态。

### 已确认需求与建议口径

- 高耗能条件：GPT-6 Astra 使用 High 或更高推理强度，或者任意模型开启 Fast。两种条件满足其一即命中。
- 用户已确认：同一对话连续使用高耗能设置满 10 轮提醒；持续使用时每再满 10 轮提醒一次，即第 20、30……轮继续提醒。
- 建议实施口径（尚未单独验收）：一条用户消息及其完成回复计一轮，工具调用和子 Agent 不另算主对话轮次；恢复普通设置后连续计数归零。仅提示，不自动切换模型、不阻断任务。
- 实施前明确异常/取消轮次、轮中切换、恢复会话、压缩及重复 Hook 的计数与去重规则；读取不到状态时标记未知，不以全局 TOML 冒充当前对话实际设置。

### 本机实测证据与边界

- 实测环境：Codex Desktop 26.901.6511.0、CLI 0.153.4。测试任务“确认模型切换实测A”，ID `01a07b39-6667-7a53-a006-2e26f6a529af`，测试后已重新归档。
- 真实 `turn_context` 含模型、effort 和 turn_id，但本次样本不含 service_tier。官方 Hook 文档提供模型及 transcript_path；不能据此宣称原生 Hook 已直接提供 Fast 字段。
- 在界面开启 Fast 后发送一轮仅回复“收到”的测试请求，日志 `codex_core::session::handlers` 的 TurnInput 记录 `service_tier: Some(Some("priority"))`，轮次 ID 为 `01a07b55-b90b-7b60-b476-93f76f336c45`。
- 在界面恢复标准速度后再发送一轮对照请求，日志记录 `service_tier: Some(None)`，轮次 ID 为 `01a07b56-6058-7d03-8894-efb810a5cac3`。两轮日志 Submission ID 均与真实 turn_context.turn_id 相同；配置全文已核对恢复到实测前状态，项目配置未改。
- 候选读取路径是本机 `~/.codex/logs_2.sqlite` 内的逐轮调试记录；这是内部格式，尚不是可靠产品合同。`Some(None)`、`None`、默认档位及继承的语义必须分别处理，禁止将所有空值直接解释为 Fast 已关闭。
- 未验证：其他模型/Fast 档位、跨轮继承、日志缺失或轮转、数据库锁定及保留策略、Hook 触发时的记录可见性、不同 Codex 版本、真正的第 10/20/30 轮提示与界面呈现、真实下游。当前仅证明本机开关对照和轮次关联可观察，不代表 Hook 已实现或端到端验收。
- 官方参考：[Hooks](https://learn.chatgpt.com/docs/hooks)。实现需保持确定性计数和短提示，避免为提醒另开模型任务；读取范围限定当前对话，不扫描其他项目正文。

### 后续实施与传播

本次属于工厂元文档记录，不修改 Hook、配置、Skill、公共 AGENTS 或运行产物，不发布版本或同步下游。未来实现应作为通用产品能力进入 `templates/` 并同步 `.codex/` dogfood，配套受管 Rust 测试、产品版本、CHANGELOG 和生成清单；真实下游升级另行授权。

先前“60～90 分钟”仅为实现与验证的初步估算，不是已获确认的实施预算。恢复此待办时复用本条需求和证据，补齐技术边界、规模及预算，再开始实施；不重复询问已确认的高耗能条件与提醒频率。
