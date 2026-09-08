# 执行计划

- [x] 收敛手动入口、M 系列架构、未签名/未公证范围及同版本重跑策略。
- [x] 核实 GitHub macOS runner、Tauri 构建命令和所需工具链，记录来源及固定版本。
- [x] 配置 implement.jsonl / check.jsonl；规划评审通过后再启动实现。
- [x] 新增 .github/workflows/release.yml，配置显式触发、版本校验、构建、附件上传、并发和权限。
- [x] 抽取版本校验脚本，覆盖四处版本不一致及打包目标、最低系统版本约束；工作流负责 ARM64 产物、已有标签冲突与同版本重跑检查。
- [x] 更新 README 与发布操作说明，包含首版 0.1.0 操作、下载、Apple Silicon/macOS 13+ 限制、未签名安装提示和失败恢复。
- [x] 使用 actionlint 检查工作流；运行相关脚本测试、pnpm build、git diff --check。
- [x] 在 GitHub Actions 手动触发 `v0.1.0` 发布；ARM64 DMG 构建、挂载、版本、最低系统和架构检查通过，公开 Release 与附件状态验证通过。

## 实际发布结果

- 工作流运行：<https://github.com/zhexin233-ui/easytoagents/actions/runs/34193892224>
- Release：<https://github.com/zhexin233-ui/easytoagents/releases/tag/v0.1.0>
- 标签 `v0.1.0` 指向提交 `b80c8358474408326e6ccdc32b34dc307f8f4b01`。
- 附件 `EasyToAgents_0.1.0_aarch64.dmg` 已公开，大小 6,169,806 字节，SHA-256 为 `96342f0f3f6756b7f2cb237f77820559d6494d594602769e24d8921e6ef0b12a`。

## 风险与回退
发布写入集中在最后阶段。构建验证通过前不得创建公开版本；已存在的标签不得移动。首版不配置签名凭据。工作流变更可通过移除或禁用 release workflow 回退，已公开 Release 不由回退步骤自动删除。
