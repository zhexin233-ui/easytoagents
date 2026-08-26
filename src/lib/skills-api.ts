import { queryOptions } from "@tanstack/react-query";

import { commands, type Tool } from "@/bindings/commands";
import { unwrapResult } from "@/lib/profile-api";

export const skillKeys = {
  all: ["skills"] as const,
  list: () => [...skillKeys.all, "list"] as const,
  projects: () => [...skillKeys.all, "projects"] as const,
  projectOptions: (projectId: string, tool: Tool) =>
    [...skillKeys.all, "project-options", projectId, tool] as const,
  globalStatuses: () => [...skillKeys.all, "global-statuses"] as const,
};

// 每次显式检测使用独立缓存，避免关闭或换工具后复用旧来源证据。
export function skillImportQueryOptions(tool: Tool, requestId: string) {
  return queryOptions({
    queryKey: ["skill-import", tool, requestId] as const,
    queryFn: async () => unwrapResult(await commands.discoverSkillImport(tool)),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
}

export function skillsQueryOptions() {
  return queryOptions({
    queryKey: skillKeys.list(),
    queryFn: async () => unwrapResult(await commands.listSkills()),
  });
}

export function skillProjectsQueryOptions() {
  return queryOptions({
    queryKey: skillKeys.projects(),
    queryFn: async () => unwrapResult(await commands.listSkillProjects()),
  });
}

export function skillProjectOptionsQueryOptions(projectId: string, tool: Tool) {
  return queryOptions({
    queryKey: skillKeys.projectOptions(projectId, tool),
    queryFn: async () =>
      unwrapResult(await commands.listSkillProjectOptions({ projectId, tool })),
    enabled: projectId.length > 0,
  });
}

export function globalSkillStatusesQueryOptions() {
  return queryOptions({
    queryKey: skillKeys.globalStatuses(),
    queryFn: async () =>
      unwrapResult(await commands.listGlobalSkillTargetStatuses()),
  });
}
