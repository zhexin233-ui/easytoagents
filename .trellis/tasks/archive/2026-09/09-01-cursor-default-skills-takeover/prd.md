# 修复 Cursor 默认技能无法接管

## Goal

让 Cursor 全局 Skills 目标在现有条目已经与中央库技能完全一致时，能够经过用户显式确认安全接管，结束 `SKILL_TARGET_INITIAL_UNMANAGED` 的死路，同时继续保护内容不同、来源不明或已经漂移的外部安装。

用户价值：用户不需要手工删除 Cursor 已有技能入口，也不会因为中央库已存在同名技能而既无法导入、又无法建立受管同步。

## Background

- Cursor 全局 Skills 的正式受管目标为 `~/.cursor/skills`，采用逐名称符号链接模型和 `ManagedChildrenOnly` 所有权；中央副本位于应用私有 Skills 目录（`src-tauri/src/adapters/cursor/mod.rs:70-80`、`src-tauri/src/app/mod.rs:47-55`）。
- 当前实机 `~/.cursor/skills/skill-install` 与 `~/.cursor/skills/smart-search-cli` 都是符号链接，分别指向 `~/.skills-manager/skills/<name>`；接管这两个入口无需修改其外部目标内容。
- 初始目标只有在无基线、无受管条目、无 desired assignment 且目录非空时返回 `SKILL_TARGET_INITIAL_UNMANAGED`（`src-tauri/src/skills/service.rs:415-447`）。复制或识别中央技能不会改变这些条件。
- Skills 检测会把名称、完整树哈希、记录状态与中央副本都一致的候选标记为 `already_imported`，但该状态不可选择，也不会创建 assignment 或受管基线（`src-tauri/src/skills/import.rs:329-377`）。
- 原设计明确规定 Skills 导入只复制中央副本、不接管原安装；复制后继续显示“未纳入同步管理”是当时的预期边界（`.trellis/tasks/archive/2026-08/08-26-skills-global-import/design.md:1-10`、`:101-120`）。本任务有意重新审阅该边界，而不是把现状当作偶发 UI 缺陷。
- 普通同步在 desired 名称遇到同名外部目录或外部链接时继续执行冲突保护，因而“先分配、再 Apply”不能解决当前同名入口（`.trellis/spec/backend/skill-import-guidelines.md` §3、§4）。
- MCP 已有“检测后显式确认、原生字节不变、原子写 assignment 与 ownership baseline”的接管先例，但 Skills 需要额外处理目录入口替换和完整树哈希复验，不能直接复用 MCP 的字段级事务（`.trellis/spec/backend/mcp-import-guidelines.md` §3）。

## Requirements

- **R1 安全资格**：只有候选名称与中央记录名称精确一致、完整树哈希一致、中央记录为 Ready、中央副本复核有效，且目标入口仍与预览时的路径身份、链接文本/目录身份和内容哈希一致时，才允许接管。
- **R2 显式确认**：检测本身保持只读；接管必须通过用户明确选择和确认，不得因打开页面、检测、导入或普通分配而自动发生。
- **R3 持久化 Preview/Apply**：选择接管只写中央分配意图并生成持久化预览；原生入口替换只能由用户随后确认的 Apply 执行。Apply 成功事务必须一起写入真实扫描对应的 target/item baseline，不能留下“链接已换但数据库未接管”或相反的可见半状态。
- **R4 外部来源保护与普通目录恢复点**：对于符号链接入口，只允许替换目标目录中的入口链接本身，不得修改、移动或删除其外部链接目标。对于内容完全一致的普通目录，必须先在应用私有 snapshots 根中创建并复核完整目录树恢复点，再隔离原入口并建立中央链接；恢复点一直保留到用户显式清理，未选择的兄弟条目保持不变。
- **R5 冲突保护**：同名但内容不同、仅大小写相同、中央副本无效、入口在确认前变化、存在活动 Apply/Restore、旧 managed baseline 漂移或目标不可安全读取时拒绝整批接管，并要求重新检测或先处理冲突。
- **R6 状态闭环**：成功接管后目标不再显示 `SKILL_TARGET_INITIAL_UNMANAGED`，而是基于真实扫描和新基线显示 `in_sync`；后续 Preview/Apply/Restore 沿用既有受管同步合同。
- **R7 兼容性**：Claude、Codex、Cursor 共用同一安全资格与事务语义；不得削弱 `.system` 内置排除、未知私有路径拒绝、普通 Skills 复制导入和既有同步冲突保护。
- **R8 交互可解释**：UI 明确区分“复制到中央库”和“接管到当前工具”；精确复用候选可被选择接管，冲突候选仍不可选，并说明接管只替换入口、不会修改外部来源内容。
- **R9 恢复与清理**：普通目录恢复点必须出现在现有恢复点列表中，可预览并恢复为普通目录。恢复后目标按真实状态显示受管内容漂移，不得假装仍为 `in_sync`；只有用户显式删除恢复点时，才能在严格路径、身份和内容校验后递归清理私有快照树。

