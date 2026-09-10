# 工程配置与依赖整理

父任务：`09-10-codebase-optimization`。覆盖审阅条目 F1、F2、F3、F4、E15。

## 目标
让每次 push 与 PR 都有质量门，清理过时文案、归档依赖与死代码。

## 需求
- R1 / F1：新增 `.github/workflows/check.yml`，对 push 到 `main` 与全部 PR 执行 `pnpm install --frozen-lockfile`、`pnpm check`、`pnpm bindings:check`；运行在 macOS runner 上（Tauri 与 rust:check 需要）；使用与 `release.yml` 相同的 pinned action SHA 风格与 `permissions: contents: read`。
- R2 / F2：`src-tauri/Cargo.toml description`、`tauri.conf.json shortDescription` 更新为覆盖五个工具的描述，与 README 口径一致。
- R3 / F3：`reqwest` 升级到 0.12（保持 `default-features = false` 与 `rustls-tls`、`socks`、`json`）；`serde_yaml` 替换为维护中的 `serde_yml` 或等价方案，行为不变；`cargo test` 全绿。
- R4 / F4：为 `src-tauri/src/hooks/import.rs` 补 discover/confirm 的 fixture 测试，覆盖至少：发现候选、确认导入、重复候选、无效脚本。
- R5 / E15：删除全部 7 个 `.gitkeep`（`src/hooks/`、`src/features/sync/`、`src/features/providers/` 空目录随之删除，`sync/` 目录由前端重构任务重新创建）；删除无引用的 `src/lib/app-info.ts appInfoQueryOptions`（若整个文件无引用则删文件）；把 `unwrapResult/ProfileRpcError/profileErrorText` 从 `profile-api.ts` 拆到 `src/lib/rpc.ts`，`profile-api.ts` 只保留 profiles 查询并 re-export 以兼容；`hookImportQueryOptions` 从 `hook-import-dialog.tsx` 移到 `hooks-api.ts`。

## 验收条件
- A1：工作流文件通过 `actionlint`（若本地可用）或人工核对语法；本地 `pnpm check` 与 `pnpm bindings:check` 全绿。
- A2：`grep -rn "Claude 与 Codex" src-tauri` 无命中。
- A3：`Cargo.lock` 中不再出现 `reqwest 0.11`、`hyper 0.14`、`serde_yaml`；GitHub 导入的单元测试全部通过。
- A4：`hooks/import.rs` 测试行数大于 0 且覆盖四类场景。
- A5：`git ls-files | grep .gitkeep` 为空；`rg appInfoQueryOptions src` 为空；所有原 `profile-api.ts` 导入方仍编译通过。

## 范围外
发布流程改动、依赖以外的 Rust 代码重构。
