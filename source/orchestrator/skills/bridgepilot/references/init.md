# init：首次配置与安装

适用 `$bridgepilot init`。当前模式不授权业务开发或接管另一控制机。安装与恢复完整说明统一从产品根 INSTALL.md 进入。

1. 先读本机绑定及既有配置；有已确认的 workspace／同步配置时直接复用，不重复询问，不创建第二份空档案掩盖错误。完全没有 BP 时，读取用户指定产品仓库的正式 Release 引导，完成下载、真实 digest／manifest 校验后才运行程序；没有 Skill 或 Git／gh 的冷启动也沿该引导，不要求预先装 Rust。
2. `environment-check --context <已有绝对目录>` 核对 Git／gh。冷启动缺少这些依赖时，先沿 INSTALL 使用系统 HTTPS 能力下载并校验包，init 本机工作区并完成 BP 程序安装，再从固定 home 准备依赖；程序安装本身不依赖 Git／gh。确实缺失或不足时，使用本人已授权的 `environment-install --home <已安装BP目录> --scope user --source <来源>`，安装意图和锁固定在该 home，不随项目工作目录变化。只处理 Git 与 GitHub CLI；用户级不可用、winget 缺失、安装失败／未知、需管理员或重启时报告实际状态，不自动改 scope 或提权。机器级安装须明确授权才可使用 `--scope machine`。成功但当前 PATH 尚未刷新时核实实际安装路径／新终端，再检查，不重复安装。登录本人完成，不在聊天里索要令牌；不得复制旧机凭据。
3. 仅全新用户确认一次私有存档库：读取真实 GitHub.com 登录身份，建议 `<本人账号>/BridgePilotData`，也允许本人指定已有库。记录真实回答后才绑定或新建；仓库名只是建议，不是创建／上传授权。本人明确选择新建时，通过当前真实 GitHub／gh 工具创建 **private** 库并读回；创建结果不明只查询。不要从公共包默认读取开发者的私人库。
4. 目录选择与规则见 INSTALL：个人工作区与产品 home 分离。底层 `init --workspace` 创建本机身份；`sync-configure --workspace --input` 以 JSON 保存本人确认的 repository／main／source。先查询远端是否已有 BP 档案；已有则 `sync-download` 保持观察者身份，不 enrol 或改 owner，按真实 handoff-status 报告。只对确认的新空库，且本机档案没有旧 owner／任务，才使用当前 revision 的 sync-enrol 和 sync-upload 初始化备份。网络 pending／分叉沿既有核销入口处理，不能删记录重试。
5. 先 `install-preview` 后 `install`，指定已核实包、home、workspace。安装成功后，解析本机 `CODEX_HOME`（未设则本人 `~/.codex`）下的 skills 目录，运行 `skill-install-preview --home --skills-root`，再 `skill-install`，最后 `skill-status`。三个入口均由 BP 自管，不使用 Forge 共享分发器，不覆盖未知或用户改动的同名 Skill。中断用 `skill-recover --home --skills-root`。程序安装与 Skill 安装分阶段核对，部分成功要准确报告。
6. 用稳定 launcher 的 self-test、install-status、status 和 skill-status 核对实际版本、档案、存档库、控制机及可发现入口。Codex 未发现新 Skill 时如实报告需要客户端刷新／新对话，不能把文件已写入当发现成功。完成后引导登记项目；不自动创建聊天、开始执行或恢复 heartbeat。已有档案需要换机接管时，用户显式调用 handoff restore 后再按交接模式处理。