## Acceptance Criteria

- [x] **AC1**：当 Cursor 目标只有两个外部符号链接，且二者名称和完整内容都与两个 Ready 中央技能一致时，检测结果允许显式选择接管；确认后两个入口都指向应用中央副本，原 `~/.skills-manager/skills/...` 目录及内容完全不变。
- [x] **AC2**：当同名入口是内容与中央副本完全一致的普通目录时，也能显式预览并接管；切换中央链接前应用私有恢复点已经持久化且完整树哈希一致，任一提交前失败都能恢复原路径和目录内容。
- [x] **AC3**：AC1 或 AC2 成功后创建/复用正确的 Cursor 全局 assignments、managed target/items 与完整 baseline；重新读取状态为 `in_sync`，不再返回 `SKILL_TARGET_INITIAL_UNMANAGED`。
- [x] **AC4**：用户只选一个匹配候选时，只接管该入口；未选的入口、未知文件和其它兄弟技能保持外部且后续 Apply 不被删除。
- [x] **AC5**：同名不同内容、大小写碰撞、中央记录/副本异常、目标入口或内容在预览后变化时不可接管或确认失败；原入口、外部目标、数据库 ownership 和令牌状态保持安全一致。
- [x] **AC6**：批量接管第二项失败、数据库失败、崩溃或提交结果不确定时，按持久化 journal 与恢复证据回滚或保持阻断，不能覆盖外部目标、丢失原链接文本/普通目录或宣称成功。
- [x] **AC7**：Skills 页面能区分 `importable`、可复用并接管、已受管、名称冲突和无效候选；接管最终仍经过持久化 Preview/Apply，且即使应用设置为直接 Apply 也不能跳过本次接管确认。
- [x] **AC8**：普通目录恢复点在现有恢复点界面标明为可恢复目录树；恢复可重建原普通目录，恢复后目标显示真实冲突/漂移；显式清理可删除恢复点而不能越出应用私有 snapshots 根。
- [x] **AC9**：现有“仅复制导入、不分配不接管”的路径仍可测试且语义清楚；内置技能排除、普通同步冲突、Claude/Codex/Cursor 及现有 MCP 回归通过。
- [x] **AC10**：Rust、前端、生成绑定与前向迁移测试通过，`pnpm check` 和 `git diff --check` 通过；测试仅使用隔离目录，不对真实用户目录执行接管。

## Out of Scope

- 自动接管未经过显式确认和持久化 Preview/Apply 的 Skills。
- 内容不同技能的覆盖、自动改名、自动合并或删除。
- 修改或接管 `~/.skills-manager` 中的中央库/更新机制。
- 改变 Cursor 的 Skills 目标路径、Copy 模式或项目级接管流程。
- 把 Codex `.system` 或其它工具内置技能导入中央库。
- 自动过期或后台清理普通目录恢复点。

## Key Decisions

- 用户确认同时支持内容一致的外部符号链接和普通目录，不把普通目录留作人工迁移。
- 普通目录的原始树语义（相对路径、文件字节、内部链接文本与可执行位）保存为应用私有目录恢复点；inode、mtime 等非 Skill 哈希语义不承诺保持。
- 普通目录恢复点不会在成功后自动删除，只有用户显式清理时才删除。
- 接管仍遵守“Preview 只读原生目标、Apply 才写原生目标”；直接 Apply 设置不适用于首次接管。
