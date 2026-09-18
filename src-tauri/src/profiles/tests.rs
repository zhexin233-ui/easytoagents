#[cfg(test)]
mod tests {
    use std::{fs, sync::Mutex};

    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        adopt_prompt_native, adopt_provider_native, apply_profile_preview, confirm_prompt_import,
        confirm_provider_import, copy_provider_profile, create_prompt_profile,
        create_provider_profile, discover_prompt_import, discover_provider_import,
        get_tool_profile_status, list_provider_profiles, preview_prompt_sync,
        preview_provider_sync, readopt_provider_target, set_active_provider_profile,
        set_global_prompt_assignment, update_prompt_profile, update_provider_profile,
        AdoptProviderNativeInput, ConfirmProviderImportInput, CopyProviderProfileInput,
        PromptProfileDto, PromptProfileInput, ProviderAuthKind, ProviderImportCandidateDto,
        ProviderImportCandidateStatus, ProviderImportPreviewDto, ProviderOptionsInput,
        ProviderProfileDto, ProviderProfileInput, ReadoptProviderTargetInput,
        SetGlobalPromptAssignmentInput, UpdatePromptProfileInput, UpdateProviderProfileInput,
        CLAUDE_MODEL_KEY,
    };
    use crate::{
        adapters::{
            CapabilityState, ExplicitEnvironment, PolicyState, ToolAvailability,
            ToolAvailabilityState,
        },
        app::AppPaths,
        db::Database,
        domain::{ArtifactKind, SyncScopeDto, Tool},
        error::AppError,
        profiles::{
            AdoptPromptNativeInput, ConfirmImportInput, ConfirmProviderImportItem, SecretUpdate,
        },
        security::SecretRedactor,
    };

    struct Fixture {
        _temporary: tempfile::TempDir,
        home: std::path::PathBuf,
        paths: AppPaths,
        database: Database,
        environment: ExplicitEnvironment,
    }

    fn fixture() -> Fixture {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        fs::create_dir(home.join(".claude")).unwrap();
        fs::create_dir(home.join(".codex")).unwrap();
        fs::create_dir_all(home.join(".zcode/v2")).unwrap();
        let paths = AppPaths::from_data_root(home.join("private/app/data")).unwrap();
        let database = Database::open(&paths).unwrap();
        let environment =
            ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed())
                .unwrap()
                .with_claude_provider_policy(PolicyState::Allowed);
        Fixture {
            _temporary: temporary,
            home,
            paths,
            database,
            environment,
        }
    }

    /// 新建档案并立即对指定工具启用（替代旧 activate 入参的测试夹具）。
    fn create_enabled_prompt(
        fixture: &mut Fixture,
        tool: Tool,
        name: &str,
        body: &str,
    ) -> PromptProfileDto {
        let profile = create_prompt_profile(
            &mut fixture.database,
            PromptProfileInput {
                name: name.to_owned(),
                body: body.to_owned(),
            },
        )
        .unwrap();
        set_global_prompt_assignment(
            &mut fixture.database,
            &SetGlobalPromptAssignmentInput {
                tool,
                prompt_profile_id: profile.id.clone(),
                assigned: true,
                row_version: profile.row_version,
            },
        )
        .unwrap()
    }

    fn prompt_native_input(plan: &crate::sync::PreviewPlan, tool: Tool) -> AdoptPromptNativeInput {
        let target = plan.targets.first().expect("Prompt 预览应包含目标");
        AdoptPromptNativeInput {
            tool,
            target_id: target.target_id.clone(),
            target_row_version: target.target_row_version,
            target_path: target
                .descriptor
                .path
                .clone()
                .expect("Prompt 目标应包含路径"),
            row_versions: target.row_versions.clone(),
            observed_full_hash: target.current_full_hash.clone(),
            observed_managed_hash: target.current_managed_hash.clone(),
        }
    }

    fn provider(tool: Tool, name: &str, key: &str, activate: bool) -> ProviderProfileInput {
        ProviderProfileInput {
            tool,
            name: name.to_owned(),
            api_base_url: "https://provider.example.com/v1".to_owned(),
            api_key: key.to_owned(),
            default_model: "fixture-model".to_owned(),
            options: ProviderOptionsInput::default(),
            activate,
        }
    }

    /// 检测结果里唯一的可导入候选（单 provider 工具的既有用例夹具）。
    fn single_candidate(preview: &ProviderImportPreviewDto) -> &ProviderImportCandidateDto {
        let importable = preview
            .candidates
            .iter()
            .filter(|candidate| candidate.status == ProviderImportCandidateStatus::Importable)
            .collect::<Vec<_>>();
        assert_eq!(importable.len(), 1, "fixture 应恰好有一个可导入候选");
        importable[0]
    }

    /// 可导入候选由服务端签发预览 id；没有可导入候选时为空。
    fn preview_id(preview: &ProviderImportPreviewDto) -> String {
        preview.preview_id.clone().expect("可导入候选应签发预览 id")
    }

    /// 旧的「一次导入一个渠道」确认入口：批量改造后仍用于单候选工具的用例，
    /// 并返回导入后的档案（单候选场景下名称唯一）。
    fn confirm_provider_import_single(
        database: &mut Database,
        environment: &ExplicitEnvironment,
        redactor: &mut SecretRedactor,
        preview: &ProviderImportPreviewDto,
        name: String,
    ) -> Result<ProviderProfileDto, AppError> {
        let candidate = single_candidate(preview);
        confirm_provider_import(
            database,
            environment,
            redactor,
            ConfirmProviderImportInput {
                preview_id: preview_id(preview),
                items: vec![ConfirmProviderImportItem {
                    candidate_id: candidate.candidate_id.clone(),
                    name: name.clone(),
                }],
            },
        )?;
        list_provider_profiles(database, preview.tool)?
            .into_iter()
            .find(|profile| profile.name == name)
            .ok_or_else(|| AppError::internal("导入后应存在同名档案"))
    }

    #[test]
    fn opencode_provider_discovery_reads_model_and_provider_in_json_and_jsonc() {
        let fixture = fixture();
        let directory = fixture.environment.opencode_config_dir();
        fs::create_dir_all(directory).unwrap();
        assert!(
            super::discover_native_providers(&fixture.environment, Tool::Opencode)
                .unwrap()
                .is_empty()
        );
        for (filename, comment) in [("opencode.json", ""), ("opencode.jsonc", "// 已有配置\n")]
        {
            let path = directory.join(filename);
            let content = format!(
                "{{{comment}\"model\":\"fixture/model-a\",\"provider\":{{\"fixture\":{{\"npm\":\"@ai-sdk/openai-compatible\",\"name\":\"测试渠道\",\"options\":{{\"baseURL\":\"https://fixture.invalid/v1\",\"apiKey\":\"fixture-secret\"}}}}}},\"unrelated\":true}}"
            );
            fs::write(&path, &content).unwrap();
            let discovered = super::discover_native_providers(&fixture.environment, Tool::Opencode)
                .unwrap()
                .into_iter()
                .next()
                .unwrap();
            assert_eq!(discovered.provider_id.as_deref(), Some("fixture"));
            assert_eq!(discovered.default_model, "model-a");
            assert_eq!(discovered.api_key.as_deref(), Some("fixture-secret"));
            assert_eq!(discovered.target_path, path.to_str().unwrap());
            assert_eq!(fs::read_to_string(&path).unwrap(), content);
        }
    }

    #[test]
    fn opencode_provider_initial_sync_can_overwrite_owned_selectors_without_baseline() {
        let mut fixture = fixture();
        let directory = fixture.environment.opencode_config_dir();
        fs::create_dir_all(directory).unwrap();
        let settings = directory.join("opencode.json");
        fs::write(
            &settings,
            r#"{
  "$schema": "https://opencode.ai/config.json",
  "model": "opencode/native-model",
  "provider": {
    "ccgo": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Native only",
      "options": {"baseURL": "https://native.invalid/v1", "apiKey": "native-secret"},
      "models": {"native-model": {"name": "Native model"}}
    }
  },
  "unmanaged": {"keep": true}
}
"#,
        )
        .unwrap();
        assert!(
            super::discover_native_providers(&fixture.environment, Tool::Opencode)
                .unwrap()
                .is_empty()
        );

        let mut redactor = SecretRedactor::default();
        let profile = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            ProviderProfileInput {
                tool: Tool::Opencode,
                name: "中央渠道".to_owned(),
                api_base_url: "https://central.invalid/v1".to_owned(),
                api_key: "central-secret".to_owned(),
                default_model: "central-model".to_owned(),
                options: ProviderOptionsInput {
                    opencode_npm: Some("@ai-sdk/openai-compatible".to_owned()),
                    opencode_api: Some("openai-compatible".to_owned()),
                    ..ProviderOptionsInput::default()
                },
                activate: true,
            },
        )
        .unwrap();
        let provider_id = profile.options.provider_id.clone().unwrap();

        let preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Opencode,
        )
        .unwrap();
        let target = &preview.targets[0];
        assert_eq!(target.status, crate::domain::SyncStatus::ExternalNonOwnedChange);
        assert_eq!(target.change_kind, crate::domain::ChangeKind::Update);
        assert_eq!(target.error_code, None);
        assert_eq!(target.warning_codes, vec!["EXTERNAL_NON_OWNED_CHANGE"]);
        assert!(!target.readopt_available);
        let external_plan = crate::sync::ExternalChangePlanDto::from_preview(
            &preview,
            ArtifactKind::Provider,
            Tool::Opencode,
        );
        assert!(external_plan.can_overwrite_central);
        assert!(!external_plan.can_adopt_native);
        assert_eq!(
            external_plan.adopt_blocked_reason.as_deref(),
            Some("MATCH_OR_IMPORT_REQUIRED")
        );

        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &preview.preview_id,
            Tool::Opencode,
            ArtifactKind::Provider,
        )
        .unwrap();

        let native: Value = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
        assert_eq!(native["model"], format!("{provider_id}/central-model"));
        assert_eq!(native["provider"]["ccgo"]["name"], "Native only");
        assert_eq!(native["provider"][&provider_id]["options"]["baseURL"], "https://central.invalid/v1");
        assert_eq!(native["unmanaged"]["keep"], true);

        let settled = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Opencode,
        )
        .unwrap();
        assert_eq!(settled.targets[0].status, crate::domain::SyncStatus::InSync);
        assert_eq!(settled.targets[0].change_kind, crate::domain::ChangeKind::Unchanged);
    }

    #[test]
    fn provider_copy_is_independent_and_revalidated_for_target_tool() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        let source = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Claude, "Claude 主渠道", "fixture-copy-secret", true),
        )
        .unwrap();
        let copied = copy_provider_profile(
            &mut fixture.database,
            &mut redactor,
            CopyProviderProfileInput {
                source_id: source.id.clone(),
                target_tool: Tool::Codex,
                target_name: "Codex 复制渠道".to_owned(),
                activate: true,
            },
        )
        .unwrap();
        assert_ne!(source.id, copied.id);
        assert_eq!(copied.tool, Tool::Codex);
        assert!(copied
            .options
            .provider_id
            .as_deref()
            .is_some_and(|id| id.starts_with("easytoagents_")));
        assert_eq!(copied.options.credential_env_key, None);
    }

    #[test]
    fn profile_status_scan_skips_unowned_provider_without_central_intent() {
        let fixture = fixture();
        let statuses = super::list_global_profile_target_statuses(
            &fixture.database,
            &fixture.environment,
            Tool::Codex,
        )
        .unwrap();
        assert!(statuses
            .iter()
            .all(|status| status.artifact_kind == ArtifactKind::Prompt));
    }

    #[test]
    fn inactive_pi_provider_mutations_return_the_global_sync_scope() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        let expected = Some(vec![SyncScopeDto::global(ArtifactKind::Provider, Tool::Pi)]);

        let created = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Pi, "Pi 非当前渠道", "fixture-pi-secret", false),
        )
        .unwrap();
        assert_eq!(created.affected_sync_scopes, expected);

        let updated = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: created.id.clone(),
                name: "Pi 非当前渠道（已编辑）".to_owned(),
                api_base_url: created.api_base_url.clone(),
                api_key: SecretUpdate::Keep,
                default_model: "fixture-updated-model".to_owned(),
                options: ProviderOptionsInput::default(),
                row_version: created.row_version,
            },
        )
        .unwrap();
        assert_eq!(updated.affected_sync_scopes, expected);

        let deleted = super::delete_provider_profile(
            &mut fixture.database,
            &crate::profiles::VersionedProfileInput {
                id: updated.id,
                row_version: updated.row_version,
            },
        )
        .unwrap();
        assert_eq!(deleted.affected_sync_scopes, expected);
    }

    #[test]
    fn zcode_provider_import_and_sync_preserve_app_owned_fields() {
        let mut fixture = fixture();
        let config = fixture.home.join(".zcode/v2/config.json");
        fs::write(
            &config,
            r#"{
  "provider": {
    "builtin:fixture": {
      "name": "Fixture Plan",
      "kind": "anthropic",
      "options": {
        "apiKey": "fixture-native-secret",
        "baseURL": "https://fixture.invalid/v1"
      },
      "enabled": true,
      "source": "custom",
      "models": {
        "GLM-5.3": {"zcode": {"priority": 1}}
      }
    }
  },
  "unrelated": {"keep": true}
}
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview_dto = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Zcode,
        )
        .unwrap();
        assert!(preview_dto.preview_id.is_some());
        assert_eq!(
            single_candidate(&preview_dto).suggested_name,
            "Fixture Plan"
        );
        assert_eq!(
            single_candidate(&preview_dto).api_base_url,
            "https://fixture.invalid/v1"
        );
        assert!(single_candidate(&preview_dto).api_key_configured);
        let serialized = serde_json::to_string(&preview_dto).unwrap();
        assert!(!serialized.contains("fixture-native-secret"));

        let created = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &preview_dto,
            "Fixture Plan".to_owned(),
        )
        .unwrap();
        assert_eq!(
            created.options.provider_id.as_deref(),
            Some("builtin:fixture")
        );
        assert_eq!(created.options.zcode_kind.as_deref(), Some("anthropic"));

        let sync_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Zcode,
        )
        .unwrap();
        assert!(!serde_json::to_string(&sync_preview)
            .unwrap()
            .contains("fixture-native-secret"));
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &sync_preview.preview_id,
            Tool::Zcode,
            ArtifactKind::Provider,
        )
        .unwrap();

        let document: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        let entry = &document["provider"]["builtin:fixture"];
        assert_eq!(entry["name"], "Fixture Plan");
        assert_eq!(entry["kind"], "anthropic");
        assert_eq!(entry["options"]["apiKey"], "fixture-native-secret");
        assert_eq!(entry["options"]["baseURL"], "https://fixture.invalid/v1");
        assert_eq!(entry["enabled"], true);
        // ZCode 自管字段与无关键必须原样保留。
        assert_eq!(entry["source"], "custom");
        assert_eq!(entry["models"]["GLM-5.3"]["zcode"]["priority"], 1);
        assert_eq!(document["unrelated"]["keep"], true);

        // 切换到新档案后，旧条目的受管子键被移除，models/source 保留。
        let second = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            ProviderProfileInput {
                options: ProviderOptionsInput {
                    zcode_kind: Some("openai".to_owned()),
                    ..ProviderOptionsInput::default()
                },
                ..provider(Tool::Zcode, "第二渠道", "fixture-second-secret", true)
            },
        )
        .unwrap();
        let second_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Zcode,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &second_preview.preview_id,
            Tool::Zcode,
            ArtifactKind::Provider,
        )
        .unwrap();
        let document: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        assert!(document["provider"]["builtin:fixture"]
            .get("name")
            .is_none());
        assert_eq!(
            document["provider"]["builtin:fixture"]["models"]["GLM-5.3"]["zcode"]["priority"],
            1
        );
        let new_entry = &document["provider"][second.options.provider_id.unwrap().as_str()];
        assert_eq!(new_entry["name"], "第二渠道");
        assert_eq!(new_entry["kind"], "openai");
        assert_eq!(new_entry["options"]["apiKey"], "fixture-second-secret");
        assert_eq!(document["unrelated"]["keep"], true);
    }

    #[test]
    fn codex_provider_rename_preserves_stable_provider_id_and_delete_removes_profile() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        let created = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Codex, "重命名前", "fixture-stable-secret", true),
        )
        .unwrap();
        let provider_id = created.options.provider_id.clone();

        let updated = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: created.id.clone(),
                name: "重命名后".to_owned(),
                api_base_url: created.api_base_url.clone(),
                api_key: SecretUpdate::Keep,
                default_model: "fixture-updated-model".to_owned(),
                options: ProviderOptionsInput {
                    wire_api: Some("responses".to_owned()),
                    ..ProviderOptionsInput::default()
                },
                row_version: created.row_version,
            },
        )
        .unwrap();
        assert_eq!(updated.name, "重命名后");
        assert_eq!(updated.default_model, "fixture-updated-model");
        assert_eq!(updated.options.provider_id, provider_id);

        super::delete_provider_profile(
            &mut fixture.database,
            &crate::profiles::VersionedProfileInput {
                id: created.id.clone(),
                row_version: updated.row_version,
            },
        )
        .unwrap();
        assert!(list_provider_profiles(&fixture.database, Tool::Codex)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn claude_provider_switch_cleans_old_owned_keys_and_preserves_other_settings() {
        let mut fixture = fixture();
        let settings = fixture.home.join(".claude/settings.json");
        fs::write(
            &settings,
            r#"{
  "env": {"UNRELATED_ENV": "keep"},
  "permissions": {"allow": ["Read"]},
  "plugins": {"fixture": true}
}
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let first = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            ProviderProfileInput {
                options: ProviderOptionsInput {
                    credential_env_key: Some(crate::profiles::ClaudeCredentialEnvKey::ApiKey),
                    extra_env: [(
                        "ANTHROPIC_DEFAULT_OPUS_MODEL".to_owned(),
                        "old-opus".to_owned(),
                    )]
                    .into_iter()
                    .collect(),
                    ..ProviderOptionsInput::default()
                },
                ..provider(Tool::Claude, "第一档", "fixture-first-secret", true)
            },
        )
        .unwrap();
        let first_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert!(!serde_json::to_string(&first_preview)
            .unwrap()
            .contains("fixture-first-secret"));
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &first_preview.preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
        )
        .unwrap();

        let second = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Claude, "第二档", "fixture-second-secret", false),
        )
        .unwrap();
        set_active_provider_profile(
            &mut fixture.database,
            Tool::Claude,
            &crate::profiles::VersionedProfileInput {
                id: second.id.clone(),
                row_version: second.row_version,
            },
        )
        .unwrap();
        let second_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &second_preview.preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
        )
        .unwrap();
        let written: Value = serde_json::from_slice(&fs::read(settings).unwrap()).unwrap();
        assert_eq!(written["env"]["UNRELATED_ENV"], "keep");
        assert!(written["env"].get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none());
        assert_eq!(written["permissions"]["allow"][0], "Read");
        assert_eq!(written["plugins"]["fixture"], true);
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn provider_external_drift_can_readopt_then_requires_a_new_preview() {
        let mut fixture = fixture();
        let settings = fixture.home.join(".claude/settings.json");
        fs::write(
            &settings,
            r#"{
  "env": {"UNRELATED_ENV": "keep"},
  "permissions": {"allow": ["Read"]}
}
"#,
        )
        .unwrap();
        let write_operations = Mutex::new(());
        let mut redactor = SecretRedactor::default();
        let profile = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Claude, "第一渠道", "fixture-provider-secret", true),
        )
        .unwrap();
        let first_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        apply_profile_preview(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &first_preview.preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
        )
        .unwrap();

        // 外部改写受管字段后，预览必须阻止 Apply，同时明确提供 readopt。
        let externally_changed = fs::read_to_string(&settings)
            .unwrap()
            .replace("fixture-provider-secret", "external-provider-secret");
        fs::write(&settings, &externally_changed).unwrap();
        let conflicted = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        let target = conflicted.targets.first().expect("Provider 预览应包含目标");
        assert_eq!(target.change_kind, crate::domain::ChangeKind::Update);
        assert_eq!(
            target.status,
            crate::domain::SyncStatus::ExternalOwnedChange
        );
        assert!(target.readopt_available);
        let old_preview_id = conflicted.preview_id.clone();

        let central_before = list_provider_profiles(&fixture.database, Tool::Claude).unwrap();
        let native_before_readopt = fs::read(&settings).unwrap();
        let path_error = readopt_provider_target(
            &mut fixture.database,
            &fixture.environment,
            &ReadoptProviderTargetInput {
                tool: Tool::Claude,
                target_path: "/wrong/settings.json".to_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(path_error.code(), crate::error::ErrorCode::InvalidInput);
        assert_eq!(fs::read(&settings).unwrap(), native_before_readopt);

        let readopt = readopt_provider_target(
            &mut fixture.database,
            &fixture.environment,
            &ReadoptProviderTargetInput {
                tool: Tool::Claude,
                target_path: settings.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        assert_eq!(readopt.target_path, settings.to_string_lossy());
        // readopt 只刷新应用基线，不写原生文件或中央渠道 row_version。
        assert_eq!(fs::read(&settings).unwrap(), native_before_readopt);
        let central_after = list_provider_profiles(&fixture.database, Tool::Claude).unwrap();
        assert_eq!(central_after[0].row_version, central_before[0].row_version);
        assert_eq!(central_after[0].id, profile.id);

        // 旧冲突 Preview 永远不可消费；恢复只能使用 readopt 后生成的新 Preview。
        let old_apply = apply_profile_preview(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &old_preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
        )
        .unwrap_err();
        assert_eq!(old_apply.code(), crate::error::ErrorCode::StalePreview);
        assert_eq!(fs::read(&settings).unwrap(), native_before_readopt);

        let recovered = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_ne!(recovered.preview_id, old_preview_id);
        assert_eq!(
            recovered.targets[0].status,
            crate::domain::SyncStatus::InSync
        );
        assert_eq!(
            recovered.targets[0].change_kind,
            crate::domain::ChangeKind::Update
        );
        assert!(!recovered.targets[0].readopt_available);
        apply_profile_preview(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &recovered.preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
        )
        .unwrap();
        let native: Value = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
        assert_eq!(
            native["env"]["ANTHROPIC_API_KEY"],
            "fixture-provider-secret"
        );
        assert_eq!(native["env"]["UNRELATED_ENV"], "keep");
    }

    #[test]
    fn provider_readopt_rejects_unreadable_target_without_changing_baseline() {
        let mut fixture = fixture();
        let settings = fixture.home.join(".claude/settings.json");
        let mut redactor = SecretRedactor::default();
        let _profile = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Claude, "渠道", "fixture-provider-secret", true),
        )
        .unwrap();
        let first_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &first_preview.preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
        )
        .unwrap();

        let baseline_before = fixture
            .database
            .connection()
            .query_row(
                "SELECT baseline_full_hash, baseline_managed_hash
                 FROM managed_targets
                 WHERE tool = 'claude' AND artifact_kind = 'provider'
                   AND scope = 'global' AND project_id IS NULL",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .unwrap();
        fs::write(&settings, b"{not-json").unwrap();
        let error = readopt_provider_target(
            &mut fixture.database,
            &fixture.environment,
            &ReadoptProviderTargetInput {
                tool: Tool::Claude,
                target_path: settings.to_string_lossy().into_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::Conflict);
        assert_eq!(fs::read(&settings).unwrap(), b"{not-json");
        let baseline_after = fixture
            .database
            .connection()
            .query_row(
                "SELECT baseline_full_hash, baseline_managed_hash
                 FROM managed_targets
                 WHERE tool = 'claude' AND artifact_kind = 'provider'
                   AND scope = 'global' AND project_id IS NULL",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(baseline_after, baseline_before);
    }

    #[test]
    fn prompt_import_is_lossless_and_requires_confirmed_persisted_preview() {
        let mut fixture = fixture();
        let prompt_path = fixture.home.join(".codex/AGENTS.md");
        let original = "# 原有指令\n\n保留末尾空格  \n";
        fs::write(&prompt_path, original).unwrap();
        fs::write(
            fixture.home.join(".codex/AGENTS.override.md"),
            "# 覆盖指令\n",
        )
        .unwrap();
        let preview =
            discover_prompt_import(&mut fixture.database, &fixture.environment, Tool::Codex)
                .unwrap()
                .unwrap();
        assert_eq!(preview.body, original);
        assert_eq!(fs::read_to_string(&prompt_path).unwrap(), original);
        let imported = confirm_prompt_import(
            &mut fixture.database,
            &fixture.environment,
            ConfirmImportInput {
                preview_id: preview.preview_id.clone(),
                name: "原有提示词".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(imported.body, original);
        assert_eq!(fs::read_to_string(&prompt_path).unwrap(), original);
        let repeated = confirm_prompt_import(
            &mut fixture.database,
            &fixture.environment,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "重复".to_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(
            repeated.code(),
            crate::error::ErrorCode::PreviewAlreadyConsumed
        );
    }

    #[test]
    fn provider_import_preview_is_persisted_redacted_and_adopts_without_writing() {
        let mut fixture = fixture();
        let settings_path = fixture.home.join(".claude/settings.json");
        let secret = "fixture-import-provider-secret";
        let original = format!(
            r#"{{
  "env": {{
    "ANTHROPIC_BASE_URL": "https://import.example.com/v1",
    "ANTHROPIC_API_KEY": "{secret}",
    "ANTHROPIC_MODEL": "claude-imported",
    "UNRELATED_ENV": "keep"
  }},
  "permissions": {{"allow": ["Read"]}}
}}
"#,
        );
        fs::write(&settings_path, &original).unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        assert!(single_candidate(&preview).api_key_configured);
        assert!(!serde_json::to_string(&preview).unwrap().contains(secret));
        let persisted: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT redacted_preview_json FROM provider_import_previews WHERE id = ?1",
                [&preview_id(&preview)],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!persisted.contains(secret));
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), original);

        let externally_changed = original.replace(
            r#""permissions": {"allow": ["Read"]}"#,
            r#""permissions": {"allow": ["Read", "Glob"]}"#,
        );
        fs::write(&settings_path, &externally_changed).unwrap();
        let stale = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &preview,
            "过期导入".to_owned(),
        )
        .unwrap_err();
        assert_eq!(stale.code(), crate::error::ErrorCode::StalePreview);
        assert!(list_provider_profiles(&fixture.database, Tool::Claude)
            .unwrap()
            .is_empty());
        assert_eq!(
            fs::read_to_string(&settings_path).unwrap(),
            externally_changed
        );

        fs::write(&settings_path, &original).unwrap();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();

        let imported = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &preview,
            "导入渠道".to_owned(),
        )
        .unwrap();
        assert!(imported.api_key_configured);
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), original);
        let sync_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert!(matches!(
            sync_preview.targets[0].change_kind,
            crate::domain::ChangeKind::Unchanged | crate::domain::ChangeKind::Warning
        ));
    }

    /// 删光渠道后重新导入：历史导入/同步留下的孤儿受管基线应被刷新而不是报 CONFLICT。
    #[test]
    fn provider_import_reattaches_orphaned_managed_baseline_after_profile_delete() {
        fn baseline_row(fixture: &Fixture) -> (String, String, String) {
            fixture
                .database
                .connection()
                .query_row(
                    "SELECT baseline_full_hash, baseline_managed_hash, last_status
                     FROM managed_targets
                     WHERE tool = 'claude' AND artifact_kind = 'provider'
                       AND scope = 'global' AND project_id IS NULL",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .unwrap()
        }

        let mut fixture = fixture();
        let settings_path = fixture.home.join(".claude/settings.json");
        let secret = "fixture-reattach-provider-secret";
        let original = format!(
            r#"{{
  "env": {{
    "ANTHROPIC_BASE_URL": "https://reattach.example.com/v1",
    "ANTHROPIC_API_KEY": "{secret}",
    "ANTHROPIC_MODEL": "claude-reattach"
  }}
}}
"#,
        );
        fs::write(&settings_path, &original).unwrap();
        let mut redactor = SecretRedactor::default();
        let first_preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        let first = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &first_preview,
            "首次导入".to_owned(),
        )
        .unwrap();
        let (first_full_hash, first_managed_hash, first_status) = baseline_row(&fixture);
        assert_eq!(first_status, "in_sync");

        super::delete_provider_profile(
            &mut fixture.database,
            &crate::profiles::VersionedProfileInput {
                id: first.id.clone(),
                row_version: first.row_version,
            },
        )
        .unwrap();
        assert!(list_provider_profiles(&fixture.database, Tool::Claude)
            .unwrap()
            .is_empty());

        // 原生文件在删除后被外部修改；重新导入必须接管当前内容并刷新基线。
        let externally_changed = original.replace("reattach.example.com", "moved.example.com");
        fs::write(&settings_path, &externally_changed).unwrap();
        let second_preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        let second = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &second_preview,
            "重新导入".to_owned(),
        )
        .unwrap();
        assert_eq!(second.name, "重新导入");
        let profiles = list_provider_profiles(&fixture.database, Tool::Claude).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, second.id);

        let (second_full_hash, second_managed_hash, second_status) = baseline_row(&fixture);
        assert_eq!(second_status, "in_sync");
        assert_ne!(second_full_hash, first_full_hash);
        assert_ne!(second_managed_hash, first_managed_hash);
        assert_eq!(
            fs::read_to_string(&settings_path).unwrap(),
            externally_changed
        );
    }

    /// 基线行的 row_version 会随历史同步递增；重导入必须在确认时重读当前
    /// row_version 并刷新基线，而不是依赖某个固定版本。
    #[test]
    fn provider_import_refreshes_orphaned_baseline_regardless_of_row_version() {
        let mut fixture = fixture();
        let settings_path = fixture.home.join(".claude/settings.json");
        fs::write(
            &settings_path,
            r#"{
  "env": {
    "ANTHROPIC_BASE_URL": "https://lock.example.com/v1",
    "ANTHROPIC_API_KEY": "fixture-lock-provider-secret",
    "ANTHROPIC_MODEL": "claude-lock"
  }
}
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let first_preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        let first = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &first_preview,
            "首次导入".to_owned(),
        )
        .unwrap();
        super::delete_provider_profile(
            &mut fixture.database,
            &crate::profiles::VersionedProfileInput {
                id: first.id,
                row_version: first.row_version,
            },
        )
        .unwrap();

        // 模拟历史同步对基线行的多次写入（row_version 递增）。
        fixture
            .database
            .connection_mut()
            .execute(
                "UPDATE managed_targets SET row_version = row_version + 2
                 WHERE tool = 'claude' AND artifact_kind = 'provider' AND scope = 'global'",
                [],
            )
            .unwrap();
        let (before_version,) = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM managed_targets
                 WHERE tool = 'claude' AND artifact_kind = 'provider' AND scope = 'global'",
                [],
                |row| Ok((row.get::<_, i64>(0)?,)),
            )
            .unwrap();
        assert!(before_version > 1);

        let second_preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        let second = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &second_preview,
            "重新导入".to_owned(),
        )
        .unwrap();
        assert_eq!(second.name, "重新导入");
        let (after_version, status) = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version, last_status FROM managed_targets
                 WHERE tool = 'claude' AND artifact_kind = 'provider' AND scope = 'global'",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert_eq!(after_version, before_version + 1);
        assert_eq!(status, "in_sync");
    }

    #[test]
    fn claude_provider_import_keeps_default_model_family_as_extra_env_without_anthropic_model() {
        let mut fixture = fixture();
        let settings_path = fixture.home.join(".claude/settings.json");
        let secret = "fixture-default-model-secret";
        let original = format!(
            r#"{{
  "env": {{
    "ANTHROPIC_BASE_URL": "https://default-family.example.com/v1",
    "ANTHROPIC_AUTH_TOKEN": "{secret}",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "claude-haiku-bg",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-opus-plan",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-main",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "主 Sonnet",
    "UNRELATED_ENV": "keep"
  }}
}}
"#,
        );
        fs::write(&settings_path, &original).unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        assert!(single_candidate(&preview).api_key_configured);
        assert_eq!(
            single_candidate(&preview).auth_kind,
            ProviderAuthKind::ApiKey
        );
        // 默认模型只来自 ANTHROPIC_MODEL；模型族键作为额外 env 原样保留。
        assert_eq!(single_candidate(&preview).default_model, "");
        assert!(single_candidate(&preview).skipped_env_keys.is_empty());
        assert!(!serde_json::to_string(&preview).unwrap().contains(secret));
        assert!(single_candidate(&preview).redacted_projection["env"]
            .get(CLAUDE_MODEL_KEY)
            .is_none());
        assert_eq!(
            single_candidate(&preview).redacted_projection["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            crate::security::REDACTED
        );
        assert_eq!(
            single_candidate(&preview).redacted_projection["env"]["ANTHROPIC_AUTH_TOKEN"],
            crate::security::REDACTED
        );

        let imported = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &preview,
            "导入默认模型族".to_owned(),
        )
        .unwrap();
        assert_eq!(imported.default_model, "");
        assert_eq!(imported.options.auth_kind, ProviderAuthKind::ApiKey);
        assert_eq!(
            imported.options.credential_env_key,
            Some(crate::profiles::ClaudeCredentialEnvKey::AuthToken)
        );
        assert_eq!(
            imported.options.extra_env["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            "claude-sonnet-main"
        );
        assert_eq!(
            imported.options.extra_env["ANTHROPIC_DEFAULT_OPUS_MODEL"],
            "claude-opus-plan"
        );
        assert_eq!(imported.options.extra_env["UNRELATED_ENV"], "keep");
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), original);
        // 导入后的首次同步预览必须与磁盘一致，不会"补写"一个从未存在的 ANTHROPIC_MODEL。
        let sync_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            sync_preview.targets[0].change_kind,
            crate::domain::ChangeKind::Unchanged
        );
    }

    /// 额外 env 接管 settings.json 里的全部普通键；疑似凭据与非字符串值只报告键名并保持不动。
    #[test]
    fn claude_provider_import_captures_all_env_and_official_switch_removes_credentials() {
        let mut fixture = fixture();
        let settings_path = fixture.home.join(".claude/settings.json");
        let secret = "fixture-full-env-secret";
        let header_secret = "fixture-custom-header-secret";
        fs::write(
            &settings_path,
            format!(
                r#"{{
  "env": {{
    "ANTHROPIC_BASE_URL": "https://relay.example.com",
    "ANTHROPIC_AUTH_TOKEN": "{secret}",
    "ANTHROPIC_MODEL": "claude-relay",
    "ANTHROPIC_CUSTOM_HEADERS": "Authorization: Bearer {header_secret}",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-relay",
    "API_TIMEOUT_MS": "600000",
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "32000",
    "CLAUDE_CODE_EFFORT_LEVEL": "max",
    "SOME_FLAG": true
  }},
  "permissions": {{"allow": ["Read"]}}
}}
"#,
            ),
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(single_candidate(&preview).default_model, "claude-relay");
        assert_eq!(
            single_candidate(&preview).skipped_env_keys,
            vec![
                "ANTHROPIC_CUSTOM_HEADERS".to_owned(),
                "SOME_FLAG".to_owned()
            ]
        );
        let serialized = serde_json::to_string(&preview).unwrap();
        assert!(!serialized.contains(secret));
        assert!(!serialized.contains(header_secret));
        assert!(single_candidate(&preview).redacted_projection["env"]
            .get("ANTHROPIC_CUSTOM_HEADERS")
            .is_none());
        let imported = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &preview,
            "中转渠道".to_owned(),
        )
        .unwrap();
        assert_eq!(
            imported
                .options
                .extra_env
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![
                "ANTHROPIC_DEFAULT_OPUS_MODEL",
                "API_TIMEOUT_MS",
                "CLAUDE_CODE_EFFORT_LEVEL",
                "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
            ]
        );
        assert_eq!(
            imported.options.extra_env["CLAUDE_CODE_MAX_OUTPUT_TOKENS"],
            "32000"
        );
        let unchanged = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            unchanged.targets[0].change_kind,
            crate::domain::ChangeKind::Unchanged
        );

        // 切换到官方账号登录渠道：移除接入地址与凭据，保留额外 env 与未接管内容。
        let official = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            ProviderProfileInput {
                tool: Tool::Claude,
                name: "Claude 官方账号".to_owned(),
                api_base_url: String::new(),
                api_key: String::new(),
                default_model: String::new(),
                options: ProviderOptionsInput {
                    auth_kind: ProviderAuthKind::OfficialLogin,
                    extra_env: [(
                        "CLAUDE_CODE_MAX_OUTPUT_TOKENS".to_owned(),
                        "64000".to_owned(),
                    )]
                    .into_iter()
                    .collect(),
                    ..ProviderOptionsInput::default()
                },
                activate: true,
            },
        )
        .unwrap();
        assert_eq!(official.options.auth_kind, ProviderAuthKind::OfficialLogin);
        assert_eq!(official.options.credential_env_key, None);
        assert!(!official.api_key_configured);
        let official_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            official_preview.targets[0].change_kind,
            crate::domain::ChangeKind::Update
        );
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &official_preview.preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
        )
        .unwrap();
        let written: Value = serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
        let env = written["env"].as_object().unwrap();
        for removed in [
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "API_TIMEOUT_MS",
            "CLAUDE_CODE_EFFORT_LEVEL",
        ] {
            assert!(env.get(removed).is_none(), "{removed} 应被移除");
        }
        assert_eq!(env["CLAUDE_CODE_MAX_OUTPUT_TOKENS"], "64000");
        assert_eq!(
            env["ANTHROPIC_CUSTOM_HEADERS"],
            format!("Authorization: Bearer {header_secret}")
        );
        assert_eq!(env["SOME_FLAG"], true);
        assert_eq!(written["permissions"]["allow"][0], "Read");

        // 切回第三方渠道：接入地址与凭据恢复（停用时 row_version 已递增，需重新读取）。
        let imported_now = list_provider_profiles(&fixture.database, Tool::Claude)
            .unwrap()
            .into_iter()
            .find(|profile| profile.id == imported.id)
            .unwrap();
        set_active_provider_profile(
            &mut fixture.database,
            Tool::Claude,
            &crate::profiles::VersionedProfileInput {
                id: imported_now.id.clone(),
                row_version: imported_now.row_version,
            },
        )
        .unwrap();
        let back_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &back_preview.preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
        )
        .unwrap();
        let restored: Value = serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
        assert_eq!(
            restored["env"]["ANTHROPIC_BASE_URL"],
            "https://relay.example.com"
        );
        assert_eq!(restored["env"]["ANTHROPIC_AUTH_TOKEN"], secret);
        assert_eq!(restored["env"]["ANTHROPIC_MODEL"], "claude-relay");
        assert_eq!(restored["env"]["CLAUDE_CODE_MAX_OUTPUT_TOKENS"], "32000");
        assert_eq!(restored["env"]["SOME_FLAG"], true);
    }

    #[test]
    fn claude_provider_import_without_base_url_or_credentials_yields_official_login_profile() {
        let mut fixture = fixture();
        let settings_path = fixture.home.join(".claude/settings.json");
        fs::write(
            &settings_path,
            r#"{"env": {"MCP_TIMEOUT": "300000", "DISABLE_TELEMETRY": "1"}, "model": "opus"}
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            single_candidate(&preview).auth_kind,
            ProviderAuthKind::OfficialLogin
        );
        assert_eq!(single_candidate(&preview).api_base_url, "");
        assert_eq!(single_candidate(&preview).default_model, "");
        assert!(!single_candidate(&preview).api_key_configured);
        let imported = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &preview,
            "官方登录".to_owned(),
        )
        .unwrap();
        assert_eq!(imported.options.auth_kind, ProviderAuthKind::OfficialLogin);
        assert_eq!(imported.options.extra_env["MCP_TIMEOUT"], "300000");
        let sync_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            sync_preview.targets[0].change_kind,
            crate::domain::ChangeKind::Unchanged
        );

        // 完全空的 env 没有可导入项；只有接入地址没有模型仍可导入。
        fs::write(&settings_path, r#"{"env": {}}"#).unwrap();
        let empty_preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        assert!(empty_preview.preview_id.is_none());
        assert!(empty_preview
            .candidates
            .iter()
            .all(|candidate| candidate.status != ProviderImportCandidateStatus::Importable));
    }

    #[test]
    fn claude_provider_import_with_base_url_but_no_model_keeps_empty_default_model() {
        let mut fixture = fixture();
        fs::write(
            fixture.home.join(".claude/settings.json"),
            r#"{"env": {"ANTHROPIC_BASE_URL": "https://relay.example.com", "ANTHROPIC_API_KEY": "fixture-no-model-secret"}}
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            single_candidate(&preview).auth_kind,
            ProviderAuthKind::ApiKey
        );
        assert_eq!(single_candidate(&preview).default_model, "");
        assert!(single_candidate(&preview).api_key_configured);
        let imported = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &preview,
            "无模型渠道".to_owned(),
        )
        .unwrap();
        assert_eq!(imported.default_model, "");
        assert_eq!(
            imported.options.credential_env_key,
            Some(crate::profiles::ClaudeCredentialEnvKey::ApiKey)
        );
    }

    #[test]
    fn provider_auth_kind_rules_are_enforced_on_create_update_and_copy() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        // 官方登录渠道不能带接入地址或密钥。
        for (url, key) in [("https://relay.example.com", ""), ("", "fixture-key")] {
            let error = create_provider_profile(
                &mut fixture.database,
                &mut redactor,
                ProviderProfileInput {
                    tool: Tool::Claude,
                    name: "错误官方渠道".to_owned(),
                    api_base_url: url.to_owned(),
                    api_key: key.to_owned(),
                    default_model: String::new(),
                    options: ProviderOptionsInput {
                        auth_kind: ProviderAuthKind::OfficialLogin,
                        ..ProviderOptionsInput::default()
                    },
                    activate: false,
                },
            )
            .unwrap_err();
            assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
        }
        // ZCode/OpenCode 不支持官方登录。
        let error = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            ProviderProfileInput {
                tool: Tool::Zcode,
                name: "ZCode 官方".to_owned(),
                api_base_url: String::new(),
                api_key: String::new(),
                default_model: "model".to_owned(),
                options: ProviderOptionsInput {
                    auth_kind: ProviderAuthKind::OfficialLogin,
                    ..ProviderOptionsInput::default()
                },
                activate: false,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);

        // Codex 官方登录渠道固定使用内置 openai provider，模型可空，不能跨工具复制。
        let codex_official = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            ProviderProfileInput {
                tool: Tool::Codex,
                name: "Codex 官方账号".to_owned(),
                api_base_url: String::new(),
                api_key: String::new(),
                default_model: String::new(),
                options: ProviderOptionsInput {
                    auth_kind: ProviderAuthKind::OfficialLogin,
                    ..ProviderOptionsInput::default()
                },
                activate: true,
            },
        )
        .unwrap();
        assert_eq!(
            codex_official.options.provider_id.as_deref(),
            Some("openai")
        );
        assert_eq!(codex_official.default_model, "");
        let copy_error = copy_provider_profile(
            &mut fixture.database,
            &mut redactor,
            CopyProviderProfileInput {
                source_id: codex_official.id.clone(),
                target_tool: Tool::Claude,
                target_name: "复制官方".to_owned(),
                activate: false,
            },
        )
        .unwrap_err();
        assert_eq!(copy_error.code(), crate::error::ErrorCode::InvalidInput);
        // 认证方式创建后不可更改。
        let switch_error = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: codex_official.id.clone(),
                name: codex_official.name.clone(),
                api_base_url: "https://relay.example.com/v1".to_owned(),
                api_key: SecretUpdate::Replace("fixture-switch-key".to_owned()),
                default_model: "gpt-fixture".to_owned(),
                options: ProviderOptionsInput::default(),
                row_version: codex_official.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(switch_error.code(), crate::error::ErrorCode::InvalidInput);
        assert!(!serde_json::to_string(
            &list_provider_profiles(&fixture.database, Tool::Codex).unwrap()
        )
        .unwrap()
        .contains("fixture-switch-key"));

        // API Key 渠道：Claude 接入地址可空（官方端点 + 自己的 API Key），模型可空。
        let direct = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            ProviderProfileInput {
                tool: Tool::Claude,
                name: "官方端点自带 Key".to_owned(),
                api_base_url: String::new(),
                api_key: "fixture-direct-key".to_owned(),
                default_model: String::new(),
                options: ProviderOptionsInput::default(),
                activate: true,
            },
        )
        .unwrap();
        assert_eq!(direct.api_base_url, "");
        assert_eq!(direct.default_model, "");
        assert!(direct.api_key_configured);
        let direct_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &direct_preview.preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
        )
        .unwrap();
        let written: Value =
            serde_json::from_slice(&fs::read(fixture.home.join(".claude/settings.json")).unwrap())
                .unwrap();
        assert_eq!(written["env"]["ANTHROPIC_API_KEY"], "fixture-direct-key");
        assert!(written["env"].get("ANTHROPIC_BASE_URL").is_none());
        assert!(written["env"].get("ANTHROPIC_MODEL").is_none());
    }

    #[test]
    fn claude_extra_env_accepts_numeric_limit_keys_and_rejects_credentials_and_reserved_keys() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        let mut accepted = provider(Tool::Claude, "限额 env", "fixture-limit-secret", false);
        accepted.options.extra_env = [
            ("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "32000"),
            ("MAX_THINKING_TOKENS", "16000"),
            ("CLAUDE_CODE_API_KEY_HELPER_TTL_MS", "3600000"),
            ("DISABLE_TELEMETRY", "true"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
        let created =
            create_provider_profile(&mut fixture.database, &mut redactor, accepted).unwrap();
        assert_eq!(
            created.options.extra_env["CLAUDE_CODE_MAX_OUTPUT_TOKENS"],
            "32000"
        );

        for (key, value) in [
            ("AWS_BEARER_TOKEN_BEDROCK", "opaque-bedrock-secret"),
            ("ANTHROPIC_MODEL", "claude-reserved"),
            ("ANTHROPIC_AUTH_TOKEN", "fixture-reserved-secret"),
        ] {
            let mut rejected = provider(Tool::Claude, "被拒 env", "fixture-reject-secret", false);
            rejected
                .options
                .extra_env
                .insert(key.to_owned(), value.to_owned());
            assert_eq!(
                create_provider_profile(&mut fixture.database, &mut redactor, rejected)
                    .unwrap_err()
                    .code(),
                crate::error::ErrorCode::InvalidInput,
                "{key} 应被拒绝"
            );
        }
    }

    #[test]
    fn codex_official_login_switch_and_chat_wire_api_are_supported() {
        let mut fixture = fixture();
        let config_path = fixture.home.join(".codex/config.toml");
        fs::write(
            &config_path,
            r#"model_reasoning_effort = "high"

[mcp_servers.fixture]
command = "keep"
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let mut chat = provider(Tool::Codex, "Chat 中转", "fixture-chat-secret", true);
        chat.options.wire_api = Some("chat".to_owned());
        let chat = create_provider_profile(&mut fixture.database, &mut redactor, chat).unwrap();
        assert_eq!(chat.options.wire_api.as_deref(), Some("chat"));
        let chat_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &chat_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
        )
        .unwrap();
        let chat_provider_id = chat.options.provider_id.clone().unwrap();
        let written: Value =
            toml_edit::de::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(
            written["model_providers"][&chat_provider_id]["wire_api"],
            "chat"
        );
        assert_eq!(written["model"], "fixture-model");

        let official = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            ProviderProfileInput {
                tool: Tool::Codex,
                name: "Codex 官方账号".to_owned(),
                api_base_url: String::new(),
                api_key: String::new(),
                default_model: String::new(),
                options: ProviderOptionsInput {
                    auth_kind: ProviderAuthKind::OfficialLogin,
                    ..ProviderOptionsInput::default()
                },
                activate: true,
            },
        )
        .unwrap();
        assert_eq!(official.options.auth_kind, ProviderAuthKind::OfficialLogin);
        let official_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &official_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
        )
        .unwrap();
        let text = fs::read_to_string(&config_path).unwrap();
        let written: Value = toml_edit::de::from_str(&text).unwrap();
        assert_eq!(written["model_provider"], "openai");
        assert!(written.get("model").is_none());
        assert!(written["model_providers"].get(&chat_provider_id).is_none());
        assert_eq!(written["model_reasoning_effort"], "high");
        assert_eq!(written["mcp_servers"]["fixture"]["command"], "keep");
        assert!(!text.contains("fixture-chat-secret"));
    }

    #[test]
    fn codex_provider_import_without_model_keeps_empty_default_model() {
        let mut fixture = fixture();
        let token = "fixture-no-model-codex-token";
        fs::write(
            fixture.home.join(".codex/config.toml"),
            format!(
                r#"model_provider = "relay"

[model_providers.relay]
name = "Relay"
base_url = "https://relay.example.com/v1"
experimental_bearer_token = "{token}"
wire_api = "chat"
"#
            ),
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
        )
        .unwrap();
        assert_eq!(single_candidate(&preview).default_model, "");
        assert_eq!(
            single_candidate(&preview).auth_kind,
            ProviderAuthKind::ApiKey
        );
        let imported = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &preview,
            "Relay".to_owned(),
        )
        .unwrap();
        assert_eq!(imported.default_model, "");
        assert_eq!(imported.options.wire_api.as_deref(), Some("chat"));
        let sync_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        assert_eq!(
            sync_preview.targets[0].change_kind,
            crate::domain::ChangeKind::Unchanged
        );
    }

    #[test]
    fn codex_provider_preserves_other_tables_cleans_old_table_and_never_leaks_tokens() {
        let mut fixture = fixture();
        let config_path = fixture.home.join(".codex/config.toml");
        fs::write(
            &config_path,
            r#"# keep this comment
[mcp_servers.fixture]
command = "keep"

[plugins]
fixture = true

[model_providers.external]
name = "External"
base_url = "https://external.example.com/v1"
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let first_secret = "fixture-codex-secret-first";
        let first = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Codex, "Codex 第一档", first_secret, true),
        )
        .unwrap();
        let first_provider_id = first.options.provider_id.clone().unwrap();
        let first_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        assert!(!serde_json::to_string(&first_preview)
            .unwrap()
            .contains(first_secret));
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &first_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
        )
        .unwrap();
        let journal = fs::read_to_string(
            fixture
                .paths
                .journals()
                .join(format!("{}.json", first_preview.preview_id)),
        )
        .unwrap();
        assert!(!journal.contains(first_secret));
        let preview_row: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT redacted_diff_json FROM sync_items WHERE run_id = ?1",
                [&first_preview.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!preview_row.contains(first_secret));

        let second_secret = "fixture-codex-secret-second";
        let second = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Codex, "Codex 第二档", second_secret, false),
        )
        .unwrap();
        let second_provider_id = second.options.provider_id.clone().unwrap();
        let activated_second = set_active_provider_profile(
            &mut fixture.database,
            Tool::Codex,
            &crate::profiles::VersionedProfileInput {
                id: second.id.clone(),
                row_version: second.row_version,
            },
        )
        .unwrap();
        let second_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &second_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
        )
        .unwrap();

        let text = fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("# keep this comment"));
        let parsed: Value = toml_edit::de::from_str(&text).unwrap();
        assert_eq!(parsed["mcp_servers"]["fixture"]["command"], "keep");
        assert_eq!(parsed["plugins"]["fixture"], true);
        assert!(parsed["model_providers"].get("external").is_some());
        assert!(parsed["model_providers"].get(&first_provider_id).is_none());
        assert_eq!(
            parsed["model_providers"][&second_provider_id]["experimental_bearer_token"],
            second_secret
        );
        assert_eq!(parsed["model_provider"], second_provider_id);

        super::delete_provider_profile(
            &mut fixture.database,
            &crate::profiles::VersionedProfileInput {
                id: activated_second.id,
                row_version: activated_second.row_version,
            },
        )
        .unwrap();
        let cleanup_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &cleanup_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
        )
        .unwrap();
        let cleaned: Value =
            toml_edit::de::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
        assert!(cleaned.get("model").is_none());
        assert!(cleaned.get("model_provider").is_none());
        assert!(cleaned["model_providers"]
            .get(&second_provider_id)
            .is_none());
        assert!(cleaned["model_providers"].get("external").is_some());
        assert_eq!(cleaned["mcp_servers"]["fixture"]["command"], "keep");
    }

    #[test]
    fn codex_status_reports_override_and_new_session_notice() {
        let fixture = fixture();
        fs::write(
            fixture.home.join(".codex/AGENTS.override.md"),
            "# 覆盖指令\n",
        )
        .unwrap();
        let status = get_tool_profile_status(&fixture.environment, Tool::Codex).unwrap();
        assert_eq!(
            status.prompt_override,
            crate::adapters::PromptOverrideState::Present
        );
        assert!(status.new_session_notice.contains("新"));
        assert!(status.bearer_token_warning.is_some());
        assert!(list_provider_profiles(&fixture.database, Tool::Codex)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cursor_provider_stays_fail_closed_while_prompt_opens() {
        let mut fixture = fixture();
        let status = get_tool_profile_status(&fixture.environment, Tool::Cursor).unwrap();
        assert_eq!(
            status.provider_capability.state,
            CapabilityState::Unsupported
        );
        assert_eq!(status.prompt_capability.state, CapabilityState::Supported);
        assert_eq!(
            status.provider_capability.diagnostic_code.as_deref(),
            Some("CURSOR_PROVIDER_UNSUPPORTED")
        );
        assert!(status.provider_target_path.is_none());
        assert_eq!(
            status.prompt_target_path.as_deref(),
            Some(
                fixture
                    .home
                    .join(".cursor/rules/easytoagents.mdc")
                    .to_str()
                    .unwrap()
            )
        );

        assert_eq!(
            list_provider_profiles(&fixture.database, Tool::Cursor)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert_eq!(
            discover_provider_import(
                &mut fixture.database,
                &fixture.environment,
                &SecretRedactor::default(),
                Tool::Cursor,
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::InvalidInput
        );
        // 提示词导入在无原生规则文件时返回 None（不再因 capability 拒绝）。
        assert_eq!(
            discover_prompt_import(&mut fixture.database, &fixture.environment, Tool::Cursor)
                .unwrap(),
            None
        );
        assert_eq!(
            preview_provider_sync(
                &mut fixture.database,
                &fixture.environment,
                &mut SecretRedactor::default(),
                Tool::Cursor,
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert_eq!(
            fixture
                .database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM managed_targets WHERE tool = 'cursor'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn tool_status_serializes_release_availability_and_imports_fail_before_native_reads() {
        let mut fixture = fixture();
        fs::write(
            fixture.home.join(".claude/settings.json"),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://should-not-import.example","ANTHROPIC_MODEL":"blocked","ANTHROPIC_API_KEY":"fixture-secret"}}"#,
        )
        .unwrap();
        fs::write(fixture.home.join(".codex/AGENTS.md"), "# 不应读取的提示词").unwrap();
        let environment = ExplicitEnvironment::new(
            &fixture.home,
            None,
            None,
            ToolAvailability::from_states([
                ToolAvailabilityState::Unavailable,
                ToolAvailabilityState::Unsupported,
                ToolAvailabilityState::Unavailable,
                ToolAvailabilityState::Unavailable,
                ToolAvailabilityState::Unavailable,
                ToolAvailabilityState::Unavailable,
            ]),
        )
        .unwrap()
        .with_claude_provider_policy(PolicyState::Allowed);

        let claude = get_tool_profile_status(&environment, Tool::Claude).unwrap();
        let codex = get_tool_profile_status(&environment, Tool::Codex).unwrap();
        assert_eq!(claude.availability, ToolAvailabilityState::Unavailable);
        assert_eq!(codex.availability, ToolAvailabilityState::Unsupported);
        assert_eq!(claude.installation_version, None);
        assert_eq!(codex.installation_version, None);
        assert!(serde_json::to_string(&claude)
            .unwrap()
            .contains("\"availability\":\"unavailable\""));
        assert_eq!(
            discover_provider_import(
                &mut fixture.database,
                &environment,
                &SecretRedactor::default(),
                Tool::Claude,
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::NotFound
        );
        assert_eq!(
            discover_prompt_import(&mut fixture.database, &environment, Tool::Codex)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
    }

    #[test]
    fn claude_provider_unknown_or_host_managed_policy_blocks_preview() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(
                Tool::Claude,
                "受策略保护渠道",
                "fixture-policy-secret",
                true,
            ),
        )
        .unwrap();
        let unknown_environment =
            ExplicitEnvironment::new(&fixture.home, None, None, ToolAvailability::all_installed())
                .unwrap();
        let unknown = preview_provider_sync(
            &mut fixture.database,
            &unknown_environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            unknown.targets[0].status,
            crate::domain::SyncStatus::PolicyBlocked
        );
        assert_eq!(
            unknown.targets[0].error_code,
            Some(crate::error::ErrorCode::PolicyBlocked)
        );
        assert!(!fixture.home.join(".claude/settings.json").exists());

        fs::write(
            fixture.home.join(".claude/settings.json"),
            r#"{"env":{"CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST":"1","UNRELATED":"keep"}}"#,
        )
        .unwrap();
        let blocked = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            blocked.targets[0].status,
            crate::domain::SyncStatus::PolicyBlocked
        );
        assert_eq!(
            get_tool_profile_status(&fixture.environment, Tool::Claude)
                .unwrap()
                .provider_policy,
            PolicyState::Blocked
        );

        fs::write(
            fixture.home.join(".claude/settings.json"),
            "{ invalid settings",
        )
        .unwrap();
        let malformed = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            malformed.targets[0].status,
            crate::domain::SyncStatus::PolicyBlocked
        );
        assert_eq!(
            get_tool_profile_status(&fixture.environment, Tool::Claude)
                .unwrap()
                .provider_policy,
            PolicyState::Unknown
        );
    }

    #[test]
    fn codex_import_preserves_supported_provider_fields_and_redacts_all_secrets() {
        let mut fixture = fixture();
        let config_path = fixture.home.join(".codex/config.toml");
        let token = "fixture-imported-codex-token";
        let header = "fixture-imported-header-secret";
        fs::write(
            &config_path,
            format!(
                r#"model = "gpt-fixture"
model_provider = "external_fixture"

[model_providers.external_fixture]
name = "External Fixture"
base_url = "https://external.example.com/v1"
experimental_bearer_token = "{token}"
wire_api = "responses"
request_max_retries = 7

[model_providers.external_fixture.http_headers]
Authorization = "Bearer {header}"

[model_providers.external_fixture.query_params]
tenant = "fixture"
"#,
            ),
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
        )
        .unwrap();
        assert_eq!(
            single_candidate(&preview).suggested_name,
            "External Fixture"
        );
        let serialized = serde_json::to_string(&preview).unwrap();
        assert!(!serialized.contains(token));
        assert!(!serialized.contains(header));

        let imported = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &preview,
            single_candidate(&preview).suggested_name.clone(),
        )
        .unwrap();
        assert!(!serde_json::to_string(&imported).unwrap().contains(token));
        let unchanged = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        assert!(matches!(
            unchanged.targets[0].change_kind,
            crate::domain::ChangeKind::Unchanged | crate::domain::ChangeKind::Warning
        ));

        let updated = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: imported.id,
                name: "Renamed Fixture".to_owned(),
                api_base_url: imported.api_base_url,
                api_key: SecretUpdate::Keep,
                default_model: imported.default_model,
                options: ProviderOptionsInput {
                    wire_api: Some("responses".to_owned()),
                    ..ProviderOptionsInput::default()
                },
                row_version: imported.row_version,
            },
        )
        .unwrap();
        let apply_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &apply_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
        )
        .unwrap();
        let written: Value =
            toml_edit::de::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
        let provider_id = updated.options.provider_id.unwrap();
        assert_eq!(
            written["model_providers"][&provider_id]["request_max_retries"],
            7
        );
        assert_eq!(
            written["model_providers"][&provider_id]["http_headers"]["Authorization"],
            format!("Bearer {header}")
        );
        assert_eq!(
            written["model_providers"][&provider_id]["query_params"]["tenant"],
            "fixture"
        );
    }

    #[test]
    fn codex_oauth_import_adopts_openai_login_without_copying_tokens() {
        let mut fixture = fixture();
        let config_path = fixture.home.join(".codex/config.toml");
        let auth_path = fixture.home.join(".codex/auth.json");
        let access_token = "fixture-codex-oauth-access-token";
        let refresh_token = "fixture-codex-oauth-refresh-token";
        fs::write(
            &config_path,
            r#"model = "gpt-5.5"
"#,
        )
        .unwrap();
        fs::write(
            &auth_path,
            format!(
                r#"{{
  "auth_mode": "chatgpt",
  "tokens": {{
    "access_token": "{access_token}",
    "refresh_token": "{refresh_token}"
  }}
}}
"#
            ),
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
        )
        .unwrap();
        assert_eq!(
            single_candidate(&preview).suggested_name,
            "Codex 官方账号登录"
        );
        assert_eq!(single_candidate(&preview).default_model, "gpt-5.5");
        assert_eq!(
            single_candidate(&preview).auth_kind,
            ProviderAuthKind::OfficialLogin
        );
        assert!(!single_candidate(&preview).api_key_configured);
        let serialized_preview = serde_json::to_string(&preview).unwrap();
        assert!(!serialized_preview.contains(access_token));
        assert!(!serialized_preview.contains(refresh_token));
        assert_eq!(
            single_candidate(&preview).redacted_projection["model"],
            "gpt-5.5"
        );
        assert!(single_candidate(&preview)
            .redacted_projection
            .get("model_provider")
            .is_none());

        let imported = confirm_provider_import_single(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &preview,
            "Codex OAuth 登录".to_owned(),
        )
        .unwrap();
        assert!(!imported.api_key_configured);
        assert_eq!(imported.options.provider_id.as_deref(), Some("openai"));
        assert_eq!(imported.options.auth_kind, ProviderAuthKind::OfficialLogin);
        let official_options = ProviderOptionsInput {
            auth_kind: ProviderAuthKind::OfficialLogin,
            ..ProviderOptionsInput::default()
        };
        let edited = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: imported.id.clone(),
                name: "Codex OAuth 编辑".to_owned(),
                api_base_url: imported.api_base_url.clone(),
                api_key: SecretUpdate::Keep,
                default_model: imported.default_model.clone(),
                options: official_options.clone(),
                row_version: imported.row_version,
            },
        )
        .unwrap();
        assert!(!edited.api_key_configured);
        let key_update = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: edited.id.clone(),
                name: edited.name.clone(),
                api_base_url: edited.api_base_url.clone(),
                api_key: SecretUpdate::Replace("fixture-should-not-store".to_owned()),
                default_model: edited.default_model.clone(),
                options: official_options,
                row_version: edited.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(key_update.code(), crate::error::ErrorCode::InvalidInput);

        let sync_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &sync_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
        )
        .unwrap();
        let written: Value =
            toml_edit::de::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
        assert_eq!(written["model"], "gpt-5.5");
        assert_eq!(written["model_provider"], "openai");
        assert!(written.get("model_providers").is_none());
        let serialized_imported = serde_json::to_string(&imported).unwrap();
        assert!(!serialized_imported.contains(access_token));
        assert!(!serialized_imported.contains(refresh_token));
    }

    #[test]
    fn codex_oauth_import_does_not_report_without_auth_tokens() {
        let mut fixture = fixture();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            r#"model = "gpt-5.5"
"#,
        )
        .unwrap();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Codex,
        )
        .unwrap();
        assert!(preview.preview_id.is_none());
    }

    #[test]
    fn prompt_apply_is_exact_and_external_change_makes_preview_stale() {
        let mut fixture = fixture();
        let prompt = create_enabled_prompt(
            &mut fixture,
            Tool::Codex,
            "精确提示词",
            "# 第一版\n\n保留末尾空格  \n",
        );
        let redactor = SecretRedactor::default();
        let first_preview = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut SecretRedactor::default(),
            &first_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Prompt,
        )
        .unwrap();
        let prompt_path = fixture.home.join(".codex/AGENTS.md");
        assert_eq!(
            fs::read_to_string(&prompt_path).unwrap(),
            "# 第一版\n\n保留末尾空格  \n"
        );

        update_prompt_profile(
            &mut fixture.database,
            UpdatePromptProfileInput {
                id: prompt.id,
                name: prompt.name,
                body: "# 第二版\n".to_owned(),
                row_version: prompt.row_version,
            },
        )
        .unwrap();
        let stale_preview = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
        )
        .unwrap();
        fs::write(&prompt_path, "# 外部修改\n").unwrap();
        let error = apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut SecretRedactor::default(),
            &stale_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Prompt,
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(prompt_path).unwrap(), "# 外部修改\n");

        assert_eq!(
            create_prompt_profile(
                &mut fixture.database,
                PromptProfileInput {
                    name: "空提示词".to_owned(),
                    body: String::new(),
                },
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::InvalidInput
        );
    }

    #[test]
    fn prompt_native_adopt_updates_profile_and_baseline_and_returns_in_sync() {
        let mut fixture = fixture();
        let profile =
            create_enabled_prompt(&mut fixture, Tool::Codex, "可采纳提示词", "# 中央正文\n");
        let initial = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut SecretRedactor::default(),
            &initial.preview_id,
            Tool::Codex,
            ArtifactKind::Prompt,
        )
        .unwrap();

        let prompt_path = fixture.home.join(".codex/AGENTS.md");
        fs::write(&prompt_path, "# 原生正文\n\n保留未知空白  \n").unwrap();
        let plan = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Codex,
        )
        .unwrap();
        assert_eq!(
            plan.targets[0].status,
            crate::domain::SyncStatus::ExternalOwnedChange
        );
        let result = adopt_prompt_native(
            &mut fixture.database,
            &fixture.environment,
            prompt_native_input(&plan, Tool::Codex),
        )
        .unwrap();
        assert_eq!(result.adopted, vec![profile.name.clone()]);
        assert_eq!(
            result.affected_sync_scopes,
            Some(vec![SyncScopeDto::global(
                ArtifactKind::Prompt,
                Tool::Codex
            )])
        );
        let adopted = super::list_prompt_profiles(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|value| value.id == profile.id)
            .expect("采纳后中央档案应仍存在");
        assert_eq!(adopted.body, "# 原生正文\n\n保留未知空白  \n");
        assert_eq!(adopted.row_version, profile.row_version + 1);

        let in_sync = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Codex,
        )
        .unwrap();
        assert_eq!(in_sync.targets[0].status, crate::domain::SyncStatus::InSync);
        assert_eq!(
            in_sync.targets[0].change_kind,
            crate::domain::ChangeKind::Unchanged
        );
        let in_sync_adopt = adopt_prompt_native(
            &mut fixture.database,
            &fixture.environment,
            prompt_native_input(&in_sync, Tool::Codex),
        )
        .unwrap();
        assert_eq!(
            in_sync_adopt.adopted,
            Vec::<String>::new(),
            "in-sync 采纳应为幂等 no-op"
        );
    }

    #[test]
    fn prompt_native_adopt_rejects_stale_hash_and_target_row() {
        let mut fixture = fixture();
        let profile =
            create_enabled_prompt(&mut fixture, Tool::Codex, "采纳证据提示词", "# 初始正文\n");
        let initial = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut SecretRedactor::default(),
            &initial.preview_id,
            Tool::Codex,
            ArtifactKind::Prompt,
        )
        .unwrap();
        let prompt_path = fixture.home.join(".codex/AGENTS.md");
        fs::write(&prompt_path, "# 外部第一版\n").unwrap();
        let plan = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Codex,
        )
        .unwrap();
        let evidence = prompt_native_input(&plan, Tool::Codex);

        fs::write(&prompt_path, "# 外部竞态版\n").unwrap();
        let stale_hash = adopt_prompt_native(
            &mut fixture.database,
            &fixture.environment,
            evidence.clone(),
        )
        .unwrap_err();
        assert_eq!(stale_hash.code(), crate::error::ErrorCode::StalePreview);
        let unchanged = super::list_prompt_profiles(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|value| value.id == profile.id)
            .unwrap();
        assert_eq!(unchanged.body, profile.body);

        fs::write(&prompt_path, "# 外部第一版\n").unwrap();
        let stale_row = evidence;
        fixture
            .database
            .connection_mut()
            .execute(
                "UPDATE managed_targets SET last_status = 'external_owned_change' WHERE id = ?1",
                [&stale_row.target_id],
            )
            .unwrap();
        let stale_target =
            adopt_prompt_native(&mut fixture.database, &fixture.environment, stale_row)
                .unwrap_err();
        assert_eq!(stale_target.code(), crate::error::ErrorCode::StalePreview);
    }

    #[test]
    fn cursor_prompt_native_adopt_strips_frontmatter_and_rejects_malformed_text() {
        let mut fixture = fixture();
        fs::create_dir_all(fixture.home.join(".cursor/rules")).unwrap();
        let profile = create_enabled_prompt(
            &mut fixture,
            Tool::Cursor,
            "Cursor 原生采纳",
            "# 中央 Cursor 规则\n",
        );
        let initial = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Cursor,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut SecretRedactor::default(),
            &initial.preview_id,
            Tool::Cursor,
            ArtifactKind::Prompt,
        )
        .unwrap();
        let prompt_path = fixture.home.join(".cursor/rules/easytoagents.mdc");
        fs::write(
            &prompt_path,
            "---\nalwaysApply: false\n---\n\n# 原生 Cursor 规则\n",
        )
        .unwrap();
        let plan = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Cursor,
        )
        .unwrap();
        adopt_prompt_native(
            &mut fixture.database,
            &fixture.environment,
            prompt_native_input(&plan, Tool::Cursor),
        )
        .unwrap();
        let adopted = super::list_prompt_profiles(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|value| value.id == profile.id)
            .unwrap();
        assert_eq!(adopted.body, "# 原生 Cursor 规则\n");

        fs::write(
            &prompt_path,
            "---\nalwaysApply: false\n# 缺少闭合 frontmatter\n",
        )
        .unwrap();
        let malformed = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Cursor,
        )
        .unwrap();
        let error = adopt_prompt_native(
            &mut fixture.database,
            &fixture.environment,
            prompt_native_input(&malformed, Tool::Cursor),
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::ParseError);
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("缺少闭合 frontmatter"));
    }

    #[test]
    fn cursor_prompt_writes_official_mdc_contract_and_import_strips_frontmatter() {
        let mut fixture = fixture();
        // 导入路径：预置带 frontmatter 的官方 `.mdc` 规则文件。
        fs::create_dir_all(fixture.home.join(".cursor/rules")).unwrap();
        fs::write(
            fixture.home.join(".cursor/rules/easytoagents.mdc"),
            "---\nalwaysApply: false\n---\n\n# 既有规则\n\n- 保持精确\n",
        )
        .unwrap();
        let preview_dto =
            discover_prompt_import(&mut fixture.database, &fixture.environment, Tool::Cursor)
                .unwrap()
                .expect("Cursor 全局规则文件应可发现");
        assert_eq!(
            preview_dto.body, "# 既有规则\n\n- 保持精确\n",
            "导入正文必须剥离 frontmatter"
        );
        let imported = confirm_prompt_import(
            &mut fixture.database,
            &fixture.environment,
            ConfirmImportInput {
                preview_id: preview_dto.preview_id,
                name: "导入的 Cursor 规则".to_owned(),
            },
        )
        .unwrap();
        assert!(imported.global_tools.contains(&Tool::Cursor));
        // 无损导入合同：导入本身不修改原生文件（frontmatter 保持用户现状）。
        assert_eq!(
            fs::read_to_string(fixture.home.join(".cursor/rules/easytoagents.mdc")).unwrap(),
            "---\nalwaysApply: false\n---\n\n# 既有规则\n\n- 保持精确\n"
        );

        // 全局启用走完整预览/应用链路；应用发起的任何写入都归一化为
        // 固定 alwaysApply: true frontmatter + 档案正文。
        update_prompt_profile(
            &mut fixture.database,
            UpdatePromptProfileInput {
                id: imported.id.clone(),
                name: imported.name.clone(),
                body: "# 既有规则\n\n- 保持精确\n- 新增条款\n".to_owned(),
                row_version: imported.row_version,
            },
        )
        .unwrap();
        let first_preview = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Cursor,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut SecretRedactor::default(),
            &first_preview.preview_id,
            Tool::Cursor,
            ArtifactKind::Prompt,
        )
        .unwrap();
        let prompt_path = fixture.home.join(".cursor/rules/easytoagents.mdc");
        assert_eq!(
            fs::read_to_string(&prompt_path).unwrap(),
            "---\nalwaysApply: true\n---\n\n# 既有规则\n\n- 保持精确\n- 新增条款\n"
        );
    }

    #[test]
    fn provider_validation_rejects_url_credentials_and_unsupported_wire_api() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        let mut credential_url = provider(
            Tool::Codex,
            "凭据 URL",
            "fixture-url-provider-secret",
            false,
        );
        credential_url.api_base_url = "https://user:password@provider.example.com/v1".to_owned();
        assert_eq!(
            create_provider_profile(&mut fixture.database, &mut redactor, credential_url)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );

        let mut unsupported_wire = provider(
            Tool::Codex,
            "旧 wire API",
            "fixture-wire-provider-secret",
            false,
        );
        unsupported_wire.options.wire_api = Some("grpc".to_owned());
        assert_eq!(
            create_provider_profile(&mut fixture.database, &mut redactor, unsupported_wire)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );

        let mut secret_extra_env = provider(
            Tool::Claude,
            "扩展 env 秘密",
            "fixture-extra-env-provider-secret",
            false,
        );
        secret_extra_env.options.extra_env.insert(
            "ANTHROPIC_CUSTOM_HEADERS".to_owned(),
            "Authorization: Bearer fixture-hidden-header-secret".to_owned(),
        );
        assert_eq!(
            create_provider_profile(&mut fixture.database, &mut redactor, secret_extra_env)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert!(!serde_json::to_string(
            &list_provider_profiles(&fixture.database, Tool::Claude).unwrap()
        )
        .unwrap()
        .contains("fixture-hidden-header-secret"));

        let mut multiline_extra_env = provider(
            Tool::Claude,
            "多行扩展 env",
            "fixture-multiline-provider-secret",
            false,
        );
        multiline_extra_env.options.extra_env.insert(
            "ANTHROPIC_DEFAULT_OPUS_MODEL".to_owned(),
            "first-line\nsecond-line".to_owned(),
        );
        assert_eq!(
            create_provider_profile(&mut fixture.database, &mut redactor, multiline_extra_env)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
    }

    /// Pi 的 `models.json` 是多 provider 文件：逐个候选导入，并且 Apply 后
    /// `models` 的逐模型元数据与 `api` 必须逐字段保留。
    #[test]
    fn pi_provider_import_preserves_every_provider_and_model_metadata() {
        let mut fixture = fixture();
        let agent_dir = fixture.home.join(".pi/agent");
        let models_path = agent_dir.join("models.json");
        fs::create_dir_all(&agent_dir).unwrap();
        let original = r#"{
  "providers": {
    "cc": {
      "baseUrl": "https://cc.example.test/v1",
      "api": "openai-completions",
      "apiKey": "fixture-cc-secret",
      "models": [
        {
          "id": "deepseek/v4.1-flash",
          "contextWindow": 1000000,
          "reasoning": true,
          "cost": { "input": 0.28, "output": 1.11 },
          "thinkingLevelMap": { "off": null, "low": "low" }
        }
      ]
    },
    "gemini": {
      "baseUrl": "https://gemini.example.test/v1",
      "api": "openai-completions",
      "apiKey": "fixture-gemini-secret",
      "models": [{ "id": "gemini-3.8-flash-high", "name": "Gemini 3.8 Flash High" }]
    }
  }
}
"#;
        fs::write(&models_path, original).unwrap();
        fs::write(
            agent_dir.join("settings.json"),
            r#"{"defaultProvider":"cc","defaultModel":"cc/deepseek/v4.1-flash"}"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();

        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Pi,
        )
        .unwrap();
        // 两个 provider 都是候选（旧行为只发现 defaultProvider 那一个）。
        assert_eq!(preview.candidates.len(), 2);
        let cc = preview
            .candidates
            .iter()
            .find(|candidate| candidate.provider_id == "cc")
            .unwrap();
        let gemini = preview
            .candidates
            .iter()
            .find(|candidate| candidate.provider_id == "gemini")
            .unwrap();
        assert_eq!(cc.status, ProviderImportCandidateStatus::Importable);
        assert!(cc.default_provider);
        assert_eq!(cc.api_format.as_deref(), Some("openai-completions"));
        assert_eq!(cc.model_count, 1);
        assert_eq!(cc.default_model, "deepseek/v4.1-flash");
        assert!(!gemini.default_provider);
        let serialized = serde_json::to_string(&preview).unwrap();
        assert!(!serialized.contains("fixture-cc-secret"));
        assert!(!serialized.contains("fixture-gemini-secret"));

        let result = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmProviderImportInput {
                preview_id: preview_id(&preview),
                items: vec![
                    ConfirmProviderImportItem {
                        candidate_id: cc.candidate_id.clone(),
                        name: "CC 渠道".to_owned(),
                    },
                    ConfirmProviderImportItem {
                        candidate_id: gemini.candidate_id.clone(),
                        name: "Gemini 渠道".to_owned(),
                    },
                ],
            },
        )
        .unwrap();
        assert_eq!(result.imported_count, 2);
        let profiles = list_provider_profiles(&fixture.database, Tool::Pi).unwrap();
        assert_eq!(profiles.len(), 2);
        // 默认渠道成为生效档案；只读摘要让用户看到 api 与模型列表确实保留。
        let active = profiles.iter().find(|profile| profile.is_active).unwrap();
        assert_eq!(active.name, "CC 渠道");
        let summary = active.pi.as_ref().unwrap();
        assert_eq!(summary.api_format.as_deref(), Some("openai-completions"));
        assert_eq!(summary.models.len(), 1);
        assert_eq!(summary.models[0].id, "deepseek/v4.1-flash");
        assert!(serde_json::to_string(&profiles)
            .unwrap()
            .contains("deepseek/v4.1-flash"));
        assert!(!serde_json::to_string(&profiles)
            .unwrap()
            .contains("fixture-cc-secret"));

        // 改一次档案也不会丢 `api`/`models`（编辑路径保留 extra 字段）。
        let edited = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: active.id.clone(),
                name: active.name.clone(),
                api_base_url: active.api_base_url.clone(),
                api_key: SecretUpdate::Keep,
                default_model: "second-model".to_owned(),
                options: ProviderOptionsInput::default(),
                row_version: active.row_version,
            },
        )
        .unwrap();
        let summary = edited.pi.as_ref().unwrap();
        assert_eq!(summary.api_format.as_deref(), Some("openai-completions"));
        assert_eq!(summary.models[0].id, "deepseek/v4.1-flash");

        let plan = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Pi,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &plan.preview_id,
            Tool::Pi,
            ArtifactKind::Provider,
        )
        .unwrap();

        let rendered: Value =
            serde_json::from_str(&fs::read_to_string(&models_path).unwrap()).unwrap();
        let cc_entry = &rendered["providers"]["cc"];
        assert_eq!(cc_entry["api"], "openai-completions");
        assert_eq!(cc_entry["baseUrl"], "https://cc.example.test/v1");
        // 逐模型元数据必须逐字段保留。
        assert_eq!(cc_entry["models"][0]["id"], "deepseek/v4.1-flash");
        assert_eq!(cc_entry["models"][0]["contextWindow"], 1000000);
        assert_eq!(cc_entry["models"][0]["reasoning"], true);
        assert_eq!(cc_entry["models"][0]["cost"]["output"], 1.11);
        assert!(cc_entry["models"][0]["thinkingLevelMap"]["off"].is_null());
        // 编辑后追加的新默认模型出现在数组里，原条目不动。
        assert_eq!(cc_entry["models"][1]["id"], "second-model");
        // 未受管的另一个 provider 逐字节不变。
        assert_eq!(
            rendered["providers"]["gemini"],
            serde_json::from_str::<Value>(original)
                .unwrap()
                .get("providers")
                .unwrap()
                .get("gemini")
                .unwrap()
                .clone()
        );
    }

    /// 只选一个导入后，另一个仍可继续导入；受管基线取并集而不是覆盖。
    #[test]
    fn pi_provider_import_supports_incremental_import_and_unions_baseline() {
        let mut fixture = fixture();
        let agent_dir = fixture.home.join(".pi/agent");
        let models_path = agent_dir.join("models.json");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::write(
            &models_path,
            r#"{
  "providers": {
    "cc": { "baseUrl": "https://cc.example.test/v1", "apiKey": "fixture-cc", "models": [{ "id": "m1" }] },
    "gemini": { "baseUrl": "https://gemini.example.test/v1", "apiKey": "fixture-gemini", "models": [{ "id": "m2" }] }
  }
}
"#,
        )
        .unwrap();
        fs::write(
            agent_dir.join("settings.json"),
            r#"{"defaultProvider":"cc"}"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();

        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Pi,
        )
        .unwrap();
        let cc = preview
            .candidates
            .iter()
            .find(|candidate| candidate.provider_id == "cc")
            .unwrap();
        confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmProviderImportInput {
                preview_id: preview_id(&preview),
                items: vec![ConfirmProviderImportItem {
                    candidate_id: cc.candidate_id.clone(),
                    name: "CC".to_owned(),
                }],
            },
        )
        .unwrap();
        assert_eq!(
            list_provider_profiles(&fixture.database, Tool::Pi)
                .unwrap()
                .len(),
            1
        );

        // 再次检测：已导入的 provider 标记为已纳入管理，另一个仍可导入。
        let second = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Pi,
        )
        .unwrap();
        assert!(second.candidates.iter().any(|candidate| {
            candidate.provider_id == "cc"
                && candidate.status == ProviderImportCandidateStatus::AlreadyManaged
        }));
        let gemini = second
            .candidates
            .iter()
            .find(|candidate| candidate.provider_id == "gemini")
            .unwrap();
        assert_eq!(gemini.status, ProviderImportCandidateStatus::Importable);
        confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmProviderImportInput {
                preview_id: preview_id(&second),
                items: vec![ConfirmProviderImportItem {
                    candidate_id: gemini.candidate_id.clone(),
                    name: "Gemini".to_owned(),
                }],
            },
        )
        .unwrap();
        assert_eq!(
            list_provider_profiles(&fixture.database, Tool::Pi)
                .unwrap()
                .len(),
            2
        );

        // 基线是并集：Apply 不会因为第二个档案而删掉第一个 provider。
        let stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT baseline_projection_json FROM managed_targets
                 WHERE tool = 'pi' AND artifact_kind = 'provider' AND scope = 'global'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let baseline: Value = serde_json::from_str(&stored).unwrap();
        assert!(baseline["providers"]["cc"].is_object());
        assert!(baseline["providers"]["gemini"].is_object());

        let plan = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Pi,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &plan.preview_id,
            Tool::Pi,
            ArtifactKind::Provider,
        )
        .unwrap();
        let rendered: Value =
            serde_json::from_str(&fs::read_to_string(&models_path).unwrap()).unwrap();
        assert_eq!(rendered["providers"]["cc"]["models"][0]["id"], "m1");
        assert_eq!(rendered["providers"]["gemini"]["models"][0]["id"], "m2");
    }

    /// 重复导入同一 provider 必须被拒绝，而不是产生第二份档案。
    #[test]
    fn pi_provider_import_rejects_a_duplicate_provider() {
        let mut fixture = fixture();
        let agent_dir = fixture.home.join(".pi/agent");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::write(
            agent_dir.join("models.json"),
            r#"{"providers": {"cc": {"baseUrl": "https://cc.example.test/v1", "apiKey": "fixture-cc", "models": [{"id": "m1"}]}}}"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Pi,
        )
        .unwrap();
        let candidate = single_candidate(&preview);
        confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmProviderImportInput {
                preview_id: preview_id(&preview),
                items: vec![ConfirmProviderImportItem {
                    candidate_id: candidate.candidate_id.clone(),
                    name: "CC".to_owned(),
                }],
            },
        )
        .unwrap();

        // 旧上下文已被消费。
        let consumed = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmProviderImportInput {
                preview_id: preview_id(&preview),
                items: vec![ConfirmProviderImportItem {
                    candidate_id: candidate.candidate_id.clone(),
                    name: "CC 再来一次".to_owned(),
                }],
            },
        )
        .unwrap_err();
        assert_eq!(
            consumed.code(),
            crate::error::ErrorCode::PreviewAlreadyConsumed
        );

        // 新一次检测：该 provider 已是 already_managed，强行确认必须冲突。
        let again = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Pi,
        )
        .unwrap();
        assert!(again.preview_id.is_none());
        assert!(again
            .candidates
            .iter()
            .all(|candidate| candidate.status == ProviderImportCandidateStatus::AlreadyManaged));
        assert_eq!(
            list_provider_profiles(&fixture.database, Tool::Pi)
                .unwrap()
                .len(),
            1
        );
    }

    /// 条目字段不完整时只作废该条目，同文件其他 provider 仍可导入。
    #[test]
    fn pi_provider_import_marks_incomplete_entry_invalid_without_blocking_others() {
        let mut fixture = fixture();
        let agent_dir = fixture.home.join(".pi/agent");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::write(
            agent_dir.join("models.json"),
            r#"{
  "providers": {
    "no-key": { "baseUrl": "https://no-key.example.test/v1", "models": [{ "id": "m" }] },
    "scalar": "not-an-object",
    "good": { "baseUrl": "https://good.example.test/v1", "apiKey": "fixture-good", "models": [{ "id": "m" }] }
  }
}"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Pi,
        )
        .unwrap();
        let status = |provider_id: &str| {
            preview
                .candidates
                .iter()
                .find(|candidate| candidate.provider_id == provider_id)
                .unwrap()
                .clone()
        };
        assert_eq!(
            status("no-key").status,
            ProviderImportCandidateStatus::Invalid
        );
        assert_eq!(
            status("no-key").reason.as_deref(),
            Some(crate::adapters::pi::PI_PROVIDER_FIELDS_INVALID)
        );
        assert_eq!(
            status("scalar").status,
            ProviderImportCandidateStatus::Invalid
        );
        assert_eq!(
            status("scalar").reason.as_deref(),
            Some(crate::adapters::pi::PI_PROVIDER_ENTRY_INVALID)
        );
        assert_eq!(
            status("good").status,
            ProviderImportCandidateStatus::Importable
        );

        // 直接确认不可导入候选必须被拒绝。
        let invalid = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmProviderImportInput {
                preview_id: preview_id(&preview),
                items: vec![ConfirmProviderImportItem {
                    candidate_id: status("no-key").candidate_id,
                    name: "坏的".to_owned(),
                }],
            },
        )
        .unwrap_err();
        assert_eq!(invalid.code(), crate::error::ErrorCode::InvalidInput);
        assert!(list_provider_profiles(&fixture.database, Tool::Pi)
            .unwrap()
            .is_empty());
    }

    /// 没有 `defaultProvider` 时也要枚举全部候选（旧行为直接放弃整份文件）。
    #[test]
    fn pi_provider_import_without_default_provider_still_lists_every_candidate() {
        let mut fixture = fixture();
        let agent_dir = fixture.home.join(".pi/agent");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::write(
            agent_dir.join("models.json"),
            r#"{"providers": {"a": {"baseUrl": "https://a.test/v1", "apiKey": "k", "models": [{"id": "m"}]}, "b": {"baseUrl": "https://b.test/v1", "apiKey": "k", "models": [{"id": "m"}]}}}"#,
        )
        .unwrap();
        fs::write(agent_dir.join("settings.json"), "{}").unwrap();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Pi,
        )
        .unwrap();
        assert_eq!(preview.candidates.len(), 2);
        assert!(preview.preview_id.is_some());
        assert!(preview
            .candidates
            .iter()
            .all(|candidate| candidate.status == ProviderImportCandidateStatus::Importable));
    }

    /// 手改 models.json 后「按原生内容接管」：档案改按文件内容，且下次 Apply 不再改写文件。
    #[test]
    fn pi_provider_adopt_native_takes_the_file_content_as_authority() {
        let mut fixture = fixture();
        let agent_dir = fixture.home.join(".pi/agent");
        let models_path = agent_dir.join("models.json");
        fs::create_dir_all(&agent_dir).unwrap();
        let original = r#"{
  "providers": {
    "cc": {
      "baseUrl": "https://cc.example.test/v1",
      "api": "openai-completions",
      "apiKey": "fixture-cc",
      "models": [{ "id": "m1", "contextWindow": 1000 }]
    }
  }
}
"#;
        fs::write(&models_path, original).unwrap();
        fs::write(
            agent_dir.join("settings.json"),
            r#"{"defaultProvider":"cc","defaultModel":"cc/m1"}"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Pi,
        )
        .unwrap();
        let candidate = single_candidate(&preview);
        confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmProviderImportInput {
                preview_id: preview_id(&preview),
                items: vec![ConfirmProviderImportItem {
                    candidate_id: candidate.candidate_id.clone(),
                    name: "CC".to_owned(),
                }],
            },
        )
        .unwrap();

        // 用户手改原生文件：改了一个模型的元数据，并新增一个模型。
        let edited = original
            .replace("\"contextWindow\": 1000", "\"contextWindow\": 2000")
            .replace(
                r#"{ "id": "m1", "contextWindow": 2000 }"#,
                r#"{ "id": "m1", "contextWindow": 2000 }, { "id": "m2", "name": "Second" }"#,
            );
        fs::write(&models_path, &edited).unwrap();

        // 受管内容被外部修改 → 冲突，且提供「重新接管」入口。
        let conflicted = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Pi,
        )
        .unwrap();
        assert_eq!(
            conflicted.targets[0].change_kind,
            crate::domain::ChangeKind::Update
        );
        assert!(conflicted.targets[0].readopt_available);

        // 按原生内容接管：只接管漂移的渠道，且不写原生文件。
        let adopted = adopt_provider_native(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            AdoptProviderNativeInput {
                preview_id: None,
                tool: Tool::Pi,
                target_id: conflicted.targets[0].target_id.clone(),
                target_row_version: conflicted.targets[0].target_row_version,
                target_path: models_path.to_str().unwrap().to_owned(),
                row_versions: conflicted.targets[0].row_versions.clone(),
                observed_full_hash: None,
            },
        )
        .unwrap();
        assert_eq!(adopted.adopted, vec!["CC".to_owned()]);
        assert_eq!(fs::read_to_string(&models_path).unwrap(), edited);

        // 档案内容改为以文件为准（只读摘要里能看到手改后的模型列表）。
        let profiles = list_provider_profiles(&fixture.database, Tool::Pi).unwrap();
        assert_eq!(profiles.len(), 1);
        let summary = profiles[0].pi.as_ref().unwrap();
        assert_eq!(summary.api_format.as_deref(), Some("openai-completions"));
        assert_eq!(summary.models.len(), 2);
        assert_eq!(summary.models[0].id, "m1");
        assert_eq!(summary.models[1].id, "m2");

        // 接管后重新预览：不再冲突（基线已随档案一起刷新）。
        let settled = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Pi,
        )
        .unwrap();
        assert!(matches!(
            settled.targets[0].change_kind,
            crate::domain::ChangeKind::Unchanged | crate::domain::ChangeKind::Warning
        ));
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &settled.preview_id,
            Tool::Pi,
            ArtifactKind::Provider,
        )
        .unwrap();
        // Apply 不得回写用户手改的原生内容。
        assert_eq!(fs::read_to_string(&models_path).unwrap(), edited);
    }

    /// 已漂移但行版本过期时拒绝接管，避免覆盖其他窗口的并发编辑。
    #[test]
    fn pi_provider_adopt_native_rejects_a_stale_row_version() {
        let mut fixture = fixture();
        let agent_dir = fixture.home.join(".pi/agent");
        let models_path = agent_dir.join("models.json");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::write(
            &models_path,
            r#"{"providers": {"cc": {"baseUrl": "https://cc.example.test/v1", "apiKey": "fixture-cc", "models": [{"id": "m1"}]}}}"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Pi,
        )
        .unwrap();
        let candidate = single_candidate(&preview);
        confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmProviderImportInput {
                preview_id: preview_id(&preview),
                items: vec![ConfirmProviderImportItem {
                    candidate_id: candidate.candidate_id.clone(),
                    name: "CC".to_owned(),
                }],
            },
        )
        .unwrap();
        // 手改原生文件造成漂移，并取得预览绑定的行版本。
        fs::write(
            &models_path,
            r#"{"providers": {"cc": {"baseUrl": "https://cc.example.test/v1", "apiKey": "fixture-cc", "models": [{"id": "m1"}, {"id": "m2"}]}}}"#,
        )
        .unwrap();
        let plan = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Pi,
        )
        .unwrap();
        let preview_versions = plan.targets[0].row_versions.clone();
        // 另一个窗口更新了档案：预览绑定的版本已过期。
        fixture
            .database
            .connection_mut()
            .execute(
                "UPDATE provider_profiles SET row_version = row_version + 1",
                [],
            )
            .unwrap();

        let stale = adopt_provider_native(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            AdoptProviderNativeInput {
                preview_id: None,
                tool: Tool::Pi,
                target_id: plan.targets[0].target_id.clone(),
                target_row_version: plan.targets[0].target_row_version,
                target_path: models_path.to_str().unwrap().to_owned(),
                row_versions: preview_versions,
                observed_full_hash: None,
            },
        )
        .unwrap_err();
        assert_eq!(stale.code(), crate::error::ErrorCode::StalePreview);
    }
}
