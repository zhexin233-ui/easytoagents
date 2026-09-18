import { describe, expect, it } from "vitest";

import type { AppError } from "@/bindings/commands";
import {
  presentDiagnosticReason,
  presentPreviewCode,
  presentRpcError,
  presentTargetDiagnostic,
  presentSyncRunError,
} from "./diagnostic-presentations";

function appError(overrides: Partial<AppError> = {}): AppError {
  return {
    code: "CONFLICT",
    message: "检测到配置冲突",
    recoverable: true,
    action: "review_conflict",
    ...overrides,
  };
}

describe("统一诊断 presentation", () => {
  it("为 Codex 探针失效提供重启和重新检测动作", () => {
    const result = presentTargetDiagnostic(
      "failed",
      "CODEX_INSTALLATION_PROBE_UNSUPPORTED",
      { tool: "codex" },
    );

    expect(result.label).toBe("Codex 需要重新检测");
    expect(result.nextStep).toContain("重启 easytoagents 后重试");
    expect(result.nextStep).toContain("重新检测 Codex");
    expect(result.previewBlocked).toBe(true);
  });

  it("首次 Skills 诊断也由 registry 提供可执行提示", () => {
    const result = presentTargetDiagnostic(
      "external_non_owned_change",
      "SKILL_TARGET_INITIAL_UNMANAGED",
      { tool: "codex", artifactKind: "skill" },
    );

    expect(result.label).toBe("未纳入同步管理");
    expect(result.nextStep).toContain("检测并导入");
    expect(result.description).not.toContain("SKILL_TARGET_INITIAL_UNMANAGED");
  });

  it("已知 RPC 错误优先展示安全 reason，但不暴露内部码", () => {
    const result = presentRpcError(
      appError({
        details: {
          reason: "该资源仍有项目分配，不能直接创建重复的全局分配",
        },
      }),
    );

    expect(result.description).toContain("该资源仍有项目分配");
    expect(result.nextStep).toContain("检查冲突");
    expect(result.description).not.toContain("CONFLICT");
  });

  it("Preview warning 和未知码都有可理解的 fallback", () => {
    expect(presentPreviewCode("GIT_TRACKED", "warning").description).toContain(
      "Git 跟踪",
    );
    const unknown = presentPreviewCode("FUTURE_WARNING_CODE", "warning");
    expect(unknown.description).toContain("同步计划");
    expect(unknown.nextStep).toContain("确认");
    expect(unknown.description).not.toContain("FUTURE_WARNING_CODE");
  });

  it("未知目标码安全降级，不返回空文案", () => {
    const result = presentTargetDiagnostic(
      "failed",
      "FUTURE_TARGET_DIAGNOSTIC",
      { artifactKind: "skill" },
    );

    expect(result.label).toBe("需要重新检测");
    expect(result.description).toContain("重新检测");
    expect(result.nextStep).toContain("刷新环境");
    expect(result.previewBlocked).toBe(true);
    expect(result.description).not.toContain("FUTURE_TARGET_DIAGNOSTIC");
  });

  it("details.reason 为内部码时使用对应中文解释", () => {
    const result = presentRpcError(
      appError({
        code: "CONFLICT",
        message: "检测到配置冲突",
        details: { reason: "MATCH_OR_IMPORT_REQUIRED" },
      }),
    );

    expect(result.description).toContain("尚未纳入中央库");
    expect(result.nextStep).toContain("匹配或导入");
    expect(result.description).not.toContain("MATCH_OR_IMPORT_REQUIRED");
  });

  it("AppError.message 为内部码时也使用安全解释", () => {
    const result = presentRpcError(
      appError({
        code: "INVALID_INPUT",
        message: "MATCH_OR_IMPORT_REQUIRED",
      }),
    );

    expect(result.description).toContain("尚未纳入中央库");
    expect(result.nextStep).toContain("匹配或导入");
    expect(result.description).not.toContain("MATCH_OR_IMPORT_REQUIRED");
  });

  it("未知 RPC 码和未知运行记录码都使用安全 fallback", () => {
    const unknownRpcError = appError({ message: "FUTURE_RPC_CODE" });
    Object.defineProperty(unknownRpcError, "code", {
      value: "FUTURE_RPC_CODE",
    });
    const unknownRpc = presentRpcError(unknownRpcError);
    expect(unknownRpc.description).toBe("操作未完成。");
    expect(unknownRpc.description).not.toContain("FUTURE_RPC_CODE");

    const run = presentSyncRunError("DATABASE_ERROR");
    expect(run?.description).toContain("中央数据");
    expect(run?.description).not.toContain("DATABASE_ERROR");
  });

  it("外部动作原因码不会回显机器码", () => {
    const result = presentDiagnosticReason("NATIVE_ADOPTION_UNAVAILABLE", {
      tool: "codex",
      artifactKind: "mcp",
    });

    expect(result.description).toContain("安全采纳");
    expect(result.nextStep).toContain("匹配或导入");
    expect(result.description).not.toContain("NATIVE_ADOPTION_UNAVAILABLE");
  });
});
