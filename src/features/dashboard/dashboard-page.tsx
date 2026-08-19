import { useQuery } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";
import { appInfoQueryOptions } from "@/lib/app-info";

export function DashboardPage() {
  const appInfoQuery = useQuery(appInfoQueryOptions);

  return (
    <main className="grid min-h-screen place-items-center p-8">
      <section
        aria-labelledby="app-title"
        className="w-full max-w-xl rounded-lg border bg-white p-8 shadow-sm"
      >
        <p className="text-muted-foreground text-sm font-medium">
          EasyToAgents
        </p>
        <h1
          id="app-title"
          className="mt-2 text-3xl font-semibold tracking-tight"
        >
          Claude 与 Codex 配置中心
        </h1>
        <p className="text-muted-foreground mt-3 text-sm leading-6">
          桌面端基础框架已就绪，后续配置变更都将经过预览与确认。
        </p>

        <div
          className="bg-muted mt-8 rounded-md p-4 text-sm"
          aria-live="polite"
        >
          {appInfoQuery.isPending ? <p>正在检查桌面后端…</p> : null}

          {appInfoQuery.isError ? (
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p role="alert">暂时无法连接桌面后端。</p>
              <Button
                variant="outline"
                size="sm"
                onClick={() => void appInfoQuery.refetch()}
              >
                重新检查
              </Button>
            </div>
          ) : null}

          {appInfoQuery.data ? (
            <p>
              桌面后端已连接 · {appInfoQuery.data.name}{" "}
              {appInfoQuery.data.version}
            </p>
          ) : null}
        </div>
      </section>
    </main>
  );
}
