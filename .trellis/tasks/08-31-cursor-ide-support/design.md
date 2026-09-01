# Cursor 产品扩展：技术设计

## 1. 设计目标

Cursor 是第三种 Tool，但不是 Claude/Codex 的全功能复制。所有层以同一 capability matrix 为准，只让 MCP 与 Skills 进入同步管线；Provider 和 Prompt 在 API 边界显式拒绝。

核心不变量：

1. 未分配 Cursor 的中央资源不产生 Cursor 原生目标变更。
2. Cursor 只写官方公开的 MCP/Skills 路径。
3. Provider/Prompt 没有路径 fallback，也不能生成可 Apply 预览。
4. Cursor 与 Claude/Codex 共享同步安全内核，但拥有独立的原生路径、容器和敏感 selector。
5. 数据库只为受支持 artifact 放宽 Cursor 值，unsupported 能力在存储层也保持拒绝。

## 2. Capability Matrix

| Artifact | Global | Project | Import | Apply | 诊断 |
| --- | --- | --- | --- | --- | --- |
| Provider | Unsupported | N/A | Unsupported | Unsupported | CURSOR_PROVIDER_UNSUPPORTED |
| Prompt / Rules | Unsupported | Unsupported | Unsupported | Unsupported | CURSOR_PROMPT_UNSUPPORTED |
| MCP | Supported | Supported | Global | Supported | 标准 target capability |
| Skills | Supported* | Supported* | Global | Supported* | 未通过兼容验证时 CURSOR_SKILL_SYMLINK_UNVERIFIED |

Skills 必须通过本机 Cursor 对受管目录符号链接的兼容验证；失败时 capability 保持 Unsupported，不引入 Copy 模式。

## 3. 领域模型与工具注册

- Tool 新增 Cursor => cursor，Rust 继续作为 TypeScript Tool 联合类型的唯一源。
- 后端建立共享工具集合，避免各服务重复 Claude/Codex 数组。
- 前端 tool-metadata.ts 维护 label、icon、profile route 与 MCP/Skills/Prompt/Provider 能力。
- PROFILE_TOOLS 只含 Claude/Codex；ASSIGNABLE_MCP_TOOLS 与 ASSIGNABLE_SKILL_TOOLS 含 Cursor。
- Cursor 不新增 Provider/Prompt CRUD 导航。Dashboard 展示 Cursor 工具状态和 MCP/Skills 计数，并将不支持能力标为 Unsupported。

## 4. 安装与环境发现

### 4.1 Desktop 优先

扩展 ReleaseToolProbeInput/Result、ToolAvailability 与 ExplicitEnvironment：

- Cursor 不需要独立 cursor_home；配置根固定为显式 home/.cursor。
- 探测生产 Bundle ID com.todesktop.230313mzl4w4u92。
- 在显式候选应用路径中读取 Contents/Info.plist，校验 Bundle ID，再读取版本；默认候选至少含 /Applications/Cursor.app 与 $HOME/Applications/Cursor.app。
- plist 解析必须有大小限制、类型校验与测试 fixture；路径输入保持显式，测试不读取真实 HOME。

### 4.2 CLI 补充

- PATH 中存在官方 agent 时可执行 agent --version 作为补充证据。
- Desktop 或 CLI 任一可信探测成功即可标记 Installed；Desktop 已安装时 CLI 缺失不降级。
- 两种探测都失败为 Unavailable；存在路径但 bundle/version 不可信为 Unsupported。

## 5. Cursor Adapter

新增 src-tauri/src/adapters/cursor/mod.rs，并注册到所有 Adapter registry。

### 5.1 Descriptors

| Artifact | Scope | Path | Format | Ownership root | Sensitive roots |
| --- | --- | --- | --- | --- | --- |
| Provider | Global | None | JSON 占位 | 无 | 无；capability Unsupported |
| Prompt | Global | None | Markdown 占位 | 无 | 无；capability Unsupported |
| MCP | Global | $HOME/.cursor/mcp.json | JSON | mcpServers | headers/env/auth |
| Skill | Global | $HOME/.cursor/skills | SymlinkDirectory | $children | 无 |
| Prompt | Project | None | Markdown 占位 | 无 | 无；capability Unsupported |
| MCP | Project | <root>/.cursor/mcp.json | JSON | mcpServers | headers/env/auth |
| Skill | Project | <root>/.cursor/skills | SymlinkDirectory | $children | 无 |

Unsupported descriptor 必须在任何 path unwrap、scan 或 preview 持久化之前被 capability gate 拦截。

### 5.2 MCP 格式

