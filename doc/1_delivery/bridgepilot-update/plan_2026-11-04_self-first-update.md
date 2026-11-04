---
lifecycle: active
validation_status: in_progress
---

# BP 统一升级入口改名与自身优先更新

## 需求与授权

用户确认 `bridgeforge-update` 改名为 BP 专属 `bridgepilot-update`；正常调用先更新本机 BP 程序及专用 Skills，核验成功后默认逐个升级全部已登记下游，支持明确子集并检查暂停、控制权和在途操作。默认不提交、推送或 release；只读／dry-run 保持零写。用户于 2026-11-04 要求开始实施，当前范围为开发及必要验证，正式发布和真实安装分别按明确授权执行，不继承已结束批次的发布许可。

本轮按 M 级组织，主对话实施、最多一名独立复核 Agent、最多两轮整体验证；预计开发及必要验证约 45 分钟。token 未实测。不改私有档案、业务源码或 Forge 公共模板。

## 实现与传播

1. BP install 固定资产保留 `bp.controller.skill.forge-update` ID，新包使用 `bridgepilot-update-skill.md` 和新 home 路径；历史包按旧名称精确识别。旧源文件的退役在程序安装事务中校验归属、原字节并保存恢复信息。
2. 用户级 Skill 投影切换为 `bridgepilot-update`，先完成并验证新入口，再退役受管旧入口的两个文件。schema 2 日志明确更新入口名和旧字节；schema 1 默认旧名，并兼容 0.10.0 的五文件主 Skill 布局。未知内容、用户编辑和外部并发变化保留。
3. 新 Skill 复用 BP 源码更新脚本，以 `source_update_completed` 与实际程序／Skill 核验作为下游写入前置条件。更新后从新入口的下游阶段继续，避免递归自更新。项目范围复用新版 `project-list`；公共 Forge 产物每批准备一次。
4. 更新脚本将已验证安装身份保存到已有更新收据；同官方提交、同安装身份、全部受管文件哈希和 Skill 内容一致时跳过 Cargo、bundle、install，不改写成功收据。无法证明时走正常准备，不按版本字符串猜测。
5. 默认范围与名称改变属于不兼容用户入口变更，BP 候选 1.3.0 → 2.0.0；install 0.7.2 → 0.8.0、orchestrator 0.16.0 → 0.17.0，Forge 17.3.0 不变。root AGENTS 项目专区、README、INSTALL 和 BP 自有引用同步；不把 BP 事实下沉到公共模板或 dogfood 公共源码。

## 验证安排与结果

验证围绕旧包／旧日志兼容、改名逐写入与退役边界、编辑保护、更新失败门禁、重复复用及包内入口实际一致性进行，使用现有 Rust 安装测试与 PowerShell 源码更新夹具，不运行无关全量回归。

- `cargo check --locked ... -p bridgepilot-orchestrator` 已通过。
- BP 源码更新脚本 14 项定向回归通过，含实际调用的同身份复用、来源变化、Skill 失效、文件编辑和 pending 拒绝。
- install 包首轮 28 项中 27 项通过；新增旧五文件完成态夹具因 binding 写入序列化格式与其日志字节不同失败，修正夹具使用原日志字节后，该项单独重测通过。保护逻辑未放松。
- 独立复核发现并修正 PowerShell 保留变量名、旧五文件日志及旧启动器新日志兼容边界；增量复核确认上述意见已解决，限定范围内未发现新增明确阻断。
- 最终 `cargo test --locked --manifest-path Cargo.toml --target-dir .build/bridgeforge/test-target -p bridgepilot-install --lib`：29 项通过，覆盖源文件退役恢复、Skill 逐边界恢复、外部编辑保护和版本语义。
- `bridgepilot-orchestrator --features test-fixtures --test orchestration` 的 `skill` 过滤：5 项通过；`portable_install_is_read_only`：1 项通过。首次使用 `setup::` 过滤未匹配用例，不计为验证通过，随后已纠正过滤并执行真实用例。
- `powershell -File scripts/tests/bridgepilot_source_update.ps1 -FullLifecycle`：17 项通过，包含真实 Release 构建、程序及新 Skill 安装、第二次 `mode=reused` / `build_invocations=0` 和私有配置／SQLite 档案不变。隔离来源与安装位于 `.runtime/tests/bpu/1209e2b4/`，没有改真实用户 home。
- `scripts/tests/bridgepilot_update_migration.ps1` 使用保留的真实 0.10.0 包和已发布 1.3.0 包分别验证隔离升级到 2.0.0，新入口就绪、旧源和旧用户入口退役、重复 Skill 安装 no-op 均通过。结果分别位于 `.runtime/tests/bp-update-migration/79e4f063c63844a09889d34fd236b5e0/result.json` 和 `.runtime/tests/bp-update-migration/6229d8d5d0724bc1be2aa3be22a05860/result.json`。
- `manifest --root . --check` 返回 `changed=false`，证明本次不需要改变公共 dogfood 投影；`git diff --check` 通过。用户级自然发现和真实下游调用仍未验证。

## 恢复与交付边界

程序和 Skill 安装仍是两个事务。任一阶段失败均不进入下游写入，保留固定来源、完整包、日志和配置。新 launcher 在内容变化时受管刷新；迁移日志产生后若旧 launcher 尚未切换就中断，使用失败收据 `recovery_core` 指向的固定新 core 恢复，不能声称旧程序可解析新日志。已启用新入口后禁止回退到不支持它的程序。

本机正式 0.10.0 安装、四下游自然调用以及新版本正式发布尚未执行，不以隔离验证代替真实传播。当前没有新的需求分叉待确认。

## 用户验收与发布授权（2026-11-04）

用户明确调用 `summary accept` 和本项目 `git-sync release`，验收上述改名、自身优先升级和安装复用的开发及隔离验证，授权本次 BP 2.0.0 改动正式发布。沿用已完成的 29 项安装测试、6 项集成回归、17 项源码更新检查、真实旧包的两条隔离迁移以及独立复核证据；本次记录不重复执行这些验证。

实际用户安装与菜单发现尚未完成，四下游通过新入口的自然调用仍未验证，本文保持 active / in_progress，不把开发验收等同于真实传播完成。此前开始的本机 1.3.0 更新被用户中断；核对发现更新子进程尚在运行，已按取消意图停止其已验证进程树并保留缓存。本机仍为 0.10.0，未发现程序或 Skill 的 pending 安装日志。本轮先完成正式发布，安装接续另按有效授权执行，不用本地未发布候选替代官方来源。

发布结果以本次受管 CLI 的 `status=released` 且 `publication_verified=true` 为准。主版本 1.3.0 → 2.0.0 对应旧可调用名称退役及默认范围改变；install 0.7.2 → 0.8.0、orchestrator 0.16.0 → 0.17.0，其他组件版本不变。
