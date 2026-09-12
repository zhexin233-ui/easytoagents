import { TOOL_CAPABILITIES, type Tool } from "@/bindings/commands";
import claudeIconUrl from "@/assets/brand/claude-icon-square.svg";
import codexIconUrl from "@/assets/brand/codex-icon-light.png";
import cursorIconUrl from "@/assets/brand/cursor-icon.svg";
import zcodeIconUrl from "@/assets/brand/zcode-icon.svg";
import opencodeIconUrl from "@/assets/brand/opencode-icon.svg";

export interface ToolMetadata {
  id: Tool;
  label: string;
  icon: string;
  profileRoute: string | null;
  capabilities: {
    provider: boolean;
    promptGlobal: boolean;
    mcp: boolean;
    skills: boolean;
    hooks: boolean;
    agents: boolean;
    projectAgents: boolean;
  };
}

type ToolCapability = ToolMetadata["capabilities"];

function capabilitiesFor(tool: Tool): ToolCapability {
  const capability = TOOL_CAPABILITIES.find((item) => item.tool === tool);
  if (!capability) {
    throw new Error(`后端未导出工具能力：${tool}`);
  }
  return {
    provider: capability.provider,
    promptGlobal: capability.promptGlobal,
    mcp: capability.mcp,
    skills: capability.skills,
    hooks: capability.hooks,
    agents: capability.agents,
    projectAgents: capability.projectAgents,
  };
}

export const TOOL_METADATA = {
  claude: {
    id: "claude",
    label: "Claude",
    icon: claudeIconUrl,
    profileRoute: "/claude",
    capabilities: capabilitiesFor("claude"),
  },
  codex: {
    id: "codex",
    label: "Codex",
    icon: codexIconUrl,
    profileRoute: "/codex",
    capabilities: capabilitiesFor("codex"),
  },
  cursor: {
    id: "cursor",
    label: "Cursor",
    icon: cursorIconUrl,
    profileRoute: "/cursor",
    capabilities: capabilitiesFor("cursor"),
  },
  zcode: {
    id: "zcode",
    label: "ZCode",
    icon: zcodeIconUrl,
    profileRoute: "/zcode",
    capabilities: capabilitiesFor("zcode"),
  },
  opencode: {
    id: "opencode",
    label: "OpenCode",
    icon: opencodeIconUrl,
    profileRoute: "/opencode",
    capabilities: capabilitiesFor("opencode"),
  },
} as const satisfies Record<Tool, ToolMetadata>;

const ALL_TOOLS: Tool[] = TOOL_CAPABILITIES.map(({ tool }) => tool);

export const PROFILE_TOOLS = ALL_TOOLS.filter((tool) => {
  const capabilities = capabilitiesFor(tool);
  return capabilities.provider || capabilities.promptGlobal;
});
export const MCP_TOOLS = ALL_TOOLS.filter((tool) => capabilitiesFor(tool).mcp);
export const SKILL_TOOLS = ALL_TOOLS.filter(
  (tool) => capabilitiesFor(tool).skills,
);
export const HOOK_TOOLS = ALL_TOOLS.filter(
  (tool) => capabilitiesFor(tool).hooks,
);
export const AGENT_TOOLS = ALL_TOOLS.filter(
  (tool) => capabilitiesFor(tool).agents,
);
export const PROJECT_AGENT_TOOLS = ALL_TOOLS.filter(
  (tool) => capabilitiesFor(tool).projectAgents,
);

export const DEFAULT_ENABLED_TOOLS = [
  "claude",
  "codex",
] as const satisfies readonly Tool[];

export function filterEnabledTools<T extends Tool>(
  tools: readonly T[],
  enabled: ReadonlySet<Tool>,
): T[] {
  return tools.filter((tool) => enabled.has(tool));
}

export function toolMetadata<T extends Tool>(
  tool: T,
): (typeof TOOL_METADATA)[T] {
  return TOOL_METADATA[tool];
}
