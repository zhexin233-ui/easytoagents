//! MCP 中央意图、分配与原生同步纵向功能。

mod import;
mod models;
mod service;

pub use import::{confirm_mcp_import, discover_mcp_import};
pub use models::*;
pub(crate) use service::register_native_projection_secrets;
pub use service::*;
