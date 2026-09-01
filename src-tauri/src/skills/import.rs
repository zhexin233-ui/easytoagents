//! 显式发现用户 Skills 并批量复制到中央库；原安装与同步元数据保持不变。

use std::{
    cell::Cell,
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use rusqlite::{OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{
    library::{self, PreparedSkillImport, SkillSourceEvidence, SkillTakeoverEntryKind},
    service::{self, descriptor_for},
    ConfirmSkillImportInput, PrepareSkillTakeoverInput, PreparedSkillRecord,
    SkillImportCandidateDto, SkillImportCandidateStatus as CandidateStatus, SkillImportPreviewDto,
    SkillImportResultDto, SkillImportSourceDto, SkillImportSourceKind as SourceKind,
    SkillImportSourceStatus as SourceStatus, SkillTakeoverPreviewResultDto,
};
use crate::{
    adapters::{CapabilityState, ExplicitEnvironment, PolicyState, TargetDescriptor},
    app::AppPaths,
    db::{skill_imports as repository, skills, Database},
    domain::{SkillStatus, Tool},
    error::{AppError, ErrorCode},
    security::SecretRedactor,
    sync::{hash_json, SkillTakeoverEntry, SkillTakeoverEntryType},
};

const MAX_CANDIDATE_ENTRIES: usize = 256;
const MAX_SELECTED: usize = 32;
const MAX_READ_BYTES: u64 = 128 * 1024 * 1024;
const CONTEXT_VERSION: u32 = 2;

#[derive(Deserialize, Serialize)]
struct ImportContext {
    version: u32,
    environment: String,
    central_state: String,
    candidates: Vec<CandidateEvidence>,
}

#[derive(Deserialize, Serialize)]
struct CandidateEvidence {
    id: String,
    name: String,
    hash: String,
    sources: Vec<SkillSourceEvidence>,
    existing_skill_id: Option<String>,
    takeover_source: Option<SkillSourceEvidence>,
    takeover_entry_type: Option<SkillTakeoverEntryType>,
    takeover_fingerprint: Option<String>,
    central_path: Option<String>,
}

fn is_managed_source(kind: SourceKind) -> bool {
    matches!(
        kind,
        SourceKind::ClaudeGlobal | SourceKind::CodexHome | SourceKind::CursorHome
    )
}

fn source_roots(environment: &ExplicitEnvironment, tool: Tool) -> Vec<(SourceKind, PathBuf)> {
    match tool {
        Tool::Claude => vec![(
            SourceKind::ClaudeGlobal,
            environment.claude_config_dir().join("skills"),
        )],
        Tool::Codex => vec![
            (
                // 官方目录，正式同步目标，优先展示。
                SourceKind::CodexHome,
                environment.codex_home().join("skills"),
            ),
            (
                // 跨工具通用目录，仅作为导入来源，不再是同步目标。
                SourceKind::CodexAgents,
                environment.home().join(".agents/skills"),
            ),
        ],
        Tool::Cursor => vec![
            (
                SourceKind::CursorHome,
                environment.home().join(".cursor/skills"),
            ),
            (
                SourceKind::CursorAgents,
                environment.home().join(".agents/skills"),
            ),
        ],
    }
}

fn is_broad_source(
    environment: &ExplicitEnvironment,
    paths: &AppPaths,
    tool: Tool,
    source: &Path,
) -> bool {
    [
        environment.home(),
        environment.claude_config_dir(),
        environment.codex_home(),
        paths.data_root(),
    ]
    .iter()
    .any(|root| root.starts_with(source))
        || source_roots(environment, tool)
            .iter()
            .any(|(_, root)| root.starts_with(source))
}

fn builtin_exclusions(environment: &ExplicitEnvironment) -> Vec<PathBuf> {
    let mut excluded = Vec::new();
    // 内置归属不随检测工具改变；Claude 入口也可能指向 Codex 内置树。
    for (_, root) in source_roots(environment, Tool::Codex) {
        let builtin = root.join(".system");
        excluded.push(builtin.clone());
        // 这里只解析目录身份，不读取集合或技能正文。
        if let Ok(evidence) = library::resolve_skill_source(&root, &builtin) {
            excluded.push(evidence.resolved);
        }
    }
    excluded
}

fn environment_fingerprint(
    environment: &ExplicitEnvironment,
    tool: Tool,
    descriptor: &TargetDescriptor,
) -> String {
    hash_json(
        &json!({"roots": source_roots(environment, tool), "builtinRoots": source_roots(environment, Tool::Codex), "descriptor": descriptor, "version": environment.installation_version(tool)}),
    )
}

pub fn discover_skill_import(
    database: &Database,
    paths: &AppPaths,
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<SkillImportPreviewDto, AppError> {
    let descriptor = descriptor_for(
        environment,
        tool,
        None,
        environment.claude_customization_policy_probe(),
    )?;
    let roots = source_roots(environment, tool);
    let allowed = descriptor.capability.state == CapabilityState::Supported
        && descriptor.policy == PolicyState::Allowed;
    let mut preview = SkillImportPreviewDto {
        preview_id: None, tool, sources: Vec::new(), candidates: Vec::new(),
        message: Some("仅复制所选用户技能到中央库；原安装不变，不自动分配或同步。中央副本不会随来源自动更新。".to_owned()),
    };
    let records = skills::list_skills(database)?;
    let mut context = ImportContext {
        version: CONTEXT_VERSION,
        environment: environment_fingerprint(environment, tool, &descriptor),
        central_state: repository::state_fingerprint(database.connection())?,
        candidates: Vec::new(),
    };
    let budget = Cell::new(MAX_READ_BYTES);
    let mut entry_count = 0usize;
    let excluded_roots = if allowed {
        builtin_exclusions(environment)
    } else {
        Vec::new()
    };
    for (kind, root) in roots {
        let mut source = SkillImportSourceDto {
            kind,
            path: root.to_string_lossy().into_owned(),
            status: SourceStatus::Ready,
            diagnostic_code: None,
            message: None,
        };
        if !allowed {
            source.status = SourceStatus::Unavailable;
            source.diagnostic_code = Some(
                if descriptor.capability.state != CapabilityState::Supported {
                    "SKILL_IMPORT_TOOL_UNAVAILABLE"
                } else {
                    "SKILL_IMPORT_POLICY_BLOCKED"
                }
                .to_owned(),
            );
            source.message = Some("工具不可用或策略未允许读取用户技能".to_owned());
            preview.sources.push(source);
            continue;
        }
        let entries = match library::enumerate_skill_entries(&root) {
            Ok(entries) => entries,
            Err(error) => {
                source.status = if error.code() == ErrorCode::NotFound {
                    SourceStatus::Missing
                } else {
                    SourceStatus::Unavailable
                };
                source.diagnostic_code = Some(
                    if source.status == SourceStatus::Missing {
                        "SKILL_IMPORT_SOURCE_MISSING"
                    } else {
                        "SKILL_IMPORT_SOURCE_UNAVAILABLE"
                    }
                    .to_owned(),
                );
                source.message = Some(
                    if source.status == SourceStatus::Missing {
                        "来源目录不存在"
                    } else {
                        "来源无法安全读取，可能存在权限、路径类型或条目数量问题"
                    }
                    .to_owned(),
                );
                preview.sources.push(source);
                continue;
            }
        };
        let mut user_entries = 0usize;
        let mut excluded = false;
        for entry in entries {
            if tool == Tool::Codex && entry.file_name().is_some_and(|name| name == ".system") {
                excluded = true;
                continue;
            }
            entry_count += 1;
            if entry_count > MAX_CANDIDATE_ENTRIES || budget.get() == 0 {
                source.status = SourceStatus::Unavailable;
                source.diagnostic_code = Some("SKILL_IMPORT_SCAN_LIMIT".to_owned());
                source.message = Some(
                    "检测达到候选数量或 128 MiB 读取上限，结果不完整；请减少来源内容后重新检测"
                        .to_owned(),
                );
                break;
            }
            let evidence =
                match library::resolve_skill_source_excluding(&root, &entry, &excluded_roots) {
                    Ok(evidence) => evidence,
                    Err(error) => {
                        if error
                            .details()
                            .and_then(|details| details.get("field"))
                            .and_then(|value| value.as_str())
                            == Some("builtin")
                        {
                            excluded = true;
                            continue;
                        }
                        user_entries += 1;
                        preview
                            .candidates
                            .push(invalid_candidate(&entry, "来源链接、目录身份或权限无效"));
                        continue;
                    }
                };
            if excluded_roots
                .iter()
                .any(|excluded| evidence.resolved.starts_with(excluded))
            {
                excluded = true;
                continue;
            }
            user_entries += 1;
            if is_broad_source(environment, paths, tool, &evidence.resolved) {
                preview.candidates.push(invalid_candidate(
                    &entry,
                    "来源不能是主目录、配置根或其祖先",
                ));
                continue;
            }
            // 私有目录只允许已知中央记录；未知私有目录不得读取或重新复制。
            let private_record = if evidence.resolved.starts_with(paths.data_root()) {
                match records
                    .iter()
                    .find(|record| Path::new(&record.central_path) == evidence.resolved)
                {
                    Some(record) if central_record_in_private_root(paths, record) => Some(record),
                    _ => {
                        preview
                            .candidates
                            .push(invalid_candidate(&entry, "不能读取未知应用私有目录"));
                        continue;
                    }
                }
            } else {
                None
            };
            let inspection = match library::inspect_skill_source(&evidence, &budget) {
                Ok(inspection) => inspection,
                Err(error) => {
                    preview.candidates.push(invalid_candidate(
                        &entry,
                        "技能内容、链接或资源限制不满足安全校验",
                    ));
                    if error
                        .details()
                        .and_then(|details| details.get("field"))
                        .and_then(|value| value.as_str())
                        == Some("budget")
                    {
                        budget.set(0);
                        source.status = SourceStatus::Unavailable;
                        source.diagnostic_code = Some("SKILL_IMPORT_SCAN_LIMIT".to_owned());
                        source.message = Some("检测达到 128 MiB 读取上限，结果不完整".to_owned());
                        break;
                    }
                    continue;
                }
            };
            if let Some(index) = context.candidates.iter().position(|candidate| {
                candidate.name == inspection.name && candidate.hash == inspection.hash
            }) {
                let candidate = &mut context.candidates[index];
                if !candidate.sources.contains(&evidence) {
                    candidate.sources.push(evidence);
                }
                if let Some(display) = preview
                    .candidates
                    .iter_mut()
                    .find(|display| display.candidate_id == candidate.id)
                {
                    let entry = entry.to_string_lossy().into_owned();
                    if !display.source_paths.contains(&entry) {
                        display.source_paths.push(entry);
                    }
                }
                continue;
            }
            let id = Uuid::new_v4().to_string();
            let mut candidate = SkillImportCandidateDto {
                candidate_id: id.clone(),
                name: inspection.name.clone(),
                description: inspection.description,
                source_paths: vec![entry.to_string_lossy().into_owned()],
                status: CandidateStatus::Importable,
                reason: None,
                existing_skill_id: None,
                takeover_eligible: false,
                takeover_entry_type: None,
            };
            let mut takeover_source = None;
            let mut takeover_entry_type = None;
            let mut takeover_fingerprint = None;
            let mut central_path = None;
            if let Some(record) = records
                .iter()
                .find(|record| record.name.eq_ignore_ascii_case(&candidate.name))
            {
                let central_valid = if record.name == candidate.name
                    && record.content_hash == inspection.hash
                    && record.status == SkillStatus::Ready
                {
                    if let Ok(central) = library::resolve_skill_source(
                        paths.central_skills(),
                        Path::new(&record.central_path),
                    ) {
                        match library::inspect_skill_source(&central, &budget) {
                            Ok(central) => {
                                central.hash == record.content_hash && central.name == record.name
                            }
                            Err(error) => {
                                if error
                                    .details()
                                    .and_then(|details| details.get("field"))
                                    .and_then(|value| value.as_str())
                                    == Some("budget")
                                {
                                    budget.set(0);
                                    source.status = SourceStatus::Unavailable;
                                    source.diagnostic_code =
                                        Some("SKILL_IMPORT_SCAN_LIMIT".to_owned());
                                    source.message =
                                        Some("中央副本核验达到读取上限，检测结果不完整".to_owned());
                                }
                                false
                            }
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                if central_valid {
                    candidate.status = CandidateStatus::AlreadyImported;
                    candidate.reason =
                        Some("同名且完整内容一致，已在中央库；不新增副本或分配".to_owned());
                    candidate.existing_skill_id = Some(record.id.clone());
                    if is_managed_source(kind)
                        && descriptor.path.as_deref() == root.to_str()
                        && !evidence.resolved.starts_with(paths.data_root())
                    {
                        if let Ok(takeover) = library::inspect_skill_takeover_entry(&entry) {
                            if takeover.content_hash == inspection.hash {
                                let entry_type = match takeover.entry_type {
                                    SkillTakeoverEntryKind::ExternalSymlink => {
                                        SkillTakeoverEntryType::ExternalSymlink
                                    }
                                    SkillTakeoverEntryKind::Directory => {
                                        SkillTakeoverEntryType::Directory
                                    }
                                };
                                candidate.takeover_eligible = true;
                                candidate.takeover_entry_type = Some(entry_type);
                                candidate.reason = Some(
                                    "中央库已有完全一致内容；可显式预览接管当前工具入口".to_owned(),
                                );
                                takeover_source = Some(evidence.clone());
                                takeover_entry_type = Some(entry_type);
                                takeover_fingerprint = Some(takeover.fingerprint);
                                central_path = Some(record.central_path.clone());
                            }
                        }
                    }
                } else {
                    candidate.status = CandidateStatus::NameConflict;
                    candidate.reason =
                        Some("中央库存在同名技能，但名称大小写、完整内容或中央状态不同".to_owned());
                }
            } else if private_record.is_some() {
                candidate.status = CandidateStatus::Invalid;
                candidate.reason = Some("中央目录身份与记录不一致".to_owned());
            }
            context.candidates.push(CandidateEvidence {
                id,
                name: inspection.name,
                hash: inspection.hash,
                sources: vec![evidence],
                existing_skill_id: candidate.existing_skill_id.clone(),
                takeover_source,
                takeover_entry_type,
                takeover_fingerprint,
                central_path,
            });
            preview.candidates.push(candidate);
        }
        if source.status == SourceStatus::Ready && user_entries == 0 {
            source.status = SourceStatus::Empty;
        }
        if excluded && source.diagnostic_code.is_none() {
            source.diagnostic_code = Some("SKILL_IMPORT_BUILTIN_EXCLUDED".to_owned());
            source.message = Some("内置技能不在本次导入范围；未读取内置技能正文".to_owned());
        }
        preview.sources.push(source);
    }
    // 同名不同树不得由遍历顺序决定胜者；所有碰撞项都不可确认。
    let conflicts: BTreeSet<String> = context
        .candidates
        .iter()
        .filter(|candidate| {
            context.candidates.iter().any(|other| {
                candidate.id != other.id && candidate.name.eq_ignore_ascii_case(&other.name)
            })
        })
        .map(|candidate| candidate.id.clone())
        .collect();
    for candidate in &mut preview.candidates {
        if conflicts.contains(&candidate.candidate_id) {
            candidate.status = CandidateStatus::NameConflict;
            candidate.reason =
                Some("多个来源存在同名但内容不同的技能，请先处理来源冲突".to_owned());
            candidate.existing_skill_id = None;
        }
    }
    context.candidates.retain(|candidate| {
        preview.candidates.iter().any(|display| {
            display.candidate_id == candidate.id
                && (display.status == CandidateStatus::Importable || display.takeover_eligible)
        })
    });
    if !context.candidates.is_empty() {
        // 只持久化可确认项；发现阶段始终不创建 staging 或中央副本。
        let id = Uuid::new_v4().to_string();
        preview.preview_id = Some(id.clone());
        repository::persist_preview(
            database.connection(),
            &repository::SkillImportPreviewRecord {
                id,
                tool,
                context_json: serde_json::to_string(&context)
                    .map_err(|_| AppError::invalid_input("import", "导入证据无法序列化"))?,
                status: "previewed".to_owned(),
            },
            &serde_json::to_string(&preview)
                .map_err(|_| AppError::invalid_input("import", "导入展示无法序列化"))?,
        )?;
    }
    Ok(preview)
}

/// 中央记录必须是中央根的直属私有子目录；名称化是当前布局，id 命名是启动迁移前的历史布局。
fn central_record_in_private_root(paths: &AppPaths, record: &skills::SkillRecord) -> bool {
    let central = Path::new(&record.central_path);
    central == paths.central_skills().join(&record.id)
        || central == paths.central_skills().join(&record.name)
}

fn invalid_candidate(entry: &Path, reason: &str) -> SkillImportCandidateDto {
    SkillImportCandidateDto {
        candidate_id: Uuid::new_v4().to_string(),
        name: entry
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "无效技能".to_owned()),
        description: String::new(),
        source_paths: vec![entry.to_string_lossy().into_owned()],
        status: CandidateStatus::Invalid,
        reason: Some(reason.to_owned()),
        existing_skill_id: None,
        takeover_eligible: false,
        takeover_entry_type: None,
    }
}

fn validate_sources(
    environment: &ExplicitEnvironment,
    candidates: &[&CandidateEvidence],
    budget: &Cell<u64>,
    hash: bool,
) -> Result<(), AppError> {
    // 排除集合本身也可能在检测后或批量复制期间被改成链接，不能只在确认入口检查。
    let excluded = builtin_exclusions(environment);
    for candidate in candidates {
        for source in &candidate.sources {
            if library::resolve_skill_source_excluding(&source.root, &source.entry, &excluded)?
                != *source
            {
                return Err(AppError::conflict(
                    "sourcePath",
                    "Skill 来源入口或目录身份已经变化，请重新检测",
                ));
            }
            if hash {
                let inspection = library::inspect_skill_source(source, budget)?;
                if inspection.name != candidate.name || inspection.hash != candidate.hash {
                    return Err(AppError::conflict(
                        "sourcePath",
                        "Skill 来源内容已变化，请重新检测",
                    ));
                }
            }
        }
    }
    Ok(())
}

pub fn confirm_skill_import(
    database: &mut Database,
    paths: &AppPaths,
    environment: &ExplicitEnvironment,
    input: &ConfirmSkillImportInput,
) -> Result<SkillImportResultDto, AppError> {
    confirm_with_fault(database, paths, environment, input, &|_, _| Ok(()))
}

fn confirm_with_fault(
    database: &mut Database,
    paths: &AppPaths,
    environment: &ExplicitEnvironment,
    input: &ConfirmSkillImportInput,
    fault: &dyn Fn(&str, usize) -> Result<(), AppError>,
) -> Result<SkillImportResultDto, AppError> {
    let selected_ids: BTreeSet<_> = input.candidate_ids.iter().collect();
    if selected_ids.len() != input.candidate_ids.len()
        || selected_ids.is_empty()
        || selected_ids.len() > MAX_SELECTED
    {
        return Err(AppError::invalid_input(
            "candidateIds",
            "请选择 1 到 32 个不重复的可导入技能",
        ));
    }
    let record = repository::get_preview(database.connection(), &input.preview_id)?;
    let context: ImportContext = serde_json::from_str(&record.context_json)
        .map_err(|_| AppError::invalid_input("previewId", "导入证据无效，请重新检测"))?;
    if context.version != CONTEXT_VERSION {
        return Err(AppError::stale_preview(&record.id, "skillImport"));
    }
    repository::validate_preview(database.connection(), &record, &context.central_state)?;
    let descriptor = descriptor_for(
        environment,
        record.tool,
        None,
        environment.claude_customization_policy_probe(),
    )?;
    if descriptor.capability.state != CapabilityState::Supported
        || descriptor.policy != PolicyState::Allowed
        || environment_fingerprint(environment, record.tool, &descriptor) != context.environment
    {
        return Err(AppError::stale_preview(&record.id, "skillImport"));
    }
    let selected = selected_ids
        .iter()
        .map(|id| {
            context
                .candidates
                .iter()
                .find(|candidate| &candidate.id == *id)
                .ok_or_else(|| {
                    AppError::invalid_input("candidateIds", "选择包含未知或不可导入候选")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if selected
        .iter()
        .any(|candidate| candidate.existing_skill_id.is_some())
    {
        return Err(AppError::invalid_input(
            "candidateIds",
            "复制确认只能选择尚未进入中央库的候选",
        ));
    }
    let roots = source_roots(environment, record.tool);
    let excluded = builtin_exclusions(environment);
    if selected.iter().any(|candidate| {
        candidate.sources.is_empty()
            || candidate.sources.iter().any(|source| {
                !roots.iter().any(|(_, root)| source.root == *root)
                    || source.resolved.starts_with(paths.data_root())
                    || is_broad_source(environment, paths, record.tool, &source.resolved)
                    || excluded
                        .iter()
                        .any(|path| source.resolved.starts_with(path))
            })
    }) {
        return Err(AppError::stale_preview(&record.id, "skillImport"));
    }
    let budget = Cell::new(MAX_READ_BYTES);
    validate_sources(environment, &selected, &budget, true)
        .map_err(|_| AppError::stale_preview(&record.id, "skillImport"))?;
    let mut prepared: Vec<PreparedSkillImport> = Vec::new();
    let prepare_result = (|| {
        for (index, candidate) in selected.iter().enumerate() {
            fault("copy", index)?;
            validate_sources(environment, &[candidate], &budget, false)?;
            let item =
                library::prepare_discovered_skill_import(paths, &candidate.sources[0], &budget)?;
            let matches = item.name == candidate.name && item.content_hash == candidate.hash;
            prepared.push(item);
            if !matches {
                return Err(AppError::stale_preview(&record.id, "skillImport"));
            }
            validate_sources(environment, &[candidate], &budget, false)?;
        }
        Ok(())
    })();
    if let Err(error) = prepare_result {
        cleanup_batch(paths, &prepared)?;
        return Err(error);
    }
    let database_path = database.path().to_string_lossy().into_owned();
    let mut commit_attempted = false;
    let transaction_result = (|| {
        let transaction = database
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| AppError::database(&database_path, "begin_skill_import"))?;
        repository::validate_preview(&transaction, &record, &context.central_state)?;
        validate_sources(environment, &selected, &budget, true)?;
        for (index, item) in prepared.iter_mut().enumerate() {
            fault("rename", index)?;
            library::finalize_skill_import_budgeted(paths, item, Some(&budget))?;
            fault("sql", index)?;
            skills::insert_skill_in_transaction(
                &transaction,
                &database_path,
                &PreparedSkillRecord {
                    id: item.id.clone(),
                    name: item.name.clone(),
                    source_path: item.source_path.clone(),
                    central_path: item.central_path.clone(),
                    content_hash: item.content_hash.clone(),
                    frontmatter: item.frontmatter.clone(),
                },
            )?;
        }
        validate_sources(environment, &selected, &budget, true)?;
        for item in &prepared {
            library::verify_prepared_import_budgeted(paths, item, Some(&budget))?;
        }
        repository::consume_preview(&transaction, &record.id)?;
        fault("commit", prepared.len())?;
        commit_attempted = true;
        fault("uncertain_rollback", prepared.len())?;
        transaction
            .commit()
            .map_err(|_| AppError::database(&database_path, "commit_skill_import"))?;
        fault("after_commit", prepared.len())?;
        Ok(())
    })();
    if let Err(error) = transaction_result {
        if commit_attempted {
            // 提交返回错误时重新核验；不能删除可能已被数据库确认的副本。
            match committed_batch(database, paths, &record.id, &prepared) {
                Ok(Some(true)) => {
                    return Ok(SkillImportResultDto {
                        tool: record.tool,
                        created_count: prepared.len() as u32,
                    })
                }
                Ok(Some(false)) => {}
                _ => {
                    return Err(AppError::database(
                        &database_path,
                        "verify_uncertain_skill_import_commit",
                    ))
                }
            }
        }
        cleanup_batch(paths, &prepared)?;
        return Err(error);
    }
    Ok(SkillImportResultDto {
        tool: record.tool,
        created_count: prepared.len() as u32,
    })
}

pub fn prepare_skill_takeover(
    database: &mut Database,
    paths: &AppPaths,
    environment: &ExplicitEnvironment,
    redactor: &SecretRedactor,
    input: &PrepareSkillTakeoverInput,
) -> Result<SkillTakeoverPreviewResultDto, AppError> {
    let selected_ids = input.candidate_ids.iter().collect::<BTreeSet<_>>();
    if selected_ids.is_empty()
        || selected_ids.len() != input.candidate_ids.len()
        || selected_ids.len() > MAX_SELECTED
    {
        return Err(AppError::invalid_input(
            "candidateIds",
            "请选择 1 到 32 个不重复的可接管技能",
        ));
    }
    let record = repository::get_preview(database.connection(), &input.preview_id)?;
    let context: ImportContext = serde_json::from_str(&record.context_json)
        .map_err(|_| AppError::invalid_input("previewId", "接管证据无效，请重新检测"))?;
    if context.version != CONTEXT_VERSION {
        return Err(AppError::stale_preview(&record.id, "skillTakeover"));
    }
    repository::validate_preview(database.connection(), &record, &context.central_state)?;
    let descriptor = descriptor_for(
        environment,
        record.tool,
        None,
        environment.claude_customization_policy_probe(),
    )?;
    if descriptor.capability.state != CapabilityState::Supported
        || descriptor.policy != PolicyState::Allowed
        || environment_fingerprint(environment, record.tool, &descriptor) != context.environment
    {
        return Err(AppError::stale_preview(&record.id, "skillTakeover"));
    }
    let target_root = descriptor
        .path
        .as_deref()
        .ok_or_else(|| AppError::stale_preview(&record.id, "skillTakeover"))?;
    let selected = selected_ids
        .iter()
        .map(|id| {
            context
                .candidates
                .iter()
                .find(|candidate| &candidate.id == *id)
                .ok_or_else(|| {
                    AppError::invalid_input("candidateIds", "选择包含未知或不可接管候选")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_prepare_skill_takeover"))?;
    repository::validate_preview(&transaction, &record, &context.central_state)?;
    let mut entries = Vec::with_capacity(selected.len());
    let mut assigned_count = 0_u32;
    let mut reused_count = 0_u32;
    for candidate in &selected {
        let skill_id = candidate.existing_skill_id.as_deref().ok_or_else(|| {
            AppError::invalid_input("candidateIds", "接管候选缺少中央 Skill 身份")
        })?;
        let source = candidate.takeover_source.as_ref().ok_or_else(|| {
            AppError::invalid_input("candidateIds", "候选不来自当前工具的正式全局目标")
        })?;
        if source.root != Path::new(target_root) || source.entry.parent() != Some(&source.root) {
            return Err(AppError::stale_preview(&record.id, "skillTakeover"));
        }
        let entry_type = candidate
            .takeover_entry_type
            .ok_or_else(|| AppError::invalid_input("candidateIds", "接管候选缺少入口类型"))?;
        let expected_fingerprint = candidate
            .takeover_fingerprint
            .as_deref()
            .ok_or_else(|| AppError::invalid_input("candidateIds", "接管候选缺少入口身份"))?;
        let central_path = candidate
            .central_path
            .as_deref()
            .ok_or_else(|| AppError::invalid_input("candidateIds", "接管候选缺少中央路径"))?;
        validate_takeover_candidate_entry(
            paths,
            &record.id,
            candidate,
            source,
            entry_type,
            expected_fingerprint,
        )?;
        let central = skills::get_skill_from_connection(&transaction, &database_path, skill_id)?;
        if central.name != candidate.name
            || central.content_hash != candidate.hash
            || central.central_path != central_path
            || central.status != SkillStatus::Ready
        {
            return Err(AppError::stale_preview(&record.id, "centralSkill"));
        }
        let inspection = library::inspect_central_skill(
            paths,
            &central.id,
            &central.name,
            &central.central_path,
            &central.content_hash,
            central.status,
            false,
        )?;
        if inspection.status != SkillStatus::Ready {
            return Err(AppError::stale_preview(&record.id, "centralSkill"));
        }
        let already_assigned = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM skill_global_assignments
                    WHERE tool = ?1 AND skill_id = ?2
                 )",
                rusqlite::params![record.tool.as_str(), skill_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| AppError::database(&database_path, "read_takeover_assignment"))?;
        if already_assigned {
            reused_count = reused_count.saturating_add(1);
        } else {
            let row_version = u32::try_from(central.row_version)
                .map_err(|_| AppError::invalid_input("rowVersion", "Skill 版本超出范围"))?;
            skills::set_global_assignment_in_connection(
                &transaction,
                &database_path,
                record.tool,
                skill_id,
                true,
                row_version,
            )?;
            assigned_count = assigned_count.saturating_add(1);
        }
        entries.push(SkillTakeoverEntry {
            name: candidate.name.clone(),
            entry_path: source.entry.to_string_lossy().into_owned(),
            entry_type,
            expected_fingerprint: expected_fingerprint.to_owned(),
            content_hash: candidate.hash.clone(),
            central_path: central_path.to_owned(),
        });
    }
    let plan = service::build_skill_takeover_preview_in_connection(
        &transaction,
        &database_path,
        paths,
        environment,
        redactor,
        record.tool,
        entries,
    )?;
    for candidate in &selected {
        let source = candidate.takeover_source.as_ref().ok_or_else(|| {
            AppError::invalid_input("candidateIds", "候选不来自当前工具的正式全局目标")
        })?;
        validate_takeover_candidate_entry(
            paths,
            &record.id,
            candidate,
            source,
            candidate
                .takeover_entry_type
                .ok_or_else(|| AppError::invalid_input("candidateIds", "接管候选缺少入口类型"))?,
            candidate
                .takeover_fingerprint
                .as_deref()
                .ok_or_else(|| AppError::invalid_input("candidateIds", "接管候选缺少入口身份"))?,
        )?;
    }
    crate::sync::persist_preview_in_connection(&transaction, &plan, &database_path)?;
    repository::consume_preview(&transaction, &record.id)?;
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_prepare_skill_takeover"))?;
    Ok(SkillTakeoverPreviewResultDto {
        tool: record.tool,
        assigned_count,
        reused_count,
        plan,
    })
}

fn validate_takeover_candidate_entry(
    paths: &AppPaths,
    preview_id: &str,
    candidate: &CandidateEvidence,
    source: &SkillSourceEvidence,
    entry_type: SkillTakeoverEntryType,
    expected_fingerprint: &str,
) -> Result<(), AppError> {
    let current = library::inspect_skill_takeover_entry(&source.entry)
        .map_err(|_| AppError::stale_preview(preview_id, "skillTakeover"))?;
    let current_type = match current.entry_type {
        SkillTakeoverEntryKind::ExternalSymlink => SkillTakeoverEntryType::ExternalSymlink,
        SkillTakeoverEntryKind::Directory => SkillTakeoverEntryType::Directory,
    };
    if current_type != entry_type
        || current.fingerprint != expected_fingerprint
        || current.content_hash != candidate.hash
        || current.resolved.starts_with(paths.data_root())
    {
        return Err(AppError::stale_preview(preview_id, "skillTakeover"));
    }
    Ok(())
}

fn cleanup_batch(paths: &AppPaths, prepared: &[PreparedSkillImport]) -> Result<(), AppError> {
    let mut failure = None;
    for item in prepared.iter().rev() {
        if let Err(error) = library::cleanup_failed_import(paths, item) {
            failure = Some(error);
        }
    }
    if let Some(error) = failure {
        Err(error)
    } else {
        Ok(())
    }
}

fn committed_batch(
    database: &Database,
    paths: &AppPaths,
    id: &str,
    prepared: &[PreparedSkillImport],
) -> Result<Option<bool>, AppError> {
    let preview = repository::get_preview(database.connection(), id)?;
    let mut present = 0;
    for item in prepared {
        let row = database
            .connection()
            .query_row(
                "SELECT central_path, content_hash FROM skills WHERE id = ?1",
                [&item.id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|_| AppError::database("skills", "verify_skill_import_commit"))?;
        match row {
            Some((path, hash)) if path == item.central_path && hash == item.content_hash => {
                present += 1
            }
            Some(_) => return Ok(None),
            None => {}
        }
    }
    if preview.status == "consumed" && present == prepared.len() {
        for item in prepared {
            library::verify_prepared_import(paths, item)?;
        }
        Ok(Some(true))
    } else if preview.status == "previewed" && present == 0 {
        Ok(Some(false))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{ToolAvailability, VerifiedClaudeCustomizationPolicyEvidence};
    use std::{
        fs,
        os::unix::fs::{symlink, MetadataExt, PermissionsExt},
        sync::{Arc, Barrier, Mutex},
    };

    const BODY: &str = "PRIVATE_WORKFLOW_BODY_47281";
    const PRIVATE: &str = "PRIVATE_FRONTMATTER_49271";

    struct Fixture {
        _temporary: tempfile::TempDir,
        paths: AppPaths,
        database: Database,
        environment: ExplicitEnvironment,
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let home = root.join("home");
            fs::create_dir(&home).unwrap();
            let paths = AppPaths::from_data_root(root.join("data")).unwrap();
            let database = Database::open(&paths).unwrap();
            let environment =
                ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed())
                    .unwrap()
                    .with_claude_installation_version("fixture-1.0.0")
                    .unwrap()
                    .with_claude_customization_policy_evidence(
                        VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
                            "fixture-1.0.0",
                            None,
                        )
                        .unwrap(),
                    );
            Self {
                _temporary: temporary,
                paths,
                database,
                environment,
                root,
            }
        }

        fn skill(&self, path: &Path, name: &str) {
            fs::create_dir_all(path).unwrap();
            fs::write(
                path.join("SKILL.md"),
                format!(
                    "---\nname: {name}\ndescription: 测试技能\nprivate: {PRIVATE}\n---\n{BODY}\n"
                ),
            )
            .unwrap();
            fs::write(path.join("asset.sh"), "exit 0\n").unwrap();
            fs::set_permissions(path.join("asset.sh"), fs::Permissions::from_mode(0o751)).unwrap();
        }

        fn preview(&self, tool: Tool) -> SkillImportPreviewDto {
            discover_skill_import(&self.database, &self.paths, &self.environment, tool).unwrap()
        }

        fn input(preview: &SkillImportPreviewDto) -> ConfirmSkillImportInput {
            ConfirmSkillImportInput {
                preview_id: preview.preview_id.clone().unwrap(),
                candidate_ids: preview
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.status == CandidateStatus::Importable)
                    .map(|candidate| candidate.candidate_id.clone())
                    .collect(),
            }
        }

        fn confirm(
            &mut self,
            input: &ConfirmSkillImportInput,
        ) -> Result<SkillImportResultDto, AppError> {
            confirm_skill_import(&mut self.database, &self.paths, &self.environment, input)
        }

        fn assert_no_partial(&self) {
            assert!(skills::list_skills(&self.database).unwrap().is_empty());
            assert_eq!(
                fs::read_dir(self.paths.central_skills()).unwrap().count(),
                0
            );
            assert_eq!(fs::read_dir(self.paths.staging()).unwrap().count(), 0);
        }

        fn metadata_counts(&self) -> Vec<i64> {
            [
                "skill_global_assignments",
                "skill_project_assignments",
                "managed_targets",
                "managed_items",
                "sync_runs",
                "sync_items",
                "snapshots",
            ]
            .into_iter()
            .map(|table| {
                self.database
                    .connection()
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .unwrap()
            })
            .collect()
        }
    }

    #[test]
    fn compatibility_links_are_readonly_deduplicated_and_selected_without_adoption() {
        let mut fixture = Fixture::new();
        let source = fixture.root.join("manager/skill-one");
        fixture.skill(&source, "skill-one");
        let compat = fixture.environment.codex_home().join("skills");
        fs::create_dir_all(&compat).unwrap();
        symlink(&source, compat.join("first-alias")).unwrap();
        symlink("first-alias", compat.join("second-alias")).unwrap();
        fixture.skill(&compat.join("other"), "other");
        fs::write(compat.join(".DS_Store"), "metadata").unwrap();
        let before = fs::metadata(source.join("asset.sh")).unwrap();
        let metadata = fixture.metadata_counts();
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(preview.sources[0].status, SourceStatus::Ready);
        assert_eq!(preview.sources[1].status, SourceStatus::Missing);
        assert_eq!(preview.candidates.len(), 2);
        let candidate = preview
            .candidates
            .iter()
            .find(|candidate| candidate.name == "skill-one")
            .unwrap();
        assert_eq!(candidate.source_paths.len(), 2);
        fixture.assert_no_partial();
        let carriers = [
            serde_json::to_string(&preview).unwrap(),
            fixture
                .database
                .connection()
                .query_row(
                    "SELECT context_json || redacted_preview_json FROM skill_import_previews",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        ];
        for carrier in carriers {
            assert!(!carrier.is_empty());
            assert!(!carrier.contains(BODY));
            assert!(!carrier.contains(PRIVATE));
        }
        let input = ConfirmSkillImportInput {
            preview_id: preview.preview_id.unwrap(),
            candidate_ids: vec![candidate.candidate_id.clone()],
        };
        assert_eq!(fixture.confirm(&input).unwrap().created_count, 1);
        let records = skills::list_skills(&fixture.database).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source_path, source.to_string_lossy());
        assert_eq!(fixture.metadata_counts(), metadata);
        assert_eq!(fs::read_link(compat.join("first-alias")).unwrap(), source);
        let after = fs::metadata(source.join("asset.sh")).unwrap();
        assert_eq!((before.ino(), before.mode()), (after.ino(), after.mode()));
        assert_eq!(
            fs::read_to_string(source.join("asset.sh")).unwrap(),
            "exit 0\n"
        );
        assert!(!fixture.environment.home().join(".agents/skills").exists());
        assert_eq!(
            fixture.confirm(&input).unwrap_err().code(),
            ErrorCode::PreviewAlreadyConsumed
        );
        let repeated = fixture.preview(Tool::Codex);
        assert_eq!(
            repeated
                .candidates
                .iter()
                .find(|candidate| candidate.name == "skill-one")
                .unwrap()
                .status,
            CandidateStatus::AlreadyImported
        );
    }

    #[test]
    fn cursor_uses_dedicated_and_agents_import_sources_without_assigning() {
        let mut fixture = Fixture::new();
        let cursor_source = fixture.environment.home().join(".cursor/skills/cursor-one");
        let agents_source = fixture.environment.home().join(".agents/skills/agents-one");
        fixture.skill(&cursor_source, "cursor-one");
        fixture.skill(&agents_source, "agents-one");
        let metadata = fixture.metadata_counts();

        let preview = fixture.preview(Tool::Cursor);
        assert_eq!(preview.sources.len(), 2);
        assert_eq!(preview.sources[0].kind, SourceKind::CursorHome);
        assert_eq!(preview.sources[1].kind, SourceKind::CursorAgents);
        assert_eq!(preview.candidates.len(), 2);
        let input = Fixture::input(&preview);
        let result = fixture.confirm(&input).unwrap();
        assert_eq!(result.tool, Tool::Cursor);
        assert_eq!(result.created_count, 2);
        assert_eq!(fixture.metadata_counts(), metadata);
        assert!(cursor_source.join("SKILL.md").is_file());
        assert!(agents_source.join("SKILL.md").is_file());
    }

    #[test]
    fn exact_cursor_external_link_requires_takeover_preview_before_apply() {
        let mut fixture = Fixture::new();
        let external = fixture.root.join("external/one");
        fixture.skill(&external, "one");
        let agents_root = fixture.environment.home().join(".agents/skills");
        fs::create_dir_all(&agents_root).unwrap();
        symlink(&external, agents_root.join("one")).unwrap();
        let import_preview = fixture.preview(Tool::Cursor);
        let import_input = Fixture::input(&import_preview);
        assert_eq!(fixture.confirm(&import_input).unwrap().created_count, 1);
        fs::remove_file(agents_root.join("one")).unwrap();

        let cursor_root = fixture.environment.home().join(".cursor/skills");
        fs::create_dir_all(&cursor_root).unwrap();
        let cursor_entry = cursor_root.join("one");
        symlink(&external, &cursor_entry).unwrap();
        let preview = fixture.preview(Tool::Cursor);
        let candidate = preview
            .candidates
            .iter()
            .find(|candidate| candidate.name == "one")
            .unwrap();
        assert_eq!(candidate.status, CandidateStatus::AlreadyImported);
        assert!(candidate.takeover_eligible);
        assert_eq!(
            candidate.takeover_entry_type,
            Some(SkillTakeoverEntryType::ExternalSymlink)
        );
        let takeover_input = PrepareSkillTakeoverInput {
            preview_id: preview.preview_id.clone().unwrap(),
            candidate_ids: vec![candidate.candidate_id.clone()],
        };
        assert_eq!(
            fixture
                .confirm(&ConfirmSkillImportInput {
                    preview_id: takeover_input.preview_id.clone(),
                    candidate_ids: takeover_input.candidate_ids.clone(),
                })
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
        let external_before = fs::metadata(external.join("SKILL.md")).unwrap();
        let takeover = prepare_skill_takeover(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &takeover_input,
        )
        .unwrap();
        assert_eq!(takeover.assigned_count, 1);
        assert_eq!(takeover.reused_count, 0);
        assert!(takeover
            .plan
            .warning_codes
            .iter()
            .any(|code| code == crate::sync::WARNING_SKILL_TAKEOVER_CONFIRMATION));
        assert_eq!(fs::read_link(&cursor_entry).unwrap(), external);

        service::apply_skill_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &crate::skills::ApplySkillPreviewInput {
                preview_id: takeover.plan.preview_id,
                tool: Tool::Cursor,
                project_id: None,
            },
        )
        .unwrap();
        let central = skills::list_skills(&fixture.database).unwrap()[0]
            .central_path
            .clone();
        assert_eq!(
            fs::canonicalize(&cursor_entry).unwrap(),
            PathBuf::from(central)
        );
        let external_after = fs::metadata(external.join("SKILL.md")).unwrap();
        assert_eq!(external_before.ino(), external_after.ino());
        assert_eq!(
            fs::read_to_string(external.join("asset.sh")).unwrap(),
            "exit 0\n"
        );
    }

    #[test]
    fn takeover_prepare_failure_rolls_back_assignment_target_preview_and_token() {
        let mut fixture = Fixture::new();
        let external = fixture.root.join("external/one");
        fixture.skill(&external, "one");
        let agents_root = fixture.environment.home().join(".agents/skills");
        fs::create_dir_all(&agents_root).unwrap();
        symlink(&external, agents_root.join("one")).unwrap();
        let input = Fixture::input(&fixture.preview(Tool::Cursor));
        fixture.confirm(&input).unwrap();
        fs::remove_file(agents_root.join("one")).unwrap();
        let cursor_root = fixture.environment.home().join(".cursor/skills");
        fs::create_dir_all(&cursor_root).unwrap();
        symlink(&external, cursor_root.join("one")).unwrap();
        let preview = fixture.preview(Tool::Cursor);
        let candidate = preview
            .candidates
            .iter()
            .find(|candidate| candidate.takeover_eligible)
            .unwrap();
        let skill = skills::list_skills(&fixture.database).unwrap()[0].clone();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json
                 ) VALUES (?1, 'cursor', 'skill', 'global', ?2, ?3, ?3, '{}')",
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    cursor_root.to_string_lossy(),
                    "b".repeat(64),
                ],
            )
            .unwrap();

        let error = prepare_skill_takeover(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &PrepareSkillTakeoverInput {
                preview_id: preview.preview_id.clone().unwrap(),
                candidate_ids: vec![candidate.candidate_id.clone()],
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert!(skills::global_tools_for_skill(&fixture.database, &skill.id)
            .unwrap()
            .is_empty());
        assert_eq!(
            repository::get_preview(
                fixture.database.connection(),
                preview.preview_id.as_deref().unwrap(),
            )
            .unwrap()
            .status,
            "previewed"
        );
        assert_eq!(
            fixture
                .database
                .connection()
                .query_row("SELECT COUNT(*) FROM sync_runs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            skills::get_skill(&fixture.database, &skill.id)
                .unwrap()
                .row_version,
            skill.row_version
        );
        assert_eq!(fs::read_link(cursor_root.join("one")).unwrap(), external);
    }

    #[test]
    fn builtin_collections_aliases_and_symlinked_collection_aliases_are_never_read() {
        for tool in [Tool::Codex, Tool::Claude] {
            let fixture = Fixture::new();
            let compat = fixture.environment.codex_home().join("skills");
            let source = match tool {
                Tool::Codex => compat.clone(),
                Tool::Claude => fixture.environment.claude_config_dir().join("skills"),
                Tool::Cursor => fixture.environment.home().join(".cursor/skills"),
            };
            fixture.skill(&compat.join(".system/builtin"), "builtin");
            fs::create_dir_all(&source).unwrap();
            symlink(compat.join(".system/builtin"), source.join("builtin-alias")).unwrap();
            let preview = fixture.preview(tool);
            let source_status = preview
                .sources
                .iter()
                .find(|entry| entry.path == source.to_string_lossy())
                .unwrap();
            assert!(preview.candidates.is_empty(), "{tool:?}");
            assert_eq!(source_status.status, SourceStatus::Empty);
            assert_eq!(
                source_status.diagnostic_code.as_deref(),
                Some("SKILL_IMPORT_BUILTIN_EXCLUDED")
            );
            assert!(preview.preview_id.is_none());
            // 集合本身没有 SKILL.md，仍须解析其真实目录并排除跨工具别名。
            let actual = fixture.root.join("bundled");
            fs::rename(compat.join(".system"), &actual).unwrap();
            symlink(&actual, compat.join(".system")).unwrap();
            symlink(actual.join("builtin"), source.join("resolved-alias")).unwrap();
            assert!(fixture.preview(tool).candidates.is_empty(), "{tool:?}");
        }
    }

    #[test]
    fn builtin_aliases_created_during_confirmation_reject_the_whole_batch() {
        for tool in [Tool::Codex, Tool::Claude] {
            for changed_at in ["before", "copy", "sql"] {
                let mut fixture = Fixture::new();
                let compat = fixture.environment.codex_home().join("skills");
                let source = match tool {
                    Tool::Codex => compat.clone(),
                    Tool::Claude => fixture.environment.claude_config_dir().join("skills"),
                    Tool::Cursor => fixture.environment.home().join(".cursor/skills"),
                };
                let actual = fixture.root.join("external");
                fixture.skill(&actual.join("one"), "one");
                fs::create_dir_all(&source).unwrap();
                fs::create_dir_all(&compat).unwrap();
                symlink(actual.join("one"), source.join("one")).unwrap();
                let input = Fixture::input(&fixture.preview(tool));
                if changed_at == "before" {
                    symlink(&actual, compat.join(".system")).unwrap();
                }
                let result = confirm_with_fault(
                    &mut fixture.database,
                    &fixture.paths,
                    &fixture.environment,
                    &input,
                    &|stage, _| {
                        if stage == changed_at {
                            symlink(&actual, compat.join(".system")).unwrap();
                        }
                        Ok(())
                    },
                );
                assert!(result.is_err(), "{tool:?} / {changed_at}");
                fixture.assert_no_partial();
                assert_eq!(
                    repository::get_preview(fixture.database.connection(), &input.preview_id)
                        .unwrap()
                        .status,
                    "previewed"
                );
            }
        }
    }

    #[test]
    fn custom_roots_same_content_conflicts_invalid_links_and_private_paths() {
        let mut fixture = Fixture::new();
        let custom = fixture.root.join("custom-codex");
        fixture.environment = ExplicitEnvironment::new(
            fixture.environment.home(),
            None,
            Some(custom.clone()),
            ToolAvailability::all_installed(),
        )
        .unwrap();
        let agents = fixture.environment.home().join(".agents/skills");
        fixture.skill(&agents.join("one"), "same");
        fixture.skill(&custom.join("skills/two"), "same");
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(preview.candidates.len(), 1);
        assert_eq!(preview.candidates[0].source_paths.len(), 2);
        fs::write(custom.join("skills/two/asset.sh"), "different\n").unwrap();
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(preview.candidates.len(), 2);
        assert!(preview
            .candidates
            .iter()
            .all(|candidate| candidate.status == CandidateStatus::NameConflict));
        assert!(preview.preview_id.is_none());
        symlink("missing", agents.join("broken")).unwrap();
        symlink("cycle", agents.join("cycle")).unwrap();
        symlink(fixture.paths.staging(), agents.join("private")).unwrap();
        fixture.skill(&agents.join("escape"), "escape");
        symlink("../../outside", agents.join("escape/link")).unwrap();
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(
            preview
                .candidates
                .iter()
                .filter(|candidate| candidate.status == CandidateStatus::Invalid)
                .count(),
            4
        );
        fixture.assert_no_partial();
    }

    #[test]
    fn stale_source_content_link_identity_and_central_versions_reject_confirmation() {
        for change in ["content", "link", "central"] {
            let mut fixture = Fixture::new();
            let source = fixture.root.join("managed-external");
            fixture.skill(&source, "one");
            let root = fixture.environment.codex_home().join("skills");
            fs::create_dir_all(&root).unwrap();
            symlink(&source, root.join("alias")).unwrap();
            let input = Fixture::input(&fixture.preview(Tool::Codex));
            match change {
                "content" => fs::write(source.join("asset.sh"), "changed").unwrap(),
                "link" => {
                    let other = fixture.root.join("other");
                    fixture.skill(&other, "one");
                    fs::remove_file(root.join("alias")).unwrap();
                    symlink(&other, root.join("alias")).unwrap();
                }
                _ => {
                    fixture.database.connection().execute("UPDATE skill_import_previews SET context_json = json_set(context_json, '$.central_state', 'different')", []).unwrap();
                }
            }
            assert!(fixture.confirm(&input).is_err(), "{change}");
            fixture.assert_no_partial();
        }
    }

    #[test]
    fn invalid_selection_is_rejected_and_batch_faults_roll_back_every_item() {
        for stage in ["copy", "rename", "sql", "commit"] {
            let mut fixture = Fixture::new();
            let root = fixture.environment.codex_home().join("skills");
            fixture.skill(&root.join("one"), "one");
            fixture.skill(&root.join("two"), "two");
            let input = Fixture::input(&fixture.preview(Tool::Codex));
            for ids in [
                vec![],
                vec![input.candidate_ids[0].clone(); 2],
                vec![Uuid::new_v4().to_string()],
            ] {
                assert!(fixture
                    .confirm(&ConfirmSkillImportInput {
                        preview_id: input.preview_id.clone(),
                        candidate_ids: ids
                    })
                    .is_err());
            }
            let result = confirm_with_fault(
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &input,
                &|boundary, index| {
                    if boundary == stage && (index == 1 || boundary == "commit") {
                        Err(AppError::atomic_write("fixture", "injected_batch_failure"))
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(result.is_err(), "{stage}");
            fixture.assert_no_partial();
            assert_eq!(
                repository::get_preview(fixture.database.connection(), &input.preview_id)
                    .unwrap()
                    .status,
                "previewed"
            );
            assert_eq!(fixture.metadata_counts(), vec![0; 7]);
            assert_eq!(fixture.confirm(&input).unwrap().created_count, 2);
        }
    }

    #[test]
    fn uncertain_commit_is_verified_without_deleting_committed_copies() {
        let mut fixture = Fixture::new();
        fixture.skill(&fixture.environment.codex_home().join("skills/one"), "one");
        let input = Fixture::input(&fixture.preview(Tool::Codex));
        let result = confirm_with_fault(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &input,
            &|stage, _| {
                if stage == "after_commit" {
                    Err(AppError::database("fixture", "uncertain_commit"))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
        assert_eq!(result.created_count, 1);
        assert_eq!(
            crate::skills::list_skills(&fixture.database, &fixture.paths).unwrap()[0].status,
            SkillStatus::Ready
        );
    }

    #[test]
    fn independent_connections_only_consume_the_token_once() {
        let fixture = Fixture::new();
        fixture.skill(&fixture.environment.codex_home().join("skills/one"), "one");
        let input = Fixture::input(&fixture.preview(Tool::Codex));
        let barrier = Arc::new(Barrier::new(2));
        let connections = [
            Database::open(&fixture.paths).unwrap(),
            Database::open(&fixture.paths).unwrap(),
        ];
        let handles = connections
            .into_iter()
            .map(|mut database| {
                let paths = fixture.paths.clone();
                let environment = fixture.environment.clone();
                let input = input.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    confirm_skill_import(&mut database, &paths, &environment, &input)
                })
            })
            .collect::<Vec<_>>();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(skills::list_skills(&fixture.database).unwrap().len(), 1);
        assert_eq!(
            fs::read_dir(fixture.paths.central_skills())
                .unwrap()
                .count(),
            1
        );
        assert_eq!(fs::read_dir(fixture.paths.staging()).unwrap().count(), 0);
    }

    #[test]
    fn policy_blocked_and_unavailable_sources_do_not_hide_other_valid_sources() {
        let mut fixture = Fixture::new();
        let claude = fixture.environment.claude_config_dir().join("skills");
        fixture.skill(&claude.join("one"), "one");
        fixture.environment = ExplicitEnvironment::new(
            fixture.environment.home(),
            None,
            None,
            ToolAvailability::all_installed(),
        )
        .unwrap();
        let blocked = fixture.preview(Tool::Claude);
        assert!(blocked.candidates.is_empty());
        assert_eq!(blocked.sources[0].status, SourceStatus::Unavailable);
        let agents_parent = fixture.environment.home().join(".agents");
        fs::create_dir(&agents_parent).unwrap();
        symlink(&claude, agents_parent.join("skills")).unwrap();
        fixture.skill(&fixture.environment.codex_home().join("skills/two"), "two");
        let preview = fixture.preview(Tool::Codex);
        // codex_home/skills 优先且可用；.agents/skills 不可用不隐藏其余来源。
        assert_eq!(preview.sources[1].status, SourceStatus::Unavailable);
        assert_eq!(preview.candidates[0].name, "two");
    }
    #[test]
    fn detection_and_confirmation_limits_are_explicit_and_never_stage_during_detection() {
        let fixture = Fixture::new();
        let root = fixture.environment.codex_home().join("skills");
        for index in 0..=MAX_CANDIDATE_ENTRIES {
            fs::create_dir_all(root.join(format!("entry-{index:03}"))).unwrap();
        }
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(preview.candidates.len(), MAX_CANDIDATE_ENTRIES);
        assert_eq!(
            preview.sources[0].diagnostic_code.as_deref(),
            Some("SKILL_IMPORT_SCAN_LIMIT")
        );
        assert_eq!(preview.sources[0].status, SourceStatus::Unavailable);
        fixture.assert_no_partial();
        let one = root.join("entry-000");
        fixture.skill(&one, "one");
        let evidence = library::resolve_skill_source(&root, &one).unwrap();
        assert!(library::inspect_skill_source(&evidence, &Cell::new(1)).is_err());
        fixture.assert_no_partial();
    }

    #[test]
    fn writer_and_central_changes_block_import_and_do_not_consume_the_token() {
        let mut fixture = Fixture::new();
        fixture.skill(&fixture.environment.codex_home().join("skills/one"), "one");
        let input = Fixture::input(&fixture.preview(Tool::Codex));
        let run = Uuid::new_v4().to_string();
        fixture.database.connection().execute("INSERT INTO sync_runs(id, kind, status, scope, db_version) VALUES (?1, 'apply', 'applying', 'global', 1)", [&run]).unwrap();
        assert_eq!(
            fixture.confirm(&input).unwrap_err().code(),
            ErrorCode::WriteInProgress
        );
        fixture.assert_no_partial();
        fixture
            .database
            .connection()
            .execute("DELETE FROM sync_runs WHERE id = ?1", [&run])
            .unwrap();
        let other = fixture.root.join("other-source");
        fixture.skill(&other, "other");
        crate::skills::import_skill(
            &mut fixture.database,
            &fixture.paths,
            &crate::skills::ImportSkillInput {
                source_path: other.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        assert_eq!(
            fixture.confirm(&input).unwrap_err().code(),
            ErrorCode::StalePreview
        );
        assert_eq!(skills::list_skills(&fixture.database).unwrap().len(), 1);
        assert_eq!(
            repository::get_preview(fixture.database.connection(), &input.preview_id)
                .unwrap()
                .status,
            "previewed"
        );
    }

    #[test]
    fn changes_during_copy_and_uncertain_rollback_preserve_whole_batch_boundary() {
        for mode in ["source", "uncertain_rollback"] {
            let mut fixture = Fixture::new();
            let root = fixture.environment.codex_home().join("skills");
            fixture.skill(&root.join("one"), "one");
            fixture.skill(&root.join("two"), "two");
            let input = Fixture::input(&fixture.preview(Tool::Codex));
            let result = confirm_with_fault(
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &input,
                &|stage, index| {
                    if mode == "source" && stage == "copy" && index == 1 {
                        fs::write(root.join("one/asset.sh"), "changed").unwrap();
                    }
                    if mode == "uncertain_rollback" && stage == "uncertain_rollback" {
                        return Err(AppError::database("fixture", "uncertain_rollback"));
                    }
                    Ok(())
                },
            );
            assert!(result.is_err());
            fixture.assert_no_partial();
        }
    }

    #[test]
    fn known_central_links_reuse_across_tools_but_case_only_names_conflict() {
        let mut fixture = Fixture::new();
        fixture.skill(&fixture.environment.codex_home().join("skills/one"), "one");
        let input = Fixture::input(&fixture.preview(Tool::Codex));
        fixture.confirm(&input).unwrap();
        let central = skills::list_skills(&fixture.database).unwrap().remove(0);
        let claude = fixture.environment.claude_config_dir().join("skills");
        fs::create_dir_all(&claude).unwrap();
        symlink(&central.central_path, claude.join("central-alias")).unwrap();
        let preview = fixture.preview(Tool::Claude);
        assert_eq!(
            preview.candidates[0].status,
            CandidateStatus::AlreadyImported
        );
        assert_eq!(
            preview.candidates[0].existing_skill_id.as_deref(),
            Some(central.id.as_str())
        );
        assert!(preview.preview_id.is_none());
        fixture
            .database
            .connection()
            .execute(
                "UPDATE skills SET name = 'ONE' WHERE id = ?1",
                [&central.id],
            )
            .unwrap();
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(preview.candidates[0].status, CandidateStatus::NameConflict);
        assert!(preview.preview_id.is_none());
    }
    #[test]
    fn broad_links_and_new_builtin_aliases_cannot_be_confirmed() {
        let mut fixture = Fixture::new();
        let root = fixture.environment.codex_home().join("skills");
        fixture.skill(&root.join("one"), "one");
        symlink(fixture.environment.home(), root.join("broad-home")).unwrap();
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(
            preview
                .candidates
                .iter()
                .find(|candidate| candidate.name == "broad-home")
                .unwrap()
                .status,
            CandidateStatus::Invalid
        );
        let input = Fixture::input(&preview);
        symlink(root.join("one"), root.join(".system")).unwrap();
        assert!(fixture.confirm(&input).is_err());
        fixture.assert_no_partial();
    }
}
