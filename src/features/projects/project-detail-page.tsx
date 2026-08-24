import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "react-router-dom";

import {
  commands,
  type ArtifactKind,
  type McpProjectOptionDto,
  type PreviewPlan,
  type ProjectDto,
  type SkillProjectOptionDto,
  type Tool,
} from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { mcpKeys, mcpProjectOptionsQueryOptions } from "@/lib/mcp-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { projectKeys, projectQueryOptions } from "@/lib/projects-api";
import { skillKeys, skillProjectOptionsQueryOptions } from "@/lib/skills-api";

interface OpenProjectPreview {
  plan: PreviewPlan;
  tool: Tool;
  artifactKind: ArtifactKind;
}

export function ProjectDetailPage() {
  const { projectId = "" } = useParams();
  const queryClient = useQueryClient();
  const projectQuery = useQuery(projectQueryOptions(projectId));
  const [openPreview, setOpenPreview] = useState<OpenProjectPreview | null>(
    null,
  );
  const [message, setMessage] = useState<string | null>(null);
  const applyMutation = useMutation({
    mutationFn: async (preview: OpenProjectPreview) => {
      if (preview.artifactKind === "mcp") {
        return unwrapResult(
          await commands.applyMcpPreview({
            previewId: preview.plan.previewId,
            tool: preview.tool,
            projectId,
          }),
        );
      }
      return unwrapResult(
        await commands.applySkillPreview({
          previewId: preview.plan.previewId,
          tool: preview.tool,
          projectId,
        }),
      );
    },
    onSuccess: async () => {
      setOpenPreview(null);
      setMessage("项目原生配置已通过持久化预览应用并完成写后验证。");
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: projectKeys.all }),
        queryClient.invalidateQueries({ queryKey: mcpKeys.all }),
        queryClient.invalidateQueries({ queryKey: skillKeys.all }),
      ]);
    },
  });

  if (projectQuery.isPending) {
    return (
      <main className="min-h-screen p-8">
        <p role="status">正在读取项目详情…</p>
      </main>
    );
  }
  if (projectQuery.isError || !projectQuery.data) {
    return (
      <main className="min-h-screen p-8">
        <BlockingState
          title="项目详情不可用"
          description={
            profileErrorText(projectQuery.error) ?? "项目不存在或已移除。"
          }
          actionLabel="返回项目列表"
          onAction={() => window.location.assign("#/projects")}
        />
      </main>
    );
  }
  const project = projectQuery.data;

  return (
    <main className="min-h-screen p-6 lg:p-8">
      <header className="mx-auto max-w-6xl">
        <Link className="text-sm underline" to="/projects">
          ← 返回项目列表
        </Link>
        <h1 className="mt-4 text-2xl font-semibold">{project.displayName}</h1>
        <code className="mt-2 block text-xs break-all">{project.rootPath}</code>
        <p className="text-muted-foreground mt-2 text-sm">
          Git：{project.gitStatus} · Codex trust：{project.codexTrustStatus} ·
          Claude policy：{project.claudePolicyStatus}
        </p>
      </header>

      <div className="mx-auto mt-5 max-w-6xl space-y-3" aria-live="polite">
        {project.pathStatus !== "valid" ? (
          <BlockingState
            title="项目根不可安全使用"
            description="重新扫描确认路径恢复前，所有项目预览与应用都会被阻止。"
            code={project.pathStatus}
          />
        ) : null}
        {message ? <p className="text-sm text-emerald-800">{message}</p> : null}
        {applyMutation.isError ? (
          <BlockingState
            title="应用项目预览失败"
            description={profileErrorText(applyMutation.error) ?? "应用失败"}
          />
        ) : null}
      </div>

      <section
        className="mx-auto mt-6 max-w-6xl rounded-xl border bg-white p-5"
        aria-labelledby="project-status-title"
      >
        <h2 id="project-status-title" className="text-lg font-semibold">
          双工具配置状态
        </h2>
        <div className="mt-4 grid gap-3 md:grid-cols-2">
          {project.targets.map((target) => (
            <article
              key={`${target.tool}-${target.artifactKind}`}
              className="rounded-lg border p-4"
            >
              <div className="flex items-center justify-between gap-3">
                <p className="font-medium">
                  {toolLabel(target.tool)} ·{" "}
                  {artifactLabel(target.artifactKind)}
                </p>
                <SyncStatusBadge status={target.status} />
              </div>
              <code className="mt-2 block text-xs break-all">
                {target.targetPath ?? "目标路径不可用"}
              </code>
              {target.diagnosticCode ? (
                <p className="mt-2 text-xs">诊断：{target.diagnosticCode}</p>
              ) : null}
            </article>
          ))}
        </div>
      </section>

      <div className="mx-auto mt-6 grid max-w-6xl gap-6 xl:grid-cols-2">
        {(["claude", "codex"] as const).map((tool) => (
          <section key={tool} className="space-y-5">
            <h2 className="text-xl font-semibold">
              {toolLabel(tool)} 项目追加
            </h2>
            <ProjectMcpAssignments
              project={project}
              tool={tool}
              onPreview={(plan) =>
                setOpenPreview({ plan, tool, artifactKind: "mcp" })
              }
              onMessage={setMessage}
            />
            <ProjectSkillAssignments
              project={project}
              tool={tool}
              onPreview={(plan) =>
                setOpenPreview({ plan, tool, artifactKind: "skill" })
              }
              onMessage={setMessage}
            />
          </section>
        ))}
      </div>

      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? "claude"}
        artifactKind={openPreview?.artifactKind ?? "mcp"}
        applying={applyMutation.isPending}
        onClose={() => setOpenPreview(null)}
        onApply={() => {
          if (openPreview) applyMutation.mutate(openPreview);
        }}
      />
    </main>
  );
}

