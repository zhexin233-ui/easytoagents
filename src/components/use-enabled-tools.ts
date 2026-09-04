import { useQuery } from "@tanstack/react-query";

import { type Tool } from "@/bindings/commands";
import { appSettingsQueryOptions } from "@/lib/settings-api";
import { DEFAULT_ENABLED_TOOLS } from "@/lib/tool-metadata";

/**
 * 读取「启用的工具」设置；查询进行中或失败时回落到默认集合，
 * 保证渲染确定性（避免首帧闪现未启用的工具）。
 */
export function useEnabledTools(): ReadonlySet<Tool> {
  const { data } = useQuery(appSettingsQueryOptions());
  return new Set(data?.enabledTools ?? DEFAULT_ENABLED_TOOLS);
}
