//! Pi（`@earendil-works/pi-coding-agent`）适配层。
//!
//! 本模块只实现 `design.md` §1 冻结的**六个** descriptor（Provider 全局、
//! Prompt 全局、Skill 全局/项目、MCP 全局/项目）与只读 codec/探测辅助函数。
//! 阶段 3/4 的数据库迁移、仓储列与服务接线不在本模块内。
//!
//! Pi 的永久 fail-closed 边界：
//! - 无声明式 Hooks（事件是 TypeScript 扩展 API），不生成 Hook descriptor；
//! - 无官方 Agents 目录/schema（`.pi/agents/*.md` 是 Trellis 扩展私有约定），
//!   不生成 Agent descriptor；
//! - MCP 只在第三方 `pi-mcp-adapter` 已声明、已安装且已加载且版本达标时才有
//!   目标路径；适配器未就绪时 `path = None`（写无人读的 `mcp.json` 必然无效）；
//! - `PI_CODING_AGENT_DIR` 无法安全映射时全部 descriptor unsupported 且无路径，
//!   绝不静默回退默认值。
//!
//! 绝对禁区（不得出现任何写路径）：`auth.json`、`models-store.json`、
//! `trust.json`、`settings.json`、`SYSTEM.md`、`APPEND_SYSTEM.md`、
//! `extensions/`、`npm/`、`git/`、`themes/`、`bin/`、`~/.pi/skills`、
//! `~/.agents/skills`、`<root>/.agents/skills`，MCP 共享文件
//! （`~/.config/mcp/mcp.json`、`~/.agents/mcp.json`、`~/.agents/mcp/mcp.json`、
//! `<root>/.mcp.json`）与适配器旁路文件（`mcp-oauth/`、OS 凭据库、
//! `mcp-cache.json`、`mcp-npx-cache.json`、`mcp-onboarding.json`、
//! `agent-plugin-data/`、`.pi/mcp-traces/`）。共享 MCP 文件只做只读同名遮蔽
//! 检测。
//!
//! 只读探测面（与阶段 1 的安装探针一致）：`settings.json` 的 `packages` /
//! `defaultProvider` / `defaultProjectTrust`，以及 `trust.json` 的最近祖先决定。
//! 这些读取只用于映射就绪/选择/信任状态，绝不写回，也不读取任何凭据值。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};

use crate::{
    adapters::{
        descriptor_path, DiscoveryContext, ManagedOwnership, PromptOverrideState, ProviderCodec,
        ProviderCodecDiscovery, ProviderCodecInput, ProviderCodecOptions,
        ProviderCodecProfileInput, SymlinkPolicy, TargetCapability, TargetDescriptor, TargetFormat,
        TargetTrustState, ToolAdapter, ToolAvailabilityState,
    },
    domain::{ArtifactKind, Scope, Tool},
    error::AppError,
};

pub mod probe;

/// 进程存在 `PI_CODING_AGENT_DIR`，但调用方无法把它安全映射为绝对路径
/// （相对路径或 rebrand）。此时不得静默回退默认值：写默认位置意味着写到
/// Pi 实际不会读取的目录，因此全部 Pi descriptor 必须 unsupported。
pub const PI_AGENT_DIR_OVERRIDE_UNMAPPED: &str = "PI_AGENT_DIR_OVERRIDE_UNMAPPED";

/// 安装探针无法得出可信结果（超时、权限、异常输出）。
pub const PI_INSTALLATION_PROBE_UNSUPPORTED: &str = "PI_INSTALLATION_PROBE_UNSUPPORTED";

/// 任何 Pi Hook 请求：Pi 的事件机制是 TypeScript 扩展 API，不存在声明式
/// hook 文件，无法用 command-only 模型表达。
pub const PI_HOOKS_UNSUPPORTED: &str = "PI_HOOKS_UNSUPPORTED";
/// 任何 Pi Agents 请求（全局与项目）：Pi 无官方子代理目录/schema。
pub const PI_AGENTS_UNSUPPORTED: &str = "PI_AGENTS_UNSUPPORTED";
/// 项目级 Pi Prompt 请求：Prompt 为产品政策上的全局独占能力。
pub const PI_PROJECT_PROMPT_UNSUPPORTED: &str = "PI_PROJECT_PROMPT_UNSUPPORTED";

