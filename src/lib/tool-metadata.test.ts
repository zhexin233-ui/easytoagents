import { describe, expect, it } from "vitest";

import opencodeIconSource from "@/assets/brand/opencode-icon.svg?raw";
import { TOOL_CAPABILITIES } from "@/bindings/commands";
import {
  HOOK_TOOLS,
  MCP_TOOLS,
  PROFILE_TOOLS,
  SKILL_TOOLS,
  TOOL_METADATA,
  toolMetadata,
} from "@/lib/tool-metadata";

describe("tool metadata", () => {
  it("前端能力集合与后端导出的常量保持一致", () => {
    const capabilities = TOOL_CAPABILITIES.map(({ tool }) => tool);
    expect(Object.keys(TOOL_METADATA)).toEqual(capabilities);

    for (const capability of TOOL_CAPABILITIES) {
      expect(toolMetadata(capability.tool).capabilities).toEqual({
        provider: capability.provider,
        promptGlobal: capability.promptGlobal,
        mcp: capability.mcp,
        skills: capability.skills,
        hooks: capability.hooks,
      });
    }

    expect(PROFILE_TOOLS).toEqual(
      TOOL_CAPABILITIES.filter(
        ({ provider, promptGlobal }) => provider || promptGlobal,
      ).map(({ tool }) => tool),
    );
    expect(MCP_TOOLS).toEqual(
      TOOL_CAPABILITIES.filter(({ mcp }) => mcp).map(({ tool }) => tool),
    );
    expect(SKILL_TOOLS).toEqual(
      TOOL_CAPABILITIES.filter(({ skills }) => skills).map(({ tool }) => tool),
    );
    expect(HOOK_TOOLS).toEqual(
      TOOL_CAPABILITIES.filter(({ hooks }) => hooks).map(({ tool }) => tool),
    );
  });

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

    expect(opencodeIconSource).toContain(
      '<svg width="300" height="300" viewBox="0 0 300 300"',
    );
    expect(opencodeIconSource).toContain('fill="#CFCECD"');
    expect(opencodeIconSource).toContain('fill="#211E1E"');
    expect(opencodeIconSource).not.toContain('fill="#111827"');
  });
});
