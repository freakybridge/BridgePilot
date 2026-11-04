# Forge 内部维护流程

本文件是 BP 按路径读取的内部维护手册，不是可发现或可显式调用的 Skill。由主对话在已有明确维护授权内编排，公共用户入口为 `$bridgepilot-update`；咨询和只读要求仍优先，不从读取手册推导执行授权。

## 0. 先分流意图

咨询直接回答；诊断、审计或只读预览仅检查已有相关文件，涉及运行时先读 [runtime preflight](references/runtime-preflight.md)，不进入下文维护流程，不刷新、安装或修复。未查上游时禁止称本机版本为最新。

明确安装、升级或 BP 维护入口的对应授权才进入 §1；附带的只读要求优先。意图不明且会写入时，只补问该决定，禁止从产品名称推断维护授权。

涉及项目安装、换机恢复、重装或环境迁移时，在执行相关安装或恢复操作前，按以下规则处理：

1. 先读取当前项目根目录的 `INSTALL.md`，以它为统一入口；可以继续读取它明确指向的细节文档，不自行改从 README.md、AGENTS.md 或其他文件拼凑安装步骤。
2. `INSTALL.md` 缺失、步骤不完整或与实际项目不符时，先报告缺口，只暂停依赖这些说明的安装或恢复操作。依据真实代码、配置和已有资料核实，在已有授权范围内补齐或修正根目录 `INSTALL.md` 后再继续；没有文档写入授权时，只给出补齐建议。不得猜测仓库地址、照抄占位命令或先凭猜测安装。
3. 项目业务环境及项目自有 Hook 按项目 `INSTALL.md` 恢复；BridgeForge 工具安装与骨架维护仍使用本 Skill 规定的受管入口，不用项目命令替代。普通骨架升级不自动恢复业务环境；只读诊断不补写文档、不执行安装或恢复。

## 1. 刷新产品入口

维护仅支持 Windows；其他平台在下载或写入前停止。维护同时更新用户级 `~/.codex/AGENTS.md`：采用仓库统一模板，本地内容不同则先备份再覆盖，相同则不写。向用户说明这一范围后，复用已有维护授权，从薄入口运行一次 updater：

```powershell
$BRIDGEFORGE_CODEX_ENTRY = Join-Path $env:USERPROFILE ".codex\skills\bridgeforge-codex"
$BRIDGEFORGE_CODEX_HOME = Join-Path $env:USERPROFILE ".bridgeforge-codex"
& powershell -NoProfile -ExecutionPolicy Bypass -File `
  (Join-Path $BRIDGEFORGE_CODEX_ENTRY "scripts\bridgeforge_codex_shared_update.ps1")
