# 内部技术收据

仅在需要核对成功条件、异常或回答技术追问时读取。默认用户结果来自同步器 `human` 区，不倾倒字段清单。

当前 Template-only 收据：

- plan：`schema=1`、`status=planned`、`model=template-only-v1`、`safe/risk/retired/gaps/blockers`、`aggregate_fingerprint`、`target_json_paths` 和产品版本。指纹绑定实际公共源码、锁文件、目标原字节、运行工具和预期写集，不读取项目旧版本配置或合同。
- apply 成功：`status=succeeded`、`aggregate_fingerprint`、`retired`、`written`、`recovery` 和 `baseline`。baseline 必须为 clean，运行工具来源已验证，递归 JSON 名单仅 `.codex/hooks.json`。不写骨架版本戳、不生成 PreservationManifest 或项目合同。
- 失败：非零退出码及 stderr 的阻断或恢复说明；只有明确报告回滚完成时才能声明已恢复，外部并发修改保留并报告回滚不完整。恢复副本保留在本次任务 `.runtime/tests/`，不作为在线配置。

用户级 updater 的 commit 与刷新收据单独核对，不属于项目 apply JSON。迁移源/目标完整清单来自紧邻 apply 的 plan，并以指纹、manifest hash、实际文件终态相互核对。

Native Memory 使用独立 status 收据：`consent`、`enabled`、`hookInstalled`、`hookRuntimeVerified`、`remoteConfigured`、`pendingAgeSeconds`、`syncHealth`、`workerActive`、`activeConflict`、`lastReceipt`、`healthReceipt`、`alertId`。项目成功不证明用户级 Memory 健康；`busy/pending/gap` 不能证明同步完成。未执行的真实生命周期或 reconcile 必须写明未验证。

新入口只以当前收据字段推进流程，不假设旧版 human/combined、stamp_written_last 或账本字段仍存在；不补造缺失结果。
