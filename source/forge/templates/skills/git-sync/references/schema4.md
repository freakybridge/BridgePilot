# 旧 schema4 发布机制已退役

项目版本配置、开发收口账本及其自动发布门禁不再受支持。不要生成、恢复或迁移旧 JSON，不调用旧 develop/setup/migration/release 参数。版本与发布遵守项目原生 manifest、依赖锁及 AGENTS / INSTALL；普通受管 git-sync 不主动升版。
