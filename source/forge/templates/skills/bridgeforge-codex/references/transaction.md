# 骨架事务与回滚

仅当根入口已取得有效 plan、fingerprint 和所需用户选择，准备执行 Apply 时读取。本文是骨架同步 preflight、事务、回滚和版本戳写入顺序的唯一操作 owner；根 `AGENTS.md` 只保留版本域红线，其他 Skill 或说明文档不得复制本流程。

Apply 必须传 `--apply --plan-fingerprint <fingerprint> --public-artifacts <updater 收据路径>` 和已取得的用户选择；plan 有 risk 时传 `--confirmed-risk`，该标志只证明前面逐项决定已齐全，不得触发额外确认。destructive rebuild 还必须传已确认的 `PreservationManifest`。旧语义迁移参数已退役，传入时拒绝执行。禁止人工 copy、merge、删除或写戳。

用户级事务与项目事务分开：用户级成功后，项目阻断或回滚不得撤销已完成的共享安装。公共 CLI/Hook 在用户级准备，同一项目事务安装其确定产物和项目文件；项目规划与安装不重新编译公共程序。普通 update 不构建项目自有 Hook。

同步器必须：

- 只以刷新后产品 home 的 latest current-only 合同生成目标；禁止解释旧 schema 或逐版本执行历史迁移。
- 按当前合同固定 `compatibility_baseline` 分流：基线及以上常规兼容更新保留 project-owned、未知文件和人工定制；低基线 latest rebuild 将未被当前合同覆盖的普通文件列为精确路径的保留/删除决定；链接、危险 Hook 结构和未确认项阻断，并严格执行 `PreservationManifest`。
- 仅对受支持的 8.x 及以后项目执行当前基线更新或合法首次接入；<8.0.0 在项目写入前拒绝。事务精确退役已有旧 `.codex/find-doc.map.md` 与 `.codex/sync-docs.map.md`，保留 C04 在 `.runtime/bridgeforge-codex/` 重建索引的能力，禁止把运行时索引加入 Git。
- Apply、`$git-sync` 与 pre-commit 复用同一 baseline 规则；项目规划只计算必要差异，不另做全量健康巡检。独立项目 Hook 的源码、依赖和开发构建不属于 baseline 常规检查。
- legacy Rule／项目 Memory 作为普通项目自有资产保留或显式决定删除，不再解析正文、转换或固定退役派生文件。
- Skill 只允许确定性修复 frontmatter；缺少 description 或 routing 语义时必须阻断。
- 同一事务组合并验证最新基线和项目自有区，只删除已确认的项目资产或合同明确管理的退役资产，最后写 `.codex/.bridgeforge_codex_version`。
- 任一失败回滚本事务全部写入；成功后不得保留 before 包。
- Claude 项目遗留只提示，不读取、不修改。

执行入口只做必要的一次规划复核，锁内合并漂移检查与回滚原文件读取。安装完整验证一次，最后写版本戳，终态核对已验证内容未变及戳正确，不重复完整语义检查；项目锁不能代替内容证据。外部改动不能被回滚覆盖，回滚未完成必须明确报告。有效完整收据足以收口，不在成功后手动再跑同类检查。
