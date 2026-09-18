import { queryOptions } from "@tanstack/react-query";

import { commands } from "@/bindings/commands";
import { unwrapResult } from "@/lib/profile-api";

export const settingsKeys = {
  all: ["settings"] as const,
};

export function appSettingsQueryOptions() {
  return queryOptions({
    queryKey: settingsKeys.all,
    queryFn: async () => unwrapResult(await commands.getAppSettings()),
  });
}
