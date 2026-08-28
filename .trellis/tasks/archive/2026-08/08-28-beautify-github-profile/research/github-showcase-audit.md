# GitHub 项目展示审计

## 当前状态

- 远端仓库：`https://github.com/zhexin233-ui/easytoagents`，公开仓库，默认分支为 `main`。
- 根目录缺少 `README.md`；GitHub README API 返回 404。
- GitHub description、homepage、topics 均为空；无 Release、Tag、Pages、License 或可引用的 CI 状态。
- `index.html:6` 将项目描述为“Claude 与 Codex 的本地配置管理桌面端”。
- `src-tauri/tauri.conf.json:29-35` 说明项目是 macOS 13+ 的 `DeveloperTool`，产物目标为 `app` 与 `dmg`。

## 可验证能力

- 主导航覆盖总览、Claude、Codex、MCP、Skills、项目：`src/app/app-shell.tsx:5-12`。
- Dashboard 展示中央意图、原生目标状态、同步历史与恢复点：`src/features/dashboard/dashboard-page.tsx:24-29`。
- 首次接管流程是“检测 → 选择 → 预览 → 应用”，检测阶段只读：`src/features/onboarding/onboarding-wizard.tsx:287-300,333-335`。
- Profiles 的中央档案操作不会直接改写原生配置，写入需要预览并确认 Apply：`src/features/tool-profiles/tool-profiles-page.tsx:47-54`。
- Skills 导入会复制来源目录，目标使用指向中央副本的符号链接，写入前需要预览，应用后创建快照：`src/features/skills/skills-page.tsx:135-163`。
- MCP 保存和删除先影响中央库，原生配置变更需要预览确认：`src/features/mcp/mcp-page.tsx:100-105,141-152`。
- 项目登记与实际同步分离，移除登记不会破坏已有原生配置：`src/features/projects/projects-page.tsx:59-80`。

## 文档与元信息约束

- 开发命令来自 `package.json:6-19`，前端统一使用 pnpm。
- 技术栈来自 `package.json:21-54`：React 19、React Router 7、TanStack Query、Tauri、Vite、Tailwind CSS 4、TypeScript 与 Rust。
- `src/assets/brand/README.md:3-10` 要求 Claude/Codex 品牌素材本地加载且不得改色；README 横幅不应篡改这些素材。
- 旧 Trellis 研究截图可能与当前界面不一致，不作为本次 README 主视觉。
- 可更新的 Topics：`claude`、`openai-codex`、`mcp`、`ai-agents`、`developer-tools`、`desktop-app`、`tauri`、`react`、`rust`、`configuration-management`。
- 暂无独立官网或下载页，homepage 应保持为空。
