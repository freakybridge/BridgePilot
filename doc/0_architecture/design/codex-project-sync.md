# Template-only 项目同步

当前入口为 `bridgeforge project-sync`，实现位于 `source/forge/workflow/src/template_sync.rs`。下游 `.codex` 递归 JSON 集合严格为 `hooks.json`；不生成项目分发合同、版本配置、开发账本、目录覆盖、Hook 旁文件或骨架版本戳，不把旧数据换目录或格式继续使用。

## 事实源与边界

`source/forge/templates/` 维护公共内容。工厂 `template-standard` 从模板资产及必要内嵌忽略规则生成 `core/src/template_assets.rs`，仅含通用规则、payload 和哈希，不含项目私有配置。工厂 `manifest` 核对该生成物并维护上游产品构建清单。产品清单是工具侧构建定义，不复制为下游配置。

`init/adopt/update/auto` 保留调用兼容，使用同一规则，不解释旧戳、旧版本配置或旧合同决定政策。工厂禁止使用下游同步器接管自身；它使用 factory-template 模型，共享模板入口规则，但维护自己的文档索引和历史资料。工厂文档索引及 seed 不属于工厂入口镜像；此边界不放宽下游完整模板验证。

有效的项目 Hook、Skill、AGENTS 项目区、pre-commit 私有扩展和业务资料保持项目所有。公共非 JSON Rust 源码完整投影到 `.codex/hooks/`，保留现有原生 Hook 对 core 的依赖；源码 JSON 数据文件退出，通用 TODO 规则由 Rust 原生实现承担，旧二进制指纹自动识别机制明确退役。

## 公共工具

固定官方源码的工厂准备入口从公共源投影构建或复用不可变 CLI／Hook 产物。项目事务从 prepared 收据捕获已验证字节；项目级安装既不构建公共工具，也不构建项目自有 Hook。

通用工作流随公共骨架进入 `.agents/skills/`，内部维护入口进入 `.codex/maintenance/bridgeforge-codex/`。工作流以显式路径、稳定 ID 和可验证历史哈希判断归属；已有同名未知内容时暂停并保留现场。工厂使用同一资产规则生成项目镜像。用户级统一指令、原生 Memory Hook 和旧用户级 Forge 退役由独立事务处理。

编译器由受管 builder 注入 `BRIDGEFORGE_BUILD_SOURCE_SHA` 和 `BRIDGEFORGE_BUILD_LOCK_SHA`，工具自检暴露 compiled identity。基线以此作为独立来源期望，核对源码、锁、配方、自检、平台与二进制回执。回执不作为它自己的来源期望。

全仓 Cargo bootstrap 仅是 updater helper；实际安装的用户 CLI 必须来自准备好的公共投影。安装前后继续核验真实 capability、编译环境身份和二进制哈希，不因旧协议缺口跳过检查。

## 计划与事务

1. Plan 零写，校验真实路径，重算产品公共源树／锁身份，核对 compiled identity。未知额外 JSON、链接、未结开发现场或 Git commit guard 阻断并保留。
2. 以通用模板渲染目标，保留项目区和原生 handler；精确读取已知退役名单的原字节，不解释旧配置。模板已有内容的替换与退役列入 risk。
3. 既有公共运行工具必须有可识别 schema2、ID、平台及匹配二进制摘要的回执；未知或改写目标拒绝覆盖。工具原字节进入计划，安装目标限定为两个公共工具及对应回执的固定原生位置。
4. 指纹绑定产品清单、真实源身份、所有目标原状态、递归 JSON 名单及预期写集。Apply 取得项目锁后重算，漂移即停止。
5. 写入／删除前在 `.runtime/tests/template-only-retirement/<指纹>/before/` 校验备份原字节，仅用于恢复；不作为在线配置。
6. 同一事务安装模板、公共源码和公共工具，删除已确认退役项，随后验证完整模板、工具来源和递归仅 hooks.json；最后复核写集未变、退役目标未重建。
7. 失败恢复自身已写／已删内容。外部改写或重建保留并报告回滚不完整，不用 reset、覆盖用户变化或重写 Git 历史恢复。

## 基线和退役入口

`skeleton::template::verify` 校验工作区；`verify_index` 只读暂存模板资产及暂存路径名单，不读取旧合同 blob，不改暂存区。尚未暂存的真实删除可能仍在 index，必须如实阻断；不为证明通过修改真实 index。

普通 git-sync 保留 Git／索引／远端保护，不读旧项目版本配置、不主动升版。配置化 develop、release/setup/migration、项目本地公共工具构建和依赖旧 keep 规则的 cleanup 明确退役；BP 旧账本交付认证也明确失败，不自动将任务标成 Released。原生构建、测试、版本、显式发布和人工审阅按项目实际入口执行。

实现和实盘证据见 [四项目退役交付记录](../../1_delivery/template-retirement/requirements_2026-11-02_four-projects.md)。历史版本戳、schema4 项目合同及账本流程仅作为历史参考，不再作为当前操作指引。
