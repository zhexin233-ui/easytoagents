# 执行计划

## 实现步骤

- [x] 1. 在 `tool_probe.rs` 区分官方策略文件的可信缺失与不安全/无效读取；为缺失文件、空 drop-in、字段缺失、显式值、损坏文件、动态与多来源补测试。
- [x] 2. 在策略证据构造层增加“官方来源确认不存在”的 Allowed 证据，继续绑定 Claude 版本与配置根；验证版本/根变化仍失效为 Unknown。
- [x] 3. 锁定 MCP 与 Skills 全局状态合同：Allowed + 首次无目标为 `missing`，Unknown/Blocked 分别携带独立诊断码。
- [x] 4. 将全局目标 UI helper 扩展为诊断感知的标签、说明和色调映射；给 `SyncStatusBadge` 增加默认不变的显式色调覆盖。
- [x] 5. 更新 MCP 与 Skills 页面使用共享展示模型，保持 Unknown/Blocked 禁用、Missing 可预览。
- [x] 6. 增加两页回归测试，覆盖 Missing、Unknown、Blocked 的可见文案、颜色语义、按钮状态与命令调用。
- [x] 7. 更新后端质量规范中“策略源缺失”的合同；仅在形成新的前端通用约定时更新前端规范。

## 验证命令

```bash
pnpm test --run src/features/mcp/mcp-page.test.tsx src/features/skills/skills-page.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml app::tool_probe
cargo test --manifest-path src-tauri/Cargo.toml mcp::service
cargo test --manifest-path src-tauri/Cargo.toml skills::service
pnpm format:check
pnpm lint
pnpm typecheck
pnpm check
```

## 风险与回滚点

- `tool_probe.rs` 的读取结果分类是核心风险点；先完成矩阵测试，再修改调用逻辑。
- 不修改用户配置、不创建系统级策略文件、不写入原生 Claude/Codex 目标。
- 若全量 Rust 测试发现其它调用依赖“缺失即 Unknown”，回到规划阶段审查，而不是放宽更多错误分支。
- UI 色调覆盖必须保持所有未传覆盖参数的 `SyncStatusBadge` 调用行为不变。

## 启动前检查

- [x] PRD 无未决问题，验收标准覆盖安全与可用性。
- [x] `design.md` 的后端判定矩阵与用户批准一致。
- [x] `implement.jsonl` 与 `check.jsonl` 含真实规范上下文。
- [x] 用户明确批准本计划后再运行 `task.py start`。
