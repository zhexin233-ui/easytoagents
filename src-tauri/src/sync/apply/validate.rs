fn validate_preview_inputs(
    database: &Database,
    preview: &PersistedPreview,
    inputs: &[ApplyTargetInput],
) -> Result<(), AppError> {
    if preview.items.len() != inputs.len() {
        return Err(AppError::stale_preview(&preview.preview_id, "targetSet"));
    }
    let by_path = inputs_by_target_path(preview, inputs)?;
    let database_path = database.path().to_string_lossy().into_owned();
    let mut expected_versions = BTreeMap::new();
    for item in &preview.items {
        if item.change_kind == ChangeKind::Conflict || item.error_code.is_some() {
            return Err(AppError::conflict("preview", "包含冲突的 Preview 不能应用"));
        }
        let input = by_path
            .get(item.target_path.as_str())
            .copied()
            .ok_or_else(|| AppError::stale_preview(&preview.preview_id, &item.target_id))?;
        if input.descriptor != item.envelope.descriptor
            || input.ownership != item.envelope.ownership
            || input.descriptor.path.as_deref() != Some(item.target_path.as_str())
        {
            return Err(AppError::stale_preview(
                &preview.preview_id,
                &item.target_id,
            ));
        }
        if hash_json(&input.desired_projection) != item.envelope.desired_managed_hash {
            return Err(AppError::stale_preview(
                &preview.preview_id,
                &item.target_id,
            ));
        }
        if input.delete_target && item.change_kind != ChangeKind::Delete {
            return Err(AppError::invalid_input(
                "deleteTarget",
                "只有 delete Preview 可以删除 whole-document 目标",
            ));
        }
        if input.delete_target && input.ownership != ManagedOwnership::WholeDocument {
            return Err(AppError::invalid_input(
                "deleteTarget",
                "只有 whole-document 所有权可以删除整个目标",
            ));
        }
        validate_descriptor_identity(
            database,
            item,
            preview.scope,
            preview.project_id.as_deref(),
            &database_path,
        )?;
        validate_managed_item_inputs(
            database.connection(),
            item,
            input,
            &preview.preview_id,
            &database_path,
        )?;
        validate_skill_takeover_inputs(item, input)?;
        validate_native_resource_inputs(item, input)?;
        record_expected_versions(item, &mut expected_versions)?;
        validate_allowed_path(
            Path::new(&item.target_path),
            &input.allowed_root,
            input.descriptor.format == TargetFormat::SymlinkDirectory,
        )?;
        validate_preview_hashes(item, input)?;
    }
    verify_database_versions(
        database.connection(),
        &expected_versions,
        &preview.preview_id,
        &database_path,
    )
}

fn validate_native_resource_inputs(
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
) -> Result<(), AppError> {
    if input.project_native_action != item.envelope.project_native_action {
        return Err(AppError::stale_preview("persisted", &item.target_id));
    }
    let Some(evidence) = input.project_native_action.as_ref() else {
        return Ok(());
    };
    if !input.skill_takeover_entries.is_empty() {
        return Err(AppError::invalid_input(
            "projectNativeAction",
            "原生资源动作不能与 Skills 接管证据同时出现",
        ));
    }
    if evidence.resource_id.is_empty()
        || evidence.external_key.is_empty()
        || !is_sha256(&evidence.observed_item_hash)
    {
        return Err(AppError::invalid_input(
            "projectNativeAction",
            "原生资源证据缺少稳定身份或 hash",
        ));
    }
    match evidence.action {
        super::NativeResourceActionKind::Disable
            if item.change_kind != ChangeKind::Delete && item.change_kind != ChangeKind::Update =>
        {
            return Err(AppError::invalid_input(
                "projectNativeAction",
                "禁用预览必须删除或更新目标选择器",
            ));
        }
        super::NativeResourceActionKind::Restore
            if item.change_kind != ChangeKind::Add && item.change_kind != ChangeKind::Update =>
        {
            return Err(AppError::invalid_input(
                "projectNativeAction",
                "恢复预览必须新增或更新目标选择器",
            ));
        }
        _ => {}
    }
    Ok(())
}

