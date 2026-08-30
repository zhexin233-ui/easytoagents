# 实施计划

1. 读取即将修改的完整 README、相关素材与项目清单，复核文案事实和当前 GitHub 元数据。
2. 启动真实应用界面，选择不暴露本机数据的页面状态，截取并裁剪为 `docs/assets/app-overview.png`；检查分辨率、体积和隐私。
3. 按设计中的信息架构重写 `README.md`，保留 Hero，加入应用实景、导航、首次使用、本地构建和准确的范围说明。
4. 校验 README 中所有相对路径、外部链接、徽章、命令和版本约束；执行 Markdown/格式检查以及与文档改动相关的项目检查。
5. 通过 `gh repo edit` 更新 Description 和 Topics，Homepage 保持为空；回读 GitHub API 验证结果。
6. 在确认不会覆盖未知自定义图的前提下设置 GitHub Social Preview；若自动化渠道不可用，记录精确人工步骤。
7. 执行 Trellis 全量质量检查，复核 README 渲染、窄屏可读性、截图隐私和线上元数据一致性。

## 验证命令与检查

- `pnpm check`
- `git diff --check`
- `rg` 检查 README 中的相对资源路径、GitHub URL、macOS/pnpm/Rust 版本与范围声明
- `gh repo view zhexin233-ui/easytoagents --json description,homepageUrl,repositoryTopics`
- 以 GitHub 渲染效果检查首屏、目录锚点、表格、图片和窄屏阅读

## 风险与回滚点

- 截图包含本地信息：提交前进行像素级检查，发现敏感内容则重新生成，不通过模糊遮盖勉强发布。
- 技术栈徽章随依赖升级漂移：只保留稳定且当前可证实的徽章，版本事实与清单一致。
- 外部元数据操作失败：README 与素材仍可独立完成；保留原 Description/Topics 以便恢复。
- Social Preview 无可用 API：不绕过权限或覆盖未知图片，改为输出人工设置步骤。