- 容器为 mcpServers。
- stdio 写 type: stdio、command、可选 args/env。
- streamable HTTP 写 url、可选 headers；保留验证过的扩展字段。
- 导入兼容官方示例中省略 type 的 stdio/remote 条目，并拒绝无法唯一判断 transport 的条目。
- 导入的 auth 等扩展字段继续走 extra 保存和 secret 检测，不新增会把明文回填到 DTO 的路径。

### 5.3 Skills

- 同步目标固定使用 .cursor/skills，不把 .agents/skills 作为受管目标。
- 导入可扫描 .cursor/skills 与 .agents/skills，用 source kind 区分并复用显式选择、去重、复制到中央库和不隐式接管合同。
- allowed root：用户级为 $HOME/.cursor，项目级为登记项目根。

## 6. 数据库迁移

新增 0010_cursor_tool_support.sql，注册为 schema version 10。

只放宽实际存储 Cursor 的表：

- mcp_global_assignments
- skill_global_assignments
- mcp_project_assignments
- skill_project_assignments
- managed_targets
- mcp_import_previews
- skill_import_previews

不放宽 provider_profiles、profile_import_previews、prompt_profiles、prompt_project_assignments。

SQLite CHECK 无法直接 ALTER。沿用已验证的 writable_schema 方案，但每次替换必须限定表名和精确旧锚点；迁移测试验证命中后的 schema、同连接插入、重开、索引、外键与旧行。数据库打开前备份和 IMMEDIATE transaction 保持不变。

## 7. 服务层与同步

- MCP/Skills service、import、project service、overview 和 restore registry 加入 Cursor。
- Profiles service 对 Cursor Provider/Prompt 在入口返回稳定 Unsupported；数据库写方法不得收到 Cursor。
- Project service 为 Cursor 返回 MCP/Skill target status，不创建 prompt assignment。
- Overview 工具列表加入 Cursor；前端根据 metadata 把 Provider/Prompt 显示为不支持而非“未接管”。
- Restore allowed root 根据 Cursor descriptor 解析为 $HOME/.cursor 或项目根，不能 fallback 到 HOME。
- Sync adapter registry 加入 Cursor；现有 scan/diff/baseline/stale/journal/snapshot/rollback 逻辑不复制。

## 8. 前端

新增 src/lib/tool-metadata.ts，集中维护 ToolMetadata：id、label、icon、profileRoute 与 provider/promptGlobal/promptProject/mcp/skills 能力。

界面变化：

- MCP/Skills 中央卡片的全局分配按钮加入 Cursor。
- MCP/Skills 全局目标状态与导入对话框加入 Cursor。
- Project Detail 工具切换加入 Cursor；Cursor 视图只显示 MCP/Skills，Prompt 区不渲染。
- Dashboard 加入 Cursor 卡片，Provider/Prompt 显示 Unsupported。
- AppShell 的 Provider/Prompt 专用工具导航仍只显示 Claude/Codex；顶栏描述改为不枚举固定两工具。
- Onboarding 继续只扫描 Claude/Codex Provider/Prompt，文案明确其范围，不添加无动作 Cursor 卡片。
- 新增来源可追溯的本地 Cursor 品牌图标；所有 label/icon 通过 metadata 获取。

## 9. 兼容与安全

- 现有 Claude/Codex 路径、MCP 映射、Skill 目标和 prompt active flags 不改语义。
- Cursor JSON 采用字段级 ownership，只修改已分配名称。
- 项目配置仍遵循 tracked 文件警告与可选 .git/info/exclude 流程。
- 任意能力不确定时返回 Unsupported，不用空配置覆盖。
- Cursor Skills symlink 验证失败的回滚方式是关闭该 capability；不迁移到 Copy 模式。

## 10. 测试策略

- Adapter：全局/项目 descriptor、unsupported 能力、安装状态、敏感 selector。
- Probe：Desktop bundle、CLI fallback、CLI 缺失、错误 Bundle ID、异常版本、路径异常和 timeout。
- Migration：v9→v10、旧数据保留、七张表接受 Cursor、四张 unsupported 表拒绝 Cursor、同连接和重开。
- MCP：Cursor render/import、未知字段保留、凭据脱敏、preview/apply/stale/conflict/restore。
- Skills：Cursor import source、全局/项目 assignment、symlink 冲突/恢复，以及本机 Cursor 兼容 smoke。
- Frontend：Cursor assignment button、导入、target status、project view、dashboard unsupported 文案、未分配不写入意图。
- Contract：生成 bindings 一致性与完整 pnpm check。

## 11. 回滚

- 代码回滚可移除 Cursor UI/registry，但数据库 v10 不倒迁；放宽 CHECK 对旧数据无破坏。
- 发布前若 Cursor Skills 兼容验证失败，只关闭 Skills capability，MCP 支持可独立保留。
- 任一 Cursor Apply 失败使用现有 snapshot/journal 恢复，不新增旁路写入。
