import { queryOptions } from "@tanstack/react-query";

import { commands, type Tool } from "@/bindings/commands";
import { unwrapResult } from "@/lib/rpc";

// 通用 RPC 解包与错误文案已迁到 `@/lib/rpc`；这里 re-export 以兼容既有导入方。
export { ProfileRpcError, profileErrorText, unwrapResult } from "@/lib/rpc";

const profileKeyBase = ["profiles"] as const;

export const profileKeys = {
  all: profileKeyBase,
  providers: (tool: Tool) => [...profileKeyBase, tool, "providers"] as const,
  prompts: [...profileKeyBase, "prompts"] as const,
  status: (tool: Tool) => [...profileKeyBase, tool, "status"] as const,
};

export function providerProfilesQueryOptions(tool: Tool) {
  return queryOptions({
    queryKey: profileKeys.providers(tool),
    queryFn: async () =>
      unwrapResult(await commands.listProviderProfiles(tool)),
  });
}

export function promptProfilesQueryOptions() {
  return queryOptions({
    queryKey: profileKeys.prompts,
    queryFn: async () => unwrapResult(await commands.listPromptProfiles()),
  });
}

export function toolProfileStatusQueryOptions(tool: Tool) {
  return queryOptions({
    queryKey: profileKeys.status(tool),
    queryFn: async () =>
      unwrapResult(await commands.getToolProfileStatus(tool)),
  });
}
