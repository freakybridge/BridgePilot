# 子包开发收口与主程序发布合同

> 开发能力从实际入口收据读取并复用：需要 package-development-v1、package-stability-v1、task-selected-development-checks-v1 和 version-coverage-v1，阶段间复用还需 staged-development-validation-v1，直接 apply 需 direct-development-apply-v1。发布从 git-sync --preflight 的 tool 字段核对 git-sync-remote-context-v1、git-sync-development-closure-v1 和 version-coverage-v1，不独立 self-test。旧工具缺能力时报告维护缺口，不手工改号或伪造记录；开发既有诊断边界不变。

本文是 develop / git-sync 共用的版本判级、稳定条件和机器入口事实源。轻量便捷是总目标：开发完成本次必要验证和版本事务即可结束；发布核验累计变化和版本后完成 Git 同步，不补跑 develop。普通同步不主动升版。

## 1. 版本对象与两个基线

根 `VERSION` 管主程序；`product_manifests` 管整 workspace 产品映射，`product_packages` 登记混合 Rust workspace 中跟随产品版本的载体。独立子包登记稳定 id、原生名称、语言、输入归属和唯一版本来源，允许版本不同。普通目录、第三方依赖和骨架工具不自动登记为业务子包；不新增全仓漏登记扫描，保留已有归属检查。

| 阶段 | 比较范围 | 写入责任 |
|---|---|---|
| develop | 每包上次版本记录与当前完整内容；首次以可证明的已发布基线衔接 | 受影响子包版本、必要原生依赖/锁、追加版本记录 |
| release | 上次正式发布至当前最终状态，含已提交、暂存、未暂存和未忽略新文件 | 主程序 VERSION、原生产品载体、必要锁及正式发布记录 |

两个基线分别管理，不能吞掉累计未发布变化。同一内容不重复升版，无实际变化不因达到稳定条件单独升版。包改名、移动、移除、拆分、合并保留显式身份和历史，不复用退役 id。既有版本不统一重置，新包可从 0.1.0 开始。

语义版本用 MAJOR.MINOR.PATCH，高位递增时低位归零；已发布版本不得覆盖、复用或倒退。Python 保留 PEP 440 语义，不支持的格式报告适配缺口，不猜测转换。Rust/Python 之外的语言不能仅凭文档宣称支持。

## 2. 判级与自动 1.0.0

| 实际变化 | 0.x 子包 | 稳定子包 |
|---|---|---|
| 兼容修复或必要内部调整 | patch | patch |
| 兼容新增或公开接口弃用 | minor | minor |
| 不兼容接口、数据、配置或使用要求变化 | minor | major |
| 无实际变化 | 不升版 | 不升版 |

按公开合同逐包分析，不从提交标题推导。依赖升版不自动带升调用方；源码、有效依赖要求或公开合同确有变化时才纳入。未知兼容性停止相关收口。数字级别与兼容性分别记录，breaking 须有绑定当前 fingerprint、scope、reason、source 的明确接受依据；已有适用授权直接复用。

有实际改动的 0.x 子包只有同时满足以下条件才自动选择 1.0.0：公开合同基线明确；非空稳定清单覆盖 core 和实际调用方/场景 integration；完整稳定清单在最终候选上通过；本次无不兼容变化。不逐包追加批准，缺失、失败、跳过或未知不算通过。

稳定检查失败只让对应包回到 0.x 候选，重新验证最终依赖图并报告缺口。普通任务检查失败则回滚，不完成收口。有限检查不证明没有 Bug，不以开发时长或版本已到 0.9 代替稳定条件。

## 3. 必要检查与短期保护

每个实际待处理包在分析的 `validation` 中选择非空的已登记普通检查，依据本次风险选择并保留项目明确必需的检查。配置清单不自动删减；自动 1.0.0 的稳定清单独立完整执行，不能以普通子集替代。

所选检查保留原 id、program、inputs、artifacts、kind、always_run 和参数。唯一参数扩展是给 Cargo test 在原参数末尾、原 -- 之前追加一个非选项测试名过滤条件，不替换 flags、manifest、package、runner 或原有后缀。超时可缩短不可放宽，不能自行开启 reusable 或关闭 always_run。过滤命令须有可解析且执行数大于零的 Rust 测试摘要；仅列出、仅编译、零测试或无法证明执行均不通过。既有未过滤自定义 harness 保留原退出码/产物合同，不声称自动识别其数量。

