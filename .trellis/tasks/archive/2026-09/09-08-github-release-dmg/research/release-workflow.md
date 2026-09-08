# GitHub Release 工作流现状

## 仓库证据

- `package.json:4`、`src-tauri/tauri.conf.json:4`、`src-tauri/Cargo.toml:3` 与 `src-tauri/Cargo.lock:888` 的应用版本均为 `0.1.0`。
- `src-tauri/tauri.conf.json:31` 已启用 `app` 与 `dmg` 打包目标；`src-tauri/tauri.conf.json:41` 将最低系统版本设为 macOS 13.0。
- `package.json:33` 声明 `pnpm@10.13.1`；Tauri 的构建前命令为 `pnpm build`。
- 仓库当前没有 `.github` 工作流。
- `README.md:90-96` 只描述本地构建；`README.md:133-138` 仍声明没有公开 Release、预编译安装包和 LICENSE。

## 已确定的产品决策

- 维护者从 GitHub Actions 手动运行发布，普通 push 与 PR 不触发发布。
- 首版为 `0.1.0` / `v0.1.0`，公开发布一个 Apple Silicon ARM64 DMG，支持 macOS 13+。
- 首版不签名、不公证；下载文档必须告知用户 Gatekeeper 影响与安装方式。
- 同标签仅可对同一提交重跑，替换同名附件；不得移动标签或影响其他版本。
- Intel、Universal、其他操作系统、自动更新与许可证选择均不在本任务范围内。

## 实施期技术核验

- GitHub 官方托管 runner 文档将 `macos-15` 列为 M1/arm64 标准 runner；工作流据此固定 `macos-15`，并将所有 `uses` 固定到对应版本的完整提交 SHA。
- Tauri CLI 2 使用仓库已有命令 `pnpm tauri build --target aarch64-apple-darwin --bundles app,dmg --ci --no-sign`；构建后挂载 DMG，并用 `lipo -archs` 验证主二进制仅包含 `arm64`。
- 使用最小权限：构建阶段只读，发布阶段仅授予 `contents: write`。

参考：

- GitHub Docs，GitHub-hosted runners：<https://docs.github.com/en/actions/reference/runners/github-hosted-runners>
- Tauri CLI build 命令：<https://v2.tauri.app/reference/cli/#build>
- `actions/checkout` v7.0.1：`3d3c42e5aac5ba805825da76410c181273ba90b1`
- `actions/setup-node` v7.0.0：`820762786026740c76f36085b0efc47a31fe5020`
- `pnpm/action-setup` v6.1.0：`ea17c68df8912ef543352723c149a84f56e3d413`
- `dtolnay/rust-toolchain`：`d1031067263f94b142dd6c0ce24c5eb9d02d52a0`
- `actions/upload-artifact` v7.0.1：`043fb46d1a93c77aae656e7c1c64a875d1fc6a0a`
- `actions/download-artifact` v8.0.1：`3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c`

## 实际发布验证

- GitHub Actions run `34193892224` 在 `main` 的提交 `b80c8358474408326e6ccdc32b34dc307f8f4b01` 上成功完成。
- 构建作业实际运行于 ARM64 `macos-15`，并通过 DMG 挂载、应用版本 `0.1.0`、最低系统 `13.0` 和纯 `arm64` 二进制检查。
- 公开 Release `v0.1.0` 已创建，标签指向上述提交，附件 `EasyToAgents_0.1.0_aarch64.dmg` 的 GitHub 摘要为 `sha256:96342f0f3f6756b7f2cb237f77820559d6494d594602769e24d8921e6ef0b12a`。
