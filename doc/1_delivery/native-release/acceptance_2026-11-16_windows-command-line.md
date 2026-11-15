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
