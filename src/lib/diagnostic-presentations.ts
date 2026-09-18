import type {
  AppError,
  ArtifactKind,
  ErrorCode,
  SyncStatus,
  Tool,
} from "@/bindings/commands";
import type { SyncStatusBadgeTone } from "@/components/sync-status-badge";

/**
 * 用户界面使用的稳定诊断投影。
 *
 * 诊断码属于后端稳定协议，不能直接作为主文案渲染。这个模块只拥有
 * 静态、安全的解释；页面仍然决定是否展示按钮、是否发起重新检测或
 * 是否阻止预览，presentation 本身不执行任何副作用。
 */
export interface DiagnosticPresentation {
  label: string;
  description: string;
  nextStep: string;
  tone: SyncStatusBadgeTone;
  previewBlocked: boolean;
}

export interface TargetDiagnosticContext {
  tool?: Tool;
  artifactKind?: ArtifactKind;
}

type PresentationCopy = Pick<
  DiagnosticPresentation,
  "label" | "description" | "nextStep" | "tone"
>;

const TOOL_LABELS: Record<Tool, string> = {
  claude: "Claude",
  codex: "Codex",
  cursor: "Cursor",
  zcode: "ZCode",
  opencode: "OpenCode",
  pi: "Pi",
};

const TARGET_PREVIEW_BLOCKING_STATUSES = new Set<SyncStatus>([
  "failed",
  "policy_blocked",
  "untrusted",
]);

const AGENT_DIAGNOSTICS: Record<string, PresentationCopy> = {
  AGENTS_UNSUPPORTED: {
    label: "不支持 Agents",
    description: "当前工具不支持 Agents 目标，无法生成同步预览。",
    nextStep: "请改用支持 Agents 的工具，或在工具设置中关闭该资源。",
    tone: "blocked",
  },
  AGENT_FRONTMATTER_INVALID: {
    label: "Agent 格式错误",
    description: "Agent 文件的 frontmatter 或 TOML 格式无效。",
    nextStep: "请修复文件格式后重新检测。",
    tone: "blocked",
  },
  AGENT_REQUIRED_FIELD_MISSING: {
    label: "Agent 缺少必填字段",
    description: "Agent 必须包含描述和正文。",
    nextStep: "请补齐必填字段后重新检测。",
    tone: "warning",
  },
  AGENT_NAME_INVALID: {
    label: "Agent 名称无效",
    description: "名称只能使用小写字母、数字和连字符，长度为 1–64 个字符。",
    nextStep: "请修改名称后重新检测。",
    tone: "warning",
  },
  AGENT_FIELD_INVALID: {
    label: "Agent 字段无效",
    description: "Agent 字段超出长度限制或包含无效内容。",
    nextStep: "请修复字段内容后重新检测。",
    tone: "warning",
  },
  AGENT_NAME_CONFLICT: {
    label: "Agent 名称冲突",
    description: "中央库中已有同名 Agent。",
    nextStep: "请处理同名记录或改用其他名称后重试。",
    tone: "blocked",
  },
  AGENT_FILE_TOO_LARGE: {
    label: "Agent 文件过大",
    description: "Agent 文件超过可导入大小限制。",
    nextStep: "请缩小文件后重新检测或导入。",
    tone: "warning",
  },
  ZCODE_PROJECT_AGENTS_UNSUPPORTED: {
    label: "ZCode 不支持项目 Agents",
    description: "ZCode 官方暂不支持项目级 Agents。",
    nextStep: "请在全局 Agents 页面管理该 Agent。",
    tone: "blocked",
  },
  AGENT_TOOL_SETTINGS_UNSUPPORTED: {
    label: "工具设置不受支持",
    description: "当前工具不支持该 Agent 的工具特有设置。",
    nextStep: "请移除不受支持的设置后重新检测。",
    tone: "warning",
  },
};

