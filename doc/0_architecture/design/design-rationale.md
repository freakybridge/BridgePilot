# 设计原则

> 本文只描述 bridgeforge-codex 当前正式版本的现行设计。历史迁移、Claude 双骨架、
> switch、junction 与版本谱系不属于当前运行时契约。

## 1. 产品边界

bridgeforge-codex 只维护 Codex 协作骨架：

- `source/` 是 Rust 唯一维护源，公共八包经生成器投影到下游 `.codex/hooks/source/`；
- `templates/` 保存下游公共骨架资产，`templates/skills/` 是用户级通用 Skill 的唯一来源；
- `scripts/` 只放工厂发布、同步和校验工具；
- `.codex/` 是工厂自身的 dogfood 投影，不是第二份产品源。

工厂专属检查不得下沉到普通项目。下游也不得反向把业务约束吸收到 Template。

## 2. 指令分层

- 根 `AGENTS.md` 承载全项目常驻红线；
- 嵌套 `AGENTS.md` 承载目录专属红线；
- hook 与 pre-commit 承载可机器判断的约束；
- Skill 承载用户主动调用的流程；
- `doc/` 承载原理、方案、案例和长 SOP。

Markdown `paths:` 不是 Codex 的自动指令路由机制。项目不得建立第二套隐式规则索引。

## 3. 公共资产与项目资产

同一个文件可以同时包含公共区和项目区，但 ownership 必须可解析：

- `AGENTS.md`：公共区由 Template 管理，项目专区逐字保留；
- `.githooks/pre-commit`：受管区由 Template 管理，项目扩展区逐字保留；
- `.codex/hooks.json`：BridgeForge handler 与项目 handler 分开校验；
- BridgeForge 受管 Hook：6 个 handler 直接调用同一个 `.codex/bin/bridgeforge-hook[.exe]`；
  Rust 源码随 Template 下发，安装/升级事务负责构建、自检和收据，日常触发不依赖 Python；
- legacy Rule：`.codex/rules/*.md` 逐源文件迁往 `AGENTS.md`、`.rules` 或文档；禁止改名冒充原生加载。
- 项目 Hook：每个 Hook 独占 `.codex/hooks/project_XXXX/`，入口为受管 Rust binary，
  私有代码、配置和数据都放在同一目录内；
- legacy 项目 Memory：逐源文件迁往 AGENTS、Skill、Hook/test、Delivery/Bug/TODO 或文档；三个派生文件固定退役。机器不做语义判断，只执行经用户确认的 manifest。

版本分类比较 ownership projection，而不是只看文件路径。项目区发生变化就属于业务变化；
公共区发生变化才属于骨架变化。

## 4. Dynamic latest current-only

### 4.1 旧项目接入

任意合法旧戳只证明骨架身份，不选择旧实现；无戳但已有骨架资产进入 adopt。Planner 只读盘点项目资产，生成临时 `PreservationManifest` 与 Rule / Memory inventory；所有需要用户决策的项目必须明确选择，未选择不得默认为删除。

重装不解析旧 `.codex/managed-skeleton.json` 恢复 ownership。未被当前合同覆盖的普通文件逐路径确认保留或删除；链接、危险 Hook 包与未知 Rule 格式阻断。需要执行的旧 Hook 必须先整理为自包含目录，再重新规划。

清单只服务本次升级：Apply 重新核对指纹、安装当前骨架、回灌确认资产、完成全部校验并
写入最终版本戳后，清单即失效，不作为长期迁移状态保存。

### 4.2 同版本更新

同版本项目需要修复时，目标仍来自可信产品 home 的 current-only 合同。整文件覆盖需要确认；双戳、非法身份和计划后的文件漂移必须阻断。事务允许版本戳内容不变，并在最后核验完整基线和唯一版本戳。

旧文件名在 latest rebuild 事务内删除并最终只写当前戳。双戳、非法戳和高于当前产品的版本仍然阻断。

当前合同未覆盖的旧文件不能依据旧 manifest 或旧 hash 自动删除。同步器只执行用户按精确路径确认的保留/删除决定；未决定时不写入。旧 Rule／项目 Memory 的语义迁移已退役：兼容更新保留，重建按普通项目自有资产处理。

## 5. 事务与失败边界

- Planner、`--check` 与 pre-commit 必须零写；
- Apply 在任何写入前重算输入指纹；
- latest 基线、迁移写入、已确认源删除和 Git index 都纳入同一事务；
- 可捕获失败恢复本轮写入，并保留用户原有 staged/unstaged 边界；
- 版本戳最后写；失败不得留下“已升级”假象；
- hook、JSON、路径和合同解析均 fail-closed，禁止用宽松 fallback 吸收损坏状态。

pre-commit 只验证当前工作树和 index。需要重建 manifest 或版本文件时，用户
先运行 repo-local `$git-sync`，在仓库外先完成版本投影、锁定 Cargo 构建和自检，再由单一写事务安装版本、manifest、Hook / CLI 和实测收据，并暂存可跟踪文件；二进制与收据留在被 Git 忽略的本地运行目录。提交前失败时恢复这些资产及完整 index；pre-commit 不在 `git commit` 内构建或暗写。

## 6. 工厂与 dogfood

工厂身份由严格校验的 `source/forge/bridgeforge-manifest.json` 主张，并要求 Template 合同、
入口 Skill 与项目同步器三项 factory witness 同时存在。单独复制任一 witness 或提供畸形
manifest 都只能得到身份不一致，不能绕过普通下游必须具备的当前版本戳。

Template 与 `.codex/` 的公共运行时必须保持当前投影一致。校验通过合同和当前文件直接比较，
不保留逐版本 hash 谱系，也不向下游复制工厂专属检查器。

## 7. 文档与项目知识

项目文档固定使用 `0_architecture / 1_delivery / 2_bugs / 3_reference / 4_archive / 5_project_knowledgebase` 六层结构，
`doc/README.md` 是唯一索引。活跃架构文档只描述当前行为；历史决策留在交付记录、Bug、
archive 与 Git 历史中。

新骨架不再创建或运行项目 `.codex/memory/`。下游既有目录必须逐文件扫描、人工确认并在一次事务迁往正确资产；确认同时授权删除对应源，不另建清理任务。Codex 原生
`~/.codex/memories/` 保留官方生成和注入机制；summary 可只读检索和阅读正文提出建议，同步器只做不透明整树同步，见
`codex-native-memory-sync.md`。项目 Skill 保存在 `.codex/skills/<name>/SKILL.md`，必须满足
当前 metadata、结构、引用和体积规则。

## 8. 有意不做

- 不自动迁移 `.claude/`、旧 ledger、旧用户目录或任意未知项目结构；明确纳入合同的 `.codex/rules/*.md` 与 `.codex/memory/**` 由用户逐项确认迁移；
- 不维护每个历史版本的 adapter、hash 表或 retirement 表；
- 不执行目标项目自带的旧校验脚本来证明兼容性；
- 不解析任意 shell 命令猜测项目 Hook 的动态依赖；
- 不自动 Git commit 或 push；
- 不用工厂扫描器监管全部下游项目。

这些边界减少活动部件和派生状态：无法由当前合同与明确 ownership 证明的情况，停下来让用户
处理，比继续堆兼容分支更安全、更容易删除。
