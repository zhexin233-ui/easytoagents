# Skills 初始接管：根因与恢复能力核验

## 根因

- `initial_diagnostic` 只有在 baseline 双空、existing managed items 为空、desired 为空且目标目录扫描非空时返回 `SKILL_TARGET_INITIAL_UNMANAGED`；中央库是否已有同款技能不参与接管状态（`src-tauri/src/skills/service.rs:415-447`）。
- `discover_skill_import` 会把精确名称、完整树 hash、Ready 状态与中央副本复核都一致的候选标记为 `AlreadyImported`，但只把 `Importable` 候选写入可确认 context（`src-tauri/src/skills/import.rs:329-377`、`:418-422`）。
- `confirm_skill_import` 只复制中央副本并消费导入令牌，不写 assignment、managed target/item 或 native 入口（`src-tauri/src/skills/import.rs:497-658`）。
- Skills 普通 Apply 只允许创建 Missing 链接或替换已经指向已知中央根的受管链接；外部链接和普通目录被明确拒绝（`src-tauri/src/sync/apply.rs:1249-1338`、`:2136-2248`）。
- 因此“中央已有 exact match”只解决去重，不会建立 ownership；用户分配同名技能后普通 Preview 仍把同名外部入口识别为 managed projection 冲突。

## 实机形态

- `/Users/zhexin/.cursor/skills/skill-install` 是外部符号链接，目标为 `/Users/zhexin/.skills-manager/skills/skill-install`。
- `/Users/zhexin/.cursor/skills/smart-search-cli` 是外部符号链接，目标为 `/Users/zhexin/.skills-manager/skills/smart-search-cli`。
- 当前用例只需替换 Cursor 入口链接，不应修改或删除 `.skills-manager` 中的目标内容。

## 可复用 Apply 能力

- `apply_persisted_preview` 是现有唯一原生 Apply 入口，负责 preview claim、snapshot、durable journal、逐 mutation revalidation、逆序 rollback 与数据库 finalize（`src-tauri/src/sync/apply.rs:337-650`）。
- `RunJournal` / `JournalTarget` 已记录 phase、snapshot、before/after fingerprint 和临时路径，但没有普通目录 quarantine/tree snapshot 字段（`src-tauri/src/sync/apply.rs:173-196`）。
- `atomic_replace_symlink` 支持中央链接的同目录临时项、rename 与 fsync，但明确拒绝外部 symlink/Directory；应增加证据绑定的专用 takeover mutation，不能放宽普通分支（`src-tauri/src/sync/apply.rs:2136-2248`）。
- `finish_successful_apply` 已在一个 SQLite 事务中更新 target baseline、managed items 与 run success（`src-tauri/src/sync/apply.rs:2370-2550`）。

## 目录快照缺口

- `snapshots.target_type` 已允许 `directory`，但 `create_snapshot` 对目录只写空 marker；`mutation_from_snapshot` 明确拒绝递归恢复普通目录（`src-tauri/src/db/migrations/0001_initial.sql:519-557`、`src-tauri/src/sync/apply.rs:1746-1815`、`:3763-3785`）。
- snapshot storage 路径当前必须是 `snapshots/<run>/<id>.snapshot` 普通文件，显式删除也只调用 `remove_file`（`src-tauri/src/sync/apply.rs:2831-2937`、`:3861-3890`）。
- 已有 Restore Preview 能证明 Skill child snapshot 与父 managed target 的直属关系；恢复成功会把父目标标为 `external_owned_change`，这适合恢复为普通目录后的真实状态（`src-tauri/src/sync/apply.rs:3010-3185`、`:3790-3858`）。

## 规划结论

1. 复制导入与接管保持两个动作；接管准备只增加中央 assignment 并生成持久化 sync Preview，原生写仍走 `apply_persisted_preview`。
2. 外部链接和普通目录只有 exact name/full-tree-hash 且入口身份仍一致时可进入专用 takeover mutation。
3. 普通目录 mutation 前创建应用私有 `directory_tree` snapshot；保留到用户显式清理。
4. 工具 allowed root 内 quarantine 只服务当次原子替换/崩溃恢复，成功后清理；持久恢复来源是私有 snapshot。
5. Restore 从目录树 snapshot 重建普通目录后保留 assignment/managed item，并显示真实 managed drift；用户再决定取消分配或重新接管。