const PI_DIAGNOSTICS: Record<string, PresentationCopy> = {
  PI_MCP_ADAPTER_MISSING: {
    label: "未安装 Pi MCP 适配器",
    description:
      "Pi 自身不含内置 MCP，当前适配器尚未安装；请先执行 pi install npm:pi-mcp-adapter。",
    nextStep:
      "请先执行 pi install npm:pi-mcp-adapter，再用 pi config 确认扩展已启用。",
    tone: "blocked",
  },
  PI_MCP_ADAPTER_NOT_LOADED: {
    label: "Pi MCP 适配器未加载",
    description: "已声明 pi-mcp-adapter，但当前没有加载。",
    nextStep: "请用 pi config 启用扩展，并确认项目已受信任后重新检测。",
    tone: "blocked",
  },
  PI_MCP_ADAPTER_VERSION_UNSUPPORTED: {
    label: "Pi MCP 适配器版本过低",
    description: "当前 Pi MCP 适配器版本不满足受支持的核验基线（2.33.0）。",
    nextStep: "请升级适配器后重新检测：pi install npm:pi-mcp-adapter。",
    tone: "blocked",
  },
  PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED: {
    label: "Pi MCP 处于 exclusive 模式",
    description: "exclusive 模式下适配器会忽略项目 MCP 配置。",
    nextStep: "请改用全局 MCP 或以非 exclusive 模式运行 Pi。",
    tone: "blocked",
  },
  PI_AGENT_DIR_OVERRIDE_UNMAPPED: {
    label: "Pi 目录覆盖无法定位",
    description: "指定的 Pi 目录覆盖无法安全映射到本机目录。",
    nextStep: "请检查 Pi 目录设置后重新检测。",
    tone: "blocked",
  },
  PI_AGENTS_UNSUPPORTED: {
    label: "Pi 不支持 Agents",
    description: "当前 Pi 环境不支持 Agents 目标。",
    nextStep: "请在支持 Agents 的工具中管理该资源。",
    tone: "blocked",
  },
  PI_HOOKS_UNSUPPORTED: {
    label: "Pi 不支持 Hooks",
    description: "当前 Pi 环境不支持 Hooks 目标。",
    nextStep: "请在支持 Hooks 的工具中管理该资源。",
    tone: "blocked",
  },
  PI_PROJECT_PROMPT_UNSUPPORTED: {
    label: "Pi 不支持项目提示词",
    description: "Pi 当前不支持项目级提示词目标。",
    nextStep: "请改用全局提示词或在其他工具中配置项目提示词。",
    tone: "blocked",
  },
  PI_PROJECT_SKILLS_UNTRUSTED: {
    label: "Pi 项目未受信任",
    description: "未信任的 Pi 项目不能读取或同步项目 Skills。",
    nextStep: "请先在 Pi 中信任项目，再重新检测。",
    tone: "blocked",
  },
  PI_PROJECT_SKILLS_TRUST_UNKNOWN: {
    label: "Pi 项目信任状态待确认",
    description: "暂时无法确认 Pi 项目的信任状态。",
    nextStep: "请确认项目已受信任后重新检测。",
    tone: "warning",
  },
  PI_PROVIDER_ENTRY_INVALID: {
    label: "Pi 渠道条目无效",
    description: "原生条目不是 JSON 对象，无法识别为 Pi 渠道。",
    nextStep: "请修复或移除该条目后重新检测。",
    tone: "blocked",
  },
  PI_PROVIDER_ID_INVALID: {
    label: "Pi 渠道标识无效",
    description: "原生 provider id 非法，不能作为安全的写入键。",
    nextStep: "请修复渠道标识后重新检测。",
    tone: "blocked",
  },
  PI_PROVIDER_FIELDS_INVALID: {
    label: "Pi 渠道字段无效",
    description: "原生 Pi 渠道缺少接入地址或 API Key 等必需字段。",
    nextStep: "请修复渠道字段后重新检测。",
    tone: "blocked",
  },
  PI_PROVIDER_INLINE_API_KEY: {
    label: "Pi 渠道包含内嵌密钥",
    description: "原生 Pi 渠道使用了不受支持的内嵌密钥形式。",
    nextStep: "请改用受支持的密钥配置后重新检测。",
    tone: "warning",
  },
  PI_MCP_ADAPTER_WRITES_MANAGED_ENTRY: {
    label: "Pi MCP 适配器会覆盖受管条目",
    description: "当前 Pi MCP 适配器可能直接写入应用管理的条目。",
    nextStep: "请升级或调整适配器配置后重新检测。",
    tone: "blocked",
  },
  PI_MCP_CONTAINER_ALIAS_DETECTED: {
    label: "Pi MCP 容器别名待处理",
    description: "检测到 Pi MCP 容器别名，无法安全判断实际目标。",
    nextStep: "请检查 MCP 容器配置后重新检测。",
    tone: "warning",
  },
  PI_MCP_SHADOWED_BY_PROJECT_PI: {
    label: "Pi MCP 被项目配置遮蔽",
    description: "项目级 Pi 配置遮蔽了当前全局 MCP。",
    nextStep: "请检查项目 Pi 配置后重新检测。",
    tone: "warning",
  },
  PI_MCP_SHADOWED_BY_PROJECT_SHARED: {
    label: "Pi MCP 被共享项目配置遮蔽",
    description: "共享项目配置遮蔽了当前 Pi MCP。",
    nextStep: "请检查共享项目配置后重新检测。",
    tone: "warning",
  },
  PI_SKILL_SYMLINK_BROKEN: {
    label: "Pi Skill 链接已失效",
    description: "Pi Skill 符号链接指向的内容不存在。",
    nextStep: "请修复链接或重新导入 Skill 后重试。",
    tone: "blocked",
  },
  PI_SKILL_SYMLINK_ESCAPE: {
    label: "Pi Skill 链接越界",
    description: "Pi Skill 符号链接指向了受管理目录之外的位置。",
    nextStep: "请修复链接目标后重新检测。",
    tone: "blocked",
  },
};

