# 集中管理项目 MCP 与 Skill — 技术设计

## 边界

本次只调整 React 前端页面与对应测试。项目级 MCP/Skill 查询、assignment、preview、apply 的生成命令和 DTO 保持不变；Rust 服务、数据库和绑定文件均不修改。

## 页面结构

### 中央 MCP / Skill 页面

- 删除项目列表查询、选中项目/工具的本地状态、项目选项查询、项目 assignment mutation，以及“项目追加与全局继承”整段 UI。
- 将页面上的同步预览与 Apply 状态收敛为纯全局作用域，调用仍显式传递 `projectId: null`。
- 保留中央库 CRUD/导入、全局 Claude/Codex 分配、全局目标状态、全局预览与显式 Apply。

### 项目详情页

- 在现有项目管理区域顶部增加 MCP/Skill 切换控件。
- 本地状态使用窄联合类型（`"mcp" | "skill"`），默认值为 `"mcp"`。
- 切换控件由两个原生按钮组成，具备组标签、明确的 accessible name 与 `aria-pressed`。
- Claude/Codex 两列保持不变；每列只渲染当前资源类型对应的 assignment 组件。
- 现有 assignment 输入、双 row-version CAS、三组 query key 联合失效、阻断逻辑、零目标提示、预览及 Apply 流程原样复用。

## 数据流

1. 路由中的 `projectId` 加载 `ProjectDto`。
2. 当前视图选择对应 `mcpProjectOptionsQueryOptions` 或 `skillProjectOptionsQueryOptions`，分别为 Claude/Codex 加载选项。
3. checkbox 变更调用已有 `setProjectMcpAssignment` / `setProjectSkillAssignment`，并携带项目、工具、资源 ID 与双方行版本。
4. 成功后同时失效 project/MCP/Skill key family；界面重新获取最新项目行版本。
5. 用户生成项目预览后，通过共享 `ChangePreviewDialog` 显式 Apply，绝不在 assignment 成功时写入原生配置。

## 兼容性与取舍

- 不新增子路由，现有项目详情链接与浏览器历史保持兼容。
- 不引入通用 Tabs 组件：当前切换仅在一个页面使用，原生按钮组足以满足语义与测试需求。
- 非活动资源视图采用条件渲染，避免继续请求与展示当前不管理的选项；其临时 Git exclude 选择在离开视图后重置，这是页面本地、尚未提交的预览参数，不属于持久化配置。

## 回滚

改动均为前端结构调整。若切换交互出现回归，可恢复项目详情页同时渲染两类卡片，并恢复中央页面的项目区块；后端数据无需回滚。
