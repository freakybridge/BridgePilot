---
lifecycle: completed
validation_status: verified
---

# BP 源码根迁移与公共目录规则补强

## 目标与授权

2026-11-02 用户要求完整修改和迁移计划，随后明确“开始吧。其他对话框已经完成工作了”。本轮实施范围为：纠正本机正式开发仓库位置、保护全部已有开发现场与运行资料、补强公共 AGENTS、同步工厂 dogfood、修正必要路径和说明、完成必要验证及独立复核。预估 60–90 分钟，必要重建时预留至 120 分钟；此为估算，不冒充实际工时。

不提交、推送或 release，不自动升级用户级安装、下游骨架、私人档案或任务状态。不从迁移授权推导恢复副本删除授权。原有 BP 安装/清理/源码更新相关开发改动保持原来源，不算本轮新增实现。

## 现状、归属和目标

- 迁移前外层 `D:\Quant\BridgePilot` 只有 `.runtime/` 及本地扩展 `source/orchestrator/skills/bridgepilot-git-sync/`，不是 Git 根。
- 完整仓库位于 `D:\Quant\BridgePilot\.runtime\bp-source-development`。origin 为 `https://github.com/freakybridge/BridgePilot.git`，分支 `main`，HEAD `da0a00a33e9e835ba1ab367ebd0f38982c729ead`，主程序版本 `16.1.0`。
- 内层存在未提交及未跟踪开发内容，涉及 BP CLI、install、Skills、测试、锁文件与版本合同；须完整保护，不通过干净 clone 或 reset 替代。
- 产品已合仓：`source/forge/` 和 `source/orchestrator/` 平行，共用根 Cargo workspace。问题是本机源码根选择错误，不是尚未合并 BP/BF。
- 当前安装 home 为 `C:\Users\bridg\Documents\Codex\BridgePilot`，档案工作区由其配置解析为 `C:\Users\bridg\Documents\Codex\BridgePilotWorkspace`；两者保持独立，不搬入开发仓库。

目标为 `D:\Quant\BridgePilot` 本身成为 Git 根，直接包含 `.git/`、根文档、Cargo 清单、`source/forge/`、`source/orchestrator/`、`scripts/`、`doc/` 及三类产物目录。

## 影响文件及传播边界

| 项目 | 修改与验证 |
|---|---|
| `source/forge/templates/AGENTS.md` | 公共规则事实源，明确正式开发根不得进入产物目录，允许受控依赖、临时测试与冻结恢复副本 |
| 根 `AGENTS.md` | 由工厂生成公共区，保留项目专区，不手工维护第二份公共规则 |
| 根 `bridgeforge-manifest.json`、`.codex/managed-skeleton.json` | 按实际生成差异更新受管源哈希与投影合同，不手改通过收据 |
| `INSTALL.md`、`README.md` | 安装恢复边界只在 INSTALL 维护，README 导航；不把脏开发仓库交给自动源码更新脚本 |
| 本文及 `doc/README.md` | 唯一交付记录与索引，保存实际状态、证据、恢复方式 |
| 外层 `bridgepilot-git-sync` 两文件 | 合入正式源码树，保留当前 local-extension 身份，不擅自扩为内置安装器功能 |
| 旧路径实际调用方 | 仅修正实际生效引用；历史日志、收据、旧交付正文保留当时事实 |

传播四问：公共 AGENTS 为通用产品内容；本机迁移为自身工作区整理；INSTALL 与交付记录为说明。公共变更同步本仓库 dogfood；不在本轮分发到用户级安装或真实下游。子包是否受影响由受管预览和实际差异判断，不手工升版或要求无关包清零；主程序发布另需明确授权。

## 执行步骤与完成条件

