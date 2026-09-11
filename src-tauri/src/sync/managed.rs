//! 受管资源同步的共用边界。
//!
//! MCP、Skills 与 Hooks 的原生投影仍由各自服务保留；本模块只承载三类资源
//! 都必须遵守的记录、row-version、项目 DTO、目标状态和重新接管事务合同。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, TransactionBehavior};
use serde_json::Value;

use crate::{
    adapters::{projection_value_at, TargetDescriptor},
    db::{
        hooks::{HookRecord, ManagedHookItemRecord},
        mcp::{ManagedMcpItemRecord, McpProjectRecord, McpServerRecord},
        skills::{ManagedSkillItemRecord, SkillProjectRecord, SkillRecord},
        Database,
    },
    domain::{ArtifactKind, HookEvent, ManagedProjectDto, SyncStatus, Tool},
    error::AppError,
};

use super::{
    DatabaseEntityType, DatabaseRowVersion, ManagedTargetBaseline, ObservedTarget, TargetScan,
};

/// 受管条目的最小记录合同。
pub(crate) trait ManagedItemRecord {
    fn id(&self) -> &str;
    fn resource_id(&self) -> &str;
    fn row_version(&self) -> i64;
}

/// 受管资源仓储与同步层之间的最小记录合同。
///
/// `current_item_hash` 接收 descriptor，是因为 MCP 的原生容器路径由 Adapter
/// 决定；Hooks 还需要使用 descriptor 的工具值选择事件树。`claimed` 让数组型
/// Hooks 在同一 matcher 组内逐条重新认领时保持与旧实现相同的“一条原生项只
/// 能被一个 managed item 认领”语义。
pub(crate) trait ManagedArtifact {
    const KIND: ArtifactKind;
    const ENTITY: DatabaseEntityType;
    type Record;
    type Item: ManagedItemRecord;

    fn record_id(record: &Self::Record) -> &str;
    fn row_version(record: &Self::Record) -> i64;
    fn current_item_hash(
        observed: &ObservedTarget,
        item: &Self::Item,
        descriptor: &TargetDescriptor,
        claimed: &mut BTreeSet<String>,
    ) -> Option<String>;
    fn global_tools(database: &Database, id: &str) -> Result<Vec<Tool>, AppError>;
    fn resource_row_versions(
        connection: &Connection,
        database_path: &str,
        ids: &[&str],
    ) -> Result<BTreeMap<String, i64>, AppError>;
}

/// Project 记录在三个资源仓储中的字段相同，但具体类型不同。
pub(crate) trait ManagedProjectRecord {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn root_path(&self) -> &str;
    fn codex_trust_status(&self) -> crate::domain::TrustStatus;
    fn row_version(&self) -> i64;
}

impl ManagedItemRecord for ManagedMcpItemRecord {
    fn id(&self) -> &str {
        &self.id
    }

    fn resource_id(&self) -> &str {
        &self.resource_id
    }

    fn row_version(&self) -> i64 {
        self.row_version
    }
}

impl ManagedItemRecord for ManagedSkillItemRecord {
    fn id(&self) -> &str {
        &self.id
    }

    fn resource_id(&self) -> &str {
        &self.resource_id
    }

    fn row_version(&self) -> i64 {
        self.row_version
    }
}

impl ManagedItemRecord for ManagedHookItemRecord {
    fn id(&self) -> &str {
        &self.id
    }

    fn resource_id(&self) -> &str {
        &self.resource_id
    }

    fn row_version(&self) -> i64 {
        self.row_version
    }
}

impl ManagedProjectRecord for McpProjectRecord {
    fn id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn root_path(&self) -> &str {
        &self.root_path
    }

    fn codex_trust_status(&self) -> crate::domain::TrustStatus {
        self.codex_trust_status
    }

    fn row_version(&self) -> i64 {
        self.row_version
    }
}

impl ManagedProjectRecord for SkillProjectRecord {
    fn id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn root_path(&self) -> &str {
        &self.root_path
    }

    fn codex_trust_status(&self) -> crate::domain::TrustStatus {
        self.codex_trust_status
    }

    fn row_version(&self) -> i64 {
        self.row_version
    }
}

impl ManagedArtifact for McpManagedArtifact {
    const KIND: ArtifactKind = ArtifactKind::Mcp;
    const ENTITY: DatabaseEntityType = DatabaseEntityType::McpServer;
    type Record = McpServerRecord;
    type Item = ManagedMcpItemRecord;

    fn record_id(record: &Self::Record) -> &str {
        &record.id
    }

    fn row_version(record: &Self::Record) -> i64 {
        record.row_version
    }

