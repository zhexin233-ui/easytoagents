# 提示词档案工具无关化并按 Skills 范式按工具图标启用

## Goal

提示词页移除 Claude/Codex 工具页签，改为 Skills 页同款单一中央列表：每份提示词档案是工具无关对象，卡片下方提供 Claude 与 Codex 品牌图标按钮，点击即在对应工具上**启用/停用**（每工具至多一份生效，启用新的自动替换旧的）。

## Background（已核实事实）

- 现模型：`prompt_profiles` 每行绑定 `tool` + `is_active`（部分唯一索引保证每工具至多一份生效）；前端按工具页签过滤列表；同步按 `find_active_prompt_profile(tool)` 取生效档案写入 `~/.claude/CLAUDE.md` / `$CODEX_HOME/AGENTS.md`。
- Skills 范式（参照 `skills-page.tsx` + `setGlobalSkillAssignment`）：中央库工具无关，DTO 带 `globalTools: Tool[]`，卡片用 `PlatformAssignmentButton` 图标切换，状态区每工具一张卡（检测导入 + 预览同步）。
- 迁移约束（`.trellis/spec/backend/database-guidelines.md` + 0008 先例）：迁移在 IMMEDIATE 事务内执行且 foreign_keys=ON 不可关闭；`prompt_project_assignments` 对 `prompt_profiles` 有 RESTRICT 外键 → **不可重建父表**；但 `ALTER TABLE ADD/DROP` 列/索引与 writable_schema 的 CHECK 文本原地修订是安全且已有先例的操作。
- 无生效档案 + 已有基线时，全局同步写入空文档（清理语义，既有行为）；导入确认（`confirm_prompt_import`）当前 `is_active: true` 并建立基线。
- `discover_prompt_import` 现以「该工具无档案」为前置；工具无关化后需改为「该工具未启用档案且无同源导入」。

## Requirements

- **R1 工具无关档案**：档案不再绑定工具；新建只需名称 + Markdown 正文；同一份档案可同时启用到 Claude 与 Codex（同一内容写入两工具全局文件）。
- **R2 每工具至多一份生效**：`is_active_claude` / `is_active_codex` 两列 + 各自部分唯一索引；启用 A 时自动停用该工具原生效档案；图标可再次点击停用（清空标志，同步时按既有清理语义处理基线）。
- **R3 中央列表 UI**：移除工具页签；卡片含名称、状态行、正文预览、`PlatformAssignmentButton`（Claude/Codex 图标，启用=选中）、编辑/删除；列表区按钮仅「新增提示词」+ 布局切换；「检测已有提示词」与「预览/直接应用全局同步」移入每工具状态卡（与 Skills 页一致）；状态卡展示 availability、目标路径、promptOverride 警告。
- **R4 项目级兼容**：项目分配列表隐藏「对该工具全局启用」的档案（已分配档案例外保留）；跨工具分配校验随 tool 列废弃而移除。
- **R5 命令与绑定**：`list_prompt_profiles()` 去参；移除 `set_active_prompt_profile`；新增 `set_global_prompt_assignment(tool, promptProfileId, assigned, rowVersion)`；`create_prompt_profile` 输入收敛为 `{name, body}`；DTO 改为 `{id, name, body, globalTools, importedFromPath, rowVersion}`；bindings 再生成。
- **R6 兼容**：迁移把现有 `is_active` 按工具种子到新标志列；总览「当前提示词」、onboarding「promptManaged」改读新标志；导入确认自动启用到来源工具。

## Acceptance Criteria

- [ ] **AC1** 迁移 0009 后：旧生效档案按工具映射到新标志列；可插入 `tool='central'` 新档案；每工具唯一启用由索引强制（含金丝雀测试）。
- [ ] **AC2** 提示词页无工具页签；每卡片 Claude/Codex 图标可启用/停用；启用走中央写入 + 预览/直接应用链路；同工具旧启用档案自动被替换。
- [ ] **AC3** 每工具状态卡提供检测导入与同步入口；导入确认后档案自动启用到该工具。
- [ ] **AC4** 项目分配列表按「对该工具全局启用」过滤；后端仍拒绝把对该工具启用的档案分配到项目（幂等例外保留）。
- [ ] **AC5** 总览与 onboarding 行为不回归。
- [ ] **AC6** `pnpm check` 全绿（含 bindings 一致性）；`cargo test` 通过。

## Out of Scope

- 项目级分配语义（硬拷贝/漂移/解除）不变。
- 不支持一份档案同时写入同一工具的多个目标；不引入第三工具。
- 不改 Skills/MCP/Provider 行为。
