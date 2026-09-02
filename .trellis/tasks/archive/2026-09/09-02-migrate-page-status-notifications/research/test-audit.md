# 通知迁移测试审计

## 共享契约

- `src/components/notify.test.tsx:18-112` 已覆盖 success/error role、3,000 ms 消失、通知替换重启完整计时和 unmount 清理。
- 页面测试不必复制全部 timer 单测；应通过 `role="status"`/`role="alert"` + 文案 + 唯一呈现证明结果走 `Notify`，并保留对命令 payload/时序的断言。

## MCP

- 表单错误保留：`src/features/mcp/mcp-page.test.tsx:725-778` 已验证校验/RPC 错误在对话框内、输入保留与关闭清理。
- 手动预览/Apply：`:966-1000` 已断言预览内容和精确 `previewId`，需增加手动 Apply success notify。
- direct 成功/失败：`:1002-1066` 已覆盖 success status 与唯一 error alert，迁移时保持。
- 重新接管：`:1098-1150` 已断言 payload 和再预览，需增加动态计数 success notify 与失败替换断言。
- 删除 `:531-550`、分配/启停 `:552-643`、导入 `:1420-1474` 等现有流程需增补 role 断言；保存、删除失败缺口需新用例。

## Skills

- 表层内容预览/删除失败：`src/features/skills/skills-page.test.tsx:691-721` 当前只断言内联 alert，需改为唯一 error notify，保留错误前缀。
- 手动 Apply `:818-847` 需增 success notify；direct 成功/失败 `:849-934` 已覆盖 role，保持。
- 零目标 `:936-958` 、preview-confirm 分配 `:960-1063` 当前断言旧内联文案，需改为 success status 且唯一。
- 本地目录导入 `:620-690`、已有 Skills 导入 `:1192-1371`、takeover `:1689-1769` 需增/更新页面 success notify 断言；对话框内错误断言保持。

## Prompts

- 表单保存失败 `src/features/prompts/prompts-page.test.tsx:261-291` 保持对话框内错误和输入状态断言。
- 删除 `:293-317` 需增 success/error notify；编辑、分配、手动 Apply `:437-510` 需将成功反馈断言改为 status。
- direct 成功/失败 `:513-596` 已覆盖 success status 和唯一 error alert，保持。
- discover 无结果和 confirm import 成功需增 success status；assignment/delete/discover/confirm-import 失败需增唯一 error alert。

## 验证命令

```bash
pnpm test --run src/features/mcp/mcp-page.test.tsx src/features/skills/skills-page.test.tsx src/features/prompts/prompts-page.test.tsx
pnpm typecheck
pnpm lint
pnpm check
```

