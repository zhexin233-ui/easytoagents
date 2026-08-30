# GitHub 仓库展示美化设计

## 变更边界

本任务只调整仓库展示层：`README.md`、`docs/assets/` 下的公开展示素材，以及 GitHub 仓库 Description、Topics 与 Social Preview。产品源码、运行逻辑、发布流程和许可证状态不变。

## README 信息架构

1. 首屏：现有 Hero、简洁的中英文定位、少量事实型徽章、核心入口导航。
2. 产品价值：说明中央意图、原生目标与 local-first 的关系。
3. 核心能力：Providers/提示词、MCP、Skills、Projects 四块能力。
4. 产品实景：一张真实运行截图，并配简短说明。
5. 安全同步：只读检测、选择性接管、预览、确认应用、快照恢复。
6. 快速开始：环境要求、源码运行、仅前端调试、本地构建。
7. 首次使用：从发现配置到预览、应用和恢复的最短路径。
8. 开发与边界：常用命令、技术栈、当前仅 macOS 13+ 且暂无公开 Release/官网/许可证。
9. 贡献入口：Issues、Pull Requests 与提交前检查。

## 视觉与素材

- 保留 `docs/assets/github-hero.png`，不改变深色蓝金风格。
- 新增 `docs/assets/app-overview.png`，优先从真实 Tauri 窗口或可验证的真实前端运行界面截取。
- 截图在提交前检查可读性、尺寸、文件体积和隐私；不得包含本机用户名、绝对路径、密钥或私有项目名。
- README 使用 GitHub 原生 Markdown 与少量稳定 HTML 布局，不依赖易失效的动态卡片服务。

## GitHub 元数据

- Description：采用与 README 一致的简洁英文，覆盖 local-first、macOS、preview/sync/restore 以及 Claude、Codex、MCP、prompts、skills。
- Topics：保留现有事实型 Topics，并补充 `macos`、`local-first`；不添加无法从仓库证明的生态或发布标签。
- Homepage：保持为空，因为仓库没有独立官网、下载页或公开 Release。
- Social Preview：尝试将现有 1280×640 Hero 设置为预览图；若 CLI/API 不支持，则通过已登录 GitHub 界面完成，仍失败时记录人工步骤。

## 兼容与回滚

- README 中保留 macOS 13+、pnpm 10.13.1、Rust 1.77.2+ 和无公开 Release 等真实边界。
- 文件变更可通过 Git 提交回滚；元数据变更前记录原 Description、Topics 与 Homepage，必要时使用 `gh repo edit` 恢复。
- Social Preview 若被替换，优先在操作前确认当前状态；无法读取旧图时不覆盖未知的自定义预览图。
