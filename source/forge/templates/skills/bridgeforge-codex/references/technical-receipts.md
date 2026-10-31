# 内部技术收据

仅在需要核对成功条件、异常或回答技术追问时读取。默认用户结果来自同步器 `human` 区，不倾倒字段清单。

当前项目合同：

- plan：`schema`、`status=planned|current`、`readiness`、`mode`、版本、`safe/risk/gaps/blockers`、`preservation_manifest`、`confirmation_required`、`aggregate_fingerprint`，以及 `public_installation_required`。公共安装未核对前不能仅因项目内容无变化就结束整个 update。`preservation_manifest.compatibility_baseline` 是固定分界线，`destructive_rebuild` 表示本次是否重建；不得用目标版本代替分界线。
- apply 成功：`status=applied`、`execution_status=succeeded`、`project_readiness=ready`、`current_version`、`aggregate_fingerprint`、`applied`、`rollback_performed=false`、`stamp_written_last` 和 `preserved_asset_ids`。实际安装最后写戳；no-op 的 applied 为空且不重写版本戳。旧语义迁移字段及 `hook_migration` 已移除，项目 Hook 不再生成迁移提示或状态。
- apply 失败：combined 的 `machine` 为 `status=blocked` 和 `error`；只有错误明确包含完整回滚结果时才能声明已回滚，不能从退出码推断。原始问题留在技术收据，用户显示稳定中文结果。

用户级 updater 的 commit 与刷新收据单独核对，不属于项目 apply JSON。迁移源/目标完整清单来自紧邻 apply 的 plan，并以指纹、manifest hash、实际文件终态相互核对。

Native Memory 使用独立 status 收据：`consent`、`enabled`、`hookInstalled`、`hookRuntimeVerified`、`remoteConfigured`、`pendingAgeSeconds`、`syncHealth`、`workerActive`、`activeConflict`、`lastReceipt`、`healthReceipt`、`alertId`。项目成功不证明用户级 Memory 健康；`busy/pending/gap` 不能证明同步完成。未执行的真实生命周期或 reconcile 必须写明未验证。

`machine` 只提供当前版本 JSON，不保持旧版本字段或枚举兼容；`human` 提供确定性的“结论、待处理事项、下一步”；`combined` 同时提供两区。主对话按 machine 推进流程，原样展示 human，禁止补造缺失字段或用临场解释覆盖结论。
