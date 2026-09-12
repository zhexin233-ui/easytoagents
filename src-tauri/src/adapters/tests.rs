#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink, path::PathBuf};

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        canonicalize_project_root, parse_jsonc, render_cursor_mdc, strip_mdc_frontmatter,
        CapabilityState, ConservativeClaudeCustomizationPolicyProbe,
        ConservativeClaudeUserMcpProbe, DiscoveryContext, ExplicitEnvironment, ManagedOwnership,
        ObservedRaw, PolicyState, PromptOverrideState, RenderedTarget, TargetFormat,
        TargetTrustState, ToolAdapter, ToolAvailability, ToolAvailabilityState,
        VerifiedClaudeCustomizationPolicyEvidence, VerifiedClaudeUserMcpEvidence,
    };
    use crate::{
        adapters::{claude::ClaudeAdapter, codex::CodexAdapter},
        domain::{ArtifactKind, Scope, Tool},
    };

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/phase2")
            .join(name)
    }

    #[test]
    fn cursor_mdc_frontmatter_strip_is_inverse_of_render() {
        let body = "# 团队规范\n\n- 使用简体中文回复\n";
        let rendered = render_cursor_mdc(body);
        assert_eq!(
            rendered,
            "---\nalwaysApply: true\n---\n\n# 团队规范\n\n- 使用简体中文回复\n"
        );
        assert_eq!(strip_mdc_frontmatter(&rendered), body);

        // 正文以 `---` 开头：只剥离首个 frontmatter 块，正文原样保留。
        let tricky_body = "---\nother: yaml\n---\n\n正文";
        assert_eq!(
            strip_mdc_frontmatter(&render_cursor_mdc(tricky_body)),
            tricky_body
        );

        // 无 frontmatter 的文件按原文返回。
        assert_eq!(strip_mdc_frontmatter("# 纯规则\n"), "# 纯规则\n");
        assert_eq!(strip_mdc_frontmatter(""), "");

        // 未闭合的 frontmatter 不剥除（可能是正文的一部分）。
        assert_eq!(
            strip_mdc_frontmatter("---\nalwaysApply: true\n"),
            "---\nalwaysApply: true\n"
        );

        // CRLF 文件：剥离 frontmatter 后正文保留（正文自身的行尾不被改写）。
        let crlf = "---\r\nalwaysApply: true\r\n---\r\n\r\n规则正文\r\n";
        assert_eq!(strip_mdc_frontmatter(crlf), "规则正文\r\n");

        // 空正文渲染为仅含 frontmatter 的文件，剥离后得到空串。
        assert_eq!(strip_mdc_frontmatter(&render_cursor_mdc("")), "");
    }

    #[test]
    fn tool_availability_is_indexed_by_tool() {
        let availability = ToolAvailability::from_states([
            ToolAvailabilityState::Installed,
            ToolAvailabilityState::Unavailable,
            ToolAvailabilityState::Unsupported,
            ToolAvailabilityState::Installed,
            ToolAvailabilityState::Unavailable,
        ]);
        assert_eq!(
            availability[crate::domain::Tool::Claude],
            ToolAvailabilityState::Installed
        );
        assert_eq!(
            availability[crate::domain::Tool::Cursor],
            ToolAvailabilityState::Unsupported
        );
        assert_eq!(
            availability.get(crate::domain::Tool::Opencode),
            ToolAvailabilityState::Unavailable
        );
    }

    #[test]
    fn adapters_populate_write_boundaries_and_mcp_containers() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let project = home.join("project");
        fs::create_dir(&project).unwrap();
        let project = canonicalize_project_root(&project).unwrap();
        let environment = environment(&home, None, None);
        let user_probe = ConservativeClaudeUserMcpProbe;
        let context = DiscoveryContext {
            environment: &environment,
            project_root: Some(&project),
            claude_user_mcp_probe: &user_probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };

        for tool in crate::domain::Tool::ALL {
            let targets = tool.adapter().discover(&context).unwrap();
            assert!(targets.iter().all(|target| target.allowed_root.is_some()));
            for target in targets {
                if target.artifact_kind == ArtifactKind::Mcp {
                    assert_eq!(
                        target.mcp_container.as_deref(),
                        Some(
                            crate::adapters::native_mcp_container(target.tool)
                                .iter()
                                .map(|segment| (*segment).to_owned())
                                .collect::<Vec<_>>()
                                .as_slice()
                        )
                    );
                }
            }
        }
    }

    #[test]
    fn agent_descriptors_follow_each_tool_contract() {
        // 五工具 Agent descriptor 矩阵（官方子代理目录合同，2026-09-12 核验）：
        // 全局五工具目录齐备；项目级仅 ZCode 不支持且无路径。
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let project = home.join("project");
        fs::create_dir(&project).unwrap();
        let project = canonicalize_project_root(&project).unwrap();
        let environment = environment(&home, None, None);
        let user_probe = ConservativeClaudeUserMcpProbe;
        let context = DiscoveryContext {
            environment: &environment,
            project_root: Some(&project),
            claude_user_mcp_probe: &user_probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };

        let expected_global_directory = |tool: Tool, environment: &ExplicitEnvironment| {
            match tool {
                Tool::Claude => environment.claude_config_dir().join("agents"),
                Tool::Codex => environment.codex_home().join("agents"),
                Tool::Cursor => environment.home().join(".cursor/agents"),
                Tool::Zcode => environment.home().join(".zcode/agents"),
                Tool::Opencode => environment.opencode_config_dir().join("agents"),
            }
        };
        let expected_allowed_root = |tool: Tool, environment: &ExplicitEnvironment| {
            match tool {
                Tool::Claude => environment.claude_config_dir().to_path_buf(),
                Tool::Codex => environment.codex_home().to_path_buf(),
                Tool::Cursor => environment.home().join(".cursor"),
                Tool::Zcode => environment.home().join(".zcode"),
                Tool::Opencode => environment.opencode_config_dir().to_path_buf(),
            }
        };

        for tool in Tool::ALL {
            let targets = tool.adapter().discover(&context).unwrap();
            let global = targets
                .iter()
                .find(|target| {
                    target.artifact_kind == ArtifactKind::Agent && target.scope == Scope::Global
                })
                .unwrap_or_else(|| panic!("{tool} 缺少全局 Agent descriptor"));
            let directory = expected_global_directory(tool, &environment);
            assert_eq!(
                global.path.as_deref(),
                Some(directory.to_str().unwrap()),
                "{tool} 全局 Agent 目录不符"
            );
            assert_eq!(
                global.allowed_root.as_deref(),
                Some(expected_allowed_root(tool, &environment).to_str().unwrap()),
                "{tool} 全局 Agent allowed_root 必须是工具配置根"
            );
            assert_eq!(
                global.format,
                if tool == Tool::Codex {
                    TargetFormat::Toml
                } else {
                    TargetFormat::Markdown
                }
            );
            assert_eq!(
                crate::adapters::agent_file_extension(tool),
                if tool == Tool::Codex { "toml" } else { "md" }
            );
            assert_eq!(
                global.capability.state,
                CapabilityState::Supported,
                "{tool} 全局 Agents 必须受支持"
            );
            if tool == Tool::Claude {
                // strictPluginOnlyCustomization 封锁本地 agents：与 skill
                // 同一策略字段。保守探针返回 Unknown，fail closed。
                assert_eq!(global.policy, PolicyState::Unknown);
            }

            let project_descriptor = targets
                .iter()
                .find(|target| {
                    target.artifact_kind == ArtifactKind::Agent && target.scope == Scope::Project
                })
                .unwrap_or_else(|| panic!("{tool} 缺少项目级 Agent descriptor"));
            if tool == Tool::Zcode {
                // 项目级官方明示不支持：无路径 + 稳定诊断码，杜绝任何写入。
                assert!(project_descriptor.path.is_none());
                assert_eq!(
                    project_descriptor.capability.state,
                    CapabilityState::Unsupported
                );
                assert_eq!(
                    project_descriptor.capability.diagnostic_code.as_deref(),
                    Some("ZCODE_PROJECT_AGENTS_UNSUPPORTED")
                );
                continue;
            }
            assert_eq!(
                project_descriptor.capability.state,
                CapabilityState::Supported,
                "{tool} 项目级 Agents 必须受支持"
            );
            let expected_project_directory = match tool {
                Tool::Claude => std::path::PathBuf::from(project.as_str()).join(".claude/agents"),
                Tool::Codex => std::path::PathBuf::from(project.as_str()).join(".codex/agents"),
                Tool::Cursor => std::path::PathBuf::from(project.as_str()).join(".cursor/agents"),
                Tool::Opencode => {
                    std::path::PathBuf::from(project.as_str()).join(".opencode/agents")
                }
                Tool::Zcode => unreachable!("ZCode 项目级已 continue"),
            };
            assert_eq!(
                project_descriptor.path.as_deref(),
                Some(expected_project_directory.to_str().unwrap()),
                "{tool} 项目级 Agent 目录不符"
            );
            assert_eq!(
                project_descriptor.allowed_root.as_deref(),
                Some(project.as_str()),
                "{tool} 项目级 Agent 写入边界必须是项目根"
            );
            if tool == Tool::Codex {
                // 项目级 `.codex` 信任层与项目 MCP/Skills/Hooks 相同；
                // 无 trust fixture 时保持 Unknown（fail closed）。
                let codex_mcp = targets
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp
                            && target.scope == Scope::Project
                    })
                    .unwrap();
                assert_eq!(project_descriptor.trust, codex_mcp.trust);
                assert_eq!(project_descriptor.trust, TargetTrustState::Unknown);
            }

            // 文件级 descriptor：目录 + <name>.<ext>；allowed_root 保持为目录。
            let file_descriptor = global
                .for_agent_file("code-reviewer", crate::adapters::agent_file_extension(tool))
                .unwrap();
            let expected_file = expected_global_directory(tool, &environment).join(format!(
                "code-reviewer.{}",
                crate::adapters::agent_file_extension(tool)
            ));
            assert_eq!(
                file_descriptor.path.as_deref(),
                Some(expected_file.to_str().unwrap())
            );
            assert_eq!(file_descriptor.allowed_root, global.allowed_root);
            assert_eq!(file_descriptor.capability, global.capability);
            assert_eq!(file_descriptor.policy, global.policy);
            // 非法文件名 fail closed。
            assert!(global.for_agent_file("../escape", "md").is_err());
            assert!(global.for_agent_file("a/b", "md").is_err());
        }
    }

    #[test]
    fn jsonc_parser_preserves_unicode_and_comment_like_string_content() {
        let source = r#"{
  // 用户可读的说明
  "description": "中文 🌏 // 不是注释",
  "nested": {
    "提示": "保留 /* 字符串内容 */",
  },
}
"#;
        let value = parse_jsonc(source).unwrap();
        assert_eq!(value["description"], "中文 🌏 // 不是注释");
        assert_eq!(value["nested"]["提示"], "保留 /* 字符串内容 */");
    }

    #[test]
    fn jsonc_parser_rejects_duplicate_keys_at_any_depth() {
        assert!(parse_jsonc(r#"{"provider": {}, "provider": {}}"#).is_err());
        assert!(parse_jsonc(r#"{"provider": {"name": "a", "name": "b"}}"#).is_err());
    }

    #[test]
    fn jsonc_root_replacement_keeps_unmanaged_unicode_and_comments() {
        let source = r#"{
  // provider-owned comment
  "provider": {"fixture": {"name": "old"}},
  "unmanaged": "未受管 / /* 保留 */",
}
"#;
        let desired = json!({
            "provider": {"fixture": {"name": "new"}},
            "unmanaged": "未受管 / /* 保留 */",
        });
        let roots = vec!["provider".to_owned()];
        let rendered = super::replace_jsonc_roots(source, &desired, &roots);
        assert!(rendered.contains("provider-owned comment"));
        assert!(rendered.contains("未受管 / /* 保留 */"));
        assert_eq!(
            parse_jsonc(&rendered).unwrap()["provider"]["fixture"]["name"],
            "new"
        );
    }

    fn environment(
        home: &std::path::Path,
        claude_root: Option<PathBuf>,
        codex_root: Option<PathBuf>,
    ) -> ExplicitEnvironment {
        ExplicitEnvironment::new(
            home,
            claude_root,
            codex_root,
            ToolAvailability::all_installed(),
        )
        .unwrap()
    }

    #[test]
    fn default_and_override_matrix_never_reads_process_tool_environment() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let project = home.join("project");
        fs::create_dir(&project).unwrap();
        let project_root = fs::canonicalize(&project).unwrap();
        let project = canonicalize_project_root(&project).unwrap();
        let default_environment = environment(&home, None, None);
        let conservative_probe = ConservativeClaudeUserMcpProbe;
        let default_context = DiscoveryContext {
            environment: &default_environment,
            project_root: Some(&project),
            claude_user_mcp_probe: &conservative_probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };

        let claude = ClaudeAdapter.discover(&default_context).unwrap();
        let claude_mcp = claude
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
            })
            .unwrap();
        assert_eq!(
            claude_mcp.path.as_deref(),
            Some(home.join(".claude.json").to_str().unwrap())
        );
        assert_eq!(
            claude
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Prompt)
                .unwrap()
                .path
                .as_deref(),
            Some(home.join(".claude/CLAUDE.md").to_str().unwrap())
        );

        let custom_claude = home.join("custom-claude");
        let custom_codex = home.join("custom-codex");
        let custom_environment = environment(
            &home,
            Some(custom_claude.clone()),
            Some(custom_codex.clone()),
        )
        .with_claude_installation_version("fixture-1.0.0")
        .unwrap();
        let custom_context = DiscoveryContext {
            environment: &custom_environment,
            project_root: Some(&project),
            claude_user_mcp_probe: &conservative_probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let claude = ClaudeAdapter.discover(&custom_context).unwrap();
        let unsupported_mcp = claude
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
            })
            .unwrap();
        assert_eq!(unsupported_mcp.path, None);
        assert_eq!(
            unsupported_mcp.capability.state,
            CapabilityState::Unsupported
        );

        let codex = CodexAdapter.discover(&custom_context).unwrap();
        assert_eq!(
            codex
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Provider)
                .unwrap()
                .path
                .as_deref(),
            Some(custom_codex.join("config.toml").to_str().unwrap())
        );
        assert_eq!(
            codex
                .iter()
                .find(|target| {
                    target.artifact_kind == ArtifactKind::Skill && target.scope == Scope::Global
                })
                .unwrap()
                .path
                .as_deref(),
            Some(custom_codex.join("skills").to_str().unwrap()),
            "Codex 用户 Skills 跟随 CODEX_HOME，与 Codex 自身读取规则一致"
        );
        assert_eq!(
            codex
                .iter()
                .find(|target| {
                    target.artifact_kind == ArtifactKind::Skill && target.scope == Scope::Project
                })
                .unwrap()
                .path
                .as_deref(),
            Some(project_root.join(".codex/skills").to_str().unwrap()),
            "Codex 项目级 Skills 固定在项目 .codex/skills"
        );

        let verified_path = home.join("verified/user-mcp.json");
        let evidence =
            VerifiedClaudeUserMcpEvidence::new("fixture-1.0.0", &custom_claude, &verified_path)
                .unwrap();
        let verified_context = DiscoveryContext {
            environment: &custom_environment,
            project_root: None,
            claude_user_mcp_probe: &evidence,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let verified = ClaudeAdapter.discover(&verified_context).unwrap();
        let verified_mcp = verified
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        assert_eq!(
            verified_mcp.path.as_deref(),
            Some(verified_path.to_str().unwrap())
        );
        assert_eq!(verified_mcp.capability.state, CapabilityState::Supported);

        let default_with_version = environment(&home, None, None)
            .with_claude_installation_version("fixture-1.0.0")
            .unwrap();
        let invalid_default_evidence = VerifiedClaudeUserMcpEvidence::new(
            "fixture-1.0.0",
            default_with_version.claude_config_dir(),
            home.join("unexpected-default-user-mcp.json"),
        )
        .unwrap();
        let fixed_default = ClaudeAdapter
            .discover(&DiscoveryContext {
                environment: &default_with_version,
                project_root: None,
                claude_user_mcp_probe: &invalid_default_evidence,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap();
        assert_eq!(
            fixed_default
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Mcp)
                .unwrap()
                .path
                .as_deref(),
            Some(home.join(".claude.json").to_str().unwrap()),
            "默认 Claude 配置根的用户 MCP 位置不能被外部证据改写"
        );
    }

    #[test]
    fn project_root_is_canonicalized_and_symlink_alias_is_not_persisted() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let project = home.join("real-project");
        fs::create_dir(&project).unwrap();
        let alias = home.join("project-alias");
        symlink(&project, &alias).unwrap();

        let canonical = canonicalize_project_root(&alias).unwrap();
        assert_eq!(canonical.as_str(), project.to_str().unwrap());

        let loop_one = home.join("loop-one");
        let loop_two = home.join("loop-two");
        symlink(&loop_two, &loop_one).unwrap();
        symlink(&loop_one, &loop_two).unwrap();
        assert_eq!(
            canonicalize_project_root(&loop_one).unwrap_err().code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert_eq!(
            canonicalize_project_root(&home.join("missing-project"))
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::NotFound
        );
    }

    #[test]
    fn missing_override_root_is_canonicalized_from_its_existing_symlink_ancestor() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let outside = home.join("real-config-parent");
        fs::create_dir(&outside).unwrap();
        let alias = home.join("config-alias");
        symlink(&outside, &alias).unwrap();
        let environment = environment(&home, Some(alias.join("claude")), None);

        assert_eq!(environment.claude_config_dir(), outside.join("claude"));
    }

    #[test]
    fn unavailable_tools_have_a_distinct_capability_state() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment =
            ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_unavailable())
                .unwrap();
        let probe = ConservativeClaudeUserMcpProbe;
        let context = DiscoveryContext {
            environment: &environment,
            project_root: None,
            claude_user_mcp_probe: &probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        assert!(ClaudeAdapter
            .discover(&context)
            .unwrap()
            .iter()
            .all(|target| target.capability.state == CapabilityState::ToolNotInstalled));
        assert!(CodexAdapter
            .discover(&context)
            .unwrap()
            .iter()
            .all(|target| target.capability.state == CapabilityState::ToolNotInstalled));
    }

    #[test]
    fn claude_policy_and_codex_trust_are_discovered_from_isolated_fixtures() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let claude_root = home.join("claude-root");
        let codex_root = home.join("codex-root");
        let project_path = home.join("project");
        fs::create_dir_all(&claude_root).unwrap();
        fs::create_dir_all(&codex_root).unwrap();
        fs::create_dir(&project_path).unwrap();
        let codex_fixture = fs::read_to_string(fixture("codex-config.toml"))
            .unwrap()
            .replace("/fixture/project", project_path.to_str().unwrap());
        fs::write(codex_root.join("config.toml"), codex_fixture).unwrap();
        let environment = environment(&home, Some(claude_root), Some(codex_root))
            .with_claude_installation_version("fixture-1.0.0")
            .unwrap();
        let project = canonicalize_project_root(&project_path).unwrap();
        let evidence = VerifiedClaudeUserMcpEvidence::new(
            "fixture-1.0.0",
            environment.claude_config_dir(),
            home.join("verified-user-mcp.json"),
        )
        .unwrap();
        let policy_fixture: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture("claude-policy-blocked.json")).unwrap())
                .unwrap();
        let policy_evidence = VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
            "fixture-1.0.0",
            policy_fixture.get("strictPluginOnlyCustomization"),
        )
        .unwrap();
        let context = DiscoveryContext {
            environment: &environment,
            project_root: Some(&project),
            claude_user_mcp_probe: &evidence,
            claude_customization_policy_probe: &policy_evidence,
        };

        let claude = ClaudeAdapter.discover(&context).unwrap();
        assert!(claude
            .iter()
            .filter(|target| matches!(
                target.artifact_kind,
                ArtifactKind::Mcp | ArtifactKind::Skill
            ))
            .all(|target| target.policy == PolicyState::Blocked));
        let codex = CodexAdapter.discover(&context).unwrap();
        assert_eq!(
            codex
                .iter()
                .find(|target| {
                    target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Project
                })
                .unwrap()
                .trust,
            TargetTrustState::Trusted
        );

        fs::write(
            environment.codex_home().join("config.toml"),
            format!(
                "[projects.\"{}\"]\ntrust_level = \"untrusted\"\n",
                project.as_str()
            ),
        )
        .unwrap();
        let untrusted = CodexAdapter.discover(&context).unwrap();
        assert_eq!(
            untrusted
                .iter()
                .filter(|target| target.scope == Scope::Project)
                .map(|target| target.trust)
                .collect::<Vec<_>>(),
            vec![
                TargetTrustState::Untrusted,
                TargetTrustState::Untrusted,
                TargetTrustState::Untrusted,
                TargetTrustState::Untrusted,
            ]
        );
    }

    #[test]
    fn claude_policy_evidence_distinguishes_mcp_and_skills_and_fails_closed_when_stale() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let current_environment = environment(&home, None, None)
            .with_claude_installation_version("fixture-1.0.0")
            .unwrap();
        let user_mcp_probe = ConservativeClaudeUserMcpProbe;
        let policy = VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
            "fixture-1.0.0",
            Some(&json!(["skills"])),
        )
        .unwrap();
        let targets = ClaudeAdapter
            .discover(&DiscoveryContext {
                environment: &current_environment,
                project_root: None,
                claude_user_mcp_probe: &user_mcp_probe,
                claude_customization_policy_probe: &policy,
            })
            .unwrap();
        assert_eq!(
            targets
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Mcp)
                .unwrap()
                .policy,
            PolicyState::Allowed
        );
        assert_eq!(
            targets
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Skill)
                .unwrap()
                .policy,
            PolicyState::Blocked
        );

        let upgraded_environment = environment(&home, None, None)
            .with_claude_installation_version("fixture-2.0.0")
            .unwrap();
        assert!(ClaudeAdapter
            .discover(&DiscoveryContext {
                environment: &upgraded_environment,
                project_root: None,
                claude_user_mcp_probe: &user_mcp_probe,
                claude_customization_policy_probe: &policy,
            })
            .unwrap()
            .iter()
            .filter(|target| matches!(
                target.artifact_kind,
                ArtifactKind::Mcp | ArtifactKind::Skill
            ))
            .all(|target| target.policy == PolicyState::Unknown));
    }

    #[test]
    fn codex_prompt_override_is_reported_without_following_unknown_links() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let codex_root = home.join("codex-root");
        fs::create_dir(&codex_root).unwrap();
        let current_environment = environment(&home, None, Some(codex_root.clone()));
        let user_mcp_probe = ConservativeClaudeUserMcpProbe;
        let discover = || {
            CodexAdapter
                .discover(&DiscoveryContext {
                    environment: &current_environment,
                    project_root: None,
                    claude_user_mcp_probe: &user_mcp_probe,
                    claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
                })
                .unwrap()
                .into_iter()
                .find(|target| target.artifact_kind == ArtifactKind::Prompt)
                .unwrap()
                .prompt_override
        };

        assert_eq!(discover(), PromptOverrideState::NotPresent);
        fs::write(codex_root.join("AGENTS.override.md"), "覆盖提示词").unwrap();
        assert_eq!(discover(), PromptOverrideState::Present);
        fs::write(codex_root.join("AGENTS.override.md"), "").unwrap();
        assert_eq!(discover(), PromptOverrideState::NotPresent);
        fs::write(codex_root.join("AGENTS.override.md"), " \n\t").unwrap();
        assert_eq!(discover(), PromptOverrideState::NotPresent);
        fs::write(codex_root.join("AGENTS.override.md"), [0xff]).unwrap();
        assert_eq!(discover(), PromptOverrideState::Unknown);
        fs::remove_file(codex_root.join("AGENTS.override.md")).unwrap();
        let outside = home.join("outside-override.md");
        fs::write(&outside, "未知链接内容").unwrap();
        symlink(&outside, codex_root.join("AGENTS.override.md")).unwrap();
        assert_eq!(discover(), PromptOverrideState::Unknown);

        let late_root = home.join("late-codex-root");
        let late_environment = environment(&home, None, Some(late_root.clone()));
        let outside_root = home.join("outside-codex-root");
        fs::create_dir(&outside_root).unwrap();
        fs::write(outside_root.join("AGENTS.override.md"), "不得读取").unwrap();
        symlink(&outside_root, &late_root).unwrap();
        let late_prompt = CodexAdapter
            .discover(&DiscoveryContext {
                environment: &late_environment,
                project_root: None,
                claude_user_mcp_probe: &user_mcp_probe,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap()
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        assert_eq!(late_prompt.prompt_override, PromptOverrideState::Unknown);
    }

    #[test]
    fn toml_projection_render_preserves_unmanaged_tables_and_comments() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let codex_root = home.join("codex-root");
        fs::create_dir(&codex_root).unwrap();
        let environment = environment(&home, None, Some(codex_root));
        let probe = ConservativeClaudeUserMcpProbe;
        let context = DiscoveryContext {
            environment: &environment,
            project_root: None,
            claude_user_mcp_probe: &probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let adapter = CodexAdapter;
        let descriptor = adapter
            .discover(&context)
            .unwrap()
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        let raw = fs::read(fixture("codex-config.toml")).unwrap();
        let document = adapter.parse(&descriptor, ObservedRaw::File(raw)).unwrap();
        let ownership = ManagedOwnership::selectors([
            vec!["model"],
            vec!["model_provider"],
            vec!["model_providers", "easytoagents_fixture"],
        ]);
        let projection = adapter.project_managed(&document, &ownership).unwrap();
        assert_eq!(projection["model"], "fixture-model");

        let desired = json!({
            "model": "replacement-model",
            "model_provider": "easytoagents_fixture",
            "model_providers": {
                "easytoagents_fixture": {
                    "name": "Replacement",
                    "base_url": "https://replacement.invalid/v1",
                    "experimental_bearer_token": "replacement-secret"
                }
            }
        });
        let RenderedTarget::File(rendered) = adapter
            .render(&descriptor, Some(&document), &desired, &ownership)
            .unwrap();
        let rendered = String::from_utf8(rendered).unwrap();
        assert!(
            rendered.contains("# 必须保留的文件头注释"),
            "渲染结果：{rendered}"
        );
        assert!(rendered.contains("# 此注释和表不属于应用管理范围"));
        assert!(rendered.contains("[plugins]"));
        assert!(rendered.contains("[mcp_servers.fixture_user]"));
        assert!(rendered.contains("replacement-model"));
    }
}
