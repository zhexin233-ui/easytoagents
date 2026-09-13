// ---------------------------------------------------------------------------
// Descriptor / 投影 / 基线基础设施
// ---------------------------------------------------------------------------

/// ZCode 项目级子代理的稳定诊断码（官方明示不支持）。
pub(super) const ZCODE_PROJECT_AGENTS_UNSUPPORTED: &str = "ZCODE_PROJECT_AGENTS_UNSUPPORTED";

fn discovery_context<'a>(
    environment: &'a crate::adapters::ExplicitEnvironment,
    project_root: Option<&'a ProjectRoot>,
) -> DiscoveryContext<'a> {
    DiscoveryContext {
        environment,
        project_root,
        claude_user_mcp_probe: environment.claude_user_mcp_probe(),
        claude_customization_policy_probe: environment.claude_customization_policy_probe(),
    }
}

/// 目录 descriptor（不做能力门禁）；状态聚合需要拿到 Unsupported 诊断码。
pub(super) fn find_agent_descriptor(
    environment: &crate::adapters::ExplicitEnvironment,
    tool: Tool,
    project_root: Option<&ProjectRoot>,
) -> Result<TargetDescriptor, AppError> {
    find_descriptor(
        tool,
        &discovery_context(environment, project_root),
        ArtifactKind::Agent,
        if project_root.is_some() {
            Scope::Project
        } else {
            Scope::Global
        },
        "agentTarget",
        tool.as_str(),
    )
}

/// 能力门禁：Unsupported / ToolNotInstalled 直接返回对应诊断，不产生目标。
fn ensure_agents_supported(descriptor: &TargetDescriptor) -> Result<(), AppError> {
    match descriptor.capability.state {
        crate::adapters::CapabilityState::Supported => Ok(()),
        crate::adapters::CapabilityState::ToolNotInstalled => Err(AppError::not_found(
            "toolInstallation",
            descriptor.tool.as_str(),
        )),
        crate::adapters::CapabilityState::Unsupported => {
            // reason 只接受 'static 字面量；把稳定诊断码映射成常量。
            let code: &'static str = match descriptor.capability.diagnostic_code.as_deref() {
                Some("ZCODE_PROJECT_AGENTS_UNSUPPORTED") => "ZCODE_PROJECT_AGENTS_UNSUPPORTED",
                Some("OPENCODE_DISCOVERY_DISABLED") => "OPENCODE_DISCOVERY_DISABLED",
                Some("OPENCODE_CONFIG_CONTENT_OVERRIDE") => "OPENCODE_CONFIG_CONTENT_OVERRIDE",
                Some("OPENCODE_INSTALLATION_PROBE_UNSUPPORTED") => {
                    "OPENCODE_INSTALLATION_PROBE_UNSUPPORTED"
                }
                Some("CLAUDE_INSTALLATION_PROBE_UNSUPPORTED") => {
                    "CLAUDE_INSTALLATION_PROBE_UNSUPPORTED"
                }
                Some("CODEX_INSTALLATION_PROBE_UNSUPPORTED") => {
                    "CODEX_INSTALLATION_PROBE_UNSUPPORTED"
                }
                Some("CURSOR_INSTALLATION_PROBE_UNSUPPORTED") => {
                    "CURSOR_INSTALLATION_PROBE_UNSUPPORTED"
                }
                Some("ZCODE_INSTALLATION_PROBE_UNSUPPORTED") => {
                    "ZCODE_INSTALLATION_PROBE_UNSUPPORTED"
                }
                _ => "AGENTS_UNSUPPORTED",
            };
            Err(AppError::invalid_input("capability", code))
        }
    }
}

/// 目录 descriptor + 能力门禁。后续所有 scan / preview / apply / restore
/// 只处理由 `for_agent_file` 派生的文件级 descriptor。
pub(super) fn agent_directory_descriptor(
    environment: &crate::adapters::ExplicitEnvironment,
    tool: Tool,
    project_root: Option<&ProjectRoot>,
) -> Result<TargetDescriptor, AppError> {
    let descriptor = find_agent_descriptor(environment, tool, project_root)?;
    ensure_agents_supported(&descriptor)?;
    Ok(descriptor)
}