    fn current_item_hash(
        observed: &ObservedTarget,
        item: &Self::Item,
        descriptor: &TargetDescriptor,
        claimed: &mut BTreeSet<String>,
    ) -> Option<String> {
        let container = descriptor.mcp_container.as_ref()?;
        let container = container.iter().map(String::as_str).collect::<Vec<_>>();
        let value = projection_value_at(&observed.managed_projection, &container)
            .and_then(Value::as_object)
            .and_then(|items| items.get(&item.external_key))?;
        let _ = claimed;
        Some(super::hash_json(value))
    }

    fn global_tools(database: &Database, id: &str) -> Result<Vec<Tool>, AppError> {
        crate::db::mcp::global_tools_for_mcp(database, id)
    }

    fn resource_row_versions(
        connection: &Connection,
        database_path: &str,
        ids: &[&str],
    ) -> Result<BTreeMap<String, i64>, AppError> {
        crate::db::skills::row_versions_by_id(
            connection,
            database_path,
            "mcp_servers",
            ids,
            "mcp_row_versions",
        )
    }
}

impl ManagedArtifact for SkillManagedArtifact {
    const KIND: ArtifactKind = ArtifactKind::Skill;
    const ENTITY: DatabaseEntityType = DatabaseEntityType::Skill;
    type Record = SkillRecord;
    type Item = ManagedSkillItemRecord;

    fn record_id(record: &Self::Record) -> &str {
        &record.id
    }

    fn row_version(record: &Self::Record) -> i64 {
        record.row_version
    }

    fn current_item_hash(
        observed: &ObservedTarget,
        item: &Self::Item,
        _descriptor: &TargetDescriptor,
        claimed: &mut BTreeSet<String>,
    ) -> Option<String> {
        let value = observed
            .managed_projection
            .as_object()
            .and_then(|items| items.get(&item.external_key))?;
        let _ = claimed;
        Some(super::hash_json(value))
    }

    fn global_tools(database: &Database, id: &str) -> Result<Vec<Tool>, AppError> {
        crate::db::skills::global_tools_for_skill(database, id)
    }

    fn resource_row_versions(
        connection: &Connection,
        database_path: &str,
        ids: &[&str],
    ) -> Result<BTreeMap<String, i64>, AppError> {
        crate::db::skills::row_versions_by_id(
            connection,
            database_path,
            "skills",
            ids,
            "skill_row_versions",
        )
    }
}

impl ManagedArtifact for HookManagedArtifact {
    const KIND: ArtifactKind = ArtifactKind::Hook;
    const ENTITY: DatabaseEntityType = DatabaseEntityType::Hook;
    type Record = HookRecord;
    type Item = ManagedHookItemRecord;

    fn record_id(record: &Self::Record) -> &str {
        &record.id
    }

    fn row_version(record: &Self::Record) -> i64 {
        record.row_version
    }

    fn current_item_hash(
        observed: &ObservedTarget,
        item: &Self::Item,
        descriptor: &TargetDescriptor,
        claimed: &mut BTreeSet<String>,
    ) -> Option<String> {
        let mut key = item.external_key.splitn(3, '|');
        let canonical_event = key.next()?;
        let event = HookEvent::from_stable_str(canonical_event)?.native_key(descriptor.tool);
        let _identity = key.next()?;
        let matcher = key.next().unwrap_or_default();
        let events_path: &[&str] = match descriptor.tool {
            Tool::Zcode => &["hooks", "events"],
            _ => &["hooks"],
        };
        let events = projection_value_at(&observed.managed_projection, events_path)
            .and_then(Value::as_object)?;
        let groups = events.get(event)?.as_array()?;
        for group in groups {
            let group_matcher = group.get("matcher").and_then(Value::as_str);
            let entries: &[Value] = group
                .get("hooks")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_else(|| std::slice::from_ref(group));
            for (index, entry) in entries.iter().enumerate() {
                let entry_matcher = entry
                    .get("matcher")
                    .and_then(Value::as_str)
                    .or(group_matcher)
                    .unwrap_or_default();
                if entry_matcher != matcher {
                    continue;
                }
                let hash = super::hash_json(entry);
                if claimed.insert(format!("{canonical_event}|{matcher}|{index}")) {
                    return Some(hash);
                }
            }
        }
        None
    }

    fn global_tools(database: &Database, id: &str) -> Result<Vec<Tool>, AppError> {
        Ok(crate::db::hooks::global_assignments_for_hook(database, id)?
            .into_iter()
            .map(|(tool, _event)| tool)
            .collect())
    }

    fn resource_row_versions(
        connection: &Connection,
        database_path: &str,
        ids: &[&str],
    ) -> Result<BTreeMap<String, i64>, AppError> {
        crate::db::skills::row_versions_by_id(
            connection,
            database_path,
            "hooks",
            ids,
            "hook_row_versions",
        )
    }
}

