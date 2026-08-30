# 技术设计：Provider/提示词接入与中央操作自动同步

## Provider/Prompt 接入（tool-profiles）

- `tool-profiles-page.tsx`：新增 `settingsQuery`/`directApply`；抽 `handlePreview(plan, artifactKind)`
  —— `directApply && canAutoApplyPreview(plan)` 时 `applyMutation.mutate({ plan, artifactKind })`
  （onSuccess 消息沿用「已应用 N 个目标」），否则 `setOpenPreview`。把 `directApply` 传给两个面板。
- `provider-panel.tsx` / `prompt-panel.tsx`：新增 `directApply` prop，按钮文案
  「预览渠道同步」→「直接应用渠道同步」、「预览提示词同步」→「直接应用提示词同步」、
  「切换并预览」→「切换并直接应用」。`activateMutation`/`previewMutation` 的 `onPreview`
  回调签名不变，决策集中在页面。

## 中央操作自动同步

复用各页面既有 previewMutation（其 onSuccess 已含直接应用分支），在中央操作成功后触发：

- `skills-page.tsx` `globalAssignmentMutation.onSuccess`：invalidate 后
  `if (directApply) previewMutation.mutate({ tool })`，否则保留原提示。
- `mcp-page.tsx` `globalAssignmentMutation.onSuccess`：同理 `previewMutation.mutate({ tool })`。
- `mcp-page.tsx` `enabledMutation.onSuccess`：`server.globalTools` 逐个
  `previewMutation.mutate({ tool })`（未分配任何工具则跳过）。
- `project-detail-page.tsx` 两个子组件的 `assignmentMutation.onSuccess`：invalidate 后
  `if (directApply) previewMutation.mutate()`，否则保留原提示。

预览读后端 DB（ mutations 已提交），与前端缓存刷新时序无关；连续操作的多次
apply 由后端单写者互斥串行，过期预览按既有 `STALE_PREVIEW` 拒绝（安全失败）。

## 设置页与 spec

- settings-page 描述改为：覆盖 MCP/Skills 全局同步与项目追加、Provider 与提示词同步；
  分配、启停等中央操作会自动同步；冲突/错误回退对话框。
- frontend/quality-guidelines.md 的 direct-apply scenario 更新签名与契约。

## 测试

- tool-profiles-page.test：direct 模式 provider 无冲突自动 Apply（断言 previewId 与无对话框）、
  冲突回退；默认模式回归不变。
- skills-page.test：direct 模式分配切换 → 自动 previewSkillSync + applySkillPreview。
- mcp-page.test：direct 模式分配切换与启停 → 自动 previewMcpSync + applyMcpPreview。
- project-detail-page.test：direct 模式勾选项目追加 → 自动 applyMcpPreview。