fn validate_skill_takeover_inputs(
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
) -> Result<(), AppError> {
    if input.skill_takeover_entries != item.envelope.skill_takeover_entries {
        return Err(AppError::stale_preview("persisted", &item.target_id));
    }
    if input.skill_takeover_entries.is_empty() {
        return Ok(());
    }
    if input.descriptor.artifact_kind != ArtifactKind::Skill
        || input.descriptor.scope != Scope::Global
        || input.descriptor.format != TargetFormat::SymlinkDirectory
    {
        return Err(AppError::invalid_input(
            "skillTakeover",
            "只有全局 Skills 符号链接目录可以接管",
        ));
    }
    let central_root = input
        .central_skills_root
        .as_deref()
        .ok_or_else(|| AppError::invalid_input("centralSkillsRoot", "接管缺少中央 Skills 根"))?;
    let desired = input
        .desired_projection
        .as_object()
        .ok_or_else(|| AppError::invalid_input("desiredProjection", "Skills 投影必须是对象"))?;
    let target_root = Path::new(&item.target_path);
    let mut names = BTreeSet::new();
    for entry in &input.skill_takeover_entries {
        validate_child_name(&entry.name)?;
        if !names.insert(entry.name.as_str())
            || entry.entry_path != target_root.join(&entry.name).to_string_lossy()
            || !is_sha256(&entry.content_hash)
            || !is_sha256(&entry.expected_fingerprint)
            || desired
                .get(&entry.name)
                .and_then(|value| value.get("linkTarget"))
                .and_then(Value::as_str)
                != Some(entry.central_path.as_str())
        {
            return Err(AppError::invalid_input(
                "skillTakeover",
                "接管名称、入口、hash 或中央投影无效",
            ));
        }
        validate_central_link_target(
            Path::new(&entry.entry_path),
            Path::new(&entry.central_path),
            central_root,
        )?;
        let current = skill_library::inspect_skill_takeover_entry(Path::new(&entry.entry_path))
            .map_err(|error| {
                AppError::stale_preview("persisted", &item.target_id).with_source(error)
            })?;
        let current_type = match current.entry_type {
            SkillTakeoverEntryKind::ExternalSymlink => SkillTakeoverEntryType::ExternalSymlink,
            SkillTakeoverEntryKind::Directory => SkillTakeoverEntryType::Directory,
        };
        if current_type != entry.entry_type
            || current.fingerprint != entry.expected_fingerprint
            || current.content_hash != entry.content_hash
            || current.resolved.starts_with(central_root)
        {
            return Err(AppError::stale_preview("persisted", &item.target_id));
        }
    }
    Ok(())
}

