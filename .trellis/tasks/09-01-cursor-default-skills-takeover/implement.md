# Skills 初始目标接管：实施计划

## 1. 合同与迁移

- [x] 更新 Skills import/takeover Rust DTO：候选动作、入口类型、`PrepareSkillTakeoverInput`、接管预览结果；保持正文和外部目标细节不进入 RPC。
- [x] 新增 snapshots `storage_kind` 前向迁移并注册 schema version；回填历史 file/metadata 行，验证升级、重开、约束和旧快照恢复行为。
- [x] 扩展私有 preview/journal 版本与向后兼容默认字段；更新生成 TypeScript bindings。

## 2. 可恢复目录树快照

- [x] 在 `sync/apply.rs` 与 `skills/library.rs` 的现有边界上提取安全目录树复制/inspect helper；保持 no-follow、预算、完整树 hash、内部链接和执行位合同。
- [x] 扩展 snapshot create/load/validate：`directory_tree` 使用应用私有 `.snapshot.d`，完整落盘并 fsync 后才允许目标 mutation。
- [x] 扩展 snapshot summary/list/delete；目录删除必须严格验证私有路径、身份/hash、活动 run 和树内条目后 no-follow 清理。
- [x] 扩展 restore preview/apply：从私有目录树重建目标同目录临时树，再替换中央链接；恢复成功保持 snapshot 并把 target 标为真实 drift。

## 3. 接管检测与准备

- [x] 扩展 `discover_skill_import`：只为正式全局目标中的 exact name/full-hash `already_imported` 候选生成 takeover eligibility；兼容来源、中央链接、冲突和内置集合保持原边界。
- [x] 将入口身份、链接链/文本、普通目录身份/hash、目标扫描与中央 row version 写入版本化私有证据；检测仍不写 assignment/target。
- [x] 实现 `prepare_skill_takeover`：验证选择、重验全部证据，在一个事务中创建/复用全局 assignments、ensure managed target、持久化 takeover-aware sync Preview 并消费检测令牌。
- [x] 对 direct Apply 明确禁用；接管准备只返回可供 `ChangePreviewDialog` 使用的持久化 Preview。

## 4. Takeover-aware Preview/Apply

- [x] 扩展 `PreviewTargetRequest`、`PersistedPreviewEnvelope`、`ApplyTargetInput`，绑定 takeover entries 与中央/目标/入口证据；普通 preview/apply 默认空集合，行为不变。
- [x] 在 `build_preview_plan` 中只为“初始双空基线 + 无旧 managed items + 已选择且 exact match”的名称解除首次同名冲突；未选碰撞和所有真实漂移继续 conflict。
- [x] 在 `build_symlink_mutations` 增加专用 `TakeoverSymlink`，不放宽普通 `ReplaceSymlink`。
- [x] Apply 前、snapshot 后、quarantine 前后和 DB finalize 前重验入口与完整树；用 allowed-root 内 run 专属 quarantine、临时链接、rename、fingerprint 和父目录 fsync 完成替换。
- [x] 扩展 journal、同步逆序回滚和启动中断恢复；目标/quarantine 外部变化时保留证据并进入 `rollback_failed`。
- [x] DB finalize 继续一次性写 managed items、完整 target baseline 与 run success；提交不确定时不得猜测回滚。成功后清理临时 quarantine，目录树 snapshot 保留。

## 5. 前端

- [x] 更新 `SkillImportDialog`：复制/接管分组、独立选择、精确可用性与理由、pending/关闭/重扫隔离；混合动作要求分步并重扫。
- [x] 接入 `prepareSkillTakeover`，把返回 plan 打开在现有 `ChangePreviewDialog`，首次接管始终要求 Apply 确认。
- [x] 更新 Skills 页面成功/错误/首次状态文案和 query invalidation；Apply 后刷新中央列表与全局状态。
- [x] 更新恢复点列表/对话框：目录树标签、可恢复状态、恢复后漂移说明和显式删除反馈。

## 6. 测试与验证

- [x] Rust import/service：Cursor 两个外部链接 exact match、普通目录 exact match、subset、多来源、已有 assignment、三工具全局目标和所有冲突/stale 反例。
- [x] Rust apply/snapshot：目录树持久化、恢复、显式删除；第二项失败；rename/link/SQL/commit 前后 fault；崩溃检测、逆序回滚、rollback_failed、提交后 quarantine 清理。
- [x] 证明外部符号链接目标 inode/字节/权限不变；普通目录恢复点 hash 一致；未选兄弟、`.system`、assignment/managed/sync 表边界正确。
- [x] React：两类选择与 payload、takeover Preview、direct Apply 禁止、重复提交/关闭/晚到响应、成功刷新、目录恢复点预览/恢复/删除。
- [x] 运行目标 Rust 测试、前端目标测试、bindings 生成/检查、`pnpm check` 与 `git diff --check`。

## 7. 规范、评审与回滚点

- [x] 更新 `.trellis/spec/backend/skill-import-guidelines.md`：接管动作、普通目录快照、Preview/Apply、恢复矩阵和必需测试。
- [x] 更新 `.trellis/spec/backend/quality-guidelines.md`：持久化 Preview 的专用 takeover mutation 仍由唯一 Apply 写入口执行。
- [x] 复核 README/用户文案是否需要说明恢复点占用空间；不扩展项目级接管。
- [x] 质量检查重点：任何普通目录路径都不能在私有 snapshot 持久化前被移动/删除；普通 Apply 不能借 takeover 代码覆盖未证明目录。
- [x] 回滚点：若目录树恢复或 journal 无法证明安全，禁用普通目录 eligibility，保留外部链接接管；不得自动删除普通目录。

## 验证命令

实施前根据实际 Cargo 测试模块补齐精确过滤器；最终至少执行：

```bash
pnpm test -- --run src/features/skills/skills-page.test.tsx src/components/snapshot-restore-dialog.test.tsx
pnpm check
cargo test --manifest-path src-tauri/Cargo.toml skills::
cargo test --manifest-path src-tauri/Cargo.toml sync::
git diff --check
```
