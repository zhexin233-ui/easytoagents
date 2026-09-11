//! 显式发现用户 Skills 并批量复制到中央库；原安装与同步元数据保持不变。

use std::{
    cell::Cell,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{
    library::{self, PreparedSkillImport, SkillSourceEvidence, SkillTakeoverEntryKind},
    service::{self, skill_target_descriptor},
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
        SourceKind::ClaudeGlobal
            | SourceKind::CodexHome
            | SourceKind::CursorHome
            | SourceKind::ZcodeHome
            | SourceKind::OpencodeGlobal
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
        Tool::Zcode => vec![
            (
                // 官方目录，正式同步目标，优先展示。
                SourceKind::ZcodeHome,
                environment.home().join(".zcode/skills"),
            ),
            (
                // 跨工具通用目录，仅作为导入来源，不再是同步目标。
                SourceKind::ZcodeAgents,
                environment.home().join(".agents/skills"),
            ),
        ],
        Tool::Opencode => vec![(
            SourceKind::OpencodeGlobal,
            environment.opencode_config_dir().join("skills"),
        )],
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
    let descriptor = skill_target_descriptor(
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
            let evidence = match library::resolve_skill_source_excluding(
                &root,
                &entry,
                &excluded_roots,
            ) {
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
                    let reason = if fs::symlink_metadata(&entry)
                        .is_ok_and(|metadata| metadata.file_type().is_symlink())
                        && fs::metadata(&entry)
                            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    {
                        "来源软链接的目标已不存在，无法复制或比对内容；请恢复链接目标，或备份并移走失效链接后同步中央版本"
                    } else {
                        "来源链接、目录身份或权限无效"
                    };
                    preview.candidates.push(invalid_candidate(&entry, reason));
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
                context_json: serde_json::to_string(&context).map_err(|error| {
                    AppError::invalid_input("import", "导入证据无法序列化").with_source(error)
                })?,
                status: "previewed".to_owned(),
            },
            &serde_json::to_string(&preview).map_err(|error| {
                AppError::invalid_input("import", "导入展示无法序列化").with_source(error)
            })?,
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
    let context: ImportContext = serde_json::from_str(&record.context_json).map_err(|error| {
        AppError::invalid_input("previewId", "导入证据无效，请重新检测").with_source(error)
    })?;
    if context.version != CONTEXT_VERSION {
        return Err(AppError::stale_preview(&record.id, "skillImport"));
    }
    repository::validate_preview(database.connection(), &record, &context.central_state)?;
    let descriptor = skill_target_descriptor(
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
        .map_err(|error| AppError::stale_preview(&record.id, "skillImport").with_source(error))?;
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
            .map_err(|error| {
                AppError::database(&database_path, "begin_skill_import").with_source(error)
            })?;
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
        transaction.commit().map_err(|error| {
            AppError::database(&database_path, "commit_skill_import").with_source(error)
        })?;
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
    let context: ImportContext = serde_json::from_str(&record.context_json).map_err(|error| {
        AppError::invalid_input("previewId", "接管证据无效，请重新检测")
            .with_source_redacted(error, redactor)
    })?;
    if context.version != CONTEXT_VERSION {
        return Err(AppError::stale_preview(&record.id, "skillTakeover"));
    }
    repository::validate_preview(database.connection(), &record, &context.central_state)?;
    let descriptor = skill_target_descriptor(
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
        .map_err(|error| {
            AppError::database(&database_path, "begin_prepare_skill_takeover").with_source(error)
        })?;
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
            .map_err(|error| {
                AppError::database(&database_path, "read_takeover_assignment").with_source(error)
            })?;
        if already_assigned {
            reused_count = reused_count.saturating_add(1);
        } else {
            let row_version = u32::try_from(central.row_version).map_err(|error| {
                AppError::invalid_input("rowVersion", "Skill 版本超出范围").with_source(error)
            })?;
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
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_prepare_skill_takeover").with_source(error)
    })?;
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
        .map_err(|error| AppError::stale_preview(preview_id, "skillTakeover").with_source(error))?;
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
            .map_err(|error| {
                AppError::database("skills", "verify_skill_import_commit").with_source(error)
            })?;
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
include!("import_tests.rs");
