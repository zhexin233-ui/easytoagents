import { queryOptions } from "@tanstack/react-query";

import { commands } from "@/bindings/commands";
import { unwrapResult } from "@/lib/profile-api";

export const syncKeys = {
  all: ["sync"] as const,
  snapshots: () => [...syncKeys.all, "snapshots"] as const,
  interrupted: () => [...syncKeys.all, "interrupted"] as const,
};

export function snapshotsQueryOptions() {
  return queryOptions({
    queryKey: syncKeys.snapshots(),
    queryFn: async () => unwrapResult(await commands.listSnapshots()),
  });
}

export function interruptedRunQueryOptions() {
  return queryOptions({
    queryKey: syncKeys.interrupted(),
    queryFn: async () => unwrapResult(await commands.getInterruptedRun()),
  });
}
