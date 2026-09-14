# 执行计划：修复渠道切换冲突弹窗卡死

按序执行；后端 command 与 bindings 完成后再接前端。每一阶段先补失败测试，再完成实现。

## 步骤 1：Provider readopt 后端能力

- [x] 1.1 在 Profiles DTO 中增加 `ReadoptProviderTargetInput` / `ReadoptProviderTargetResultDto`，从模块公开导出。
- [x] 1.2 在 Profiles 同步服务增加 `readopt_provider_target`：校验 capability 与精确 targetPath，安全扫描当前整文档目标，在 `IMMEDIATE` 事务中更新或清空 baseline，不写原生文件。
- [x] 1.3 Provider 预览只为 `ExternalOwnedChange` 等可安全读取的冲突设置 `readopt_available=true`，其他阻塞状态继续 fail closed。
- [x] 1.4 增加 Tauri command 并登记到 app command surface。
- [x] 1.5 增加后端测试：外部修改 -> 冲突 -> readopt -> 新预览 -> Apply；路径不匹配/不可读拒绝；旧冲突 Preview 仍拒绝 Apply。

验证：`cargo test --manifest-path src-tauri/Cargo.toml profiles::`、`cargo fmt --check --manifest-path src-tauri/Cargo.toml`、`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`。

## 步骤 2：生成绑定与前端续跑

- [x] 2.1 运行 `pnpm bindings:generate`，确认新 command/DTO 只由 Specta 生成；运行 `pnpm bindings:check`。
- [x] 2.2 `ToolProfilesPage` 接入 hook 的 `readopt`、`onReadopted` 和错误消息，路径缺失时 fail closed。
- [x] 2.3 向 `ChangePreviewDialog` 传 `readopting` / `onReadopt`；沿用现有按钮、禁用态、焦点和关闭语义。
- [x] 2.4 修改 Provider 页面测试：直接模式冲突可 readopt，调用精确 payload，旧 Preview 不 Apply，新 Preview 自动 Apply且无二次普通确认。
- [x] 2.5 增加预览确认模式测试：readopt 后生成新 Preview但等待用户确认；覆盖 readopt 失败后可取消/重试。

验证：`pnpm test --run src/features/tool-profiles/tool-profiles-page.test.tsx`、`pnpm typecheck`、`pnpm lint`、`pnpm format:check`。

## 步骤 3：全量质量检查

- [x] 3.1 按前后端 quality/error/hook 规范复核 Preview/Apply、错误反馈、路径身份、敏感信息与查询失效。
- [x] 3.2 运行 `pnpm check` 与 `git diff --check`。
- [x] 3.3 定向复核 MCP/Hook/Agent 既有 readopt 流程未因共享 hook 改动回归。

## 步骤 4：Spec、提交与收尾

- [x] 4.1 若实现确认 Provider readopt 成为稳定合同，使用 `trellis-update-spec` 更新前后端质量规范。
- [x] 4.2 经最终检查后按项目规范创建简体中文 commit，归档 Trellis 任务并记录 journal。

## 风险与回滚点

- `readoptAvailable` 判定过宽会把 parse/权限/路径安全错误误显示成可恢复冲突；测试必须覆盖不可 readopt 矩阵。
- `onReadopted` 若丢失原 `directApply` 值，会让直接模式再次停在普通确认弹窗；两种 applyMode 都要有状态机测试。
- 旧 Preview 必须保持不可消费；readopt 后只能使用新 `previewId`。
- 本任务无 schema 变更；后端、bindings、前端应作为一个兼容单元回滚。