const RESOURCE_DIAGNOSTICS: Record<string, PresentationCopy> = {
  CENTRAL_SKILL_CONTENT_CHANGED: {
    label: "中央 Skill 内容已变化",
    description: "中央文件与应用记录中的受管内容不一致。",
    nextStep: "请确认是否采纳当前中央文件为权威内容。",
    tone: "warning",
  },
  CENTRAL_SKILL_INVALID: {
    label: "中央 Skill 无效",
    description: "中央 Skill 内容无法通过格式检查。",
    nextStep: "请修复内容后重新检测。",
    tone: "blocked",
  },
  CENTRAL_SKILL_MISSING: {
    label: "中央 Skill 缺失",
    description: "记录指向的中央 Skill 文件不存在。",
    nextStep: "请重新导入 Skill 或移除失效记录。",
    tone: "blocked",
  },
  CENTRAL_SKILL_PATH_CHANGED: {
    label: "中央 Skill 路径已变化",
    description: "中央 Skill 的路径与记录不一致。",
    nextStep: "请重新扫描并确认新的中央文件位置。",
    tone: "warning",
  },
  CENTRAL_SKILL_TYPE_CHANGED: {
    label: "中央 Skill 类型已变化",
    description: "中央 Skill 的文件类型与受管记录不一致。",
    nextStep: "请恢复正确的文件类型后重新检测。",
    tone: "blocked",
  },
  SKILL_PARSE_ERROR: {
    label: "Skill 格式错误",
    description: "Skill 文件无法解析。",
    nextStep: "请修复 Skill 文件格式后重新检测。",
    tone: "blocked",
  },
  TARGET_PARSE_ERROR: {
    label: "目标格式错误",
    description: "工具目标文件无法解析。",
    nextStep: "请修复目标文件格式后重新检测。",
    tone: "blocked",
  },
  TARGET_READ_FAILED: {
    label: "无法读取目标",
    description: "工具目标文件暂时无法读取。",
    nextStep: "请检查文件状态与权限后重新检测。",
    tone: "blocked",
  },
  TARGET_PERMISSION_DENIED: {
    label: "目标权限不足",
    description: "应用没有读取或更新目标文件所需的权限。",
    nextStep: "请修复文件权限后重新检测。",
    tone: "blocked",
  },
  NATIVE_CONFIGURATION_PARSE_ERROR: {
    label: "原生配置格式错误",
    description: "工具原生配置无法解析。",
    nextStep: "请修复原生配置格式后重新检测。",
    tone: "blocked",
  },
  NATIVE_CONFIGURATION_PERMISSION_DENIED: {
    label: "原生配置权限不足",
    description: "应用没有访问工具原生配置所需的权限。",
    nextStep: "请修复配置文件权限后重新检测。",
    tone: "blocked",
  },
  NATIVE_CONFIGURATION_UNAVAILABLE: {
    label: "原生配置不可用",
    description: "工具原生配置当前不可用。",
    nextStep: "请确认工具已安装并重新检测。",
    tone: "warning",
  },
  NATIVE_TARGET_TYPE_CHANGED: {
    label: "目标类型已变化",
    description: "目标位置的文件类型与受管记录不一致。",
    nextStep: "请恢复目标类型后重新检测，或使用接管/导入操作。",
    tone: "blocked",
  },
  TARGET_TYPE_CHANGED: {
    label: "目标类型已变化",
    description: "目标位置的文件类型与受管记录不一致。",
    nextStep: "请恢复目标类型后重新检测。",
    tone: "blocked",
  },
  EXTERNAL_NON_OWNED_CHANGE: {
    label: "发现外部变更",
    description: "目标包含应用未拥有的外部内容。",
    nextStep: "请先匹配或导入内容，再决定是否纳入同步。",
    tone: "warning",
  },
  EXTERNAL_OWNED_CHANGE: {
    label: "受管内容发生变化",
    description: "目标中的受管内容与最近一次基线不一致。",
    nextStep: "请检查冲突后选择采纳外部内容或用中央配置覆盖。",
    tone: "warning",
  },
  INCOMPLETE_MANAGED_BASELINE: {
    label: "受管基线不完整",
    description: "当前目标缺少完成安全同步所需的基线证据。",
    nextStep: "请重新检测；必要时从恢复点恢复后再试。",
    tone: "blocked",
  },
  MANAGED_ITEM_BASELINE_MISMATCH: {
    label: "受管基线不匹配",
    description: "目标与保存的受管基线不一致。",
    nextStep: "请重新预览并处理冲突后再应用。",
    tone: "blocked",
  },
  PROJECT_TARGET_INITIAL_UNMANAGED: {
    label: "项目目标未纳管",
    description: "该目标由外部维护，本项目暂无需要写入的项目级配置。",
    nextStep: "请先导入或分配资源，再决定是否纳入项目同步。",
    tone: "warning",
  },
  PROJECT_NATIVE_RESOURCE_MISSING: {
    label: "项目原生资源缺失",
    description: "项目原生资源已被外部移除，当前没有可恢复的禁用快照。",
    nextStep: "请重新扫描项目或从中央配置重新生成资源。",
    tone: "warning",
  },
  PROJECT_NATIVE_RESOURCE_CONFLICT: {
    label: "项目原生资源冲突",
    description: "项目原生资源位置被重新占用或发生外部变化。",
    nextStep: "请处理占用后重新预览；需要时从恢复点恢复。",
    tone: "blocked",
  },
  PROJECT_NATIVE_RESOURCE_DISABLED: {
    label: "项目原生资源已禁用",
    description: "该项目原生资源已被应用临时禁用。",
    nextStep: "如需恢复，请使用恢复操作并重新验证。",
    tone: "warning",
  },
  PROJECT_ROOT_MISSING: {
    label: "项目目录不存在",
    description: "项目根目录当前不存在。",
    nextStep: "请修复项目路径后重新扫描。",
    tone: "blocked",
  },
  PROJECT_ROOT_PERMISSION_DENIED: {
    label: "项目目录权限不足",
    description: "应用没有访问项目根目录所需的权限。",
    nextStep: "请修复目录权限后重新扫描。",
    tone: "blocked",
  },
  PROJECT_ROOT_CHANGED: {
    label: "项目路径已变化",
    description: "项目根目录与已登记路径不一致。",
    nextStep: "请重新登记或修复项目路径后扫描。",
    tone: "blocked",
  },
  PROJECT_ROOT_INVALID: {
    label: "项目路径无效",
    description: "登记的项目路径无法作为有效目录使用。",
    nextStep: "请修复项目路径后重新扫描。",
    tone: "blocked",
  },
  UNMANAGED_NATIVE_CONFIGURATION: {
    label: "原生配置未纳管",
    description: "发现原生配置，但它不属于应用当前管理的内容。",
    nextStep: "请通过匹配或导入建立受管记录。",
    tone: "warning",
  },
};

