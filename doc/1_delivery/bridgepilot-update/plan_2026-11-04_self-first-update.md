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

## 同版本重建复用修复验收（2026-11-05）

用户明确调用 `summary accept`，验收本次 BP 同版本源码重建的更新判断修复，并调用 `git-sync release` 授权提交、推送和正式发布。范围仅为本次 BP 私有安装比较、源码更新收据及对应验证；不包含用户级程序安装、真实下游升级或发布。此前各日期的安装状态为当时记录，不代表本次现场。

### 问题与处理

- 本机已安装的 BP 2.2.0 与官方发布资产哈希一致，但独立源码更新器的旧成功收据仍对应 2.0.2；收据失配触发了相同提交 `d6e8eb98a9b0bb5f498339ca3e0eddf26ea0f54a` 的重新构建。
- 逐字节比较确认，core 与 launcher 各有 24 字节差异，全部来自 PE 时间戳与 CodeView PDB GUID。代码区一致，排除这些合法元数据字段后整个文件相同；原安装器按同版本完整文件哈希不同而拒绝安装。
- 新判断保留完整 SHA256 的安装完整性用途，核对相同产品、组件、全部精确资产及当前文件归属；仅上述元数据不同则保留原程序、launcher、安装身份及旧包，不替换文件。程序／launcher 的其他字节、组件和配套资产不同仍阻断同版本复用；无法安全解析的结构保持拒绝。
- 真正复用以 `mode=reused`、`writes=false` 明确表示，不能只用正常安装也会返回的 `installation_current` 判定。安装写锁内重新核对原安装、全部文件、候选文件、Skill 标记及 pending，外部修改保留。
- 旧收据补验成功返回 `reused_after_build`、`build_invocations=1` 并保存本次成功记录；下一次相同源码提交返回 `reused`、`build_invocations=0`。这不等同于仅凭版本号接受任意重建结果。

### 实际验证

1. `cargo test --locked --offline --manifest-path Cargo.toml --target-dir .build/bridgepilot/test-target -p bridgepilot-install --lib`：35 项通过，包含 PE 元数据比较、畸形／截断结构、代码和数据差异、降级与恢复、锁内复核及并发漂移保护。
2. `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/bridgepilot_rebuild_install.ps1 -Core .build/bridgepilot/cargo/release/bp.exe`：真实 EXE 元数据变化可复用、preview 零写、重复调用保持安装文件及历史，其他字节变化、launcher 单独变化、用户修改和 pending 阻断。收据：`.runtime/tests/bp-rebuild/d95c9e0ec0cf4989956a6b4a830028e5/receipt.json`。
3. `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/bridgepilot_source_update.ps1 -FullLifecycle`：21 项通过。隔离官方来源映射、首次源码构建安装、旧收据补验、再次零构建复用均成功，私有配置及 SQLite 档案字节保持。场景位于 `.runtime/tests/bpu/3f8d89cc/`。首次在沙箱中执行因 Git/MSYS 系统对象权限失败，未计为通过；同一测试在沙箱外完成，没有改真实用户 home。
4. 本次实际故障的两份 2.2.0 重放：原 core SHA256 `3fef7e62f788f7db0bcb81c14e302a8f0c1b0bfdff2e2cf0a592a0470e204544`，重建 core SHA256 `4dad8a795fcab71b66f60dc782ca1fbb172622cc8ee2b1ff338bab95a7b03960`；新安装器在隔离目录返回 `mode=reused`，preview 零写且全部安装文件保持。收据：`.runtime/tests/bp-real-rebuild/b2ef5c4de38f44ac8e26e79217451bd0/receipt.json`。
5. 最终锁定离线 Release 构建及 `git diff --check` 通过。独立审查发现的正常安装误报、launcher 单独变化和测试 GUID 定位问题均已修正，最终只读复核无遗留阻断项；没有重复进行业务全量回归。

### 传播与保留边界

| 证据类别 | 本次状态 |
|---|---|
| 源码 | BP install、私有更新脚本及定向测试已实现并验证 |
| 产品传播 | 本地候选已构建；本轮已授权正式发布，以 CLI 真实核验收据为准 |
| dogfood | BP 私有实现，不进入 Forge 公共源码或模板投影；由本仓库候选执行验证 |
| fixture | 35 项单元验证、21 项源码更新检查及真实 EXE 隔离安装通过 |
| 真实下游 | 未执行，不据本次验收关闭四下游升级事项 |
| runtime | 两份真实故障产物的隔离重放通过；真实用户安装仍为官方 2.2.0，未替换 |

本修复是兼容性 patch；主版本由本次原生发布适配器决定。Forge 18.0.0、orchestrator 0.18.0 及 install 原生包 0.10.0 未在本次开发中改变。未找到与本修复唯一对应的独立 TODO，不改其他总管、发布或下游事项；本文整体仍保留真实传播未完成的 active / in_progress 状态。
