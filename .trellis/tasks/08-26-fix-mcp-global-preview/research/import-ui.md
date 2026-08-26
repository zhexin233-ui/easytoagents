# 导入 UI 研究（只读检索）

## 1. Provider 可复用交互与失效

- `ProviderPanel` 以 `importPreview: ProviderImportPreviewDto | null` 保存发现结果；“检测已有配置”只触发 `discoverMutation`，成功后 `setImportPreview`，中央导入确认是另一条 mutation，明确不改原生文件（`src/features/tool-profiles/provider-panel.tsx:39-40,183-213,285-318`）。
- 确认只提交持久化 `previewId` 与 `suggestedName`：`commands.confirmProviderImport({ previewId, name })`；成功后清空预览、提示、`invalidateQueries({queryKey: profileKeys.providers(tool)})`（同文件 `:203-218,64-68`）。
- 展示投影使用 `targetPath`、`defaultModel`、`importCredentialText` 与 `redactedProjection`；确认/跳过按钮独立于“预览渠道同步”（`:285-318`）。可复用的 mutation 错误汇总为 `profileErrorText`（`:121-135`）。
- Provider 测试 fixture 与断言集中于 `src/features/tool-profiles/tool-profiles-page.test.tsx:154-206,355-418`，已有 `discoverProviderImport`、`confirmProviderImport`、脱敏 DTO、previewId Apply 链路；适合扩展导入 loading/null/错误/确认后列表刷新。

## 2. 最小 MCP 导入 UI 结构建议（依据现有模式）

- 页面已有 query/mutation 状态骨架：服务器、项目、全局目标状态并行 query；所有中央 CRUD/分配成功统一 `invalidateQueries({queryKey: mcpKeys.all})`（`src/features/mcp/mcp-page.tsx:70-91,93-157`）。导入建议增加 `discoverMcpImport` mutation 和 `McpImportPreviewDto[]` 局部状态，不与 `openPreview`（原生同步 Apply）混用。
- 每工具扫描：工具列表 `claude/codex` 各一项，扫描结果按工具显示 loading、无候选、错误；候选行有 checkbox/选择状态，确认按钮只把所选候选提交给独立 confirm command，成功后刷新 `mcpKeys.all`。关闭/跳过只清局部导入状态；重开重新扫描，避免复用过期预览。
- 预览 DTO 应只给名称、transport、command/url、args、header/env 名称或脱敏投影；现有普通 DTO 已遵循 `headerNames/envNames/redactedExtra`，测试断言明确不得出现 secret（`src/features/mcp/mcp-page.test.tsx:39-58,182-201`）。路径显示应沿用现有隔离路径 fixture，避免真实 HOME。
- 冲突/过期由后端错误映射到 `operationError`，确认成功后清空候选；确认按钮提交 previewId/版本或候选 token，不能依赖页面当前字段。Provider 目前仅有单预览、无显式 loading/null UI，可补齐 MCP 专用状态。
- 现有同步预览已经处理空目标：`targets.length===0` 显示“当前没有需要写入全局配置的 MCP。”并不打开 Apply dialog（`mcp-page.tsx:159-180`；测试 `mcp-page.test.tsx:349-390`）。导入空候选应采用同类无动作提示，但不能误当同步空预览。

## 3. 测试 fixture/关键断言

- 首选扩展 `src/features/mcp/mcp-page.test.tsx`：mock commands 对象（`:21-40`）加入新 discover/confirm；复用 `server` fixture（`:39-58`）并新增两个工具、一个冲突候选、一个脱敏字段。
- 关键断言：每个工具各调用一次扫描且参数正确；扫描中按钮/状态；null 候选空态；错误 `role=alert`；多选只提交所选 previewId/token；确认不调用 `createMcpServer` 或 `applyMcpPreview`；确认后 `listMcpServers` 被重新查询；关闭后候选不残留，重开再次扫描；过期/冲突错误不清除既有中央列表且不可重复确认。
- 可参考 Provider 导入命令断言 `tool-profiles-page.test.tsx:355-418`，以及脱敏编辑断言 `mcp-page.test.tsx:203-231`。没有发现独立 `provider-panel.test.tsx`。

## 4. 命令/DTO/Specta/绑定触点

- Rust MCP RPC 全部在 `src-tauri/src/commands/mcp.rs`，每个函数同时标注 `#[tauri::command] #[specta::specta]`（`:14-29`；同步预览/Apply `:122-145`）。新增扫描/确认函数应置于此模块，并委托 `crate::mcp` service。
- 注册清单在 `src-tauri/src/lib.rs:147-159` 的 `collect_commands![]`；新增 command 必须加入。
- Specta DTO 注册在 `src-tauri/src/lib.rs:72-87`（MCP input/DTO），新增 ImportPreview/Candidate/Confirm input 或结果类型必须 `.typ::<mcp::...>()` 注册。
- 生成绑定导出由 `export_typescript_bindings`（`src-tauri/src/lib.rs:176-181` 附近）调用；现有 TypeScript 方法位置：`src/bindings/commands.ts:251-347`，MCP DTO 示例 `:508`。新增命令完成后需重新生成该文件并让测试 mock 同步更新。
- 现有 Rust DTO 定义集中 `src-tauri/src/mcp/models.rs:88-190`；Provider 的 discover/confirm 入口可作为签名参照：`src-tauri/src/commands/profiles.rs:144-180`，其模型 `src-tauri/src/profiles/models.rs:148-178`。

## 事实与推断边界

事实为上述路径、符号、文案与现有断言。MCP “每工具扫描、候选 token/previewId、冲突错误 DTO”是实现建议；仓库当前未发现 MCP 原生扫描/导入 command 或对应 DTO，因此其具体字段需由主代理/后端设计决定。
