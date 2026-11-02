---
lifecycle: active
validation_status: in_progress
---

# 四个下游骨架升级与 Release Plan

日期：2026-11-01。本记录承接本控制对话，是本轮 DR 汇总、决定、现场证据与后续发布计划的唯一记录。方案确认、实施、工程验证、用户验收和正式发布分别记录；本次更新只落盘计划，不实施或发布。

执行追加（2026-11-01）：用户明确“计划新增：本次直接将 forge 版本号升级至 14.0.0，然后开始推进”。本计划进入实施，Forge 主程序正式发布目标固定为 **14.0.0**，由受管 release 事务写入，不手工改 `VERSION`；原四下游发布授权继续有效。此次开工承接计划内 DR-04、DR-05 专项与 DR-06 源文件仅去 BOM 的候选处理，保护正文、项目定制、已有未提交改动及 stash；不新增兼容代码、不改发布门禁或夹带原有 BP 私有改动。以下待确认措辞作为计划生成时历史边界，已由本次明确开工及 Forge 版本决定覆盖的部分无需重复询问；新风险仍先解释。

### 执行进度（尚未完成发布）

- Forge 最终受管开发收口已完成：`develop --apply --packages skeleton,lifecycle --analysis-file .runtime/tests/forge14/develop-analysis-final-scope.json` 返回 `development-closed`，最终候选 executed=8、reused=0。收据为 `.runtime/tests/forge14/develop-apply-final-scope.json`；实际子包 skeleton `0.5.2 → 0.6.0`、lifecycle `0.8.3 → 0.8.4`，checks/factory/workflow 仅刷新必要依赖覆盖、版本不变。缺稳定合同，未自动升至 1.0.0。主程序仍为 13.1.0，尚未发布 14.0.0。
- 八项受管检查为 checks-unit、factory-unit（manifest 过滤）、factory-distribution、lifecycle-unit（attribute_ 过滤）、skeleton-unit（attribute 过滤）、workflow-unit（project_sync 过滤）、workflow-settings-preservation、workflow-hook-independence。均保留已登记命令边界，过滤检查实际执行数大于零；前轮未修改产品源码的完整 release 模块已执行通过，不能将前轮整体失败记为收口成功。真实 14.0.0 下游安装和 runtime 仍未验证。
- CRS 独立候选已通过受管迁移精确新增 `.gitattributes` product_inputs，返回 migrated、version_changes=false。程序执行前后解析比较证明除此之外的原配置完全一致；候选收据在 `.runtime/tests/forge14/checkouts/crs/.runtime/tests/forge14/ownership-apply.json`。没有复制原工作区的三目录迁移或业务配置差异。
- 下游发布安排新增一个待选项：鉴于 Stratus 已提交但尚未正式发布的依赖精简无法在现有 master 发布流程中排除，已询问“Stratus 暂缓、其余三项目只发骨架”或“四下游统一暂缓”。未收到选择前不发布任何下游；Forge 自身 14.0.0 及已授权骨架升级继续。
- 用户追加确认：先核实仅发布本次骨架升级及必要适配的可行性，原有业务改动保留且不纳入。本轮不继续为原有业务变化执行 DR-05 或 DR-07 的全包收口。原工作区、已有提交与 stash 均保留。
- 应用 create-worktree 工具拒绝为未列入本聊天工作区的三个下游创建工作树，未产生目标工作树。改为在本工厂忽略目录 `.runtime/tests/forge14/checkouts/` 创建 CRS、Stratus、BL 的独立干净临时克隆，原仓库工作区不变；远端地址采用各原仓库已核验的 origin。
- 官方 13.1.0 CLI 对干净临时副本的只读发布预览：CRS、BL 为 `nothing-to-release` 且 affected_packages 为空；Stratus 仍触发 DR-05。Stratus 当前 HEAD `8eefef3369e354dbdf9ade87d02c534265239858` 在最近 VERSION 变化提交 `809cfa92` 后还有一个依赖精简提交，涉及 34 个业务路径。因此仅隔离未提交业务改动不足以使当前 master 的正式发布仅包含骨架；不擅自删除该提交、重切正式发布基线或改推其他发布分支。此结论是旧工具及未安装 14.0.0 的前置核查，不替代升级后最终预览。
- Forge 定向受管收口曾在 workflow-unit 的 `r05_render_asset_special_policy_precedence` 失败并自动回滚版本。原因是旧测试复用了前段 JSON 源作为属性模板，且仍期待旧追加顺序；不是已证实的真实项目规则误拒绝。只修正夹具为合法默认 LF 模板并对应“公共规则在先、项目规则在后”的结果，不放宽产品校验。此前同轮 checks/factory/distribution/lifecycle/skeleton 已执行的检查不能冒充整轮收口成功。
- Forge 已在受管隔离工作树 `C:/Users/bridg/.codex/worktrees/forge-14-release/BridgePilot`、分支 `codex/forge-14-release` 实施公共规则、合并与静态校验、发布归属投影配套，正在受管构建。原工作区 BP 私有改动未复制进入候选；本轮不是恢复 DR-01 的旧名兼容代码。
- 传播四问：本次属性策略属于产品层通用能力，需要受管子包收口、主程序 14.0.0 与 `[product]` 发布说明；公共模板、合同及本工厂 dogfood 必须同步。计划/现场记录属于元文档，原 BP 自身配置不纳入。
- CRS 归属迁移已返回 `migrated`，仅新增 `.gitattributes` 产品输入，其他配置与原件逻辑一致、版本仍 3.2.3；收据在该项目 `.runtime/tests/forge14/ownership-apply.json`。首次 apply 缺 `--confirmed-choice` 在写入前拒绝，补齐已有 independent 策略确认后成功，没有改变策略。
- BL 已仅移除活库源文件三个 BOM 字节，7028→7025 字节；正文原始字节完全保留，项目 Hook snapshot 后镜像与源一致，SHA256 为 `5D1A7D44C3DBF02C5B18F3EAB9F1B2D4FFCC59C6165915D3F8EE2C6FB1084B5C`。原始备份为该项目 `.runtime/tests/forge14/06_projects-before.md`。未手改镜像、未提交发布；直接 GUI Hook 调用未捕获退出码，仅以退出后真实源/镜像摘要证明该文件已同步，不冒充完整 Hook 事务收据。
- Stratus 当前受管预览仍为 51 个待收口身份：41 个缺记录、10 个内容变化，含两个退役 UI 身份；原始预览为该项目 `.runtime/tests/forge14/development-preview.txt`。已找到此前三目录迁移的收口分析与失败日志，未重绑过期 fingerprint；旧尝试曾因 market-loader 一项 TCP listener 测试失败而回滚。正在做单项现状核验，未改业务代码或触发全量重测。
- Stratus 单项现状核验已复现失败：1 项执行失败、231 项过滤，原始结果在该项目 `.runtime/tests/forge14/market-loader-targeted.log`。不改变代码的独立 Windows API 内存探针连续三次请求 IPv4、`TCP_TABLE_OWNER_PID_LISTENER=3`，均返回 state 0/1/2/6（计数 23/9/8/9）；这与监听表应提供监听端点的预期不符，底层原因尚未确认，不能为放行而删除失败保护。DR-05 不再只有记账缺口，实际验证也有阻断；尚未修改该项目业务/监控代码。
- CRS 归属修复后首次走到子包收口门禁，新增 DR-07：43 包提示缺口，其中 18 个 `content-changed`、25 个 `coverage-mismatch`（build 或 dependencies）。诊断在该项目 `.runtime/tests/forge14/release-preview-after-ownership.stderr.log`。当前 requirements.txt 是显式全局 build_inputs 且有 1 行替换，桌面包也有源码变化；未证明具体记录失效均由这两项导致，不推定需要全部重测。用户尚在澄清含义，未确认额外业务收口范围，保持只读核查。
- Forge 首轮受管检查因新工作树缺声明的测试日志目录而在列举测试前退出，版本已回滚；创建 `.runtime/tests/development/` 后重试。随后复核发现候选把严格只读语法限制带入普通检查，会缩小原有 Git 属性支持，因此中止本任务进程并通过 `develop --recover` 得到 `development-recovered`。修正版恢复普通检查完整 Git 语义，只读仍静态证明；补充宏/binary 不退化及 Git 属性对照用例，重新准备工具。前轮未完成的五包收口不算成功，未写主程序 VERSION、未提交或发布。
- 修正版首次完整收口在 `factory-distribution` 失败并自动回滚：分发测试夹具构造的 PowerShell 临时路径达 278 字符。只缩短测试目录 `distribution-regressions/bfc-distribution-*` 为 `distribution/d-*`，没有改正式安装器。针对性复测四项分发回归全部通过。另补齐“项目原有一条/两条相同批处理规则被公共合同接管”的归属边界，避免误算为业务变化，同时保留其他项目规则改动识别；两项归属测试通过，九项属性/普通 Git 宏与 binary/严格只读/独立 Git 对照测试通过，共 15 项定向通过。源码已冻结，仍须最终受管五包收口及发布证据，不把定向通过当作 development-closed。