/// 由既有受管目标行还原文件级 descriptor；target_path 必须位于该工具的
/// agents 目录内（与 `for_agent_file` 的派生结果一致），否则拒绝。
pub(super) fn agent_file_descriptor(
    directory_descriptor: &TargetDescriptor,
    target_path: &str,
    tool: Tool,
) -> Result<TargetDescriptor, AppError> {
    let name = file_stem_of(target_path)
        .ok_or_else(|| AppError::invalid_input("targetPath", "受管 agent 目标路径缺少文件名"))?;
    let derived = directory_descriptor.for_agent_file(&name, agent_file_extension(tool))?;
    if derived.path.as_deref() != Some(target_path) {
        return Err(AppError::invalid_input(
            "targetPath",
            "受管 agent 目标路径不在该工具的 agents 目录内",
        ));
    }
    Ok(derived)
}

fn file_stem_of(target_path: &str) -> Option<String> {
    Path::new(target_path)
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_owned)
}

impl AgentManagedTargetRecord {
    fn to_baseline(&self) -> Result<ManagedTargetBaseline, AppError> {
        Ok(ManagedTargetBaseline {
            target_id: self.id.clone(),
            target_row_version: self.row_version,
            full_hash: self.baseline_full_hash.clone(),
            managed_hash: self.baseline_managed_hash.clone(),
        })
    }
}

/// 预览需要绑定的 row versions：项目（如有）+ 本次涉及的 agent 记录。
/// 目标行自身的版本由 build_preview_plan 自动绑定。
fn agent_row_versions<'a>(
    project: Option<&McpProjectRecord>,
    records: impl Iterator<Item = &'a AgentRecord>,
) -> Result<Vec<DatabaseRowVersion>, AppError> {
    let mut versions = Vec::new();
    if let Some(project) = project {
        versions.push(DatabaseRowVersion {
            entity_type: DatabaseEntityType::Project,
            entity_id: project.id.clone(),
            row_version: safe_row_version(project.row_version)?,
        });
    }
    for record in records {
        versions.push(DatabaseRowVersion {
            entity_type: DatabaseEntityType::Agent,
            entity_id: record.id.clone(),
            row_version: safe_row_version(record.row_version)?,
        });
    }
    Ok(versions)
}

/// 找到或创建受管目标行（一个受管文件 = 一行 managed_targets）。
fn ensure_agent_target(
    database: &mut Database,
    descriptor: &TargetDescriptor,
    project: Option<&McpProjectRecord>,
) -> Result<ManagedTargetBaseline, AppError> {
    let target_path = descriptor
        .path
        .as_deref()
        .ok_or_else(|| AppError::not_found("agentTarget", descriptor.tool.as_str()))?;
    let database_path = database.path().to_string_lossy().into_owned();
    let project_id = project.map(|project| project.id.as_str());
    let existing = find_agent_target_baseline(database, descriptor, project_id)?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let id = Uuid::new_v4().to_string();
    database
        .connection_mut()
        .execute(
            "INSERT INTO managed_targets(
                id, tool, artifact_kind, scope, project_id, target_path
             ) VALUES (?1, ?2, 'agent', ?3, ?4, ?5)",
            params![
                id,
                descriptor.tool.as_str(),
                descriptor.scope.as_str(),
                project_id,
                target_path,
            ],
        )
        .map_err(|error| {
            AppError::database(&database_path, "insert_agent_managed_target").with_source(error)
        })?;
    load_managed_target_baseline(database, &id)
}

/// 当原生文件的交集字段与中央 Agent 一致时，首次分配可以安全地把当前
/// 内容登记为基线。这个动作只写数据库，不改原生文件；后续真正的外部改写
/// 仍会按完整文档漂移阻断。
pub(super) fn adopt_initial_agent_baseline(
    database: &mut Database,
    baseline: &ManagedTargetBaseline,
    observed: &crate::sync::ObservedTarget,
) -> Result<ManagedTargetBaseline, AppError> {
    if baseline.full_hash.is_some() || baseline.managed_hash.is_some() {
        return Ok(baseline.clone());
    }
    let database_path = database.path().to_string_lossy().into_owned();
    let projection = serde_json::to_string(&observed.managed_projection).map_err(|error| {
        AppError::database(&database_path, "serialize_initial_agent_baseline")
            .with_source(error)
    })?;
    let expected_row_version = safe_row_version(baseline.target_row_version)?;
    let updated = crate::db::sync::update_managed_target_baseline(
        database.connection(),
        &baseline.target_id,
        Some(&observed.full_hash),
        Some(&observed.managed_hash),
        &projection,
        expected_row_version,
        &database_path,
    )?;
    if updated != 1 {
        return Err(AppError::conflict(
            "agentTarget",
            "首次接管 Agent 基线时目标已被其他操作更新",
        ));
    }
    load_managed_target_baseline(database, &baseline.target_id)
}

