import { lazy, Suspense, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronRight } from "lucide-react";
import { Link } from "react-router-dom";

import type { DashboardToolSummaryDto } from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { EmptyState } from "@/components/empty-state";
import { PageHeader } from "@/components/page-header";
import { RefreshEnvironmentButton } from "@/components/refresh-environment-button";
import { Button } from "@/components/ui/button";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { dashboardSummaryQueryOptions } from "@/lib/dashboard-api";
import { profileErrorText } from "@/lib/profile-api";
import { toolMetadata } from "@/lib/tool-metadata";

const OnboardingWizard = lazy(() =>
  import("@/features/onboarding/onboarding-wizard").then((module) => ({
    default: module.OnboardingWizard,
  })),
);
const SnapshotRestoreDialog = lazy(() =>
  import("@/components/snapshot-restore-dialog").then((module) => ({
    default: module.SnapshotRestoreDialog,
  })),
);

export function DashboardPage() {
  const dashboardQuery = useQuery(dashboardSummaryQueryOptions());
  const enabledTools = useEnabledTools();
  const [wizardOpen, setWizardOpen] = useState(false);
  const [restoreOpen, setRestoreOpen] = useState(false);

  return (
    <>
      <PageHeader
        title="工具配置总览"
        actions={
          dashboardQuery.data && !dashboardQuery.data.needsOnboarding ? (
            // 空状态刻意只保留"开始首次检测"这一个下一步；工具重新检测入口
            // 在设置对话框里始终可用。
            <div className="flex items-center gap-1">
              <Button variant="outline" onClick={() => setWizardOpen(true)}>
                检测现有配置
              </Button>
              <RefreshEnvironmentButton appearance="icon" />
            </div>
          ) : undefined
        }
      />
      <main className="space-y-6 px-8 py-6">
        {dashboardQuery.isPending ? (
          <p role="status">正在汇总配置状态…</p>
        ) : null}
        {dashboardQuery.isError ? (
          <BlockingState
            title="无法读取总览"
            description={profileErrorText(dashboardQuery.error) ?? "读取失败"}
            actionLabel="重试"
            onAction={() => void dashboardQuery.refetch()}
          />
        ) : null}

        {dashboardQuery.data?.interruptedRun ? (
          <BlockingState
            title="检测到未完成的写入或恢复"
            description="新的 Apply/Restore 已被阻止。请先在恢复点中处理。"
            code={`${dashboardQuery.data.interruptedRun.status} · ${dashboardQuery.data.interruptedRun.runId}`}
            actionLabel="打开恢复入口"
            onAction={() => setRestoreOpen(true)}
          />
        ) : null}

        {dashboardQuery.data?.needsOnboarding ? (
          <EmptyState
            title="尚未接管任何配置"
            description="运行只读检测后，可逐工具选择导入或跳过。"
            action={
              <Button onClick={() => setWizardOpen(true)}>开始首次检测</Button>
            }
          />
        ) : null}

        {dashboardQuery.data && !dashboardQuery.data.needsOnboarding ? (
          <>
            <section
              className="grid gap-4 md:grid-cols-2 2xl:grid-cols-3"
              aria-label="工具配置卡片"
            >
              {dashboardQuery.data.tools
                .filter((tool) => enabledTools.has(tool.tool))
                .map((tool) => (
                  <ToolSummaryCard key={tool.tool} tool={tool} />
                ))}
            </section>

            <section className="grid gap-4 sm:grid-cols-3">
              <MetricCard
                label="项目"
                value={dashboardQuery.data.projectCount}
                link="/projects"
              />
              <MetricCard
                label="待处理冲突"
                value={dashboardQuery.data.conflictCount}
                link="/projects?status=conflict"
              />
              <article className="bg-card rounded-lg border p-5">
                <p className="text-muted-foreground text-sm">私有快照</p>
                <p className="mt-2 text-3xl font-semibold">
                  {dashboardQuery.data.snapshotCount}
                </p>
                <Button
                  className="mt-4"
                  size="sm"
                  variant="outline"
                  onClick={() => setRestoreOpen(true)}
                >
                  查看恢复点
                </Button>
              </article>
            </section>

            <section
              className="bg-card rounded-lg border p-5"
              aria-labelledby="recent-sync-title"
            >
              <h2 id="recent-sync-title" className="text-[15px] font-semibold">
                最近同步
              </h2>
              <div className="mt-3 divide-y">
                {dashboardQuery.data.recentSyncRuns.map((run) => (
                  <article
                    key={run.id}
                    className="hover:bg-muted/50 flex flex-wrap items-center justify-between gap-3 px-1 py-2.5 text-sm"
                  >
                    <div>
                      <p className="font-medium">
                        {run.kind} · {run.scope}
                      </p>
                      <p className="text-muted-foreground mt-1 text-xs">
                        {run.startedAt}
                      </p>
                    </div>
                    <span className="bg-muted rounded-full px-2 py-1 text-xs">
                      {run.status}
                      {run.errorCode ? ` · ${run.errorCode}` : ""}
                    </span>
                  </article>
                ))}
                {dashboardQuery.data.recentSyncRuns.length === 0 ? (
                  <p className="text-muted-foreground py-2.5 text-sm">
                    尚无同步记录。
                  </p>
                ) : null}
              </div>
            </section>
          </>
        ) : null}
      </main>

      {wizardOpen ? (
        <Suspense fallback={<DialogLoading label="正在打开检测向导…" />}>
          <OnboardingWizard open onClose={() => setWizardOpen(false)} />
        </Suspense>
      ) : null}
      {restoreOpen ? (
        <Suspense fallback={<DialogLoading label="正在打开恢复点…" />}>
          <SnapshotRestoreDialog
            open
            onClose={() => setRestoreOpen(false)}
            initialSnapshotId={
              dashboardQuery.data?.interruptedRun?.targets.find(
                (target) => target.snapshotId,
              )?.snapshotId ?? null
            }
          />
        </Suspense>
      ) : null}
    </>
  );
}