/// `<pi_agent_dir>/AGENTS.override.md` 存在：我们的 `AGENTS.md` 必然无效，
/// Apply 必须硬阻断（不是 warning）。
pub const PI_PROMPT_OVERRIDE_DETECTED: &str = "PI_PROMPT_OVERRIDE_DETECTED";
/// `AGENTS.md` 缺失但存在 `CLAUDE.md`/`CLAUDE.MD`/`AGENTS.MD` 回退文件：
/// 导入不完整且写入会反向遮蔽用户文件。
pub const PI_PROMPT_FALLBACK_PRESENT: &str = "PI_PROMPT_FALLBACK_PRESENT";

/// `PI_MCP_CONFIG_MODE=exclusive` 时项目 `.pi/mcp.json` 被适配器忽略。
pub const PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED: &str = "PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED";
/// 全局受管条目被 `<root>/.mcp.json` 同名 server 覆盖（Apply 硬阻断）。
pub const PI_MCP_SHADOWED_BY_PROJECT_SHARED: &str = "PI_MCP_SHADOWED_BY_PROJECT_SHARED";
/// 全局受管条目被用户手写 `<root>/.pi/mcp.json` 同名 server 覆盖（Apply 硬阻断）。
pub const PI_MCP_SHADOWED_BY_PROJECT_PI: &str = "PI_MCP_SHADOWED_BY_PROJECT_PI";
/// 观测到适配器接受的容器别名 `mcp-servers`（写入仍只写 canonical `mcpServers`）。
pub const PI_MCP_CONTAINER_ALIAS_DETECTED: &str = "PI_MCP_CONTAINER_ALIAS_DETECTED";
/// 受管 MCP 条目被适配器追加 `directTools` 等字段（正常漂移，走重新接管）。
pub const PI_MCP_ADAPTER_WRITES_MANAGED_ENTRY: &str = "PI_MCP_ADAPTER_WRITES_MANAGED_ENTRY";

/// Provider 条目含明文字面 `apiKey`（非 `$ENV`/`!command` 引用）：脱敏 + 提示。
pub const PI_PROVIDER_INLINE_API_KEY: &str = "PI_PROVIDER_INLINE_API_KEY";

/// 受管符号链接断链（Pi 静默忽略，必须自检）。
pub const PI_SKILL_SYMLINK_BROKEN: &str = "PI_SKILL_SYMLINK_BROKEN";
/// 受管符号链接逃逸 allowed_root（Pi 静默忽略，必须自检）。
pub const PI_SKILL_SYMLINK_ESCAPE: &str = "PI_SKILL_SYMLINK_ESCAPE";

/// 项目 `.pi/skills` 在未受信任项目下不会加载。
pub const PI_PROJECT_SKILLS_UNTRUSTED: &str = "PI_PROJECT_SKILLS_UNTRUSTED";
/// 无法判定项目信任状态（trust.json 损坏或缺省 `ask` 且无持久决定）。
pub const PI_PROJECT_SKILLS_TRUST_UNKNOWN: &str = "PI_PROJECT_SKILLS_TRUST_UNKNOWN";

/// Pi 的唯一写入树：`<pi_agent_dir>/models.json`、`AGENTS.md`、`skills/`、
/// `mcp.json`，以及项目 `<root>/.pi/{skills,mcp.json}`。
#[derive(Debug, Default)]
pub struct PiAdapter;

impl PiAdapter {
    /// 六个 descriptor 的公共 capability 基线：`PI_CODING_AGENT_DIR` 不可映射
    /// 优先于一切安装状态；未映射时不得暴露任何默认路径。
    pub fn capability(context: &DiscoveryContext<'_>) -> TargetCapability {
        let environment = context.environment;
        if environment.pi_agent_dir_unmapped() {
            return TargetCapability::unsupported(PI_AGENT_DIR_OVERRIDE_UNMAPPED);
        }
        match environment.tool_availability(Tool::Pi) {
            ToolAvailabilityState::Installed => TargetCapability::supported(),
            ToolAvailabilityState::Unavailable => TargetCapability::tool_not_installed(),
            ToolAvailabilityState::Unsupported => {
                TargetCapability::unsupported(PI_INSTALLATION_PROBE_UNSUPPORTED)
            }
        }
    }
}

