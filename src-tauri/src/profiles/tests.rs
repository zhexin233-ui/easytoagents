#[cfg(test)]
mod tests {
    use std::{fs, sync::Mutex};

    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        apply_profile_preview, confirm_prompt_import, confirm_provider_import,
        copy_provider_profile, create_prompt_profile, create_provider_profile,
        discover_prompt_import, discover_provider_import, get_tool_profile_status,
        list_provider_profiles, preview_prompt_sync, preview_provider_sync,
        set_active_provider_profile, set_global_prompt_assignment, update_prompt_profile,
        update_provider_profile, CopyProviderProfileInput, PromptProfileDto, PromptProfileInput,
        ProviderAuthKind, ProviderOptionsInput, ProviderProfileInput,
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
        domain::{ArtifactKind, Tool},
        profiles::{ConfirmImportInput, SecretUpdate},
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

    #[test]
    fn opencode_provider_discovery_reads_model_and_provider_in_json_and_jsonc() {
        let fixture = fixture();
        let directory = fixture.environment.opencode_config_dir();
        fs::create_dir_all(directory).unwrap();
        assert!(
            super::discover_native_provider(&fixture.environment, Tool::Opencode)
                .unwrap()
                .is_none()
        );
        for (filename, comment) in [("opencode.json", ""), ("opencode.jsonc", "// 已有配置\n")]
        {
            let path = directory.join(filename);
            let content = format!(
                "{{{comment}\"model\":\"fixture/model-a\",\"provider\":{{\"fixture\":{{\"npm\":\"@ai-sdk/openai-compatible\",\"name\":\"测试渠道\",\"options\":{{\"baseURL\":\"https://fixture.invalid/v1\",\"apiKey\":\"fixture-secret\"}}}}}},\"unrelated\":true}}"
            );
            fs::write(&path, &content).unwrap();
            let discovered = super::discover_native_provider(&fixture.environment, Tool::Opencode)
                .unwrap()
                .unwrap();
            assert_eq!(discovered.provider_id.as_deref(), Some("fixture"));
            assert_eq!(discovered.default_model, "model-a");
            assert_eq!(discovered.api_key.as_deref(), Some("fixture-secret"));
            assert_eq!(discovered.target_path, path.to_str().unwrap());
            assert_eq!(fs::read_to_string(&path).unwrap(), content);
        }
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
        .unwrap()
        .expect("fixture 配置应包含一个可导入的 provider");
        assert_eq!(preview_dto.suggested_name, "Fixture Plan");
        assert_eq!(preview_dto.api_base_url, "https://fixture.invalid/v1");
        assert!(preview_dto.api_key_configured);
        let serialized = serde_json::to_string(&preview_dto).unwrap();
        assert!(!serialized.contains("fixture-native-secret"));

        let created = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            crate::profiles::ConfirmImportInput {
                preview_id: preview_dto.preview_id,
                name: "Fixture Plan".to_owned(),
            },
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
        .unwrap()
        .unwrap();
        assert!(preview.api_key_configured);
        assert!(!serde_json::to_string(&preview).unwrap().contains(secret));
        let persisted: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT redacted_preview_json FROM profile_import_previews WHERE id = ?1",
                [&preview.preview_id],
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
        let stale = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "过期导入".to_owned(),
            },
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
        .unwrap()
        .unwrap();

        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "导入渠道".to_owned(),
            },
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
        .unwrap()
        .unwrap();
        assert!(preview.api_key_configured);
        assert_eq!(preview.auth_kind, ProviderAuthKind::ApiKey);
        // 默认模型只来自 ANTHROPIC_MODEL；模型族键作为额外 env 原样保留。
        assert_eq!(preview.default_model, "");
        assert!(preview.skipped_env_keys.is_empty());
        assert!(!serde_json::to_string(&preview).unwrap().contains(secret));
        assert!(preview.redacted_projection["env"]
            .get(CLAUDE_MODEL_KEY)
            .is_none());
        assert_eq!(
            preview.redacted_projection["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            crate::security::REDACTED
        );
        assert_eq!(
            preview.redacted_projection["env"]["ANTHROPIC_AUTH_TOKEN"],
            crate::security::REDACTED
        );

        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "导入默认模型族".to_owned(),
            },
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
        .unwrap()
        .unwrap();
        assert_eq!(preview.default_model, "claude-relay");
        assert_eq!(
            preview.skipped_env_keys,
            vec!["ANTHROPIC_CUSTOM_HEADERS".to_owned(), "SOME_FLAG".to_owned()]
        );
        let serialized = serde_json::to_string(&preview).unwrap();
        assert!(!serialized.contains(secret));
        assert!(!serialized.contains(header_secret));
        assert!(preview.redacted_projection["env"]
            .get("ANTHROPIC_CUSTOM_HEADERS")
            .is_none());
        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "中转渠道".to_owned(),
            },
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
                    extra_env: [("CLAUDE_CODE_MAX_OUTPUT_TOKENS".to_owned(), "64000".to_owned())]
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
        assert_eq!(restored["env"]["ANTHROPIC_BASE_URL"], "https://relay.example.com");
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
        .unwrap()
        .unwrap();
        assert_eq!(preview.auth_kind, ProviderAuthKind::OfficialLogin);
        assert_eq!(preview.api_base_url, "");
        assert_eq!(preview.default_model, "");
        assert!(!preview.api_key_configured);
        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "官方登录".to_owned(),
            },
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
        assert!(discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap()
        .is_none());
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
        .unwrap()
        .unwrap();
        assert_eq!(preview.auth_kind, ProviderAuthKind::ApiKey);
        assert_eq!(preview.default_model, "");
        assert!(preview.api_key_configured);
        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "无模型渠道".to_owned(),
            },
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
        assert_eq!(codex_official.options.provider_id.as_deref(), Some("openai"));
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
        .unwrap()
        .unwrap();
        assert_eq!(preview.default_model, "");
        assert_eq!(preview.auth_kind, ProviderAuthKind::ApiKey);
        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "Relay".to_owned(),
            },
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
        .unwrap()
        .unwrap();
        assert_eq!(preview.suggested_name, "External Fixture");
        let serialized = serde_json::to_string(&preview).unwrap();
        assert!(!serialized.contains(token));
        assert!(!serialized.contains(header));

        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: preview.suggested_name,
            },
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
        .unwrap()
        .unwrap();
        assert_eq!(preview.suggested_name, "Codex 官方账号登录");
        assert_eq!(preview.default_model, "gpt-5.5");
        assert_eq!(preview.auth_kind, ProviderAuthKind::OfficialLogin);
        assert!(!preview.api_key_configured);
        let serialized_preview = serde_json::to_string(&preview).unwrap();
        assert!(!serialized_preview.contains(access_token));
        assert!(!serialized_preview.contains(refresh_token));
        assert_eq!(preview.redacted_projection["model"], "gpt-5.5");
        assert!(preview.redacted_projection.get("model_provider").is_none());

        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "Codex OAuth 登录".to_owned(),
            },
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
        assert!(preview.is_none());
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
}
