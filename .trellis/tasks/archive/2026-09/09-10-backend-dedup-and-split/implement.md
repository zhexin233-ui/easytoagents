# 实施计划

- [x] 加载后端规范与代码复用思考指南；确认 perf 与 robustness 任务已归档。
- [x] R7-a：所有大文件内联测试外置为 `tests.rs`，纯移动，`cargo test` 用例数不变。
- [x] R1：`Tool::ALL`、`adapter_for` 单例、descriptor 字段化、`ToolAvailability`/`ExplicitEnvironment` 按工具索引、字符串解析统一、`projects/service.rs` 迭代化；绑定差异已核对。
- [x] R3：`TargetDescriptor::builder`，五个 adapter 迁移；`path_text` 上移。
- [x] R2：`sync/managed.rs` 与三个 service 的泛型替换；共享领域投影；绑定核对。
- [x] R4：`ProviderCodec` ownership/default/discover/render 下沉；Profiles 改为 codec 编排。
- [x] R5：`skills::limits`。
- [x] R6：`connection()` 收窄、`db/sync.rs`、`with_immediate_transaction`、`active_writer` 合一。
- [x] R7-b：`sync/apply/` 目录化与 `ApplyContext`；`profiles/service.rs` 拆分；行数校验。
- [x] 更新 `docs/maintainers/adding-tool-adapter.md`、`.trellis/spec/backend/directory-structure.md`、`database-guidelines.md`。
- [x] `pnpm bindings:check`、`pnpm rust:check`、`pnpm check`（前端 32 个测试文件/280 项通过；Rust 318 项通过、2 项忽略；bindings 校验通过）。

进度记录：已完成测试外置、`sync/apply/` 职责拆分、Adapter/descriptor/限额去重、
`allowed_root`/MCP 容器字段化、工具索引映射、`ApplyContext`、三类受管流程泛型化、
ProviderCodec 全量下沉、连接可见性收窄、IMMEDIATE 事务辅助器，以及 sync
run/item/target/snapshot/readopt SQL 下沉到 `db/sync.rs`。保留各 RPC DTO 名称，
共享领域投影仅作为内部 canonical 类型，避免 Specta 类型别名折叠造成前端绑定改名。
资源 CRUD、导入和项目原生观测所需的领域 SQL 仍留在各自 `db/`/service 模块；本轮
`db/sync.rs` 边界针对跨资源同步引擎及其恢复合同，不扩大为所有业务表访问。

## 风险文件
`domain/mod.rs`、`adapters/**`、`sync/**`、`db/**`、`mcp/service.rs`、`skills/service.rs`、`hooks/service.rs`、`profiles/service.rs`、`projects/**`、`overview/mod.rs`、`lib.rs`。

## 回滚点
R7-a、R1+R3、R2+R4+R5、R6、R7-b 五段提交。
