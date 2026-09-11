//! Skill 扫描与下载共用的安全限额。

pub(crate) const MAX_FILES: usize = 4_096;
pub(crate) const MAX_DEPTH: usize = 32;
pub(crate) const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_SKILL_MD_BYTES: u64 = 512 * 1024;
pub(crate) const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
pub(crate) const MAX_RELATIVE_PATH_BYTES: usize = 1_024;