impl ToolAdapter for PiAdapter {
    fn tool(&self) -> Tool {
        Tool::Pi
    }

    fn provider_codec(&self) -> Option<&dyn ProviderCodec> {
        Some(self)
    }

    fn discover(&self, context: &DiscoveryContext<'_>) -> Result<Vec<TargetDescriptor>, AppError> {
        let environment = context.environment;
        let mapped = !environment.pi_agent_dir_unmapped();
        let agent_dir = environment.pi_agent_dir();
        let installed = environment.tool_availability(Tool::Pi) == ToolAvailabilityState::Installed;
        let capability = Self::capability(context);
        let supported = capability.state == crate::adapters::CapabilityState::Supported;

        // 不可映射的覆盖：连默认路径都不能暴露，否则用户以为写对了位置。
        let global_path = |relative: &str| -> Option<String> {
            mapped
                .then(|| agent_dir.join(relative))
                .and_then(|path| path.to_str().map(str::to_owned))
        };

        let prompt_override = if mapped && installed {
            discover_prompt_override(&agent_dir.join("AGENTS.override.md"))
        } else {
            PromptOverrideState::Unknown
        };

        let project_trust = context
            .project_root
            .map_or(TargetTrustState::NotRequired, |root| {
                if mapped {
                    discover_project_trust(agent_dir, root.as_str())
                } else {
                    TargetTrustState::Unknown
                }
            });

        let mcp_capability = if supported {
            let probe = probe::probe_mcp_adapter(&probe::PiMcpAdapterProbeInput {
                pi_agent_dir: agent_dir,
                project_root: context.project_root.map(|root| Path::new(root.as_str())),
                project_trusted: project_trust == TargetTrustState::Trusted,
                exclusive_mode: environment.pi_mcp_exclusive_mode(),
            });
            match probe.state.diagnostic_code() {
                None => TargetCapability::supported(),
                Some(code) => TargetCapability::unsupported(code),
            }
        } else {
            capability.clone()
        };
        let mcp_path = |ready_path: &str| -> Option<String> {
            if mcp_capability.state == crate::adapters::CapabilityState::Supported {
                global_path(ready_path)
            } else {
                None
            }
        };

        let mut targets = vec![
            TargetDescriptor::builder(Tool::Pi, ArtifactKind::Provider, Scope::Global)
                .path(global_path("models.json"))
                .format(TargetFormat::Json)
                .managed_selectors(["providers"])
                .sensitive_selectors([
                    "providers/*/apiKey",
                    "providers/*/headers",
                    "providers/*/models/*/headers",
                    "providers/*/modelOverrides/*/headers",
                ])
                .capability(capability.clone())
                .build(),
            TargetDescriptor::builder(Tool::Pi, ArtifactKind::Prompt, Scope::Global)
                .path(global_path("AGENTS.md"))
                .format(TargetFormat::Markdown)
                .managed_selectors(["$document"])
                .capability(capability.clone())
                .prompt_override(prompt_override)
                .build(),
            TargetDescriptor::builder(Tool::Pi, ArtifactKind::Skill, Scope::Global)
                .path(global_path("skills"))
                .format(TargetFormat::SymlinkDirectory)
                .managed_selectors(["$children"])
                .capability(capability.clone())
                .symlink_policy(SymlinkPolicy::ManagedChildrenOnly)
                .build(),
            TargetDescriptor::builder(Tool::Pi, ArtifactKind::Mcp, Scope::Global)
                .path(mcp_path("mcp.json"))
                .format(TargetFormat::Json)
                .managed_selectors(["mcpServers"])
                .sensitive_selectors(MCP_SENSITIVE_SELECTORS)
                .capability(mcp_capability.clone())
                .build(),
        ];

        if let Some(project_root) = context.project_root {
            let root = Path::new(project_root.as_str());
            let project_root_text = Some(project_root.as_str().to_owned());
            // `PI_MCP_CONFIG_MODE=exclusive`：适配器只读 `<pi_agent_dir>/mcp.json`，
            // 项目文件被忽略 → 项目目标必须 unsupported 且无路径。
            let project_mcp_capability = if supported && environment.pi_mcp_exclusive_mode() {
                TargetCapability::unsupported(PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED)
            } else {
                mcp_capability
            };
            let project_mcp_path = (project_mcp_capability.state
                == crate::adapters::CapabilityState::Supported)
                .then(|| root.join(".pi/mcp.json"))
                .and_then(|path| path.to_str().map(str::to_owned));
            targets.extend([
                TargetDescriptor::builder(Tool::Pi, ArtifactKind::Skill, Scope::Project)
                    .project_root(project_root_text.clone())
                    .path(
                        mapped
                            .then(|| path_string(&root.join(".pi/skills")))
                            .transpose()?,
                    )
                    .format(TargetFormat::SymlinkDirectory)
                    .managed_selectors(["$children"])
                    .capability(capability.clone())
                    .trust(project_trust)
                    .symlink_policy(SymlinkPolicy::ManagedChildrenOnly)
                    .build(),
                // `.pi/mcp.json` 不受 Pi project trust 门禁（适配器源码零 trust
                // 读取），因此如实标注 `NotRequired`，但 Apply 前需要安全提示。
                TargetDescriptor::builder(Tool::Pi, ArtifactKind::Mcp, Scope::Project)
                    .project_root(project_root_text)
                    .path(project_mcp_path)
                    .format(TargetFormat::Json)
                    .managed_selectors(["mcpServers"])
                    .sensitive_selectors(MCP_SENSITIVE_SELECTORS)
                    .capability(project_mcp_capability)
                    .build(),
            ]);
        }

        crate::adapters::populate_descriptor_allowed_roots(environment, &mut targets)?;
        Ok(targets)
    }
}

