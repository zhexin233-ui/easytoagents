# Design: 启用的工具全局设置

## 1. 总体方案

新增一个应用级设置 `enabledTools: Tool[]`，沿 `apply_mode` 的既有链路贯穿：
Rust 单例设置存储 → specta 生成 TS 绑定 → TanStack Query（`["settings"]` 键）
→ 各渲染点用共享 hook 过滤。显示层过滤，不触碰同步/数据层。

## 2. 后端（src-tauri）

### 2.1 存储（settings.rs）

- 新增常量 `ENABLED_TOOLS_KEY: &str = "enabled_tools"`。
- 值为 JSON 数组字符串，如 `["claude","codex"]`，用 `serde_json` 序列化 /
  反序列化 `Vec<Tool>`（`Tool` 已 `#[serde(rename = "claude")]` 等小写形式）。
- `AppSettingsDto` 增加 `enabled_tools: Vec<Tool>`（camelCase 序列化为
  `enabledTools`）；`UpdateAppSettingsInput` 同步增加该字段。
- 默认值：`vec![Tool::Claude, Tool::Codex]`（缺 key 时返回，不写库）。
- 读取错误约定与 `apply_mode` 一致：JSON 解析失败或未知工具取值 →
  `ErrorCode::DatabaseError`（“应用设置包含未知取值”），绝不静默回退。
- 保存：单 key UPSERT，沿用现有 `save_app_settings` 事务模式（现函数只写一个
  key，改为同事务写两个 key）。
- 说明：`UpdateAppSettingsInput` 整体提交（read-modify-write），与现有单命令
  模式一致；设置属于单例偏好，无乐观并发控制（文件头注释已声明）。

### 2.2 绑定再生成

- `pnpm bindings:generate`（cargo example export-bindings）刷新
  `src/bindings/commands.ts`；`pnpm bindings:check`（cargo test
  generated_bindings_are_current）验证。
- 预期类型变化：
  - `AppSettingsDto = { applyMode: ApplyMode; enabledTools: Tool[] }`
  - `UpdateAppSettingsInput = { applyMode: ApplyMode; enabledTools: Tool[] }`

## 3. 前端

### 3.1 共享过滤设施

- `src/lib/tool-metadata.ts`：新增
  `export const DEFAULT_ENABLED_TOOLS = ["claude", "codex"] as const satisfies readonly Tool[];`
  与纯函数
  `filterEnabledTools<T extends Tool>(tools: readonly T[], enabled: ReadonlySet<Tool>): T[]`。
- 新 hook `src/components/use-enabled-tools.ts`（与 use-theme / use-notify 同层）：

  ```ts
  export function useEnabledTools(): ReadonlySet<Tool> {
    const { data } = useQuery(appSettingsQueryOptions());
    return new Set(data?.enabledTools ?? DEFAULT_ENABLED_TOOLS);
  }
  ```

  - 加载中 / 查询失败时回落到默认集合（确定性渲染，避免闪现 Cursor）。
  - 所有消费方共享 `["settings"]` 缓存键，设置对话框保存后的
    `invalidateQueries` 会即时驱动所有渲染点更新，无需事件总线。

### 3.2 设置对话框（settings-dialog.tsx）

- 新区块「启用的工具」，放在「应用方式」区块之后；沿用现有 checkbox + 说明
  文案模式，每项渲染 `toolMetadata(tool).icon` + label。
- 三个 checkbox 分别绑定 claude / codex / cursor；change 时调用现有
  `updateMutation.mutate({ applyMode: settingsQuery.data.applyMode,
  enabledTools: next })`（整包提交）。
- 说明文案注明：关闭的工具不再显示在顶部工具入口、中央列表与项目详情；
  已有配置数据不会被删除或停止同步。
- 错误 / 加载态复用现有 `settingsQuery.isPending / isError` 展示。

### 3.3 渲染点过滤（全部为缩小列表的安全改动）

