import { queryOptions } from "@tanstack/react-query";

import { commands, type Tool } from "@/bindings/commands";
import { unwrapResult } from "@/lib/rpc";

export const hooksKeys = {
  all: ["hooks"] as const,
  hooks: () => [...hooksKeys.all, "hooks"] as const,
  projects: () => [...hooksKeys.all, "projects"] as const,
  projectOptions: (projectId: string, tool: Tool) =>
    [...hooksKeys.all, "project-options", projectId, tool] as const,
  globalStatuses: () => [...hooksKeys.all, "global-statuses"] as const,
};

export function hooksQueryOptions() {
  return queryOptions({
    queryKey: hooksKeys.hooks(),
    queryFn: async () => unwrapResult(await commands.listHooks()),
  });
}

export function hookProjectsQueryOptions() {
  return queryOptions({
    queryKey: hooksKeys.projects(),
    queryFn: async () => unwrapResult(await commands.listHookProjects()),
  });
}

export function hookProjectOptionsQueryOptions(projectId: string, tool: Tool) {
  return queryOptions({
    queryKey: hooksKeys.projectOptions(projectId, tool),
    queryFn: async () =>
      unwrapResult(await commands.listHookProjectOptions({ projectId, tool })),
    enabled: projectId.length > 0,
  });
}

export function globalHookStatusesQueryOptions() {
  return queryOptions({
    queryKey: hooksKeys.globalStatuses(),
    queryFn: async () =>
      unwrapResult(await commands.listGlobalHookTargetStatuses()),
  });
}

/** 一次"检测并导入"请求的只读发现结果；`requestId` 让每次打开对话框都重新扫描。 */
export function hookImportQueryOptions(tool: Tool, requestId: string) {
  return queryOptions({
    queryKey: ["hook-import", tool, requestId] as const,
    queryFn: async () =>
      unwrapResult(await commands.discoverHookImport({ tool })),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
}