/// 受管 MCP 字段的敏感 selector；不参与写，只用于 preview/diff 脱敏。
const MCP_SENSITIVE_SELECTORS: [&str; 14] = [
    "mcpServers/*/env",
    "mcpServers/*/headers",
    "mcpServers/*/bearerToken",
    "mcpServers/*/bearerTokenEnv",
    "mcpServers/*/oauth",
    "mcpServers/*/requestHeadersCommand/env",
    "mcpServers/*/requestHeadersCommand/args",
    "mcpServers/*/args",
    "mcpServers/*/url",
    "mcpServers/*/requestHeadersCommand/command",
    "mcpServers/*/caFile",
    "mcpServers/*/socket",
    "mcpServers/*/cwd",
    "mcpServers/*/pluginDataDir",
];

fn path_string(path: &Path) -> Result<String, AppError> {
    crate::adapters::path_text(path)
}

impl ProviderCodec for PiAdapter {
    fn discovery_ownership(&self) -> Result<ManagedOwnership, AppError> {
        Ok(ManagedOwnership::selectors([["providers"]]))
    }

    fn ownership(
        &self,
        baseline: Option<&Value>,
        desired: &Value,
    ) -> Result<ManagedOwnership, AppError> {
        // 条目级局部合并：只拥有 baseline/desired 中真实出现的 key，未知字段
        // （modelOverrides、第三方扩展字段）除非由档案显式携带，否则不被触碰。
        // 只给 `baseUrl`/`headers` 且无 `models` 的条目不生成 `models` selector，
        // 因此不会写入 `models: []`，内置模型合并语义得以保留。
        let mut selectors = BTreeSet::<Vec<String>>::new();
        for projection in [baseline, Some(desired)].into_iter().flatten() {
            let Some(providers) = projection.get("providers").and_then(Value::as_object) else {
                continue;
            };
            for (provider_id, entry) in providers {
                let Some(entry) = entry.as_object() else {
                    continue;
                };
                for key in entry.keys() {
                    selectors.insert(vec![
                        "providers".to_owned(),
                        provider_id.clone(),
                        key.clone(),
                    ]);
                }
            }
        }
        if selectors.is_empty() {
            return Err(AppError::invalid_input(
                "managedOwnership",
                "Pi Provider 同步没有可证明拥有的字段",
            ));
        }
        Ok(ManagedOwnership::Selectors(selectors.into_iter().collect()))
    }

