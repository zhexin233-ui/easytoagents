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
    expect(Object.keys(TOOL_METADATA)).toEqual([
      "claude",
      "codex",
      "cursor",
      "zcode",
      "opencode",
    ]);
    expect(PROFILE_TOOLS).toEqual([
      "claude",
      "codex",
      "cursor",
      "zcode",
      "opencode",
    ]);
    expect(MCP_TOOLS).toEqual([
      "claude",
      "codex",
      "cursor",
      "zcode",
      "opencode",
    ]);
    expect(SKILL_TOOLS).toEqual([
      "claude",
      "codex",
      "cursor",
      "zcode",
      "opencode",
    ]);
    expect(toolMetadata("cursor")).toMatchObject({
      label: "Cursor",
      profileRoute: "/cursor",
      capabilities: {
        provider: false,
        promptGlobal: true,
        mcp: true,
        skills: true,
      },
    });
    expect(toolMetadata("cursor").icon).toMatch(/cursor-icon\.svg|svg\+xml/);
    expect(toolMetadata("zcode")).toMatchObject({
      label: "ZCode",
      profileRoute: "/zcode",
      capabilities: {
        provider: true,
        promptGlobal: true,
        mcp: true,
        skills: true,
      },
    });
    expect(toolMetadata("zcode").icon).toMatch(/zcode-icon\.svg|svg\+xml/);
    expect(toolMetadata("opencode")).toMatchObject({
      label: "OpenCode",
      profileRoute: "/opencode",
      capabilities: {
        provider: true,
        promptGlobal: true,
        mcp: true,
        skills: true,
        hooks: false,
      },
    });
    expect(toolMetadata("opencode").icon).toMatch(
      /opencode-icon\.svg|svg\+xml/,
    );
  });
});
