import type { Tool } from "@/bindings/commands";
import claudeIconUrl from "@/assets/brand/claude-icon-square.svg";
import codexIconUrl from "@/assets/brand/codex-icon-light.png";
import cursorIconUrl from "@/assets/brand/cursor-icon.svg";
import zcodeIconUrl from "@/assets/brand/zcode-icon.svg";

export interface ToolMetadata {
  id: Tool;
  label: string;
  icon: string;
  profileRoute: string | null;
  capabilities: {
    provider: boolean;
    promptGlobal: boolean;
    promptProject: boolean;
    mcp: boolean;
    skills: boolean;
  };
}

export const TOOL_METADATA = {
  claude: {
    id: "claude",
    label: "Claude",
    icon: claudeIconUrl,
    profileRoute: "/claude",
    capabilities: {
      provider: true,
      promptGlobal: true,
      promptProject: true,
      mcp: true,
      skills: true,
    },
  },
  codex: {
    id: "codex",
    label: "Codex",
    icon: codexIconUrl,
    profileRoute: "/codex",
    capabilities: {
      provider: true,
      promptGlobal: true,
      promptProject: true,
      mcp: true,
      skills: true,
    },
  },
  cursor: {
    id: "cursor",
    label: "Cursor",
    icon: cursorIconUrl,
    profileRoute: null,
    capabilities: {
      provider: false,
      promptGlobal: false,
      promptProject: false,
      mcp: true,
      skills: true,
    },
  },
  zcode: {
    id: "zcode",
    label: "ZCode",
    icon: zcodeIconUrl,
    profileRoute: "/zcode",
    capabilities: {
      provider: true,
      promptGlobal: true,
      promptProject: true,
      mcp: true,
      skills: true,
    },
  },
} as const satisfies Record<Tool, ToolMetadata>;

export const PROFILE_TOOLS = [
  "claude",
  "codex",
  "zcode",
] as const satisfies readonly Tool[];
export const MCP_TOOLS = [
  "claude",
  "codex",
  "cursor",
  "zcode",
] as const satisfies readonly Tool[];
export const SKILL_TOOLS = [
  "claude",
  "codex",
  "cursor",
  "zcode",
] as const satisfies readonly Tool[];

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