    fn default_options(
        &self,
        _input: &ProviderCodecInput<'_>,
    ) -> Result<ProviderCodecOptions, AppError> {
        // Pi Provider 的全部工具专属信息都通过 `extra_provider_fields`
        // 逐字段保留；没有额外的结构化选项。
        Ok(ProviderCodecOptions::default())
    }

    fn render(&self, input: &ProviderCodecProfileInput<'_>) -> Result<Value, AppError> {
        let provider_id = input.provider_id.ok_or_else(|| {
            AppError::invalid_input("providerOptions", "Pi Provider 缺少稳定 provider id")
        })?;
        validate_provider_id(provider_id)?;
        let mut entry = input
            .extra_provider_fields
            .clone()
            .into_iter()
            .collect::<Map<_, _>>();
        set_or_remove(
            &mut entry,
            "baseUrl",
            input
                .api_base_url
                .filter(|value| !value.is_empty())
                .map(|value| Value::String(value.to_owned())),
        );
        // `apiKey` 原样保留：明文直接用字面量，环境变量引用用 `$KEY`。
        // 绝不展开 `$ENV`、绝不执行 `!command`。
        let api_key = match (
            input.api_key.filter(|value| !value.is_empty()),
            input.credential_env_key.filter(|value| !value.is_empty()),
        ) {
            (Some(literal), _) => Some(Value::String(literal.to_owned())),
            (None, Some(env_key)) => Some(Value::String(format!("${env_key}"))),
            (None, None) => None,
        };
        set_or_remove(&mut entry, "apiKey", api_key);
        // 没有默认模型时不写 `models`（不得写成 `models: []`）。
        set_or_remove(
            &mut entry,
            "models",
            input
                .default_model
                .filter(|value| !value.is_empty())
                .map(|model| Value::Array(vec![serde_json::json!({ "id": model })])),
        );
        Ok(serde_json::json!({
            "providers": { provider_id: Value::Object(entry) }
        }))
    }

    fn discover(
        &self,
        descriptor: &TargetDescriptor,
        managed_projection: &Value,
        full_hash: &str,
    ) -> Result<Option<ProviderCodecDiscovery>, AppError> {
        let target_path = descriptor_path(descriptor)?;
        let Some(providers) = managed_projection
            .get("providers")
            .and_then(Value::as_object)
        else {
            return Err(AppError::parse(&target_path, "json"));
        };
        if providers.is_empty() {
            return Ok(None);
        }
        let agent_dir = Path::new(&target_path).parent();
        let default_provider = agent_dir.and_then(read_default_provider);
        let selected = default_provider
            .as_deref()
            .filter(|id| providers.contains_key(*id))
            .map(|id| (id.to_owned(), providers.get(id)))
            .or_else(|| {
                // 没有 `defaultProvider` 时只在唯一 provider 条目下导入；
                // 多条目且无默认值属歧义，fail closed 由用户显式选择。
                (providers.len() == 1)
                    .then(|| providers.iter().next())
                    .flatten()
                    .map(|(id, entry)| (id.clone(), Some(entry)))
            });
        let Some((provider_id, Some(entry))) = selected else {
            return Ok(None);
        };
        let Some(entry) = entry.as_object() else {
            return Ok(None);
        };
        let api_base_url = entry
            .get("baseUrl")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let api_key = entry
            .get("apiKey")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let default_model = agent_dir
            .map(|dir| resolve_default_model(dir, &provider_id, entry))
            .unwrap_or_default();
        let extra_provider_fields = entry
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "baseUrl" | "apiKey" | "models"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        Ok(Some(ProviderCodecDiscovery {
            target_path,
            full_hash: full_hash.to_owned(),
            projection: serde_json::json!({
                "providers": { provider_id.clone(): Value::Object(entry.clone()) }
            }),
            auth_kind: crate::adapters::PROVIDER_AUTH_KIND_API_KEY.to_owned(),
            api_base_url,
            api_key,
            default_model,
            // 与既有非 Claude codec 一致：Pi 不使用 Claude 凭据环境变量契约。
            credential_env_key: "ANTHROPIC_API_KEY".to_owned(),
            extra_env: BTreeMap::new(),
            skipped_env_keys: Vec::new(),
            provider_id: Some(provider_id),
            wire_api: None,
            zcode_kind: None,
            opencode_npm: None,
            opencode_api: None,
            extra_provider_fields,
            suggested_name: entry.get("name").and_then(Value::as_str).map(str::to_owned),
        }))
    }
}