## 当前边界

DR-01～03 的原授权为“将三个问题一并修复，然后开始正式升级骨架并release”；四个下游已完成官方 13.1.0 安装，HoldemTrainer 已完成一次受管 release 同步。DR-02 继续并入 DR-01，不单独修改风险策略，不重新执行已经完成的一次性重装。

后续决定为：DR-04 的公共方案已确认，只吸收 `.bat`、`.cmd` 的 CRLF 例外，保留默认 LF；DR-05 按 StratusAgent 专项处理；DR-06 暂认定为 BridgeLibrary（BL）专项问题，尚不能将分类确认当作去 BOM 方案已获实施确认。用户最新要求是汇总并生成 release plan，本轮只修改本交付记录和文档索引。

DR-04 引入此前范围之外的新 Forge 产品改动。计划将 Forge 正式发布列为公共规则官方下发的前置依赖，但不能从已确认公共方案或原四下游 release 授权推导出 Forge 发布授权。此前排除 Forge 发布的决定与本次新增范围需要在执行前明确衔接；不预写主程序或子包版本，不把未发布规则塞入已发布的 13.1.0，也不夹带当前仓库已有的 BP 私有改动。其他已有效的专项与四下游发布授权继续保留，不重复确认。

### DR-01/02 的 Forge 升版范围纠正（历史阶段）

用户指出本轮没有需要升级 Forge 的产品改动。以此纠正执行解读：DR-01/02 为一次性维护方案，不新增强制最低版本校验，不发布 Forge 新版本；本次正式安装目标为已发布的 **13.1.0**，release 范围为四个下游。此前把维护范围决定扩成产品代码门槛属于 Agent 扩大范围；本轮新增的源码、测试和公共说明已经撤回，受管镜像及工具恢复完成。根 VERSION 仍为 13.1.0，source/forge、公共投影、产品清单与已安装合同没有本轮遗留 diff；manifest --check 与 build-assets --factory --check 均通过。此前已有的 BP 变更继续保留。原候选门槛测试仅为历史过程，不作为最终产品改动或发布依据。

DR-03 已在 Causis 根 `product_inputs` 中移除三条重复登记，其余配置逻辑一致；原字节备份位于该项目 `.runtime/bridgeforge-codex/upgrade-20261101/configuration-before.json`。受管预检已恢复 ready，官方 13.1.0 dry-run 无 blocker/gap，仅余与其他项目相同的 58 项公共覆盖确认，纳入 DR-01/02 一次性处理。

## 问题与决定

| 编号 | 类型与范围 | 事实与方案 | 当前状态 |
|---|---|---|---|
| DR-01 | 共性：Forge 更名衔接与一次性维护 | 以已发布 **13.1.0** 为本阶段维护目标，不保留旧名兼容代码，也不新增产品强制版本门槛；四个 13.0.3 下游经逐项归属核验后退出旧公共资产，再从官方产品 home 受管安装。13.1.0 是本轮维护目标，不宣称已实现永久最低版本硬门槛。 | 四项目安装完成；本项不需要 Forge 升版或发布。 |
| DR-02 | 共性风险提示：三个初始预检项目及修复配置后的 Causis | 58 项公共文件替换均匹配原安装记录，按用户决定并入 DR-01。通过一次性退出旧公共资产后新安装完成处理，没有修改 risk 判断逻辑。 | 已并入 DR-01 完成，不单独实施产品改动。 |
| DR-03 | 项目特有：CausisRiskSuite | 保留 `scripts/`，仅移除根 product_inputs 的三条重复文件登记，不删除实际文件；其余配置逻辑完全一致，保留原始字节备份。 | 已修复；受管 preflight ready，升级预检无 blocker/gap。 |
| DR-04 | CRS 发布归属缺口，并确认公共规则反哺 Forge | 原阻断是 `.gitattributes` 未归属发布输入；CRS 当前文件与原骨架一样只有默认 LF，Git 差异为删除五条项目附加规则，不能据此认定误改。后续核验确定骨架只吸收 `.bat`、`.cmd` CRLF 例外，不统一吸收 `.ps1`、`.vbs` CRLF 或 `.githooks/*` LF。公共传播和 CRS 发布归属是两个均需完成的子项。 | 公共实现与候选镜像已完成，最终受管收口未完成；Forge 正式目标已授权 14.0.0。CRS 原工作区归属迁移已完成，临时发布候选仍须按已提交配置单独适配。 |
| DR-05 | StratusAgent 专项发布阻断 | release-preview 报若干包缺版本覆盖记录或内容已变；原工作区配置登记 49 包，账本 61 条记录仅涉及 10 个包 ID。干净 HEAD 副本仍包含上次正式发布后已提交的依赖精简，不能通过隔离未提交改动排除。 | 用户最新要求排除原有业务改动，故不继续补做该业务范围的 develop 或修复。Stratus 的骨架升级继续，当前 master 仅骨架 release 不可直接执行；下游统一/部分发布安排待选。无证据表明需改公共骨架。 |
| DR-06 | 暂定 BL 专项编码阻断 | pre-commit 曾拒绝 `vault-mirror/UserProfile/06_projects.md` 的 UTF-8 BOM；活库源文件已仅去 BOM，正文原字节保留，项目 Hook 已生成同摘要镜像，详见上方执行记录。 | 修复保留在原工作区，未发布。仅发布骨架的候选不复制原有镜像内容差异；是否仍阻断以候选提交检查为准。 |
| DR-07 | CRS 原工作区包收口阻断；排除本轮业务发布 | DR-04 原先提前终止发布预览；修复归属后，原工作区门禁继续发现 18 个内容变化及 25 个依赖/构建覆盖不匹配。干净 HEAD 副本在升级前为 nothing-to-release、affected_packages 为空。 | 用户已明确原有业务改动不纳入本轮，不补做 43 包业务收口、不改公共发布门禁；改用独立候选验证仅骨架发布，14.0.0 安装后仍须最终预览。 |

