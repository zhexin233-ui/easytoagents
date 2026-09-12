//! 原生全局 Agents 的只读发现与显式导入。
//!
//! 导入沿用 Hooks 简化模式：只读发现 → 用户显式选择 → 仅创建中央记录，
//! 不做基线接管；之后通过常规分配 + 预览 / Apply 进入受管（目标文件内容
//! 与投影一致时 Apply 近似 no-op）。发现只扫描目录**直属**文件：符号链接、
//! 子目录与非目标扩展名一律跳过并计数，不递归。

use std::{collections::BTreeSet, fs, path::Path};

use super::service::{
    agent_directory_descriptor, create_agent, parse_codex_agent_file, parse_markdown_agent_file,
    AGENT_FIELD_INVALID, AGENT_FILE_TOO_LARGE, AGENT_FRONTMATTER_INVALID, AGENT_NAME_CONFLICT,
    AGENT_NAME_INVALID, AGENT_REQUIRED_FIELD_MISSING,
};
use super::{
    AgentImportCandidateDto, AgentImportPreviewDto, AgentImportResultDto, ConfirmAgentImportInput,
    DiscoverAgentImportInput,
};
use crate::{
    adapters::{agent_file_extension, ExplicitEnvironment, PolicyState},
    db::{agents as repository, Database},
    domain::{AgentName, Tool},
    error::AppError,
};

/// 单个 agent 源文件的大小上限（与 hooks 脚本接管同一量级）。
const MAX_AGENT_FILE_BYTES: u64 = 512 * 1024;

pub fn discover_agent_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: &DiscoverAgentImportInput,
) -> Result<AgentImportPreviewDto, AppError> {
    let tool = input.tool;
    let descriptor = agent_directory_descriptor(environment, tool, None)?;
    let directory_path = descriptor
        .path
        .clone()
        .ok_or_else(|| AppError::not_found("agentTarget", tool.as_str()))?;
    ensure_readable(&descriptor, &directory_path)?;
    let extension = agent_file_extension(tool);

    // The configured agents directory is itself an input boundary.  Do not let
    // `read_dir` follow a user-created symlink and expose files outside the
    // adapter's allowed root; regular child symlinks are filtered below too.
    match fs::symlink_metadata(&directory_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(AppError::conflict(
                "agentTarget",
                "Agents 目录不能是符号链接",
            ));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(AppError::conflict("agentTarget", "Agents 目标必须是目录"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AgentImportPreviewDto {
                tool,
                directory_path: directory_path.clone(),
                candidates: Vec::new(),
                message: Some("该工具的全局 agents 目录尚不存在。".to_owned()),
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(
                AppError::permission(&directory_path, "lstat_agents_directory").with_source(error),
            );
        }
        Err(error) => {
            return Err(
                AppError::io_from(&directory_path, "lstat_agents_directory", &error)
                    .with_source(error),
            );
        }
    }

    let entries = match fs::read_dir(&directory_path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AgentImportPreviewDto {
                tool,
                directory_path: directory_path.clone(),
                candidates: Vec::new(),
                message: Some("该工具的全局 agents 目录尚不存在。".to_owned()),
            });
        }
        Err(error) => {
            return Err(
                AppError::io_from(&directory_path, "read_agents_directory", &error)
                    .with_source(error),
            );
        }
    };

    // 中央库名称集合：候选与库内同名（NOCASE）即标记冲突。
    let central_names = repository::list_agents(database)?
        .into_iter()
        .map(|record| record.name)
        .collect::<BTreeSet<_>>();
    let mut used_names: BTreeSet<String> = central_names
        .iter()
        .map(|name| name.to_lowercase())
        .collect();

    let mut sources = Vec::new();
    let mut skipped = 0usize;
    for entry in entries {
        let entry = entry.map_err(|error| {
            AppError::io_from(&directory_path, "read_agents_directory_entry", &error)
                .with_source(error)
        })?;
        let path = entry.path();
        // no-follow：符号链接与特殊文件一律跳过；只取目录直属的普通文件。
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if metadata.is_dir() || !metadata.is_file() {
            skipped += 1;
            continue;
        }
        let extension_matches = path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|candidate| candidate == extension);
        if !extension_matches {
            skipped += 1;
            continue;
        }
        sources.push(path);
    }
    sources.sort();

    let mut candidates = Vec::new();
    for path in sources {
        let source_path = path.to_string_lossy().into_owned();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            AppError::io_from(&source_path, "lstat_agent_source", &error).with_source(error)
        })?;
        let file_name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
            .to_owned();
        let stem = Path::new(&file_name)
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
            .to_owned();

        let mut candidate = AgentImportCandidateDto {
            candidate_id: uuid::Uuid::new_v4().to_string(),
            source_path: source_path.clone(),
            name: stem.clone(),
            description: String::new(),
            prompt: String::new(),
            dropped_fields: Vec::new(),
            importable: false,
            diagnostic_code: None,
            reason: None,
        };

        if metadata.len() > MAX_AGENT_FILE_BYTES {
            candidate.diagnostic_code = Some(AGENT_FILE_TOO_LARGE.to_owned());
            candidate.reason = Some("文件超过 512 KiB，不能导入。".to_owned());
            candidates.push(candidate);
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                candidate.diagnostic_code = Some(AGENT_FRONTMATTER_INVALID.to_owned());
                candidate.reason = Some("文件不可读取。".to_owned());
                candidates.push(candidate);
                continue;
            }
        };
        let Ok(text) = String::from_utf8(bytes) else {
            candidate.diagnostic_code = Some(AGENT_FRONTMATTER_INVALID.to_owned());
            candidate.reason = Some("文件不是有效的 UTF-8 文本。".to_owned());
            candidates.push(candidate);
            continue;
        };

        let parsed = if tool == Tool::Codex {
            parse_codex_agent_file(&text)
        } else {
            parse_markdown_agent_file(&text)
        };
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(code) => {
                candidate.diagnostic_code = Some(code.to_owned());
                candidate.reason = Some("文件格式无法解析（frontmatter / TOML 无效）。".to_owned());
                candidates.push(candidate);
                continue;
            }
        };
        candidate.dropped_fields = parsed.dropped_fields.clone();
        candidate.prompt = parsed.prompt.clone();
        candidate.description = parsed.description.clone().unwrap_or_default();
        // Markdown 系 name 缺省取文件名去扩展名（OpenCode 以文件名为名）。
        let name = parsed.name.clone().unwrap_or(stem);
        candidate.name = name.clone();

        if let Err(code) = validate_candidate(&name, parsed.description.as_deref(), &parsed.prompt)
        {
            candidate.diagnostic_code = Some(code.to_owned());
            candidate.reason = Some(required_field_reason(code, &name));
            candidates.push(candidate);
            continue;
        }

        // 与中央库或同轮候选同名（NOCASE）→ 冲突，保持不可导入。
        if !used_names.insert(name.to_lowercase()) {
            candidate.diagnostic_code = Some(AGENT_NAME_CONFLICT.to_owned());
            candidate.reason = Some("中央库已存在同名 Agent（不区分大小写）。".to_owned());
            candidates.push(candidate);
            continue;
        }
        candidate.importable = true;
        candidates.push(candidate);
    }

    let message = if candidates.is_empty() {
        Some("该目录中没有可导入的子代理文件。".to_owned())
    } else if skipped > 0 {
        Some(format!(
            "已跳过 {skipped} 个子目录、符号链接或非 .{extension} 文件。"
        ))
    } else {
        None
    };
    Ok(AgentImportPreviewDto {
        tool,
        directory_path,
        candidates,
        message,
    })
}

