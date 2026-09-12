# 修复项目级 Hooks 在项目原生资源中显示为空

## Goal

项目详情页 Hooks 视图的"项目原生资源"分区目前对任何项目都显示为空，
但 picslicer 等项目实际存在 `.claude/settings.json` / `.codex/hooks.json` /
`.cursor/hooks.json` 等项目级 hooks 配置。本任务让项目级 Hooks 像 MCP、Skill
一样被只读识别并列出，消除"有文件却显示为空"的误导。

## Background

- 根因与可复用能力见 `research/root-cause.md`。
- 后端 `src-tauri/src/projects/native_resources.rs` 在 MVP 阶段显式把 Hook
  排除在观测之外（描述符过滤、`observe_items` 返回 `None`），而前端
  `src/features/projects/detail/page.tsx` 已对 Hooks 视图挂载原生资源分区。
- Hook 原生条目是匿名数组元素，现有 ownership 机制无法按单条定位，因此
  本任务只做只读识别，禁用/恢复不纳入。

## Requirements

### R1 项目级 Hook 条目被只读识别

- 对 Claude（`.claude/settings.json`）、Codex（`.codex/hooks.json`）、
  Cursor（`.cursor/hooks.json`）、ZCode（`.zcode/config.json`）四个工具的
  项目级 Hook 目标做只读观测；OpenCode 保持不支持，行为不变。
- 每条原生 hook 条目（按事件 → matcher 组 → 条目拍平；Cursor 为扁平条目）
  登记为一条项目原生资源，`entry_type` 为新增的 `hook_entry`。
- 条目状态只会出现 `active`（当前观测到）与 `missing`（曾观测到、现已被
  外部移除）。不会产生 `disabled` / `conflict`。
- 观测、登记、对账不得改写任何原生文件（沿用既有只读扫描与 IMMEDIATE
  事务原子写库）。

### R2 中央托管条目不重复展示

- 已由中央 Hook 库通过项目分配同步写入的条目（受管 hook 条目，按内容哈希
  匹配）不展示为项目原生资源，与 MCP/Skill 的"中央资源隐藏"行为一致。

### R3 展示信息可辨识且不泄露凭据

- 列表项需要让用户能辨认该 hook：显示原生事件名、matcher（若有）、命令
  与超时。命令若包含可识别的凭据（复用现有 `contains_detectable_secret`
  判定），只显示"已脱敏"提示而不显示命令原文。
- 前端 Hook 条目显示"Hook 条目"类型标签；不显示"临时禁用/恢复"按钮，
  改为说明"Hooks 暂不支持临时禁用与恢复"。

### R4 动作入口 fail closed

- `previewProjectNativeResourceAction` 对 `hook_entry` 资源一律返回
  `INVALID_INPUT`，DTO 的 `canDisable` / `canRestore` 恒为 false。

### R5 数据库迁移

- 新增迁移放宽 `project_native_resources.entry_type` CHECK 以允许
  `hook_entry`，遵循 `.trellis/spec/backend/database-guidelines.md` 的表重建
  策略（不得改写已发布迁移或使用 `writable_schema`）。
- 既有 `mcp_entry` / `directory` / `symlink` 行、索引、触发器与
  `snapshots` 上的交叉保护触发器在迁移后完整保留。

## Acceptance Criteria

- [ ] 后端测试：登记含 Claude 项目级 hooks（多事件、多 matcher 组、无 matcher
      组）的项目后，`listProjectNativeResources(tool=claude, artifactKind=hook)`
      返回对应条目，状态 `active`，`canDisable=false`、`canRestore=false`，
      原生文件字节不变。
- [ ] 后端测试：Cursor 扁平格式与 ZCode `hooks.events` 格式同样被识别。
- [ ] 后端测试：外部删除一条 hook 后重新列出，该条目变为 `missing`，
      其余条目仍 `active`。
- [ ] 后端测试：通过项目分配同步写入的中央 Hook 条目不出现在原生列表中；
      同一文件中的其他原生条目仍列出。
- [ ] 后端测试：对 `hook_entry` 资源调用禁用/恢复预览返回 `INVALID_INPUT`。
- [ ] 后端测试：命令含可识别凭据的条目，`safeSummary` 不含命令原文。
- [ ] 迁移测试：旧库（含 `mcp_entry`/`directory`/`symlink` 行）迁移后行数与
      内容不变，可插入 `hook_entry` 行，`prompt_file` 仍被拒绝，重开幂等。
- [ ] 前端测试：Hooks 视图下原生资源分区渲染 hook 条目（事件、matcher、
      命令、"Hook 条目"标签），不出现禁用/恢复按钮；脱敏条目不显示命令。
- [ ] `pnpm lint`、`pnpm typecheck`、相关 vitest、`cargo test`（projects、
      hooks、db 模块）全部通过。
- [ ] 手动验收：在应用中登记 picslicer，切到 Claude / Codex / Cursor 的
      Hooks 视图，原生资源分区列出对应文件里的 hooks；MCP、Skill 视图不回归。

## Constraints

- 后端 Rust + SQLite，遵循 `.trellis/spec/backend/`；前端遵循
  `.trellis/spec/frontend/`。
- 不改变 MCP / Skill 原生资源的既有行为与测试。
- 不改变中央 Hooks 的同步、导入、接管合同（`hooks/service_core.rs` 只允许
  提高既有函数可见性，不改语义）。
- 不放宽 OpenCode Hooks 的 fail closed。

## Out of Scope

- Hook 条目的临时禁用 / 恢复（需要新的按条目 ownership 设计，另立任务）。
- 从项目原生 hooks 一键导入到中央库（现有导入只支持全局 hooks）。
- Claude `settings.local.json` 等非主配置文件的 hooks。
