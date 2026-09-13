# 技术设计：Agents 中央列表按工具特有字段配置

## 1. 总体思路

在既有「中央记录 + 分配 + 整文件投影 + 预览 / Apply」管线上叠加一个**按工具的覆盖层**，
不改动交集字段、目标模型与同步链路：

```
agents (交集四字段)                       agent_tool_settings (agent_id, tool) → settings_json
        │                                            │
        └───────── build_agent_projection(tool, record, settings_for_tool) ──────────┐
                                                                                     ▼
                  claude/cursor/zcode/opencode: YAML frontmatter(交集 ∪ 覆盖) + 正文
                  codex: TOML {name, description, developer_instructions} ∪ 覆盖
```

核心决策：

- **覆盖层独立成表、按 `(agent_id, tool)` 存一份 JSON**，而不是在 `agents` 表加列或加单个 `extra_json`。
  理由：字段属于具体工具，Claude 的 `color` 不应出现在 Codex 投影；每工具一行也使后续给
  Cursor / OpenCode / ZCode 加白名单时零迁移。
- **白名单结构体校验，不自由透传**。每个工具一个 `#[serde(deny_unknown_fields)]` 结构体，
  未知键、类型不符、枚举外值均拒绝；与 MCP `extra` 的 `validate_extra` 精神一致但更严格。
- **覆盖层版本由 `agents.row_version` 统一承载**。写覆盖层走 `verify_row_version` +
  `touch_versioned_row`，与分配写入同一模式；`prepare_agents_sync` 绑定的 row versions 无需改动，
  已持久化预览在覆盖层变更后自然失效。

被否决的替代方案见 §8。

## 2. 领域合同

### 2.1 domain / agents 模型（`src-tauri/src/agents/models.rs`）

```rust
#[derive(Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeAgentSettings {
    pub model: Option<String>,
    pub color: Option<ClaudeAgentColor>,      // 八色枚举，serde 小写
    pub tools: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexAgentSettings {
    pub model: Option<String>,
    pub model_reasoning_effort: Option<CodexReasoningEffort>, // low/medium/high/xhigh/max/ultra
    pub features: Option<BTreeMap<String, bool>>,
}

/// 单个 Agent 的全部工具覆盖；缺省工具 = 无覆盖。
#[derive(Deserialize, Serialize, Type, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolSettingsDto {
    pub claude: Option<ClaudeAgentSettings>,
    pub codex: Option<CodexAgentSettings>,
}
```

- 校验入口 `validate_agent_tool_settings(tool, value: &serde_json::Value) -> Result<ValidatedToolSettings, AppError>`：
  - 先按 `tool` 反序列化到对应结构体（`deny_unknown_fields` 拒绝未知键）；
  - 再做 PRD 列出的取值校验（`model` 非空/无空白/≤128、`tools` 项正则与去重、`features` 键正则与数量上限）；
  - Cursor / OpenCode / ZCode → `AppError::invalid_input("tool", "AGENT_TOOL_SETTINGS_UNSUPPORTED")`。
  - 校验通过后**规范化**：`tools` 去重保序；全部字段为 `None` / 空集合的设置视为「无覆盖」→ 删除行。
- `ValidatedToolSettings` 持有规范化后的 `serde_json::Value`（BTreeMap 键序稳定），写库时序列化为紧凑 JSON。

### 2.2 DTO 变化

- `AgentDto` 新增 `tool_settings: AgentToolSettingsDto`（列表与详情均带；无覆盖为全 `None`）。
- 新增 `SetAgentToolSettingsInput { agent_id, tool, settings: Option<serde_json::Value>, row_version }`；
  `settings = None` 或空对象 = 清除该工具覆盖。返回更新后的 `AgentDto`。
- `CreateAgentInput` / `UpdateAgentInput` **不**携带覆盖层（保持交集校验路径不变，前端保存时先 CRUD 再逐工具调用设置命令；单次弹窗提交串行两到三次调用，失败时以最后一次错误提示，已成功部分保留）。
- `AgentImportCandidateDto` 新增 `retained_fields: Vec<String>` 与 `tool_settings: Option<serde_json::Value>`；`ConfirmAgentImportInput.agents` 元素改为 `{ definition: CreateAgentInput, tool_settings: Option<serde_json::Value> }`。

## 3. 数据库（迁移 `0023_agent_tool_settings.sql`）