pub(super) fn find_agent_target_baseline(
    database: &Database,
    descriptor: &TargetDescriptor,
    project_id: Option<&str>,
) -> Result<Option<ManagedTargetBaseline>, AppError> {
    let Some(target_path) = descriptor.path.as_deref() else {
        return Ok(None);
    };
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, row_version, baseline_full_hash, baseline_managed_hash
             FROM managed_targets
             WHERE tool = ?1 AND artifact_kind = 'agent' AND scope = ?2
               AND ifnull(project_id, '') = ifnull(?3, '') AND target_path = ?4",
            params![
                descriptor.tool.as_str(),
                descriptor.scope.as_str(),
                project_id,
                target_path,
            ],
            |row| {
                Ok(ManagedTargetBaseline {
                    target_id: row.get(0)?,
                    target_row_version: row.get(1)?,
                    full_hash: row.get(2)?,
                    managed_hash: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            AppError::database(&database_path, "find_agent_managed_target").with_source(error)
        })
}

/// 判断一个尚未登记基线的原生文件，是否至少在中央交集字段上代表同一个
/// Agent。匹配成功时首次同步沿用原文件投影，保留导入时的未知字段与排版；
/// 后续中央编辑再回到确定性的中央投影。
pub(super) fn agent_observed_matches_central(
    tool: Tool,
    descriptor: &TargetDescriptor,
    record: &AgentRecord,
    settings: Option<&Value>,
    observed: &crate::sync::ObservedTarget,
) -> bool {
    match (tool, observed.document()) {
        (Tool::Claude | Tool::Cursor | Tool::Zcode | Tool::Opencode,
         crate::adapters::ObservedDocument::Markdown(text)) => {
            let Ok(parsed) = parse_markdown_agent_file(text, tool) else {
                return false;
            };
            let fallback_name = descriptor.path.as_deref().and_then(file_stem_of);
            let name = parsed.name.or(fallback_name);
            if name.as_deref() != Some(record.name.as_str())
                || parsed.description.as_deref() != Some(record.description.as_str())
                || parsed.prompt != record.prompt
            {
                return false;
            }
            if tool != Tool::Claude {
                return settings.is_none();
            }
            let Ok(normalized) = validate_agent_tool_settings(
                Tool::Claude,
                &Value::Object(parsed.retained),
            ) else {
                return false;
            };
            normalized.map(|value| value.value) == settings.cloned()
        }
        (Tool::Codex, crate::adapters::ObservedDocument::Toml { semantic, .. }) => {
            let Some(object) = semantic.as_object() else {
                return false;
            };
            if object.get("name").and_then(Value::as_str) != Some(record.name.as_str())
                || object.get("description").and_then(Value::as_str)
                    != Some(record.description.as_str())
                || object.get("developer_instructions").and_then(Value::as_str)
                    != Some(record.prompt.as_str())
            {
                return false;
            }
            let mut retained = serde_json::Map::new();
            for (source_key, central_key) in [
                ("model", "model"),
                ("model_reasoning_effort", "modelReasoningEffort"),
                ("features", "features"),
            ] {
                if let Some(value) = object.get(source_key) {
                    retained.insert(central_key.to_owned(), value.clone());
                }
            }
            let Ok(normalized) =
                validate_agent_tool_settings(Tool::Codex, &Value::Object(retained))
            else {
                return false;
            };
            normalized.map(|value| value.value) == settings.cloned()
        }
        _ => false,
    }
}

fn canonical_project(path: &str) -> Result<ProjectRoot, AppError> {
    let canonical = canonicalize_project_root(Path::new(path))?;
    if canonical.as_str() != path {
        return Err(AppError::conflict(
            "projectRoot",
            "登记项目根与当前 canonical 路径不一致",
        ));
    }
    Ok(canonical)
}

// ---------------------------------------------------------------------------
// DTO 辅助
// ---------------------------------------------------------------------------

fn agent_dto(database: &Database, record: &AgentRecord) -> Result<AgentDto, AppError> {
    agent_dto_with_assignments(
        record,
        repository::global_assignments_for_agent(database, &record.id)?,
        repository::tool_settings_for_agent(database, &record.id)?,
    )
}

fn agent_dto_with_assignments(
    record: &AgentRecord,
    assignments: Vec<Tool>,
    tool_settings: BTreeMap<Tool, Value>,
) -> Result<AgentDto, AppError> {
    Ok(AgentDto {
        id: record.id.clone(),
        name: record.name.clone(),
        description: record.description.clone(),
        prompt: record.prompt.clone(),
        enabled: record.enabled,
        global_assignments: assignments,
        tool_settings: tool_settings_dto(&tool_settings)?,
        // 整文件目标没有 managed items，行版本直接取中央记录。
        row_version: safe_row_version(record.row_version)?,
    })
}

// ---------------------------------------------------------------------------
// 原生投影（整文件内容，确定性输出）
// ---------------------------------------------------------------------------

/// 各工具原生投影（整文件内容）：
/// - claude / cursor / zcode：YAML frontmatter（name + description；Claude
///   另合并白名单覆盖层），由 serde_yaml_ng 序列化 BTreeMap，键序确定并自动
///   处理引号与多行 + 正文；
/// - opencode：frontmatter 为 description + `mode: subagent`，不写 name
///   （OpenCode 以文件名为名；固定 subagent 模式避免出现在主代理切换列表）；
/// - codex：TOML 三字段 name / description / developer_instructions，
///   由 `TargetFormat::Toml + WholeDocument` 渲染分支输出（阶段 0 已核验
///   多行字符串与键序的确定性）。
///
/// 重复渲染字节一致，保证漂移判定稳定。
pub(super) fn build_agent_projection(
    tool: Tool,
    record: &AgentRecord,
    settings: Option<&Value>,
) -> Result<Value, AppError> {
    match tool {
        Tool::Claude | Tool::Cursor | Tool::Zcode => {
            let mut frontmatter = BTreeMap::<&str, String>::new();
            frontmatter.insert("name", record.name.clone());
            frontmatter.insert("description", record.description.clone());
            if tool == Tool::Claude {
                if let Some(settings) = settings.and_then(Value::as_object) {
                    if let Some(model) = settings.get("model").and_then(Value::as_str) {
                        frontmatter.insert("model", model.to_owned());
                    }
                    if let Some(color) = settings.get("color").and_then(Value::as_str) {
                        frontmatter.insert("color", color.to_owned());
                    }
                    if let Some(tools) = settings.get("tools").and_then(Value::as_array) {
                        if !tools.is_empty() {
                            let rendered = tools
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", ");
                            if !rendered.is_empty() {
                                frontmatter.insert("tools", rendered);
                            }
                        }
                    }
                }
            }
            let yaml = serde_yaml_ng::to_string(&frontmatter).map_err(|error| {
                AppError::internal("Agent frontmatter 序列化失败").with_source(error)
            })?;
            Ok(Value::String(render_markdown_agent_file(
                &yaml,
                &record.prompt,
            )))
        }
        Tool::Opencode => {
            let mut frontmatter = BTreeMap::<&str, String>::new();
            frontmatter.insert("description", record.description.clone());
            frontmatter.insert("mode", "subagent".to_owned());
            let yaml = serde_yaml_ng::to_string(&frontmatter).map_err(|error| {
                AppError::internal("Agent frontmatter 序列化失败").with_source(error)
            })?;
            Ok(Value::String(render_markdown_agent_file(
                &yaml,
                &record.prompt,
            )))
        }
        Tool::Codex => {
            let mut object = serde_json::Map::new();
            object.insert("name".to_owned(), Value::String(record.name.clone()));
            object.insert(
                "description".to_owned(),
                Value::String(record.description.clone()),
            );
            object.insert(
                "developer_instructions".to_owned(),
                Value::String(record.prompt.clone()),
            );
            if let Some(settings) = settings.and_then(Value::as_object) {
                if let Some(model) = settings.get("model").and_then(Value::as_str) {
                    object.insert("model".to_owned(), Value::String(model.to_owned()));
                }
                if let Some(effort) = settings
                    .get("modelReasoningEffort")
                    .and_then(Value::as_str)
                {
                    object.insert(
                        "model_reasoning_effort".to_owned(),
                        Value::String(effort.to_owned()),
                    );
                }
                if let Some(features) = settings.get("features") {
                    object.insert("features".to_owned(), features.clone());
                }
            }
            Ok(Value::Object(object))
        }
    }
}

