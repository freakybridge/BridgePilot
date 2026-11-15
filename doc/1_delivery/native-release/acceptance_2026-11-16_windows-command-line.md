---
lifecycle: completed
validation_status: verified
---

# 原生发布版本分析的 Windows 命令行超长修复

## 问题与范围

StratusAgent 的 `git-sync release` 在 `source-version` 阶段返回 Windows `os error 206`。当时相对上次正式发布的累计代码变更有 955 条路径，仅路径参数约 45,085 字符。工具将全部路径追加到一条 `git log --format=%B%x00 <baseline>..HEAD -- <paths>`，进程创建前即超过命令行上限；并非推送鉴权失败。分批提交或推送不会缩短相对上次正式发布的累计路径集合。

本轮仅修复 BridgePilot 的 Forge 通用产品实现，保持路径筛选、历史消息、版本分类、子包升版、合理候选复用和错误处理语义，不修改真实下游或发布现场。

## 实现与边界

- 维护源为 `source/forge/workflow/src/project_release.rs`：普通路径通过 `git log --stdin` 的路径输入传递，保留完整路径集合的一次历史遍历，不拆成多个查询。分批查询可能改变合并提交的历史简化结果，不能仅靠合并消息或去重保持原语义。
- Git 标准输入按原始行读取路径，不套用 shell 或 JSON 引号。含换行或末尾回车的历史路径仍经原始 argv 传递，避免分行及 CRLF 处理改变路径；这类特殊历史路径自身累计超长仍保留原平台限制。
- `code_paths` 筛选、`%B` 与 NUL 消息解析、`level`、`increment`、`package_edits` 和 Release 复用判断保持原实现。
- Forge `20.1.1 → 20.1.2`、workflow `0.16.1 → 0.16.2` 为兼容修复，原生锁文件与 CHANGELOG 同步；BP 主版本在工程交付时保持 `2.7.1`，正式发布版本由原生适配器决定。

## 验证依据

工程交付时执行：

```powershell
cargo test --locked --offline --manifest-path Cargo.toml --target-dir .build/bridgeforge/cargo/local -p bridgeforge-workflow project_release::tests -- --test-threads=2
cargo run --locked --offline --release --manifest-path Cargo.toml --target-dir .build/bridgeforge/cargo/local --bin bridgeforge-factory -- build-assets --factory --project-root .
.build/bridgeforge/bin/bridgeforge-factory.exe build-assets --factory --project-root . --check
.build/bridgeforge/bin/bridgeforge-factory.exe manifest --root . --check
.build/bridgeforge/bin/bridgeforge-factory.exe check factory-version --root .
```

6 项原生回归全部通过，涵盖：

| 场景 | 实际标准与结果 |
|---|---|
| 1,000 条长路径 | Windows 下旧查询实际返回 206；新查询成功，输出与该夹具的等价短查询逐字节一致 |
| 合并历史、顺序及重复消息 | 与原始完整路径查询逐字节一致；合并提交保留，同主题不同正文的两条提交均保留 |
| 普通及特殊历史路径 | 中文、空格、TAB、引号、反斜线、LF、CR、Ctrl-Z 及 pathspec magic 与原查询输出一致；Windows 不可落盘名称通过 Git 树对象构造 |
| 版本分类及候选 | patch、minor、破坏性标记、正文 BREAKING CHANGE、0.x 子包和已有更高版本候选保持预期结果 |
| 复用、未提交输入及失败 | 纯文档复用原 Release；缺分类消息、非法代码提交消息及坏 revision 继续失败 |

首轮有一项测试夹具尝试在 Windows 创建 TAB 文件名而失败；将该名称移入纯历史对象夹具后通过。此失败未进入待验证的 Git 查询，也未据此改动生产语义。

独立 review-auditor 完成真实 diff 的只读复核，未发现剩余生产代码问题；未重复主对话的构建和测试。受管准备返回 `status=prepared`、三项工具就绪、`written_files=12`，自身 dogfood 与产品 manifest 同步；最终只读检查返回 `status=current`、`reused=3`、`built=0`，manifest 无漂移，factory-version `20.1.2` 且 healthy=true。`git diff --check` 和改动 Rust 文件的 rustfmt 检查通过。

实际新公共 CLI 的 `project-release source` 在同一长路径隔离夹具运行成功：主版本计划 `1.0.0 → 1.0.1`，子包计划 `0.4.0 → 0.4.1`，未混入纯文档提交的破坏性分类，夹具工作区仍干净。夹具及输入位于 `.runtime/tests/project-release-history/`；它们是本机验证证据，不是正式项目或版本账本。

## 用户验收与发布授权（2026-11-16）

用户显式调用 `summary accept`，确认本轮修复与上述针对性验证符合需求，并另行调用 `git-sync release` 授权本次完整发布。本记录确认源码及工程验证已验收，正式发布是否完成以受管 CLI 的 `status=released` 和 `publication_verified=true` 收据为准，不由本验收决定推定。

本次新增的六项源码内联测试固定了故障回归与既有语义，属于长期回归测试，保留；没有已确认可删除的一次性测试源码、注册或专属辅助文件。本机运行夹具和失败现场保留，不执行额外清理。未发现总表中同项 TODO，不补造编号或结算无关事项。

