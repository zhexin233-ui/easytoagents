import { describe, expect, it } from "vitest";

import {
  MCP_TOOLS,
  PROFILE_TOOLS,
  SKILL_TOOLS,
  TOOL_METADATA,
  toolMetadata,
} from "@/lib/tool-metadata";

describe("tool metadata", () => {
  it("集中声明完整工具集合与 Cursor 能力边界", () => {
    expect(Object.keys(TOOL_METADATA)).toEqual(["claude", "codex", "cursor"]);
    expect(PROFILE_TOOLS).toEqual(["claude", "codex"]);
    expect(MCP_TOOLS).toEqual(["claude", "codex", "cursor"]);
    expect(SKILL_TOOLS).toEqual(["claude", "codex", "cursor"]);
    expect(toolMetadata("cursor")).toMatchObject({
      label: "Cursor",
      profileRoute: null,
      capabilities: {
        provider: false,
        promptGlobal: false,
        promptProject: false,
        mcp: true,
        skills: true,
      },
    });
    expect(toolMetadata("cursor").icon).toMatch(/cursor-icon\.svg|svg\+xml/);
  });
});