fn set_or_remove(entry: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    match value {
        Some(value) => {
            entry.insert(key.to_owned(), value);
        }
        None => {
            entry.remove(key);
        }
    }
}

/// Pi provider id 只作为 JSON 对象 key 使用；限制为安全、可读的字符集合，
/// 拒绝空值/控制字符/路径分隔符。
fn validate_provider_id(provider_id: &str) -> Result<(), AppError> {
    if provider_id.is_empty()
        || provider_id.len() > 200
        || !provider_id.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'+' | b'@')
        })
    {
        return Err(AppError::invalid_input("providerId", "Pi provider id 非法"));
    }
    Ok(())
}

/// 明文（非 `$ENV`/`!command` 引用）`apiKey` 的提示诊断；空值与引用表达式返回 `None`。
pub fn inline_api_key_diagnostic(api_key: &str) -> Option<&'static str> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() || trimmed.starts_with('$') || trimmed.starts_with('!') {
        None
    } else {
        Some(PI_PROVIDER_INLINE_API_KEY)
    }
}

/// `<pi_agent_dir>/settings.json` 的 `defaultProvider`（只读，非空字符串）。
fn read_default_provider(agent_dir: &Path) -> Option<String> {
    let value = probe::read_json_file(&agent_dir.join("settings.json"))?;
    value
        .get("defaultProvider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// 项目默认模型：优先 `settings.json` 的 `defaultModel`（`<provider>/<model>`），
/// 否则取该 provider `models[]` 的第一个 `id`。
fn resolve_default_model(
    agent_dir: &Path,
    provider_id: &str,
    entry: &Map<String, Value>,
) -> String {
    if let Some(default_model) =
        probe::read_json_file(&agent_dir.join("settings.json")).and_then(|value| {
            value
                .get("defaultModel")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
    {
        if let Some((prefix, model)) = default_model.split_once('/') {
            if prefix == provider_id && !model.is_empty() {
                return model.to_owned();
            }
        }
    }
    entry
        .get("models")
        .and_then(Value::as_array)
        .and_then(|models| {
            models
                .iter()
                .find_map(|model| model.get("id").and_then(Value::as_str))
        })
        .unwrap_or_default()
        .to_owned()
}

/// `<pi_agent_dir>/AGENTS.override.md` 的三态探测（Present / NotPresent / Unknown）。
fn discover_prompt_override(path: &Path) -> PromptOverrideState {
    match read_discovery_file(path) {
        DiscoveryFile::Missing => PromptOverrideState::NotPresent,
        DiscoveryFile::File(bytes) => match std::str::from_utf8(&bytes) {
            Ok(text) if text.trim().is_empty() => PromptOverrideState::NotPresent,
            Ok(_) => PromptOverrideState::Present,
            Err(_) => PromptOverrideState::Unknown,
        },
        DiscoveryFile::Unavailable => PromptOverrideState::Unknown,
    }
}

/// `AGENTS.md` 缺失但存在 `CLAUDE.md`/`CLAUDE.MD`/`AGENTS.MD` 回退文件。
/// 该诊断与 descriptor 的 `prompt_override` 分开：它说明导入不完整且写入会
/// 反向遮蔽用户文件，由服务层在导入/预览文案中透出。
pub fn prompt_fallback_present(agent_dir: &Path) -> bool {
    if matches!(
        read_discovery_file(&agent_dir.join("AGENTS.md")),
        DiscoveryFile::File(_)
    ) {
        return false;
    }
    ["CLAUDE.md", "CLAUDE.MD", "AGENTS.MD"].iter().any(|name| {
        matches!(
            read_discovery_file(&agent_dir.join(name)),
            DiscoveryFile::File(_)
        )
    })
}

/// 项目 `.pi/skills` 的 Pi trust 状态。
///
/// 只读映射（不写回）：`trust.json` 的最近祖先决定优先；无命中时再读
/// `settings.json` 的 `defaultProjectTrust`（`always` → Trusted、`never` →
/// Untrusted）。默认 `ask` 在非交互模式下等于忽略项目资源，但交互模式下会
/// 询问，静态不可判定，因此保守映射为 `Unknown`（Apply 前阻断）。
fn discover_project_trust(agent_dir: &Path, project_root: &str) -> TargetTrustState {
    match read_discovery_file(&agent_dir.join("trust.json")) {
        DiscoveryFile::Missing => {}
        DiscoveryFile::Unavailable => return TargetTrustState::Unknown,
        DiscoveryFile::File(bytes) => {
            let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
                return TargetTrustState::Unknown;
            };
            let Some(entries) = value.as_object() else {
                return TargetTrustState::Unknown;
            };
            let mut candidate = Some(PathBuf::from(project_root));
            while let Some(directory) = candidate {
                if let Some(entry) = directory.to_str().and_then(|key| entries.get(key)) {
                    return match entry {
                        Value::Bool(true) => TargetTrustState::Trusted,
                        Value::Bool(false) => TargetTrustState::Untrusted,
                        _ => TargetTrustState::Unknown,
                    };
                }
                candidate = directory.parent().map(Path::to_path_buf);
            }
        }
    }
    let default_project_trust =
        probe::read_json_file(&agent_dir.join("settings.json")).and_then(|value| {
            value
                .get("defaultProjectTrust")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    match default_project_trust.as_deref() {
        Some("always") => TargetTrustState::Trusted,
        Some("never") => TargetTrustState::Untrusted,
        _ => TargetTrustState::Unknown,
    }
}

/// Pi 的 MCP 文件接受 `mcpServers` 及其别名 `mcp-servers`（`config.ts:733`）。
/// 读取必须同时识别两者，否则会漏读用户条目；写入只写 canonical `mcpServers`。
pub struct ObservedMcpServers<'a> {
    pub servers: &'a Map<String, Value>,
    /// 观测来自别名 `mcp-servers` 而非 canonical 容器。
    pub alias_used: bool,
}

impl ObservedMcpServers<'_> {
    /// 别名命中的提示性诊断；canonical 容器返回 `None`。
    pub const fn diagnostic_code(&self) -> Option<&'static str> {
        if self.alias_used {
            Some(PI_MCP_CONTAINER_ALIAS_DETECTED)
        } else {
            None
        }
    }
}

/// 只读识别 Pi MCP 容器（canonical 优先，其次别名）。
pub fn read_mcp_servers(root: &Value) -> Option<ObservedMcpServers<'_>> {
    if let Some(servers) = root.get("mcpServers").and_then(Value::as_object) {
        return Some(ObservedMcpServers {
            servers,
            alias_used: false,
        });
    }
    root.get("mcp-servers")
        .and_then(Value::as_object)
        .map(|servers| ObservedMcpServers {
            servers,
            alias_used: true,
        })
}

/// 同名遮蔽来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiMcpShadowSource {
    /// 被跨工具共享的 `<root>/.mcp.json` 覆盖。
    ProjectShared,
    /// 被用户手写（非本应用受管）的 `<root>/.pi/mcp.json` 覆盖。
    ProjectPi,
}