const INSTALLATION_DIAGNOSTICS: Record<string, PresentationCopy> = {
  INSTALLATION_PROBE_SKIPPED_PATH_ENTRIES: {
    label: "安装位置未完全检测",
    description: "部分 PATH 条目不安全，安装探针已跳过这些位置。",
    nextStep: "请使用绝对路径配置工具，或修复 PATH 后重新检测。",
    tone: "warning",
  },
  INSTALLATION_PROBE_NO_SAFE_PATH_ENTRIES: {
    label: "没有可检测的安装路径",
    description: "PATH 为空或没有安全的绝对路径条目。",
    nextStep: "请修复 PATH 后重新检测工具。",
    tone: "warning",
  },
  INSTALLATION_PROBE_UNSAFE_CANDIDATE: {
    label: "安装候选无法确认",
    description: "探针找到的同名文件无法安全确认。",
    nextStep: "请检查工具安装位置和权限后重新检测。",
    tone: "warning",
  },
};

const IMPORT_DIAGNOSTICS: Record<string, PresentationCopy> = {
  SKILL_IMPORT_SOURCE_MISSING: {
    label: "导入来源不存在",
    description: "扫描到的 Skill 来源目录或文件不存在。",
    nextStep: "请确认工具安装和来源路径后重新检测。",
    tone: "warning",
  },
  SKILL_IMPORT_SOURCE_UNAVAILABLE: {
    label: "导入来源不可用",
    description: "暂时无法读取 Skill 导入来源。",
    nextStep: "请检查工具状态和文件权限后重新检测。",
    tone: "warning",
  },
  SKILL_IMPORT_TOOL_UNAVAILABLE: {
    label: "工具不可用",
    description: "当前工具无法提供可读取的 Skill 来源。",
    nextStep: "请先安装或启动工具，再重新检测。",
    tone: "blocked",
  },
  SKILL_IMPORT_POLICY_BLOCKED: {
    label: "导入被策略阻止",
    description: "当前工具策略不允许读取该 Skill 来源。",
    nextStep: "请调整工具策略或改用受支持的来源后重试。",
    tone: "blocked",
  },
  SKILL_IMPORT_BUILTIN_EXCLUDED: {
    label: "内置 Skill 不导入",
    description: "该 Skill 属于工具内置内容，不会复制到中央库。",
    nextStep: "请选择用户维护的 Skill，或继续使用工具内置版本。",
    tone: "muted",
  },
  SKILL_IMPORT_SCAN_LIMIT: {
    label: "扫描范围受限",
    description: "来源内容超过安全扫描范围，部分项目未处理。",
    nextStep: "请缩小来源范围后重新检测。",
    tone: "warning",
  },
  SKILL_IMPORT_BUDGET_EXCEEDED: {
    label: "导入内容过多",
    description: "来源内容超过单次安全导入上限。",
    nextStep: "请分批选择内容后重试。",
    tone: "warning",
  },
  MATCH_OR_IMPORT_REQUIRED: {
    label: "需要先匹配或导入",
    description: "当前目标包含尚未纳入中央库的内容。",
    nextStep: "请先匹配或导入内容，再继续同步。",
    tone: "warning",
  },
  NATIVE_ADOPTION_UNAVAILABLE: {
    label: "无法接管原生内容",
    description: "当前原生内容不满足安全接管条件。",
    nextStep: "请修复目标后重新检测，或先导入中央库。",
    tone: "blocked",
  },
};

const CAPABILITY_DIAGNOSTICS: Record<string, PresentationCopy> = {
  TOOL_NOT_INSTALLED: {
    label: "工具未安装",
    description: "未检测到目标工具，无法生成同步目标。",
    nextStep: "请先安装工具，然后重新检测。",
    tone: "blocked",
  },
  CURSOR_PROVIDER_UNSUPPORTED: {
    label: "Cursor 不支持渠道",
    description: "Cursor 当前不支持应用管理的渠道配置。",
    nextStep: "请在 Cursor 中管理渠道，或使用提示词配置。",
    tone: "blocked",
  },
  OPENCODE_HOOKS_UNSUPPORTED: {
    label: "OpenCode 不支持 Hooks",
    description: "OpenCode 当前不支持应用管理的 Hooks 目标。",
    nextStep: "请在支持 Hooks 的工具中管理该资源。",
    tone: "blocked",
  },
  HOOK_TARGET_INITIAL_EMPTY_HOOKS: {
    label: "尚无 Hook",
    description: "工具 Hook 目录为空，当前没有可接管的 Hook。",
    nextStep: "请先导入或创建 Hook，再进行分配。",
    tone: "warning",
  },
  NATIVE_ADOPTION_UNAVAILABLE: {
    label: "暂时无法采纳原生更改",
    description: "当前原生内容不满足安全采纳条件。",
    nextStep: "请先在应用内匹配或导入内容，再重试采纳。",
    tone: "blocked",
  },
  CENTRAL_OVERWRITE_UNAVAILABLE: {
    label: "暂时无法覆盖原生更改",
    description: "当前目标不满足安全覆盖条件。",
    nextStep: "请重新检测目标后再试，或先处理外部内容。",
    tone: "blocked",
  },
};

