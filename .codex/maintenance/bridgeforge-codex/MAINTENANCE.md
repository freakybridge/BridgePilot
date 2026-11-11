# Forge 项目骨架维护

本手册随骨架进入项目 `.codex/maintenance/bridgeforge-codex/`。项目工作流由 `.codex/skills/` 发现，公共 CLI、Hook 和来源收据位于 `.deps/bridgeforge/bin/`。

## 1. 固定项目与官方来源

从目标仓库 INSTALL.md 开始，核对真实 Git 根、项目自有扩展、授权和既有原生发布入口。只读诊断复用已验证来源和 prepared 产物，缺任一项就报告预览缺口。

使用 BP 批次时，以本次官方源码更新收据固定来源根和提交。独立维护从官方仓库获取干净的 `main`，核对提交后读取该来源的手册。批次只准备一次，所有项目复用同一提交。

## 2. 准备公共工具

有准备授权时，从已核对的官方源码执行 `source/forge/templates/maintenance/bridgeforge-codex/scripts/prepare-project.ps1`，传入 `-SourceRoot` 和 `-SourceCommit`。项目内投影也可调用同一入口，仍须明确传入该来源。脚本复用受管工厂构建及原生收据，返回 `source_root`、`source_commit`、`cli` 和 `public_artifacts`。以返回的 CLI 和产物路径进入项目计划，来源或收据不一致时停止。

## 3. Hook 边界

项目升级只维护项目公共 Hook。用户级 AGENTS.md、既有 Forge 用户安装退役和 Memory Hook 执行器切换走独立的[用户级迁移](references/user-skill-maintenance.md)。Memory 同步状态见 [Native Memory 状态处理](references/native-memory.md)。

项目自有 Hook 由项目独立开发、构建和注册；骨架维护只更新公共 Hook，保留项目程序、收据与注册，不扫描完整源码、编译或记录迁移状态。此边界固定在代码中，不依赖策略文件。用户询问或明确要求维护项目 Hook 时读取 [项目 Hook 独立维护](references/project-hook-independence.md)，在项目自身开发流程执行。

## 4. 计划与应用

兼容模式名的说明见[初始化](references/init.md)与[已有资产接入](references/adopt.md)；异常或需要核对字段时读[技术收据](references/technical-receipts.md)。三种模式仍使用同一 Template-only 规则，不恢复旧戳分流。

读取 [更新规则](references/update.md)，使用上节已验证的 CLI 与公共产物：

```powershell
& $prepared.cli project-sync --project-root . --template-root $prepared.source_root --public-artifacts $prepared.public_artifacts --mode auto
```

计划输出 aggregate_fingerprint、safe、risk、retired、gaps、blockers。blocker 停止；不存在风险或已有风险授权时直接继续，不重复询问。新增真实风险一次合并说明并确认，确认前不执行任何写入。

发布入口部署属于本次安装／升级：按更新规则复用已有项目声明，首次缺失时在安装阶段整理并确认必要版本／构建／产物规则；将发布驱动、注册和收据纳入同一计划、事务及验收。gaps 未解决不能报告整体成功。批次复用同一公共准备产物，不逐项目重编公共驱动，不把业务构建或真实 release 当安装探针。

读取 [事务与恢复](references/transaction.md)，用同一来源、产物收据和指纹执行 --apply。工具在锁内重算计划，失败保护现场；不手工复制、删除或写戳。

## 5. 结果

报告真实 succeeded/blocked/rolled-back、写入及退役路径、恢复副本和剩余事项；终态是四类规则一致：模板公共资产、原生业务 handler 保留、工具编译来源一致、.codex 递归 JSON 仅 hooks.json。业务构建、测试、版本与发布由项目原生流程维护，骨架更新不认证未执行的业务验证。不宣称旧 develop 或配置化 release 仍可用，不迁移旧配置续用。