```sql
CREATE TABLE agent_tool_settings (
    agent_id TEXT NOT NULL REFERENCES agents(id) ON UPDATE CASCADE ON DELETE CASCADE,
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    settings_json TEXT NOT NULL
        CHECK(json_valid(settings_json) AND json_type(settings_json) = 'object'
              AND length(settings_json) <= 16384),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(agent_id, tool)
);
```

- `tool` CHECK 只放行首期两工具，形成服务层之外的第二道边界；后续扩展工具时随新迁移放宽（与 0022 对 ZCode 的做法同型）。
- 不改 `managed_targets`、`agents`；不需要 writable_schema。
- `db/mod.rs` 注册 23；`app/mod.rs` 三处 schema_version 断言 22 → 23；升级测试：从 v22 升级、旧 agents 行保留、级联删除、JSON CHECK 金丝雀、tool CHECK 拒绝 `cursor`。
- `db/agents.rs` 新增：
  - `tool_settings_for_agent(database, agent_id) -> BTreeMap<Tool, Value>`；
  - `tool_settings_for_all_agents(database) -> BTreeMap<String, BTreeMap<Tool, Value>>`（列表页一次查询）；
  - `upsert_tool_settings(database, agent_id, tool, settings_json, expected_row_version)` 与
    `delete_tool_settings(...)`，事务内 `verify_row_version` → 写 → `touch_versioned_row`（镜像 `set_global_assignment`）。
  - `AgentRecord` 不变；覆盖层由服务层按需拼装，避免 `list_assigned_agents` 等既有查询改签名。

## 4. 服务层

### 4.1 `service_core.rs`

- `set_agent_tool_settings(database, input)`：校验 → 规范化 → 空则删除、否则 upsert → 返回 `AgentDto`。
- `agent_dto` / `agent_dto_with_assignments` 增加覆盖层拼装；`list_agents` 用批量查询避免 N+1。
- `prepare_agents_sync`：为每个 desired agent 取该工具的覆盖（`tool_settings_for_agent` 后按 `input.tool` 取值），传入投影。row versions 绑定不变。
- `delete_agent`：外键级联，无需额外处理。

### 4.2 `service_native.rs` 投影

```rust
pub(super) fn build_agent_projection(
    tool: Tool, record: &AgentRecord, settings: Option<&ValidatedToolSettings>,
) -> Result<Value, AppError>
```

- Claude：frontmatter `BTreeMap<&str, String>` 在 `name`、`description` 之后插入 `model`、`color`、`tools`（`tools` 渲染为 `", "` 连接的单行字符串；空列表不写键）。`BTreeMap` 键序：`color`、`description`、`model`、`name`、`tools`，由 serde_yaml_ng 稳定输出。
- Cursor / ZCode / OpenCode：`settings` 必为 `None`（服务层保证），投影不变。
- Codex：在三字段 JSON 对象上合并 `model`、`model_reasoning_effort`，`features` 作为嵌套对象；`toml_edit::ser::to_document` 会把嵌套对象输出为 `[features]` 子表，顶层键按 BTreeMap 字母序。阶段 0 golden 核验：子表位于所有顶层键之后、重复渲染字节一致。
- 无覆盖时代码路径与现在完全相同，既有 golden 不变。

### 4.3 导入解析

`ParsedAgentFile` 增加 `retained: serde_json::Map<String, Value>`（原始键 → 原始值）；解析器按工具白名单分流：

- Markdown（仅当 `tool == Claude` 时应用 Claude 白名单；Cursor/ZCode/OpenCode 全部丢弃）：
  `model` 取字符串；`color` 取字符串；`tools` 接受 YAML 字符串（按 `,` 切分、trim）或字符串序列。类型不符 → 该候选 `AGENT_FIELD_INVALID`。
- Codex：`model`、`model_reasoning_effort` 取字符串；`features` 必须是表且所有值为布尔，否则 `features` 整体进入 `dropped_fields`（官方允许 `features.network_proxy` 为表，属于不建模范围，不算文件错误）。
- 分流后调用 `validate_agent_tool_settings` 复用同一校验；失败 → `AGENT_FIELD_INVALID` + reason 指明字段。
- `AgentImportCandidateDto.retained_fields` 列出保留键名、`tool_settings` 携带规范化后的 JSON；`dropped_fields` 只剩真正丢弃的键。
- `confirm_agent_import`：每条 `create_agent` 后若 `tool_settings` 非空则 `set_agent_tool_settings`（以创建返回的 `row_version`），同一函数内顺序执行；任一失败即返回错误，已创建记录保留（与 Hooks 导入的逐条语义一致）。

