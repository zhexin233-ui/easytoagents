import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Link } from "react-router-dom";

import { commands, type ProjectDto } from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { EmptyState } from "@/components/empty-state";
import { PageHeader } from "@/components/page-header";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import {
  invalidateProjectScope,
  projectsQueryOptions,
} from "@/lib/projects-api";

export function ProjectsPage() {
  const queryClient = useQueryClient();
  const projectsQuery = useQuery(projectsQueryOptions());
  const [rootPath, setRootPath] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const invalidateProjects = async () => {
    await invalidateProjectScope(queryClient, ["project"]);
  };
  const registerMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(await commands.registerProject({ rootPath, displayName })),
    onSuccess: async (project) => {
      setRootPath("");
      setDisplayName("");
      setMessage(
        `项目已登记；尚未对项目原生配置执行任何写入。原生资源：启用 ${project.nativeResources.active} · 已禁用 ${project.nativeResources.disabled}。`,
      );
      await invalidateProjects();
    },
  });
  const rescanMutation = useMutation({
    mutationFn: async (project: ProjectDto) =>
      unwrapResult(
        await commands.rescanProject({
          id: project.id,
          rowVersion: project.rowVersion,
        }),
      ),
    onSuccess: async () => {
      setMessage("项目状态已重新扫描。此次扫描是只读操作。");
      await invalidateProjects();
    },
  });
  const removeMutation = useMutation({
    mutationFn: async (project: ProjectDto) =>
      unwrapResult(
        await commands.removeProject({
          id: project.id,
          rowVersion: project.rowVersion,
        }),
      ),
    onSuccess: async (result) => {
      setMessage(
        result.nativeConfigurationLeftUnmanaged
          ? "项目登记已移除；已有原生配置保持原样并转为非受管。"
          : "项目登记已移除；未修改项目目录。",
      );
      await invalidateProjects();
    },
  });
  const operationError = profileErrorText(
    registerMutation.error ?? rescanMutation.error ?? removeMutation.error,
  );

  return (
    <>
      <PageHeader title="项目" />
      <main className="max-w-6xl space-y-6 px-8 py-6">
        <section
          className="bg-card rounded-xl border p-5"
          aria-labelledby="register-project-title"
        >
          <h2 id="register-project-title" className="text-[15px] font-semibold">
            登记本地项目
          </h2>
          <div className="mt-4 grid gap-4 md:grid-cols-[1fr_1fr_auto] md:items-end">
            <label className="text-sm font-medium">
              项目目录
              <input
                className="field mt-2"
                value={rootPath}
                onChange={(event) => setRootPath(event.target.value)}
                placeholder="选择规范化项目根"
              />
            </label>
            <label className="text-sm font-medium">
              显示名称
              <input
                className="field mt-2"
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
                placeholder="例如：客户端应用"
              />
            </label>
            <Button
              variant="outline"
              onClick={() => {
                void open({ directory: true, multiple: false }).then(
                  (selection) => {
                    if (typeof selection !== "string") return;
                    setRootPath(selection);
                    setDisplayName(
                      selection.split("/").filter(Boolean).at(-1) ?? "本地项目",
                    );
                  },
                );
              }}
            >
              选择目录
            </Button>
          </div>
          <Button
            className="mt-4"
            disabled={
              rootPath.trim().length === 0 ||
              displayName.trim().length === 0 ||
              registerMutation.isPending
            }
            onClick={() => registerMutation.mutate()}
          >
            {registerMutation.isPending ? "正在登记…" : "登记项目"}
          </Button>
        </section>

        <div aria-live="polite">
          {message ? <p className="text-success text-sm">{message}</p> : null}
          {operationError ? (
            <BlockingState
              title="项目操作未完成"
              description={operationError}
              actionLabel="重新读取"
              onAction={() => void projectsQuery.refetch()}
            />
          ) : null}
        </div>

        <section
          className="bg-card rounded-xl border p-5"
          aria-labelledby="project-list-title"
        >
          <h2 id="project-list-title" className="text-[15px] font-semibold">
            已登记项目
          </h2>
          {projectsQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在读取项目…
            </p>
          ) : null}
          {projectsQuery.isError ? (
            <div className="mt-4">
              <BlockingState
                title="无法读取项目"
                description={
                  profileErrorText(projectsQuery.error) ?? "读取失败"
                }
                actionLabel="重试"
                onAction={() => void projectsQuery.refetch()}
              />
            </div>
          ) : null}
          {projectsQuery.data?.length === 0 ? (
            <EmptyState
              title="尚无项目"
              description="选择目录登记第一个项目。"
            />
          ) : null}
          <div className="mt-4 grid gap-4 lg:grid-cols-2">
            {projectsQuery.data?.map((project) => (
              <article key={project.id} className="rounded-lg border p-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <h3 className="font-semibold">{project.displayName}</h3>
                    <code className="mt-1 block text-xs break-all">
                      {project.rootPath}
                    </code>
                  </div>
                  <span className="bg-muted rounded-full px-2 py-1 text-xs">
                    路径：{project.pathStatus}
                  </span>
                </div>
                <p className="text-muted-foreground mt-3 text-xs">
                  Git：{project.gitStatus} · Codex trust：
                  {project.codexTrustStatus} · Claude policy：
                  {project.claudePolicyStatus}
                </p>
                <p className="text-muted-foreground mt-2 text-xs">
                  原生资源：启用 {project.nativeResources.active} · 已禁用{" "}
                  {project.nativeResources.disabled}
                  {project.nativeResources.disabled > 0
                    ? "。移除登记前须先恢复已禁用资源。"
                    : ""}
                </p>
                <div className="mt-3 flex flex-wrap gap-2">
                  {project.targets.map((target) => (
                    <SyncStatusBadge
                      key={`${target.tool}-${target.artifactKind}`}
                      status={target.status}
                    />
                  ))}
                </div>
                <div className="mt-4 flex flex-wrap gap-2">
                  <Button asChild size="sm">
                    <Link to={`/projects/${project.id}`}>打开详情</Link>
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={rescanMutation.isPending}
                    onClick={() => rescanMutation.mutate(project)}
                  >
                    重新扫描
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={
                      removeMutation.isPending ||
                      project.nativeResources.disabled +
                        project.nativeResources.conflict >
                        0
                    }
                    onClick={() => removeMutation.mutate(project)}
                  >
                    移除登记
                  </Button>
                </div>
              </article>
            ))}
          </div>
        </section>
      </main>
    </>
  );
}
