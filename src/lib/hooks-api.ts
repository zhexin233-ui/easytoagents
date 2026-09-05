import { queryOptions } from "@tanstack/react-query";

import { commands, type Tool } from "@/bindings/commands";
import { unwrapResult } from "@/lib/profile-api";

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
