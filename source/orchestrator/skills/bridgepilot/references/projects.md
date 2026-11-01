# project add：登记项目

适用 `$bridgepilot project add <本机目录>`，也可接受明确的自然语言登记请求。

1. 读取本人已配置档案与当前 revision；没有初始化时先沿 init，不猜私有仓库。核对真实目录、Git 根／remote、项目名称，读取该项目实际 AGENTS 与 INSTALL，保留原工程和本机配置。仅登记不安装或升级下游骨架，不改业务文件、不运行项目、不提交或发布。
2. 从当前原生 list_projects 等真实工具取得相应本机 Codex projectId／host；同名项目不以名称猜匹配，核对实际路径。缺少原生项目时沿支持的项目添加界面处理，必要时请用户完成目录信任，不伪造 ID、不改客户端数据库、不用创建聊天代替创建项目。
3. 从已有 projects／bindings 去重，复用同一仓库及项目身份。新项目的名称／仓库不明确才补问；登记项目不是接管在途旧任务。仓库 URL 变化必须由现有 project-add 的真实同 ID 更名核验处理，不能覆盖身份。活跃或 Unknown 的绑定不得替换。
4. 将真实 `{id,name,repository,root,desktop_project_id,host}` 写入工作区 `.runtime` 的输入文件，通过 `project-add --workspace --expected <当前revision> --input` 保存；CAS 漂移重新核对，不盲覆写。空闲边界依既有权限同步私有档案，读取实际回执，结果未知只核销。
5. 返回登记结果、本机目录、仓库、规则入口与兼容性／缺口。后续由用户给明确任务，简单直接推进，复杂先 confirm；不因登记或工程规则缺口自动修改其他项目。
