# Codex 骨架更新

仅在根 skill 判定为 `update` 后读取。

1. 只运行 `bridgeforge project-sync --mode update` 生成计划。
2. 接受任意不高于产品 home 的合法单戳。以当前产品合同 `compatibility_baseline` 为固定分界：低于基线或使用旧戳文件名时直接重建最新骨架，禁止读取旧合同或历史兼容分支；等于或高于基线时兼容更新、保留项目定制，不做全目录重建。分界线必须固定，不得随 `release_version` 自动抬高。
3. 旧戳文件名由同一 latest 事务退役；终态只保留 `.codex/.bridgeforge_codex_version`。
4. 旧 Markdown Rule／项目 Memory 不再迁移；兼容更新保留，重建时按普通项目自有资产逐项决定保留或删除。其他 project-owned、未知文件和人工定制遵守相同归属保护。全部确认前零写入；不存在任何决策资产时零确认直接安装最新基线。
5. 准备 apply 时返回根入口并读取 `references/transaction.md`。

不得调用已退役的 switch、finalizer、parity 或布局迁移工具，不得手工写戳。
