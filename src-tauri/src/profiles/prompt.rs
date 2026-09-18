pub fn list_prompt_profiles(database: &Database) -> Result<Vec<PromptProfileDto>, AppError> {
    repository::list_prompt_profiles(database)?
        .iter()
        .map(prompt_dto)
        .collect()
}

pub fn create_prompt_profile(
    database: &mut Database,
    input: PromptProfileInput,
) -> Result<PromptProfileDto, AppError> {
    validate_prompt_fields(&input.name, &input.body)?;
    // 新建档案不绑定工具也不自动启用；用图标按工具启用（导入路径除外，见 confirm_prompt_import）。
    let record = repository::insert_prompt_profile(
        database,
        &NewPromptProfileRecord {
            id: Uuid::new_v4().to_string(),
            name: input.name,
            body: input.body,
            is_active_claude: false,
            is_active_codex: false,
            is_active_zcode: false,
            is_active_cursor: false,
            is_active_opencode: false,
            is_active_pi: false,
            imported_from_path: None,
        },
    )?;
    let mut dto = prompt_dto(&record)?;
    dto.affected_sync_scopes = Some(Vec::new());
    Ok(dto)
}

pub fn update_prompt_profile(
    database: &mut Database,
    input: UpdatePromptProfileInput,
) -> Result<PromptProfileDto, AppError> {
    validate_prompt_fields(&input.name, &input.body)?;
    let current = repository::get_prompt_profile(database, &input.id)?;
    let scopes = prompt_global_scopes(&current);
    let record = repository::update_prompt_profile(
        database,
        &input.id,
        &input.name,
        &input.body,
        i64::from(input.row_version),
    )?;
    let mut dto = prompt_dto(&record)?;
    dto.affected_sync_scopes = Some(scopes);
    Ok(dto)
}

pub fn set_global_prompt_assignment(
    database: &mut Database,
    input: &SetGlobalPromptAssignmentInput,
) -> Result<PromptProfileDto, AppError> {
    ensure_profile_capability(input.tool, ArtifactKind::Prompt)?;
    let current = repository::get_prompt_profile(database, &input.prompt_profile_id)?;
    let already_assigned = match input.tool {
        Tool::Claude => current.is_active_claude,
        Tool::Codex => current.is_active_codex,
        Tool::Zcode => current.is_active_zcode,
        Tool::Cursor => current.is_active_cursor,
        Tool::Opencode => current.is_active_opencode,
        Tool::Pi => current.is_active_pi,
    };
    let record = repository::set_global_prompt_assignment(
        database,
        input.tool,
        &input.prompt_profile_id,
        input.assigned,
        i64::from(input.row_version),
    )?;
    let mut dto = prompt_dto(&record)?;
    dto.affected_sync_scopes = Some(if already_assigned == input.assigned {
        Vec::new()
    } else {
        vec![SyncScopeDto::global(ArtifactKind::Prompt, input.tool)]
    });
    Ok(dto)
}

pub fn delete_prompt_profile(
    database: &mut Database,
    input: &VersionedProfileInput,
) -> Result<DeleteProfileResultDto, AppError> {
    let current = repository::get_prompt_profile(database, &input.id)?;
    repository::delete_prompt_profile(database, &input.id, i64::from(input.row_version))?;
    Ok(DeleteProfileResultDto {
        id: input.id.clone(),
        deleted: true,
        affected_sync_scopes: Some(prompt_global_scopes(&current)),
    })
}

