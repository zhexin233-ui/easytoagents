# 参考项目调研

调研日期：2026-08-19

来源：

- <https://github.com/coulsontl/ai-toolbox>
- <https://github.com/xingkongliang/skills-manager>

说明：两个仓库均按当前公开源码进行只读分析。`smart-search doctor --format json` 显示 URL 抓取能力可用，但宽泛搜索模型配置不可用；因此本次采用已知 URL 的源码核验，不使用宽泛搜索生成结论。

## ai-toolbox

### 已验证能力

- `README.md:20-49`：覆盖 Claude/Codex 配置、全局 Prompt、MCP 集中管理、多工具同步、备份和托盘等功能。
- `package.json:20-64`：Tauri 2、React 19、TypeScript、Vite、Ant Design、Zustand、Monaco、i18next 和 TOML 解析。
- `tauri/src/coding/mcp/types.rs:8-68`：MCP 支持 stdio/http/sse，并保存原始服务配置和同步详情。
- `tauri/src/coding/mcp/mcp_store.rs:20-95`：SQLite JSONB 作为 MCP 中央存储。
- `tauri/src/coding/mcp/config_sync.rs:24-62,144-204`：按目标工具解析路径，并通过不同格式适配器写入 JSON、JSONC、TOML 等配置。
- `tauri/src/coding/mcp/commands.rs:78-141,163-290`：创建/更新后同步启用目标；重命名或移除目标时清理旧配置。
- Codex `commands.rs:44-46`：渠道切换保护 `mcp_servers`、`plugins` 等非渠道顶层字段。
- Codex `commands.rs:159-204,391-439,795-853`：解析 `CODEX_HOME`/数据库覆盖路径，分别处理 `config.toml` 与提示词文件。
- Claude Desktop `prompt.rs:112-126,176-212`：数据库无记录时读取现有 `AGENTS.md`，更新已应用提示词时回写文件。
- `config_writer.rs:226-267`：配置应用具有 rollback 包装。

### 可借鉴设计

- 中央数据库作为真源，目标文件只作为同步结果。
- 配置格式与路径解析独立成工具适配器。
- 数据库存 portable 原始路径，落盘时才展开本机路径。
- 同步详情逐目标记录，便于界面显示漂移和失败。
- 渠道切换只修改自己拥有的字段，不整文件覆盖。

### 风险

- 通用提示词写入路径存在直接 `fs::write`，原子性不足。
- MCP 使用宽泛 JSON Value，缺少强结构校验。
- 多工具、多格式和跨平台路径分支容易产生配置漂移。

## skills-manager

### 已验证能力

- `README.md`：中央库默认 `~/.skills-manager`；支持全局与项目 workspace、按 agent 分配、symlink/copy、Preset、Git 更新与安全快照。
- `src-tauri/src/core/central_repo.rs:75-117,139-189`：中央库路径配置独立保存，要求绝对路径，并区分缺失与损坏配置。
- `src-tauri/src/core/migrations.rs:70-176`：SQLite 保存 Skill 来源、版本、内容 hash、目标工具、目标路径、同步模式、状态、项目、场景与标签。
- `src-tauri/src/core/tool_adapters.rs`：Claude 全局目录为 `~/.claude/skills`；Codex 全局目录为 `~/.codex/skills`，项目目录为 `<repo>/.codex/skills`；目标路径由工具适配器定义。
- `src-tauri/src/core/sync_engine.rs:264-274,386-412`：支持符号链接与复制，通过 source hash 判断复制目标是否过期。
- `src-tauri/src/core/merge/decision.rs:1-180`：采用纯函数三方合并，冲突默认保留本地并进入待处理状态。
- `package.json:20-59`、`src-tauri/Cargo.toml:22-30,43`：Tauri 2、React 19、TypeScript、Vite、Tailwind、SQLite 与 git2。

### 可借鉴设计

- 中央 Skill library 与各工具 target 映射解耦。
- 全局 Preset 与项目级逐工具 assignment 明确区分。
- Copy 模式使用 source hash 检测漂移；Symlink 模式验证链接目标。
- 配置损坏不静默回退为“首次安装”。
- 合并冲突保留本地版本，先做 safety snapshot 再解决。

### 风险

- 安装器覆盖中央目标前会递归删除，必须增加来源确认和恢复点。
- 多工具默认路径、fallback discovery 路径可能让用户难以理解实际生效位置。
- SQLite、桌面端与潜在 CLI 共用数据时需要锁和事务边界。

## 对本项目的结论

> 校正：上述 Codex Skills 路径反映该参考项目的实现，不是本项目的规范来源。2026-08-19 核验的 Codex 官方文档使用 `$HOME/.agents/skills` 与项目 `.agents/skills`。本项目必须以 `research/official-config-paths.md` 为准。

1. 采用“本地中央真源 + Tool Adapter + Preview/Apply”架构。
2. MCP、Skills、渠道和提示词共享同步任务、快照、漂移检测与错误模型，不共享具体文件格式实现。
3. 项目是一级实体，保存 Claude/Codex 各自的 MCP 与 Skills assignment。
4. 所有外部配置写入应使用原子替换、快照和拥有字段级合并，避免整文件覆盖。
5. MVP 不应照搬参考项目的会话、远程同步、市场等外围能力。
