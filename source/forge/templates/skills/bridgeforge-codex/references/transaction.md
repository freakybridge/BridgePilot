# Template-only 事务与恢复

计划零写。Apply 传 `--apply --plan-fingerprint <aggregate_fingerprint> --public-artifacts <准备收据>`；存在风险时须有已有授权并传 --confirmed-risk，不因换步骤重复请求授权。

工具取得项目锁后重算指纹，绑定产品、编译来源、所有目标原字节及递归 JSON 名单；漂移即停止。公共工具只消费不可变准备产物，不在下游编译，也不构建业务 Hook。写入及退役前，把原字节保存到忽略目录 `.runtime/tests/template-only-retirement/<指纹>/before/` 并校验，仅用于恢复，不能作为在线配置。

事务更新模板、保留项目公共区之外的正文及 native handler、更新公共非 JSON 工具依赖源码，再清理精确退役名单。删除后校验模板、公共二进制来源和 `.codex` 递归仅 hooks.json，最后复核写集与退役缺失状态。失败恢复自身改动，外部并发变化保留并明确报告回滚不完整。不写新身份戳、不创建项目合同、版本登记或收口账本，恢复副本未经另行授权不删除。