pub fn discover_prompt_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<Option<PromptImportPreviewDto>, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Prompt)?;
    let descriptor = descriptor_for(environment, tool, ArtifactKind::Prompt)?;
    ensure_tool_is_available(&descriptor)?;
    // 该工具已有生效档案、或档案库已有同源导入（imported_from_path 相同）时不再检测。
    let prompt_path = descriptor_path(&descriptor)?;
    if repository::prompt_import_blocked(database, tool, &prompt_path)? {
        return Ok(None);
    }
    let scan = scan_target(
        tool.adapter(),
        &descriptor,
        &ManagedOwnership::WholeDocument,
    );
    let observed = match scan {
        TargetScan::Observed(observed) => observed,
        TargetScan::Missing => return Ok(None),
        TargetScan::ParseError => {
            return Err(AppError::parse(&descriptor_path(&descriptor)?, "markdown"));
        }
        _ => return Err(scan_error(&descriptor, &scan)),
    };
    let prompt_path = descriptor_path(&descriptor)?;
    let body = observed
        .managed_projection
        .as_str()
        .ok_or_else(|| AppError::parse(&prompt_path, "markdown"))?
        .to_owned();
    if body.trim().is_empty() {
        return Ok(None);
    }
    let preview_id = Uuid::new_v4().to_string();
    let target_path = prompt_path;
    let suggested_name = "已导入提示词".to_owned();
    repository::persist_import_preview(
        database,
        &ImportPreviewRecord {
            id: preview_id.clone(),
            tool,
            artifact_kind: ArtifactKind::Prompt,
            target_path: target_path.clone(),
            observed_full_hash: observed.full_hash.clone(),
            suggested_name: suggested_name.clone(),
            redacted_preview_json: "{}".to_owned(),
            status: "previewed".to_owned(),
        },
    )?;
    Ok(Some(PromptImportPreviewDto {
        preview_id,
        tool,
        target_path,
        suggested_name,
        body,
    }))
}

