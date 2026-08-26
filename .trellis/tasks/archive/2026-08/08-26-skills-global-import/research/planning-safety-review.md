# 规划安全核对

核对范围：`design.md` §2–4、`implement.md` §1–2，以及 `library.rs`、`db/skills.rs`、`db/mcp_imports.rs` 和 backend quality/database 规范。未修改实现文件，未执行会写用户目录的测试。

## 事实核对

- 现有单目录导入明确先做 `validate_source_root`，随后 `canonical_source_directory`；后者对来源做 `canonicalize`、`symlink_metadata` 并要求最终对象为非链接目录（`src-tauri/src/skills/library.rs:126-134`, `447-459`）。因此设计中“仅对已枚举直属入口放行链接”必须落在新的显式 helper 中，不能直接把入口传给 `prepare_skill_import`，也不能把 `canonicalize` 当作链证据。
- 现有树复制在复制前后以 root identity 重验，内部链接由 `validate_source_symlink` 校验；目录链接、逃逸、断链等被拒绝（`src-tauri/src/skills/library.rs:144-160`, `471-478`, `728-776`）。这与 design §2.3/implement §1 的“不弱化内部 no-follow、复制前后重验”一致。新增入口链解析不得改变这些内部规则。
- `finalize_skill_import` 先 `fs::rename`，再同步 staging/central；rename 后设置 `prepared.finalized = true`，清理据此选择中央目录而非 staging（`src-tauri/src/skills/library.rs:178-201`）。design §4.3 所写“rename 后失败不得按 staging 清理”与现有机制一致。
- 现有 `insert_skill` 自己开启 `TransactionBehavior::Immediate` 并 commit（`src-tauri/src/db/skills.rs:79-121`）。design §4.2/implement §2 要求提取事务内 helper、旧 API 保留包装是必要的；若批量路径循环调用 `insert_skill`，无法满足“全批同一事务”。
- MCP 导入已经展示同类消费合同：Immediate 事务内重读 preview、检查 `applying/restoring/rollback_failed`、写入、末尾重验来源、条件更新 `status='consumed'` 后 commit（`src-tauri/src/db/mcp_imports.rs:155-276`）。Skills 可复用该事务形状，但不能复用其 managed/assignment 写入；design §4.2 第 4–7 步边界正确。
- 规范要求 preview 绑定目标身份及所有参与 row version，JSON 顶层类型/哈希有约束，迁移在 Immediate 事务中应用并可用 tempfile 重开验证（`.trellis/spec/backend/quality-guidelines.md:34-43`, `.trellis/spec/backend/database-guidelines.md:23-45`）。implement §2 的迁移约束、中央快照指纹、隔离 DB/目录测试方向覆盖这些要求。

## 阻塞问题

未发现 design §2–4 或 implement §1–2 明确违反既有安全边界的阻塞项，前提是实现严格执行其文字合同：入口链逐跳身份/no-follow 证据、`.system`（含别名）排除、确认前后重验、单一 Immediate 事务、rename 后按 finalized 状态清理、提交不确定时不盲删。

## 非阻塞但需实现时钉死的细节

1. `finalize_skill_import` 的 rename 与 SQLite commit 本身不是跨资源原子操作；现有代码只能提供“已知失败可按身份清理”的补偿。实现不能把 design §4.2 的“同批一起可见”表述解释为文件系统原子性；§4.3 的崩溃残留说明应作为实际错误语义保留。（非阻塞，设计已有说明。）
2. `skills.source_path` schema 要求绝对、无 `//`、无 `.`/`..`、无尾斜杠（`src-tauri/src/db/migrations/0001_initial.sql:119-123`）。入口原文路径与 canonical 溯源路径应分字段/仅证据 JSON 保存，写入 `source_path` 前必须经过现有 path text 合同；不能把用户展示入口原样塞入该列。（非阻塞，设计已区分展示与真实路径。）
3. 新迁移的 `context_json`/展示 JSON 应复制 `mcp_import_previews` 的 `json_valid` + object CHECK（`src-tauri/src/db/migrations/0005_mcp_import_previews.sql:1-29`），并满足数据库规范的 UUID、时间、lowercase SHA-256 约束；不能只靠 Rust 序列化保证。（非阻塞实现核验项。）
4. 入口链接的“已打开真实目录描述符”若在 Rust API 中无法长期持有，应至少以逐跳 lstat/identity + 打开后再次验证实现等价证据；单次 `canonicalize` 加后续普通路径读取明确不够（quality forbidden pattern 与 `library.rs:447-459` 可核验）。
