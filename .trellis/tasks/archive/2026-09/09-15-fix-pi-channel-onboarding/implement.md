# 实施计划

## 1. 路由与向导过滤

- [ ] 在 `TOOL_PROFILE_ROUTES` 注册 Pi，并补真实路由清单/导航回归测试。
- [ ] 修改 onboarding 的发现、选择恢复、卡片渲染、`canPrepare` 与 prepare 编排，统一使用派生的可处理 Provider/Prompt 状态。
- [ ] 补测试：完全接管隐藏卡片、部分接管隐藏单项、已接管项零 preview 调用、暂停恢复旧选择后不提交已接管项、未发现/未安装/显式跳过行为不回归。

## 2. 布局修复

- [ ] 为共享 `DialogBody` 增加安全收缩约束，为 onboarding Grid/卡片/预览容器增加局部 `min-w-0` 与必要的 `max-w-full`。
- [ ] 扩展 Dialog/Onboarding 类合同测试，断言滚动所有权仍属于 body 与 `<pre>`。
- [ ] 启动隔离前端页面，用浏览器窄 viewport 验证无对话框级横向溢出，并记录无法由 jsdom 证明的桌面差异。

## 3. Pi Skills 来源

- [ ] 在 Rust source enum 中增加 Pi 全局来源并接入 `<pi_agent_dir>/skills`；将其纳入正式 managed source 判定。
- [ ] 复核 discovery、confirm、takeover 的环境指纹、正式根、allowed root、中央链接排除和 stale 重验均覆盖 Pi，不新增绕过分支。
- [ ] 重新生成 TypeScript bindings。
- [ ] 扩展隔离 fixture 测试：Pi 来源 missing/empty/ready、复制导入不改原入口且无 assignment/managed/sync 副作用、同 hash 外部链接/目录可显式接管、中央受管链接不可接管、确认前来源变化失败。
- [ ] 扩展 Skills 前端测试，确保 Pi 检测结果使用现有复制/接管分组与精确 payload，direct 模式仍只打开 Preview。

## 4. 规范与质量门禁

- [ ] 更新 `.trellis/spec/backend/skill-import-guidelines.md` 的来源表、DTO/source kind、正式接管来源与 Pi 测试矩阵。
- [ ] 更新 `.trellis/spec/frontend/quality-guidelines.md` 的 onboarding 已接管项过滤/恢复合同和测试要求。
- [ ] 运行聚焦测试，再运行 `pnpm bindings:generate`、`pnpm bindings:check`、`pnpm typecheck`、`pnpm lint`、`pnpm test --run`、`pnpm rust:check`、`pnpm build`、`git diff --check`。
- [ ] 使用 Trellis check 子代理执行 spec、跨层数据流、测试覆盖和回归复核；修复所有已验证问题后再提交。

## 风险文件与回滚点

- `src/features/onboarding/onboarding-wizard.tsx`：选择状态与 preview 编排耦合，任何过滤必须同时覆盖渲染、恢复和提交。
- `src/components/ui/dialog.tsx`：共享样式影响面广；若其他弹窗回归，保留 onboarding 局部约束并撤销共享层改动。
- `src-tauri/src/skills/import.rs`：安全边界敏感；Pi 只能进入现有证据链，不允许新增弱化验证的专用确认路径。
- `src-tauri/src/skills/models.rs` / generated bindings：必须成对更新并通过严格绑定检查。

