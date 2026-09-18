import { useLayoutEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams } from "react-router-dom";
import {
  commands,
  type ArtifactKind,
  type PreviewPlan,
  type ProjectPathStatus,
  type Tool,
} from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { PageHeader } from "@/components/page-header";
import { ToolIconToggle } from "@/components/tool-icon-toggle";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { ExternalChangeActions } from "@/features/sync/external-change-actions";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { useNotify } from "@/components/use-notify";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import {
  presentPreviewCode,
  presentTargetDiagnostic,
} from "@/lib/diagnostic-presentations";
import {
  invalidateProjectScope,
  projectQueryOptions,
} from "@/lib/projects-api";
import { interruptedRunQueryOptions } from "@/lib/sync-api";
import {
  PROJECT_AGENT_TOOLS,
  MCP_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";
import { cn } from "@/lib/utils";
import { ProjectNativeResources } from "./native-resources";
import type { ProjectResourceView } from "./resource-types";
import { ProjectMcpAssignments } from "./assignments/mcp";
import { ProjectHookAssignments } from "./assignments/hook";
import { ProjectSkillAssignments } from "./assignments/skill";
import { ProjectAgentAssignments } from "./assignments/agent";

const PROJECT_PATH_DIAGNOSTIC_CODES: Record<
  Exclude<ProjectPathStatus, "valid">,
  string
> = {
  missing: "PROJECT_ROOT_MISSING",
  permission_denied: "PROJECT_ROOT_PERMISSION_DENIED",
  invalid: "PROJECT_ROOT_INVALID",
};

function projectPathDescription(status: Exclude<ProjectPathStatus, "valid">) {
  const presentation = presentTargetDiagnostic(
    "failed",
    PROJECT_PATH_DIAGNOSTIC_CODES[status],
    { artifactKind: "mcp" },
  );
  return `${presentation.description} ${presentation.nextStep}`;
}

const PROJECT_RESOURCE_VIEWS = [
  {
    id: "mcp",
    label: "MCP",
    ariaLabel: "管理项目 MCP",
    capability: "mcp",
  },
  {
    id: "hook",
    label: "Hooks",
    ariaLabel: "管理项目 Hook",
    capability: "hooks",
  },
  {
    id: "skill",
    label: "Skill",
    ariaLabel: "管理项目 Skill",
    capability: "skills",
  },
  {
    id: "agent",
    label: "Agents",
    ariaLabel: "管理项目 Agents",
    capability: "projectAgents",
  },
] as const satisfies readonly {
  id: ProjectResourceView;
  label: string;
  ariaLabel: string;
  capability: "mcp" | "hooks" | "skills" | "projectAgents";
}[];

export function ProjectDetailPage() {
  const { projectId = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const projectQuery = useQuery(projectQueryOptions(projectId));
  const interruptedQuery = useQuery(interruptedRunQueryOptions());
  const writerBlocked = interruptedQuery.data != null;
  const [resourceView, setResourceView] = useState<ProjectResourceView>("mcp");
  const [toolView, setToolView] = useState<Tool>("claude");
  const enabledTools = useEnabledTools();
  const visibleTools = filterEnabledTools(MCP_TOOLS, enabledTools);
  // 选中工具被关闭时在 render 期夹逼到第一个启用工具；toolView 本身保持，
  // 重新启用后恢复原选中态。
  const activeTool = visibleTools.includes(toolView)
    ? toolView
    : (visibleTools[0] ?? toolView);
  const visibleResourceViews = PROJECT_RESOURCE_VIEWS.filter(
    (view) => toolMetadata(activeTool).capabilities[view.capability],
  );
  // 与启用工具回落一致，资源视图也在 render 期夹逼。这样切换到不支持 Hooks
  // 的工具时不会短暂挂载 Hook 子树并触发不受支持的查询。
  const activeResourceView = visibleResourceViews.some(
    (view) => view.id === resourceView,
  )
    ? resourceView
    : (visibleResourceViews[0]?.id ?? resourceView);
  const visibleProjectTools =
    activeResourceView === "agent"
      ? filterEnabledTools(PROJECT_AGENT_TOOLS, enabledTools)
      : visibleTools;
  const [toolStatusOpen, setToolStatusOpen] = useState(false);
  const viewKey = projectViewKey(projectId, activeTool, activeResourceView);
  // 原生资源 Preview 可能在 keyed 子树卸载后才返回；用 ref 保持当前视图，
  // 防止晚到结果为过期的工具/资源组合执行动作。
  const currentViewKeyRef = useRef(viewKey);
  useLayoutEffect(() => {
    currentViewKeyRef.current = viewKey;
  }, [viewKey]);
  const { notify, clear } = useNotify();
  const applyMutation = useMutation({
    mutationFn: async ({ plan }: { plan: PreviewPlan; viewKey: string }) => {
      return unwrapResult(
        await commands.applyProjectNativeResourcePreview({
          previewId: plan.previewId,
        }),
      );
    },
    onSuccess: async (
      _result,
      { viewKey: requestViewKey }: { plan: PreviewPlan; viewKey: string },
    ) => {
      await invalidateProjectScope(queryClient, [
        "project",
        "mcp",
        "skill",
        "hook",
        "agent",
      ]);
      if (currentViewKeyRef.current !== requestViewKey) return;
      notify({
        kind: "success",
        message: "项目原生配置已通过持久化预览应用并完成写后验证。",
      });
    },
  });
  const changeResourceView = (nextView: ProjectResourceView) => {
    if (nextView === resourceView) return;
    setResourceView(nextView);
    applyMutation.reset();
    clear();
  };

  // 原生资源动作本身就是用户授权边界。Preview 仍提供 hash、row version
  // 和恢复快照证据，但成功生成后立即由唯一的 Apply 内核消费。
  const handleNativePreview = (
    plan: PreviewPlan,
    planTool: Tool,
    planArtifactKind: ProjectResourceView,
  ) => {
    if (
      projectViewKey(projectId, planTool, planArtifactKind) !==
      currentViewKeyRef.current
    ) {
      return;
    }
    if (plan.targets.length === 0) {
      notify({ kind: "success", message: "当前项目原生资源无需变更。" });
      return;
    }
    if (
      plan.targets.some(
        (target) =>
          target.changeKind === "conflict" || target.errorCode !== null,
      )
    ) {
      notify({
        kind: "error",
        message: "当前项目原生资源无法安全同步，请重试。",
      });
      return;
    }
    applyMutation.mutate({
      plan,
      viewKey: currentViewKeyRef.current,
    });
  };
  const changeToolView = (nextTool: Tool) => {
    if (nextTool === toolView) return;
    setToolView(nextTool);
    applyMutation.reset();
    clear();
  };

  if (projectQuery.isPending) {
    return (
      <>
        <PageHeader title="项目详情" />
        <main className="px-8 py-6">
          <p role="status">正在读取项目详情…</p>
        </main>
      </>
    );
  }
  if (projectQuery.isError || !projectQuery.data) {
    return (
      <>
        <PageHeader title="项目详情" />
        <main className="px-8 py-6">
          <BlockingState
            title="项目详情不可用"
            description={
              profileErrorText(projectQuery.error) ?? "项目不存在或已移除。"
            }
            actionLabel="返回项目列表"
            onAction={() => void navigate("/projects")}
          />
        </main>
      </>
    );
  }
  const project = projectQuery.data;

  return (
    <>
      <PageHeader
        title={project.displayName}
        backTo="/projects"
        meta={
          <div className="text-muted-foreground mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
            <code className="break-all">{project.rootPath}</code>
            <span>Git：{project.gitStatus}</span>
            <span>Codex trust：{project.codexTrustStatus}</span>
            <span>Claude policy：{project.claudePolicyStatus}</span>
          </div>
        }
      />
      <main className="space-y-6 px-8 py-6">
        <div className="space-y-3" aria-live="polite">
          {project.pathStatus !== "valid" ? (
            <BlockingState
              title="项目目录不可用"
              description={projectPathDescription(project.pathStatus)}
            />
          ) : null}
          {applyMutation.isError &&
          applyMutation.variables?.viewKey === viewKey ? (
            <BlockingState
              title="应用项目预览失败"
              description={profileErrorText(applyMutation.error) ?? "应用失败"}
            />
          ) : null}
        </div>

        {project.targets.some(
          (target) =>
            enabledTools.has(target.tool) &&
            isProjectResourceKind(target.artifactKind),
        ) ? (
          <section
            className="bg-card rounded-lg border p-5"
            aria-labelledby="project-status-title"
          >
            <div className="flex items-center justify-between gap-3">
              <h2
                id="project-status-title"
                className="text-[15px] font-semibold"
              >
                工具配置状态
              </h2>
              <button
                type="button"
                aria-controls="project-status-content"
                aria-expanded={toolStatusOpen}
                aria-label={
                  toolStatusOpen ? "收起工具配置状态" : "展开工具配置状态"
                }
                title={toolStatusOpen ? "收起工具配置状态" : "展开工具配置状态"}
                className="text-muted-foreground hover:bg-muted hover:text-foreground rounded-control flex size-7 shrink-0 items-center justify-center transition-colors"
                onClick={() => setToolStatusOpen((open) => !open)}
              >
                <svg
                  aria-hidden="true"
                  viewBox="0 0 16 16"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  className={cn(
                    "size-4 transition-transform",
                    toolStatusOpen && "rotate-90",
                  )}
                >
                  <path d="M6 3.5 10.5 8 6 12.5" />
                </svg>
              </button>
            </div>
            {toolStatusOpen ? (
              <div
                id="project-status-content"
                className="mt-4 grid gap-3 md:grid-cols-2"
              >
                {project.targets
                  .filter(
                    (target) =>
                      enabledTools.has(target.tool) &&
                      isProjectResourceKind(target.artifactKind),
                  )
                  .map((target) => {
                    if (!isProjectResourceKind(target.artifactKind))
                      return null;
                    const targetArtifactKind = target.artifactKind;
                    const initialUnmanaged =
                      target.diagnosticCode ===
                      "PROJECT_TARGET_INITIAL_UNMANAGED";
                    const diagnosticPresentation = target.diagnosticCode
                      ? presentTargetDiagnostic(
                          target.status,
                          target.diagnosticCode,
                          {
                            tool: target.tool,
                            artifactKind: target.artifactKind,
                          },
                        )
                      : null;
                    return (
                      <article
                        key={`${target.tool}-${target.artifactKind}`}
                        className="rounded-lg border p-4"
                      >
                        <div className="flex items-center justify-between gap-3">
                          <p className="font-medium">
                            {toolLabel(target.tool)} ·{" "}
                            {artifactLabel(targetArtifactKind)}
                          </p>
                          {initialUnmanaged ? (
                            <SyncStatusBadge
                              status={target.status}
                              label="未纳管"
                              tone="muted"
                            />
                          ) : (
                            <SyncStatusBadge status={target.status} />
                          )}
                        </div>
                        <code className="mt-2 block text-xs break-all">
                          {target.targetPath ?? "目标路径不可用"}
                        </code>
                        {initialUnmanaged ? (
                          <p className="text-muted-foreground mt-2 text-xs">
                            该目标由外部维护，本项目暂无需要写入的项目级配置；全局配置持续继承。
                          </p>
                        ) : diagnosticPresentation ? (
                          <p className="text-muted-foreground mt-2 text-xs">
                            {diagnosticPresentation.description}{" "}
                            {diagnosticPresentation.nextStep}
                          </p>
                        ) : null}
                        <ExternalChangeActions
                          artifactKind={targetArtifactKind}
                          tool={target.tool}
                          projectId={project.id}
                          status={target.status}
                          onInvalidate={async () => {
                            await invalidateProjectScope(queryClient, [
                              targetArtifactKind,
                            ]);
                          }}
                          onMatchOrImport={() => {
                            setResourceView(targetArtifactKind);
                            setToolView(target.tool);
                          }}
                        />
                      </article>
                    );
                  })}
              </div>
            ) : null}
          </section>
        ) : null}

        <section
          className="bg-card rounded-lg border p-5"
          aria-labelledby="project-resource-management-title"
        >
          <div className="flex flex-wrap items-center justify-between gap-4">
            <h2
              id="project-resource-management-title"
              className="text-[15px] font-semibold"
            >
              项目资源管理
            </h2>
            <div className="flex flex-wrap items-center gap-3">
              <div
                className="flex items-center gap-2"
                role="group"
                aria-label="项目资源管理视图"
              >
                {visibleResourceViews.map((view) => (
                  <Button
                    key={view.id}
                    type="button"
                    size="sm"
                    variant={
                      activeResourceView === view.id ? "default" : "outline"
                    }
                    aria-label={view.ariaLabel}
                    aria-pressed={activeResourceView === view.id}
                    onClick={() => changeResourceView(view.id)}
                  >
                    {view.label}
                  </Button>
                ))}
              </div>
              <div
                className="flex items-center gap-2"
                role="group"
                aria-label="项目平台管理视图"
              >
                {visibleProjectTools.map((tool) => (
                  <ProjectToolViewButton
                    key={tool}
                    tool={tool}
                    selected={activeTool === tool}
                    onClick={() => changeToolView(tool)}
                  />
                ))}
              </div>
            </div>
          </div>
        </section>

        <div>
          <section key={viewKey} className="space-y-5">
            {writerBlocked ? (
              <BlockingState
                title="存在未完成的写入或回滚失败"
                description={`${
                  presentPreviewCode(
                    interruptedQuery.data?.status === "rollback_failed"
                      ? "ROLLBACK_FAILED"
                      : "WRITE_IN_PROGRESS",
                    "error",
                  ).description
                } ${
                  presentPreviewCode(
                    interruptedQuery.data?.status === "rollback_failed"
                      ? "ROLLBACK_FAILED"
                      : "WRITE_IN_PROGRESS",
                    "error",
                  ).nextStep
                }`}
              />
            ) : null}
            <ProjectNativeResources
              project={project}
              tool={activeTool}
              artifactKind={activeResourceView}
              writerBlocked={writerBlocked}
              applyPending={applyMutation.isPending}
              onPreview={handleNativePreview}
            />
            <h2 className="text-[15px] font-semibold">
              {toolLabel(activeTool)}{" "}
              {activeResourceView === "mcp"
                ? "MCP"
                : activeResourceView === "hook"
                  ? "Hook"
                  : activeResourceView === "skill"
                    ? "Skill"
                    : "Agents"}{" "}
              项目追加
            </h2>
            {activeResourceView === "mcp" ? (
              <ProjectMcpAssignments project={project} tool={activeTool} />
            ) : activeResourceView === "hook" ? (
              <ProjectHookAssignments
                project={project}
                tool={activeTool}
                onMessage={(message) => notify({ kind: "success", message })}
              />
            ) : activeResourceView === "skill" ? (
              <ProjectSkillAssignments project={project} tool={activeTool} />
            ) : (
              <ProjectAgentAssignments project={project} tool={activeTool} />
            )}
          </section>
        </div>
      </main>
    </>
  );
}

function projectViewKey(
  projectId: string,
  tool: Tool,
  resourceView: ProjectResourceView,
) {
  return `${projectId}:${tool}:${resourceView}`;
}

function isProjectResourceKind(
  artifactKind: ArtifactKind,
): artifactKind is ProjectResourceView {
  return (
    artifactKind === "mcp" ||
    artifactKind === "hook" ||
    artifactKind === "skill" ||
    artifactKind === "agent"
  );
}

interface ProjectToolViewButtonProps {
  tool: Tool;
  selected: boolean;
  onClick: () => void;
}

function ProjectToolViewButton({
  tool,
  selected,
  onClick,
}: ProjectToolViewButtonProps) {
  return (
    <ToolIconToggle
      tool={tool}
      active={selected}
      label={`管理 ${toolMetadata(tool).label} 项目资源`}
      onClick={onClick}
    />
  );
}

function toolLabel(tool: Tool) {
  return toolMetadata(tool).label;
}

// 用 Record 穷举 ArtifactKind：后端新增资源种类时这里会直接编译失败，
// 而不是像 switch 那样静默返回 undefined 渲染出空标签。
const ARTIFACT_LABELS: Record<ArtifactKind, string> = {
  provider: "Provider",
  prompt: "提示词",
  mcp: "MCP",
  skill: "Skills",
  hook: "Hooks",
  agent: "Agents",
};

function artifactLabel(kind: ArtifactKind) {
  return ARTIFACT_LABELS[kind];
}