const TARGET_PRESENTATIONS: Record<string, PresentationCopy> = {
  ...AGENT_DIAGNOSTICS,
  ...PI_DIAGNOSTICS,
  ...RESOURCE_DIAGNOSTICS,
  ...INSTALLATION_DIAGNOSTICS,
  ...IMPORT_DIAGNOSTICS,
  ...CAPABILITY_DIAGNOSTICS,
  // 目标状态也可能直接携带 RPC 层的稳定码；在目标卡片中仍使用
  // 面向用户的目标语义，避免回退为机器码或笼统的未知状态。
  NOT_FOUND: {
    label: "目标不存在",
    description: "工具目标文件或目录不存在。",
    nextStep: "请确认工具安装和目标路径后重新检测。",
    tone: "warning",
  },
  PARSE_ERROR: {
    label: "格式错误",
    description: "目标文件无法解析。",
    nextStep: "请修复目标文件格式后重新检测。",
    tone: "blocked",
  },
  PERMISSION_DENIED: {
    label: "权限不足",
    description: "应用没有访问目标文件所需的权限。",
    nextStep: "请修复文件权限后重新检测。",
    tone: "blocked",
  },
  POLICY_BLOCKED: {
    label: "策略阻止",
    description: "当前工具策略阻止修改该目标。",
    nextStep: "请调整策略后重新检测。",
    tone: "blocked",
  },
  UNTRUSTED_PROJECT: {
    label: "项目未受信任",
    description: "未受信任项目不能读取或更新该目标。",
    nextStep: "请先信任项目后重新检测。",
    tone: "blocked",
  },
  CONFLICT: {
    label: "目标存在冲突",
    description: "目标与当前受管基线发生冲突。",
    nextStep: "请重新检测并处理冲突后再试。",
    tone: "blocked",
  },
  CLAUDE_POLICY_BLOCKED: {
    label: "策略阻止",
    description: "Claude 管理策略禁止修改该类自定义目标。",
    nextStep: "请调整宿主策略后重新检测；应用不会强行覆盖配置。",
    tone: "blocked",
  },
  CLAUDE_POLICY_UNKNOWN: {
    label: "策略状态待确认",
    description: "无法确认 Claude 管理策略，预览已阻止。",
    nextStep: "请确认宿主策略后重新检测。",
    tone: "warning",
  },
  CLAUDE_CAPABILITY_EVIDENCE_STALE: {
    label: "Claude 能力证据已过期",
    description: "上一次 Claude 能力检测结果已失效。",
    nextStep: "请重新检测工具环境后重试。",
    tone: "warning",
  },
  CLAUDE_INSTALLATION_VERSION_UNKNOWN: {
    label: "Claude 版本待确认",
    description: "无法确认当前 Claude 版本。",
    nextStep: "请确认 Claude 已安装并重新检测。",
    tone: "warning",
  },
  CLAUDE_USER_MCP_LOCATION_UNSUPPORTED: {
    label: "Claude MCP 位置不受支持",
    description: "当前 Claude MCP 位置不能作为安全同步目标。",
    nextStep: "请改用受支持的位置后重新检测。",
    tone: "blocked",
  },
  OPENCODE_CONFIG_CONTENT_OVERRIDE: {
    label: "OpenCode 配置被覆盖",
    description: "检测到更高优先级的 OpenCode 配置内容。",
    nextStep: "请检查配置覆盖来源后重新检测。",
    tone: "warning",
  },
  OPENCODE_DISCOVERY_DISABLED: {
    label: "OpenCode 配置发现已禁用",
    description: "OpenCode 当前不会发现该配置位置。",
    nextStep: "请启用配置发现或改用受支持的位置后重新检测。",
    tone: "blocked",
  },
  CODEX_PROJECT_UNTRUSTED: {
    label: "Codex 项目未受信任",
    description: "Codex 不会在未受信任项目中读取或写入项目资源。",
    nextStep: "请先在 Codex 中信任项目，再重新检测。",
    tone: "blocked",
  },
  CODEX_TRUST_UNKNOWN: {
    label: "Codex 信任状态待确认",
    description: "暂时无法确认 Codex 项目是否受信任。",
    nextStep: "请确认项目的信任状态后重新检测。",
    tone: "warning",
  },
  CODEX_PROMPT_OVERRIDE_DETECTED: {
    label: "Codex 指令被覆盖",
    description: "检测到更高优先级的 Codex 指令来源。",
    nextStep: "请检查 AGENTS.override.md 后再应用。",
    tone: "warning",
  },
  CODEX_PROMPT_OVERRIDE_UNKNOWN: {
    label: "Codex 指令覆盖状态待确认",
    description: "无法安全确认 Codex 指令是否被覆盖。",
    nextStep: "请检查 AGENTS.override.md 后重新检测。",
    tone: "warning",
  },
  PI_PROMPT_OVERRIDE_DETECTED: {
    label: "Pi 提示词被覆盖",
    description: "检测到更高优先级的 Pi 提示词来源。",
    nextStep: "请检查覆盖来源后再应用。",
    tone: "warning",
  },
  PI_PROMPT_FALLBACK_PRESENT: {
    label: "Pi 存在提示词回退",
    description: "Pi 当前会使用回退提示词来源。",
    nextStep: "请检查提示词优先级后重新检测。",
    tone: "warning",
  },
  SKILL_TARGET_INITIAL_TAKEOVER_REQUIRED: {
    label: "已有同名安装，待接管",
    description:
      "工具目录中已有同名技能，可检测并接管；选择接管后会由应用安全替换入口。",
    nextStep: "请点击“检测并接管已有 Skills”，确认后再继续。",
    tone: "warning",
  },
  SKILL_TARGET_INITIAL_SYNC_PENDING: {
    label: "已分配，待同步",
    description: "分配已写入，尚未写入工具目录；当前主操作会自动完成同步。",
    nextStep: "请重新检测目标，确认同步范围后再试。",
    tone: "warning",
  },
  SKILL_TARGET_INITIAL_EMPTY: {
    label: "空目录，待配置",
    description:
      "目标目录为空；可先导入技能到中央库，再分配；分配后会自动同步。",
    nextStep: "请先导入技能到中央库，再分配到该工具。",
    tone: "warning",
  },
  SKILL_TARGET_INITIAL_UNMANAGED: {
    label: "未纳入同步管理",
    description:
      "已有目录尚未纳管；可检测其中的用户技能并复制到中央库，不会自动接管。",
    nextStep: "请点击“检测并导入已有 Skills”，选择要复制的内容。",
    tone: "warning",
  },
};

