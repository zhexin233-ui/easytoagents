# 技术设计

状态：已实施并通过 GitHub 实际发布验证。

## 流程与边界
采用 `workflow_dispatch`，维护者从需要发布的分支手动运行并输入版本号（默认 `0.1.0`）。工作流校验版本并固定触发提交，然后构建 ARM64 DMG。构建和产物检查全部成功后，创建 `v<版本>` 标签与草稿 Release，上传附件后再公开 Release。

## 合同
- 版本采用应用版本 0.1.0、标签 v0.1.0 的映射，发布步骤不自动提交版本变更。
- 检查 package.json、Tauri 配置、Cargo 清单与锁文件中的应用版本一致。
- 同版本并发串行化；已有标签指向不同提交时拒绝发布，避免移动标签。
- 构建作业只读仓库；发布作业使用 contents: write，优先使用 GITHUB_TOKEN。
- pnpm 使用仓库声明版本，依赖按锁文件安装；显式选择受支持的 Node、Rust 与 macOS runner。
- 构建目标固定为 `aarch64-apple-darwin`，仅发布 M 系列 Mac 使用的 ARM64 DMG；最低系统版本为 macOS 13。实施时根据 GitHub 官方文档选择并固定可用的 ARM macOS runner 与 Action 版本。
- 首版不配置 Apple Developer 证书、签名或公证 Secrets。Release 与 README 明确标注未签名、未公证，并给出 Gatekeeper 安装提示。
- 上传前检查 DMG 存在、文件名包含版本与 `aarch64`，并验证其中应用二进制的架构为 arm64。

## 失败恢复
构建失败不创建公开 Release。上传中断或同版本作业失败时，仅在标签仍指向同一提交的前提下允许重跑；重跑删除并替换该 Release 的同名 DMG。若标签指向其他提交则直接失败。已公开版本出现功能问题时发布修复版本，不移动已发布标签。
