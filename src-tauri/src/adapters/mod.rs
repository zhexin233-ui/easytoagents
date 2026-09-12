//! Claude 与 Codex 原生格式适配层。
//!
//! 路径和工具环境必须由调用方显式注入。本模块不会读取进程的 `HOME`、
//! `CLAUDE_CONFIG_DIR` 或 `CODEX_HOME`，也不执行任何外部写入。

use std::{
    collections::BTreeMap,
    fs,
    ops::Index,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use specta::Type;
use toml_edit::{Array, DocumentMut, Item, Table, TableLike};

use crate::{
    domain::{ArtifactKind, ProjectRoot, Scope, TargetType, Tool},
    error::AppError,
};

pub mod claude;
pub mod codex;
pub mod cursor;
pub mod opencode;
pub mod zcode;

pub use claude::CLAUDE_RESERVED_ENV_KEYS;

include!("discovery.rs");
include!("document.rs");
#[cfg(test)]
include!("tests.rs");
