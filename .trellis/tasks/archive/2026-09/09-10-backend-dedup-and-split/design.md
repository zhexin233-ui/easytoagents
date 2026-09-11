# 技术设计

## 顺序
1. 先做测试外置与文件拆分（R7）中"纯移动"的部分，保证后续 diff 可读。
2. 再做 Tool 分派集中（R1）与 descriptor builder（R3），这两项被其余项依赖。
3. 然后 `ManagedArtifact`（R2）、`ProviderCodec`（R4）、limits（R5）、SQL 下沉（R6）。
4. 最后按职责拆 `sync/apply` 与 `profiles/service`。

## Tool 分派
`domain/mod.rs`：`impl Tool { pub const ALL: [Tool; 5] = [...]; }`。`adapters/mod.rs`：`pub fn adapter_for(tool: Tool) -> &'static dyn ToolAdapter` 用 `static` 单例；`Tool::adapter()` 委托。`TargetDescriptor` 新增 `allowed_root: PathBuf`、`mcp_container: Option<JsonPointer>`（名称按现有术语调整），由各 adapter `discover` 填写；service 层改为读字段。`ToolAvailability` 改为 `BTreeMap<Tool, ToolAvailabilityState>` 或 `[ToolAvailabilityState; 5]` 加 `impl Index<Tool>`；specta 导出保持为对象形状（用自定义 `Serialize` 或保留 DTO 结构体仅在 RPC 边界转换）。

## ManagedArtifact
```rust
pub(crate) trait ManagedArtifact {
    const KIND: ArtifactKind;
    const ENTITY: DatabaseEntityType;
    type Record;
    fn get_record(db: &Database, id: &str) -> Result<Option<Self::Record>, AppError>;
    fn row_version(record: &Self::Record) -> i64;
    fn current_item_hash(observed: &ObservedTarget, item: &ManagedItem) -> Option<String>;
    fn global_tools(db: &Database, id: &str) -> Result<Vec<Tool>, AppError>;
}
```
`sync/managed.rs` 提供 `readopt_with_scan::<A>`、`collect_row_versions::<A>`、`list_global_target_statuses::<A>`、`descriptor_for::<A>`、`project_dto::<A>`。`mcp/service.rs` 等保留薄包装函数以维持命令层调用签名。共享 DTO 放 `domain`，各模块 `pub type McpProjectDto = domain::ManagedProjectDto;` 并在 specta 注册处保留原名导出（核对 `lib.rs` 的 `.typ::<>()` 列表；顺带删除其中被命令签名已引用而冗余的注册项）。

## ProviderCodec
```rust
pub trait ProviderCodec { fn ownership(&self) -> ...; fn default_options(&self) -> ...; fn discover(&self, ...) -> ...; fn render(&self, ...) -> ...; }
```
Claude/Codex/ZCode/OpenCode 实现，Cursor 返回 `None`。`profiles/service.rs` 9 个 `match tool` 改为 `tool.adapter().provider_codec().ok_or_else(unsupported)`。

## SQL 下沉
`db/sync.rs` 承接 `sync/apply.rs` 与 `sync/mod.rs` 的 58+13 条 SQL；`Database::with_immediate_transaction`。`connection()` 改 `pub(crate)` 后，编译错误清单即为剩余越层点，逐一下沉或保留在 `db/` 子模块。

## 拆分
`sync/apply/` 目录布局按 PRD；`ApplyContext { fault_injector, journal, paths, redactor }`。所有 `pub` 项在 `sync/apply/mod.rs` re-export，外部路径不变。

## 风险
- `TargetDescriptor` 增字段影响 specta 导出与前端类型，前端只读不写，预期为新增可选字段。
- Claude `allowed_root` 口径统一可能改变某些路径校验结果，需要用现有 mcp 测试确认。

## 回滚
按顺序四段提交；每段独立可 revert，后段依赖前段。
