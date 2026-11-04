---
lifecycle: completed
validation_status: verified
---

# bridgepilot-git-sync release 模式验收

## 用户决定与范围

2026-11-03，用户要求为 `bridgepilot-git-sync` 增加类似 `git-sync release` 的批量发布模式，并选择：单项目失败保留现场，继续其余独立项目，最后统一汇总。用户随后授权实施，并在交付后显式调用 `summary accept`，确认本次技能交付符合需求。

本次验收范围是技能规则及现有用户级副本同步。默认全部已登记下游，也可用重复的 `--project` 指定子集；不带 release 保持普通同步。逐项目复用受管 `git-sync release --publication-required`，只有 `status=released` 且 `publication_verified=true` 才计为发布完成。未知结果先核销，接续仅针对受影响项目，不整批盲重发。受管命令由主对话执行，子 Agent 可分担只读核对。

实现事实源：[技能源码](../../../source/orchestrator/skills/bridgepilot-git-sync/SKILL.md)。本次变更属于 BP 私有本地扩展，未进入 Forge 公共模板、dogfood 或 BP 安装包；未修改 Rust 代码、项目名单脚本或程序版本。发布说明记录于根 CHANGELOG 的 Unreleased 段。

## 已有验证证据与限制

- `git diff --check` 通过；主对话核对两种模式、参数边界、失败继续策略、成功收据和接续要求。未执行独立 Agent 审计。
- 同步前确认用户级 SKILL.md 与原始源码一致，名单脚本两侧 SHA256 相同；保存旧技能备份后更新现有用户级副本。
- 更新后源码与用户级 SKILL.md SHA256 均为 `C25D3926E152FDE24D79837152AAEF6555DE848518BE370F59E5FBA0F7078C13`；用户级 git-sync、bridgepilot 及名单脚本引用路径检查通过。客户端随后提供了更新后的技能描述，已观察到原生技能发现更新。
- `python -X utf8 .../skill-creator/scripts/quick_validate.py source/orchestrator/skills/bridgepilot-git-sync` 因 `ModuleNotFoundError: No module named 'yaml'` 未能运行，不记为校验器通过；未安装依赖。
- 本次没有执行真实下游发布，故不证明各下游当前工具、鉴权、构建或发布链路可用；未做隔离端到端发布 fixture 或业务 runtime smoke。

用户验收不抹去上述自动校验及真实执行边界。检查现有 TODO 总表未发现本次独立技能扩展的唯一匹配条目；不将其他发布或 BP 演进任务据此结算，也不补造历史 TODO。

## 经验与发布边界

模式必须先于项目范围解析，不能将 release 传给项目名单脚本；普通同步的 preflight 和 no-op 规则不能套到完整发布。部分项目发布成功、源码已推送或工作区干净，均不能替代每个项目的正式发布核验。

用户同轮调用的是当前 BridgePilot 项目的 `git-sync release`，已授权该项目正式发布，不是执行 `bridgepilot-git-sync release`。本记录保存时发布尚未执行，最终结果以本轮受管 CLI 收据为准；不据用户验收或本地安装推定已发布。

## 版本报告格式增量2026-11-04

用户确认“两张表”的示例及修改计划后授权实施，随后于 2026-11-04 明确调用 `summary accept`，验收本次报告格式改动，并调用当前项目的 `git-sync release` 授权正式发布。本节补充此次验收，不改写前述历史决定。

- 主程序表固定为“项目／旧版本／新版本／发布结果／变化子包数”，每个目标项目均列出；子包明细固定为“所属项目／子包／旧版本／新版本”，列全已确认的版本变化并核对数量。
- 比较口径固定为上次正式发布至本次正式发布；不能用本地 VERSION、发布前 HEAD 或本轮自动编辑清单替代正式基线。复用已有 Release 显示相同旧、新版本和 0 个变化包，不重放首次发布时的升级明细。
- 缺基线或包范围不完整时保留未知，首次发布需明确证据；失败候选和未核验结果不混入成功发布统计。全部子包未变时用明确说明替代空占位行，发布凭证和实际失败阻点仍保留。
- 只修改此 Skill 的报告段，仓库源码和用户级副本同步；不改名单脚本、安装器、Rust 发布协议或任何项目版本。用户级旧文件同步前哈希与源码一致，备份位于 `.runtime/tests/bridgepilot-git-sync-report/before/SKILL.md`。
- Skill Creator 格式校验通过。依据已有四项目复用发布收据生成 `reuse-example.md`，并用 `mixed-example.md` 离线核对多个项目变化、全部复用、仅主版本变化、缺旧基线、部分失败的表格和计数；两份样例均位于 `.runtime/tests/bridgepilot-git-sync-report/`。未启动新的真实发布、构建或业务测试，未修改 Memory。

