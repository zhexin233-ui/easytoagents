# 技术设计：Agents（子代理）全局级与项目级管理

## 1. 总体思路

完整复用「中央库 + 分配 + 持久化预览 / Apply + 快照 / 恢复」管线，不新增并行机制：

```
前端 agents-page / project-detail-page(Agents 页签)
  → commands/agents.rs (tauri-specta)
    → agents/service_core.rs (prepare_agents_sync)
      → tool.adapter().discover() 取 ArtifactKind::Agent 的目录 descriptor
      → repository::list_assigned_agents(tool, project_id|None)
      → 为每个 assigned agent 派生一个"文件级 descriptor"（目录 + <name>.<ext>）
      → build_desired_projection(tool, agent)（整文件投影）
      → sync::scan_target / build_preview_plan / apply_persisted_preview（完全复用）
        → managed_targets（每文件一行）+ snapshots
```

唯一新增的领域概念：`ArtifactKind::Agent`（序列化值 `"agent"`）。

**核心决策：一个受管文件 = 一个 `managed_targets` 行，整文件所有权（`WholeDocument`）。**
不引入"受管文件目录"这种新 `TargetFormat`，也不使用 `managed_items`。
理由：Prompt（Markdown 整文件）与 Codex Provider（TOML）已经证明整文件目标的观测、渲染、
快照与恢复链路成熟；`uq_managed_targets_identity` 以 `target_path` 参与唯一键，天然允许同一
`(tool, artifact, scope, project)` 下多个文件目标；`sync_targets` 以 `(run_id, target_id)` 唯一，
一次预览运行可包含多个目标。代价是"目录级状态"要在服务层聚合（§4.3），可接受。

被否决的方案 B（目录级目标 + `managed_items` 按文件名）：需要新 `TargetFormat`、新的
observe/render 分支、新的 Mutation 变体，并把"每文件按工具渲染不同内容"塞进目前只处理
符号链接名的目录机制，改动面大且与 Skills 的接管语义纠缠。

## 2. 领域合同（domain + adapters）

### 2.1 domain/mod.rs

- `ArtifactKind` 增加 `Agent => "agent"`，更新序列化往返测试。
- 新增 `AgentName` 校验：`^[a-z0-9][a-z0-9-]{0,63}$`。这是五工具的交集：
  ZCode 命令/技能名规则 `^[a-z0-9][a-z0-9_:-]{0,63}$` 去掉 `_` 与 `:`；Claude 建议小写连字符；
  Codex `name` 作为 spawn 标识；文件名同时用于 OpenCode / Cursor 的缺省名称。
- `Scope` 不变。

### 2.2 adapters/mod.rs

- `ASSIGNABLE_AGENT_TOOLS: [Tool; 5] = Tool::ALL`（全局层五工具全支持）。
- `PROJECT_AGENT_TOOLS: [Tool; 4] = [Claude, Codex, Cursor, Opencode]`（ZCode 排除）。
- 新增 `agent_file_extension(tool) -> &'static str`：Codex `toml`，其余 `md`。

### 2.3 各 adapter `discover()` 新增目录 descriptor

descriptor 的 `path` 指向**目录**，`format` 为该工具文件格式，`managed_selectors` 为空
（整文件），`allowed_root` 由 `populate_descriptor_allowed_roots` 推导为该目录。

| 工具 | Scope | 目录 | Format | capability / policy |
| --- | --- | --- | --- | --- |
| claude | Global | `<claude_config_dir>/agents` | Markdown | tool_capability；沿用 `customization_policy.skill` 同类策略字段（`strictPluginOnlyCustomization` 封锁本地 agents，见 cc-skills.md L119） |
| claude | Project | `<root>/.claude/agents` | Markdown | 同上 |
| codex | Global | `<codex_home>/agents` | Toml | tool_capability |
| codex | Project | `<root>/.codex/agents` | Toml | tool_capability + `.trust(project_trust)`（与项目 MCP/Skills/Hooks 同） |
| cursor | Global | `~/.cursor/agents` | Markdown | tool_capability |
| cursor | Project | `<root>/.cursor/agents` | Markdown | tool_capability |
| zcode | Global | `~/.zcode/agents` | Markdown | tool_capability |
| zcode | Project | 无路径 | — | `TargetCapability::unsupported("ZCODE_PROJECT_AGENTS_UNSUPPORTED")` |
| opencode | Global | `<opencode_config_dir>/agents` | Markdown | 沿用 OpenCode `opencode_disabled()` 门禁 |
| opencode | Project | `<root>/.opencode/agents` | Markdown | 同上 |

目录 descriptor 本身不能直接 `scan_target`（会把目录当文件读）。服务层通过
`descriptor.for_agent_file(name, ext)` 克隆出文件级 descriptor（`path = dir/<name>.<ext>`，
其余字段原样），所有 scan / preview / apply / restore 只处理文件级 descriptor。
`allowed_root` 保持为目录，写入边界不变。

### 2.4 原生投影（build_desired_projection）

中央记录 `{name, description, prompt}` → 每工具整文件：

