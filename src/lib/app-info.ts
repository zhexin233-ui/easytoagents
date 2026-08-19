import { queryOptions } from "@tanstack/react-query";

import { commands } from "@/bindings/commands";

export const appInfoQueryOptions = queryOptions({
  queryKey: ["app-info"],
  queryFn: () => commands.getAppInfo(),
  staleTime: Number.POSITIVE_INFINITY,
});
