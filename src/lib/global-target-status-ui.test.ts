import { describe, expect, it } from "vitest";

import { globalTargetStatusPresentation } from "./global-target-status-ui";

describe("首次 Skills 接管状态", () => {
  it("引导显式接管同名安装，保留检测入口", () => {
    const result = globalTargetStatusPresentation(
      "external_owned_change",
      "SKILL_TARGET_INITIAL_TAKEOVER_REQUIRED",
      { directApply: true },
    );
    expect(result.label).toBe("已有同名安装，待接管");
    expect(result.description).toContain("检测并接管已有 Skills");
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
