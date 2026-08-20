import { queryOptions } from "@tanstack/react-query";

import { commands, type Tool } from "@/bindings/commands";
import { unwrapResult } from "@/lib/profile-api";

export const mcpKeys = {
  all: ["mcp"] as const,
  servers: () => [...mcpKeys.all, "servers"] as const,
  projects: () => [...mcpKeys.all, "projects"] as const,
  projectOptions: (projectId: string, tool: Tool) =>
    [...mcpKeys.all, "project-options", projectId, tool] as const,
  globalStatuses: () => [...mcpKeys.all, "global-statuses"] as const,
};

export function mcpServersQueryOptions() {
  return queryOptions({
    queryKey: mcpKeys.servers(),
    queryFn: async () => unwrapResult(await commands.listMcpServers()),
  });
}

export function mcpProjectsQueryOptions() {
  return queryOptions({
    queryKey: mcpKeys.projects(),
    queryFn: async () => unwrapResult(await commands.listMcpProjects()),
  });
}

export function mcpProjectOptionsQueryOptions(projectId: string, tool: Tool) {
  return queryOptions({
    queryKey: mcpKeys.projectOptions(projectId, tool),
    queryFn: async () =>
      unwrapResult(await commands.listMcpProjectOptions({ projectId, tool })),
    enabled: projectId.length > 0,
  });
}

export function globalMcpStatusesQueryOptions() {
  return queryOptions({
    queryKey: mcpKeys.globalStatuses(),
    queryFn: async () =>
      unwrapResult(await commands.listGlobalMcpTargetStatuses()),
  });
}
