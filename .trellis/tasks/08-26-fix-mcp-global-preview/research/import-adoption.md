# MCP 原生全局导入：可复用机制核验

## 事实（代码证据）

- Provider 导入已有完整“发现→持久化预览→确认”令牌：`discover_provider_import` 仅在该工具没有中央 profile 时发现，保存 `target_path` 与 `observed_full_hash`，状态为 `previewed`（`src-tauri/src/profiles/service.rs:323-374`）。确认重新读取原生配置，要求路径和 hash 都与预览一致，否则 `STALE_PREVIEW`（`service.rs:387-407`）。
- Provider 确认委托 `adopt_imported_provider`；该函数开启 `TransactionBehavior::Immediate` 事务，在同一事务中校验 preview、拒绝已有该工具中央档案、插入 profile、建立 baseline、消费 preview 后 commit（`src-tauri/src/db/profiles.rs:136-194`）。这是一套可直接仿照的原子 adoption 结构。
- preview 校验要求数据库中的 `tool/artifact_kind/target_path/observed_full_hash/status='previewed'` 全部不变（`db/profiles.rs:700-732`）；消费使用 `WHERE ... status='previewed'`，重复确认会报“预览已消费”（`db/profiles.rs:821-835`）。
- `adopt_baseline` 按 `(tool, artifact_kind, scope='global', project_id IS NULL, target_path)` 查找目标；无 baseline 的旧行可复用，有任何 baseline 的行直接冲突；写 baseline 也带 `WHERE baseline_full_hash IS NULL AND baseline_managed_hash IS NULL`，不会刷新旧漂移或覆盖既有基线（`db/profiles.rs:755-818`）。
- MCP 中央名是数据库级全局唯一且不区分大小写：`mcp_servers.name ... COLLATE NOCASE UNIQUE`（`src-tauri/src/db/migrations/0001_initial.sql:69-103`）。因此原生配置同名本身不会触发 DB 冲突（文件不是 DB）；但导入中央记录若 DB 已有同名中央项，会在 insert 时冲突（`src-tauri/src/db/mcp.rs:604-620` 的 `mcp_servers.name` 映射）。
- 全局分配键是 `(tool,mcp_id)`，assignment 插入 `INSERT OR IGNORE`，并与项目 assignment 互斥触发器约束；写入前校验 MCP `row_version`，发生实际变更后 touch 版本，整个操作为 Immediate 事务（`src-tauri/src/db/mcp.rs:210-267`；schema `0001_initial.sql:176-230`）。重复确认若复用该 API 本身幂等，但不能把 assignment API 误当作导入原子事务：它不包含中央记录插入、preview 消费或 baseline。
- 现有 MCP sync 的 ownership 由“本次 desired、继承项、已有 managed_items 的 external_key”组成（`src-tauri/src/mcp/service.rs:478-490`，`build_mcp_ownership` 附近）；外部同名条目在扫描时参与冲突判断。已有 managed item 先按 `resource_id`，再按 `external_key` 复用；重复 resource/key 报冲突（`mcp/service.rs:745-794`）。
- MCP managed item 读取接口只读出 `id/resource_id/external_key/last_applied_item_hash/row_version`，且限定 `resource_kind='mcp'`（`src-tauri/src/db/mcp.rs:450-476`）。同步 apply 的 managed item 更新/插入/删除与目标写入在统一 apply 事务中，并校验 item row version、target、外部 key 和 hash（`src-tauri/src/sync/apply.rs:781-890`, `2424-2520`）。
- `managed_targets` identity 有唯一索引；baseline 必须 full/managed hash 成对存在。`managed_items` 要求挂在同 artifact kind target，且 `(target_id, external_key)` 唯一（`0001_initial.sql:292-387`）。

## 推断与窄范围建议（供主代理取舍）

- 可复用的安全骨架是 `profile_import_previews` token + Immediate 事务 + preview stale hash 校验 + “仅空 baseline 才 adoption”守卫。MCP 可新增同等 token/confirm repository，或抽取通用 helper；不应先分别提交 mcp_server、assignment、managed target。
- 确认时应把“用户选中的原生条目”映射为中央 `mcp_servers`，只为选中项写全局 assignment，并只为选中项写 `managed_items(resource_id=中央 id, external_key=原生 name, last_applied_item_hash=该条目 hash)`。未选中条目不进入 ownership/managed_items，因而保留为外部非受管内容，不会被后续 sync 删除或覆盖。
- 目标 baseline 应记录确认时观察到的原生目标 full hash、managed projection/hash（至少包含所选条目的投影语义）；不能把确认当成“刷新旧漂移 baseline”。若目标已有任何完整 baseline，应沿用 `adopt_baseline` 的冲突语义并要求用户走既有同步/修复路径；仅有空 target 行时才填充。
- 同名处理必须在 Immediate 事务内先查中央 `mcp_servers.name COLLATE NOCASE`：同名中央项即使配置相同也不能静默新建；应明确冲突或显式复用该中央 id（复用时仍必须核对 tool assignment、选中项身份和 stale hash）。原生不同工具的同名允许，因为 assignment 的 tool 在键中，但中央 name 本身跨工具唯一，故 DB 仍会冲突。
- 重复确认：先校验 preview status，再在同一事务中插入/查重中央记录、assignment、managed target/items，最后消费 token；任何一步失败整体回滚。并发确认由 Immediate + status 条件和唯一约束兜底。
- stale hash 应同时覆盖 discovery 预览到确认期间的原生文件变化，以及确认时选择条目集合/内容变化；仅比较文件 full hash 不足以证明选中条目未被替换，建议 token 保存规范化发现清单或选择项 hash，并在 confirm 重读后逐项核验。
- “随后预览”应调用既有 MCP `prepare_mcp_sync` 流程：它读取全局 assignments，构造 desired/inherited projection，读取 managed_items，再按 item hash 扫描（`mcp/service.rs:465-504`）。确认阶段若漏写 managed_items，随后预览会把已导入项当未拥有，产生错误冲突或错误 add。

## 未覆盖/存疑

- 本轮未通读 PRD/设计文档，也未核验当前分支上 MCP discovery/confirm DTO 是否已有未命名实现；以上结论基于指定文件与迁移/schema。具体“选择条目”的原生解析结构需主代理继续定位。
