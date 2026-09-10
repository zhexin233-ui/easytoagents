import { queryOptions, type QueryClient } from "@tanstack/react-query";

import { commands } from "@/bindings/commands";
import { dashboardKeys } from "@/lib/dashboard-api";
import { hooksKeys } from "@/lib/hooks-api";
import { mcpKeys } from "@/lib/mcp-api";
import { profileKeys } from "@/lib/profile-api";
import { projectKeys } from "@/lib/projects-api";
import { unwrapResult } from "@/lib/rpc";
import { skillKeys } from "@/lib/skills-api";

export const environmentKeys = {
  all: ["environment"] as const,
  state: () => [...environmentKeys.all, "state"] as const,
};

/** 后端工具探测状态：`probing` 为真时主窗口已可用，但依赖环境的命令会暂时失败。 */
export function environmentStateQueryOptions() {
  return queryOptions({
    queryKey: environmentKeys.state(),
    queryFn: async () => unwrapResult(await commands.getEnvironmentState()),
  });
}

/**
 * 探测完成或刷新后，所有读取 `ExplicitEnvironment` 的查询家族都要重新拉取：
 * 总览、各工具状态卡、MCP/Skills/Hooks 全局目标状态与项目目标状态。
 */
export function invalidateEnvironmentDependents(queryClient: QueryClient) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: environmentKeys.all }),
    queryClient.invalidateQueries({ queryKey: dashboardKeys.all }),
    queryClient.invalidateQueries({ queryKey: profileKeys.all }),
    queryClient.invalidateQueries({ queryKey: mcpKeys.all }),
    queryClient.invalidateQueries({ queryKey: skillKeys.all }),
    queryClient.invalidateQueries({ queryKey: hooksKeys.all }),
    queryClient.invalidateQueries({ queryKey: projectKeys.all }),
  ]);
}