/// Markdown 系文件渲染：`---\n<yaml>---\n\n<正文>`；正文末尾保证单个换行。
/// yaml 字符串自带结尾换行，因此闭合 `---` 顶行书写。
fn render_markdown_agent_file(frontmatter_yaml: &str, prompt: &str) -> String {
    let prompt = format!("{}\n", prompt.trim_end_matches(['\n', '\r']));
    format!("---\n{frontmatter_yaml}---\n\n{prompt}")
}

/// 停用 / 取消分配后的空投影：触发 change_kind = Delete（配合
/// delete_target = true，Apply 走 Mutation::Remove 删除整个受管文件）。
fn empty_projection(tool: Tool) -> Value {
    match tool {
        Tool::Codex => Value::Object(serde_json::Map::new()),
        _ => Value::String(String::new()),
    }
}

// ---------------------------------------------------------------------------
// 导入解析（只读；供 import.rs 复用）
// ---------------------------------------------------------------------------

/// 导入候选解析失败的稳定诊断码。
pub(super) const AGENT_FRONTMATTER_INVALID: &str = "AGENT_FRONTMATTER_INVALID";
pub(super) const AGENT_REQUIRED_FIELD_MISSING: &str = "AGENT_REQUIRED_FIELD_MISSING";
pub(super) const AGENT_NAME_INVALID: &str = "AGENT_NAME_INVALID";
pub(super) const AGENT_FIELD_INVALID: &str = "AGENT_FIELD_INVALID";
pub(super) const AGENT_NAME_CONFLICT: &str = "AGENT_NAME_CONFLICT";
pub(super) const AGENT_FILE_TOO_LARGE: &str = "AGENT_FILE_TOO_LARGE";