### 4.4 commands / bindings

- `commands/agents.rs` 新增 `set_agent_tool_settings`；`lib.rs` 注册类型（`AgentToolSettingsDto`、`ClaudeAgentSettings`、`CodexAgentSettings`、两个枚举、`SetAgentToolSettingsInput`、导入 DTO 变更）。
- `pnpm bindings:generate && pnpm bindings:check`。

## 5. 前端

- `src/lib/agents-api.ts`：新增 `setAgentToolSettings` mutation，成功后失效 agents 列表与全局状态查询。
- `src/lib/tool-metadata.ts`：新增 `capabilities.agentToolSettings`（Claude / Codex true），导出 `AGENT_TOOL_SETTINGS_TOOLS`；面板只按此集合渲染。
- `features/agents/agent-tool-settings-form.tsx`（新文件，拆出以控制 `agents-page.tsx` 体积）：
  - Claude 面板：`model`（下拉 `inherit/sonnet/opus/haiku/fable` + 「自定义」文本）、`color`（八色下拉 + 空）、`tools`（逗号分隔输入，提交时切分 / 去重，显示为标签）。
  - Codex 面板：`model` 文本、`model_reasoning_effort` 六值下拉 + 空、`features` 键值行列表（键文本 + 布尔开关，可增删）。
  - 本地校验镜像服务端规则并给出中文提示；服务端错误原样显示。
- `agents-page.tsx`：`AgentFormState` 增加 `toolSettings: AgentToolSettingsDto`；弹窗折叠区「工具特有设置」；提交流程：create/update → 逐工具 diff（与初始值不同才调用）→ 完成后关闭；列表行徽标「N 个工具有特有设置」。
- `agent-import-dialog.tsx`：候选卡片显示「将保留：…」「将丢弃：…」；确认负载携带 `toolSettings`。
- 测试：`agents-page.test.tsx` 补表单校验、提交负载、徽标；`agent-import-dialog` 的保留/丢弃展示；`tool-metadata.test.ts` 新集合断言。

## 6. 文档与 spec

- `docs/maintainers/adding-tool-adapter.md` §11.3 首条改为「中央保留交集四字段；工具特有字段通过 `agent_tool_settings` 白名单覆盖层建模，首期 Claude `model/color/tools`、Codex `model/model_reasoning_effort/features`（证据 2026-09-12）」；§11.4 加「覆盖层未知键 / 枚举外 / 类型不符 → INVALID_INPUT」「未支持工具 → AGENT_TOOL_SETTINGS_UNSUPPORTED」；§11.5 Bad 案例改为「自由透传原始 frontmatter」。
- README Agents 段补一句覆盖层说明。
- `.trellis/spec/backend/quality-guidelines.md` 沉淀「交集 + 白名单覆盖层」约定（Agents 条目下）。

## 7. 兼容性与回滚

- 前向兼容：旧库升级后无覆盖行，投影字节不变；旧预览因 row_version 未变仍可 Apply。
- API 兼容：`AgentDto` 新增字段为纯增量；`ConfirmAgentImportInput` 结构变化由 bindings 同步，前后端同版本发布。
- 回滚：先隐藏前端折叠区与 `AGENT_TOOL_SETTINGS_TOOLS`（投影仍读取已存覆盖，磁盘文件不变），再移除命令与投影合并，最后迁移前向保留（表留存无副作用）。

## 8. 权衡与被否决方案

- **单列 `agents.extra_json`（MCP 模式）**：无法区分字段归属工具，Claude `color` 会污染 Codex；否决。
- **自由透传原始 frontmatter / TOML**：无法做 UI 与导入类型拦截，与项目 fail-closed 原则冲突；作为将来"高级模式"候选，不在本任务。
- **`model` 提升为交集字段**：模型 ID 跨工具不通用，上一任务已否决；维持。
- **覆盖层随 `CreateAgentInput` / `UpdateAgentInput` 一次提交**：会把五工具校验塞进交集路径，且导入确认也要复制；改为独立命令 + 前端串行调用，代价是一次保存可能产生多次 row_version 递增，可接受。
- **覆盖层独立 `row_version`**：需要预览额外绑定实体类型，改动 `DatabaseEntityType` 与预览持久化；用 `agents.row_version` 承载更简单且语义正确（覆盖层是 Agent 的一部分）。
- **`tools` 存为字符串而非列表**：列表便于 UI 标签化与去重，渲染时再拼接；官方示例为逗号串，两种导入格式均接受。
