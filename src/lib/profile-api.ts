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
    return `${error.appError.code}：${error.appError.message}`;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return error ? "操作失败，请重新扫描后再试。" : null;
}

export const profileKeys = {
  all: ["profiles"] as const,
  providers: (tool: Tool) => [...profileKeys.all, tool, "providers"] as const,
  prompts: (tool: Tool) => [...profileKeys.all, tool, "prompts"] as const,
  status: (tool: Tool) => [...profileKeys.all, tool, "status"] as const,
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

export function toolProfileStatusQueryOptions(tool: Tool) {
  return queryOptions({
    queryKey: profileKeys.status(tool),
    queryFn: async () =>
      unwrapResult(await commands.getToolProfileStatus(tool)),
  });
}
