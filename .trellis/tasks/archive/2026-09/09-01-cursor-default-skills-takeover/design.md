# Skills 初始目标接管：技术设计

## 1. 设计目标与边界

本任务为全局 Skills 初始目标增加一条显式、安全、可恢复的接管路径。它只处理目标根直属、名称与完整树内容都和 Ready 中央技能精确一致的条目；内容不同仍是冲突。

核心不变量：

1. 检测与接管准备不写原生目标；只有持久化 Preview 后的显式 Apply 可以替换入口。
2. 接管资格由名称、完整树哈希、中央记录/副本、入口路径身份和目标扫描共同证明，不能只比较 `SKILL.md` 或链接名称。
3. 外部符号链接接管只替换入口链接，不触碰其链接目标。
4. 普通目录接管前必须生成可恢复的应用私有目录树快照；恢复点不会自动清理。
5. 同步文件变更、managed target/items、baseline 和 run 状态沿用同一 Apply journal/rollback 边界。
6. 普通复制导入仍不分配、不接管；接管是候选上的独立显式动作。
7. MVP 只处理 Claude、Codex、Cursor 的全局 Skills 目标，不扩展项目级接管。

## 2. 用户流程

### 2.1 检测与动作分流

`discover_skill_import(tool)` 继续扫描既有显式来源。候选仍保留
`importable` / `already_imported` / `name_conflict` / `invalid` 事实状态，并增加安全动作信息：

| 候选事实 | 可用动作 | 说明 |
| --- | --- | --- |
| 中央不存在同名项 | 复制到中央库 | 沿用现有 `confirm_skill_import`，不接管 |
| 中央精确 name/hash 匹配，且正式全局目标中存在同一入口 | 预览接管 | 支持外部符号链接和普通目录 |
| 中央精确匹配，但候选只来自 `.agents/skills` 等兼容来源 | 无接管动作 | 兼容来源不是本工具正式受管目标 |
| 入口已经直接指向已知中央副本且 ownership/baseline 有效 | 已受管 | 不重复接管 |
| 名称/内容/大小写/中央状态冲突 | 无动作 | 保留冲突保护 |

前端为复制和接管维护独立选择集合。复制按钮只提交 `importable`；接管按钮只提交本次检测证明可接管的 `already_imported`。若两类都存在，用户完成一种动作后重新检测另一种，避免把中央复制事务与原生 Apply 合并成一个不可恢复批次。

### 2.2 接管准备与确认

新增窄命令：

```rust
prepare_skill_takeover(
    state: State<'_, AppState>,
    input: PrepareSkillTakeoverInput,
) -> Result<SkillTakeoverPreviewResultDto, AppError>;
```

输入只包含一次性 Skills 检测 `previewId` 和 1–32 个非重复 `candidateIds`。命令完成：

1. 重验来源环境、正式目标、入口身份/类型/链接文本、完整树 hash、中央记录 row version 与中央副本。
2. 只接受正式全局目标根中的精确匹配候选；兼容来源不能被借作接管目标。
3. 原子创建或复用所选中央技能对当前工具的全局 assignment、确保 managed target，并持久化 takeover-aware sync Preview。
4. 消费 Skills 检测令牌，返回普通 `PreviewPlan` 及本次新增/复用分配计数。
5. 不写原生入口，不创建 managed item/baseline，不自动 Apply。

即使设置为 direct Apply，前端也必须打开 `ChangePreviewDialog`；首次接管不允许跳过第二次原生写入确认。用户取消 Preview 时，中央 assignment 作为已表达的同步意图保留，重新检测可再次生成接管 Preview。

## 3. 接管证据与持久化合同

### 3.1 候选证据

现有 `skill_import_previews.context_json` 版本升级，给可接管候选保存：

- tool、正式 target root/path 与目标完整扫描 hash；
- skill id/name/row version/content hash/central path/status；
- source root、直属 entry path、entry type；
- 普通目录的 device/inode/树 hash，或符号链接的原始 link text、入口身份、解析链与最终目录身份；
- 现有 managed target/item/assignment 指纹和环境指纹。

DTO 只展示安全名称、入口类型、可用动作和固定理由；不返回正文、任意 frontmatter、外部真实目标细节或恢复路径。

