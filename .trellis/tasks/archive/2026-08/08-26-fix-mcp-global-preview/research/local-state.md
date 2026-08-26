# 本机只读核验

核验日期：2026-08-26。没有改写原生配置或应用数据库；仅输出数量和结构是否有效，不输出服务配置、名称或凭证。

| 数据源 | 结果 |
| --- | --- |
| `~/Library/Application Support/com.easytoagents.desktop/easytoagents.sqlite3` | 存在；SQLite `mode=ro` 与 `query_only=ON` 查询 |
| 中央 `mcp_servers` | 0 项（启用项亦为 0） |
| `mcp_global_assignments` | 0 项 |
| MCP `managed_targets` | 0 项 |
| `~/.claude.json` 的 `mcpServers` | 有效对象，3 项 |
| `~/.codex/config.toml` 的 `mcp_servers` | 有效 TOML 表，6 项 |

运行时通过 Tauri `app.path().app_data_dir()` 创建私有路径（`src-tauri/src/lib.rs:198`），应用 identifier 为 `com.easytoagents.desktop`（`src-tauri/tauri.conf.json:5`）。用于纯路径测试的 `AppPaths::for_macos_home` 返回 `EasyToAgents` 目录，不是本次实际运行数据库的位置。

系统 `python3` 无 `tomllib`；Codex 配置最终使用 Codex bundled Python 的标准库 `tomllib` 解析后统计，未用文本正则猜测数量。

结论：本次已证明的触发条件是“原生有配置，但应用无中央候选及旧受管基线”，不是全局分配已入库后被预览遗漏。

子代理报告校正：后端报告建议测试中的 `PreviewPlan.items` 应为 `PreviewPlan.targets`。主代理已核对前端实际判断和后端空候选分支；后续测试以生成 DTO 的真实字段为准。
