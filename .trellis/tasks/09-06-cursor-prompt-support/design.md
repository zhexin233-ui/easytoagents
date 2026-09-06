# 技术设计：Cursor 提示词支持

## 1. 边界与合同

### 1.1 目标描述符（adapters/cursor/mod.rs）

| artifact | scope | path | format | managed roots | capability |
| -------- | ----- | ---- | ------ | ------------- | ---------- |
| Prompt   | Global  | `~/.cursor/rules/easytoagents.mdc` | `CursorMdc` | `["$document"]` | supported（随安装探针） |
| Prompt   | Project | `<root>/.cursor/rules/easytoagents.mdc` | `CursorMdc` | `["$document"]` | supported（随安装探针） |
| Provider | Global  | 不变（无路径） | Json | — | `CURSOR_PROVIDER_UNSUPPORTED` |

- 受管文件名固定 `easytoagents.mdc`：Cursor 规则目录是用户可自由添加 `.mdc` 的多文件目录，
  本应用只接管自己的单一文件（与 AGENTS.md/CLAUDE.md 的「单文件整文档」模式对齐），
  不做目录级 `$children` 接管。
- `prompt_override: NotApplicable`、`symlink_policy: Reject`、`trust: NotRequired` 不变。

### 1.2 新 TargetFormat 变体 `CursorMdc`

`adapters/mod.rs`：

```rust
pub enum TargetFormat { Json, Toml, Markdown, CursorMdc, SymlinkDirectory }
```

- serde `snake_case` → `cursor_mdc`；`as_str()` → `"cursor_mdc"`；`expected_type()` → `TargetType::File`。
- **渲染**（`render_document` WholeDocument 分支）：正文 `body` 输出为

  ```text
  ---
  alwaysApply: true
  ---

  <body>
  ```

  frontmatter 是应用固定产出，用户不可配置（保证档案正文工具无关）。
- **观测**（`parse_document`）：读 UTF-8 文本后，若以 `---` 首行开始，剥离至闭合 `---` 行
  （含），剩余文本去前导空白后作为 `ObservedDocument::Markdown(body)`；无 frontmatter 则
  原文作为 body。这样 `managed_projection` 与档案正文同域，导入、基线投影、漂移检测
  （`discover_prompt_import` / `confirm_prompt_import` / scan）无需任何 per-tool 分支。
- **full_hash 语义**：full_hash 仍是对整个文件字节计算 —— 与其他工具一致，frontmatter 也
  属于受管文件内容；仅 managed projection 域为正文。
- 解析失败（非 UTF-8）沿用 `AppError::parse`。

### 1.3 明确不做

- 不引入 `description` / `globs` 配置（档案语义是常驻指令，Always Apply 已覆盖）。
- 不管理 `.cursorrules`（官方 legacy）与 AGENTS.md（与 Codex/ZCode 目标冲突）。
- 不做 Team Rules / 账号 User Rules（服务端存储，无文件合同）。

## 2. 数据流

全局：档案启用（`is_active_cursor = 1`）→ `prepare` 取 body 作为 desired_projection →
`CursorMdc` 渲染 → snapshot/journal → 写 `~/.cursor/rules/easytoagents.mdc` → 基线落库。

项目：`prompt_project_assignments(project_id, 'cursor', profile_id)` → 同上写
`<root>/.cursor/rules/easytoagents.mdc`；解除分配清基线、保留文件（既有语义复用）。

导入：`discover_prompt_import` → scan 观测（frontmatter 已剥离）→ 预览正文 → 确认后
`imported_from_path` 指向 `.mdc` 路径并自动启用 `is_active_cursor`。

## 3. 数据库迁移 `0017_cursor_prompt_support.sql`

沿用 0013/0014 已验证的 `writable_schema` 原地文本修订（每条限定表名 + 精确旧锚点 + instr>0）：

1. `managed_targets`：`(tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook''))`
   → `(''mcp'', ''skill'', ''hook'', ''prompt'')`（当前文本为 0014 改写后的锚点）。
2. `prompt_project_assignments`、`profile_import_previews`：
   `tool IN (''claude'', ''codex'', ''zcode'')` → 追加 `''cursor''`
   （0013 改写后的当前锚点）。
