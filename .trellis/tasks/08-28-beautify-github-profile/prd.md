# 美化 GitHub 项目展示

## Goal

为 `zhexin233-ui/easytoagents` 建立清晰、可信且有辨识度的 GitHub 项目首页，让首次访问者能在首屏理解产品定位，并快速找到核心能力、同步安全模型与本地开发方式。

## Background

- 仓库当前没有根目录 `README.md`，GitHub README 接口返回 404。
- GitHub 仓库的 description、homepage 与 topics 当前均为空；仓库尚无 Release、Tag、License、Pages 或 CI 徽章依据。
- 产品是面向 macOS 13+ 的 Tauri 桌面开发者工具，定位为 Claude 与 Codex 的本地配置管理器（`index.html:6`、`src-tauri/tauri.conf.json:29-35`）。
- 产品真实差异点是：只读检测与选择性接管、中央意图与原生配置解耦、预览确认后写入、保留非受管内容、快照恢复。
- 项目内已有 Claude/Codex 品牌资源，但 `src/assets/brand/README.md:3-10` 要求本地加载且不可改色；旧 Trellis 研究截图可能已过时，不作为 README 主视觉。

## Requirements

### R1 — 根 README

- 新增根目录 `README.md`，首屏包含项目横幅、简洁定位与只反映真实状态的静态技术徽章。
- README 说明解决的问题、核心能力、安全同步模型、快速开始、常用命令、技术栈、当前支持范围与贡献入口。
- 核心能力至少覆盖 Providers/提示词、MCP、Skills、Projects，并避免使用不能从现有实现验证的营销承诺。
- 明确 macOS 13+、当前需从源码运行、尚无公开 Release 等边界；不得放置无依据的版本、下载量、License、CI 或 Release 徽章。
- 所有仓库内链接、图片路径和命令可验证；前端命令统一使用 pnpm。

### R2 — 项目主视觉

- 新增一张适合 GitHub README 首屏与后续 Social Preview 复用的项目横幅。
- 横幅应突出 `EasyToAgents`、`Claude · Codex · MCP · Skills` 与桌面开发者工具属性，整体风格与产品的克制、原生、可信定位一致。
- 不擅自变色、变形或错误组合受约束的第三方品牌素材；生成素材必须放在仓库内稳定路径并由 README 相对引用。

### R3 — GitHub 仓库元信息

- 更新 GitHub description，使其准确包含 Claude、Codex、MCP、Skills 与项目同步语义，并控制在 GitHub 简介适合的长度内。
- 添加与产品能力和技术栈一致的 Topics：`claude`、`openai-codex`、`mcp`、`ai-agents`、`developer-tools`、`desktop-app`、`tauri`、`react`、`rust`、`configuration-management`。
- 因暂无独立官网、文档站或下载页，homepage 保持为空。

### R4 — 变更隔离

- 不修改当前进行中的 `project-detail-tool-icon-tabs` 功能代码或测试。
- 不覆盖用户在 `AGENTS.md` 及其他文件中的既有未提交改动。
- 本任务的文档/素材变更应能与现有工作树中的其他改动清楚区分。

## Acceptance Criteria

- [x] AC1：GitHub 仓库根页能够渲染新的 `README.md`，首屏包含主视觉、项目定位与真实徽章。
- [x] AC2：README 对 Providers/提示词、MCP、Skills、Projects 与“预览后写入/快照恢复”的描述均可由现有实现验证。
- [x] AC3：README 内部图片与链接不存在 404，代码块中的开发命令与 `package.json` 一致。
- [x] AC4：仓库内存在可复用的横幅素材，尺寸与可读性适合 README，并未违反现有品牌资源约束。
- [x] AC5：GitHub description 与 Topics 已通过 GitHub API/CLI 回读确认；homepage 仍为空。
- [x] AC6：`git diff` 显示本任务仅新增或修改 README、展示素材、Trellis 任务记录，以及 GitHub 远端元信息，不包含对现有功能代码的改动。

## Out of Scope

- 建立官网、GitHub Pages、下载站或完整文档站。
- 发布 Release、构建安装包或新增 CI/CD 工作流。
- 补建 LICENSE、CONTRIBUTING、CODE_OF_CONDUCT 等社区治理文件。
- 为 README 使用可能过时的应用截图，或为了宣传而修改产品功能。
- 修改 GitHub 默认分支、权限、分支保护或 Discussions 设置。

## Key Decisions

- 本任务按轻量文档任务处理，仅维护 `prd.md`，不新增 `design.md` 或 `implement.md`。
- README 采用中文主体与英文一句副标题；GitHub description 使用英文，兼顾现有用户语境与 GitHub 国际检索。
- README 主视觉使用仓库自有横幅，不直接改造第三方品牌图标。
- Homepage 在没有独立入口时保持为空。

## Notes

- 本任务不触碰现有功能实现；远端 GitHub 元信息更新需在本地文档验证后执行并回读。