const ERROR_PRESENTATIONS: Record<ErrorCode, PresentationCopy> = {
  NOT_FOUND: {
    label: "找不到资源",
    description: "请求的资源不存在或已被移除。",
    nextStep: "请重新扫描后再试。",
    tone: "warning",
  },
  INVALID_INPUT: {
    label: "输入内容无效",
    description: "提交的内容未通过校验。",
    nextStep: "请修正输入后重试。",
    tone: "warning",
  },
  PARSE_ERROR: {
    label: "格式错误",
    description: "目标内容无法解析。",
    nextStep: "请修复格式后重新检测。",
    tone: "blocked",
  },
  PERMISSION_DENIED: {
    label: "权限不足",
    description: "应用没有完成此操作所需的权限。",
    nextStep: "请修复文件或项目权限后重试。",
    tone: "blocked",
  },
  POLICY_BLOCKED: {
    label: "策略阻止",
    description: "当前工具或项目策略不允许此操作。",
    nextStep: "请调整策略后重新检测。",
    tone: "blocked",
  },
  UNTRUSTED_PROJECT: {
    label: "项目未受信任",
    description: "未受信任项目不能执行此操作。",
    nextStep: "请先信任项目后重试。",
    tone: "blocked",
  },
  CONFLICT: {
    label: "检测到配置冲突",
    description: "目标或分配记录与当前状态冲突。",
    nextStep: "请检查冲突并重新检测后再试。",
    tone: "blocked",
  },
  STALE_PREVIEW: {
    label: "预览已过期",
    description: "预览依据在应用前发生了变化。",
    nextStep: "请重新检测并生成预览后再应用。",
    tone: "warning",
  },
  PREVIEW_ALREADY_CONSUMED: {
    label: "预览已使用",
    description: "这份一次性预览已经被消费，不能重复应用。",
    nextStep: "请重新生成预览后重试。",
    tone: "warning",
  },
  WRITE_IN_PROGRESS: {
    label: "已有写入进行中",
    description: "当前已有同步或恢复操作正在进行。",
    nextStep: "请等待当前操作完成后重试。",
    tone: "warning",
  },
  ATOMIC_WRITE_FAILED: {
    label: "写入未完成",
    description: "原生配置未能安全写入。",
    nextStep: "请检查权限并从恢复点恢复后重试。",
    tone: "blocked",
  },
  ROLLBACK_FAILED: {
    label: "恢复未完成",
    description: "写入失败后的自动恢复未能完成。",
    nextStep: "请打开恢复入口处理恢复点，并避免重复写入。",
    tone: "blocked",
  },
  SECRET_REDACTED: {
    label: "内容已脱敏",
    description: "涉及敏感内容的字段已被安全隐藏。",
    nextStep: "请重新输入敏感字段后重试。",
    tone: "warning",
  },
  DATABASE_ERROR: {
    label: "应用数据暂不可用",
    description: "应用暂时无法读取或保存中央数据。",
    nextStep: "请稍后重试；若持续发生，请重新打开应用。",
    tone: "blocked",
  },
  MIGRATION_FAILED: {
    label: "应用升级未完成",
    description: "应用数据升级未能完成。",
    nextStep: "请重新打开应用；若持续发生，请保留日志以便排查。",
    tone: "blocked",
  },
  PERMISSION_AUDIT_FAILED: {
    label: "权限检查未完成",
    description: "应用无法确认目标权限状态。",
    nextStep: "请检查文件权限后重新扫描。",
    tone: "warning",
  },
  ENVIRONMENT_PROBING: {
    label: "正在检测工具环境",
    description: "工具环境仍在后台检测中。",
    nextStep: "请等待检测完成后重试。",
    tone: "warning",
  },
};

const ERROR_PRESENTATIONS_BY_CODE: Record<string, PresentationCopy> = {
  ...ERROR_PRESENTATIONS,
};