| 编号 | 优先级 | 动作 | 完成条件 |
|---|---|---|---|
| M01 | P0 | 核对其他聊天、活动进程、Git 身份及事务状态 | 相关写入停止，无须核销的进行中事务 |
| M02 | P0 | 保存 Git、内容、属性、外层扩展及安装绑定基线，建立恢复归档 | 归档逐项可读取并通过哈希核验 |
| M03 | P0 | 生成逐文件映射和冲突清单 | 无未解决的同名内容差异，路径不越界 |
| M04 | P0 | 复制完整仓库及运行资料，迁移构建缓存 | 文件内容与 Git 现场保持，外层内容不覆盖 |
| M05 | P1 | 补齐空目录、目录属性并复核权限 | 无空目录遗漏或自定义 ACL 损失 |
| M06 | P1 | 核对生效路径、Cargo 包图及必要工具 | 新根可解析，现有安装与档案绑定保持 |
| M07 | P1 | 修改公共模板、生成 dogfood 与合同、同步文档 | 公共区与投影一致，项目专区保留 |
| M08 | P1 | 定向验证、受管范围预览、独立复核 | 有真实证据，不将预览或退出码替代完成状态 |
| M09 | P2 | 将旧仓库冻结为恢复副本，说明恢复方法 | 旧开发路径不再使用；不未经授权删除 |

外层和内层 `source/`、`.runtime/` 按相对路径合并。目标仅一侧存在则保留；内容相同可复用；内容不同、链接目标未知或占用时停止该项覆盖。枚举外层 `.runtime/` 必须排除内层旧仓库及迁移证据自身，避免递归复制。

## 验证与回退

先证明纯迁移等价，再修改规则。迁移前后核对 HEAD、分支、远端、refs、stash、完整索引、原有修改和未跟踪文件；新增加的外层扩展单独计入差异。受保护文件逐项 SHA-256 核对；目录保留空节点、属性，权限独立检查。构建缓存不重复全量哈希，不作为源代码正确性证据。

验证最小集：根定位与 Cargo metadata；公共指令区既有回归；manifest/dogfood 一致性；实际 Forge/BP 入口 smoke；外部安装绑定摘要不变；本次影响范围的受管 develop 预览。没有 Rust 行为修改时不默认执行全 workspace 冷构建、所有下游 fixture 或真实业务启动。实际产物身份失效时只按既有受管入口准备，不更改历史收据或覆盖版本账本。

失败时停止进一步写入，保留新旧两侧现场。切换前直接继续使用原仓库；切换后根据迁移清单恢复路径，复核原基线。规则修改单独保留补丁，回退不能覆盖用户已有改动。恢复归档是最后保障，不将 zip 当作活动源码工作区。恢复旧构建路径只在对应目标未占用且已核对原始绝对路径后执行；不自动执行 reset、stash、清库或重新安装。

## 本机实施证据

以下证据位于忽略目录 `.runtime/tests/bp-source-root-migration/20261102/`，不作为私人档案或第二套任务库：

- `git-before.json`、`worktree-before.patch`、`index-before.patch`：迁移前 Git 与开发现场。
- `inventory-before.json`：16,839 个原仓库文件和两个外层扩展文件；同名内容冲突为 0。
- `protected-source-before.zip`：源码与 Git 元数据恢复归档，878 个仓库文件加两个外层文件逐项解压流哈希核验通过；归档 SHA-256 为 `5AE66B57BC579D017CA2D1D64F51514305ED97754C383C11410BCA8085AA9787`。运行资料原副本另外保留，归档不包含构建缓存。
- `migration-receipt.json`、`git-after-copy.json`：16,839 文件复制后哈希一致，外层两文件保留，Git 身份、索引及原有工作区差异保持；`.build` 同盘迁至根目录。
- `directory-receipt.json`：9,823 个目录检查，补齐 699 个缺失目录（含空目录），恢复 451 项目录属性（包括 `.git` Hidden）；无重解析点。原始收据的 `empty_directories_created` 字段实际统计全部补建目录，保留原收据并在独立复核记录中澄清；脚本后续字段已更正为 `missing_directories_created`。
- 独立 ACL 检查：非 runtime/build 的 1,158 项、runtime 的 25,504 项均无显式 ACE 或受保护 DACL；最终 1,158 项完整 owner/group/DACL SDDL 源/目标一致。9,823 个目录属性全部一致，594 个实际叶子空目录完整保留。
- `cargo-metadata.json`：workspace_root 为正式根，15 个 workspace members，输出目录位于 `.build`。
- `development-before.json`：迁移后、规则修改前，选定 factory 范围返回 `task-development-current`，无待收口项；不代表全仓已发布就绪。
- `external-bindings-before.json`：安装 home 配置与两个用户级 Skill 绑定的摘要，结束前再次核对。

## 完成结果与验证边界