function ProjectMcpAssignments({
  project,
  tool,
  onPreview,
  onMessage,
}: {
  project: ProjectDto;
  tool: Tool;
  onPreview: (preview: PreviewPlan) => void;
  onMessage: (message: string) => void;
}) {
  const queryClient = useQueryClient();
  const optionsQuery = useQuery(
    mcpProjectOptionsQueryOptions(project.id, tool),
  );
  const [excludeFromGit, setExcludeFromGit] = useState(false);
  const assignmentMutation = useMutation({
    mutationFn: async ({
      option,
      assigned,
    }: {
      option: McpProjectOptionDto;
      assigned: boolean;
    }) =>
      unwrapResult(
        await commands.setProjectMcpAssignment({
          projectId: project.id,
          tool,
          mcpId: option.mcpId,
          assigned,
          mcpRowVersion: option.rowVersion,
          projectRowVersion: project.rowVersion,
        }),
      ),
    onSuccess: async () => {
      onMessage("MCP 项目追加意图已更新；原生配置尚未写入。");
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: projectKeys.all }),
        queryClient.invalidateQueries({ queryKey: mcpKeys.all }),
        queryClient.invalidateQueries({ queryKey: skillKeys.all }),
      ]);
    },
  });
  const previewMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(
        await commands.previewMcpSync({
          tool,
          projectId: project.id,
          excludeFromGit,
        }),
      ),
    onSuccess: (preview) => {
      if (preview.targets.length === 0) {
        onMessage("该项目只有全局继承 MCP，不需要创建项目配置文件。");
      } else {
        onPreview(preview);
      }
    },
  });
  const blocked = projectBlocked(project, tool);
  return (
    <AssignmentCard
      title="MCP"
      description="全局项持续继承且只读；项目只能追加其他中央 MCP。"
      blocked={blocked}
      error={profileErrorText(
        optionsQuery.error ?? assignmentMutation.error ?? previewMutation.error,
      )}
      pending={optionsQuery.isPending}
      empty={optionsQuery.data?.length === 0}
      excludeFromGit={excludeFromGit}
      onExcludeFromGit={setExcludeFromGit}
      previewPending={previewMutation.isPending}
      previewLabel={`${toolLabel(tool)} MCP 同步预览`}
      onPreview={() => previewMutation.mutate()}
    >
      {optionsQuery.data?.map((option) => (
        <label
          key={option.mcpId}
          className="flex items-center justify-between gap-3 rounded-lg border p-3 text-sm"
        >
          <span>
            {option.name} · {selectionLabel(option.state)}
            {!option.enabled ? " · 已停用" : ""}
          </span>
          <input
            aria-label={`${option.name} MCP 项目追加`}
            type="checkbox"
            checked={option.state !== "available"}
            disabled={
              option.state === "inherited" ||
              (option.state === "available" && !option.selectable) ||
              assignmentMutation.isPending
            }
            onChange={(event) =>
              assignmentMutation.mutate({
                option,
                assigned: event.target.checked,
              })
            }
          />
        </label>
      ))}
    </AssignmentCard>
  );
}