- **Markdown 系（claude / cursor / zcode）**：投影值 `Value::String(text)`，其中
  ```
  ---
  name: <name>
  description: <description>
  ---

  <prompt>
  ```
  frontmatter 由 `serde_yaml_ng` 序列化 `BTreeMap`（键序确定、自动处理引号与多行），
  不手写拼接。正文末尾保证单个换行。
- **opencode**：同上但 frontmatter 为 `description` + `mode: subagent`，不写 `name`（OpenCode 以文件名为名）。
  固定 `mode: subagent` 是为了避免中央子代理被当作主代理出现在 Tab 切换里。
- **codex**：`TargetFormat::Toml` + `WholeDocument`，投影为 JSON 对象
  `{"name", "description", "developer_instructions"}`，由现有 TOML 整文档渲染分支输出。
  `developer_instructions` 多行文本由 `toml_edit` 自动选择多行字面量。

投影为确定性输出，重复渲染字节一致，保证漂移判定稳定。

### 2.5 删除语义

`prepare_agents_sync` 计算 desired 名称集合后，读取该 `(tool, scope, project)` 下所有既有
`managed_targets`（artifact = agent），名称不在 desired 中（停用 / 取消分配 / 中央删除）的目标
以 `delete_target = true` 进入预览，Apply 走现有 `Mutation::Remove`（删除前快照，可恢复）。
中央记录删除受 `ON DELETE RESTRICT` 约束：存在分配时先解除分配。

## 3. 数据库（迁移 0022_agents.sql）

1. 新建表（风格随 0014）：
   - `agents(id PK, name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK(name GLOB 规则 + 长度 1..64), description TEXT NOT NULL CHECK(length 1..1000), prompt TEXT NOT NULL CHECK(length 1..65536), enabled, row_version, created_at, updated_at)`
   - `agent_global_assignments(tool CHECK IN ('claude','codex','cursor','zcode','opencode'), agent_id FK RESTRICT, PK(tool, agent_id))`
   - `agent_project_assignments(project_id FK CASCADE, tool CHECK IN ('claude','codex','cursor','opencode'), agent_id FK RESTRICT, PK(project_id, tool, agent_id))` —— ZCode 在表级即被拒绝，形成服务层之外的第二道边界。
   - 全局 / 项目互斥触发器四条，镜像 0014。
2. `managed_targets` writable_schema 放宽（每条限定表名 + 精确旧锚点 + `instr > 0`；锚点以 0018 / 0019 之后的当前文本为准，实施前必须 `SELECT sql FROM sqlite_master` 核对一次）：
   - `CHECK(artifact_kind IN ('provider', 'prompt', 'mcp', 'skill', 'hook'))` → 加 `'agent'`
   - 项目作用域 `artifact_kind IN ('mcp', 'skill', 'hook'))` → 加 `'agent'`
   - Cursor 约束 `artifact_kind IN ('mcp', 'skill', 'hook', 'prompt')` → 加 `'agent'`
   - OpenCode 约束 `artifact_kind IN ('provider', 'prompt', 'mcp', 'skill')` → 加 `'agent'`
   - 追加 ZCode 作用域约束：在 `tool TEXT NOT NULL CHECK(... )` 末尾追加 `AND (tool != 'zcode' OR artifact_kind != 'agent' OR scope = 'global')`
3. `managed_items` **不变**（整文件目标不用 per-item 基线）。
4. `db/mod.rs` 注册 version 22；`app/mod.rs` 三处 schema_version 断言 21 → 22。
5. `db/agents.rs`：镜像 `db/hooks.rs`（CRUD、分配、`list_assigned_agents(tool, project_id|None)`、按 `(tool, scope, project_id)` 列出 agent 受管目标）。
6. 升级测试：从 v21 升级、旧行保留、五处 CHECK 金丝雀（含 zcode 项目 agent 目标被拒）、外键 / 索引 / 重开。

## 4. 服务层（agents/）

模块拆分镜像 hooks：`models.rs`、`service_core.rs`、`service_native.rs`（投影 + 解析）、`import.rs`、`service_tests.rs`。

### 4.1 公开面

- CRUD：`list_agents / get_agent / create_agent / update_agent / set_agent_enabled / delete_agent`
- 分配：`set_global_agent_assignment / set_project_agent_assignment / list_agent_projects / list_agent_project_options`
  - 校验：`agent_scope_supported(tool, scope)`；ZCode + Project → `ZCODE_PROJECT_AGENTS_UNSUPPORTED`。
- 状态：`list_global_agent_target_statuses`（§4.3 聚合）
- 同步：`preview_agent_sync / apply_agent_preview / readopt_agent_target`
- 导入：`discover_agent_import(tool) / confirm_agent_import`

### 4.2 prepare_agents_sync

1. 取目录 descriptor；Unsupported / ToolNotInstalled 直接返回对应诊断，不产生目标。
2. desired = 已分配且 enabled 的 agent（项目级另加全局继承）。
3. 为每个 desired agent 派生文件级 descriptor 与投影，放入预览目标列表。
4. 既有受管目标中名称不在 desired 的加入删除列表。
5. 空 desired 且无既有目标 → 不建目标、不建运行（与 MCP "空投影不建目标"一致）。

