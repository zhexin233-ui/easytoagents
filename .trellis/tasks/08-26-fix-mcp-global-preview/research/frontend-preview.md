# MCP 全局预览前端调查

## 已确认事实

- 页面提示的精确分支在 `src/features/mcp/mcp-page.tsx:159-183`：点击状态卡执行 `commands.previewMcpSync({ tool: status.tool, projectId: null, excludeFromGit: false })`；成功回调若 `plan.targets.length === 0`，全局 scope 显示“当前没有需要写入全局配置的 MCP。”并关闭预览对话框。这个判断只看后端返回的 `PreviewPlan.targets`，前端没有二次筛选。
- 全局状态卡及按钮位于 `src/features/mcp/mcp-page.tsx:529-596`。状态查询来自 `globalMcpStatusesQueryOptions()` (`src/lib/mcp-api.ts:38-44`)，仅检测目标能力/策略/路径，不返回原生 MCP 条目。
- 中央候选查询 `mcpServersQueryOptions()` (`src/lib/mcp-api.ts:14-20`) 只调用 `commands.listMcpServers()`；`McpServerDto` (`src/bindings/commands.ts:508`) 只有中央库字段与 `globalTools` 分配信息，没有 native discovery/import 字段。
- 全局分配按钮 (`src/features/mcp/mcp-page.tsx:500-520`) 只调用 `setGlobalMcpAssignment`，其状态显示依赖 `server.globalTools.includes(tool)`。因此只有中央 `mcp_servers` 记录被分配后，页面模型才有“全局候选”。
- 后端全局预览实际候选集在 `src-tauri/src/mcp/service.rs:419-451`：`repository::list_assigned_mcp_servers(database, input.tool, None)` 后再 `.filter(|record| record.enabled)`；全局没有启用且已分配的中央记录时，`desired_records` 为空。
- 空目标返回条件在 `src-tauri/src/mcp/service.rs:452-459`：`desired_records.is_empty() && inherited_records.is_empty() && existing_baseline.is_none()` 时 `target: None`，最终 `build_preview_plan` (`src-tauri/src/sync/mod.rs:542-677`) 得到 `targets: []`。即使原生配置文件已有 MCP，也不会因为该条目存在而进入 desired records。
- 原生配置只在扫描阶段读入：`prepare_mcp_sync` 调用 `descriptor_for` (`src-tauri/src/mcp/service.rs:560-590`) 发现目标描述符，然后 `scan_target(...)`；扫描用于 drift/baseline/冲突判断，不会创建中央 `McpServerRecord`。代码中没有 MCP discover/import command 或对应前端 query（`src-tauri/src/mcp/service.rs:1-240` 的服务入口仅 CRUD、assignment、preview/apply）。
- 生成 native projection 的入口 `build_desired_projection` (`src-tauri/src/mcp/service.rs:608+`) 接收的是 `&[McpServerRecord]`，即中央记录；`native_mcp_item` (`src-tauri/src/mcp/service.rs:627+`) 只是序列化中央配置。现有原生项通过 adapter scan 保留/归属判定，不转成候选。
- 全局 assignment repository 查询 (`src-tauri/src/db/mcp.rs:193-227`, `:355-390`) 仅查询 `mcp_global_assignments` 与中央 `mcp_servers` 表。原生配置中的 Claude `mcpServers` / Codex `mcp_servers` 不会被该查询发现。

## 根因线索（推断）

用户所说“Claude Code 和 Codex 都已有全局 MCP”若指原生配置中已有条目，而这些条目未先手工录入中央库并分配全局，则当前链路必然返回空候选：页面只展示中央库，预览 desired set 也只取中央全局 assignment。原生条目被扫描为外部/非受管内容，不能自动作为要写入的 MCP；因此现象更像“没有中央待写入项”，并非状态 probe 失败。

若用户已经在中央库创建并分别分配 MCP，仍出现空提示，则需核验 `mcp_global_assignments` 是否实际写入、记录 `enabled` 是否为 true，以及调用的 `tool` 是否与 assignment 匹配；上述三项任一不满足都会使 `desired_records` 为空。仅凭前端代码无法断定用户属于哪一种。

## 测试覆盖与缺口

- `src/features/mcp/mcp-page.test.tsx:286-321` 覆盖正常全局预览：mock `previewMcpSync` 返回一个 target，断言打开脱敏对话框并用原 `previewId` Apply。
- `src/features/mcp/mcp-page.test.tsx:350-364` 覆盖 `targets: []` 时显示该空提示、不展示 Apply 对话框。
- `src/features/mcp/mcp-page.test.tsx:324-348` 覆盖 Claude 策略 unknown/blocked 时按钮禁用。
- 测试 fixtures 的中央 server 初始 `globalTools: []` (`mcp-page.test.tsx:38-51`)，没有 UI 流程覆盖“原生已有 MCP → 发现/导入 → 中央候选/全局分配”。也没有断言前端展示 native discovered records，因为当前 API 不提供此能力。
- Rust MCP 服务测试已有大量 preview/apply/原生保留行为（如 `src-tauri/src/mcp/service.rs:1484-1501`, `:1816-1828`, `:1995+`），但检索未发现“原生 MCP 自动导入为中央 server”测试；现有测试更偏向外部条目保留与项目继承空写入。

## 可复用逻辑、候选修复点与待决行为

- 可复用的目标发现/扫描链是 adapter `discover` + `scan_target`（`service.rs:560-590`, `:452+`）；它适合提供“原生条目发现”数据，但当前 scan 输出被用于合并安全性，不能直接当作中央 `McpServerRecord`。
- 最小修复点取决于产品行为：若目标只是让已有原生 MCP 进入中央管理，需要新增明确的 MCP discover/import/adopt API（扫描 native container、脱敏预览、确认后写入中央表，再由用户选择 globalTools），并在 `mcp-api.ts`/`mcp-page.tsx` 增加 query/UI；不能仅修改 `targets.length` 空判断，否则 desired projection 没有可写入内容。
- 若“已有原生 MCP”本意是应被当作已存在且无需写入，可改 UI 空提示为“当前没有中央 MCP 需要写入（原生已有条目不会自动纳入中央库）”，或增加扫描结果说明；这是文案/产品解释，不会导入管理。
- 需要用户/主代理决定：原生已有条目是否自动纳管；同名冲突、secret/header/env 如何确认与脱敏；导入后是否默认全局分配到发现它的 tool；已有非受管项是否应视为目标或仅作为保留项；Claude 与 Codex 格式转换及 Codex project/global 路径策略。

## 未覆盖/存疑

- 未读取用户 home 下任何真实配置或凭证；无法确认实际数据库 assignment 状态及原生文件内容。
- 本调查范围包含了必要的 Rust MCP service/repository 以追踪 query 链；未通读 Trellis 架构/设计等奠基文档。