这仍是未纳入 BP 安装包的本地扩展，本次不扩大安装体系。当前用户级副本已更新，后续调用读取新报告规则。本次验收沿用格式校验、源码／用户副本一致性和离线样例核对结果，不将其冒充一次新的真实下游发布验证。

本主题没有新增独立 TODO，不改写其他事项状态。本轮仅发布当前 BP 仓库的报告格式改动；不因该发布调用重跑下游 release、升级环境或清理资料。主产品版本由原生流程判断，子包没有预先升版。正式发布完成须以本次 CLI 的 `status=released` 且 `publication_verified=true` 为依据。

## 受管安装与菜单发现验收2026-11-04

用户要求将 `bridgepilot-git-sync` 纳入 BP 受管打包、安装和校验，并更新本机安装，使菜单能够发现该入口。2026-11-04 完成交付后，用户显式调用 `summary accept`，确认本次交付符合需求，并调用当前 BP 项目的 `git-sync release` 授权正式发布。本节是对上述“本地扩展、未纳入安装包”历史状态的后续增量，不改写历史验证范围。

### 已验收范围

- 新增稳定资产 ID `bp.controller.skill.git-sync` 与 `bp.controller.skill.git-sync.projects`，分别登记正文和 `scripts/list_projects.py`；包内十五项资产通过实际 core 的 `bp-git-sync-skill-v1` 能力核对，历史十三项包仍可识别。原生发布资产清单同步扩展；复用旧 Release 时保留旧资产数量。
- `skill-install-preview`、`skill-install`、`skill-status` 和 `skill-recover` 纳入第四个入口的完整文件及本机绑定；未知同名目录、用户修改和额外文件保持阻断。恢复测试覆盖脚本、正文、绑定的每个写入边界；已安装该入口后，不允许回退到不支持它的旧程序。
- 修复 BP 的 Windows `\\?\`／`\\?\UNC\` 本机绑定生成无效 SQLite URI 的问题。名单查询仍为只读，缺库拒绝创建，未知项目拒绝降级为全量。
- 本机稳定安装 home 为 `C:\Users\bridg\Documents\Codex\BridgePilot`；通过受管程序及 Skill 两阶段事务完成升级，未手工复制用户级入口。Forge 公共模板和共享 Skill 分发没有变化。

### 验证与本机证据

| 验证 | 结果及覆盖 |
|---|---|
| `cargo test --locked --offline --manifest-path Cargo.toml --target-dir .build/bridgepilot/cargo -p bridgepilot-install --lib` | 最终候选 31 项通过；安装身份、事务恢复、外部修改、历史发布复用及新资产清单 |
| `cargo test --locked --offline --manifest-path Cargo.toml --target-dir .build/bridgepilot/cargo -p bridgepilot-orchestrator --features test-fixtures --test orchestration setup_cases::` | 最终候选 13 项通过；缺脚本、剥离新资产、重复安装、未知目录及用户修改保护 |
| `python -B scripts/tests/bridgepilot_git_sync_projects.py` | 2 项通过；真实 Windows 路径、SQLite 只读和选择边界 |
| `cargo build --locked --offline --release --manifest-path Cargo.toml --target-dir .build/bridgepilot/cargo -p bridgepilot-orchestrator --bin bp --bin bp-launcher` | Release 构建通过；原生 `release-prepare` 生成十五项 payload 加 package.json，共十六项发布资产 |
| `scripts/tests/bridgepilot_git_sync_install.ps1` | 使用真实 2.0.2 旧包隔离升级至 2.1.1；稳定启动器、四入口、零写预览、重复安装及不兼容回退阻断通过；收据 `.runtime/tests/bridgepilot-git-sync/b4539502fe3f4d6eac2938ed4b32f530/receipt.json` |
| 本机受管安装与原生发现 | `skill-status` 四入口均 `current=true`；`skill-probe` 实际调用 Codex `skills/list`，返回唯一用户级 `bridgepilot-git-sync`、`enabled=true`、无加载错误，未启动任务回合 |
| 真实名单与配置保护 | 已安装名单脚本只读列出 5 个登记项目；原 SQLite 档案 revision 102、6 条任务不变；workspace 配置 SHA256 `FB8A0F389706E98E958B8CF213843AB51E3E0DE0D35ECA601D6005E1882063E7` 前后一致 |

本机终验收据为 `.runtime/tests/bridgepilot-git-sync/local-install-verification.json`。Rust 共 44 项，加 Python 2 项为 46 项定向用例；隔离升级和本机原生发现另有上述实际命令证据，不重复计数。早期测试夹具缺暂停、使用不同 profile 触发绑定保护等问题已修正；Windows 脚本测试最初误用只读 `$HOME`，以及 SQLite 测试连接未关闭导致临时文件占用，均修正后通过，不将失败尝试计为成功。

### 版本、经验与保留边界

本轮交付前的正式基线为 BP 2.0.2；本机候选最终为 BP 2.1.1，orchestrator 0.17.0 → 0.18.0、install 0.8.0 → 0.9.1，Forge 17.3.0 不变。2.1.0 是接入安装体系时的本地中间候选，随后路径修复使用 2.1.1，未复用已安装版本的不同程序字节；这两次本机安装本身不证明任何正式发布。

“Skill 与本机旧安装一致”只能证明安装内部一致；核对最新版本还需核对官方来源。“源码有 Skill”也不等于包内登记、用户级安装或客户端发现：本次分别取得了精确资产表、四入口校验和原生 `skills/list` 证据。实际菜单提供方发现已验证，没有另做桌面截图验收；批量执行下游同步／发布和业务 runtime smoke 未执行，安装不授予其执行权限。

本次按真实影响选择定向验证，未运行全仓回归或独立 Agent 审计。没有发现本主题对应的独立 TODO，沿用本记录，不结算 TODO-050／TODO-055 等其他事项。用户本轮发布授权只覆盖当前 BP 仓库；不执行 `bridgepilot-git-sync release`，不发布下游，也不恢复 heartbeat。正式发布必须以本轮受管 CLI 的 `status=released`、`publication_verified=true`、固定提交及远端资产核验结果为准。

本轮发布实际调用 `.build/bridgeforge/bin/bridgeforge-factory.exe git-sync release --publication-required --message-file .runtime/git-sync-message.txt`，在仓库身份识别阶段返回 `factory commands require the factory repository`。当前安装工厂工具仍是本对话初始预检的 0.10.1，当前源码工厂为 0.12.5，且工厂清单已迁至 `source/forge/bridgeforge-manifest.json`；旧工具未随 Git 源码同步自动刷新。发布未进入提交／推送阶段，HEAD 仍为 `b440ea2990bbf879db62135d375a849112e2bdbe`，本机 BP 2.1.1 安装与本次验收不受影响。按 git-sync 的不隐式安装升级边界，先请求受管工厂工具刷新授权，再接续原发布；不恢复旧清单、不改用手工 Git／gh 发布，不把本轮结果记为正式发布完成。

用户随后明确授权刷新仓库内受管工厂工具。执行 AGENTS 指定的 `cargo run --locked --release --manifest-path Cargo.toml --target-dir .build/bridgeforge/cargo/local --bin bridgeforge-factory -- build-assets --factory --project-root .`：编译工厂 0.12.5 完成，首次因沙箱维护锁权限受阻后，以所需权限沿同一入口复用缓存继续，最终 `status=prepared`、`prepared=3`、`written_files=7`、`retired_files=0`，执行了受管自检。之后已安装工厂的 `build-assets --factory --project-root . --check` 返回 `status=current`、`read_only=true`、`reused=3`、`built=0`、`self_test_executed=false`；工厂二进制 SHA256 为 `8d53a4ea32200c4feae7b982209b51d1b49ae538b649680ca3b64ca6d9f466b9`。旧工具阻点已解除，接续本轮已授权发布；正式结果仍以固定提交及完整资产的远端核验为准，不将工具刷新当作正式发布。