### 4.3 全局状态聚合

`list_global_agent_target_statuses` 对每工具返回一条 `AgentToolTargetStatusDto`：
`tool, directory_path, aggregate_status, files: Vec<AgentFileTargetStatusDto>`。
`aggregate_status` 取文件状态中最严重者（顺序：failed > parse_error > permission_denied >
policy_blocked > untrusted > target_type_changed > external_owned_change >
external_non_owned_change > missing > in_sync）；无受管文件时为 `missing`，前端显示"未同步"。
文件级状态复用 `SyncStatus` 与 `globalTargetStatusPresentation`。

### 4.4 导入解析（service_native.rs）

- Markdown 系：按 `---` 分隔 frontmatter，`serde_yaml_ng` 解析为 `Mapping`；提取 `name`
  （缺省文件名去扩展名）、`description`；其余键收集为 `dropped_fields: Vec<String>`。
- Codex：`toml_edit` 解析；`name / description / developer_instructions` 必填；其余键为 dropped。
- 校验：名称交集规则、description / prompt 长度；失败即 `importable: false` + 诊断码
  （`AGENT_NAME_INVALID`、`AGENT_FRONTMATTER_INVALID`、`AGENT_REQUIRED_FIELD_MISSING`）。
- 只扫描目录**直属**文件，不递归；符号链接、子目录、非 `.md` / `.toml` 一律跳过并计数。
- 导入沿用 Hooks 简化模式：只读发现 → 用户显式选择 → 仅创建中央记录，不做基线接管；
  之后通过常规分配 + 预览 / Apply 进入受管。目标文件内容与投影一致时 Apply 近似 no-op。

## 5. Commands 与 bindings

- `commands/agents.rs`：DTO `AgentDto / CreateAgentInput / UpdateAgentInput / VersionedAgentInput / SetGlobalAgentAssignmentInput / SetProjectAgentAssignmentInput / PreviewAgentSyncInput / ApplyAgentPreviewInput / ReadoptAgentTargetInput / AgentToolTargetStatusDto / AgentFileTargetStatusDto / AgentImportPreviewDto / ConfirmAgentImportInput`，均 `Serialize + Deserialize + Type`。
- `lib.rs` 注册类型与命令；`pnpm bindings:generate` 后 `bindings:check`。

## 6. 前端

- `tool-metadata.ts`：`capabilities.agents`（五工具 true）与 `capabilities.projectAgents`（ZCode false）；导出 `AGENT_TOOLS`、`PROJECT_AGENT_TOOLS`。
- `agents-api.ts`：镜像 `hooks-api.ts`（keys、queryOptions、导入查询 `retry:false, staleTime:Infinity, gcTime:0`）。
- `features/agents/agents-page.tsx`：镜像 `hooks-page.tsx`；表单字段 name（onBlur 校验交集规则并提示）、description、prompt（Textarea）、enabled；全局状态卡按工具展示聚合状态，可展开文件列表。
- `agent-import-dialog.tsx`：候选卡片显示名称、描述摘要、`droppedFields` 提示与不可导入诊断。
- `project-detail-page`：资源页签加 Agents，工具切换只列 `PROJECT_AGENT_TOOLS`；`ProjectAgentAssignments` 镜像 Hooks。
- 路由 `/agents`、`app-shell.tsx` 导航（图标 `Bot`）、`use-persisted-central-list-layout.ts` 加 key、`app-shell.test.tsx` 与 dashboard 断言更新。
- 文案简体中文硬编码。

## 7. 权衡与替代方案

- **交集字段 vs 每工具扩展字段**：选交集。跨工具分配是产品核心；扩展字段需要 5 套 schema 校验与 UI，且各家仍在演进（Codex 官方注明格式会变）。导入时对扩展字段"显示丢弃"而不是静默丢弃，保证知情。
- **`model` 不建模**：模型 ID 互不通用；Claude/Cursor 默认 `inherit`、Codex 走 `[agents]` 默认值、OpenCode 继承主代理。留空即各工具推荐行为。
- **OpenCode 固定 `mode: subagent`**：不写则默认 `all`，中央子代理会出现在主代理切换列表，偏离"子代理"语义。
- **Cursor 兼容目录不观测**：Cursor 会读 `.claude/agents`，同名时 `.cursor/` 优先，因此写 `.cursor/agents` 已足够；观测兼容目录会引入跨工具冲突判定，超出本版。
- **每文件一目标 vs 目录目标**：见 §1。
- **导入不做持久化证据 / 接管**：与 Hooks 一致，省去 import_previews 表；风险是导入后同名原生文件在首次 Apply 时按整文件接管语义处理，预览会展示差异。

## 8. 回滚

- UI / 共享集合先关闭 `AGENT_TOOLS`，再移除 service / commands，最后移除 adapter 分支；迁移 0022 前向保留（放宽的 CHECK 对旧数据无破坏）。
- 任何原生写入失败走现有 snapshot / journal 恢复，不加旁路清理。