pub(crate) struct McpManagedArtifact;
pub(crate) struct SkillManagedArtifact;
pub(crate) struct HookManagedArtifact;

/// RPC row_version 的唯一安全转换入口。
pub(crate) fn safe_row_version(value: i64) -> Result<u32, AppError> {
    u32::try_from(value).map_err(|error| {
        AppError::invalid_input("rowVersion", "数据库 row_version 超出 RPC 范围").with_source(error)
    })
}

/// 从受管记录读取 RPC 可用的版本号；服务层不再重复展开各记录的字段。
pub(crate) fn managed_record_row_version<A: ManagedArtifact>(
    record: &A::Record,
) -> Result<u32, AppError> {
    let _ = (A::KIND, A::record_id(record));
    safe_row_version(A::row_version(record))
}

/// 批量收集预览需要绑定的所有 row versions。
///
/// 资源记录和 managed item 来自当前准备阶段；已经解除分配但仍被旧
/// managed item 引用的资源，则由每个仓储通过一次 `WHERE id IN (...)` 查询补齐。
pub(crate) fn collect_row_versions<'a, A>(
    connection: &Connection,
    database_path: &str,
    project: Option<(&str, i64)>,
    records: impl Iterator<Item = &'a A::Record>,
    items: &[A::Item],
) -> Result<Vec<DatabaseRowVersion>, AppError>
where
    A: ManagedArtifact,
    A::Record: 'a,
{
    let mut versions = BTreeMap::<(DatabaseEntityType, String), u32>::new();
    if let Some((project_id, row_version)) = project {
        versions.insert(
            (DatabaseEntityType::Project, project_id.to_owned()),
            safe_row_version(row_version)?,
        );
    }
    for record in records {
        versions.insert(
            (A::ENTITY, A::record_id(record).to_owned()),
            safe_row_version(A::row_version(record))?,
        );
    }
    for item in items {
        versions.insert(
            (DatabaseEntityType::ManagedItem, item.id().to_owned()),
            safe_row_version(item.row_version())?,
        );
    }
    let missing = items
        .iter()
        .map(ManagedItemRecord::resource_id)
        .filter(|id| !versions.contains_key(&(A::ENTITY, (*id).to_owned())))
        .collect::<BTreeSet<_>>();
    let missing_ids = missing.iter().copied().collect::<Vec<_>>();
    for (id, row_version) in A::resource_row_versions(connection, database_path, &missing_ids)? {
        versions.insert((A::ENTITY, id), safe_row_version(row_version)?);
    }
    Ok(versions
        .into_iter()
        .map(
            |((entity_type, entity_id), row_version)| DatabaseRowVersion {
                entity_type,
                entity_id,
                row_version,
            },
        )
        .collect())
}

/// 重新接管目标时返回的资源无关结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadoptOutcome {
    pub target_path: String,
    pub updated_item_count: u32,
    pub removed_item_count: u32,
}

/// 三类受管目标共用的基线刷新事务。
pub(crate) fn readopt_with_scan<A: ManagedArtifact>(
    database: &mut Database,
    baseline: &ManagedTargetBaseline,
    existing_items: &[A::Item],
    scan: &TargetScan,
    descriptor: &TargetDescriptor,
) -> Result<ReadoptOutcome, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| AppError::database(&database_path, "begin_readopt").with_source(error))?;
    let (updated_items, removed_items) = match scan {
        TargetScan::Observed(observed) => {
            crate::db::sync::update_readopt_target_baseline(
                &transaction,
                &baseline.target_id,
                &observed.full_hash,
                &observed.managed_hash,
                &database_path,
            )?;
            let mut claimed = BTreeSet::new();
            let mut updated = 0u32;
            let mut removed = 0u32;
            for item in existing_items {
                match A::current_item_hash(observed, item, descriptor, &mut claimed) {
                    Some(hash) => {
                        crate::db::sync::update_readopt_item_baseline(
                            &transaction,
                            item.id(),
                            &baseline.target_id,
                            &hash,
                            &database_path,
                        )?;
                        updated += 1;
                    }
                    None => {
                        crate::db::sync::delete_readopt_item(
                            &transaction,
                            item.id(),
                            &baseline.target_id,
                            &database_path,
                        )?;
                        removed += 1;
                    }
                }
            }
            (updated, removed)
        }
        TargetScan::Missing => {
            crate::db::sync::clear_readopt_items(
                &transaction,
                &baseline.target_id,
                &database_path,
            )?;
            crate::db::sync::clear_readopt_target_baseline(
                &transaction,
                &baseline.target_id,
                &database_path,
            )?;
            (0, existing_items.len().min(u32::MAX as usize) as u32)
        }
        _ => {
            return Err(AppError::conflict(
                "readopt",
                "目标当前无法安全读取，请先恢复文件内容或权限后再重新接管",
            ));
        }
    };
    transaction
        .commit()
        .map_err(|error| AppError::database(&database_path, "commit_readopt").with_source(error))?;
    Ok(ReadoptOutcome {
        target_path: descriptor.path.clone().unwrap_or_default(),
        updated_item_count: updated_items,
        removed_item_count: removed_items,
    })
}

