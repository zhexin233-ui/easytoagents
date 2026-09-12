import { queryOptions } from "@tanstack/react-query";

import { commands, type Tool } from "@/bindings/commands";
import { unwrapResult } from "@/lib/rpc";

/** Agents 查询键与生成命令的唯一前端边界。 */
export const agentsKeys = {
  all: ["agents"] as const,
  list: () => [...agentsKeys.all, "list"] as const,
  agents: () => agentsKeys.list(),
  projects: () => [...agentsKeys.all, "projects"] as const,
  projectOptions: (projectId: string, tool: Tool) =>
    [...agentsKeys.all, "project-options", projectId, tool] as const,
  globalStatuses: () => [...agentsKeys.all, "global-statuses"] as const,
  globalTargetStatuses: () => agentsKeys.globalStatuses(),
};

// 允许与其他资源 API 的单数命名保持一致，调用方可逐步迁移而不复制键定义。
export const agentKeys = agentsKeys;

export function agentsQueryOptions() {
  return queryOptions({
    queryKey: agentsKeys.list(),
    queryFn: async () => unwrapResult(await commands.listAgents()),
  });
}

export function agentProjectsQueryOptions() {
  return queryOptions({
    queryKey: agentsKeys.projects(),
    queryFn: async () => unwrapResult(await commands.listAgentProjects()),
  });
}

export function agentProjectOptionsQueryOptions(projectId: string, tool: Tool) {
  return queryOptions({
    queryKey: agentsKeys.projectOptions(projectId, tool),
    queryFn: async () =>
      unwrapResult(await commands.listAgentProjectOptions({ projectId, tool })),
    enabled: projectId.length > 0,
  });
}

export function globalAgentStatusesQueryOptions() {
  return queryOptions({
    queryKey: agentsKeys.globalStatuses(),
    queryFn: async () =>
      unwrapResult(await commands.listGlobalAgentTargetStatuses()),
  });
}

export const globalAgentTargetStatusesQueryOptions =
  globalAgentStatusesQueryOptions;

/** 每次显式打开或重扫都使用独立缓存，避免复用旧的原生发现结果。 */
export function agentImportQueryOptions(tool: Tool, requestId: string) {
  return queryOptions({
    queryKey: ["agent-import", tool, requestId] as const,
    queryFn: async () =>
      unwrapResult(await commands.discoverAgentImport({ tool })),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
}
