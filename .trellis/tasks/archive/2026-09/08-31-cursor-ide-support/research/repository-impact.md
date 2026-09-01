# Cursor 产品扩展：仓库影响面

## 后端与 Adapter

- `src-tauri/src/domain/mod.rs:47`：扩展 `Tool` 稳定枚举与序列化。
- `src-tauri/src/adapters/mod.rs:138`：Cursor 目标必须通过 `TargetDescriptor` 明确 artifact、scope、path、format、ownership、敏感 selector、capability、policy/trust 与 symlink policy。
- `src-tauri/src/adapters/mod.rs:181`、`:203`：扩展工具可用性与显式环境。
- `src-tauri/src/app/tool_probe.rs:97`、`:157`：加入 `agent --version` 探测和版本解析。
- 新建 Cursor Adapter，并在 profiles、MCP、Skills、Projects、Sync、Overview 的 adapter registry 与穷举分支注册。
- Provider 和用户级全局 Prompt 没有官方文件合同，应返回稳定 `unsupported` capability；MCP、Skills 与项目 Prompt 才生成可 Apply 的目标。
- Preview/Apply、baseline/stale 校验、原子写入、快照/恢复可复用；unsupported 必须在计划阶段 fail closed，不能消费预览或产生快照。

## 数据库

- 已有迁移中共有 11 处 `tool IN ('claude','codex')` 或其后续变体，覆盖 Provider、Prompt、MCP/Skills assignment、import preview、managed target。
- 不能修改历史迁移；应追加 `0010_cursor_tool_support.sql`。
- 参考 `0009_prompt_tool_active_flags.sql`，精确放宽 SQLite CHECK schema，同时保留表、索引、外键与既有数据。
- Rust DB 解码位置包括 `src-tauri/src/db/mcp.rs:595`、`profiles.rs:872`、`skills.rs:560`。
- 迁移测试应从 v9 fixture 升级，验证原数据、索引、外键、重复打开和所有目标表接受 `cursor`。

## 前端

- `src/bindings/commands.ts:686` 的 `Tool` 是 Rust 生成合同，必须通过 `pnpm bindings:generate` 更新，禁止手改。
- `src/features/onboarding/onboarding-wizard.tsx:20` 的工具数组与 `Choices` 结构需要加入 Cursor，并验证未勾选时不产生写入。
- AppShell、Router、PlatformAssignmentButton、Dashboard、Profiles、Prompts、MCP、Skills、Projects 中存在大量 `tool === "claude" ? ... : ...` 二元假设。
- 应抽取统一 `ToolMetadata` / capability matrix，集中 label、icon、route 和 artifact 能力，避免第三工具继续复制分支。
- Cursor 没有文件化 Provider/全局 Prompt 时，UI 必须明确显示“不支持”，隐藏或禁用 CRUD/Apply，不能伪装成“未接管”。

## 文档与测试

- 新增维护者文档建议放在 `docs/maintainers/adding-tool-adapter.md`，README 维护/验证段落链接过去。
- 文档按 `artifact × scope × operation` 建 capability matrix，允许 `Supported / Unsupported / Unknown / ToolNotInstalled`，不要强求所有工具能力齐平。
- Pi/ZCode 只作为接入示例和待核验清单，本任务不实现它们。
- 验证至少覆盖 bindings、一致性迁移、Adapter fixtures、服务单测、前端选择/禁用状态和完整 `pnpm check`。

