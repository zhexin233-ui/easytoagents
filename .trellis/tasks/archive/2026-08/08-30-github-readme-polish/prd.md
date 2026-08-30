# 美化 GitHub 仓库展示

## Goal

提升 easytoagents 项目在 GitHub 上的第一印象与信息传达效率，让访客能快速理解项目定位、核心能力、安装方式和使用入口。

## Background

- 当前 README 已准确覆盖产品定位、四类核心能力、安全同步模型、源码运行方式、开发命令、技术栈和 macOS 13+ 范围边界。
- 当前线上 Description 已准确概括 local-first、macOS、Claude、Codex、MCP、Skills 和跨项目同步；现有 10 个 Topics 均与仓库事实一致。
- 仓库目前没有独立官网、公开 Release、预编译安装包、CI 工作流或 LICENSE 文件，Homepage 为空。
- 现有 `docs/assets/github-hero.png` 是 1280×640 的深色蓝金横幅，可继续作为首屏和 GitHub Social Preview 素材；仓库暂时没有实际产品界面截图或演示动图。
- 从源码可运行 `pnpm tauri dev`，也可通过 `pnpm tauri build` 在本地构建 `.app` / `.dmg`；最低 Rust 版本为 1.77.2。

## Requirements

- 重构并美化仓库 README 的内容层级与视觉呈现。
- 完善适合 GitHub 仓库首页的简介类信息，包括仓库 Description、Topics、Homepage 等可配置项的建议或更新。
- 所有对外文案、徽章、链接、命令和能力描述必须基于仓库事实，不夸大、不引入失效入口。
- 保留现有项目的实际使用方式与兼容性约束。
- 强化首屏定位、导航、核心能力、同步安全模型、首次使用路径、源码运行/构建、当前限制和贡献入口。
- 不添加没有事实依据的 CI、License、Release、下载量或版本徽章。
- 沿用现有深色蓝金、专业开发者工具的视觉方向，保留 Hero，并新增一张真实运行界面截图。
- 新增截图必须避免暴露本机用户名、绝对路径、密钥、私有项目名称或其他个人数据。
- 将 GitHub Description 调整为更简洁且与 README 一致的英文表述；保留 Homepage 为空，并仅补充有事实依据的 Topics。

## Acceptance Criteria

- [ ] GitHub 访客在 README 首屏可快速识别项目定位、价值和主要行动入口。
- [ ] README 清晰覆盖核心能力、安装/使用、支持范围与进一步了解项目的路径。
- [ ] README 中的命令、相对链接、外部链接和徽章均经过验证。
- [ ] 仓库简介类元数据形成与 README 一致的对外表述。
- [ ] macOS 13+、pnpm 10.13.1、Rust 1.77.2+、无公开 Release/官网/许可证等边界信息保持准确。
- [ ] 现有 Hero 或替代视觉在 GitHub 桌面与窄屏阅读下不会破坏信息层级。
- [ ] README 展示至少一张来自真实运行界面的截图，截图中不包含个人或敏感信息。
- [ ] GitHub Social Preview 使用现有 1280×640 Hero；若 GitHub 不允许自动上传，则给出明确的人工操作说明而不阻塞 README 完成。

## Out of Scope

- 未经进一步确认，不改动项目功能、运行逻辑或发布流程。
- 未经进一步确认，不开展完整品牌重塑或单独制作大型官网。
- 不虚构公开安装包、独立官网、许可证或自动化状态。
- 不修改现有 Hero 的品牌方向，不重做 Logo、应用图标或完整视觉识别系统。