| 证据类别 | 验收时状态 |
|---|---|
| 源码 | 通用 workflow 实现及原生版本、锁文件已更新 |
| 产品传播 | 公共 CLI 已受管构建；正式发布另由本次 release 收据核验 |
| 自身 dogfood | 镜像及 manifest 一致性通过 |
| fixture | 六项回归及新 CLI 长路径入口验证通过 |
| 真实下游 | 未升级 StratusAgent 或其他下游，未重试下游发布 |
| runtime 发布接续 | 新 CLI 的隔离版本分析入口通过；真实下游 release 接续未验证 |

先前的下游升级批次记录及其索引不属于本次修复；发布期间仅作精确备份隔离，完成或暂停后恢复，不纳入本次提交。

<a id="merge-classification-acceptance"></a>
## merge 分类补充修复验收（2026-11-16）

长路径修复后的真实只读分析越过 206，随后被历史提交 `4a36c2c076c81f200e1292d84db4a7d6291dfbaa` 的 `merge: 合并远端 Config 与基础依赖独立提交` 阻断。原因是分类白名单缺少 `merge`，不是 Git 合并失败。用户明确授权修复 BridgePilot 并再次对 StratusAgent 做只读预演，随后显式调用 `summary accept` 确认本轮交付，并另行授权 `git-sync release`。

本轮修改 Forge 通用适配器 `source/forge/workflow/src/project_release.rs` 与 BP 私有适配器 `source/orchestrator/install/release_source.rs`，不把 BP 私有实现下沉到公共模板。两处均接受 `merge:` 和带 scope 的形式，普通合并按 patch；`!` 和各自原有正文破坏性标记沿用原规则，0.x 的破坏性变化仍升 minor，累计历史仍取最高档位。其他未知类型和缺少分类的消息继续阻断，不忽略整个合并提交，也不重写历史。

工程候选为 Forge `20.1.2 → 20.1.3`、workflow `0.16.2 → 0.16.3`、BP install `0.10.3 → 0.10.4`，均为兼容修复；原生锁文件、CHANGELOG 及自身 dogfood 已同步。工程交付时 BP 主版本仍为 `2.7.2`，本次正式发布版本由原生适配器确定。

定向验证命令：

```powershell
cargo test --locked --offline --manifest-path Cargo.toml --target-dir .build/bridgeforge/cargo/local -p bridgeforge-workflow -p bridgepilot-install release -- --test-threads=2
```

结果为 workflow 7 项、install 3 项全部通过。覆盖普通及带 scope 的 merge、前缀与各适配器原有正文破坏性标记、0.x 升版、累计最高档位、未知分类继续失败；保留并通过真实合并历史、消息顺序、已有候选复用及长路径等既有回归。

沿用前文受管准备命令，实际返回三工具 `prepared`、`written_files=12`；最终 `build-assets --factory --check` 返回 `current`、`reused=3`、`built=0`，manifest 无漂移，factory-version 为 `20.1.3` 且 healthy=true。`git diff --check` 通过。没有把本次验证扩展为完整业务回归。

真实 StratusAgent 预演使用 BP 本仓库构建的候选公共 CLI `20.1.3`，没有安装到下游。当前工具不提供完整 `--dry-run`，实际执行的是零写入 `git-sync release --preflight --publication-required --root .` 和 `project-release source --project-root . --input .runtime/bridgeforge-codex/native-release/source-input.json`：

- 首轮受限沙箱中子进程分别报 109 和“必须在工作树中运行”；直接 Git 可识别工作树。放宽沙箱后相同只读调用成功，不修改 ACL、配置或工具安装。
- 预检为 `ready`、`readiness_scope=git-only`、`remote_state=not_fetched`、`publication_status=not_verified`。
- 沿用原失败输入的 `v7.1.0`／`a225d05b2fb8d03ac6bbbce1cb1ff686e9ece831` 基线及分类消息，分析实际预演时的 HEAD `a93ff7df4562812c135c55bf355662cd31df855a`；版本选定为 `7.2.6`，生成 `source/Cargo.lock`、`source/common/process/Cargo.toml`、`CHANGELOG.md` 三项拟编辑，未应用。
- 候选工具构建等待期间观察到下游 Git 状态变化，因此不把整个等待窗口称为冻结；紧邻实际预演前后重新核对，56 个原生版本与锁、Git index/HEAD、接续输入与记录、已安装工具及发布驱动文件的摘要全部一致。

源码、公共候选产物、自身 dogfood、10 项回归、真实下游只读入口和上述 runtime 分析入口均有本轮证据。真实下游工具升级、业务构建、远端发布及完整 release 接续未执行或验证；不能将本次预演描述为完整发布通过。

本轮新增或扩充的源码内联测试固定了 merge 分类契约及防回退行为，全部作为长期回归保留；没有可确认删除的一次性测试源码、注册或专属辅助文件。未发现总表中同项 TODO，不补造编号或改写无关状态。正式发布是否成功继续以 CLI 的 `released`／`publication_verified=true` 收据为准。