const PREVIEW_PRESENTATIONS: Record<string, PresentationCopy> = {
  ...TARGET_PRESENTATIONS,
  NO_EXTERNAL_CHANGE_TARGET: {
    label: "没有可处理的外部目标",
    description: "当前扫描没有发现可执行外部变更操作的目标。",
    nextStep: "请重新检测并确认目标仍然存在。",
    tone: "muted",
  },
  ONLY_NON_OWNED: {
    label: "仅包含非受管内容",
    description: "当前目标只包含应用未拥有的外部内容。",
    nextStep: "请先匹配或导入内容，再继续同步。",
    tone: "warning",
  },
  PROJECT_ASSIGNMENT_EXISTS: {
    label: "项目已有分配",
    description: "该资源已经分配到项目，不能重复创建。",
    nextStep: "请检查现有项目分配后再试。",
    tone: "warning",
  },
  GLOBAL_ASSIGNMENT_INHERITED: {
    label: "资源继承全局分配",
    description: "项目当前继承全局资源分配。",
    nextStep: "请在全局资源页调整分配，或显式创建项目分配。",
    tone: "warning",
  },
  GIT_TRACKED: {
    label: "文件已被 Git 跟踪",
    description: "目标文件已被 Git 跟踪。",
    nextStep: "如需排除，请选择不纳入 Git 的同步选项后重新预览。",
    tone: "warning",
  },
  GIT_IGNORED: {
    label: "文件已被 Git 忽略",
    description: "目标文件已被 Git 忽略。",
    nextStep: "请确认当前 Git 规则符合预期后继续。",
    tone: "warning",
  },
};

const GENERIC_TARGET_PRESENTATION: PresentationCopy = {
  label: "需要重新检测",
  description: "目标状态需要重新检测。",
  nextStep: "请刷新环境并重新检测；若仍失败，请检查工具是否已安装。",
  tone: "warning",
};

const GENERIC_RPC_PRESENTATION: PresentationCopy = {
  label: "操作失败",
  description: "操作未完成。",
  nextStep: "请重新扫描后再试。",
  tone: "blocked",
};

const GENERIC_WARNING_PRESENTATION: PresentationCopy = {
  label: "同步计划需要注意",
  description: "同步计划包含需要注意的事项。",
  nextStep: "请查看计划提示，确认后再继续。",
  tone: "warning",
};

const HUMAN_REASON_PRESENTATION: PresentationCopy = {
  label: "需要注意",
  description: "操作未完成。",
  nextStep: "请重新检测后再试。",
  tone: "warning",
};

const STATUS_PRESENTATIONS: Record<SyncStatus, PresentationCopy> = {
  in_sync: {
    label: "已同步",
    description: "目标与中央配置一致。",
    nextStep: "无需操作。",
    tone: "success",
  },
  external_non_owned_change: {
    label: "非受管变更",
    description: "目标包含应用未拥有的外部内容。",
    nextStep: "请先匹配或导入内容，再决定是否纳入同步。",
    tone: "warning",
  },
  external_owned_change: {
    label: "受管内容冲突",
    description: "目标中的受管内容与最近一次基线不一致。",
    nextStep: "请检查冲突后选择采纳外部内容或用中央配置覆盖。",
    tone: "warning",
  },
  missing: {
    label: "待初始化",
    description: "尚未写入受管目标。",
    nextStep: "分配资源后会自动初始化；也可以先导入已有内容。",
    tone: "warning",
  },
  parse_error: {
    label: "格式错误",
    description: "目标文件无法解析。",
    nextStep: "请修复文件格式后重新检测。",
    tone: "blocked",
  },
  permission_denied: {
    label: "权限不足",
    description: "应用没有访问目标文件所需的权限。",
    nextStep: "请修复文件权限后重新检测。",
    tone: "blocked",
  },
  policy_blocked: {
    label: "策略阻止",
    description: "当前工具策略阻止修改该目标。",
    nextStep: "请调整策略后重新检测。",
    tone: "blocked",
  },
  untrusted: {
    label: "项目未信任",
    description: "目标项目未受信任，当前不能预览。",
    nextStep: "请先信任项目后重新检测。",
    tone: "blocked",
  },
  target_type_changed: {
    label: "目标类型变化",
    description: "目标位置的文件类型与受管记录不一致。",
    nextStep: "请恢复目标类型后重新检测。",
    tone: "blocked",
  },
  failed: {
    label: "检测失败",
    description: "目标能力检测未完成。",
    nextStep: "请修复工具可用性后重新检测。",
    tone: "blocked",
  },
};

function installationProbePresentation(
  code: string,
  context: TargetDiagnosticContext,
): PresentationCopy | undefined {
  if (!code.endsWith("_INSTALLATION_PROBE_UNSUPPORTED")) return undefined;
  const tool = context.tool ?? toolFromInstallationCode(code);
  const toolLabel = tool ? TOOL_LABELS[tool] : "工具";
  return {
    label: `${toolLabel} 需要重新检测`,
    description: `当前未能确认 ${toolLabel} 的安装状态，检测快照可能已失效。`,
    nextStep: `请重启 easytoagents 后重试；如仍未恢复，请重新检测 ${toolLabel}。`,
    tone: "warning",
  };
}

function toolFromInstallationCode(code: string): Tool | undefined {
  if (code.startsWith("CLAUDE_")) return "claude";
  if (code.startsWith("CODEX_")) return "codex";
  if (code.startsWith("CURSOR_")) return "cursor";
  if (code.startsWith("ZCODE_")) return "zcode";
  if (code.startsWith("OPENCODE_")) return "opencode";
  if (code.startsWith("PI_")) return "pi";
  return undefined;
}

