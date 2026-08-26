# 实现独立复核与修正

日期：2026-08-26。后端与前端由两个只读探子独立核验，主代理负责结论点验、修改和最终测试；未对真实用户全局目录调用导入或 Apply。

## 后端发现与处理

1. **已修复：跨工具内置别名排除遗漏。** 原 `builtin_exclusions(environment, tool)` 仅在 `tool == Codex` 时生成排除集合，Claude 用户入口可指到 Codex `.system`。现始终从两个 Codex 显式来源建立词法/真实目录排除集合，Claude 和 Codex 共用。
2. **已修复：确认过程中排除集合变化。** 原确认仅在入口重算一次内置集合。现 `validate_sources` 每次重算集合并用排除解析器验证完整来源证据；每项复制前后及事务提交前均经过检查。环境指纹也纳入 Codex 内置来源根，避免 Claude 确认忽略相关根变化。
3. **未成立的线索：集合必须有 SKILL.md 才可解析。** 主代理核对 `library.rs::resolve_skill_source_excluding`，该函数仅解析并证明目录身份，不读取或要求 SKILL.md；inspect 才要求技能文件。现有内置集合测试已验证集合无 SKILL.md 的真实目标别名，补充后的同一测试也遍历 Claude。
4. **枚举上限确认。** `enumerate_skill_entries` 复用 `read_directory_names`，后者在 `MAX_FILES` 处关闭目录流并返回限额错误；不是无界收集。导入层另设 256 个候选预算并报告结果不完整。

回归：扩展 `builtin_collections_aliases_and_symlinked_collection_aliases_are_never_read` 为两工具矩阵；新增 `builtin_aliases_created_during_confirmation_reject_the_whole_batch`，分别在确认前、copy、SQL 时新增 `.system` 指向来源集合的链接，断言失败、无中央记录/副本/staging、令牌未消费。

## 前端结论

独立核验未发现需要修改的问题：生成 DTO、精确确认输入、requestId 查询隔离、默认无选择、候选只读状态、提交同步锁、失败重扫、刷新失败说明、共享焦点 hook，以及 Skills 专用诊断不污染 MCP。探子实跑定向 60 项测试和全量检查均通过；主代理在后端修正后再次运行完整检查。

## 最终验证归属

探子报告不作为最终完成依据。最终全量命令、浏览器实际组件验收及未覆盖项见任务根目录 `verification.md`。
