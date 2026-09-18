import { describe, expect, it } from "vitest";

import { globalTargetStatusPresentation } from "./global-target-status-ui";

describe("首次 Skills 接管状态", () => {
  it("引导显式接管同名安装，保留检测入口", () => {
    const result = globalTargetStatusPresentation(
      "external_owned_change",
      "SKILL_TARGET_INITIAL_TAKEOVER_REQUIRED",
    );
    expect(result.label).toBe("已有同名安装，待接管");
    expect(result.description).toContain("检测并接管");
    expect(result.previewBlocked).toBe(false);
  });

  it("不把真实基线冲突解释成首次接管", () => {
    const result = globalTargetStatusPresentation(
      "external_owned_change",
      "EXTERNAL_OWNED_CHANGE",
    );
    expect(result.label).toBeUndefined();
    expect(result.description).toBeNull();
  });
});

describe("Agents 目标诊断状态", () => {
  it("将 Agents 诊断码映射为中文状态并在不支持时阻止预览", () => {
    expect(
      globalTargetStatusPresentation(
        "external_non_owned_change",
        "AGENT_NAME_INVALID",
      ),
    ).toMatchObject({
      label: "Agent 名称无效",
      description: "名称只能使用小写字母、数字和连字符，长度为 1–64 个字符。",
      tone: "warning",
      previewBlocked: false,
    });
    expect(
      globalTargetStatusPresentation("failed", "AGENTS_UNSUPPORTED"),
    ).toMatchObject({
      label: "不支持 Agents",
      tone: "blocked",
      previewBlocked: true,
    });
  });
});

describe("Pi MCP 适配器诊断状态", () => {
  it("适配器缺失时给出安装指引并阻止预览", () => {
    const result = globalTargetStatusPresentation(
      "failed",
      "PI_MCP_ADAPTER_MISSING",
    );
    expect(result.label).toBe("未安装 Pi MCP 适配器");
    expect(result.description).toContain("pi install npm:pi-mcp-adapter");
    expect(result.tone).toBe("blocked");
    expect(result.previewBlocked).toBe(true);
  });

  it("适配器未加载与版本过低各自有独立文案", () => {
    expect(
      globalTargetStatusPresentation("failed", "PI_MCP_ADAPTER_NOT_LOADED")
        .label,
    ).toBe("Pi MCP 适配器未加载");
    expect(
      globalTargetStatusPresentation(
        "failed",
        "PI_MCP_ADAPTER_VERSION_UNSUPPORTED",
      ).description,
    ).toContain("2.33.0");
  });
});
