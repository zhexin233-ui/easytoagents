import { describe, expect, it } from "vitest";

import {
  ENVIRONMENT_PROBING_RETRY_LIMIT,
  ProfileRpcError,
  isEnvironmentProbingError,
  retryWhileEnvironmentProbing,
} from "@/lib/rpc";

const probing = new ProfileRpcError({
  code: "ENVIRONMENT_PROBING",
  message: "工具环境仍在检测中，请稍后重试",
  recoverable: true,
  action: null,
});

const conflict = new ProfileRpcError({
  code: "CONFLICT",
  message: "检测到配置冲突",
  recoverable: true,
  action: "review_conflict",
});

describe("环境探测中的重试策略", () => {
  it("只把 ENVIRONMENT_PROBING 识别为等待态", () => {
    expect(isEnvironmentProbingError(probing)).toBe(true);
    expect(isEnvironmentProbingError(conflict)).toBe(false);
    expect(isEnvironmentProbingError(new Error("network"))).toBe(false);
    expect(isEnvironmentProbingError(undefined)).toBe(false);
  });

  it("探测中错误在上限内重试，其它错误立即失败", () => {
    expect(retryWhileEnvironmentProbing(0, probing)).toBe(true);
    expect(
      retryWhileEnvironmentProbing(
        ENVIRONMENT_PROBING_RETRY_LIMIT - 1,
        probing,
      ),
    ).toBe(true);
    expect(
      retryWhileEnvironmentProbing(ENVIRONMENT_PROBING_RETRY_LIMIT, probing),
    ).toBe(false);
    expect(retryWhileEnvironmentProbing(0, conflict)).toBe(false);
    expect(retryWhileEnvironmentProbing(0, new Error("network"))).toBe(false);
  });
});
