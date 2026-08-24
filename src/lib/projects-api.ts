import { queryOptions } from "@tanstack/react-query";

import { commands } from "@/bindings/commands";
import { unwrapResult } from "@/lib/profile-api";

export const projectKeys = {
  all: ["projects"] as const,
  list: () => [...projectKeys.all, "list"] as const,
  detail: (id: string) => [...projectKeys.all, "detail", id] as const,
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
