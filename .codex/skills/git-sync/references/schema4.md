# 旧 schema4 发布机制已退役

项目版本配置、开发收口账本及其自动发布门禁不再受支持。不要生成、恢复或迁移旧 JSON，不调用旧 develop/setup/migration/release 参数。版本与发布遵守项目原生 manifest、依赖锁及 AGENTS / INSTALL；普通受管 git-sync 不主动升版。

完整发布使用新的位置参数入口 `git-sync release`。它调用项目在原生 Cargo metadata 中登记的适配器，由 CLI 贯通必要构建、产物、源码提交推送及真实发布核验；旧 `--release`、`--release-preview` 和旧账本不是该入口的依赖。原生入口缺失必须报告未完成，不回退普通同步。项目接续记录只进入 `.runtime/`，不是版本事实源或开发认证。
