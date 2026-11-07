---
lifecycle: completed
validation_status: passed
---

# develop 工程清理阶段验收

## 用户决定

2026-11-06，用户通过 `summary accept` 验收本次 `develop` 流程调整，并通过 `git-sync release` 授权正式发布。

## 交付与验证

- D2 保留调查与实现职责。D3 在实现结束后检查本次涉及模块的旧实现、重复逻辑、废弃入口、临时开关及专属配置、工具和测试。删除前核对实际入口、注册、调用方、兼容和回滚用途。确认可删且在授权范围内的随任务清理，其余列为候选并保留原因。
- D4 集中验证实现和清理结果，原生版本收口和交付顺延至 D5、D6。已更新 `collab`、`agent-execution`、`ml-delivery` 的阶段引用，以及四份共享 Skill 文件的分发校验值。 [develop 模板](../../../source/forge/templates/skills/develop/SKILL.md)
- D1–D6 阶段顺序与相关引用已核对，`bridgeforge-factory.exe manifest --root . --check` 返回 `status=ok`、`changed=false`，`git diff --check` 通过。用户级 Skill 安装和真实下游执行未验证。正式发布结果以受管 CLI 的远端核验收据为准。

本次验收覆盖工程清理阶段的增量。[TODO-046 的原共用流程实施计划](plan_2026-10-28_develop-common-path.md)继续承载原需求的验收状态。

## 专用清理 Skill 验收（2026-11-07）

用户通过 `summary accept` 验收 Forge 新增 `avoid-overdefensive-programming`、develop 的 D3 调用及本地开发收口，并通过 `git-sync release` 授权正式发布。本次属于上述工程清理阶段的增量，TODO-046 原共用流程的未结范围继续由原记录承载。

### 已交付内容

- 公共 Skill 以本次 diff、实际调用方和数据契约为依据清理防御分支，保留真实失败、旧数据、权限、并发、幂等及受支持的降级行为。上游同名仓库未授予再分发许可，Forge 版为独立编写。
- D3 在本次改动涉及可执行逻辑时读取该 Skill。公共模板、稳定资产 ID、显式分发目标、历史摘要、内嵌模板和本仓库镜像已同步。
- 原生候选为 Forge 19.0.0，CHANGELOG 已登记。三项工厂产物已通过受管构建和自检，后续 `build-assets --factory --project-root . --check` 返回 `status=current`、`reused=3`、`built=0`。模板、manifest、版本、Skill 元数据和镜像一致性检查通过。

### 行为验证与适用边界

- 初次生成对照采用四个独立样例，两组各运行八次，均通过行为测试且未新增无依据分支，没有观察到新增 AGENTS 规则的改善。
- D3 清理对照采用同四份预置问题代码，三组各重复三次。现行 D3、增加 AGENTS 规则、D3 读取专用 Skill 均清除 12/12 个问题并通过原定行为测试。事后补充的两项解包探针分别为 0/3、0/3、3/3。该轮因 Windows 只读执行策略阻止现场读取，使用实际指令文件的预加载内容，结果不能单独证明自然加载。
- 真实重放选择 CausisRiskSuite 历史提交 `782c54b7d8614d61151a95997c18511cb4dff724`，在用户授权的隔离 worktree 从同一 D2 暂存补丁接续。提示只点名 develop，B 组实际命令先返回 D3 内容，再读取专用 Skill，后一次命令退出码为 0。两组均沿用同一历史项目规则。
- 原版 D3 清理闲置导入、常量和过时注释，保留缺标记时默认两路成功及 `remove_date` 别名。专用 Skill 组删除两处兜底。原任务窗口、展示、AM 上传判断和爱建来源的四组离线用例，两组均为 `27 passed`。
- 补充行为探针确认 B 组保留真实结果的两项解包和单路失败识别。缺少成功标记的旧测试元组会记录错误，两路状态为失败。旧裸证券代码键与 `end` 字段仍可读取和迁移，T+11 保留、T+12 移除。
- D3 会话耗时原版 192.5 秒、专用 Skill 组 146.9 秒，D4 测试另计。每组仅一次，不能据此推断稳定提速或普遍时间增量，也未验证最初生成阶段能否稳定少写兜底。

本机完整命令、补丁、会话事件及结果位于 `.runtime/tests/causis-d3-replay-20261107/`。关键收据为 `seed.json`、`A-replay.json`、`B-replay.json`、`A-pytest.json`、`B-pytest.json`、`B-probe.json`、`skill-load-evidence.json` 和 `main-isolation.json`。CausisRiskSuite 主工作区的 HEAD、暂存区、差异和状态指纹核对一致。试验 worktree 因被客户端识别为当前对话主检出目录而未能归档，现场保留。

本次验收覆盖源码、产品传播、本仓库镜像、隔离样例及真实下游历史任务的离线重放。真实交易所、AM 上传、用户环境安装、其他项目升级及普遍收益尚未验证。正式发布以受管 CLI 的 `status=released` 与 `publication_verified=true` 收据为准。
