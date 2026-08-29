import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import { BlockingState } from "@/components/blocking-state";
import { SnapshotRestoreDialog } from "@/components/snapshot-restore-dialog";
import { Button } from "@/components/ui/button";
import { OnboardingWizard } from "@/features/onboarding/onboarding-wizard";
import { dashboardSummaryQueryOptions } from "@/lib/dashboard-api";
import { profileErrorText } from "@/lib/profile-api";

export function DashboardPage() {
  const dashboardQuery = useQuery(dashboardSummaryQueryOptions());
  const [wizardOpen, setWizardOpen] = useState(false);
  const [restoreOpen, setRestoreOpen] = useState(false);

  return (
    <main className="p-6 lg:p-8">
      <header className="mx-auto flex max-w-6xl flex-wrap items-start justify-between gap-4">
        <div>
          <p className="text-muted-foreground text-sm font-medium">总览</p>
          <h1 className="mt-1 text-2xl font-semibold">
            Claude 与 Codex 配置总览
          </h1>
          <p className="text-muted-foreground mt-2 text-sm">
            中央意图、原生目标状态、同步历史与私有恢复点集中在这里。
          </p>
        </div>
        {dashboardQuery.data && !dashboardQuery.data.needsOnboarding ? (
          <Button variant="outline" onClick={() => setWizardOpen(true)}>
            检测现有配置
          </Button>
        ) : null}
      </header>

      {dashboardQuery.isPending ? (
        <p role="status" className="mx-auto mt-8 max-w-6xl">
          正在汇总配置状态…
        </p>
      ) : null}
      {dashboardQuery.isError ? (
        <div className="mx-auto mt-8 max-w-6xl">
          <BlockingState
            title="无法读取总览"
            description={profileErrorText(dashboardQuery.error) ?? "读取失败"}
            actionLabel="重试"
            onAction={() => void dashboardQuery.refetch()}
          />
        </div>
      ) : null}

      {dashboardQuery.data?.interruptedRun ? (
        <div className="mx-auto mt-6 max-w-6xl">
          <BlockingState
            title="检测到未完成的写入或恢复"
            description="新的 Apply/Restore 已被阻止。请先查看私有快照和持久化 journal 证据。"
            code={`${dashboardQuery.data.interruptedRun.status} · ${dashboardQuery.data.interruptedRun.runId}`}
            actionLabel="打开恢复入口"
            onAction={() => setRestoreOpen(true)}
          />
        </div>
      ) : null}

      {dashboardQuery.data?.needsOnboarding ? (
        <section className="bg-card mx-auto mt-8 max-w-3xl rounded-xl border border-dashed p-8 text-center">
          <h2 className="text-xl font-semibold">尚未接管任何配置</h2>
          <p className="text-muted-foreground mt-2 text-sm leading-6">
            唯一下一步是运行只读检测；你可以逐工具选择导入/接管，也可以跳过并保持非受管。
          </p>
          <Button className="mt-5" onClick={() => setWizardOpen(true)}>
            开始首次检测
          </Button>
        </section>
      ) : null}

      {dashboardQuery.data && !dashboardQuery.data.needsOnboarding ? (
        <>
          <section
            className="mx-auto mt-6 grid max-w-6xl gap-4 md:grid-cols-2"
            aria-label="工具配置卡片"
          >
            {dashboardQuery.data.tools.map((tool) => (
              <article
                key={tool.tool}
                className="bg-card rounded-xl border p-5"
              >
                <div className="flex items-center justify-between gap-3">
                  <h2 className="text-lg font-semibold">
                    {tool.tool === "claude" ? "Claude" : "Codex"}
                  </h2>
                  <Link className="text-sm underline" to={`/${tool.tool}`}>
                    管理
                  </Link>
                </div>
                <dl className="mt-4 grid grid-cols-2 gap-3 text-sm">
                  <SummaryItem
                    label="当前渠道"
                    value={tool.activeProviderName ?? "未接管"}
                  />
                  <SummaryItem
                    label="当前提示词"
                    value={tool.activePromptName ?? "未接管"}
                  />
                  <SummaryItem
                    label="全局 MCP"
                    value={`${tool.globalMcpCount}`}
                  />
                  <SummaryItem
                    label="全局 Skills"
                    value={`${tool.globalSkillCount}`}
                  />
                </dl>
              </article>
            ))}
          </section>

          <section className="mx-auto mt-4 grid max-w-6xl gap-4 sm:grid-cols-3">
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
            <article className="bg-card rounded-xl border p-5">
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
            className="bg-card mx-auto mt-6 max-w-6xl rounded-xl border p-5"
            aria-labelledby="recent-sync-title"
          >
            <h2 id="recent-sync-title" className="text-lg font-semibold">
              最近同步
            </h2>
            <div className="mt-4 space-y-3">
              {dashboardQuery.data.recentSyncRuns.map((run) => (
                <article
                  key={run.id}
                  className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3 text-sm"
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
                <p className="text-muted-foreground text-sm">尚无同步记录。</p>
              ) : null}
            </div>
          </section>
        </>
      ) : null}

      <OnboardingWizard
        open={wizardOpen}
        onClose={() => setWizardOpen(false)}
      />
      <SnapshotRestoreDialog
        open={restoreOpen}
        onClose={() => setRestoreOpen(false)}
        initialSnapshotId={
          dashboardQuery.data?.interruptedRun?.targets.find(
            (target) => target.snapshotId,
          )?.snapshotId ?? null
        }
      />
    </main>
  );
}

function SummaryItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-muted rounded-lg p-3">
      <dt className="text-muted-foreground text-xs">{label}</dt>
      <dd className="mt-1 font-medium break-words">{value}</dd>
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
    <article className="bg-card rounded-xl border p-5">
      <p className="text-muted-foreground text-sm">{label}</p>
      <p className="mt-2 text-3xl font-semibold">{value}</p>
      <Link className="mt-4 inline-block text-sm underline" to={link}>
        查看
      </Link>
    </article>
  );
}
