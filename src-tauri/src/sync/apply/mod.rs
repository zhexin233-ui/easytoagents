//! Phase 3 写入内核：按职责拆分的 Preview 消费、快照、原子写入与恢复实现。
//!
//! 这些片段通过 `include!` 保持原有模块命名空间与私有辅助函数可见性，
//! 同时让每个职责文件保持在可审阅的规模内；对外 API 仍由本模块统一提供。

include!("core.rs");
include!("validate.rs");
include!("plan.rs");
include!("fs_ops.rs");
include!("snapshot.rs");
include!("journal.rs");
include!("mutation.rs");
include!("finalize.rs");
include!("restore.rs");

#[cfg(test)]
include!("tests.rs");
