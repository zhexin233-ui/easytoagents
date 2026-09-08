# GitHub 手动发布与 DMG 自动打包

## 目标
维护者在 GitHub Actions 中手动执行新版本发布，由 GitHub 自动构建公开可下载的 macOS DMG，并作为 GitHub Release 附件发布。首个版本为 0.1.0，对应标签 v0.1.0。

## 已确认事实
- package.json、src-tauri/tauri.conf.json、src-tauri/Cargo.toml 与 src-tauri/Cargo.lock 的应用版本均为 0.1.0。
- src-tauri/tauri.conf.json:31 已配置 app、dmg 打包目标；最低系统版本为 macOS 13.0。
- package.json 已声明 pnpm@10.13.1；pnpm tauri build 会先运行 pnpm build。
- 当前未发现 .github 工作流；README.md:96 已记录本地 app 与 dmg 构建方式。

## 需求
- R1：维护者在 GitHub Actions 手动运行 workflow，由 GitHub 构建公共 DMG 并发布到公开 Release；普通 push 与 PR 不发布版本。
- R2：仅为 Apple Silicon（M 系列）Mac 自动构建 ARM64 DMG，上传到对应版本的 GitHub Release；最低系统版本为 macOS 13。
- R3：首版应用版本为 0.1.0；后续发布须检查标签版本与应用各处版本一致，避免错误版本安装包。
- R4：构建或上传失败应在 Actions 中可见，不得报告发布成功；重跑不得覆盖其他版本的资产。
- R5：同一版本仅允许在标签仍指向原提交时重跑；重跑替换该版本的同名 DMG，不移动标签、不影响其他版本附件。
- R6：首版 DMG 不使用 Apple Developer 签名或公证。提供维护者发布步骤与用户下载说明，明确 Apple Silicon、macOS 13+、未签名/未公证状态及 Gatekeeper 安装处理方式。

## 验收条件
- AC1（R1）：普通提交和 PR 不触发发布；指定的人为操作启动一次发布流水线。
- AC2（R2、R3）：v0.1.0 Release 可下载版本为 0.1.0、架构为 ARM64 的 DMG。
- AC3（R3）：版本不一致在上传前失败并说明原因。
- AC4（R4、R5）：失败有明确日志；同标签同提交重跑会替换该 Release 的同名 DMG；标签指向不同提交时失败；其他版本附件不受影响。
- AC5（R6）：文档与实际入口、附件架构、最低系统版本、未签名/未公证状态及安装提示一致。

## 范围外
Intel Mac 与 Universal 安装包、Windows/Linux 安装包、Apple Developer 签名与公证、应用内自动更新、定时发布、提交后自动发布，以及仓库许可证选择或新增。