pub fn confirm_agent_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: &ConfirmAgentImportInput,
) -> Result<AgentImportResultDto, AppError> {
    if input.agents.is_empty() {
        return Err(AppError::invalid_input("agents", "请选择要导入的 Agent"));
    }
    // 目标可用性（capability/policy）按工具入口校验；定义复用中央 create
    // 校验（含名称唯一性 → 名称冲突报 CONFLICT）。
    let descriptor = agent_directory_descriptor(environment, input.tool, None)?;
    ensure_readable(&descriptor, descriptor.path.as_deref().unwrap_or_default())?;
    let mut created = 0u32;
    for agent in &input.agents {
        create_agent(database, agent)?;
        created += 1;
    }
    Ok(AgentImportResultDto {
        tool: input.tool,
        created_count: created,
    })
}

/// 候选必填字段校验：返回稳定诊断码。
fn validate_candidate(
    name: &str,
    description: Option<&str>,
    prompt: &str,
) -> Result<(), &'static str> {
    if AgentName::parse(name.to_owned()).is_err() {
        return Err(AGENT_NAME_INVALID);
    }
    let Some(description) = description.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(AGENT_REQUIRED_FIELD_MISSING);
    };
    if description.len() > 1000 || description.contains('\0') {
        return Err(AGENT_FIELD_INVALID);
    }
    if prompt.trim().is_empty() {
        return Err(AGENT_REQUIRED_FIELD_MISSING);
    }
    if prompt.len() > 65536 || prompt.contains('\0') {
        return Err(AGENT_FIELD_INVALID);
    }
    Ok(())
}

fn required_field_reason(code: &str, _name: &str) -> String {
    match code {
        AGENT_NAME_INVALID => "名称不符合五工具交集规则（小写字母、数字、连字符）。".to_owned(),
        AGENT_REQUIRED_FIELD_MISSING => "缺少必填字段（description 或正文）。".to_owned(),
        AGENT_FIELD_INVALID => "字段超出长度限制（description ≤1000、正文 ≤65536）。".to_owned(),
        _ => "候选不满足导入条件。".to_owned(),
    }
}

fn ensure_readable(
    descriptor: &crate::adapters::TargetDescriptor,
    path: &str,
) -> Result<(), AppError> {
    if descriptor.policy != PolicyState::Allowed {
        return Err(AppError::policy_blocked(
            descriptor.tool.as_str(),
            path,
            if descriptor.policy == PolicyState::Unknown {
                "CLAUDE_POLICY_UNKNOWN"
            } else {
                "CLAUDE_POLICY_BLOCKED"
            },
        ));
    }
    Ok(())
}