### 3.2 Sync Preview 扩展

`PreviewTargetRequest` / `PersistedPreviewEnvelope` 增加版本化的 `skillTakeoverEntries`。每项绑定：

- skill/entry identity 与 content hash；
- 预览时 entry fingerprint 和来源证据摘要；
- 期望中央 link target；
- 是否需要 `directory_tree` 恢复点。

`build_preview_plan` 只在以下条件全部成立时把初始同名 managed projection 冲突收窄为可 Apply 的接管更新：

- baseline 双空且既有 managed items 为空；
- takeover entry 覆盖全部被接管名称，未选同名碰撞仍保持冲突；
- 每项 name/hash/identity/central row version 已复核；
- assessment 的其它能力、策略、信任、权限和路径条件都允许；
- 项目级目标、半基线、旧 managed item 漂移或活动 writer 均不进入该分支。

公开 Preview 以安全 diff 展示 `directory` / `external_symlink` → `managed_symlink`，并对普通目录标记“已创建可恢复目录树快照”。私有 evidence 保存在现有 `sync_items.redacted_diff_json` envelope 中，不新增第二套 preview 表。

## 4. Apply 与文件系统事务

### 4.1 Takeover mutation

`ApplyTargetInput` 增加已从持久化 Preview 重建并复核的 takeover entries；`build_symlink_mutations` 对这些名称生成专用 `TakeoverSymlink` mutation，而不是放宽普通 `ReplaceSymlink`。

每个 mutation 的顺序：

1. 完成所有 capability/path/row-version/target full hash/entry full tree hash 预检。
2. 创建 snapshot 与 journal。普通目录必须把完整目录树复制到应用私有 snapshots 根并复核 hash；符号链接沿用 link text snapshot。
3. 在工具 allowed root 下创建 run 专属、权限受限的 quarantine 目录，绑定 UUID、父目录和身份。
4. 再次复核入口身份与完整内容，将原入口 rename 到 quarantine，fsync 两个父目录。
5. 在原位置用临时链接 + rename 建立指向中央副本的链接，记录 temporary/quarantine path 与 fingerprint，fsync 父目录。
6. 复核 quarantine 中的原入口、中央链接和完整目标扫描；任一不一致逆序恢复。
7. 全批目标验证通过后，在一个 SQLite IMMEDIATE 事务中写 managed items、完整 baseline、run success；assignment 已在接管准备阶段持久化。
8. DB 成功后只清理工具根中的临时 quarantine。应用私有目录树 snapshot 保留到用户显式删除。

普通 Apply 的 `ReplaceSymlink` 仍只允许 Missing 或已证明属于中央根的链接；外部链接/目录只有携带本次 takeover evidence 才能进入专用 mutation。

### 4.2 Journal 与崩溃恢复

`RunJournal` 版本升级，`JournalTarget` 以向后兼容默认字段增加：

- `quarantine_path` / `quarantine_fingerprint`；
- `takeover_entry_type`；
- `directory_tree_hash`；
- `snapshot_storage_kind`。

同步失败时优先用 quarantine 原子恢复；若崩溃后 quarantine 缺失但私有 `directory_tree` snapshot 有效，可在目标同目录创建临时树、复核 hash 后恢复。目标或 quarantine 被外部修改时拒绝覆盖并保持 `rollback_failed`。

DB 已提交但 quarantine 清理前崩溃时，启动清理只在 run 已 succeeded、当前入口仍为期望中央链接、quarantine identity/hash 与 journal 完全一致时清理；否则保留证据并报告，不猜测删除。

## 5. 目录树快照、恢复与删除

### 5.1 前向迁移

新增迁移，为 `snapshots` 增加 `storage_kind`：

- `payload_file`：既有普通文件快照；
- `metadata_only`：既有 missing/symlink/旧 directory marker；
- `directory_tree`：本任务新增的可恢复 Skill 目录树。

迁移把历史 file 行回填为 `payload_file`，其它历史行回填为 `metadata_only`。新目录树使用严格路径
`snapshots/<run_id>/<snapshot_id>.snapshot.d/`，`content_hash` 保存完整 Skill 树 hash；不修改历史迁移。