命令从项目根执行，真实退出码 0 且未超时才通过，缺失输入或产物不通过。release 的 analysis.checks 声明不能代替 develop 执行。检查输入、稳定合同、必要环境、原始字节和目录成员仍绑定当前严格计划，漂移时停止或回滚。

版本、原生依赖/锁、生成投影和记录在同一本地事务处理，不写主程序 VERSION 或 Git。检查在最终候选执行，交付物对应最终版本，不把升版前二进制标为最终交付。失败恢复保留外部修改，不留半套已登记状态。

## 4. 版本覆盖与历史

`version-coverage-v1` 追加 `evidence_schema=6`，长期身份仅含 content、dependencies、build 三项；检查命令/输出摘要、产物摘要、理由及接受依据保留作历史事实。记录负责去重、版本连续性及累计公告，不是跨环境长期测试证书。

换会话、机器、工具安装或修改测试策略、检查输入/稳定合同本身，不自动重新打开版本记录；本次改动仍按真实影响验证。源码、原生依赖闭包或实际构建输入变化继续要求处理。持久影响构建的工具/配置应按事实登记为包输入或 build_inputs，不能只用检查环境声明表达源码构建归属。

只使用 Git 证明的 CRLF/LF 等价；二进制、-text、特殊编码/ident 及声明产物保留原字节语义。活动 clean/process filter 阻断，不执行项目 helper。build_inputs 包含忽略文件存在状态、目录全部成员及空目录，事务中增删改仍受保护。隐式 Cargo.lock 按实际依赖闭包比较，显式全局输入仍完整核验。

旧 schema 2–5 保持原字节和哈希链，不补字段或重绑旧证明。已有分项只比较可证明的版本覆盖项；旧聚合记录无法分离证明时，经实际必要检查追加不升版覆盖记录。仅已发布且原记录未变的旧聚合记录可在内容、原生依赖及构建身份完全可证明时只读衔接；显式/未跟踪输入不明则停止。未知 schema、链断裂、覆盖缺口、版本倒退仍拒绝。

旧 portable-development-evidence-v1、git-eol-development-evidence-v1、cross-machine-development-evidence-v1 标签退役，不再广告对应长期证书写入能力；历史读取、Git 内容等价及当前显式环境执行保护继续保留，新消费者以 version-coverage-v1 为准。

## 5. release 的最小职责

release 核验累计范围内的版本覆盖、历史、归属和明确交付物，不执行业务测试或补做 develop。缺口时列包和原因并停止，不降级普通同步。仅环境或检查策略变化不强制整包重新收口；项目显式 required_checks/check_scopes 仍按当前验证指纹及真实记录核验，SDK 等显式环境变化使必要发布检查失效。

无分析预览在门禁通过后返回 `version_coverage`（包、版本、账本记录 hash 与证据 schema）和 `applicable_required_checks`（本次适用的检查到对象映射），并明确 release 不执行开发验证。这些字段引用已有事实，不生成新证书；无实际发布变化时不执行发布证明检查。显式 check_scopes 只要求覆盖与本次影响的交集，未声明 scope 的必需检查仍覆盖全部受影响对象；不额外要求所有 scoped 检查合起来覆盖未声明的对象。

记录缺失、旧记录缺少分项、真实内容变化及交付物缺口分别诊断；缺记录不等于检查未执行。已有 validate/apply 证据沿开发合同衔接，release 不追加账本、不导入任意历史日志、不补写 passed。无法证明对应关系时，只报告具体缺口。required_checks 是显式独立发布合同，不能仅凭与 develop 检查同名就自动替代；重复要求应在获授权的项目合同维护中取消或调整，保留确有必要的发布要求。全局 build_inputs 继续按完整输入保护，普通锁文件沿实际依赖闭包核验，不为减少阻断静默取消全局输入。

release_artifacts 只能引用已登记检查产物，表示发布实际依赖的交付物；当前声明新增、缺失或摘要改变均阻断，develop 可实际检查后无升版刷新。普通 checks.artifacts 在执行时核验，不因换机缺少它们否定版本覆盖。新二进制不继承旧二进制运行结论。

