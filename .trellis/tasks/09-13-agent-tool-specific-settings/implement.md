# 执行计划：Agents 中央列表按工具特有字段配置

前置：实施前运行 `trellis-before-dev` 读 `.trellis/spec/backend` 与 `frontend` 索引；遵循
`docs/maintainers/adding-tool-adapter.md` 的证据先行与 fail-closed 原则。字段证据见
`research/tool-specific-fields.md`。设计见 `design.md`。

## 阶段 0：前置核验（任何失败回到 design.md 修订）

- [x] 0.1 写一个临时单测：对 `{"name","description","developer_instructions","model","model_reasoning_effort","features":{"a":true,"b":false}}` 调用 `toml_edit::ser::to_document`，确认 `[features]` 子表位于所有顶层键之后、两次渲染字节一致、`developer_instructions` 多行字面量不受影响。核验后把该断言保留为 golden。
- [x] 0.2 确认 `serde_yaml_ng` 对含逗号、冒号的 `tools` 单行字符串会自动加引号（写 `tools: "Read, Edit, Bash"` 用例并核对 Claude 解析器可读）。
- [x] 0.3 核对 `app/mod.rs` 三处 schema_version 断言位置（22 → 23）与 `db/mod.rs` 迁移注册模式。

## 阶段 A：领域模型与校验

- [x] A1 `agents/models.rs`：`ClaudeAgentColor`、`CodexReasoningEffort`、`ClaudeAgentSettings`、`CodexAgentSettings`、`AgentToolSettingsDto`、`SetAgentToolSettingsInput`；`validate_agent_tool_settings(tool, &Value)` 与规范化（`tools` 去重保序、全空 → 无覆盖）；诊断 `AGENT_TOOL_SETTINGS_UNSUPPORTED`。
- [x] A2 单测：未知键、`color` 枚举外、`tools` 项非法 / 超 64 项、`features` 键非法 / 值非布尔、`model` 含空白、JSON 超 16 KiB、Cursor/OpenCode/ZCode 拒绝、全空规范化为 None。
  - 验证：`cargo test --manifest-path src-tauri/Cargo.toml agents::models`

## 阶段 B：数据库

- [x] B1 `db/migrations/0023_agent_tool_settings.sql`（design.md §3 建表；tool CHECK 仅 claude/codex；JSON CHECK；级联删除）。
- [x] B2 `db/mod.rs` 注册 23；`app/mod.rs` 断言 22 → 23；升级测试：从 v22 升级、agents 旧行保留、插入 cursor 被 CHECK 拒绝、非对象 JSON 被拒绝、删除 agent 级联清理、重开。
- [x] B3 `db/agents.rs`：`tool_settings_for_agent`、`tool_settings_for_all_agents`、`upsert_tool_settings`、`delete_tool_settings`（事务内 `verify_row_version` → 写 → `touch_versioned_row`）。
  - 验证：`cargo test --manifest-path src-tauri/Cargo.toml db`

## 阶段 C：服务、投影、导入、命令

- [x] C1 `service_core.rs`：`set_agent_tool_settings`；`agent_dto*` 拼装覆盖层（列表批量查询）；`prepare_agents_sync` 传覆盖到投影。
- [x] C2 `service_native.rs`：`build_agent_projection(tool, record, settings)`；Claude 合并 `model/color/tools`，Codex 合并 `model/model_reasoning_effort/features`；golden：有覆盖 / 无覆盖 / 空列表不写键 / 重复渲染一致；既有 golden 不变。
- [x] C3 导入解析：`ParsedAgentFile.retained`；Claude 白名单分流（`tools` 逗号串与序列均接受）、Codex 白名单分流（`features` 含非布尔值整体丢弃）；类型不符 → `AGENT_FIELD_INVALID`；`AgentImportCandidateDto.retained_fields / tool_settings`；`confirm_agent_import` 写覆盖层。
- [x] C4 服务层单测：CAS 冲突返回 conflict 且 row_version 递增；覆盖变更后已持久化预览失效；导入保留 / 丢弃 / 不可导入三类候选；Cursor 导入含 `model` 仍进 dropped。
- [x] C5 `commands/agents.rs` + `lib.rs` 注册；`tests/command_smoke.rs` 增加设置命令冒烟；`pnpm bindings:generate && pnpm bindings:check`。
  - 验证：`cargo test --manifest-path src-tauri/Cargo.toml`

## 阶段 D：前端

- [x] D1 `tool-metadata.ts`：`capabilities.agentToolSettings`、`AGENT_TOOL_SETTINGS_TOOLS`；`tool-metadata.test.ts`。
- [x] D2 `agents-api.ts`：`setAgentToolSettings` mutation 与查询失效。
- [x] D3 新建 `features/agents/agent-tool-settings-form.tsx`（Claude / Codex 面板 + 本地校验）；`agents-page.tsx` 折叠区、`AgentFormState.toolSettings`、提交串行 diff 调用、列表徽标。
- [x] D4 `agent-import-dialog.tsx`：「将保留 / 将丢弃」展示；确认负载携带 `toolSettings`。
- [x] D5 测试：`agents-page.test.tsx`（表单校验、提交负载、徽标）、导入对话框展示。
  - 验证：`pnpm test --run && pnpm typecheck && pnpm lint`

## 阶段 E：质量门、E2E、文档

- [x] E1 全量质量门：
  ```bash
  pnpm format:check && pnpm lint && pnpm typecheck && pnpm test --run
  pnpm bindings:check && pnpm rust:check
  git diff --check
  ```
- [x] E2 `src-tauri/tests/phase8_e2e.rs`：Claude 与 Codex 各一条「设置覆盖 → Preview(Update) → Apply → 文件含新字段 → 清除覆盖 → Preview(Update) → Apply 回到交集内容」。
- [x] E3 实机 smoke（可选，需本机安装 CLI）：未执行（本机未安装/未授权 Claude 与 Codex CLI）；隔离 fixture E2E 已覆盖同等投影与链路。
- [x] E4 文档：README、`docs/maintainers/adding-tool-adapter.md` §11.3 / §11.4 / §11.5；spec 更新（`trellis-update-spec`）沉淀「交集 + 白名单覆盖层」。
- [ ] E5 提交（`feat(agents): 支持按工具配置 Claude / Codex 特有字段`）。

## 回滚点

- 阶段 A / B / C / D 各自可独立成 commit；失败回退到上一阶段末尾。
- 阶段 0 任一核验失败：停止，修订 design.md §4.2 后再进入 A。
- 运行时回滚：先隐藏前端折叠区与 `AGENT_TOOL_SETTINGS_TOOLS` → 移除命令与投影合并 → 迁移保留。
