# 修复项目级 Agents 列表未列出项目原生资源

## Goal

项目详情页 Agents 视图目前不显示"项目原生资源"分区，用户在
`.claude/agents/*.md`、`.codex/agents/*.toml`、`.cursor/agents/*.md`、
`.opencode/agents/*.md` 里自行维护的子代理文件完全不可见。本任务让项目级 Agent
文件像 MCP、Skill、Hooks 一样被**只读识别并列出**，消除"目录里有文件、页面却什么都
不显示"的误导。

## Background

- 根因与可复用能力见 `research/root-cause.md`。这是 `09-12-add-agents-management`
  明确写入的非目标，现在补齐；实现骨架可复用 `09-12-fix-project-native-hooks-empty`。
- 后端 `src-tauri/src/projects/native_resources.rs` 在描述符过滤、观测分支、动作所有权
  与解析四处显式排除 Agent；前端 `ProjectResourceKind` 排除 `"agent"`，
  `page.tsx` 在 Agents 视图不挂载原生资源分区。
- 关键约束：Agents 同步按"一个受管文件 = 一行 `managed_targets`"建模，而原生资源
  登记需要一行**目录级**目标身份行。该身份行一旦进入 Agents 同步的删除候选逻辑，
  可能把用户原生文件列为删除目标，因此必须显式隔离并用测试守住。

## Requirements

### R1 项目级 Agent 文件被只读识别

- 对 Claude（`<root>/.claude/agents/*.md`）、Codex（`<root>/.codex/agents/*.toml`）、
  Cursor（`<root>/.cursor/agents/*.md`）、OpenCode（`<root>/.opencode/agents/*.md`）
  四个工具的项目级 Agent 目录做只读观测；ZCode 项目级子代理官方不支持，保持无目标、
  不观测。
- 只识别目录顶层、扩展名等于该工具 Agent 文件扩展名的**普通文件**（含解析为文件的
  符号链接）；子目录、其他扩展名、以 `.` 开头的隐藏文件一律跳过。
- 每个文件登记为一条项目原生资源，`entry_type` 为新增的 `agent_file`，
  `external_key` 为目录内的文件名（含扩展名）。
- 状态只会出现 `active`（当前观测到）与 `missing`（曾观测到、现已被外部移除）；不会
  产生 `disabled` / `conflict`。
- 观测、登记、对账不得改写任何原生文件（沿用只读扫描 + IMMEDIATE 事务原子写库）。
- 目录不存在视为空目录（已有记录转 `missing`）；目录或文件不可读时本轮跳过该目标，
  不改变已有记录状态。

### R2 中央托管文件不重复展示

- 已由中央 Agent 库通过项目分配**应用**（或首次接管时登记了基线）的文件，其对应的
  按文件 `managed_targets` 行带有非空 baseline，这类文件不展示为项目原生资源，与
  MCP/Skill/Hook 的"中央资源隐藏"行为一致。
- 仅由预览创建、尚无 baseline 的按文件行**不构成**中央所有权：该文件仍是原生文件，
  必须继续列出。

### R3 展示信息可辨识且不泄露凭据

- 列表项显示：显示名（frontmatter / TOML 中的 `name`，缺省取文件名去扩展名）、
  文件名、描述（`description`）。描述若包含可识别凭据（复用
  `contains_detectable_secret`），只显示"已脱敏"提示；描述过长时截断。
- 文件无法解析（frontmatter 非法、TOML 非法、非 UTF-8、超过 512 KiB）仍然列出，
  显示名回退为文件名去扩展名，并附带稳定诊断码（复用 agents 模块的诊断码常量）。
- 前端 Agent 文件显示"Agent 文件"类型标签；不显示"临时禁用 / 恢复"按钮，改为说明
  "Agent 文件暂不支持临时禁用与恢复"。

### R4 动作入口 fail closed

- `previewProjectNativeResourceAction` 对 `agent_file` 资源一律返回 `INVALID_INPUT`，
  DTO 的 `canDisable` / `canRestore` 恒为 false；所有按 `entry_type` 分派的动作分支对
  `agent_file` 返回内部错误而不是静默处理。

### R5 数据库迁移

- 新增迁移放宽 `project_native_resources.entry_type` CHECK 以允许 `agent_file`，遵循
  `.trellis/spec/backend/database-guidelines.md` 的 12 步表重建策略（不得改写已发布
  迁移或使用 `writable_schema`）。
- 既有 `mcp_entry` / `directory` / `symlink` / `hook_entry` 行、索引、触发器与
  `snapshots` 上的交叉保护触发器在迁移后完整保留；`prompt_file` 仍被拒绝。

### R6 Agents 同步与目录身份行隔离

- 原生登记为 Agent 目录创建的目录级 `managed_targets` 身份行（空 baseline）不得进入
  Agents 同步的删除候选、也不得触发"空集仍建运行"；Agents 同步只把"路径是该工具
  Agent 目录内合法 `<name>.<ext>` 文件"的行视为受管文件。
