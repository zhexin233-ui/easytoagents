import { describe, expect, it } from "vitest";

import type { AppError } from "@/bindings/commands";
import { ProfileRpcError, profileErrorText } from "@/lib/profile-api";

function rpcError(overrides: Partial<AppError>): ProfileRpcError {
  return new ProfileRpcError({
    code: "CONFLICT",
    message: "检测到配置冲突",
    recoverable: true,
    action: "review_conflict",
    ...overrides,
  });
}

describe("profileErrorText", () => {
  it("优先展示 details.reason 的具体原因", () => {
    expect(
      profileErrorText(
        rpcError({
          details: {
            field: "assignment",
            reason: "该资源仍有项目分配，不能直接创建重复的全局分配",
          },
        }),
      ),
    ).toBe("CONFLICT：该资源仍有项目分配，不能直接创建重复的全局分配");
  });

  it("缺少 details.reason 时回退到通用 message", () => {
    expect(profileErrorText(rpcError({}))).toBe("CONFLICT：检测到配置冲突");
  });

  it("details.reason 不是字符串时回退到通用 message", () => {
    expect(
      profileErrorText(
        rpcError({ details: { reason: { code: "[REDACTED]" } } }),
      ),
    ).toBe("CONFLICT：检测到配置冲突");
  });

  it("NOT_FOUND 的专用文案优先于 details.reason", () => {
    expect(
      profileErrorText(
        rpcError({
          code: "NOT_FOUND",
          message: "未找到目标资源",
          details: { resource: "activeProviderProfile" },
        }),
      ),
    ).toBe(
      "尚无生效渠道档案，也没有可清理的受管基线；请先检测已有配置或创建并激活渠道。",
    );
  });

  it("普通 Error 直接展示 message，未知值返回兜底文案", () => {
    expect(profileErrorText(new Error("网络中断"))).toBe("网络中断");
    expect(profileErrorText({})).toBe("操作失败，请重新扫描后再试。");
    expect(profileErrorText(null)).toBeNull();
  });
});
