# Template-only adopt

与 update 使用同一公共模板规则，模式名只保留入口兼容，不解释旧项目配置或版本戳。运行 `project-sync --project-root <项目根> --template-root <已验证产品根> --mode adopt` 生成零写计划；人工定制的替换和已知退役项属于 risk，已有授权直接复用。未知额外 JSON 阻断，不建立 PreservationManifest 或其他替代旁文件。

按 transaction.md 应用精确指纹；不手工复制半套骨架、不写身份戳。项目专属架构、目录地图、快速命令由项目维护，安装恢复仍从 INSTALL.md 进入。
