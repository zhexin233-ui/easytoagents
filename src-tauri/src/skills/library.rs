//! Skill 目录的安全复制、稳定 hash 与中央库所有权证明。

use std::{
    cell::Cell,
    collections::BTreeSet,
    ffi::{CStr, CString, OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt},
        },
    },
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    app::AppPaths,
    domain::{ArtifactName, SkillStatus},
    error::AppError,
    skills::limits::{
        MAX_DEPTH, MAX_FILES, MAX_FILE_BYTES, MAX_RELATIVE_PATH_BYTES, MAX_SKILL_MD_BYTES,
        MAX_TOTAL_BYTES,
    },
    sync::hash_json,
};

#[derive(Debug)]
pub(crate) struct PreparedSkillImport {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub central_path: String,
    pub content_hash: String,
    pub frontmatter: Value,
    staging_path: PathBuf,
    finalized: bool,
    directory_identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CentralSkillInspection {
    pub status: SkillStatus,
    pub diagnostic_code: Option<&'static str>,
    pub files: Vec<String>,
    pub skill_md: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AdoptedCentralSkill {
    pub name: String,
    pub content_hash: String,
    pub frontmatter: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillTakeoverEntryKind {
    ExternalSymlink,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillTakeoverInspection {
    pub entry_type: SkillTakeoverEntryKind,
    pub fingerprint: String,
    pub content_hash: String,
    pub resolved: PathBuf,
}

include!("library/core.rs");
include!("library/walk.rs");
include!("library/chain.rs");
#[cfg(test)]
include!("library_tests.rs");
