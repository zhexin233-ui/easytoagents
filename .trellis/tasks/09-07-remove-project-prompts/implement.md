# 实施计划：完整移除项目级 Prompt

## 1. 实施前检查

- [x] 阅读 implement.jsonl 注入的 backend/frontend/database/cross-layer 规范与研究结论。
- [x] 确认工作区现有改动，只修改本任务范围，不覆盖用户未提交内容。
- [x] 用 rg 建立项目 Prompt 符号基线：PromptProject、prompt_project、promptProject、prompt_file、项目提示词及 adapter project Prompt descriptor。

## 2. 新增 v18 前向迁移

- [x] 新增 0018_remove_project_prompts.sql，并在 db/mod.rs 追加版本 18。
- [x] 精确收集项目 Prompt target、snapshot 与 run；创建可重试的私有快照退休队列。
- [x] 按 FK 顺序清理 Prompt native rows、snapshot、sync item、空 run、managed target 和 assignment 表；保留混合 run 与其他资源数据。
- [x] 用精确旧锚点收紧 managed_targets/project_native_resources CHECK；为 v18 添加 precondition 校验。
- [x] 在 Database::open 后增加严格限定于 AppPaths.snapshots 的退休文件清理；不得读取或删除项目 target_path。
- [x] 更新所有 schema-version 断言到 18。
- [x] 添加 v17 升级、全新库、重开幂等、迁移失败原子性、FK check、混合数据保留及项目文件字节不变测试。

## 3. 收窄全局 Prompt API

- [x] 删除项目 Prompt assignment DTO、repository、service、command、module export 与 lib 注册。
- [x] 删除 Prompt 档案删除时的项目引用计数保护。
- [x] 把 preview_prompt_sync 和 prepare_prompt_sync 收窄为全局目标。
- [x] 从 ApplyProfilePreviewInput 与 apply_profile_preview 删除 projectId，固定 Global scope 校验；保留 Provider/Prompt 共用管线。
- [x] 更新全局 Prompt、Provider、onboarding 相关 Rust 测试，确保全局行为不变。

## 4. 移除项目 Prompt 观察与写入

- [x] 从 Claude、Codex、Cursor、ZCode adapter 删除项目 Prompt descriptor，保留全局 Prompt 与其他项目 descriptor。
- [x] 从 projects/service 的项目目标集合删除 Prompt。
- [x] 从 projects/native_resources 删除 Prompt 观察、managed 判断、PromptFile DTO/状态/动作/测试。
- [x] 从 sync 模型与 apply 删除 PromptFile variant 和 match arm，保留通用 snapshot、WholeDocument 与全局 Prompt Apply。
- [x] 回归 MCP/Skill/Hook 的项目发现、禁用、恢复、冲突与项目移除阻塞。

## 5. 生成绑定并裁剪前端

- [x] 运行 pnpm bindings:generate；确认生成文件删除项目 Prompt RPC/DTO、projectId 与 prompt_file。
- [x] tool-metadata 删除 promptProject；profile-api 删除项目 Prompt query。
- [x] project-detail-page 删除 Prompt 资源视图、中央分配组件与原生 Prompt UI 分支。
- [x] prompts-page、onboarding 和其他全局调用改用无 projectId 的全局 Preview/Apply 签名。
- [x] 删除项目 Prompt 正向测试，补充项目详情无 Prompt 的负向断言；保留并修复全局 Prompt 回归测试。

## 6. 同步现行文档

- [x] 更新 README 的项目能力和首次使用说明。
- [x] 更新 docs/maintainers/adding-tool-adapter.md 的能力矩阵与测试清单。
- [x] 更新 backend database/quality 与 frontend quality 规范为 Prompt global-only，并记录 v18 兼容合同。
- [x] 保留归档任务和 0001–0017 历史迁移不变。

## 7. 验证顺序

1. pnpm bindings:generate
2. pnpm bindings:check
3. pnpm test --run src/features/projects/project-detail-page.test.tsx
4. pnpm test --run src/features/prompts/prompts-page.test.tsx src/features/onboarding/onboarding-wizard.test.tsx src/lib/tool-metadata.test.ts src/lib/profile-api.test.ts
5. cargo test --manifest-path src-tauri/Cargo.toml
6. pnpm typecheck
7. pnpm lint
8. pnpm rust:check
9. pnpm check
10. git diff --check

残留审计：

- 当前源码、README、docs 与现行 spec 中，项目 Prompt 公共符号和用户文案应为零。
- 允许命中范围仅限 0001–0017 历史迁移、archive 下的历史任务，以及明确说明 v18 退役行为的现行迁移/规范。
- ArtifactKind::Prompt、全局 Prompt descriptor、PromptProfile、WholeDocument、Markdown、CursorMdc、promptGlobal 必须仍有有效调用与测试。

## 8. 高风险文件与回滚点

- src-tauri/src/db/mod.rs 与 0018 migration：先独立通过升级/外键测试；失败时停止后续删除。
- src-tauri/src/sync/apply.rs：只删 PromptFile 分支，任何通用 snapshot/writer 改动都需额外审查。
- src-tauri/src/profiles/service.rs：先证明全局 Prompt 与 Provider 回归，再接受 projectId 收窄。
- src/bindings/commands.ts：只由生成器更新。
- project-detail-page.tsx：保持 MCP/Skill/Hook 视图和 preview 状态机不变。

代码回滚若发生在 v18 已落库之后，必须恢复启动前数据库备份并使用匹配版本；已清除的历史 Prompt 快照不作为回滚资产。