## DR-04 公共规则决定与已完成的只读核验

用户已确认的目标公共规则如下；这是待实施方案，不是当前已安装模板：

```gitattributes
* text=auto eol=lf
*.bat text eol=crlf
*.cmd text eol=crlf
```

| 编号 | CRS 原规则 | 决定及依据 |
|---|---|---|
| GA-01 | `* text=auto eol=lf` | 保留骨架已有默认规则。 |
| GA-02 | `*.bat text eol=crlf` | 吸收为 Windows 批处理兼容例外。Stratus 当前四个批处理均为 CRLF；本轮未执行批处理换行对照运行。 |
| GA-03 | `*.cmd text eol=crlf` | 吸收为同类 Windows 命令脚本例外。微软官方 [Performance Toolkit SDK 配置](https://github.com/microsoft/microsoft-performance-toolkit-sdk/blob/main/.gitattributes) 同样对 `.bat`、`.cmd` 使用 CRLF；这是实践依据，不冒充本机运行实测。 |
| GA-04 | `*.ps1 text eol=crlf` | 不统一加入。[PowerShell 语法](https://learn.microsoft.com/en-us/powershell/scripting/lang-spec/chapter-02?view=powershell-5.1#222-line-terminators) 接受 LF/CRLF，工厂及四下游 106 个现存脚本在 PowerShell 5.1.26100.9818、7.6.5 中以两种换行内存解析均通过。仅验证语法，未执行脚本；多行字符串保留实际换行，不能把语法通过当作完整行为等价。 |
| GA-05 | `*.vbs text eol=crlf` | 暂不统一加入。本机 32 位 Microsoft Script Control/VBScript 引擎对普通语句、注释、续行、条件块四组纯算术样例，以 LF/CRLF 执行共八次均通过。64 位 Script Control 未注册，未宣称该路径通过；未实测完整项目启动或所有宿主。 |
| GA-06 | `.githooks/* text eol=lf` | 不原样加入。Git 对 CRS 的 `HEAD` 历史规则实际解析证明：`.githooks/helper.cmd` 被该规则覆盖为 LF，而 `.githooks/nested/probe.cmd` 仍为 CRLF；目录通配规则范围过宽且非递归。现有四项目 `.githooks/pre-commit` 都已为 LF。 |

核验只读取现有文件、Git 属性与内存样例，没有创建或改写项目脚本、索引或配置。PowerShell 最初一次显式 shell 选择仍实际运行在 7.6.5，随后通过绝对路径真正调用 Windows PowerShell 5.1 完成复核；不把前一次结果重复算作 5.1 证据。

## Release Plan

### 现场基线与发布范围

本次计划更新只读核对了下表本地版本、HEAD、分支、暂存区和 stash。未执行新的 fetch、remote sync、develop、project-sync apply 或 release；远端状态仍须执行时重新核验。工作区条目数包含之前已有的业务变化，不代表本任务拥有全部修改。

| 编号 | 仓库 | 本地分支 / HEAD | 当前主程序 / 骨架 | 当前现场与本轮发布安排 |
|---|---|---|---|---|
| RP-01 | BridgePilot / Forge | `main` / `b447e6954e59244d08e37cdc9d396ca8718767fd` | 13.1.0 / 工厂不写下游版本戳 | 21 项工作区条目，暂存 0、stash 0；原有 BP 私有改动保留。DR-04 新产品变更完成收口后，须以明确范围正式发布，才可按官方渠道下发。Forge 发布授权尚未从原四下游授权自动扩展。 |
| RP-02 | StratusAgent | `master` / `8eefef3369e354dbdf9ade87d02c534265239858` | 1.56.4 / 13.1.0 | 1181 项工作区条目，暂存 0、stash 10；DR-05 专项解决并安装新公共规则后，重新预览并发布。 |
| RP-03 | CausisRiskSuite | `main` / `2b210db28ceba3932d41bd0cc4b70a94ae878d78` | 3.2.3 / 13.1.0 | 275 项工作区条目，暂存 0、stash 3；解决 DR-04 的 CRS 输入归属并安装新公共规则后发布。 |
| RP-04 | HoldemTrainer | `master` / `0d0868820d4a05498c4e895df49f085188f636d1` | 0.49.0 / 13.1.0 | 工作区干净、暂存 0、stash 0；上一阶段受管 release 同步已完成。新增公共规则确实产生差异后再受管同步；不重复发布同一内容，不强行升级业务版本。 |
| RP-05 | BridgeLibrary | `master` / `737e70807e28483b1f55f169542ce6925104904b` | 0.74.0 / 13.1.0 | 120 项工作区条目，暂存 0、stash 3；DR-06 按暂定专项候选处理并安装新公共规则后重新预览。此前拟定 0.74.1 已随事务回滚，不是本计划锁定的版本。 |

Stratus 与 CRS 的工作区条目数已不同于前轮记录，不能猜测变化来源，也不能直接复用旧 fingerprint 或分析文件。已有事实和仍适用的验证可复用，最终范围、输入和受影响包须按执行现场重建。所有主程序目标版本由累计真实变化及受管约束决定，本计划不预先承诺具体新版本；子包版本只在实际需要的 develop 收口时处理，release 不重复升子包。

### 待实施的影响文件

以下为已定位入口，不授权改动所有关联目录。实施时依据实际调用链收敛差异，禁止借机重做发布所有权算法或清理项目业务改动。

| 编号 | 范围 | 预计影响文件 / 入口 | 必须保留的边界 |
|---|---|---|---|
| RF-01 | Forge 公共内容 | `source/forge/templates/.gitattributes`、本仓库 `.gitattributes` | 只纳入已确认三条规则，工厂自身保持按受控属性检出的可验证字节；不强制删除下游已有 `.ps1` 等项目定制。 |
| RF-02 | Forge 传播与校验 | `source/forge/skeleton/src/content.rs` 的 `git-attributes-default-lf` 分支；`source/forge/skeleton/src/baseline.rs` 的 `verify_gitattributes_read_only`、`verify_gitattributes` | 当前合并器只补默认 LF 行；零写入检查只证明简单默认语法。必须同时支持新公共例外及既有项目规则保留，`--check/--dry-run` 零写入，不调用会写临时库的 fallback 冒充只读。冲突不得静默覆盖。 |
| RF-03 | Forge 合同、投影与测试 | `bridgeforge-manifest.json` 的 `root.gitattributes`；`.codex/hooks/source/skeleton/` 对应受管投影；`scripts/tests/unit/core_project_sync_content.rs`、`scripts/tests/unit/core_baseline.rs`；必要的现有同步集成用例 | 保留稳定 asset id、merge ownership、历史哈希；清单、投影和版本仅由受管流程维护。若实际包闭包扩展到 factory/core，按真实受影响项处理，不预设只改一个包。 |
| RF-04 | CRS 发布归属 | `D:/Quant/causis_risk_suite/.codex/bridgeforge-version.json`、项目 `.gitattributes` | 原生所有权入口为 `source/forge/lifecycle/src/release/polyglot.rs` 的 `owner` 与 `schema4/inputs.rs`；候选是经受管配置迁移精确登记 `.gitattributes` 的产品归属，不改历史 baseline，不把源码归入 product_inputs 规避子包检查。 |
| RF-05 | Stratus 专项 | `D:/Quant/StratusAgent/.codex/bridgeforge-version.json`、`.codex/package-closures.json` 及已有验证/交付物收据 | 先核对 49 包与 61 条、10 个包 ID 的记录关系。账本只经受管流程追加；不手填 passed、不从旧安装或 tests 日志推断当前代码已验证。具体业务文件只有逐项查明缺口后才可能进入范围。 |
| RF-06 | BL 专项 | `D:/Quant/BridgeLibrary/vault/UserProfile/06_projects.md`；由项目 Stop Hook 生成 `vault-mirror/UserProfile/06_projects.md` | 写入前重新核验 junction/节点映射并展示仅去 BOM 的差异；源文件正文原字节与权限保留，镜像不手工写入。该候选尚待具体实施确认。 |
| RF-07 | 发布事务 | 各仓库根 `VERSION`、原生产品载体、`CHANGELOG.md`、适用锁与发布账本 | 仅受管事务按实际发布计划写入。BridgePilot 原有 orchestrator/BP 工作区差异不夹带发布，不为文档计划预写版本。 |

### 编号任务、依赖与验收

P0 是执行安全与范围前置，P1 是解除阻断及官方传播，P2 是统一发布与交付；这不是把全部既有业务工作重新列为开发任务。

| 编号 | 优先级 / 依赖 | 动作 | 验证及完成标准 |
|---|---|---|---|
| RL-01 | P0；首项 | 核实实施与发布边界，明确新增 Forge 发布范围及 BL 具体处理确认；执行前更新项目绑定、分支、HEAD、工具能力、变化范围与已有证据。保护 BP 原有改动及四下游全部 stash。Forge 如需隔离，优先复用合适现有 checkout，必要时走受管 create-worktree，不在混合脏工作区直接 release。 | 只读基线与实际授权对应；不可重用过期发布分析。此项不要求重复回答已确认的 DR 分类或公共规则方案。 |
| RL-02 | P1；RL-01 | 实施 DR-04 公共三规则及传播、零写入校验配套；完成真实受影响包的必要验证和受管开发收口。 | 定向覆盖：全新安装、默认 LF 项目增量更新、已有三规则重跑无差异、保留项目附加规则、明确冲突失败、无扩展名 Hook 保持 LF、普通/嵌套 `.bat/.cmd` 有效 CRLF、只读入口零写入与 apply 前漂移保护。公共内容、manifest 和 dogfood 一致；不跑无依据全 workspace 回归。真实下游写入验证只用获授权测试 worktree；正式下发另走 RL-07。 |
| RL-03 | P1；RL-01，可与 RL-02 分别准备 | 核对并按受管配置迁移处理 CRS 的 `.gitattributes` 产品归属，不因模板已管理默认行就认定整个文件差异自动有发布归属。 | 迁移提案精确、无历史重切或其他产品输入变化；新发布预览不再报该输入未归属。预览出现其他缺口时按具体原因另列，不提前宣称 CRS 全部 release 门禁已通过。 |
| RL-04 | P1；RL-01；Stratus 专项 | 逐包归类 `record-missing` / `content-changed`，核对内容、原生依赖/构建输入、交付物与已有受管证据。先复用有效证据；仅对确有缺口的包安排受管 develop，完成必要验证与版本收口后回到 release。 | 输出具体包、变化文件/输入、可复用记录及剩余缺口。不能从记录缺失推定未测，也不能借调整归属吞掉真实变化。发布阶段本身不补跑 develop；现有证据不足、具体行为/范围需改变时先解释再决定。 |
| RL-05 | P1；RL-01；BL 具体候选确认后 | 从活库源只去 UTF-8 BOM，按项目入口由 Stop Hook 生成镜像；不修改正文、规则或编码门禁。 | 源处理前后，去掉 BOM 后的正文原始字节相等；源与生成镜像摘要一致且无 BOM；待提交编码检查通过，其他镜像变化逐项审阅，不把 Hook 全量同步结果自动纳入授权。失败保留活库与镜像现场。 |
| RL-06 | P1；RL-02 收口、Forge 发布范围已明确 | 由主对话使用工厂受管 git-sync release 发布 DR-04 新公共能力；核验实际推送目标后，经官方入口刷新用户级 Forge。 | 发布只含授权差异；主程序与累计子包变化完整报告。真实 commit/远端 ref、正式版本及安装来源可验证，用户级产物身份和收据匹配。不能复用 13.1.0 版本号，不使用未发布本地模板冒充官方升级。 |
| RL-07 | P1；RL-06 | 对四个已登记下游执行新版本 project-sync dry-run，合并共性与项目特有问题；已确认适配完成且 fingerprint 未漂移后，再执行正式升级。 | 安装源/版本明确，公共规则有效，项目定制、Hook、业务资料和历史备份受保护。共性问题回到 Forge；项目问题留项目。新增未决风险先解释确认，不能静默覆盖、重装或回退以绕过风险。 |
| RL-08 | P2；RL-03～05、RL-07 | 主对话对四项目完成受管 preflight、remote sync 和最终 release-preview，核对实际待提交内容、输入归属、包覆盖、必要交付物及编码门禁；生成各自绑定 fingerprint 的发布分析。 | 四项目均达到各自门禁后才开始这一轮下游发布；不承诺零写入预览能证明真实提交/推送成功。remote sync 会写 Git 状态，执行不放在本轮只读规划中。旧指纹不复用，不把完整 develop 清单变为 release 重测清单。 |
| RL-09 | P2；RL-08 四项目就绪 | 按 HoldemTrainer → CRS → BL → Stratus 的顺序，逐仓库走受管 release；无实际新差异按工具 no-op，不制造空提交或业务新版本。 | 每仓库先取得真实受管结果，再一次独立核对固定提交、实际 push 目标、版本、工作区及 stash。Holdem 原成功记录保留，新一轮是否升业务版本由实际变化判定。中途失败停止剩余发布，不自动撤回已经成功的其他仓库提交。 |
| RL-10 | P2；RL-09 | 更新本记录和索引，汇总真实版本变化、提交/远端、DR 状态和未验证项。 | 主程序及自上次正式发布以来全部变更子包的旧新版本与原因完整；分别报告安装成功、提交推送、业务版本发布及未验证 runtime，不把骨架戳当业务版本。 |

上述“分别准备”仅指任务依赖关系，不启动子 Agent、新聊天或 BP 调度。执行始终沿本控制对话与既有授权推进；不因为写入计划自动创建任务、恢复 heartbeat 或更改跨机器控制权。

### 风险、停止点与证据边界

1. **Forge 新范围**：DR-01/02 的兼容候选已经撤回，不恢复；DR-04 是后续新增公共能力，前置发布不能夹带 BP 私有改动。未明确 Forge 发布范围时可继续独立的已授权专项核查，但不发布 Forge 或从候选副本下发。
2. **规则传播**：既有项目附加规则不自动删除；遇到与公共例外不相容的覆盖，展示路径和有效属性后等待决定。不对活跃工作区执行无差别 `git add --renormalize .`、checkout 或清理来强制换行。fixture/测试日志进项目 `.runtime/tests/`，构建产物进 `.build/`。
3. **DR-05 尚未定位完成**：当前只是证据不足的发布阻断，不能标为代码 Bug 或骨架缺陷；若专项调查证明通用误判，先单独解释影响和方案，不借专项授权直接修改 Forge release 算法。
4. **BL 源与镜像**：本次复核两者仍均带 BOM，长度 7028，SHA256 同为 `DBEDADA9BDAF22C09F05097AAA07C66B9C9810AD8C4AE930C2F1B6B0B8D618DF`；这是当前缺陷证据，不是已修复收据。实际写入前仍须核验源身份、权限和当时字节。
5. **跨仓库发布不具备整体原子性**：统一就绪后串行发布以降低部分成功风险，无法保证任何网络/Hook 失败都整体回滚。提交前失败沿工具事务恢复；提交成功推送失败保留提交并报告，不强推、不自动重复完整发布。
6. **门禁真实性**：任一新阻断按共性/专项分类并先解释，不降级普通同步，不跳过 pre-commit，不手工补造收据。必要验证只覆盖实际影响；自然 runtime/完整应用启动没有证据时继续标为未验证。

## DR-01 历史过程与撤回说明

- 用户级 Forge 已经由当时获准的修复入口刷新至官方 **13.1.0**，来源提交为 `b447e6954e59244d08e37cdc9d396ca8718767fd`，产品 home 的 origin 为 `https://github.com/freakybridge/BridgePilot.git`。这次安装已经完成，不因后续暂停而重复安装或撤销。
- 本轮新增的仓库 ID 查询、旧 origin 接受分支、额外旧地址拒绝用例及最低版本产品门槛，均已撤回；兼容修复与门槛候选没有提交或发布。
- 历史候选曾有基础层 13 项、同步事务 50 项通过，也曾因维护锁权限停止。后续恢复了权限并完成受管准备，最终在用户纠正发布范围后再次受管恢复原产品源对应工具；这些过程不等于本轮新增 Forge 产品能力。
- DR-01/02 阶段最终没有遗留 Forge 源码或测试改动，无需为这两项进行 Forge 子包升版或主程序 release。此结论不代替后来确认的 DR-04 公共规则发布计划；目前 DR-04 尚未实施。根仓库原有的 BP 变更保持，不纳入本次下游发布。

## 实际升级与发布结果

安装源统一为官方 13.1.0 产品 home 与其 prepared 公共产物收据。每个项目先以已安装合同的整文件资产及实际哈希证明归属，只移动已证明的公共文件和两份骨架身份至项目内忽略备份，保留项目自有文件，再走官方 project-sync adopt/apply 事务。只对 user-decision 项传保留决定；required-preserve 项由工具自动保留。未新增永久兼容代码、卸载工具或风险判断分支。

| 项目 | 骨架结果 | 保护核验 | 发布结果 |
|---|---|---|---|
| HoldemTrainer | 13.0.3 → 13.1.0，已完成 | 126 项公共/身份文件备份，5 个项目文件哈希及 AGENTS 项目区不变 | 受管 release synced，提交 `0d0868820d4a05498c4e895df49f085188f636d1` 已推送 origin/master；业务版本 0.49.0 不变，无子包变化。独立核验远端同提交、工作区干净、stash 0。 |
| StratusAgent | 13.0.3 → 13.1.0，已完成 | 17 个项目文件哈希不变，AGENTS 无写入动作；唯一额外删除为计划精确识别的公共 .codex/bin/.gitignore，不是项目资料 | DR-05 阻断，业务版本仍 1.56.4，未提交/推送。 |
| CausisRiskSuite | 13.0.3 → 13.1.0，已完成 | 126 项备份，7 个项目文件哈希及 AGENTS 项目区不变；三项登记修复已在安装前完成 | DR-04 阻断，业务版本仍 3.2.3，未提交/推送。 |
| BridgeLibrary | 13.0.3 → 13.1.0，已完成 | 127 项备份（包含精确归属的旧忽略文件），19 个项目文件哈希及 AGENTS 项目区不变 | 原计划 patch 0.74.1，但 DR-06 在提交检查时阻断，已回滚至 0.74.0、HEAD 仍 737e70807e28483b1f55f169542ce6925104904b，暂存区为空；当前 3 个 stash 未操作。不能称已发布。 |

Holdem 首次规划因重复传入自动保留项而停止，已完整恢复后按正确参数成功执行。Stratus 初次外层保护检查误将已获精确归属证明的公共退役忽略文件列作项目文件；原生安装实际成功，随后依据计划与原哈希复核，确认没有项目文件丢失。上述仅为执行参数/核验分类纠正，没有修改 Forge 产品。

安装收据与备份位于各项目 `.runtime/bridgeforge-codex/reinstall-20261101/`；Holdem 最终成功批次为 `reinstall-20261101-02/`。发布预览、分析及实际日志位于各项目 `.runtime/bridgeforge-codex/release-20261101/`。BridgeLibrary 首次分析路径参数在写入前拒绝，改为项目内相对路径后进入提交检查；第二次的 BOM 阻断已由事务回滚，未绕过 Hook。

## 下游保护与证据

| 项目 | 本轮 dry-run 后实际骨架戳 | 原有未提交条目数 |
|---|---|---:|
| StratusAgent | 13.0.3 | 1157 |
| CausisRiskSuite | 13.0.3 | 221 |
| HoldemTrainer | 13.0.3 | 0 |
| BridgeLibrary | 13.0.3 | 96 |

上述表格为升级前预检时点结果；最终安装和发布状态以上一节为准。原始预检与用户级刷新收据位于项目忽略目录 `.runtime/tests/forge-rename/`，历史候选范围预览位于 `.runtime/tests/support-floor/develop-before.json`；这些本机证据不保证随 Git 分发。

后续执行以本记录的 Release Plan 和最新有效授权为准：DR-04 公共方案已确认、DR-05 专项处理、DR-06 暂按 BL 专项且具体处理待确认。已完成的 DR-01～03 与 Holdem 首次发布不重做；新增问题仍先解释。门禁失败不降级普通同步，不绕过 Hook，不把安装成功替代发布成功。


### 后续经验固化：BP 私有 bridgeforge-update（已收口并安装验证）

用户已明确授权：名称为 `bridgeforge-update`，作为 BP 私有 Skill，覆盖整轮升级并复用本次有效 release 授权。本轮在实际 `D:/Quant/BridgePilot` 的 BP 0.7.2 源码完成私有资产登记、BP 主入口路由、项目导航及安装说明；新 Skill 随 BP 包进入 home，由用户级 BP 入口按实际 home 加载，不另行进入 Forge 共享分发。当前 Forge 发布 worktree 的 BP 仍为旧 0.5.0，不用其安装器认证实际 BP。

冲突来源已逐项核对：2026-11-01 20:14 的受管 Git 同步从 `b447e695` 快进到 Forge 14.0.0 的 `45a51bcb` 后，恢复 `codex_git_sync_autostash`（`1e5f5ef40642d9bf6091d2b1a4ab03d2f501550f`）产生四个冲突。各文件 stage 1、2、3 分别与 stash 基线、当前 HEAD、stash 本地文件完全对应；不是新 Skill 引入，也没有证据表明其他对话正在处理。

| 编号 | 冲突文件 | 已核实内容与处理边界 |
|---|---|---|
| CF-01 | `.codex/managed-skeleton.json` | Forge 14 的公共资产更新与本地公共运行时锁文件哈希更新相遇；本地相对基线仅三个锁文件哈希字段变化，不是 BP 业务规则冲突。保留原文件和三阶段索引，尚未重建。 |
| CF-02 | `.codex/package-closures.json` | 基线154条，Forge与BP分别新增5条，形成不同的全局 previous/hash 链。原生校验要求已发布前缀不变，不能拼接或手工重算。两条原链完整保留，尚未受管调和。 |
| CF-03 | `bridgeforge-manifest.json` | 与CF-01同源；不拿旧清单覆盖Forge14，也不丢弃BP本地锁文件变化。原文件保留。 |
| CF-04 | `doc/README.md` | Forge升级索引与BP 0.7.2 SQLite/status交付索引相遇，两方内容都保留；尚未处理索引冲突。 |

为避免覆盖已有BP改动，先将实际807个源文件复制到忽略的隔离验证目录，核验通过后仅同步本轮7个目标文件；其余801个既有文件的字节哈希不变，Git索引、HEAD、stash与四个冲突文件保持原样。同步前备份、精确增量补丁与哈希记录位于当前worktree的 `.runtime/tests/bridgeforge-update/`。没有全文件回退BP代码，也没有切换日常安装。

实际验证：锁定Cargo构建BP 0.7.2源码候选；扩展的安装回归1项通过，覆盖十一资产打包、新Skill字节一致性、安装预览零写入、用户级BP入口完整性、新Skill用户修改保护及篡改payload拒绝；相关 `setup_cases` 7项通过，覆盖入口幂等、来源/用户编辑保护、并发安装日志互斥等。实际旧BP 0.7.2生成十资产包，新候选 `install-preview` 返回 `writes:false` 且目标目录不存在。Skill Creator返回 `Skill is valid!`。原包六/十资产读取边界保留，新增十一资产对应下次受管minor版本；没有手工改包版本。日志为 `portable-tests.log`、`setup-tests.log`，旧包与预览也留在同一忽略目录。

上一轮阻点（本轮已解除）：实际主目录受管 `develop --root . --preview --packages orchestrator` 返回 `[package-development] installed skeleton identity is invalid JSON: key must be a string at line 3322 column 1`，与保留的身份文件冲突标记一致。正式子包收口和日常home安装因此未执行，未提交、推送或发布BP；不能把隔离安装测试当作正式安装，也未执行新一轮下游升级或runtime验证。全包fmt检查还显示既有BP源码格式差异，本轮只修正新增测试的格式，未格式化他人的修改。

### 2026-11-02 正式收口与日常安装结果

> 历史记录：本节中的BF新增能力和两项BF升版已撤回，当前状态以下方“最终结果：仅保留BP Skill”一节为准。

用户明确授权继续安全解决四个冲突、保留BP既有改动与两边收口记录，并完成本Skill正式收口和安装验证。实际开发与收口源码为 `D:/Quant/BridgePilot`；本Forge发布worktree只保存实现镜像、批次记录和本机证据，不用其旧BP 0.5.0安装器覆盖实际BP。

四个索引冲突均已解除：两份生成清单先恢复已发布基线作为渲染输入，再由原生 `manifest` 按当前源码和锁文件重建；文档索引保留BP 0.7.2交付条目与Forge升级导航。HEAD和原始autostash未变，原有40个BP Rust源码文件在本轮正式收口前后逐字节一致。

账本恢复补充了工厂专属 `closure-reconcile --preview` / `--apply --fingerprint <实际预览>` 入口。预览零写入；执行验证三阶段原链与共同前缀，拒绝历史、证据和版本冲突以及指纹漂移。先永久保存三份原始账本、冲突原件的可逆编码及原哈希→新哈希映射，再由Rust重接未发布BP记录的previous/hash，不改版本、内容、检查或证据。没有放宽既有Ledger校验或手工伪造收据。

永久原件和恢复收据位于 `.codex/package-closure-reconciliations/fc76ed8c125fad461a15f5ea08cf665b22c0d120d8415329bcf49c05a981d710/`；原始冲突文本以 `working-conflict.hex` 保存，解码后与原哈希一致，避免Git将备份识别为未清理冲突。活动账本先恢复为164条，正式收口再追加6条，最终170条。独立实现逐条验证整链，确认已发布159条前缀未变、BP五条历史除链指针/哈希外完全一致。

受管 `develop --apply --packages orchestrator,lifecycle,factory --analysis-file .runtime/tests/bridgeforge-update/develop-analysis.json` 返回 `development-closed`，6项最终候选检查全部实际执行，无复用：生成身份、工厂参数拒绝、4项账本恢复、BP portable安装回归、骨架合同、Git同步冲突保护。checks、skeleton、workflow源码未改，仅无升版刷新覆盖记录。

| 包 | 原版本 | 收口版本 | 原因 |
|---|---|---|---|
| orchestrator | 0.7.2 | 0.8.0 | 新增BP私有bridgeforge-update打包与入口路由 |
| lifecycle | 0.8.4 | 0.9.0 | 原生验证并接入分叉账本，保留原件和证据 |
| factory | 0.10.1 | 0.11.0 | 提供受限的账本恢复命令与能力声明 |
| checks | 0.2.2 | 0.2.2 | 构建覆盖刷新 |
| skeleton | 0.6.0 | 0.6.0 | 构建覆盖刷新 |
| workflow | 0.9.2 | 0.9.2 | 依赖及构建覆盖刷新 |

主程序VERSION保持14.0.0，未进行本轮Git提交、推送或GitHub release。新增恢复能力没有下发到其他项目，没有重跑四下游升级。旧版本包读取兼容与用户文件保护继续保留。

最终版本的工厂受管准备返回 `prepared`，CLI、Hook、factory三项产物均完成实际self-test。随后零写入核对：manifest返回 `changed:false`，build-assets返回 `current`、复用3项且未构建；最终develop预览为 `task-development-current`、`affected_packages:[]`，仅证明本轮任务范围已收口，不推导全仓release就绪。工作区与暂存的diff检查通过，未解决索引为空。

BP最终0.8.0 release构建成功，十一资产本地验证包位于 `.build/bridgepilot/bundles/0.8.0-bridgeforge-update-20261102/`，不包含私有档案。先对原home零写入预览，再通过原生install升级 `C:/Users/bridg/Documents/Codex/BridgePilot`，通过skill-install刷新原用户级BP入口。install返回 `installation_current`、`archive_migrated:false`；skill-status返回 `user_skill_current`、`current:true`。独立核验9项受管内容和Skill源码字节一致，原home绑定、SQLite档案和配置等4个文件哈希不变，安装事务无pending。

使用方式：在BP对话中提出“使用bridgeforge-update为指定项目升级骨架”。用户级入口按已验证home读取 `source/orchestrator/skills/bridgeforge-update/SKILL.md`；它随BP管理，不单独进入Forge共享分发。已安装Skill的元数据校验通过；本轮未执行新的真实下游升级、桌面或完整流程行为验收，不把安装检查当作这些场景已通过。

本机原始收据、日志和独立核验位于当前Forge发布worktree的 `.runtime/tests/bridgeforge-update/`，包括 `closure-reconcile-result.json`、`develop-formal-result.json`、`formal-closure-verification.json`、`live-install-verification.json` 与最终工厂准备记录。它们是本机证据，不假定随Git分发；永久双方账本原件另按上文保留。

### 2026-11-02 最终结果：仅保留BP Skill，撤回BF新增能力

本节取代上节联合收口作为当前状态。用户明确要求本次只添加BP Skill，并授权按恢复计划撤回额外BF能力；没有授权提交、推送、发布或新的下游升级。

已核验当时活动账本170条，索引仍保存原生恢复完成后的164条快照，SHA256为 `c9ca9577ef5f6904cccc2a2bd656052ef128b7c54ad0b789c41b00602c26f17b`。现有 `develop --recover` 只处理未完成事务，不能撤销已成功收口。因此本次执行明确授权的一次性未发布快照恢复，没有伪造develop journal、修改Ledger校验或再增加BF长期入口。

一次性Rust工具临时挂入既有锁定workspace构建，使用现有工厂/项目锁、精确11文件白名单、输入哈希、804个受保护文件及逐项备份/回退保护。隔离副本验证了预览零写入、受保护文件漂移时零写入拒绝、正常精确恢复及完整备份；独立Python核验全部恢复和保护哈希。实际恢复完成后，其临时Cargo入口与源文件已撤出生产树，审计副本留在忽略证据目录。未把一次性处理变成新BF功能。

BF源码、模板和专用测试已恢复为已发布提交 `45a51bcb1e0ad95397d1936c597b0568d43443e4` 的内容；实际仓库与发布worktree的该范围Git对比均无差异。`closure-reconcile` 命令、能力声明、恢复模块和新增测试已移除，dogfood中的退役模块也已精确清理。清单、锁文件及投影仍由既有原生入口按保留的BP工作区生成，未回退已有BP依赖和业务内容。

| 编号 | 对象 | 最终状态 |
|---|---|---|
| BR-01 | BP orchestrator | 重新受管收口0.7.2→0.8.0，仅包含BP私有Skill及必要接入 |
| BR-02 | BF lifecycle | 恢复0.8.4，撤回未发布的0.9.0能力 |
| BR-03 | BF factory | 恢复0.10.1，撤回未发布的0.11.0命令 |
| BR-04 | 主程序VERSION | 14.0.0不变 |
| BR-05 | checks / skeleton / workflow | 0.2.2 / 0.6.0 / 0.9.2不变，本次未追加其收口记录 |

本轮还清理了暂存清单里的退役BF模块引用：只同步4个本轮冲突文件及4个新增历史归档，其他既有暂存条目逐条核对未变。

原四个文件冲突保持解决：共同154条、Forge新增5条、BP新增5条组成的164条恢复结果完整保留，随后只追加一条BP原生收口记录，活动账本最终165条。先前170条账本及六条联合收口的原始收据完整封存在同一 `.codex/package-closure-reconciliations/fc76ed8c125fad461a15f5ea08cf665b22c0d120d8415329bcf49c05a981d710/`，分别为 `superseded-development-170.json`、`superseded-development-receipt.json`；一次性恢复收据和说明另存于同目录。旧记录作为历史保留，不充当当前版本证明。已发布159条前缀及其后BP五条恢复记录均未改动；独立核验165条完整哈希链通过。

最终 `develop --apply --packages orchestrator --analysis-file .runtime/tests/bp-only-recovery/develop-analysis.json` 返回 `development-closed`，只有orchestrator一项变化，实际执行一项已登记的安装相关检查（portable用例过滤），未复用。后续原生只读预览返回 `task-development-current`、`affected_packages:[]`，有效范围只有core与orchestrator，不据此声明全仓release就绪。

恢复后的CLI、Hook、factory三项原生产物准备和实际self-test通过；manifest检查为 `changed:false`，build-assets只读检查为 `current`、复用3项且未构建。最终工具不再声明 `closure-reconciliation-v1`，实测调用旧命令被原有usage分支拒绝。

BP现有0.8.0安装没有降级或重装。重新收口记录的content、dependencies和build身份与原已安装0.8.0对应记录完全一致；实际home中新Skill与源码一致，9项受管内容校验通过，用户级入口原生状态为 `user_skill_current`、`current:true`。原配置、SQLite档案等4文件字节未变，40个BP Rust源文件保护哈希未变，安装事务无pending。HEAD、原始stash保持不变，未解决索引为空。

本机演练、一次性计划、原始收据和独立核验位于当前发布worktree `.runtime/tests/bridgeforge-update/bp-only-recovery/`，包括 `rehearsal-verification.json`、`live-restore-receipt.json`、`bp-only-develop-result.json`、`final-verification.json`、`final-bp-current.json`。没有进行新的真实下游升级或完整桌面/升级流程行为验收。当前结果是BP Skill新增与必要接入完成，BF没有新增长期能力；未提交、推送或发布。

### 独立入口交付与用户验收（2026-11-02）

用户要求在斜杠命令清单直接选择 `/bridgeforge-update`，并明确仍由 BP 管理、不进入 Forge 共享分发；本节是该新增范围的当前结果，前文由 BP 统一入口间接加载的说明保留为历史。随后用户显式调用 `$summary accept`，确认本次独立入口交付符合需求，并调用 `$git-sync release` 授权当前项目受管提交、推送和正式发布。用户验收与发布结果分别判断，实际发布以本轮受管收据为准。

实现位于 BP 私有 install 包和两个本地 Skill：`skill-install-preview` / `skill-install` 在同一事务中处理 `bridgepilot` 与 `bridgeforge-update`，每个入口保存自己的 `bridgepilot-local.json`，以真实 home 定位 BP。`skill-status` 逐项报告安装和内容一致性；旧单入口可以原位补齐，旧单入口恢复日志仍可读取。未知归属、用户修改、跨 home 争用和中断后的外部修改均保留；两个入口的中断恢复共用原有锁与日志。新增 `bp-independent-update-skill-v1` 能力，已安装独立入口后拒绝回退到无法维护它的 core。未修改 Forge 公共模板、共享 Skill 登记或公共源码投影。

| 包 | 开发前 | 收口后 | 原因 |
|---|---|---|---|
| bridgepilot-install | 0.1.0 | 0.2.0 | 独立 Skill 投影、双入口事务与校验、恢复及回退保护 |
| bridgepilot-orchestrator | 0.9.0 | 0.10.0 | 声明独立入口能力并覆盖双入口安装生命周期 |

受管入口 `.build/bridgeforge/bin/bridgeforge-factory.exe develop --apply --root . --packages bp-install,orchestrator --analysis-file .runtime/tests/bridgeforge-update/analysis.json` 返回 `development-closed`，最终候选实际执行3项检查、复用0项，`git_written=false`；主程序仍为16.0.0。正式账本 `.codex/package-closures.json` 的本次两条记录 hash 分别为 `sha256:802bf00383ad4068c80555396599407cdb9b7f3219ce55c69cd2b872aaed3af9`、`sha256:707ac4b691ae70091f3e1eab344ff646ff5e757d3845358f914b3cb6161389d0`。版本仍处于0.x，原因是两包没有配置稳定验收合同。

实际验证包括 install 单元5项、`setup_cases::` 安装集成8项和 BP core／launcher Release 构建。覆盖首次安装、单入口补齐、幂等、来源与用户编辑保护、归属争用、两个安装日志的互斥，以及双入口8次文件写入的所有中断边界和外部编辑保护。公共 CLI 的 `check skill-metadata --root . --skill-root source/orchestrator/skills` 返回 issues／warnings均为空；`git diff --check` 通过。未重跑 Forge 全仓或下游业务回归。

本机已通过 BP 自有 `bundle`、`install-preview` / `install`、`skill-install-preview` / `skill-install` 更新 `C:/Users/bridg/Documents/Codex/BridgePilot` 至0.10.0并安装两个用户级入口。安装回执为 `installation_current`、`archive_migrated:false`、`legacy_forge_unchanged:true`，Skill回执为 `user_skill_installed`、`shared_forge_skills_changed:false`；随后 `skill-status` 为 `user_skill_current`，两个入口 installed／current均为true。真实 SQLite 档案安装前后只读检查均为 archive `archive-df1b25cda65f9bf5a0ff8c35`、revision102，未迁移档案或恢复任务／heartbeat。

已通过原生 `skill-probe` 调用 Codex `skills/list`（forceReload=true），在当前项目观察到 `bridgeforge-update` 与 `bridgepilot` 均为 user scope、enabled=true、errors为空，未创建聊天或启动任务回合。原始发现回执保存于 `.runtime/tests/bridgeforge-update/native-discovery.json`；本轮分析及预览同在该忽略目录。该证据证明原生发现与启用，未实际点选桌面命令菜单、执行新的骨架升级或验证完整下游发布流程。

相关 **TODO-050** 仅增加本次独立入口已验收进展，整体总管需求仍为进行中，既有真实冷机、双项目业务发布、物理双机及自然唤醒等未完成范围保持原记录。本次不新增独立待办、不归档历史主题。

可复用经验：源码目录存在 SKILL.md 不代表已注册为独立可发现入口；须经 BP 自有安装、归属与 hash 校验，再用原生 skills/list 验证。受管 develop 的严格环境指纹会使跨沙箱边界的分析失效，本轮重新核对相同两包范围后在同一执行环境生成分析并 apply；没有仅替换旧 fingerprint 或降低检查要求。
