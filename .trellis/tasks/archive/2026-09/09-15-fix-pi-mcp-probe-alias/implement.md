# Implement：修复 Pi MCP 探测与容器别名

## 阶段 1：探针修复

- [x] 1.1 在 `adapters/pi/probe.rs` 重构 global/project readiness 聚合，保证 global Ready 不被 project Filtered 覆盖。
- [x] 1.2 补 global Ready + project untrusted、global Ready + project filtered、project-only trusted/untrusted、exclusive 组合测试。
- [x] 1.3 运行 Pi probe/descriptor 定向单测。

## 阶段 2：Alias 观测与规范化

- [x] 2.1 在 Pi adapter 投影边界接入 `read_mcp_servers()`，输出统一 canonical 投影。
- [x] 2.2 在 Pi render 路径实现 alias → canonical 规范化并删除 alias，保留未知顶层字段与非受管条目。
- [x] 2.3 将 alias 诊断接入 preview/status 通道。
- [x] 2.4 核对 Import、Preview、drift/status、项目原生资源 observe/action/hash 均经过统一投影。
- [x] 2.5 修正 snapshot/restore 的条目读取，使 alias-only snapshot 可恢复。

## 阶段 3：回归测试

- [x] 3.1 Adapter：alias-only、canonical+alias canonical 优先、错误容器类型。
- [x] 3.2 Import：alias-only 可导入；双容器只导入 canonical。
- [x] 3.3 Apply/render：输出只含 canonical，未知顶层字段、非受管 server、未知字段均保留。
- [x] 3.4 Drift/status：alias-only 不得被判为空或 Missing。
- [x] 3.5 Native resources/Restore：observe、disable、restore 和 snapshot 恢复覆盖 alias-only。
- [x] 3.6 Pi 跨层 E2E 覆盖两项原始缺陷。

## 阶段 4：文档与质量门

- [x] 4.1 更新 `.trellis/spec/backend/pi-adapter-guidelines.md` 和必要的维护文档，统一 settings/trust 只读边界。
- [x] 4.2 运行 `cargo test --manifest-path src-tauri/Cargo.toml adapters::pi::tests --lib`。
- [x] 4.3 运行相关 MCP、native resources、Pi E2E 测试。
- [x] 4.4 运行 `pnpm bindings:check`、`pnpm check`、`git diff --check`。
- [x] 4.5 由独立 `trellis-check` 复核 AC1–AC8，重点检查 alias 是否遗漏任何生产观测入口。

## 风险文件与回滚点

- `src-tauri/src/adapters/pi/probe.rs`：scope 聚合；阶段 1 可独立回滚。
- `src-tauri/src/adapters/pi/mod.rs`、`src-tauri/src/adapters/document.rs`：优先只覆盖 Pi adapter，避免修改通用 JSON 行为。
- `src-tauri/src/mcp/import.rs`、`src-tauri/src/projects/native_resources.rs`：只有统一投影无法覆盖时才做最小调用点修改。
- 无迁移、无前端 schema 变更；任何其他工具测试回归立即回退通用层修改并收敛到 Pi 专用实现。
