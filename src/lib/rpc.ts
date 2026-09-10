import type { AppError, Result } from "@/bindings/commands";

/**
 * 生成的 Tauri 命令统一返回 `Result<T, AppError>`。这里是前端唯一的解包点：
 * 把结构化错误抛成 `ProfileRpcError`，让页面在 `catch`/`onError` 中拿到稳定错误码。
 */
export class ProfileRpcError extends Error {
  readonly appError: AppError;

  constructor(appError: AppError) {
    super(appError.message);
    this.name = "ProfileRpcError";
    this.appError = appError;
  }
}

export function unwrapResult<T>(result: Result<T, AppError>): T {
  if (result.status === "error") {
    throw new ProfileRpcError(result.error);
  }
  return result.data;
}

export function profileErrorText(error: unknown): string | null {
  if (error instanceof ProfileRpcError) {
    const resource = errorDetailString(error, "resource");
    if (error.appError.code === "NOT_FOUND") {
      if (resource === "activeProviderProfile") {
        return "尚无生效渠道档案，也没有可清理的受管基线；请先检测已有配置或创建并激活渠道。";
      }
      if (resource === "activePromptProfile") {
        return "该工具尚无启用的提示词档案，也没有可清理的受管基线；请先在提示词页通过图标启用一份档��。";
      }
    }
    // message 是按错误码分类的通用文案；details.reason 才是后端给出的具体原因。
    const reason = errorDetailString(error, "reason");
    return `${error.appError.code}：${reason ?? error.appError.message}`;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return error ? "操作失败，请重新扫描后再试。" : null;
}

function errorDetailString(error: ProfileRpcError, key: string): string | null {
  const value = error.appError.details?.[key];
  return typeof value === "string" ? value : null;
}

/** 后端仍在后台探测工具环境；这不是失败，而是"稍后重试即可"的等待态。 */
export function isEnvironmentProbingError(error: unknown): boolean {
  return (
    error instanceof ProfileRpcError &&
    error.appError.code === "ENVIRONMENT_PROBING"
  );
}

export const ENVIRONMENT_PROBING_RETRY_LIMIT = 40;
export const ENVIRONMENT_PROBING_RETRY_DELAY_MS = 500;

/**
 * QueryClient 默认重试策略：只对探测中错误重试，让查询停留在 pending
 * （页面据此渲染 `role="status"` 等待态），其它错误照旧立即失败。
 * 探测完成事件会触发失效重拉，重试上限只是兜底。
 */
export function retryWhileEnvironmentProbing(
  failureCount: number,
  error: unknown,
): boolean {
  return (
    isEnvironmentProbingError(error) &&
    failureCount < ENVIRONMENT_PROBING_RETRY_LIMIT
  );
}