主程序按正式发布以来累计影响判级，只升级主程序及原生产品载体，不再次升级子包。低于建议或复用已发布版本拒绝，无发布变化 no-op。依赖/锁同步不得引入未覆盖独立包变化。成功公告完整列主程序及全部累计子包旧新版本和原因，新增/退役单列，不受效果摘要三条上限限制。

普通 git-sync 不以子包收口或 prepared 为前置，保留 Git、编码、配置、Hook、漂移及最终提交保护。存在 develop journal 时先受管 recover，不同步半完成版本。--release-status 的 readiness_scope=baseline-only 不代表完整发布门禁通过。

## 6. 配置与执行入口

schema 4 保留 release_model=product-packages、原生适配和正式发布 baseline。配置只经既有 --migration-plan/--migration-apply --setup-config-file，提案不填 baseline；已有 schema 4 只更新实际合同，不重切历史或认证未发布内容。混合 workspace 独立包进 packages，产品载体进 product_packages 且明确源码归属；不用整个 product_manifests 吞掉独立包，独立包不得依赖产品载体。

首次纳管旧包用 package_adoptions 声明当前新 id、不可变来源 revision、原生版本和 source 身份；程序从紧邻首次登记的 Git revision 重建真实 workspace/锁核验。不能替代 split/move、冒领退役身份、挑方便历史或改写 baseline。

经用户明确批准的正式版本撤回通过 `release_withdrawals` 精确登记：`rollback`、`restored_release`、`withdrawn_release` 为完整不可变提交 ID，`reason` 说明批准依据；`retained_documentation` 只列实际保留差异的 Markdown 文档路径。工具逐次核验第一父链、正式发布、完整恢复树及原收口账本；文档若在任一历史配置中承担产品、包或构建输入，不能豁免。包身份或生命周期跨撤回变化暂不支持，Git replacement/graft 也不能作为撤回证明。已提交的撤回登记不得删除或改写。

撤回只改变内容比较基线，不释放已经发布的版本号。后续主程序版本和新内容子包版本必须高于对应历史已发布上限；保持原内容的重验证不重复升版。release 不补做 develop，撤回登记也不伪造收口记录。无需先安装已撤回版本，可在另行获准发布后直接升级到更高版本。

普通 `checks.inputs` 只绑定检查执行，单独承担该角色的保留文档可以不同；同时承担产品、构建、稳定性或交付物角色时仍拒绝豁免。允许撤回不等于这些检查已经通过，本次验证仍按实际开发范围安排。

development 按包 id 登记非空 checks，每项有 id、program、args，可选 timeout_seconds（默认 300，1–14400）、inputs、artifacts。验证脚本/数据须为包输入或 checks.inputs，跟踪产物须为 build_inputs。可选 stability 含非空 contracts 和至少 core/integration 各一项检查。下游 Python 项目保留 python_environment 的解释器、运行时及依赖目录要求，Forge 自身工具仍用受管 Rust。

1. develop --preview --packages <id1,id2> 读取 affected、effective scope 和 fingerprint；无待处理项不写记录。
2. 在忽略的 .runtime/develop-analysis.json 写 `{schema:1,fingerprint,packages:{id:{level,reason,compatibility,acceptance?}},validation:{id:[完整检查对象]},targets?,interface_impacts?}`。不含 product 或 release checks，必要关联包不得删除。
3. 默认使用相同包选择和分析文件直接 `develop --apply`，内部重建计划并校验分析绑定；`--fingerprint` 可省略，提供时仍严格核对。带分析预览仅在需要审阅时执行。D3/D4 紧邻时由此一次完成必要检查与记账，不先在外部重复同一组检查。
4. 需要把验证和记账分开时，先 `develop --validate --packages <ids> --analysis-file <path>`：在最终候选运行所选检查（及适用稳定检查），恢复原版本和投影，返回 development-validated，不写账本。随后同分析 apply 引用仍有效的任务证据，缺口或新增影响才执行相关检查；完成收口终态仍须 development-closed。validate 会写忽略的证据及检查产物，不接受 --check/--dry-run，不等于只读预览或用户验收。
5. 中断用 develop --recover，绑定 checkout 和 HEAD，外部冲突保留。预览/执行的选择、宿主和输入须一致，变化则重新分析，不重绑旧 JSON。成功收据已核对最终输入，无后续变化或矛盾时不强制重复预览。