/// 单个原生 agent 文件的解析结果。
pub(super) struct ParsedAgentFile {
    /// frontmatter / TOML 中的 name；None = 缺省（Markdown 系回退文件名）。
    pub name: Option<String>,
    pub description: Option<String>,
    pub prompt: String,
    /// 将被交集投影丢弃的工具特有键名（知情丢弃）。
    pub dropped_fields: Vec<String>,
    /// 当前工具首期白名单内可保留的原始字段（已转换为中央 DTO 的键名）。
    pub retained: serde_json::Map<String, Value>,
}

/// Markdown 系（claude/cursor/zcode/opencode）agent 文件解析：
/// 按 `---` 分隔 frontmatter，serde_yaml_ng 解析为 Mapping；name 缺省取
/// 文件名去扩展名。frontmatter 缺失不算解析失败（交给必填字段校验），
/// YAML 无法解析才算。
pub(super) fn parse_markdown_agent_file(
    text: &str,
    tool: Tool,
) -> Result<ParsedAgentFile, &'static str> {
    match split_frontmatter(text)? {
        Some((frontmatter, body)) => {
            let value: serde_yaml_ng::Value =
                serde_yaml_ng::from_str(frontmatter).map_err(|_| AGENT_FRONTMATTER_INVALID)?;
            let mapping = match value {
                serde_yaml_ng::Value::Mapping(mapping) => mapping,
                serde_yaml_ng::Value::Null => serde_yaml_ng::Mapping::new(),
                _ => return Err(AGENT_FRONTMATTER_INVALID),
            };
            let string_field = |key: &str| -> Result<Option<String>, &'static str> {
                match mapping.get(serde_yaml_ng::Value::String(key.to_owned())) {
                    None => Ok(None),
                    Some(value) => value
                        .as_str()
                        .map(|value| Some(value.trim().to_owned()))
                        .ok_or(AGENT_FIELD_INVALID),
                }
            };
            let mut dropped_fields = Vec::new();
            let mut retained = serde_json::Map::new();
            for key in mapping.keys() {
                if let Some(key) = key.as_str() {
                    if key == "name" || key == "description" {
                        continue;
                    }
                    if tool == Tool::Claude && matches!(key, "model" | "color" | "tools") {
                        let Some(value) = mapping.get(serde_yaml_ng::Value::String(key.to_owned())) else {
                            continue;
                        };
                        match key {
                            "model" | "color" => {
                                let string = value.as_str().ok_or(AGENT_FIELD_INVALID)?;
                                retained.insert(
                                    key.to_owned(),
                                    Value::String(string.trim().to_owned()),
                                );
                            }
                            "tools" => {
                                let values = match value {
                                    serde_yaml_ng::Value::String(string) => {
                                        if string.trim().is_empty() {
                                            Vec::new()
                                        } else {
                                            string
                                                .split(',')
                                                .map(|item| Value::String(item.trim().to_owned()))
                                                .collect()
                                        }
                                    }
                                    serde_yaml_ng::Value::Sequence(values) => values
                                        .iter()
                                        .map(|item| {
                                            item.as_str()
                                                .map(|item| Value::String(item.trim().to_owned()))
                                                .ok_or(AGENT_FIELD_INVALID)
                                        })
                                        .collect::<Result<Vec<_>, _>>()?,
                                    _ => return Err(AGENT_FIELD_INVALID),
                                };
                                retained.insert("tools".to_owned(), Value::Array(values));
                            }
                            _ => unreachable!(),
                        }
                    } else {
                        dropped_fields.push(key.to_owned());
                    }
                }
            }
            dropped_fields.sort();
            Ok(ParsedAgentFile {
                name: string_field("name")?,
                description: string_field("description")?,
                prompt: body.trim().to_owned(),
                dropped_fields,
                retained,
            })
        }
        None => Ok(ParsedAgentFile {
            name: None,
            description: None,
            prompt: text.trim().to_owned(),
            dropped_fields: Vec::new(),
            retained: serde_json::Map::new(),
        }),
    }
}