```

首次只有 BP、内部目录尚未安装时，从本轮 `source_update_completed` 收据的 `source_root` 读取同源手册，并执行该根目录下的 `scripts/bridgeforge_codex_shared_update.ps1` 引导；已有内部目录则使用上面的受管脚本。必须核验收据与实际源码来源，不从当前工作目录或其他 clone 猜替代脚本。引导成功后改读收据对应产品 home 的新手册，不递归重跑本阶段。

成功收据必须包含 `BRIDGEFORGE_CODEX_SHARED_UPDATE_RECEIPT`。随后重新读取 `$BRIDGEFORGE_CODEX_HOME\source\forge\templates\maintenance\bridgeforge-codex\MAINTENANCE.md` 并以新版本继续。旧入口成功收据完全缺少 `user_agents` 或 `public_artifacts` 字段时，在本次 Skill 调用内从刚更新的薄入口自动补跑一次 updater，传入 `-ResumeSourceCommit <首份收据的 source_commit>`，只完成同一已验证版本的过渡，不再次查询或追随上游。两个字段同时缺失也只补跑一次；禁止要求用户再次调用 Skill。字段存在但为空、状态失败或来源无法验证不属于旧协议过渡，必须停止。补跑仍缺字段或状态无效时停止，不循环重试。

复用已有维护授权；无范围限制的显式调用覆盖上述用户级维护。已有授权明确排除用户级覆盖时，在任何新增范围写入前，将范围选择与项目风险、迁移决策合并为一次确认；不能安全完成只读规划时说明阻塞，禁止分拆为多轮授权。整次调用最多一次用户确认，内部版本过渡和权限重试不要求重新调用 Skill。

结果简要展示 `user_agents.status`；覆盖时给出收据中的备份路径。`override_present=true` 时说明原有 `AGENTS.override.md` 可能优先生效，禁止删除它。用户指令由新会话加载，不声称当前会话已经重载。

内部目录不安装 SKILL.md 或菜单元数据，仅保留维护手册、引用资料和 updater，完整产品只能来自包含根 `Cargo.toml`、`Cargo.lock`、`source/forge/cli/Cargo.toml` 与受管 `bridgeforge` 二进制的官方产品 home。updater 失败、产品 home 缺文件、非普通目录、工作树不干净或 origin 不匹配时，读取 [用户级受管 Skill 维护](references/user-skill-maintenance.md) 后停止；禁止从旧用户目录、本地 clone 或当前项目补文件，也禁止读取、迁移或删除旧 BridgeForge/Claude 遗留。

## 2. 判断模式并锁定已验证工具

项目写入前核实真实 Git 根、当前产品和已准备工具来源。使用统一 Template-only 模型；`auto/init/adopt/update` 仅为调用兼容，不读取旧合同、版本登记、账本或戳决定政策。`.codex` 递归只允许 hooks.json；未知额外 JSON 阻断，已知退役项列入精确风险计划。公共工具编译来源必须与当前产品一致。

```powershell
$BRIDGEFORGE = Join-Path $env:USERPROFILE ".codex\bin\bridgeforge.exe"
```

从最终 updater 成功收据读取 `public_artifacts.receipt` 并保存为 `$PUBLIC_ARTIFACTS`；其状态必须为 `prepared`。工具与公共产物验证归用户级事务，普通维护不再重复运行 doctor。缺少有效收据时先按 §1 判定是否符合一次性旧协议过渡；不符合或过渡失败才停止，不切换到项目脚本、Python fallback 或其他 clone。主动诊断或异常定位才读取 [Rust runtime preflight](references/runtime-preflight.md)。

## 3. 用户级与项目级 Hook 边界

普通 update 不检查 Native Memory 同步状态，不执行 reconcile、首次启用或常规 repair。只有目标版本明确要求迁移用户级 Hook 注册时才读取 [Native Memory 状态处理](references/native-memory.md)，按既有授权保留启用/禁用状态；主动 Memory 维护仍是独立任务。

项目自有 Hook 由项目独立开发、构建和注册；骨架维护只更新公共 Hook，保留项目程序、收据与注册，不扫描完整源码、编译或记录迁移状态。此边界固定在代码中，不依赖策略文件。用户询问或明确要求维护项目 Hook 时读取 [项目 Hook 独立维护](references/project-hook-independence.md)，在项目自身开发流程执行。

## 4. 计划与应用

兼容模式名的说明见[初始化](references/init.md)与[已有资产接入](references/adopt.md)；异常或需要核对字段时读[技术收据](references/technical-receipts.md)。三种模式仍使用同一 Template-only 规则，不恢复旧戳分流。

读取 [更新规则](references/update.md)，使用上节已验证的 CLI 与公共产物：

```powershell
& $BRIDGEFORGE project-sync --project-root . --template-root $BRIDGEFORGE_CODEX_HOME --mode auto
```

计划输出 aggregate_fingerprint、safe、risk、retired、gaps、blockers。blocker 停止；不存在风险或已有风险授权时直接继续，不重复询问。新增真实风险一次合并说明并确认，确认前不执行任何写入。

发布入口部署属于本次安装／升级：按更新规则复用已有项目声明，首次缺失时在安装阶段整理并确认必要版本／构建／产物规则；将发布驱动、注册和收据纳入同一计划、事务及验收。gaps 未解决不能报告整体成功。批次复用同一公共准备产物，不逐项目重编公共驱动，不把业务构建或真实 release 当安装探针。

读取 [事务与恢复](references/transaction.md)，用同一指纹及 $PUBLIC_ARTIFACTS 执行 --apply。工具在锁内重算计划，失败保护现场；不手工复制、删除或写戳。

## 5. 结果

报告真实 succeeded/blocked/rolled-back、写入及退役路径、恢复副本和剩余事项；终态是四类规则一致：模板公共资产、原生业务 handler 保留、工具编译来源一致、.codex 递归 JSON 仅 hooks.json。业务构建、测试、版本与发布由项目原生流程维护，骨架更新不认证未执行的业务验证。不宣称旧 develop 或配置化 release 仍可用，不迁移旧配置续用。
