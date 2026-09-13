import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams } from "react-router-dom";
import {
  commands,
  type ArtifactKind,
  type PreviewPlan,
  type Tool,
} from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import { PageHeader } from "@/components/page-header";
import { ToolIconToggle } from "@/components/tool-icon-toggle";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { useNotify } from "@/components/use-notify";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
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
import { appSettingsQueryOptions } from "@/lib/settings-api";
import { cn } from "@/lib/utils";
import { ProjectNativeResources } from "./native-resources";
import type { ProjectResourceView } from "./resource-types";
import { ProjectMcpAssignments } from "./assignments/mcp";
import { ProjectHookAssignments } from "./assignments/hook";
import { ProjectSkillAssignments } from "./assignments/skill";
import { ProjectAgentAssignments } from "./assignments/agent";

interface OpenProjectPreview {
  plan: PreviewPlan;
  tool: Tool;
  artifactKind: ProjectResourceView;
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
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const interruptedQuery = useQuery(interruptedRunQueryOptions());
  const writerBlocked = interruptedQuery.data != null;
  const directApply = settingsQuery.data?.applyMode === "direct";
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
  const [openPreview, setOpenPreview] = useState<OpenProjectPreview | null>(
    null,
  );
  const { notify, clear } = useNotify();
  const applyMutation = useMutation({
    mutationFn: async (preview: OpenProjectPreview) => {
      return unwrapResult(
        await commands.applyProjectNativeResourcePreview({
          previewId: preview.plan.previewId,
        }),
      );
    },
    onSuccess: async () => {
      setOpenPreview(null);
      await invalidateProjectScope(queryClient, [
        "project",
        "mcp",
        "skill",
        "hook",
        "agent",
      ]);
      notify({
        kind: "success",
        message: "项目原生配置已通过持久化预览应用并完成写后验证。",
      });
    },
  });
  const changeResourceView = (nextView: ProjectResourceView) => {
    if (nextView === resourceView) return;
    setResourceView(nextView);
    setOpenPreview(null);
    applyMutation.reset();
    clear();
  };

  // 直接应用模式下仍先生成持久化预览；只有与预览对话框 Apply 可用条件一致
  // 的无冲突预览才跳过确认，冲突或错误一律回退到人工确认。
  const handleNativePreview = (
    plan: PreviewPlan,
    tool: Tool,
    artifactKind: ProjectResourceView,
  ) => {
    setOpenPreview({ plan, tool, artifactKind });
  };
  const changeToolView = (nextTool: Tool) => {
    if (nextTool === toolView) return;
    setToolView(nextTool);
    setOpenPreview(null);
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
              description="请重新扫描确认路径。"
              code={project.pathStatus}
            />
          ) : null}
          {applyMutation.isError ? (
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
                    const initialUnmanaged =
                      target.diagnosticCode ===
                      "PROJECT_TARGET_INITIAL_UNMANAGED";
                    return (
                      <article
                        key={`${target.tool}-${target.artifactKind}`}
                        className="rounded-lg border p-4"
                      >
                        <div className="flex items-center justify-between gap-3">
                          <p className="font-medium">
                            {toolLabel(target.tool)} ·{" "}
                            {artifactLabel(target.artifactKind)}
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
                        ) : target.diagnosticCode ? (
                          <p className="mt-2 text-xs">
                            诊断：{target.diagnosticCode}
                          </p>
                        ) : null}
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
                description="有同步正在进行或回滚失败，请先在恢复点中处理。"
                code={interruptedQuery.data?.status ?? "WRITE_IN_PROGRESS"}
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
              <ProjectMcpAssignments
                project={project}
                tool={activeTool}
                directApply={directApply}
                onMessage={(message) => notify({ kind: "success", message })}
              />
            ) : activeResourceView === "hook" ? (
              <ProjectHookAssignments
                project={project}
                tool={activeTool}
                directApply={directApply}
                onMessage={(message) => notify({ kind: "success", message })}
              />
            ) : activeResourceView === "skill" ? (
              <ProjectSkillAssignments
                project={project}
                tool={activeTool}
                directApply={directApply}
                onMessage={(message) => notify({ kind: "success", message })}
              />
            ) : (
              <ProjectAgentAssignments
                project={project}
                tool={activeTool}
                directApply={directApply}
                onMessage={(message) => notify({ kind: "success", message })}
              />
            )}
          </section>
        </div>

        <ChangePreviewDialog
          preview={openPreview?.plan ?? null}
          tool={openPreview?.tool ?? "claude"}
          artifactKind={openPreview?.artifactKind ?? "mcp"}
          applying={applyMutation.isPending}
          onClose={() => {
            if (applyMutation.isPending) return;
            setOpenPreview(null);
          }}
          onApply={() => {
            if (!openPreview) return;
            applyMutation.mutate(openPreview);
          }}
        />
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