/// 受管目标状态回调需要返回的“可用状态”投影；能力/策略阻断由共用 helper
/// 统一处理，资源服务只负责它自己的基线与内容诊断。
pub(crate) type AvailableTargetStatus = (SyncStatus, Option<String>);

pub(crate) fn list_global_target_statuses<A, D, S>(
    database: &Database,
    tools: impl IntoIterator<Item = Tool>,
    mut descriptor_for: D,
    mut status_for: S,
) -> Result<Vec<crate::domain::ManagedTargetStatusDto>, AppError>
where
    A: ManagedArtifact,
    D: FnMut(Tool) -> Result<TargetDescriptor, AppError>,
    S: FnMut(&Database, Tool, &TargetDescriptor) -> Result<Option<AvailableTargetStatus>, AppError>,
{
    let _artifact_kind = A::KIND;
    tools
        .into_iter()
        .map(|tool| {
            let descriptor = descriptor_for(tool)?;
            let target_path = descriptor.path.clone();
            let persisted = if target_path.is_some() {
                status_for(database, tool, &descriptor)?
            } else {
                None
            };
            let (status, diagnostic_code) =
                if descriptor.capability.state != crate::adapters::CapabilityState::Supported {
                    (
                        SyncStatus::Failed,
                        descriptor.capability.diagnostic_code.clone(),
                    )
                } else if descriptor.policy != crate::adapters::PolicyState::Allowed {
                    let diagnostic_code = match descriptor.policy {
                        crate::adapters::PolicyState::Blocked => "CLAUDE_POLICY_BLOCKED",
                        crate::adapters::PolicyState::Unknown => super::ERROR_CLAUDE_POLICY_UNKNOWN,
                        crate::adapters::PolicyState::Allowed => {
                            return Err(AppError::internal("allowed 策略不应进入阻断分支"))
                        }
                    };
                    (SyncStatus::PolicyBlocked, Some(diagnostic_code.to_owned()))
                } else {
                    persisted.unwrap_or((SyncStatus::Missing, None))
                };
            Ok(crate::domain::ManagedTargetStatusDto {
                tool,
                project_id: None,
                target_path,
                status,
                diagnostic_code,
            })
        })
        .collect()
}

pub(crate) fn project_dto<P: ManagedProjectRecord>(
    project: &P,
) -> Result<ManagedProjectDto, AppError> {
    Ok(ManagedProjectDto {
        id: project.id().to_owned(),
        display_name: project.display_name().to_owned(),
        root_path: project.root_path().to_owned(),
        codex_trust_status: project.codex_trust_status(),
        row_version: safe_row_version(project.row_version())?,
    })
}

#[cfg(test)]
mod tests {
    use super::{HookManagedArtifact, ManagedArtifact, ManagedHookItemRecord};
    use crate::{
        adapters::{ObservedDocument, TargetDescriptor},
        domain::{ArtifactKind, Scope, TargetType, Tool},
        sync::{hash_json, ObservedTarget},
    };
    use serde_json::json;
    use std::collections::BTreeSet;

    #[test]
    fn cursor_hook_readopt_uses_native_event_key() {
        let entry = json!({"command": "bash hook.sh", "timeout": 30});
        let observed = ObservedTarget {
            target_type: TargetType::File,
            full_hash: "full".to_owned(),
            managed_hash: "managed".to_owned(),
            managed_projection: json!({
                "version": 1,
                "hooks": {"stop": [entry.clone()]}
            }),
            document: ObservedDocument::Json(json!({})),
        };
        let descriptor =
            TargetDescriptor::builder(Tool::Cursor, ArtifactKind::Hook, Scope::Global).build();
        let item = ManagedHookItemRecord {
            id: "item".to_owned(),
            resource_id: "hook".to_owned(),
            external_key: "Stop|identity|".to_owned(),
            last_applied_item_hash: "old".to_owned(),
            row_version: 1,
        };
        let mut claimed = BTreeSet::new();

        let hash = <HookManagedArtifact as ManagedArtifact>::current_item_hash(
            &observed,
            &item,
            &descriptor,
            &mut claimed,
        );

        assert_eq!(hash, Some(hash_json(&entry)));
    }
}