/// Codex TOML agent 文件解析：name / description / developer_instructions
/// 必填；其余键为 dropped_fields。
pub(super) fn parse_codex_agent_file(text: &str) -> Result<ParsedAgentFile, &'static str> {
    let parsed: BTreeMap<String, Value> =
        toml_edit::de::from_str(text).map_err(|_| AGENT_FRONTMATTER_INVALID)?;
    let string_field = |key: &str| -> Result<Option<String>, &'static str> {
        match parsed.get(key) {
            None => Ok(None),
            Some(value) => value
                .as_str()
                .map(|value| Some(value.trim().to_owned()))
                .ok_or(AGENT_FIELD_INVALID),
        }
    };
    let mut dropped_fields = Vec::new();
    let mut retained = serde_json::Map::new();
    for key in parsed.keys() {
        if matches!(key.as_str(), "name" | "description" | "developer_instructions") {
            continue;
        }
        match key.as_str() {
            "model" | "model_reasoning_effort" => {
                let value = string_field(key)?.ok_or(AGENT_FIELD_INVALID)?;
                let central_key = if key == "model_reasoning_effort" {
                    "modelReasoningEffort"
                } else {
                    "model"
                };
                retained.insert(central_key.to_owned(), Value::String(value));
            }
            "features" => {
                let Some(value) = parsed.get(key) else {
                    continue;
                };
                let Some(object) = value.as_object() else {
                    dropped_fields.push(key.clone());
                    continue;
                };
                if object.values().all(Value::is_boolean) {
                    retained.insert("features".to_owned(), value.clone());
                } else {
                    // Codex 官方允许 features.network_proxy 表值；本任务不建模，
                    // 因而含任意非布尔值时整体知情丢弃，而非拒绝整份文件。
                    dropped_fields.push(key.clone());
                }
            }
            _ => dropped_fields.push(key.clone()),
        }
    }
    Ok(ParsedAgentFile {
        name: Some(
            string_field("name")?.ok_or(AGENT_REQUIRED_FIELD_MISSING)?,
        ),
        description: Some(
            string_field("description")?.ok_or(AGENT_REQUIRED_FIELD_MISSING)?,
        ),
        prompt: string_field("developer_instructions")?
            .ok_or(AGENT_REQUIRED_FIELD_MISSING)?,
        dropped_fields,
        retained,
    })
}

/// 切分 Markdown frontmatter：`---\n...\n---\n`。无 frontmatter 返回 None；
/// 只有起始没有闭合返回 frontmatter 无效诊断。
fn split_frontmatter(text: &str) -> Result<Option<(&str, &str)>, &'static str> {
    let Some(rest) = text
        .strip_prefix("---\r\n")
        .or_else(|| text.strip_prefix("---\n"))
    else {
        return Ok(None);
    };
    let mut scanned = 0usize;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        scanned += line.len();
        if trimmed == "---" {
            let frontmatter = &rest[..scanned - line.len()];
            let body = &rest[scanned..];
            return Ok(Some((frontmatter, body)));
        }
    }
    Err(AGENT_FRONTMATTER_INVALID)
}
