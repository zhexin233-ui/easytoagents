# 实施计划

## 步骤

- [ ] Step 1：tool-profiles 接入（页面 handlePreview + 两面板 prop/文案）。
- [ ] Step 2：中央操作自动同步（skills / mcp 分配与启停、项目追加勾选）。
- [ ] Step 3：设置页说明更新。
- [ ] Step 4：新增/更新测试（tool-profiles、skills、mcp、project-detail）。
- [ ] Step 5：`pnpm check` 全绿。
- [ ] Step 6：spec scenario 更新、提交、归档。

## 回滚点

- 纯前端改动，无 schema/绑定变更；按页面粒度可独立回滚。

## 验证

```bash
pnpm check
```