本轮目录迁移及规则修改已完成，尚未由用户验收。正式 Git 根为 `D:\Quant\BridgePilot`，`source/forge/` 与 `source/orchestrator/` 直接并列；原 HEAD、main、origin、refs、stash 与暂存条目保持。Git 在新路径下自动刷新索引 stat 缓存导致 `.git/index` 字节变化，但 `git ls-files --stage` 与迁移前完全一致；未暂存原有代码、版本合同或账本。

公共规则只修改模板事实源，再运行现有 `bridgeforge-factory manifest --root .` 生成根 AGENTS 与两份受管清单；项目专区与原文精确一致。旧路径未见于 `source/`、`scripts/`、`.codex/`、`.githooks/` 的实际入口，历史记录及冻结副本保留原路径事实。

实际检查如下（原始结果均在本次证据目录）：

| 检查 | 结果与范围 |
|---|---|
| 精确公共指令区回归 | `cargo test --locked --offline --manifest-path scripts/tests/Cargo.toml --target-dir .build/bridgeforge/integration-target distribution_regressions::public_instruction_region_matches_template_and_dogfood -- --exact --nocapture`；实际 1 项通过，98 项过滤；为新路径必要重编译约 86 秒，不声称全量测试通过 |
| `manifest --root . --check` | status=ok、changed=false |
| `check factory-version --root .` | 16.1.0、healthy=true、issues=[] |
| `build-assets --factory --project-root . --check` | status=current，read_only=true，reused=3、built=0；未运行产物自检，不冒充重新构建 |
| 公共 CLI baseline | state=clean；检查受管内容及入口身份 |
| 公共 CLI project-structure | errors=[]；既有归档提示与产物排除提示不扩为本轮清理 |
| BP 源码产物 smoke | `bp self-test` 返回 program=bridgepilot、version=0.12.0；launcher 自检返回 protocol=1 |
| 已安装 BP 只读状态 | install-status=installed，skill-status=user_skill_current；未升级实际 0.10.0 安装 |
| 外部绑定保护 | 六份安装 home 配置及 Skill 绑定摘要全部不变 |
| 本次范围受管预览 | `develop --preview --packages factory` 返回 task-development-current、affected_packages=[]；本轮无须子包升版，根 VERSION 保持 16.1.0；全仓发布就绪不在检查范围 |
| 独立复核 | 878 个原非 runtime 文件保护检查、两份扩展哈希、目录/ACL、逻辑索引、AGENTS 分区与链接均通过；仅计划内七个原文件发生本轮内容变化，另有本文新增 |

## 冻结恢复副本与后续使用

原 `.runtime/bp-source-development` 已同盘移动到 `.runtime/tests/bp-source-root-migration/20261102/frozen-source/`，不是删除。原运行资料也保留在该副本；原构建目录已迁至正式根 `.build/`。`frozen-recovery.json` 记录旧路径、恢复路径及正式开发根。

冻结后再次验证：旧路径确实不存在，Git 根为正式根，`build-assets --check` 仍为 current、最终 manifest check 为 changed=false、BP self-test 正常，`git diff --check` 无错误。根 Cargo 清单、锁文件、VERSION、既有版本配置及收口账本共五项与冻结原副本哈希一致，逻辑索引条目仍与原基线完全一致。

冻结副本只用于回退，不继续开发、不登记为项目、不运行其中旧路径脚本。源码及 Git 另有上述已校验 zip。删除冻结副本或归档须另行明确授权，本轮未删除；无需为了使用新根而先清理它们。

如需回退，先停止正式根的写入并单独保护本轮新规则与文档差异；核对恢复目标不存在后将冻结副本迁回原位置。原有源码及 Git 由冻结副本或 zip 恢复，构建缓存按原先绝对路径映射迁回或经受管入口重建；逐项核对原基线，不对新根执行强制清空、reset 或覆盖已有改动。只有确认恢复入口可用后才切换活动开发根。

本轮不修改用户级 Skill 安装、私人 SQLite、项目任务状态、原生 Memory 或真实下游；不提交、推送、release。公共规则仅完成产品源码及自身 dogfood 更新，实际下游和用户级规则传播尚未执行。后续从正式根继续 BP/Forge 开发，仍须读取真实 AGENTS 和保留的既有工作区改动。
