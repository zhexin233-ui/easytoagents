# 后端实现交接

## 实现范围

- 新增 `skills/import.rs`：显式 Claude 全局来源、Codex 正式与兼容来源；逐来源结果、直属入口与入口链接证据、内置与别名排除、同内容归并、中央重复/同名冲突分类。
- `skills/library.rs` 提取共享只读树扫描与累计读取预算；全路径祖先使用 no-follow 目录描述符，链接最多 32 跳；相对目标的 `..` 不能跨过未经验证的目录。普通本地导入仍拒绝根链接。
- 确认绑定来源目录身份与完整树哈希，最多选择 32 项；仅选择项产生 staging。单个 IMMEDIATE 事务重验、finalize、全部插入、消费令牌。拒绝现有同步 writer。
- finalize 使用原子不覆盖 rename：macOS `RENAME_EXCL`，Linux `RENAME_NOREPLACE`。清理验证本操作目录身份和完整哈希；目录被替换或内容变化时保留。
- 提交返回不确定结果时，重读令牌与完整批次记录并核验中央副本；无法判定则保留目录，不盲删。
- 新增 `db/skill_imports.rs` 与 `0006_skill_import_previews.sql`；提取 `insert_skill_in_transaction`，原单项 API 保留事务包装。
- `skills/service.rs` 只在空双基线、无 existing/desired、成功目录扫描且通用状态为 external_non_owned_change 时产生两种首次诊断；通用漂移枚举和算法不改。
- 新增 Rust DTO、commands 注册，并唯一生成 `src/bindings/commands.ts`。字段与派发约定完全一致。
- 附带更新 `app/mod.rs` 的 schema version 相关测试断言为 6；除此之外未改该模块。

## 文件

- `src-tauri/src/skills/{import,library,models,mod,service}.rs`
- `src-tauri/src/db/{skill_imports,skills,mod}.rs`
- `src-tauri/src/db/migrations/0006_skill_import_previews.sql`
- `src-tauri/src/commands/skills.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/app/mod.rs`（仅版本断言）
- `src/bindings/commands.ts`（生成）

没有修改手写前端、全局规范、任务状态或规划正文，没有提交/推送，也没有访问真实全局 Skills 做导入或 Apply。

## 验证结果

- `cargo test --manifest-path src-tauri/Cargo.toml --quiet`：最终 185 个单元测试、3 个集成测试全部通过。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`：通过。
- `cargo fmt --check --manifest-path src-tauri/Cargo.toml`：通过；最后新增的排他 rename 测试也已运行 cargo fmt。
- `pnpm bindings:generate`：通过；绑定随后没有手写修改。
- `pnpm bindings:check`：通过；最后全量 Rust 运行也再次执行并通过生成绑定测试。
- `pnpm lint`、`pnpm typecheck`：通过，运行时前端并发实现已存在。
- `git diff --check`：通过。

新增实质回归覆盖：仅存在 `.codex`、外部绝对/相对目录链接、同目录与同内容归并、自定义 Codex 根、`.system` 及其目录/链接别名、断链/循环/内部逃逸/私有来源/广泛来源、中央复用与 NOCASE 冲突、来源正文/入口变化、中央版本变化、选择无效/重复/空集合、writer、两个独立连接竞争、copy/rename/SQL/commit 故障补偿、不确定提交/回滚、复制中来源变化、源文件与权限和同步元数据不变、读取预算与候选上限、替换目录清理保护、排他 rename、v5→v6 与重复打开、首次状态/完整及半基线/desired/中央损坏。

## 独立复核重点与边界

- 建议 reviewer 重点检查 `skills/import.rs` 的确认事务及 `library.rs` 的来源链/清理边界，而不是再次扩大产品范围。
- 未执行真实桌面或真实安装目录验证；浏览器/桌面检查由主代理协调，不能把临时 fixture 测试称为真实宿主验证。
- SQLite 与文件系统无法跨资源原子提交。进程在 finalize 与 DB commit 之间崩溃仍可能留下私有无记录副本；本实现不自动猜测清理它们。
- 提交不确定分支用边界故障注入覆盖，未制造实际磁盘故障；Linux 专用 rename 分支未在当前 macOS 宿主执行。
- 128 MiB 是累计读取/复制预算，复制及多次重验共享预算，因此可确认的原始内容大小会低于 128 MiB。预算测试以受限 Cell 和候选上限验证拒绝路径，没有构造实际 128 MiB 的完整扫描 fixture。
