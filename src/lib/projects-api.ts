import { queryOptions } from "@tanstack/react-query";

import type { ArtifactKind, Tool } from "@/bindings/commands";
import { commands } from "@/bindings/commands";
import { unwrapResult } from "@/lib/profile-api";

export const projectKeys = {
  all: ["projects"] as const,
  list: () => [...projectKeys.all, "list"] as const,
  detail: (id: string) => [...projectKeys.all, "detail", id] as const,
  nativeResources: (id: string, tool: Tool, artifactKind: ArtifactKind) =>
    [...projectKeys.all, "native-resources", id, tool, artifactKind] as const,
};

export function projectsQueryOptions() {
  return queryOptions({
    queryKey: projectKeys.list(),
    queryFn: async () => unwrapResult(await commands.listProjects()),
  });
}

export function projectQueryOptions(id: string) {
  return queryOptions({
    queryKey: projectKeys.detail(id),
    queryFn: async () => unwrapResult(await commands.getProject(id)),
    enabled: id.length > 0,
  });
}

export function projectNativeResourcesQueryOptions(
  projectId: string,
  tool: Tool,
  artifactKind: ArtifactKind,
) {
  return queryOptions({
    queryKey: projectKeys.nativeResources(projectId, tool, artifactKind),
    queryFn: async () =>
      unwrapResult(
        await commands.listProjectNativeResources({
          projectId,
          tool,
          artifactKind,
        }),
      ),
    enabled: projectId.length > 0,
  });
}