task-scoped-development-v1 的 --packages 接收唯一非空 id，不与 recover 合用。工具展开依赖、调用方和生命周期端点；package_roles 区分 requested、affected-dependency-or-consumer、context-only，只有实际 affected 包进入分析与检查，不能从 effective scope 数量推断升版或检查数量。task-development-current 与 outside_task_evidence_checked=false 只说明本次范围；无关历史不自动纳入，不据此宣称 release 就绪。

release 先用 --sync-remote 接入远端变化，再用零写入 --release-preview 生成最终上下文；分析只含 product、可选 targets.product 和显式必要 checks。预览的 product_constraints 给出已记录包/生命周期影响的最低级别和版本、breaking 来源及 product 接受依据绑定要求；它是下限，不代替主程序真实影响分析。实际 --release --skip-fetch 内部只构建并校验一次计划再写入，不默认另跑外部带分析预览；需要独立只读审阅时仍可选用。相同输入和必要环境的分析可跨宿主进程使用，真实变化仍拒绝，不只替换 fingerprint。release-external-analysis-preview-v1 仅允许预览读取外部普通无 link/reparse 文件，实际 release/develop 仍须项目 .runtime 路径；预览不证明提交、Hook、推送或网络成功。--sync-remote 会 fetch、必要快进和恢复本地改动，不是零写入入口，不提交、推送或升版。

## 7. 必要环境与可选复用

项目可显式声明 environment 的 tools（入口/编译器/linker/wrapper）、非秘密 variables、稳定角色到文件/目录的 inputs、cargo_configs。按实际填写，不自动猜合同。Rust 工具绑定文件和版本；目录绑定成员，变量路径按所指内容核验并保留本机运行值。执行仅继承合同参数及必要定位变量，不登记秘密，不以相同 Rust 版本代替 SDK 等价。

保留现有受限 Cargo 配置建模：build/target/term 和远程 source/registries/registry/net；本地 directory/local-registry、path patch、env 注入、credential-provider/alias、未建模 include/alias、linker/sysroot flags 继续拒绝，inline 表不例外。远程索引不得嵌凭证、查询或片段；Git 拉取绑定 Git 身份，相对工具按配置文件基准解析。不为轻量化删除项目配置，不能完整声明时保留原宿主模式。不运行 setup.py、安装脚本、SCM 后端或动态宏猜元数据。

staged-development-validation-v1 在现有检查记录结构上保存本任务候选证据，位于忽略的 `.runtime/bridgeforge-codex/develop-validation/`。复用须有完整显式 environment，绑定包/依赖及声明输入、最终候选字节、命令、工具/环境、Git 身份、日志及产物；独立包修正不机械废弃未受影响包证据，事务仍核对完整当前计划。没有 environment 时实际执行、不保存可复用任务证据，最短路径应把 D3/D4 合并在一次 apply，不先单跑相同检查。验证脚本/数据须登记 inputs，未声明的外部状态不能推断等价。在线状态或必需副作用检查登记 `always_run:true`，每次实际执行；普通稳定检查满足同一最终候选时可引用完整结果，不省略稳定清单。任务证据不接受任意外部 passed 声明，不默认跨机复用，不是长期认证。

resumable-development-checks-v1 保留可选同机失败重试缓存：完整显式 environment、项目明确 reusable:true 的确定性检查才可用；always_run 优先于 reusable。输入输出不重叠，复用产物须忽略。两种复用共用检查记录、日志和产物核验；失败、超时、漂移不存成功，显式 validate 失败不继续使用该检查的旧任务成功记录。不重绑旧证据。收据 checks.scope=final-candidate 的 executed/reused 仅统计最终轮，不含失败或稳定回退成本。

独立发布候选演练及其准备状态命令已退役。正常 develop 负责本次必要验证，release 负责版本覆盖与发布；C05 的确定性检查复用和受管构建缓存继续按各自合同执行。