| 文件 | 位置 | 改法 |
|---|---|---|
| app-shell.tsx | `toolLinks`（模块级常量→组件内计算） | TopBar 内 `useEnabledTools()`，过滤 PROFILE_TOOLS 后再 map |
| prompts-page.tsx | `const tools = PROFILE_TOOLS` | 过滤；图标列（421）与「全局目标状态」（484）随之收缩；`tools.length === 0` 时隐藏状态区块 |
| mcp-page.tsx | platformActions(415)、状态卡(530) | 图标列过滤 MCP_TOOLS；状态卡对 `statusesQuery.data` 按 `enabled.has(s.tool)` 过滤；空时隐藏分组 |
| skills-page.tsx | platformActions(355)、状态卡(474) | 同上（SKILL_TOOLS / globalSkillStatuses） |
| project-detail-page.tsx | 图标列(391)、工具配置状态(289) | `visibleTools = filterEnabledTools(MCP_TOOLS, enabled)`；派生 `activeTool = visibleTools.includes(toolView) ? toolView : visibleTools[0] ?? toolView`，后续展示逻辑改用 activeTool（含 373/446/570/1094 等处）；`project.targets` 过滤 |
| dashboard-page.tsx | 工具卡(82) | `data.tools.filter(...)` |
| onboarding-wizard.tsx | `const tools = PROFILE_TOOLS` | 过滤；`Choices` 等硬编码双键结构容忍缺失键（已验证 `tools.every` 空集为 true，可正常完成引导） |
| provider-panel.tsx | 复制到另一工具(161/167/326) | 目标工具 `tool === "claude" ? "codex" : "claude"` 不在启用集合时隐藏该入口 |

### 3.4 关键边界处理

- **toolView 夹逼（project-detail）**：遵循 state-management 规范「派生状态在
  render 时计算，不进 useEffect」——`activeTool` 为纯派生值；被关闭工具上的
  选中态自动落到第一个启用工具，重新启用后恢复（stale toolView 无害）。
- **statusQueries 硬编码键（prompts-page 58-61）**：过滤只缩小列表，
  `statusQueries[tool]` 不会越界；保持现状。
- **`?? "claude"` 回落常量（prompts:615 等）**：仅在无预览时兜底，不受影响。
- **路由可达性**：/claude、/codex 不加守卫（PRD R3），tool-profiles 页不改。

## 4. 数据流

```
设置对话框 toggle
  → commands.updateAppSettings({ applyMode, enabledTools })
  → app_settings 表 UPSERT（apply_mode + enabled_tools 两个 key，同事务）
  → invalidateQueries(["settings"])
  → useQuery(同键) 的所有消费组件（TopBar / 各页 / 项目详情 / 总览）重渲染
```

## 5. 兼容与回滚

- 旧库无 `enabled_tools` 键 → 读侧默认 `["claude","codex"]`，无迁移脚本
  （app_settings 是 KV 表，无需 schema 变更）。
- 存量 Cursor 用户升级后 Cursor 列默认隐藏，需在设置中手动开启（产品预期）。
- 回滚 = revert 提交；无数据迁移需要逆写。

## 6. 测试策略

- Rust（settings.rs 测试模块）：缺省默认值、双设置 round-trip、重开保持、
  非法 JSON / 未知工具值报 DatabaseError。
- 前端：
  - settings-dialog.test：区块渲染默认态、切换后 updateAppSettings 收到整包
    （applyMode 保持 + enabledTools 更新）。
  - app-shell.test：关闭 codex 后顶部入口只剩 Claude；默认两枚。
  - mcp / skills / dashboard / project-detail：现有 cursor 断言补
    `enabledTools: ["claude","codex","cursor"]` 的 settings mock；新增关闭
    断言（至少各一处：图标列 + 状态卡/工具配置状态）。
  - project-detail：选中工具被关闭时 activeTool 回落断言。
- 所有现存 `getAppSettings` mock 需补 `enabledTools` 字段（TS 会逐个指出）。
