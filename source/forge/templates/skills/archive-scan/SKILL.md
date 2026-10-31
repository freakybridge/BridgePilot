---
name: archive-scan
description: 扫描已验收的 delivery topic 与已解决 Bug，给出归档候选，经用户逐项确认后使用 git mv 移至 doc/4_archive/ 并同步 doc/README.md；清理完成事项时使用。
user_invocable: true
argument: 无
---

# 扫描并归档完成文档

## 定位与边界

Rust 扫描器统一判定候选，结构检查、CLI 与启动提醒共用入口；用户决定移动。正式需求卡/Bug 收口决定原交付状态，历史扫描报告不单独推翻验收。原需求未关闭的 TODO 阻止归档；独立后续事项继续由总表承接。读取 [TODO 合同](../todo/references/contract.md)核对来源与证据。旧完成标记仅是 legacy evidence，不当作已验收事实；系统架构和外部资料不是候选。

## 输入

无需参数。任何移动都必须以本轮用户明确选择的候选为准。

## 核心流程

1. 主对话直接运行当前 agent 的确定性扫描脚本；此 fast path 禁止为执行脚本启动子 agent：

   ```bash
   .deps/bridgeforge/bin/bridgeforge archive-scan --json
   ```

2. 解析 JSON 的 `schema / count / candidates`；候选字段为 `source / target / kind / topic / path / lifecycle / evidence / reasons`。不猜 score、时间或未返回字段；区分正式收口和 legacy evidence。
3. 复核候选信号。只有候选仍被哪些活跃文档引用、状态证据互相冲突或需要跨多个
   topic 判断时，才把该歧义复核显式分派给 `light-explorer`；零候选、单一明确候选和
   用户已明确验收/解决的候选都留在主对话：
   - 仅子任务完成、仍属活跃验收/里程碑、仍被其他文档引用：说明依据并建议保留。
   - 用户已明确验收或确认 Bug 已解决：可补充为归档候选。
4. 使用当前平台的结构化多选提问，提供“全部归档”“选择归档”“都不移”“再看某个”的操作；列出候选后立即停止本轮。
5. 用户要求“再看”时只读指定文件，说明判断后再次结构化询问；仍不得移动。
6. 用户明确批准后先运行 `git status`：
   移动前复核 TODO 与活动文档引用，方案包含引用迁移与授权。TODO 链接维护交 summary accept，未获对应授权不移动；只迁移相关路径，不结算状态。原需求未完成仍阻断，后续事项保留来源与承接记录。
   - 工作区不干净：列出已有改动并停止，等待用户决定。
   - 工作区干净：对批准文件逐个执行：

     ```bash
     git mv <source> <target>
     ```

7. 移动成功后，把目标中的需求卡或 Bug 更新为 `lifecycle: archived`；原 `superseded_by` 必须保留，旧文档在此时补齐生命周期 frontmatter。
8. 同步 `doc/README.md`：从 current 表格删除对应行，按时间倒序加入 archive 表格；“最近归档批次”注释可选。
9. 再次检查 Git 状态，确认移动文件、生命周期更新与索引修改和用户选择一致。

## 输出与验证

回报实际移动数量、每个源/目标路径和 `doc/README.md` 更新结果。只有 `git mv` 与索引均成功才写“归档完成”。

## 停止条件

- 扫描脚本失败或 JSON 无法解析：报告错误并停止。
- 没有候选：报告“无可归档候选”并停止。
- 用户尚未选择或选择“都不移”：停止，不修改文件。
- 批量操作前工作区不干净：停止并展示现状。

## 禁止事项

- 禁止未经用户确认执行 `git mv`，或在列完候选的同一回合移动。
- 禁止移动后漏改 `doc/README.md`。
- 禁止把长期 memory 总结、系统架构或普通文档新建混入归档流程。
- 禁止把脚本分数当最终判断，忽略活跃引用和用户意见。
