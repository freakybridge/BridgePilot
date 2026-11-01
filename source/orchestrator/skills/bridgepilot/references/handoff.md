# handoff：旧机交出、新机接管

复用现有 pause、checkpoint-probe、私有同步和 handoff Rust 内核；SQLite 是唯一在线任务库，GitHub 保存通过显式共享表白名单生成的一致 SQLite 快照，不上传活库主文件或 JSON 修订链。本机完整备份与共享快照不同，配置、凭据和本机同步 outbox 不上传。不要另造交接档案、脚本执行器或手写 transfer ID。完整恢复说明见产品 INSTALL.md。

## prepare all

显式 `$bridgepilot handoff prepare all` 表示全部暂停，并授权核对后保存、推送已确认开发任务的进度到各项目专用工作分支；不发布正式版。缺少 all／明确范围的暂停请求仍拒绝猜测；整个控制器交接要求全部停止。

1. 核对实际当前控制机、archive revision、注册项目及原生 heartbeat 状态。保持原有暂停偏好；按真实原生接口暂停正在运行的唤醒并保存回执，不手改 automation.toml。拒绝其他机器／未知控制权。
2. 通过现有 pause 的 `{all:true,projects:[]}` 请求全部暂停，观察各原生执行的真实终态、owner/server 已停止。暂停请求不是停止证明。Unknown 必须先 run-reconcile／sync-reconcile 核销，不重复发送、强杀外部进程或迁移未明执行。
3. 盘点每项目实际改动和其授权任务，保存当前控制聊天的有效目标、决定、未完成事项、验证及证据链接到既有项目接续记录（summary switch 规则适用）。代码与正文不写入公共包或原生 Memory。没有本机绑定、缺 git-sync 受管入口、未知改动／仓库／分支时只报告依赖缺口，不替它认证完成。
4. 使用每个项目的已安装受管 git-sync 入口，将本次已确认任务进度保存到 `codex/` 工作分支并推送；分支创建沿实际项目 Git 规范，不把 BP 放到会隐式发布的 main 分支，不提交无关修改／凭据／个人运行数据。提交前按 Skill 审阅真实差异；不自动 install/upgrade 工具，不冒充 release，不跳过 Hook。冲突或推送结果未知保留现场，不强推或重发整个同步。
5. 逐项目 checkpoint-probe，核对真实干净 Git 根、工作分支／HEAD、登记推送端点和固定远端 ref。部分项目未成功时仍保持本机暂停，不发布成功 transfer。完整 checkpoint 列表与用户本次授权来源通过 handoff-release 写入，必须读回远端确认；旧机冻结，原定时唤醒不复活。返回实际交接 ID、每项目检查点与精确阻点。

## restore

显式 `$bridgepilot handoff restore` 授权核验成功后接管控制权，**不授权恢复任务或定时唤醒**。

1. 检测环境及已有安装；缺失则沿 init 中的依赖准备与 GitHub 官方 BP 包引导。复用已确认存档库；全新配置只问一次本人目标，恢复旧机的档案与机器身份保持分离，不能复制旧机 machine.json 或 heartbeat。
2. sync-download 后读取真实 handoff-status。旧机没有已确认释放、传输 pending／Unknown、handoff 已被其他机器接受、原执行未停止等状态均不得强制接管或 enrol。只报告已恢复和仍受阻部分。
3. 所有登记项目按实际仓库和检查点恢复代码；现有本机目录先核对，脏／冲突文件不覆盖。新位置从本机实际存在的目录和 Codex 原生项目成员确定，不把旧机 D盘路径照抄成新机事实。按项目 INSTALL 核对业务环境；本 Skill 的自动依赖安装只限 Git／gh，其他依赖缺口不擅自安装。
4. 使用真实 transfer 与全部本机 bindings，通过 handoff-accept 由远端确认独占接管，再本地激活；竞争或网络未知只核销。成功后核对所有 projects 仍暂停、无任务执行、heartbeat 不恢复。不能用模型声明、下载成功或本地文件存在代替接管回执。
5. 返回实际版本、档案、机器、代码检查点、接管结果与未完成项。用户另行明确恢复范围后才 resume／handoff-continue；未要求创建聊天时不调用 create_thread，保留既有任务、Run、confirm 与审阅状态，不重复已交付工作。
