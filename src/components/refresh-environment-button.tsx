import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type ToolAvailabilityState,
  type ToolInstallationDto,
} from "@/bindings/commands";
import { Button } from "@/components/ui/button";
import {
  environmentStateQueryOptions,
  invalidateEnvironmentDependents,
} from "@/lib/environment-api";
import { profileErrorText, unwrapResult } from "@/lib/rpc";
import { toolMetadata } from "@/lib/tool-metadata";

const AVAILABILITY_TEXT: Record<ToolAvailabilityState, string> = {
  installed: "已检测到",
  unavailable: "未检测到",
  unsupported: "探针未能确认",
};

interface RefreshEnvironmentButtonProps {
  /** 是否在按钮下方列出每个工具的检测结果。 */
  showToolList?: boolean;
}

/**
 * "重新检测工具"入口：总览页与设置对话框共用。探测在后端线程池执行，
 * 完成后失效所有依赖环境的查询家族；探测中渲染 `role="status"` 等待态。
 */
export function RefreshEnvironmentButton({
  showToolList = false,
}: RefreshEnvironmentButtonProps) {
  const queryClient = useQueryClient();
  const stateQuery = useQuery(environmentStateQueryOptions());
  const refreshMutation = useMutation({
    mutationFn: async () => unwrapResult(await commands.refreshEnvironment()),
    onSuccess: async () => {
      await invalidateEnvironmentDependents(queryClient);
    },
  });
  const probing = stateQuery.data?.probing === true;
  const busy = probing || refreshMutation.isPending;

  return (
    <div className="space-y-2 text-sm">
      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={busy}
        onClick={() => refreshMutation.mutate()}
      >
        {busy ? "正在检测工具…" : "重新检测工具"}
      </Button>
      {busy ? (
        <p role="status" className="text-muted-foreground">
          正在检测本机工具安装状态，完成后相关页面会自动刷新。
        </p>
      ) : null}
      {refreshMutation.isSuccess && !busy ? (
        <p role="status" className="text-muted-foreground">
          已重新检测工具环境。
        </p>
      ) : null}
      {refreshMutation.isError ? (
        <p role="alert" className="text-red-700 dark:text-red-300">
          {profileErrorText(refreshMutation.error) ?? "重新检测失败。"}
        </p>
      ) : null}
      {showToolList && stateQuery.data && !probing ? (
        <ul
          className="text-muted-foreground space-y-1"
          aria-label="工具检测结果"
        >
          {stateQuery.data.tools.map((tool) => (
            <li key={tool.tool}>{toolInstallationText(tool)}</li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function toolInstallationText(tool: ToolInstallationDto): string {
  const label = toolMetadata(tool.tool).label;
  const version = tool.installationVersion
    ? ` ${tool.installationVersion}`
    : "";
  const diagnostic = tool.installationProbeDiagnostic
    ? `（${tool.installationProbeDiagnostic}）`
    : "";
  return `${label}：${AVAILABILITY_TEXT[tool.availability]}${version}${diagnostic}`;
}
