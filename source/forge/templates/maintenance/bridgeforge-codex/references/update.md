# Template-only 骨架更新

以产品 Template 和预编译公共工具生成零写计划。旧项目合同、版本配置、账本及骨架身份戳均已退役，不读取它们决定更新政策，也不建立替代载体。

`project-sync --project-root <项目根> --template-root <已验证产品根> --mode update` 输出 aggregate_fingerprint、safe、risk、retired、gaps、blockers。未知额外 JSON 阻断并保留现场；已知退役文件按精确路径和原字节列出风险计划。项目原生 handler、AGENTS 项目区、pre-commit 扩展及业务 Skill 保持项目所有。

安装同时部署并验证项目发布入口。已有 `Cargo.toml` 中的原生发布声明和项目规则必须保留；首次缺入口时，主对话从 INSTALL、原生版本文件、实际 push 目标及已有构建入口整理一次明确的发布规则，作为内部 `--release-spec <本次输入>` 传入计划和 apply。规则只引用原生版本来源，不保存版本值或账本；未知构建／产物范围在安装阶段确认，不等到 release 才补配置。

内部输入使用严格 JSON 对象：`schema=1`、`product`、`repository`、可选 `gitlab_origin`（明确 scheme/host/port）、`version_file`、`changelog`、`tag_prefix`、`include`（精确源码文件或目录）、`build`（项目原生 argv 数组的数组）、`artifacts`（精确 `.build/` 文件）。不包含凭据、自动上传用户资料的通配规则或新版本号。该输入仅服务本次事务，最终规则归入项目 Cargo metadata；不维护第二份在线注册表。

受管驱动由已准备公共 CLI 字节复用，安装到 `.build/project-release/bin/`，无需每仓重复编译。入口及收据必须出现在计划精确写集并参与指纹；不能只写注册而缺程序。缺规则、未知归属、自定义入口冲突或目标不匹配时保留并报告缺口，不输出整体安装成功。无差异只核验，不重写相同文件或恢复副本。

GitLab 身份必须来自已核实实例，不能把所有非 GitHub 远端猜为 GitLab。发布 API 认证使用同主机的 `GITLAB_ORIGIN` / `GITLAB_TOKEN`，或项目忽略目录 `.runtime/config/bridgeforge-gitlab-auth.json` 的 `origin` / `token`；不在命令参数、源码或收据中写令牌。凭据设置与发布结果不能由安装静态就绪代替。

Apply 返回根入口并使用 transaction.md。终态 `.codex` 递归 JSON 集合严格为 hooks.json；不写骨架版本戳。

缺作者身份造成提交前失败、且旧工具无法接续时，已有修复授权可在 plan 和 apply 同时传 `--recover-author-identity`。该模式要求精确证明身份配置追加、旧候选源码未变、完整 before-commit guard 和已完成版本事务；拒绝 retirement、配方请求、根 Cargo.toml 与全部原生版本归属路径的写入。恢复证据、记录原字节摘要和写集外源码进入计划指纹；apply 同时持有项目锁与原生发布锁，完成后原 state/source/guard 保持。工具更新后再由项目实装 CLI 的同名 release 参数归档与恢复，不删除或手改记录。普通更新仍拒绝 pending guard。