function ProjectSkillAssignments({
  project,
  tool,
  onPreview,
  onMessage,
}: {
  project: ProjectDto;
  tool: Tool;
  onPreview: (preview: PreviewPlan) => void;
  onMessage: (message: string) => void;
}) {
  const queryClient = useQueryClient();
  const optionsQuery = useQuery(
    skillProjectOptionsQueryOptions(project.id, tool),
  );
  const [excludeFromGit, setExcludeFromGit] = useState(false);
  const assignmentMutation = useMutation({
    mutationFn: async ({
      option,
      assigned,
    }: {
      option: SkillProjectOptionDto;
      assigned: boolean;
    }) =>
      unwrapResult(
        await commands.setProjectSkillAssignment({
          projectId: project.id,
          tool,
          skillId: option.skillId,
          assigned,
          skillRowVersion: option.rowVersion,
          projectRowVersion: project.rowVersion,
        }),
      ),
    onSuccess: async () => {
      onMessage("Skill 项目追加意图已更新；项目链接尚未写入。");
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: projectKeys.all }),
        queryClient.invalidateQueries({ queryKey: mcpKeys.all }),
        queryClient.invalidateQueries({ queryKey: skillKeys.all }),
      ]);
    },
  });
  const previewMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(
        await commands.previewSkillSync({
          tool,
          projectId: project.id,
          excludeFromGit,
        }),
      ),
    onSuccess: (preview) => {
      if (preview.targets.length === 0) {
        onMessage("该项目只有全局继承 Skills，不需要创建项目链接目录。");
      } else {
        onPreview(preview);
      }
    },
  });
  const blocked = projectBlocked(project, tool);
  return (
    <AssignmentCard
      title="Skills"
      description="项目项始终是指向中央库的符号链接；全局项不可在项目中禁用。"
      blocked={blocked}
      error={profileErrorText(
        optionsQuery.error ?? assignmentMutation.error ?? previewMutation.error,
      )}
      pending={optionsQuery.isPending}
      empty={optionsQuery.data?.length === 0}
      excludeFromGit={excludeFromGit}
      onExcludeFromGit={setExcludeFromGit}
      previewPending={previewMutation.isPending}
      previewLabel={`${toolLabel(tool)} Skills 同步预览`}
      onPreview={() => previewMutation.mutate()}
    >
      {optionsQuery.data?.map((option) => (
        <label
          key={option.skillId}
          className="flex items-center justify-between gap-3 rounded-lg border p-3 text-sm"
        >
          <span>
            {option.name} · {selectionLabel(option.state)}
            {option.status !== "ready" ? ` · ${option.status}` : ""}
          </span>
          <input
            aria-label={`${option.name} Skill 项目追加`}
            type="checkbox"
            checked={option.state !== "available"}
            disabled={
              option.state === "inherited" ||
              (option.state === "available" && !option.selectable) ||
              assignmentMutation.isPending
            }
            onChange={(event) =>
              assignmentMutation.mutate({
                option,
                assigned: event.target.checked,
              })
            }
          />
        </label>
      ))}
    </AssignmentCard>
  );
}

function AssignmentCard({
  title,
  description,
  blocked,
  error,
  pending,
  empty,
  excludeFromGit,
  onExcludeFromGit,
  previewPending,
  previewLabel,
  onPreview,
  children,
}: {
  title: string;
  description: string;
  blocked: string | null;
  error: string | null;
  pending: boolean;
  empty: boolean;
  excludeFromGit: boolean;
  onExcludeFromGit: (value: boolean) => void;
  previewPending: boolean;
  previewLabel: string;
  onPreview: () => void;
  children: React.ReactNode;
}) {
  return (
    <article className="rounded-xl border bg-white p-5">
      <h3 className="font-semibold">{title}</h3>
      <p className="text-muted-foreground mt-1 text-sm leading-6">
        {description}
      </p>
      {pending ? (
        <p role="status" className="mt-3 text-sm">
          正在读取选择项…
        </p>
      ) : null}
      {error ? (
        <div className="mt-3">
          <BlockingState title="选择器不可用" description={error} />
        </div>
      ) : null}
      {blocked ? (
        <div className="mt-3">
          <BlockingState title="项目目标受阻" description={blocked} />
        </div>
      ) : null}
      {empty ? (
        <p className="text-muted-foreground mt-3 text-sm">
          中央库暂无可追加项。
        </p>
      ) : null}
      <div className="mt-3 space-y-2">{children}</div>
      <label className="mt-4 flex items-start gap-2 text-sm">
        <input
          type="checkbox"
          checked={excludeFromGit}
          onChange={(event) => onExcludeFromGit(event.target.checked)}
        />
        <span>
          若目标是应用新建且未跟踪，在预览确认后写入本机 .git/info/exclude
        </span>
      </label>
      <Button
        className="mt-4"
        variant="outline"
        aria-label={previewLabel}
        disabled={Boolean(blocked) || previewPending}
        onClick={onPreview}
      >
        {previewPending ? "正在生成…" : `预览项目 ${title} 同步`}
      </Button>
    </article>
  );
}

function projectBlocked(project: ProjectDto, tool: Tool): string | null {
  if (project.pathStatus !== "valid") {
    return "项目根路径无效，必须先重新扫描。";
  }
  if (tool === "codex" && project.codexTrustStatus !== "trusted") {
    return "Codex 项目尚未受信任；应用不会声称项目配置已生效。";
  }
  if (tool === "claude" && project.claudePolicyStatus !== "allowed") {
    return "Claude 管理策略尚未证明允许项目自定义。";
  }
  return null;
}

function selectionLabel(state: "inherited" | "selected" | "available") {
  switch (state) {
    case "inherited":
      return "全局继承（只读）";
    case "selected":
      return "项目追加";
    case "available":
      return "可追加";
  }
}

function toolLabel(tool: Tool) {
  return tool === "claude" ? "Claude" : "Codex";
}

function artifactLabel(kind: ArtifactKind) {
  switch (kind) {
    case "provider":
      return "Provider";
    case "prompt":
      return "提示词";
    case "mcp":
      return "MCP";
    case "skill":
      return "Skills";
  }
}