fn validate_managed_item_inputs(
    connection: &rusqlite::Connection,
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
    preview_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    let expected_versions = item
        .envelope
        .row_versions
        .iter()
        .filter(|row| row.entity_type == DatabaseEntityType::ManagedItem)
        .map(|row| (row.entity_id.as_str(), row.row_version))
        .collect::<BTreeMap<_, _>>();
    let mut ids = BTreeSet::new();
    for managed_item in &input.managed_items {
        if !ids.insert(managed_item.id.as_str())
            || managed_item.resource_kind != input.descriptor.artifact_kind
            || Uuid::parse_str(&managed_item.id).is_err()
            || Uuid::parse_str(&managed_item.resource_id).is_err()
            || managed_item.external_key.is_empty()
            || !is_sha256(&managed_item.last_applied_item_hash)
        {
            return Err(AppError::invalid_input(
                "managedItems",
                "managed item 的身份、类型、名称或 hash 无效",
            ));
        }
        let existing = connection
            .query_row(
                "SELECT target_id, row_version FROM managed_items WHERE id = ?1",
                [&managed_item.id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|error| {
                AppError::database(database_path, "preflight_managed_item").with_source(error)
            })?;
        match existing {
            Some((target_id, row_version)) => {
                let expected = expected_versions
                    .get(managed_item.id.as_str())
                    .copied()
                    .ok_or_else(|| AppError::stale_preview(preview_id, &managed_item.id))?;
                if target_id != item.target_id || u32::try_from(row_version).ok() != Some(expected)
                {
                    return Err(AppError::stale_preview(preview_id, &managed_item.id));
                }
            }
            None => {
                if expected_versions.contains_key(managed_item.id.as_str()) {
                    return Err(AppError::stale_preview(preview_id, &managed_item.id));
                }
                let conflicting_id = connection
                    .query_row(
                        "SELECT id FROM managed_items WHERE target_id = ?1 AND external_key = ?2",
                        params![item.target_id, managed_item.external_key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|error| {
                        AppError::database(database_path, "preflight_managed_item_key")
                            .with_source(error)
                    })?;
                if conflicting_id.is_some() {
                    return Err(AppError::conflict(
                        "managedItems",
                        "managed item 外部名称已被其他基线占用",
                    ));
                }
            }
        }
    }
    for remove_id in &input.remove_managed_item_ids {
        if !ids.insert(remove_id) || Uuid::parse_str(remove_id).is_err() {
            return Err(AppError::invalid_input(
                "managedItems",
                "managed item 删除身份无效或与更新重复",
            ));
        }
        let expected = expected_versions
            .get(remove_id.as_str())
            .copied()
            .ok_or_else(|| AppError::stale_preview(preview_id, remove_id))?;
        let actual = connection
            .query_row(
                "SELECT row_version FROM managed_items WHERE id = ?1 AND target_id = ?2",
                params![remove_id, item.target_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| {
                AppError::database(database_path, "preflight_remove_managed_item")
                    .with_source(error)
            })?;
        if actual.and_then(|value| u32::try_from(value).ok()) != Some(expected) {
            return Err(AppError::stale_preview(preview_id, remove_id));
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_descriptor_identity(
    database: &Database,
    item: &PersistedPreviewItem,
    preview_scope: Scope,
    preview_project_id: Option<&str>,
    database_path: &str,
) -> Result<(), AppError> {
    let identity = crate::db::sync::load_managed_target_identity(
        database.connection(),
        &item.target_id,
        database_path,
        "verify_apply_descriptor",
    )?
        .ok_or_else(|| AppError::stale_preview("persisted", &item.target_id))?;
    let descriptor = &item.envelope.descriptor;
    if u32::try_from(identity.row_version).ok() != Some(item.envelope.target_row_version)
        || identity.tool != descriptor.tool.as_str()
        || identity.artifact_kind != descriptor.artifact_kind.as_str()
        || identity.scope != descriptor.scope.as_str()
        || descriptor.scope != preview_scope
        || identity.project_id.as_deref() != preview_project_id
        || identity.target_path != item.target_path
        || identity.project_root != descriptor.project_root
    {
        return Err(AppError::stale_preview("persisted", &item.target_id));
    }
    Ok(())
}

fn record_expected_versions(
    item: &PersistedPreviewItem,
    versions: &mut BTreeMap<(DatabaseEntityType, String), u32>,
) -> Result<(), AppError> {
    let target = DatabaseRowVersion {
        entity_type: DatabaseEntityType::ManagedTarget,
        entity_id: item.target_id.clone(),
        row_version: item.envelope.target_row_version,
    };
    for row in std::iter::once(&target).chain(item.envelope.row_versions.iter()) {
        let key = (row.entity_type, row.entity_id.clone());
        if versions
            .insert(key, row.row_version)
            .is_some_and(|existing| existing != row.row_version)
        {
            return Err(AppError::invalid_input(
                "rowVersions",
                "Preview 的 row_version 互相矛盾",
            ));
        }
    }
    Ok(())
}

fn verify_database_versions(
    connection: &rusqlite::Connection,
    versions: &BTreeMap<(DatabaseEntityType, String), u32>,
    preview_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    for ((entity_type, entity_id), expected) in versions {
        let actual = crate::db::sync::load_row_version(
            connection,
            entity_type.table(),
            entity_id,
            database_path,
            "verify_apply_row_version",
        )?;
        if actual.and_then(|value| u32::try_from(value).ok()) != Some(*expected) {
            return Err(AppError::stale_preview(preview_id, entity_id));
        }
    }
    Ok(())
}

/// SQLite 的 `data_version`：其它连接提交写入后递增；本连接自己的写入不改变它。
fn read_data_version(database: &Database) -> Result<i64, AppError> {
    database
        .connection()
        .query_row("PRAGMA data_version", [], |row| row.get::<_, i64>(0))
        .map_err(|error| {
            AppError::database(&database.path().to_string_lossy(), "read_data_version")
                .with_source(error)
        })
}

fn revalidate_database_preflight(
    database: &Database,
    preview: &PersistedPreview,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let mut expected_versions = BTreeMap::new();
    for item in &preview.items {
        validate_descriptor_identity(
            database,
            item,
            preview.scope,
            preview.project_id.as_deref(),
            &database_path,
        )?;
        record_expected_versions(item, &mut expected_versions)?;
    }
    verify_database_versions(
        database.connection(),
        &expected_versions,
        &preview.preview_id,
        &database_path,
    )
}

fn validate_preview_hashes(
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
) -> Result<(), AppError> {
    let adapter = input.descriptor.tool.adapter();
    let scan = scan_target(adapter, &input.descriptor, &input.ownership);
    if input.project_native_action.is_some() {
        return match scan {
            TargetScan::Missing
                if item.envelope.current_full_hash.is_none()
                    && item.envelope.current_managed_hash.is_none() =>
            {
                Ok(())
            }
            TargetScan::Observed(observed)
                if item.envelope.current_managed_hash.as_deref()
                    == Some(&observed.managed_hash) =>
            {
                Ok(())
            }
            TargetScan::Missing
                if item
                    .envelope
                    .current_managed_hash
                    .as_deref()
                    .is_some_and(|hash| hash == hash_json(&serde_json::json!({}))) =>
            {
                Ok(())
            }
            _ => Err(AppError::stale_preview("persisted", &item.target_id)),
        };
    }
    match scan {
        TargetScan::Missing
            if item.envelope.current_full_hash.is_none()
                && item.envelope.current_managed_hash.is_none() =>
        {
            Ok(())
        }
        TargetScan::Observed(observed)
            if item.envelope.current_full_hash.as_deref() == Some(&observed.full_hash)
                && item.envelope.current_managed_hash.as_deref()
                    == Some(&observed.managed_hash) =>
        {
            Ok(())
        }
        _ => Err(AppError::stale_preview("persisted", &item.target_id)),
    }
}