function DialogLoading({ label }: { label: string }) {
  return (
    <p role="status" className="sr-only">
      {label}
    </p>
  );
}

function ToolSummaryCard({ tool }: { tool: DashboardToolSummaryDto }) {
  const metadata = toolMetadata(tool.tool);
  const resourceRoute = metadata.profileRoute ?? "/mcp";

  return (
    <article className="bg-card rounded-lg border p-5">
      <div className="flex items-center justify-between gap-3">
        <h2 className="flex items-center gap-2 text-lg font-semibold">
          <img
            src={metadata.icon}
            alt=""
            aria-hidden="true"
            className="rounded-control size-5 object-contain"
          />
          {metadata.label}
        </h2>
        <Button asChild variant="ghost" size="sm">
          <Link to={resourceRoute}>
            {metadata.profileRoute ? "管理" : "管理 MCP/Skills"}
          </Link>
        </Button>
      </div>
      <dl className="mt-4 grid grid-cols-2 gap-x-4 gap-y-3 text-sm">
        <SummaryItem
          label="当前渠道"
          value={
            metadata.capabilities.provider
              ? (tool.activeProviderName ?? "未接管")
              : "不支持"
          }
        />
        <SummaryItem
          label="当前提示词"
          value={
            metadata.capabilities.promptGlobal
              ? (tool.activePromptName ?? "未接管")
              : "不支持"
          }
        />
        <SummaryItem label="全局 MCP" value={`${tool.globalMcpCount}`} />
        <SummaryItem label="全局 Skills" value={`${tool.globalSkillCount}`} />
      </dl>
    </article>
  );
}

function SummaryItem({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-muted-foreground text-xs">{label}</dt>
      <dd className="mt-0.5 font-medium break-words">{value}</dd>
    </div>
  );
}

function MetricCard({
  label,
  value,
  link,
}: {
  label: string;
  value: number;
  link: string;
}) {
  return (
    <article className="bg-card rounded-lg border p-5">
      <p className="text-muted-foreground text-sm">{label}</p>
      <p className="mt-2 text-2xl font-semibold tabular-nums">{value}</p>
      <Button asChild className="mt-3" size="sm" variant="ghost">
        <Link to={link}>
          查看
          <ChevronRight aria-hidden="true" className="size-3.5" />
        </Link>
      </Button>
    </article>
  );
}
