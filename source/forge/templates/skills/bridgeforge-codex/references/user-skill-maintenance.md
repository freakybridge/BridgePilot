# 用户级受管 skill 维护

updater 先查询官方 main 提交：本机产品源同提交且完整、干净时直接复用，不 clone；变化时验证临时源，不原地 fetch/覆盖正式安装。离线、来源异常、未知本地改动或查询后提交漂移时停止。

updater 统一准备公共 CLI 与公共 Hook。重复维护先复用有效的已安装 CLI 或公共缓存中的 CLI，证据不足才按锁定配方引导构建；首次 CLI 构建结果作为已验证输入进入公共缓存，不在项目端重编。编译与环境身份必须匹配，未知 provenance 保留。产物缓存位于用户级 `.codex/.cache/bridgeforge/public-build`，沿用 ownership、锁、内容/收据/自检验证和有限保留；准备收据位于 `public-ready`，仅引用不可变构建身份。缓存不属于产品源工作树或 Git 资产。

成功收据的 `public_artifacts.receipt` 交给项目 Apply；项目只读捕获公共字节，不执行编译器、缓存自检或写缓存 recency。用户级安装与项目安装分别报告和回滚，项目失败不能撤销已成功的共享用户级安装。`build_counts` 和 `public_artifacts.build_metrics` 区分引导编译、公共编译与复用；父子阶段计时不能相加。普通 update 不重复 doctor，也不巡检 Native Memory。

用户级 `~/.codex/AGENTS.md` 由 `source/forge/templates/user/AGENTS.md` 单向分发，合同为Forge 分发清单 `source/forge/bridgeforge-manifest.json` 的 `user_instructions` 分区。相同文本（忽略换行差异）不写；不同则以原始字节备份到 `~/.codex/.AGENTS.bridgeforge-backup-<operation_id>.md` 后覆盖。成功备份长期保留，失败按同一分发日志回滚。该覆盖策略仅适用于用户指令，不改变 Skill 的归属冲突检查。

本功能仅管理默认 `USERPROFILE/.codex`，不管理自定义 `CODEX_HOME`。用户模板不进入项目骨架清单，不参与项目专区合并；不合并本地个人编辑、不删除 override 文件。源哈希、固定路径或原始文件校验失败时停止，禁止手工覆盖绕过事务。

- 活跃目录：`~/.codex/skills/`。
- 活跃账本：`~/.codex/bridgeforge-codex-managed.json`。
- 完整产品 home：`~/.bridgeforge-codex/`。
- Codex 薄入口：`~/.codex/skills/bridgeforge-codex/`，只含入口、references 与 bootstrap updater。
- 仅获准维护后运行薄入口的 `scripts/bridgeforge_codex_shared_update.ps1`；只读诊断禁止触发更新或事务恢复。刷新次数及旧入口一次性过渡条件以 SKILL.md §1 为准。
- updater 处理 `source/forge/bridgeforge-manifest.json` 中 `shared_skills` 登记的 Codex skills 和 `user_instructions` 登记的用户 AGENTS；`project_skeleton` 仅交给项目同步器；第三方 Skill 目录不得修改。
- source 必须来自 GitHub `freakybridge/BridgePilot` 的 `main` 并逐文件验 hash。
- 产品 home、用户级 `.codex/bin/bridgeforge.exe`、用户 AGENTS、skill stage/swap 与 ledger 必须由同一持久日志决定提交或回滚；恢复时核对组件路径和原始内容哈希。
- 统一提交前失败必须回滚全部组件；提交后备份清理失败不得单独回退 CLI。收据 `cleanup_pending=true` 表示分发已提交、旧备份待清理，后续维护先依据同一日志完成清理，禁止手工删除活动日志。

旧 `$bridgeforge`、旧 Codex/Claude ledger、`.bridgeforge` home 与旧 Claude Skill 已退役，禁止读取、接管或删除；需按安装说明重新安装。正式 Skill、hash 与 ledger 一致性仅由当前 updater 维护。