### 5.2 安全复制

目录树复制复用 Skills library 的有界、no-follow、描述符绑定读取规则：

- 只复制已经通过接管资格的 Skill 树；
- 保留相对路径、文件字节、安全内部链接文本和所有者可执行位；
- 不承诺 inode、mtime 或其它不参与 Skill hash 的元数据；
- 每个文件落盘后 flush/fsync，目录树和 snapshots 父目录 fsync；
- 复制前后与恢复前重新计算完整树 hash。

### 5.3 恢复点行为

`SnapshotSummary` 增加 storage kind/restorable 信息，现有恢复点 UI 显示“目录树恢复点”。`preview_restore` 对 `directory_tree`：

1. 证明 snapshot 位于应用私有 snapshots 根、路径无链接且树 hash 未变；
2. 绑定原 managed target 与直属 Skill child 关系；
3. 绑定当前入口 fingerprint；
4. 生成持久化 Restore Preview。

Restore Apply 先把私有 snapshot 安全复制到目标同目录临时树，复核后替换当前中央链接。恢复成功后保留 snapshot，并把父 managed target 标为 `external_owned_change`；assignment/managed item 不被悄悄删除，用户可选择取消分配或重新接管。

显式删除恢复点时按 `storage_kind` 分支：文件用 `remove_file`；目录树只有在路径严格匹配应用私有 snapshot 命名、无链接组件、manifest/identity/hash 有效且 source run 不活动时，才执行 no-follow 递归删除。任何未知条目或身份变化都拒绝删除。

## 6. 数据库与兼容性

- 新迁移只扩展 snapshot storage kind；takeover candidate/preview evidence 继续使用版本化 JSON，不增加明文内容列。
- assignment、managed target/items 与 baseline 沿用既有表和触发器。
- Claude/Codex/Cursor 共享全局逻辑；各自 allowed root 继续由 descriptor/environment 显式解析。
- `.system` 内置排除、未知应用私有路径拒绝、256/32/128 MiB 限额和一次性 preview token 不放宽。
- 旧 preview/journal/snapshot 通过 serde default 与 `metadata_only` 保持可读；旧 directory marker 仍明确不可恢复。
- 无需改变 `SyncStatus` 枚举；接管 Preview 用 warning/diagnostic 表达，Apply 后仍落到 `in_sync`。

## 7. 前端交互

- `SkillImportDialog` 分为“复制到中央库”和“接管到当前工具”两组选择；默认都不勾选。
- `already_imported + takeoverEligible` 可在接管组选择；兼容来源、已受管、冲突与 invalid 保持禁用并显示固定理由。
- “预览接管所选”调用 `prepareSkillTakeover`，成功后关闭导入层并打开现有 `ChangePreviewDialog`；不走 direct Apply。
- Preview 清楚展示入口类型变化、外部符号链接目标不被修改、普通目录已建立恢复点。
- 全局状态文案从“导入不会自动接管”改为同时说明：新技能先复制；中央同款可显式预览接管。
- Snapshot restore dialog 显示目录树恢复点、恢复后的漂移含义和显式清理动作。

## 8. 验证与回滚

必须覆盖：

- 外部绝对/相对符号链接与普通目录 exact match；subset takeover；多工具共享中央副本；
- 内容、名称大小写、entry type、link text、目录身份、树 hash、中央 row version、assignment、target full hash 在检测/Preview/Apply 各阶段变化；
- 普通目录 snapshot copy 第二项失败、quarantine/rename/link/SQL/commit 前后故障与崩溃恢复；
- 外部链接目标 inode/内容不变，普通目录 snapshot 可恢复，未选兄弟不变；
- succeeded 后状态/baseline/items，恢复后 external-owned drift，显式删除目录树 snapshot；
- import-only、普通 Apply 冲突、旧 snapshot/journal、MCP、Claude/Codex/Cursor 回归；
- 生成 bindings、Rust/React 测试、`pnpm check`、`git diff --check`。

若目录树恢复或 journal 证明无法闭环，回滚本任务应关闭普通目录的 takeover eligibility；不能退化为无恢复点的删除，也不能放宽普通 Apply 覆盖目录。
