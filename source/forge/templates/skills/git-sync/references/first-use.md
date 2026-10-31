# 当前发布配置与历史分界

> 支持骨架 8.0.0 及以后；更早项目迁移已退役。下述首次配置/重配置保留正式发布基线，配置项目检查但不认证开发完成。新分工需核验 `package-development-v1`、`package-stability-v1`、`git-sync-development-closure-v1`；详细字段与操作见[版本生命周期合同](schema4.md)。

普通 git-sync 不要求先选择发布策略，不自动迁移或升版。显式 release 才处理以下流程；已确认决定直接继承，不重复访谈。

## 唯一模型

唯一模型是主程序＋多语言包。根 VERSION 表达整体发布，各注册包由 develop 收口维护自身版本，release 只升主程序并汇总累计包版本变化。项目自有子包须登记唯一版本来源；不按文件扩展名把普通目录、第三方 SDK 或骨架工具登记为业务包。已有 Rust/Python 适配继续复用。`release_model=product-packages` 与固定 `strategy=independent` 是当前机器字段，不是新分工的能力证明；`product_manifests` 只表达主程序原生版本载体，不是包统一升版模式。

受管衔接保留可信的最后正式发布基线，以 `.codex/package-closures.json` 单独管理逐包开发收口历史。既有发布只作为历史事实，不伪造新验证；尚未发布的实际包改动必须补齐 develop 收口，不在迁移时默认通过。不能因为切换配置而改业务版本、倒退版本或吞掉未发布变化。

## 当前首次配置与检查配置

1. 从 git-sync --preflight 的 tool 字段核对所需能力，不独立 self-test；缺能力报告维护缺口，不在同步中隐式升级工具或试跑未知参数。
2. 已有 product-packages 合同保持原 baseline，通过显式配置提案补充项目 `development` 检查；没有包变化无需补造收口。未配置的新项目初始化使用 --migration-plan，返回配置、最后正式发布基线和 fingerprint，不写文件。规则分界设在切换时的固定 HEAD，正式发布基线保持独立，累计未发布变化继续参与发布。
3. 写配置前，依据 plan、配置提案与真实 diff 核对策略、身份、已发布版本、未发布变化、输入角色、Hook 适配及后续必须的兼容性决定，汇总缺失决定后才使用唯一确认。CLI 尚不能直接对未落盘迁移配置执行完整 release 预览；无法提前查明必要风险时停止，不先应用配置再逐轮询问，也不承诺任意项目一次确认必定完成。迁移不跳过未发布改动，不把项目构建缓存复制到公共工具。
4. 复用已明确的全部适用决定，使用 --migration-apply --migration-fingerprint <值> --confirmed-choice 应用；flag 不代表新增一次用户询问。它只原子保存配置，不修改版本/锁、不暂存、不提交推送。fingerprint 漂移则重算计划并复核授权范围，不能手改 JSON；发现实质新风险且确认已用掉时停止，不再问第二次。
5. 配置保存成功不等于开发收口完成。按[版本生命周期合同](schema4.md)在 develop 完成受影响包收口，再按根 Skill 完成远端接入、最终主程序预览与发布；复用已有提交推送授权，不重复确认。

上述命令名中的 migration 仍用于 8.x 当前首次配置与重配置，不提供旧项目格式转换。需要登记包或改变归属时，准备项目忽略目录内的 schema 4 提案，通过 --setup-config-file 提交给 plan/apply；baseline 由工具从历史产生，不能由 Agent 伪造。--setup-plan/--setup-apply 使用 --setup-fingerprint；不需要 --strategy，旧 synchronized 选择会被拒绝。

包版本允许不同。切换不改业务版本或锁文件，不允许降版、吞掉未发布变化或反复移动分界。分界后出现旧合同提交仍阻断；不能再自动分界掩盖错误。无法证明身份、归属或版本连续性时保留现场并报告原因。

## 迁移及接入边界

以下补正入口仅适用于已完成 product-packages 切换的配置。旧 schema 4 应先执行一次性切换，不用旧补正入口调整历史分界。

已有 schema 4 在首次正式发布前出现历史 Cargo 外置源码归属缺口时，从已有预检 tool 字段确认包含 `git-sync-baseline-repair-v1`，运行 `git-sync --repair-baseline-plan`。它从固定 Git 历史重建归属，返回完整配置和指纹；核对只增加显式外置 lib/bin/test/bench/example/build 入口及其静态 mod/path/include 可达文件、内容 hash，以及静态图证明缺失的精确文件 `shared_inputs`。历史与当前消费者分别取证，不相互照抄；`ownership_analysis.shared_input_additions` 分列路径、包 id、hash 和各包 manifest/source_roots 证据。既有显式 product/shared 角色优先，不替换或扩大声明，不改变包身份、版本、历史边界或已有 hash 键值。计划通过后复用本次修复授权，以 `--repair-baseline-apply --repair-baseline-fingerprint <值> --confirmed-choice` 原子补正，再继续 release 输入分析。不手改 baseline，不用 product_inputs 掩盖包源码；归属不明仍停止。已正式发布的迁移基线禁止改写。动态宏路径不猜测、不执行；缺失文件、跨语言歧义和未被静态图证明的消费者不自动补共享声明，不能把入口的父目录整体归包。

基线补正缺少适用授权时，只能与其他缺失决定合并为根 Skill 的唯一确认。已经用掉确认机会且出现新的授权缺口时停止，不能再为补正询问；既有维护授权也不能推定为接受任意基线改写或不兼容发布。

- schema 1–3 和旧 schema 4 只作为切换前的导入和不可变历史证据，不执行旧发布算法。首次新合同正式发布前仍核验封存事实与 Git 对象一致；分界之后只接受新模型。普通同步保持不升版。
- 历史 Cargo manifest 中已缺失的显式外置入口只保留精确路径声明，不读取、不递归、不生成 hash；文件出现后才绑定可达内容。精确路径不拥有同名目录后代，归属快照不代表 Cargo 构建有效。
- 静态图仅展开字面量引用；缺失的静态模块/include 不造归属或 hash，inventory 有记录但读取失败仍阻断。非字面量 include 和其他宏保持不透明，不读取本机环境变量或运行构建脚本；不能据此宣称动态消费者完整，动态共享源码须有真实 shared_inputs。
- 包稳定 id 与路径、原生包名分别表达；移动、改名、删除、拆分、合并保留显式生命周期与退役约束。
- 锁文件按真实已登记作用域验证，不因不触发包升版而删锁或放宽当前锁校验；历史豁免只沿用可核对的旧合同。
- 动态 setup.py、SCM 后端、未知 lock 或不明归属明确阻断，不执行任意代码猜测元数据。
- 项目自有 Hook 可能仍只识别旧格式；迁移预览不代表 Hook、真实构建或业务 runtime 已适配。涉及真实下游写入时遵守当前任务授权，不能把工厂验证当作真实接入。
