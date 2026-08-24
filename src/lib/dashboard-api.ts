import { queryOptions } from "@tanstack/react-query";

import { commands } from "@/bindings/commands";
import { unwrapResult } from "@/lib/profile-api";

export const dashboardKeys = {
  all: ["dashboard"] as const,
  summary: () => [...dashboardKeys.all, "summary"] as const,
};

export function dashboardSummaryQueryOptions() {
  return queryOptions({
    queryKey: dashboardKeys.summary(),
    queryFn: async () => unwrapResult(await commands.getDashboardSummary()),
  });
}
