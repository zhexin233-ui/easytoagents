# 修复 GitHub 质量门 workflow

## Goal

修复 `zhexin233-ui/easytoagents` 质量门在 Rust Clippy 阶段的失败，使当前提交能够通过 GitHub Actions 的 `pnpm check`，同时保持现有行为不变。

## Background and confirmed facts

- 最近的质量门 run #21（run ID `35301601309`）和前一 run #20 都在唯一 job 的“全量检查”步骤失败；失败命令是 `pnpm check`，不是前端格式、lint、类型或 Vitest 步骤。
- GitHub 日志在 `src-tauri/src/skills/library/walk.rs:670` 报告 `clippy::byte_char_slices`：`hasher.update([b'F']);` 可以更简洁地写成字节字符串。
- GitHub 日志在 `src-tauri/src/sync/apply/plan.rs:70` 报告 `clippy::useless_conversion`：`render_local_exclude` 的参数已经接受 `IntoIterator`，调用方不需要显式 `.into_iter()`。
- 质量门通过 `cargo clippy --all-targets -- -D warnings` 将上述 Clippy 诊断视为错误，因此 Rust 编译失败并使后续绑定检查跳过。
- 本地前端检查、绑定检查和 Rust 测试均已通过；本任务只处理已确认的两个 Clippy 阻断点。

## Requirements

1. 将 `hash_file_record` 中的单元素字节数组表达式改为 Clippy 推荐的等价字节字符串表达式，不改变写入哈希的数据。
2. 删除 `build_target_work` 调用 `render_local_exclude` 时多余的 `.into_iter()`，保持传入的 `patterns` 及渲染结果不变。
3. 不修改质量门触发条件、Node.js 警告、发布版本配置、绑定生成逻辑或任何与这两个诊断无关的产品行为。

## Acceptance Criteria

- [x] `cargo fmt --check --manifest-path src-tauri/Cargo.toml` 通过。
- [x] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` 通过，且不再报告 `walk.rs:670` 或 `plan.rs:70` 的两项诊断。
- [x] `cargo test --manifest-path src-tauri/Cargo.toml` 通过。
- [x] `pnpm check` 通过，且现有前端格式、lint、类型、Vitest 检查未回归。
- [x] `pnpm bindings:check` 通过，生成的 TypeScript 绑定无非预期变化。
- [x] `git diff` 仅包含上述两处等价 Rust 清理及必要的任务记录，不引入无关改动。

## Out of scope

- 不在本任务中重构 `.github/workflows/check.yml` 或拆分其聚合检查步骤。
- 不处理 `actions/cache` 的 Node.js 20 弃用 warning；它不是本次失败原因。
- 不修复发布 workflow 的默认版本 `0.1.0`、过时的图标文档或其他未被本次质量门日志确认的问题。

## Open questions

无。失败诊断、修复范围和可验证的验收标准均已由仓库与 GitHub run 日志确定。
