# MCP 全局导入后端核验

## P1：确认阶段没有在原子事务内重新核验源文件版本（TOCTOU）

**事实**：`confirm_mcp_import` 在事务外调用 `read_native`，并把该次读取得到的 `native.full_hash` 与预览哈希比较（`src-tauri/src/mcp/import.rs:231-238`）；随后同一份 `native` 被解析并传给 `adopt_import`（`src-tauri/src/mcp/import.rs:240-280`）。`adopt_import` 开始 `TransactionBehavior::Immediate` 事务后只重新查询预览记录、中央 DB 指纹和 writer 状态（`src-tauri/src/db/mcp_imports.rs:154-184`），没有重新读取/哈希 `preview.target_path`。

**可复现时序**：预览完成后调用 confirm；在 `read_native` 返回、`adopt_import` 提交之间替换源 MCP 文件（例如另一个进程写入新配置）。导入仍会把第一次读取的旧 `native` 内容写入中央记录，并以预览时的 `observed_full_hash` 更新 `managed_targets`（`mcp_imports.rs:192-214`）。因此源文件已过期时不会阻止提交，且基线声明的 full hash 与提交时源文件不一致。

**最小修复建议**：在持有 Immediate 事务期间重新读取/计算 `target_path` 的源版本并与 `preview.observed_full_hash` 比较，再允许插入记录、assignment、managed item 和消费 token；同时将待导入条目从该次重新读取结果解析，避免仅校验旧内存快照。若无法在事务中安全读取文件，至少在提交前再次读取并在数据库事务中增加不可绕过的版本核验/失败回滚路径。

## 已核验且未发现的问题

- 中央记录、global assignment、managed target、managed item、preview 消费均在 `adopt_import` 的同一 Immediate 事务内（`src-tauri/src/db/mcp_imports.rs:154-257`）；没有调用会另开连接/事务的 `mcp` 写入接口，`insert_mcp_configuration` 接收当前事务连接（`mcp_imports.rs:229-233`）。
- 消费 token 由 UUID 格式约束（`src-tauri/src/db/migrations/0005_mcp_import_previews.sql:2-12`），并以 `status='previewed'` 条件更新且要求影响 1 行（`mcp_imports.rs:251-257`）；confirm 的候选 ID 还要求属于 DB 中保存的 evidence 且无重复（`import.rs:220-229`）。
- 已管理条目会逐条按 `last_applied_item_hash` 核验，并要求 managed projection hash 与基线一致（`import.rs:302-314`）；选中项并入 projection 后更新 baseline，未选中项不会被写入/删除（`import.rs:244-272`）。
- DB 指纹覆盖全部 `mcp_servers` 的 id/row_version、两类 assignment、目标及 managed item 的 id/row_version（`mcp_imports.rs:88-122`），事务内再次比较 expected state（`mcp_imports.rs:167-173`），并在事务内阻止 `applying`、`restoring`、`rollback_failed` writer（`mcp_imports.rs:175-184`）。

未将“指纹未包含配置正文/managed hash”等推测列为漏洞：现有 mcp/managed 表的正常 UPDATE 由 row_version bump trigger 保护（`src-tauri/src/db/migrations/0001_initial.sql:587-645`），且本次写路径使用同一事务连接。
