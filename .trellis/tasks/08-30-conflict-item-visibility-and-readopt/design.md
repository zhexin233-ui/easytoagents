# 技术设计：冲突条目定位与重新接管

## 后端

### DTO 与预览管线

- `PreviewTargetRequest` 增加 `baseline_mismatched_items: Vec<String>` 与 `readopt_available: bool`
  （由各服务构造时填写；skills/profiles/projects 填默认值）。
- `PreviewTargetPlan` 增加同名透出字段，`#[serde(default)]` 兼容旧持久化预览。
- `build_preview_plan` 直接透传两个字段（不再自行推导，策略留在 MCP 服务）。
- `verify_managed_item_baselines` 改为返回 `(TargetScan, Vec<String>)`：Observed 时收集
  `hash_json(disk_item) != last_applied_item_hash` 或条目缺失的外部键；Missing 时返回全部既有条目键。

### readopt_mcp_target（mcp/service.rs）

```rust
pub fn readopt_mcp_target(database, environment, input: &ReadoptMcpTargetInput)
    -> Result<ReadoptMcpTargetResultDto, AppError>
```

1. 复用 prepare 的目标解析（descriptor + ensure_mcp_target + managed_targets 行 + 条目行）。
2. `scan_target` 后内联调用 `readopt_with_scan`（便于单测）：
   - `Observed`：目标行 `baseline_full_hash/full=observed.full_hash`、`managed=observed.managed_hash`
     并 bump row_version；条目按 `managed_projection[container]` 逐个 update/删除；
   - `Missing`：删除全部条目行、目标两级基线置 NULL（保持非空一致性约束）；
   - 其他：`AppError::conflict("readopt", "目标当前状态不支持重新接管")`。
3. 命令层取 `state.write_operations()` 互斥 + database 锁，模式与 restore_snapshot 一致。

## 前端

- `change-preview-dialog.tsx`：新增可选 props `readopting/onReadopt`；目标卡片在
  `baselineMismatchedItems.length > 0` 时展示「内容不一致的受管条目：a、b」；
  `target.readoptAvailable && onReadopt` 时展示「以当前内容重新接管」按钮（pending 禁用）。
- `mcp-page.tsx`：`readoptMutation` → 成功后关对话框 + 失效 + `previewMutation.mutate` 重新生成
  （直接应用模式自动续跑 Apply）。
- `project-detail-page.tsx`：`readoptMutation`（带 projectId）→ 关对话框 + 失效 + 提示再次点击
  同步按钮（子组件持有预览 mutation，不做跨层再生）。

## 测试

- Rust：verify 返回不匹配键；readopt_with_scan 的 Observed/Missing/拒绝三态；中央表不受影响；
  row_version 单调。
- 前端：mcp-page 冲突预览展示条目名并完成接管→重新预览链路；project-detail 接管携带 projectId；
  默认模式回归不变。
