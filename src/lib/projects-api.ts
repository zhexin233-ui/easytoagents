import { queryOptions, type QueryClient } from "@tanstack/react-query";

import type { ArtifactKind, Tool } from "@/bindings/commands";
import { commands } from "@/bindings/commands";
import { agentsKeys } from "@/lib/agents-api";
import { hooksKeys } from "@/lib/hooks-api";
import { mcpKeys } from "@/lib/mcp-api";
import { unwrapResult } from "@/lib/profile-api";
import { profileKeys } from "@/lib/profile-api";
import { syncKeys } from "@/lib/sync-api";
import { skillKeys } from "@/lib/skills-api";

// Agent targets use their own directory/file API.  They are not whole-project
// native resources, so keep them out of the generic resource command union;
// otherwise the frontend could send an Agent artifact to a backend command
// that intentionally rejects it.
export type ProjectResourceKind = Exclude<
  ArtifactKind,
  "provider" | "prompt" | "agent"
>;
export type ProjectScopeKind = ProjectResourceKind | "agent" | "project";

export const projectKeys = {
  all: ["projects"] as const,
  list: () => [...projectKeys.all, "list"] as const,
  detail: (id: string) => [...projectKeys.all, "detail", id] as const,
  nativeResources: (
    id: string,
    tool: Tool,
    artifactKind: ProjectResourceKind,
  ) =>
    [...projectKeys.all, "native-resources", id, tool, artifactKind] as const,
};

/**
 * Invalidate the complete cache surface affected by project/resource changes.
 * Keeping this mapping here prevents each assignment card from drifting into
 * a different refresh contract.
 */
export function invalidateProjectScope(
  queryClient: QueryClient,
  kinds: readonly ProjectScopeKind[],
) {
  const keys = new Map<string, readonly unknown[]>();
  const add = (key: readonly unknown[]) => keys.set(JSON.stringify(key), key);
  for (const kind of kinds) {
    switch (kind) {
      case "project":
        add(projectKeys.all);
        add(mcpKeys.projects());
        add(skillKeys.projects());
        add(hooksKeys.projects());
        add(agentsKeys.projects());
        add(profileKeys.all);
        add(syncKeys.all);
        break;
      case "mcp":
        add(projectKeys.all);
        add(mcpKeys.all);
        add(syncKeys.all);
        break;
      case "skill":
        add(projectKeys.all);
        add(skillKeys.all);
        add(syncKeys.all);
        break;
      case "hook":
        add(projectKeys.all);
        add(hooksKeys.all);
        add(syncKeys.all);
        break;
      case "agent":
        add(projectKeys.all);
        add(agentsKeys.all);
        add(syncKeys.all);
        break;
    }
  }
  return Promise.all(
    [...keys.values()].map((queryKey) =>
      queryClient.invalidateQueries({ queryKey }),
    ),
  );
}

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
  artifactKind: ProjectResourceKind,
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