impl PiMcpShadowSource {
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::ProjectShared => PI_MCP_SHADOWED_BY_PROJECT_SHARED,
            Self::ProjectPi => PI_MCP_SHADOWED_BY_PROJECT_PI,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiMcpShadowing {
    pub source: PiMcpShadowSource,
    pub server: String,
}

/// 全局受管条目同名遮蔽检测（Apply 前硬阻断），只读，绝不返回条目内容。
///
/// 只对 `Scope::Global` 目标调用：项目目标写入的 `<root>/.pi/mcp.json` 是
/// 适配器合并链的最高优先级，不会被 `.mcp.json` 覆盖，因此项目目标永远返回
/// `None`。调用方传入的 `managed_names` 必须是**本应用在全局作用域受管**的
/// 条目名；项目作用域自行受管的同名条目不算遮蔽。
pub fn detect_mcp_shadowing(
    project_root: &Path,
    managed_names: &[String],
    scope: Scope,
) -> Option<PiMcpShadowing> {
    if scope != Scope::Global {
        return None;
    }
    let conflict = |source, name: &String| {
        Some(PiMcpShadowing {
            source,
            server: name.clone(),
        })
    };
    let shared = read_mcp_servers_for_shadowing(&project_root.join(".mcp.json"));
    let project_pi = read_mcp_servers_for_shadowing(&project_root.join(".pi/mcp.json"));
    let mut names = managed_names.to_vec();
    names.sort();
    for name in &names {
        if shared
            .as_ref()
            .is_some_and(|servers| servers.contains(name))
        {
            return conflict(PiMcpShadowSource::ProjectShared, name);
        }
    }
    for name in &names {
        if project_pi
            .as_ref()
            .is_some_and(|servers| servers.contains(name))
        {
            return conflict(PiMcpShadowSource::ProjectPi, name);
        }
    }
    None
}

/// 只返回同名集合，值永不离开本函数（避免把凭据带进调用方）。
fn read_mcp_servers_for_shadowing(path: &Path) -> Option<BTreeSet<String>> {
    let value = probe::read_json_file(path)?;
    read_mcp_servers(&value).map(|observed| observed.servers.keys().cloned().collect())
}

enum DiscoveryFile {
    Missing,
    File(Vec<u8>),
    Unavailable,
}

/// 自检受管子链接：断链返回 [`PI_SKILL_SYMLINK_BROKEN`]，解析结果逃逸
/// `allowed_root` 返回 [`PI_SKILL_SYMLINK_ESCAPE`]；安全或不存在同名条目返回 `None`。
///
/// Pi 对断链静默忽略（无诊断），因此写入前后必须自行检查。只读取链接文本，
/// 不跟随、不修改任何一个链接。
pub fn managed_children_symlink_diagnostic(
    directory: &Path,
    managed_names: &[String],
    allowed_root: &Path,
) -> Option<&'static str> {
    let Ok(root) = std::fs::canonicalize(allowed_root) else {
        return Some(PI_SKILL_SYMLINK_ESCAPE);
    };
    let mut names = managed_names.to_vec();
    names.sort();
    for name in &names {
        let child = directory.join(name);
        let Ok(metadata) = std::fs::symlink_metadata(&child) else {
            continue;
        };
        if !metadata.file_type().is_symlink() {
            continue;
        }
        let Ok(resolved) = std::fs::canonicalize(&child) else {
            return Some(PI_SKILL_SYMLINK_BROKEN);
        };
        if !resolved.starts_with(&root) {
            return Some(PI_SKILL_SYMLINK_ESCAPE);
        }
    }
    None
}

/// 只读、无跟随的文件读取：拒绝符号链接与非普通文件。
fn read_discovery_file(path: &Path) -> DiscoveryFile {
    let Some(parent) = path.parent() else {
        return DiscoveryFile::Unavailable;
    };
    let canonical_parent = match std::fs::canonicalize(parent) {
        Ok(parent) => parent,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return DiscoveryFile::Missing
        }
        Err(_) => return DiscoveryFile::Unavailable,
    };
    if canonical_parent != parent {
        return DiscoveryFile::Unavailable;
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return DiscoveryFile::Missing
        }
        Err(_) => return DiscoveryFile::Unavailable,
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return DiscoveryFile::Unavailable;
    }
    std::fs::read(path)
        .map(DiscoveryFile::File)
        .unwrap_or(DiscoveryFile::Unavailable)
}

#[cfg(test)]
mod tests;