function copyForTarget(
  code: string | null,
  context: TargetDiagnosticContext,
): PresentationCopy {
  if (code) {
    const installation = installationProbePresentation(code, context);
    if (installation) return installation;
    const known = TARGET_PRESENTATIONS[code];
    if (known) return known;
  }
  return GENERIC_TARGET_PRESENTATION;
}

const MACHINE_CODE_PATTERN = /^[A-Z][A-Z0-9_]{2,}$/;

/** 识别边界层可能收到的未来稳定码，避免把它直接交给用户界面。 */
export function isMachineDiagnosticCode(value: string): boolean {
  return MACHINE_CODE_PATTERN.test(value.trim());
}

function knownPresentationForCode(
  code: string,
  context: TargetDiagnosticContext = {},
): PresentationCopy | undefined {
  const installation = installationProbePresentation(code, context);
  if (installation) return installation;
  return (
    TARGET_PRESENTATIONS[code] ??
    PREVIEW_PRESENTATIONS[code] ??
    ERROR_PRESENTATIONS_BY_CODE[code]
  );
}

/**
 * 将后端 details.reason 或外部变化计划原因安全地投影为用户文案。
 * 机器码只使用 registry 中的静态解释，未知机器码不会回显；后端静态中文
 * 原因则保留，因为它不包含稳定码或原生敏感值。
 */
export function presentDiagnosticReason(
  reason: string | null | undefined,
  context: TargetDiagnosticContext = {},
): DiagnosticPresentation {
  const value = typeof reason === "string" ? reason.trim() : "";
  if (!value) {
    return {
      ...GENERIC_TARGET_PRESENTATION,
      previewBlocked: true,
    };
  }
  if (isMachineDiagnosticCode(value)) {
    const known = knownPresentationForCode(value, context);
    if (known) return withPreviewBlocked(known, "failed");
    return withPreviewBlocked(GENERIC_TARGET_PRESENTATION, "failed");
  }
  return {
    ...HUMAN_REASON_PRESENTATION,
    description: value,
    previewBlocked: false,
  };
}

function withPreviewBlocked(
  copy: PresentationCopy,
  status: SyncStatus,
): DiagnosticPresentation {
  return {
    ...copy,
    previewBlocked: TARGET_PREVIEW_BLOCKING_STATUSES.has(status),
  };
}

/** 将目标状态和稳定诊断码投影为统一的中文说明。 */
export function presentTargetDiagnostic(
  status: SyncStatus,
  code: string | null,
  context: TargetDiagnosticContext = {},
): DiagnosticPresentation {
  if (!code) {
    return withPreviewBlocked(STATUS_PRESENTATIONS[status], status);
  }
  return withPreviewBlocked(copyForTarget(code, context), status);
}

/** 将 RPC AppError 投影为不暴露内部码的用户说明。 */
export function presentRpcError(error: AppError): DiagnosticPresentation {
  const knownError = ERROR_PRESENTATIONS[error.code];
  const copy = knownError ?? GENERIC_RPC_PRESENTATION;
  const messageText =
    typeof error.message === "string" ? error.message.trim() : "";
  const messagePresentation = isMachineDiagnosticCode(messageText)
    ? knownPresentationForCode(messageText)
    : null;
  const reason = error.details?.reason;
  const reasonText = typeof reason === "string" ? reason.trim() : "";
  const reasonPresentation = reasonText
    ? isMachineDiagnosticCode(reasonText)
      ? knownPresentationForCode(reasonText)
      : null
    : null;
  const safeReason =
    reasonPresentation?.description ??
    (reasonText && !isMachineDiagnosticCode(reasonText) ? reasonText : null);
  return {
    ...copy,
    description:
      safeReason ??
      messagePresentation?.description ??
      (messageText && !isMachineDiagnosticCode(messageText)
        ? messageText
        : null) ??
      copy.description,
    nextStep:
      reasonPresentation?.nextStep ??
      messagePresentation?.nextStep ??
      copy.nextStep,
    previewBlocked: copy.tone === "blocked",
  };
}

export type PreviewCodeKind = "warning" | "error" | "target";

/** 将 Preview、Onboarding 和同步历史中的 code 投影为安全中文提示。 */
export function presentPreviewCode(
  code: string | null,
  kind: PreviewCodeKind,
): DiagnosticPresentation {
  if (!code) {
    return {
      ...(kind === "warning"
        ? GENERIC_WARNING_PRESENTATION
        : kind === "target"
          ? GENERIC_TARGET_PRESENTATION
          : GENERIC_RPC_PRESENTATION),
      previewBlocked: kind === "error",
    };
  }
  const errorCopy =
    kind === "error" ? ERROR_PRESENTATIONS_BY_CODE[code] : undefined;
  const targetCopy = kind === "target" ? TARGET_PRESENTATIONS[code] : undefined;
  const copy = errorCopy ?? PREVIEW_PRESENTATIONS[code] ?? targetCopy;
  const fallback =
    copy ??
    (kind === "warning"
      ? GENERIC_WARNING_PRESENTATION
      : kind === "error"
        ? GENERIC_RPC_PRESENTATION
        : GENERIC_TARGET_PRESENTATION);
  return {
    ...fallback,
    previewBlocked: kind === "error" || fallback.tone === "blocked",
  };
}

/** 用于运行历史等需要同时显示状态和错误说明的轻量格式化。 */
export function presentSyncRunError(code: ErrorCode | null) {
  return code ? presentPreviewCode(code, "error") : null;
}

export function diagnosticText(presentation: DiagnosticPresentation): string {
  return `${presentation.description} ${presentation.nextStep}`;
}