pub fn confirm_prompt_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: ConfirmImportInput,
) -> Result<PromptProfileDto, AppError> {
    let preview = repository::get_import_preview(database, &input.preview_id)?;
    if preview.artifact_kind != ArtifactKind::Prompt || preview.status != "previewed" {
        return Err(AppError::preview_already_consumed(
            &input.preview_id,
            &preview.status,
        ));
    }
    let descriptor = descriptor_for(environment, preview.tool, ArtifactKind::Prompt)?;
    ensure_tool_is_available(&descriptor)?;
    if descriptor_path(&descriptor)? != preview.target_path {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let observed = match scan_target(
        preview.tool.adapter(),
        &descriptor,
        &ManagedOwnership::WholeDocument,
    ) {
        TargetScan::Observed(observed) if observed.full_hash == preview.observed_full_hash => {
            observed
        }
        _ => return Err(AppError::stale_preview(&preview.id, &preview.target_path)),
    };
    let body = observed
        .managed_projection
        .as_str()
        .ok_or_else(|| AppError::parse(&preview.target_path, "markdown"))?
        .to_owned();
    validate_prompt_fields(&input.name, &body)?;
    let record = repository::adopt_imported_prompt(
        database,
        &preview,
        &NewPromptProfileRecord {
            id: Uuid::new_v4().to_string(),
            name: input.name,
            body: body.clone(),
            is_active_claude: preview.tool == Tool::Claude,
            is_active_codex: preview.tool == Tool::Codex,
            is_active_zcode: preview.tool == Tool::Zcode,
            is_active_cursor: preview.tool == Tool::Cursor,
            is_active_opencode: preview.tool == Tool::Opencode,
            is_active_pi: preview.tool == Tool::Pi,
            imported_from_path: Some(preview.target_path.clone()),
        },
        &ImportedBaselineRecord {
            target_id: Uuid::new_v4().to_string(),
            target_path: preview.target_path.clone(),
            full_hash: observed.full_hash,
            managed_hash: observed.managed_hash,
            projection_json: serde_json::to_string(&Value::String(body)).map_err(|error| {
                AppError::invalid_input("body", "提示词基线无法序列化").with_source(error)
            })?,
        },
    )?;
    let mut dto = prompt_dto(&record)?;
    dto.affected_sync_scopes = Some(vec![SyncScopeDto::global(
        ArtifactKind::Prompt,
        preview.tool,
    )]);
    Ok(dto)
}

/// 将 ExternalChangePlan 观察到的原生 Prompt 正文采纳为中央档案内容。
///
/// 采纳只允许发生在一个明确的 active profile 与一个既有 managed target
/// 的配对上；消费动作会再次做能力、路径、override/fallback、类型、解析、
/// hash 和 row-version 校验，最后由仓储在单一 SQLite 事务内更新中央正文与
/// target baseline。这里不能调用 `readopt_provider_target` 一类 baseline-only
/// helper，也不能在证据不完整时猜测档案身份。
pub fn adopt_prompt_native(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: super::models::AdoptPromptNativeInput,
) -> Result<super::models::AdoptPromptNativeResultDto, AppError> {
    ensure_profile_capability(input.tool, ArtifactKind::Prompt)?;
    if input.target_id.trim().is_empty() {
        return Err(AppError::invalid_input(
            "targetId",
            "Prompt 接管缺少受管目标身份",
        ));
    }
    if input.target_path.trim().is_empty() {
        return Err(AppError::invalid_input(
            "targetPath",
            "Prompt 接管缺少目标路径",
        ));
    }
    let expected_full_hash = input.observed_full_hash.as_deref().ok_or_else(|| {
        AppError::invalid_input(
            "observedFullHash",
            "Prompt 接管缺少原生目标 observation hash",
        )
    })?;
    let expected_managed_hash = input.observed_managed_hash.as_deref().ok_or_else(|| {
        AppError::invalid_input(
            "observedManagedHash",
            "Prompt 接管缺少受管正文 observation hash",
        )
    })?;
    if !is_sha256_hex(expected_full_hash) || !is_sha256_hex(expected_managed_hash) {
        return Err(AppError::invalid_input(
            "observedHash",
            "Prompt 接管的 observation hash 无效",
        ));
    }

    let descriptor = descriptor_for(environment, input.tool, ArtifactKind::Prompt)?;
    let descriptor_target_path = descriptor_path(&descriptor)?;
    if descriptor_target_path != input.target_path {
        return Err(AppError::invalid_input(
            "targetPath",
            "Prompt 目标路径与当前工具目标不一致",
        ));
    }
    ensure_tool_is_available(&descriptor)?;
    ensure_prompt_policy_and_trust(&descriptor)?;
    ensure_prompt_native_not_shadowed(&descriptor)?;

    let active = repository::find_active_prompt_profile(database, input.tool)?
        .ok_or_else(|| AppError::not_found("activePromptProfile", input.tool.as_str()))?;
    let profile_row_version = prompt_row_version_from_evidence(&input.row_versions, &active.id)?;
    if active.row_version != i64::from(profile_row_version) {
        return Err(AppError::stale_preview("adoptPromptNative", &active.id));
    }
    let target = find_profile_target(database, &descriptor)?
        .ok_or_else(|| AppError::stale_preview("adoptPromptNative", &input.target_id))?;
    if target.baseline.target_id != input.target_id
        || target.baseline.target_row_version != i64::from(input.target_row_version)
    {
        return Err(AppError::stale_preview(
            "adoptPromptNative",
            &input.target_id,
        ));
    }
    let (Some(baseline_full_hash), Some(baseline_managed_hash)) = (
        target.baseline.full_hash.as_deref(),
        target.baseline.managed_hash.as_deref(),
    ) else {
        return Err(AppError::conflict(
            "managedBaseline",
            "Prompt 目标缺少完整受管基线，不能直接采纳",
        ));
    };
    let Some(baseline_projection) = target.projection.as_ref() else {
        return Err(AppError::conflict(
            "managedBaseline",
            "Prompt 目标受管基线缺少正文投影",
        ));
    };
    if baseline_projection.as_str().is_none()
        || crate::sync::hash_json(baseline_projection) != baseline_managed_hash
        || !is_sha256_hex(baseline_full_hash)
    {
        return Err(AppError::conflict(
            "managedBaseline",
            "Prompt 目标受管基线无法验证",
        ));
    }

    let observed = match scan_target(
        input.tool.adapter(),
        &descriptor,
        &ManagedOwnership::WholeDocument,
    ) {
        TargetScan::Observed(observed) => observed,
        scan => return Err(scan_error(&descriptor, &scan)),
    };
    if observed.full_hash != expected_full_hash || observed.managed_hash != expected_managed_hash {
        return Err(AppError::stale_preview(
            "adoptPromptNative",
            &input.target_path,
        ));
    }
    let body = observed
        .managed_projection
        .as_str()
        .ok_or_else(|| AppError::parse(&input.target_path, descriptor.format.as_str()))?
        .to_owned();
    validate_prompt_fields(&active.name, &body)?;
    if descriptor.format == crate::adapters::TargetFormat::CursorMdc {
        validate_cursor_prompt_observation(&descriptor, &observed)?;
    }
    if active.body == body
        && baseline_full_hash == observed.full_hash
        && baseline_managed_hash == observed.managed_hash
    {
        // 真正 in-sync 时保持幂等：不更新档案，也不把完整 hash 重新写成一份
        // baseline。这样该动作不会借壳成为 baseline-only readopt。
        return Ok(super::models::AdoptPromptNativeResultDto {
            tool: input.tool,
            adopted: Vec::new(),
            affected_sync_scopes: Some(Vec::new()),
        });
    }
    if active.body == body || baseline_managed_hash == observed.managed_hash {
        // 原生正文未真正改变时拒绝动作，避免把该入口退化成只刷新 baseline
        // 的 readopt；非受管变化应由普通状态/同步流程处理。
        return Err(AppError::conflict(
            "adoptPromptNative",
            "原生 Prompt 没有可采纳的受管正文变化",
        ));
    }

    let record = repository::adopt_native_prompt(
        database,
        &repository::NativePromptAdoption {
            tool: input.tool,
            profile_id: active.id,
            profile_row_version: i64::from(profile_row_version),
            target_id: input.target_id,
            target_row_version: i64::from(input.target_row_version),
            target_path: input.target_path,
            observed_full_hash: expected_full_hash.to_owned(),
            observed_managed_hash: expected_managed_hash.to_owned(),
            body,
        },
    )?;
    let affected_sync_scopes = prompt_global_scopes(&record);
    Ok(super::models::AdoptPromptNativeResultDto {
        tool: input.tool,
        adopted: vec![record.name],
        affected_sync_scopes: Some(affected_sync_scopes),
    })
}

fn prompt_row_version_from_evidence(
    rows: &[DatabaseRowVersion],
    profile_id: &str,
) -> Result<u32, AppError> {
    let mut matched = rows.iter().filter(|row| {
        row.entity_type == DatabaseEntityType::PromptProfile && row.entity_id == profile_id
    });
    let Some(first) = matched.next() else {
        return Err(AppError::invalid_input(
            "rowVersions",
            "Prompt 接管缺少 active 档案的行版本",
        ));
    };
    if matched.any(|row| row.row_version != first.row_version) {
        return Err(AppError::invalid_input(
            "rowVersions",
            "Prompt 接管包含互相矛盾的档案行版本",
        ));
    }
    if rows.iter().any(|row| {
        row.entity_type == DatabaseEntityType::PromptProfile && row.entity_id != profile_id
    }) {
        return Err(AppError::stale_preview("adoptPromptNative", profile_id));
    }
    Ok(first.row_version)
}

fn ensure_prompt_policy_and_trust(descriptor: &TargetDescriptor) -> Result<(), AppError> {
    match descriptor.policy {
        PolicyState::Allowed => {}
        PolicyState::Blocked => {
            return Err(AppError::policy_blocked(
                descriptor.tool.as_str(),
                descriptor.path.as_deref().unwrap_or("<unsupported>"),
                "prompt",
            ));
        }
        PolicyState::Unknown => {
            return Err(AppError::conflict(
                "policy",
                "工具 Prompt 管理策略无法安全确认",
            ));
        }
    }
    match descriptor.trust {
        crate::adapters::TargetTrustState::NotRequired
        | crate::adapters::TargetTrustState::Trusted => Ok(()),
        crate::adapters::TargetTrustState::Untrusted => Err(AppError::untrusted_project(
            descriptor.tool.as_str(),
            descriptor.path.as_deref().unwrap_or("<unsupported>"),
        )),
        crate::adapters::TargetTrustState::Unknown => Err(AppError::conflict(
            "trust",
            "工具 Prompt 信任状态无法安全确认",
        )),
    }
}

/// Override/fallback 会让写入的正文不再是工具真正使用的 Prompt；任何存在或
/// 无法判定的遮蔽都不能被直接采纳。Pi 还需对 fallback 目录做更严格的
/// symlink/类型/权限探测，避免既有 helper 把不可读文件当成“不存在”。
fn ensure_prompt_native_not_shadowed(descriptor: &TargetDescriptor) -> Result<(), AppError> {
    match descriptor.prompt_override {
        crate::adapters::PromptOverrideState::Present
        | crate::adapters::PromptOverrideState::Unknown => {
            return Err(AppError::conflict(
                "promptOverride",
                "原生 Prompt 存在覆盖文件或无法安全确认",
            ));
        }
        crate::adapters::PromptOverrideState::NotApplicable
        | crate::adapters::PromptOverrideState::NotPresent => {}
    }
    if descriptor.tool != Tool::Pi {
        return Ok(());
    }
    let path = descriptor_path(descriptor)?;
    let directory = std::path::Path::new(&path)
        .parent()
        .ok_or_else(|| AppError::invalid_input("targetPath", "Pi Prompt 目标目录不可用"))?;
    let target = directory.join("AGENTS.md");
    match std::fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => return Ok(()),
        Ok(_) => {
            return Err(AppError::conflict(
                "promptFallback",
                "Pi Prompt 目标不是安全的普通文件",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(AppError::permission(&path, "inspect_prompt_fallback"));
        }
        Err(_) => {
            return Err(AppError::conflict(
                "promptFallback",
                "Pi Prompt fallback 状态无法安全确认",
            ));
        }
    }
    for name in ["CLAUDE.md", "CLAUDE.MD", "AGENTS.MD"] {
        let fallback = directory.join(name);
        match std::fs::symlink_metadata(&fallback) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                return Err(AppError::conflict(
                    "promptFallback",
                    "Pi Prompt 存在会被遮蔽的 fallback 文件",
                ));
            }
            Ok(_) => {
                return Err(AppError::conflict(
                    "promptFallback",
                    "Pi Prompt fallback 类型无法安全确认",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                return Err(AppError::permission(&path, "inspect_prompt_fallback"));
            }
            Err(_) => {
                return Err(AppError::conflict(
                    "promptFallback",
                    "Pi Prompt fallback 状态无法安全确认",
                ));
            }
        }
    }
    Ok(())
}

/// `scan_target` 已经完成一次安全读取和通用解析；Cursor 额外要求首部是完整
/// `.mdc` frontmatter，避免其解析器在缺少闭合 `---` 时把整份坏文档误当正文。
/// 再次读取只用于核对首部和同一 full hash，不把第二次读取当成新的身份来源。
fn validate_cursor_prompt_observation(
    descriptor: &TargetDescriptor,
    observed: &crate::sync::ObservedTarget,
) -> Result<(), AppError> {
    let path = descriptor_path(descriptor)?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => AppError::stale_preview("adoptPromptNative", &path),
        std::io::ErrorKind::PermissionDenied => {
            AppError::permission(&path, "inspect_prompt_target")
        }
        _ => AppError::conflict("promptTarget", "Cursor Prompt 在采纳期间无法安全确认类型"),
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(AppError::conflict(
            "targetPath",
            "Cursor Prompt 目标类型在采纳期间发生变化",
        ));
    }
    let bytes = std::fs::read(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => AppError::stale_preview("adoptPromptNative", &path),
        std::io::ErrorKind::PermissionDenied => AppError::permission(&path, "read_prompt_target"),
        _ => AppError::conflict("promptTarget", "Cursor Prompt 在采纳期间无法安全读取"),
    })?;
    if crate::sync::hash_bytes(&bytes) != observed.full_hash {
        return Err(AppError::stale_preview("adoptPromptNative", &path));
    }
    let text =
        String::from_utf8(bytes).map_err(|_| AppError::parse(&path, descriptor.format.as_str()))?;
    let body = strict_cursor_prompt_body(&text)
        .ok_or_else(|| AppError::parse(&path, descriptor.format.as_str()))?;
    if observed.managed_projection.as_str() != Some(body) {
        return Err(AppError::parse(&path, descriptor.format.as_str()));
    }
    Ok(())
}

fn strict_cursor_prompt_body(text: &str) -> Option<&str> {
    let after_open = text
        .strip_prefix("---\r\n")
        .or_else(|| text.strip_prefix("---\n"))?;
    let mut offset = 0usize;
    for line in after_open.lines() {
        let line_len = line.len();
        let rest_starts = offset + line_len;
        let eol_len = if after_open[rest_starts..].starts_with("\r\n") {
            2
        } else if after_open[rest_starts..].starts_with('\n') {
            1
        } else {
            0
        };
        offset = rest_starts + eol_len;
        if line == "---" {
            return Some(after_open[offset..].trim_start_matches(['\n', '\r']));
        }
        if eol_len == 0 {
            break;
        }
    }
    None
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
