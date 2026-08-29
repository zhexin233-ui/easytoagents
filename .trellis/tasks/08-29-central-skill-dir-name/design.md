# 技术设计：中央副本目录名称化

## 现状

`skills/library.rs::prepare_skill_import_budgeted` 以 `id = Uuid::new_v4()` 同时充当
DB 主键与中央目录名：`central_path = central_skills()/join(&id)`。多处用
`validate_direct_child(path, owner, id)` 把「目录名 == id」当作完整性不变量：

- `finalize_skill_import` / `verify_prepared_import_budgeted`（新导入校验）
- `inspect_central_skill`（列表/预览/删除前的中央核验，调用方传 `record.id`）
- `skills/import.rs` discovery 的 `private_record` 检查（`central_path == central/id`）

`skills.id` 受 `EntityId`（UUID）与 DB CHECK 约束，不能改为名称；故仅解耦目录名。

## 方案

### 1. 新导入使用名称目录

- `PreparedSkillImport` 结构不变（id 仍是 UUID）。`prepare_skill_import_budgeted`：
  - staging 仍为 `staging/skill-import-{id}`（内部临时，用户不可见，保留 UUID 防撞）。
  - 预检仅保留 staging 存在性检查；central 存在性检查移到解析出 `name` 之后：
    `central_path = central_skills()/join(&name)`，若已存在 → conflict「中央已存在同名技能目录」，
    走既有 err 清理分支删除 staging。
  - `finalize_skill_import` / `verify_prepared_import_budgeted` 的 central 分支改用
    `prepared.name` 做 `validate_direct_child`。
- `skills/service.rs::import_skill`：名称与 DB 既有记录冲突时，prepare 阶段即失败
  （错误码仍为 Conflict），`cleanup_failed_import` 清理 staging；DB UNIQUE(name) 仍是最终防线。

### 2. 中央核验兼容两种布局

- `inspect_central_skill(paths, id, central_path, ...)` 参数含义不变，但目录名校验放宽为
  `file_name == id || file_name == name`（新增 `name` 参数）。调用方（service.rs
  `inspect_record`/`preview_skill_content`、`quarantine_central_skill`、import.rs
  `private_record` 检查）传 `record.name`。
- 理由：迁移对「目标名被占用 / hash 漂移」的记录会跳过重命名，这些记录必须继续可用。

### 3. 启动迁移 `migrate_legacy_central_skill_directories(database, paths)`

位置：`skills/library.rs`，`AppState::initialize_internal` 在 `Database::open` 之后调用。
对每条 `skills` 记录：

```
expected = central_skills()/join(name)
if central_path == expected: continue                     // 已名称化
if validate_direct_child(old, central, id) 失败: continue  // 未知布局，不碰
match old 状态:
  NotFound =>
    expected 存在且核验通过(hash==content_hash) → 仅补 DB 更新   // 崩溃恢复
    否则 continue
  _ =>
    校验 old（真实目录、非 symlink、canonical==自身、digest_tree==content_hash）失败 → continue
    rename_import_exclusively(old, expected) 失败（含目标被占用）→ continue
    事务内：UPDATE skills SET central_path = expected
    改写受管链接（见下）
```

受管链接改写：`managed_targets(artifact_kind='skill') ⋈ managed_items(resource_kind='skill',
resource_id=skill.id)`，对 `target_path/external_key`：

- 仅当其为 symlink 且 `read_link == 旧 central_path` 时改写：在链接父目录建临时 symlink
  `.ea-migrate-{uuid}` → 新路径，`fs::rename` 原子覆盖。
- 同事务更新 `managed_items.last_applied_item_hash = hash_json({"targetType":"symlink","linkTarget":新路径})`。
- 链接缺失或指向其它位置 → 不动，交给既有 drift 检测与重新 Apply 自愈。

### 权衡

- 目标 baseline（`managed_targets.baseline_managed_hash`）不回填：改写后首次扫描可能报一次
  drift，用户重新 Apply 即恢复 InSync；避免在迁移中复用整套 scan 机制。
- 迁移函数幂等；失败不阻塞启动（记录保持 legacy 布局可用）。
- 快照恢复旧 symlink 指向已重命名路径 → 悬空 → 重新 Apply 自愈，接受。

## 测试计划

- library.rs：导入后 `central` 目录名为 frontmatter name；同名中央目录冲突时无残留。
- service.rs / import.rs：既有断言基于 DTO `central_path`，应自然通过；补 private_record 名称化断言。
- 新增迁移测试：legacy 记录 + UUID 目录 + 受管 symlink → 迁移后三处一致；二次调用幂等；
  hash 漂移记录跳过且仍可列表（inspect 双布局）。
