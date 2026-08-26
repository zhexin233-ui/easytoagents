# 后端 MCP 全局预览调查

## 事实与链路

- Tauri `preview_mcp_sync` 仅委托 `mcp::preview_mcp_sync`（`src-tauri/src/commands/mcp.rs:122-130`）；后者调用 `prepare_mcp_sync`，构造一个目标请求后 `build_preview_plan` 并持久化 preview（`src-tauri/src/mcp/service.rs:172-214`）。
- 全局/项目候选的唯一数据库来源是 `repository::list_assigned_mcp_servers`：全局 SQL join `mcp_global_assignments`，项目 SQL join `mcp_project_assignments`（`src-tauri/src/db/mcp.rs:355-390`）。因此原生配置文件中的条目不会反向发现、不会自动入库，也不会自动形成全局分配。
- `prepare_mcp_sync` 对查询结果再筛 `record.enabled`（`src-tauri/src/mcp/service.rs:423-437`）；全局时 `desired_records` 是启用且已分配给该 tool 的中央记录。`build_desired_projection` 仅把这些记录按名称转换为 Claude `mcpServers` / Codex `mcp_servers`（`src-tauri/src/mcp/service.rs:605-624`）。
- Claude 全局 MCP descriptor 是默认 `$HOME/.claude.json`（默认 config dir 时）或经过 probe 得到的用户 MCP 路径，scope global、format JSON、selector `mcpServers`（`src-tauri/src/adapters/claude/mod.rs:47-74,103-113`）。Claude 项目目标为 `<project>/.mcp.json`（`src-tauri/src/adapters/claude/mod.rs:129-143`）。
- Codex 全局 MCP descriptor 是 `$CODEX_HOME/config.toml`，scope global、format TOML、selector `mcp_servers`；项目目标为 `<project>/.codex/config.toml`（`src-tauri/src/adapters/codex/mod.rs:37-50,80-95,112-125`）。
- 全局状态接口 `list_global_mcp_target_statuses` 只按 descriptor capability、Claude policy、已持久化 `managed_targets.sync_status` 返回状态；它不扫描“已有原生 MCP 数量”来生成候选（`src-tauri/src/mcp/service.rs:327-371`）。

## 筛选/写入条件

- 全局 scope：`project_id=None`；查询必须有 `mcp_global_assignments`，且 MCP `enabled=true` 才进入 desired projection（`src-tauri/src/mcp/service.rs:407-437`）。disabled 记录被排除，但若已有 baseline/managed item，后续仍可能生成清理型目标（`src-tauri/src/mcp/service.rs:439-475`）。
- 若 desired、inherited 都为空且没有现存 baseline，直接返回 `target: None`（`src-tauri/src/mcp/service.rs:439-449`）；这正是无中央全局分配时预览无写入候选的保护路径。
- Claude descriptor policy 必须 `Allowed` 且 capability `Supported` 才可正常处理；状态接口在 capability 非 Supported 时给 `Failed`，policy `Blocked/Unknown` 时给 `PolicyBlocked`，对应 `CLAUDE_POLICY_BLOCKED` / `CLAUDE_POLICY_UNKNOWN`（`src-tauri/src/mcp/service.rs:347-361`）。Codex descriptor policy 固定 Allowed，但 capability 仍取安装状态（`src-tauri/src/adapters/codex/mod.rs:25-35,80-95`）。
- 已有原生条目仅在“中央已有记录且被纳入 ownership”时参与 managed scan/冲突与 baseline 检查；ownership 名称来自 desired/inherited/existing managed item，绝不会把扫描到的陌生原生名称变成候选（`src-tauri/src/mcp/service.rs:696-712`）。
- 写入候选由 `PreparedMcpTarget` 交给 `build_preview_plan`；apply 前重新 prepare，并校验持久化 preview scope/tool/artifact 及 target 数量，防止 stale preview（`src-tauri/src/mcp/service.rs:286-315`）。

## 现象的确定路径与契约

确定路径：Claude/Codex 已在原生配置存在，但没有对应 `mcp_servers` 中央记录+`mcp_global_assignments`。点击全局预览时 `list_assigned_mcp_servers` 返回空；若无旧 `managed_targets` baseline，`prepare_mcp_sync` 在 `desired_records.is_empty() && inherited_records.is_empty() && existing_baseline.is_none()` 处返回 `target=None`，最终 request 空，UI 得到“没有需要写入全局配置的 MCP”。这与原生配置是否存在无关。

已有保护契约：只读扫描不应因陌生原生条目而接管；项目仅继承全局时还会扫描同名碰撞，若目标缺失或 managed projection 为空则不创建 baseline/空配置（`src-tauri/src/mcp/service.rs:451-463`、`486-490`及注释）。已有 managed item hash 不一致会变成 `TargetScan::ManagedItemBaselineMismatch`（`src-tauri/src/mcp/service.rs:715-741`），避免覆盖外部修改。

## 452dc08 关联性（仅后端 diff）

该提交改动 `src-tauri/src/mcp/service.rs` 的测试与 fixture 辅助：新增“无 policy evidence / blocked policy”状态断言（commit diff，`src-tauri/src/mcp/service.rs` 测试约 1067-1280）。生产逻辑未改动；同时 `adapters/mod.rs` 与 `app/tool_probe.rs` 改变 Claude policy 缺失/无效来源的判定。提交可能影响 Claude descriptor 的 `policy`（从缺失来源时 Unknown 变为 Allowed 的扫描结果），但不会让原生 Claude/Codex MCP 反向入库或创建全局 assignment，故不能解释“已有原生 MCP 没有写入候选”的核心原因。

## 最小回归测试/fixture

复用 `src-tauri/src/mcp/service.rs` 测试模块的 `Fixture`、`stdio_input` 与环境 helper（现有测试约 `1067-1253`）；fixture 已有 `home`、database、ExplicitEnvironment，并验证 native 文件不被 CRUD/preview 直接写入（commit diff 中 `import_preview_and_assignment_crud_never_write_native_targets`）。建议最小测试：

1. 仅写入 Claude `.claude.json` 的 `mcpServers.native` 与 Codex `config.toml` 的 `[mcp_servers.native]`，DB 不插入中央记录/assignment；调用 `preview_mcp_sync`，断言 `plan.items.is_empty()`（当前保护契约）。
2. 插入中央 MCP + `mcp_global_assignments`，`enabled=true`，同一环境调用 Claude/Codex 全局 preview，断言各自 descriptor target 存在且 desired projection key 为 `mcpServers` / `mcp_servers`。
3. 同上但 `enabled=false`，断言无新 target（无旧 baseline 时）。
4. policy evidence 缺失/blocked 时复用已有 `global_status_distinguishes_initial_missing_unknown_and_blocked_policy`，分别断言 `PolicyBlocked` 与诊断码；该测试已由 452dc08 加入（`src-tauri/src/mcp/service.rs` diff）。

## 推断/存疑

- 用户界面提示语本身未在本责任范围后端代码中出现；后端能确定的是空 `PreviewPlan.items` 的来源。若 UI 以该字段显示提示，需前端核验。
- “已有全局 MCP”若只是 Claude/Codex 原生文件条目，而未通过应用导入/创建流程进入中央 DB，则按当前架构属于外部 unmanaged 条目，设计上不会自动接管；是否产品预期要支持导入需主代理结合需求文档判断。