- 无任何项目分配时，对已列出原生 Agent 文件的项目执行 Agents 同步预览，必须返回
  零目标且原生文件字节不变。
- 中央 Agents 的分配、预览、应用、重新接管、恢复合同不改变。

### R7 前端展示

- 项目详情页 Agents 视图挂载"项目原生资源"分区，位于"<工具> Agents 项目追加"之上，
  与其他视图布局一致；工具切换沿用 Agents 视图既有的四工具集合（无 ZCode）。
- 前端类型 `ProjectResourceKind` 纳入 `"agent"`，查询键与失效范围随之覆盖 Agent。

## Acceptance Criteria

- [ ] 后端测试：登记含 `.claude/agents/code-reviewer.md`（frontmatter 含 name/description）
      以及干扰项（`notes.txt`、子目录、`.hidden.md`）的项目后，
      `listProjectNativeResources(tool=claude, artifactKind=agent)` 只返回 1 条，
      `entryType=agent_file`、`state=active`、`canDisable=false`、`canRestore=false`，
      `safeSummary` 含 `kind=agent`、`name`、`description`、`fileName`，原生文件字节不变。
- [ ] 后端测试：Codex `.toml`、Cursor `.md`（以及 fixture 可支持时的 OpenCode `.md`）同样
      被识别；ZCode 查询不产生 Agent 原生资源。
- [ ] 后端测试：外部删除一个 Agent 文件后重新列出，该条变为 `missing`，其余仍 `active`；
      目录整体被删除时全部转 `missing`。
- [ ] 后端测试：通过项目分配预览并应用写入的中央 Agent 文件不出现在原生列表；同目录的
      其他原生文件仍列出；仅预览未应用且内容不匹配的原生文件仍列出。
- [ ] 后端测试：对 `agent_file` 资源调用禁用 / 恢复预览返回 `INVALID_INPUT`。
- [ ] 后端测试：description 含可识别凭据时 `safeSummary` 不含描述原文而带脱敏标记；
      frontmatter 非法 / 非 UTF-8 / 超大文件仍列出且带诊断码。
- [ ] 后端测试（安全守卫）：列出原生 Agent 文件后，`preview_agent_sync(tool, projectId)`
      在无分配时返回零目标；在存在其他中央分配时不把原生文件或目录行列为删除目标；
      原生文件字节不变。
- [ ] 迁移测试：旧库（含四种既有类型行与禁用快照）迁移后行数与内容不变，可插入
      `agent_file` 行，`prompt_file` 仍被拒绝，重开幂等，快照交叉保护触发器仍生效；
      `schema_version` 断言更新。
- [ ] 前端测试：Agents 视图调用 `listProjectNativeResources(artifactKind="agent")`，渲染
      Agent 文件（显示名、"Agent 文件"标签、文件名、描述），不出现禁用 / 恢复按钮，
      显示"Agent 文件暂不支持临时禁用与恢复"；脱敏条目不显示描述原文。
- [ ] `pnpm bindings:generate` 后 `pnpm bindings:check` 通过；`pnpm check`（含
      `cargo test`、clippy、fmt）与 `git diff --check` 全绿。
- [ ] 手动验收：登记一个含 `.claude/agents/*.md` 与 `.codex/agents/*.toml` 的真实项目，
      Agents 视图下 Claude / Codex 各自列出原生文件；MCP / Skill / Hooks 视图不回归；
      对该项目做一次 Agents 同步预览不出现意外删除目标。

## Constraints

- 后端 Rust + SQLite，遵循 `.trellis/spec/backend/`；前端遵循 `.trellis/spec/frontend/`。
- 不改变 MCP / Skill / Hook 原生资源的既有行为与测试。
- 不改变中央 Agents 的 CRUD、导入、投影、同步合同；`agents` 模块只允许提升既有函数
  可见性、抽取常量，以及为删除候选增加"路径必须是目录内合法文件"的守卫。
- 不放宽 ZCode 项目级 Agents 的 Unsupported。
- 只读观测不得把文件正文（prompt）写入数据库或 DTO；DTO 只携带名称、文件名、
  描述（经脱敏 / 截断）与诊断码。

## Out of Scope

- Agent 文件的临时禁用 / 恢复（整文件快照与恢复需要扩展 `sync::NativeResourceEntryType`
  与 apply/restore 链路，另立任务）。
- 从项目原生 Agent 文件一键导入到中央库（现有导入只支持全局目录）。
- 全局 Agent 目录中非受管文件的原生登记。
- Cursor 兼容目录（`.claude/agents`、`.codex/agents` 被 Cursor 读取）的交叉观测。
- 嵌套子目录中的 Agent 文件。

## 决策记录

- 2026-09-13 用户确认：禁用 / 恢复不纳入本任务，动作入口 fail closed（R4）；中央所有权
  按"文件路径 + 非空 baseline"判定，不按内容哈希（R2）。
- 2026-09-13 用户指示：规划完成后暂不进入实现，待后续显式要求再执行 `task.py start`。
