# 实施计划

- [ ] Step 1：后端管线（scan 返回键、request/plan 字段、serde default、构造点补默认值）。
- [ ] Step 2：readopt 服务函数 + 模型 + 命令 + lib.rs 注册；bindings 再生成。
- [ ] Step 3：对话框 UI + mcp-page / project-detail 接线。
- [ ] Step 4：Rust 与前端测试；`pnpm check` 全绿。
- [ ] Step 5：spec 更新（预览/冲突指引）、提交、归档。

## 回滚点

- Step 1/2 后端可独立回滚（新字段 serde default，旧预览不受影响）；Step 3 纯前端。
