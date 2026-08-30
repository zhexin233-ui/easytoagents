import { queryOptions } from "@tanstack/react-query";

import {
  commands,
  type AppError,
  type Result,
  type Tool,
} from "@/bindings/commands";

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
        return "尚无生效提示词档案，也没有可清理的受管基线；请先检测已有配置或创建并激活提示词。";
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

export const profileKeys = {
  all: ["profiles"] as const,
  providers: (tool: Tool) => [...profileKeys.all, tool, "providers"] as const,
  prompts: (tool: Tool) => [...profileKeys.all, tool, "prompts"] as const,
  status: (tool: Tool) => [...profileKeys.all, tool, "status"] as const,
  promptProject: (projectId: string, tool: Tool) =>
    [...profileKeys.all, "projects", projectId, tool, "prompt"] as const,
};

export function providerProfilesQueryOptions(tool: Tool) {
  return queryOptions({
    queryKey: profileKeys.providers(tool),
    queryFn: async () =>
      unwrapResult(await commands.listProviderProfiles(tool)),
  });
}

export function promptProfilesQueryOptions(tool: Tool) {
  return queryOptions({
    queryKey: profileKeys.prompts(tool),
    queryFn: async () => unwrapResult(await commands.listPromptProfiles(tool)),
  });
}

export function promptProjectAssignmentQueryOptions(
  projectId: string,
  tool: Tool,
) {
  return queryOptions({
    queryKey: profileKeys.promptProject(projectId, tool),
    queryFn: async () =>
      unwrapResult(await commands.getPromptProjectAssignment(projectId, tool)),
  });
}

export function toolProfileStatusQueryOptions(tool: Tool) {
  return queryOptions({
    queryKey: profileKeys.status(tool),
    queryFn: async () =>
      unwrapResult(await commands.getToolProfileStatus(tool)),
  });
}