3. `ALTER TABLE prompt_profiles ADD COLUMN is_active_cursor INTEGER NOT NULL DEFAULT 0
   CHECK(is_active_cursor IN (0, 1))` + 部分唯一索引
   `uq_prompt_profiles_one_active_cursor`（与 0009/0013 同型）。
4. **不放宽** `provider_profiles` 的 tool CHECK（Cursor Provider 保持拒绝）。

## 4. 服务/DB 层改动点

- `adapters/mod.rs`：`PROFILE_TOOLS` 加入 `Tool::Cursor`。
- `profiles/models.rs`：`NewPromptProfileRecord`、`PromptProfileRecord`（及 DTO 组装处）
  增加 `is_active_cursor`。
- `db/profiles.rs`：
  - `set_global_prompt_assignment`：Cursor 分支读写 `is_active_cursor`（含乐观锁谓词）；
  - `deactivate_prompt_profiles`：Cursor 分支；
  - `set_prompt_project_assignment` 的 `globally_enabled` match：`Tool::Cursor => cursor`；
  - `find_active_prompt_profile` 的 CASE WHEN：加 `WHEN ?1 = 'cursor' THEN is_active_cursor`；
  - 移除三处 `CURSOR_PROMPT_UNSUPPORTED` 守卫（L747/L807/L1175 附近）。
- `profiles/service.rs`：
  - `ensure_profile_capability`：拒绝集合仅剩 `ArtifactKind::Provider`；
  - `cursor_unsupported` 文案更新；`confirm_prompt_import` 的启用位加 `is_active_cursor:
    preview.tool == Tool::Cursor`；`create_prompt_profile` 初始 false。
- `overview/mod.rs`：`active_prompt_name` match 加 `Tool::Cursor`（SQL CASE 加 cursor 分支）；
  `active_provider_name` 对 Cursor 保持 None。

## 5. 前端

- `tool-metadata.ts`：cursor `capabilities.promptGlobal/promptProject → true`、
  `profileRoute → "/cursor"`；`PROFILE_TOOLS` 加 `"cursor"`。
- `router.tsx`：新增 `/cursor` → `<ToolProfilesPage tool="cursor" />`。
- `tool-profiles-page.tsx`：fail-closed 分支条件改为「provider 与 prompt 均不支持」；
  Cursor 渲染状态区 + 提示词说明，跳过 `ProviderPanel`（provider 面板内部已有按能力渲染的
  前提，需核对 `ProviderPanel` 是否也需按 `capabilities.provider` 守卫——以实际实现为准，
  保持 fail closed：不支持时不渲染、不发起 provider 查询）。
- `prompts-page.tsx` / `project-detail-page.tsx` / `dashboard-page.tsx` /
  `onboarding-wizard.tsx`：由 PROFILE_TOOLS / capabilities 驱动，逐文件核对假设
  （如「该工具必有 provider」的隐式断言、空态文案）。
- `bindings` 由 `pnpm bindings:generate` 再生成（TargetFormat 新变体会进入 TS 类型）。

## 6. 兼容与回滚

- 兼容：旧库升级后所有 Cursor 提示词行为默认关闭（无启用位、无分配），零数据迁移风险；
  `.mdc` 文件首次写入即接管该文件（全局首次 Apply 遵循既有接管确认流）。
- 回滚：代码回滚顺序按 adding-tool-adapter.md §8（先 UI/共享集合，再 service/registry，
  最后 adapter 分支）；已应用的前向迁移保留，`is_active_cursor` 列对旧代码无害。

## 7. 测试策略

- 单测：`parse/render` 的 frontmatter round-trip（有/无 frontmatter、CRLF、空白正文）；
  adapter 矩阵更新；迁移 0017（锚点命中、canary、旧数据保留、provider CHECK 仍拒绝）。
- 服务测：Cursor 全局启用 → 预览 → 应用 → 漂移 → 恢复；项目分配/解除；导入剥离 frontmatter。
- 前端：`tool-metadata.test.ts` 能力集合断言更新；相关页面测试按需更新。
