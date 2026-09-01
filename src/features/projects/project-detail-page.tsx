import { useEffect, useRef, useState } from "react";
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
import {
  profileErrorText,
  profileKeys,
  promptProfilesQueryOptions,
  promptProjectAssignmentQueryOptions,
  unwrapResult,
} from "@/lib/profile-api";
import { projectKeys, projectQueryOptions } from "@/lib/projects-api";
import { skillKeys, skillProjectOptionsQueryOptions } from "@/lib/skills-api";
import { MCP_TOOLS, toolMetadata } from "@/lib/tool-metadata";
import {
  appSettingsQueryOptions,
  canAutoApplyPreview,
} from "@/lib/settings-api";
import { cn } from "@/lib/utils";

interface OpenProjectPreview {
  plan: PreviewPlan;
  tool: Tool;
  artifactKind: ArtifactKind;
}

type ProjectResourceView = "mcp" | "skill" | "prompt";

export function ProjectDetailPage() {
  const { projectId = "" } = useParams();
  const queryClient = useQueryClient();
  const projectQuery = useQuery(projectQueryOptions(projectId));
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const directApply = settingsQuery.data?.applyMode === "direct";
  const [resourceView, setResourceView] = useState<ProjectResourceView>("mcp");
  const [toolView, setToolView] = useState<Tool>("claude");
  const viewKey = projectViewKey(projectId, toolView, resourceView);
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
      if (preview.artifactKind === "prompt") {
        return unwrapResult(
          await commands.applyProfilePreview({
            previewId: preview.plan.previewId,
            tool: preview.tool,
            artifactKind: "prompt",
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
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: projectKeys.all }),
        queryClient.invalidateQueries({ queryKey: mcpKeys.all }),
        queryClient.invalidateQueries({ queryKey: skillKeys.all }),
        queryClient.invalidateQueries({ queryKey: profileKeys.all }),
      ]);
    },
  });

  const readoptMutation = useMutation({
    mutationFn: async (preview: OpenProjectPreview) =>
      unwrapResult(
        await commands.readoptMcpTarget({
          tool: preview.tool,
          projectId,
        }),
      ),
    onSuccess: async (result) => {
      setOpenPreview(null);
      setMessage(
        `已以当前内容重新接管（刷新 ${result.updatedItemCount} 个、清理 ${result.removedItemCount} 个条目基线）；请再次点击同步按钮完成写入。`,
      );
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: projectKeys.all }),
        queryClient.invalidateQueries({ queryKey: mcpKeys.all }),
        queryClient.invalidateQueries({ queryKey: skillKeys.all }),
      ]);
    },
  });

  const changeResourceView = (nextView: ProjectResourceView) => {
    if (nextView === resourceView) return;
    setResourceView(nextView);
    setOpenPreview(null);
    setMessage(null);
    applyMutation.reset();
  };

  // 直接应用模式下仍先生成持久化预览；只有与预览对话框 Apply 可用条件一致
  // 的无冲突预览才跳过确认，冲突或错误一律回退到人工确认。
  const handlePreview = (
    plan: PreviewPlan,
    tool: Tool,
    artifactKind: ArtifactKind,
  ) => {
    if (directApply && canAutoApplyPreview(plan)) {
      applyMutation.mutate(
        { plan, tool, artifactKind },
        {
          onSuccess: () => {
            setMessage("项目原生配置已通过持久化预览应用并完成写后验证。");
          },
        },
      );
      return;
    }
    setOpenPreview({ plan, tool, artifactKind });
  };
  const changeToolView = (nextTool: Tool) => {
    if (nextTool === toolView) return;
    setToolView(nextTool);
    if (
      resourceView === "prompt" &&
      !toolMetadata(nextTool).capabilities.promptProject
    ) {
      setResourceView("mcp");
    }
    setOpenPreview(null);
    setMessage(null);
    applyMutation.reset();
  };

  if (projectQuery.isPending) {
    return (
      <main className="p-6 lg:p-8">
        <p role="status">正在读取项目详情…</p>
      </main>
    );
  }
  if (projectQuery.isError || !projectQuery.data) {
    return (
      <main className="p-6 lg:p-8">
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
    <main className="p-6 lg:p-8">
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
        {message ? (
          <p className="text-sm text-emerald-800 dark:text-emerald-300">
            {message}
          </p>
        ) : null}
        {applyMutation.isError ? (
          <BlockingState
            title="应用项目预览失败"
            description={profileErrorText(applyMutation.error) ?? "应用失败"}
          />
        ) : null}
        {readoptMutation.isError ? (
          <BlockingState
            title="重新接管失败"
            description={profileErrorText(readoptMutation.error) ?? "接管失败"}
          />
        ) : null}
      </div>

      <section
        className="bg-card mx-auto mt-6 max-w-6xl rounded-xl border p-5"
        aria-labelledby="project-status-title"
      >
        <h2 id="project-status-title" className="text-lg font-semibold">
          工具配置状态
        </h2>
        <div className="mt-4 grid gap-3 md:grid-cols-2">
          {project.targets.map((target) => {
            const initialUnmanaged =
              target.diagnosticCode === "PROJECT_TARGET_INITIAL_UNMANAGED";
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
                      label="○ 未纳管"
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
                  <p className="mt-2 text-xs">诊断：{target.diagnosticCode}</p>
                ) : null}
              </article>
            );
          })}
        </div>
      </section>

      <section
        className="bg-card mx-auto mt-6 max-w-6xl rounded-xl border p-5"
        aria-labelledby="project-resource-management-title"
      >
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <h2
              id="project-resource-management-title"
              className="text-lg font-semibold"
            >
              项目资源管理
            </h2>
            <p className="text-muted-foreground mt-1 text-sm">
              分别选择资源类型与目标平台，当前只展示一个管理组合。
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <div
              className="flex items-center gap-2"
              role="group"
              aria-label="项目资源管理视图"
            >
              <Button
                type="button"
                size="sm"
                variant={resourceView === "mcp" ? "default" : "outline"}
                aria-label="管理项目 MCP"
                aria-pressed={resourceView === "mcp"}
                onClick={() => changeResourceView("mcp")}
              >
                MCP
              </Button>
              <Button
                type="button"
                size="sm"
                variant={resourceView === "skill" ? "default" : "outline"}
                aria-label="管理项目 Skill"
                aria-pressed={resourceView === "skill"}
                onClick={() => changeResourceView("skill")}
              >
                Skill
              </Button>
              {toolMetadata(toolView).capabilities.promptProject ? (
                <Button
                  type="button"
                  size="sm"
                  variant={resourceView === "prompt" ? "default" : "outline"}
                  aria-label="管理项目提示词"
                  aria-pressed={resourceView === "prompt"}
                  onClick={() => changeResourceView("prompt")}
                >
                  提示词
                </Button>
              ) : null}
            </div>
            <div
              className="flex items-center gap-2"
              role="group"
              aria-label="项目平台管理视图"
            >
              {MCP_TOOLS.map((tool) => (
                <ProjectToolViewButton
                  key={tool}
                  tool={tool}
                  selected={toolView === tool}
                  onClick={() => changeToolView(tool)}
                />
              ))}
            </div>
          </div>
        </div>
      </section>

      <div className="mx-auto mt-6 max-w-6xl">
        <section key={viewKey} className="space-y-5">
          <h2 className="text-xl font-semibold">
            {toolLabel(toolView)}{" "}
            {resourceView === "mcp"
              ? "MCP"
              : resourceView === "skill"
                ? "Skill"
                : "提示词"}{" "}
            项目追加
          </h2>
          {resourceView === "mcp" ? (
            <ProjectMcpAssignments
              project={project}
              tool={toolView}
              directApply={directApply}
              onPreview={(plan) => handlePreview(plan, toolView, "mcp")}
              onMessage={setMessage}
            />
          ) : resourceView === "skill" ? (
            <ProjectSkillAssignments
              project={project}
              tool={toolView}
              directApply={directApply}
              onPreview={(plan) => handlePreview(plan, toolView, "skill")}
              onMessage={setMessage}
            />
          ) : toolMetadata(toolView).capabilities.promptProject ? (
            <ProjectPromptAssignments
              project={project}
              tool={toolView}
              directApply={directApply}
              onPreview={(plan) => handlePreview(plan, toolView, "prompt")}
              onMessage={setMessage}
            />
          ) : (
            <BlockingState
              title={`${toolLabel(toolView)} 项目提示词不受支持`}
              description={`${toolLabel(toolView)} 项目视图只支持 MCP 与 Skills，不会读取或写入 Rules、Prompt 或 AGENTS.md。`}
              code="CURSOR_PROMPT_UNSUPPORTED"
            />
          )}
        </section>
      </div>

      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? "claude"}
        artifactKind={openPreview?.artifactKind ?? "mcp"}
        applying={applyMutation.isPending}
        readopting={readoptMutation.isPending}
        onReadopt={() => {
          if (openPreview) {
            readoptMutation.mutate(openPreview);
          }
        }}
        onClose={() => setOpenPreview(null)}
        onApply={() => {
          if (!openPreview) return;
          applyMutation.mutate(openPreview, {
            onSuccess: () => {
              setOpenPreview(null);
              setMessage("项目原生配置已通过持久化预览应用并完成写后验证。");
            },
          });
        }}
      />
    </main>
  );
}

function projectViewKey(
  projectId: string,
  tool: Tool,
  resourceView: ProjectResourceView,
) {
  return `${projectId}:${tool}:${resourceView}`;
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
  const metadata = toolMetadata(tool);
  const label = `管理 ${metadata.label} 项目资源`;

  return (
    <Button
      type="button"
      size="sm"
      variant="outline"
      className={cn(
        "size-8 p-0 shadow-none",
        selected
          ? "border-slate-300 bg-slate-50 shadow-sm dark:border-slate-600 dark:bg-slate-800"
          : "border-slate-200 bg-transparent dark:border-slate-700",
      )}
      aria-label={label}
      aria-pressed={selected}
      title={label}
      onClick={onClick}
    >
      <img
        src={metadata.icon}
        alt=""
        aria-hidden="true"
        draggable={false}
        className={cn(
          "size-5 object-contain transition-[opacity,filter]",
          selected ? "opacity-100" : "opacity-25 grayscale",
        )}
      />
    </Button>
  );
}

function ProjectMcpAssignments({
  project,
  tool,
  directApply,
  onPreview,
  onMessage,
}: {
  project: ProjectDto;
  tool: Tool;
  directApply: boolean;
  onPreview: (preview: PreviewPlan) => void;
  onMessage: (message: string) => void;
}) {
  const queryClient = useQueryClient();
  const viewActive = useViewActive();
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
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: projectKeys.all }),
        queryClient.invalidateQueries({ queryKey: mcpKeys.all }),
        queryClient.invalidateQueries({ queryKey: skillKeys.all }),
      ]);
      if (!viewActive.current) return;
      if (directApply) {
        previewMutation.mutate();
        return;
      }
      onMessage("MCP 项目追加意图已更新；原生配置尚未写入。");
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
      if (!viewActive.current) return;
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
      directApply={directApply}
      error={profileErrorText(
        optionsQuery.error ?? assignmentMutation.error ?? previewMutation.error,
      )}
      pending={optionsQuery.isPending}
      empty={optionsQuery.data?.length === 0}
      excludeFromGit={excludeFromGit}
      onExcludeFromGit={setExcludeFromGit}
      previewPending={previewMutation.isPending}
      previewLabel={
        directApply
          ? `${toolLabel(tool)} MCP 直接应用`
          : `${toolLabel(tool)} MCP 同步预览`
      }
      onPreview={() => previewMutation.mutate()}
    >
      {optionsQuery.data?.map((option) => (
        <ProjectOptionRow
          key={option.mcpId}
          name={option.name}
          state={option.state}
          actionLabel={`${option.name} MCP 项目追加`}
          actionDisabled={
            option.state === "inherited" ||
            (option.state === "available" && !option.selectable) ||
            assignmentMutation.isPending
          }
          onToggle={() =>
            assignmentMutation.mutate({
              option,
              assigned: option.state === "available",
            })
          }
        >
          {!option.enabled ? (
            <OptionTag tone="warning">已停用</OptionTag>
          ) : null}
        </ProjectOptionRow>
      ))}
    </AssignmentCard>
  );
}

function ProjectSkillAssignments({
  project,
  tool,
  directApply,
  onPreview,
  onMessage,
}: {
  project: ProjectDto;
  tool: Tool;
  directApply: boolean;
  onPreview: (preview: PreviewPlan) => void;
  onMessage: (message: string) => void;
}) {
  const queryClient = useQueryClient();
  const viewActive = useViewActive();
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
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: projectKeys.all }),
        queryClient.invalidateQueries({ queryKey: mcpKeys.all }),
        queryClient.invalidateQueries({ queryKey: skillKeys.all }),
      ]);
      if (!viewActive.current) return;
      if (directApply) {
        previewMutation.mutate();
        return;
      }
      onMessage("Skill 项目追加意图已更新；项目链接尚未写入。");
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
      if (!viewActive.current) return;
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
      directApply={directApply}
      error={profileErrorText(
        optionsQuery.error ?? assignmentMutation.error ?? previewMutation.error,
      )}
      pending={optionsQuery.isPending}
      empty={optionsQuery.data?.length === 0}
      excludeFromGit={excludeFromGit}
      onExcludeFromGit={setExcludeFromGit}
      previewPending={previewMutation.isPending}
      previewLabel={
        directApply
          ? `${toolLabel(tool)} Skills 直接应用`
          : `${toolLabel(tool)} Skills 同步预览`
      }
      onPreview={() => previewMutation.mutate()}
    >
      {optionsQuery.data?.map((option) => (
        <ProjectOptionRow
          key={option.skillId}
          name={option.name}
          state={option.state}
          actionLabel={`${option.name} Skill 项目追加`}
          actionDisabled={
            option.state === "inherited" ||
            (option.state === "available" && !option.selectable) ||
            assignmentMutation.isPending
          }
          onToggle={() =>
            assignmentMutation.mutate({
              option,
              assigned: option.state === "available",
            })
          }
        >
          {option.status !== "ready" ? (
            <OptionTag tone="warning">{option.status}</OptionTag>
          ) : null}
        </ProjectOptionRow>
      ))}
    </AssignmentCard>
  );
}

function useViewActive() {
  const active = useRef(true);

  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);

  return active;
}

function ProjectPromptAssignments({
  project,
  tool,
  directApply,
  onPreview,
  onMessage,
}: {
  project: ProjectDto;
  tool: Tool;
  directApply: boolean;
  onPreview: (preview: PreviewPlan) => void;
  onMessage: (message: string) => void;
}) {
  const queryClient = useQueryClient();
  const viewActive = useViewActive();
  const assignmentQuery = useQuery(
    promptProjectAssignmentQueryOptions(project.id, tool),
  );
  const profilesQuery = useQuery(promptProfilesQueryOptions());
  const assignmentMutation = useMutation({
    mutationFn: async (promptProfileId: string | null) =>
      unwrapResult(
        await commands.setPromptProjectAssignment({
          projectId: project.id,
          tool,
          promptProfileId,
          projectRowVersion: project.rowVersion,
        }),
      ),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: projectKeys.all }),
        queryClient.invalidateQueries({ queryKey: profileKeys.all }),
        queryClient.invalidateQueries({ queryKey: mcpKeys.all }),
        queryClient.invalidateQueries({ queryKey: skillKeys.all }),
      ]);
      if (!viewActive.current) return;
      onMessage("项目提示词分配已更新；项目记忆文件尚未写入。");
    },
  });
  const previewMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(await commands.previewPromptSync(tool, project.id)),
    onSuccess: (preview) => {
      if (!viewActive.current) return;
      if (preview.targets.length === 0) {
        onMessage("该项目提示词没有需要应用的变更。");
      } else {
        onPreview(preview);
      }
    },
  });
  const blocked = projectBlocked(project, tool);
  const assignedProfileId = assignmentQuery.data?.profileId ?? null;
  const profiles = profilesQuery.data ?? [];
  // 对该工具全局生效的档案不作为项目分配选项展示；已被本项目分配的档案例外保留，便于解除分配。
  const assignableProfiles = profiles.filter(
    (profile) =>
      !profile.globalTools.includes(tool) || profile.id === assignedProfileId,
  );
  const mutationError = profileErrorText(
    assignmentQuery.error ??
      profilesQuery.error ??
      assignmentMutation.error ??
      previewMutation.error,
  );
  if (!toolMetadata(tool).capabilities.promptProject) {
    return (
      <BlockingState
        title={`${toolLabel(tool)} 项目提示词不受支持`}
        description={`${toolLabel(tool)} 不会读取或写入项目 Rules、Prompt 或 AGENTS.md。`}
        code="CURSOR_PROMPT_UNSUPPORTED"
      />
    );
  }
  const targetFile = tool === "claude" ? "CLAUDE.md" : "AGENTS.md";

  return (
    <article className="bg-card rounded-xl border p-5">
      <h3 className="font-semibold">提示词</h3>
      <p className="text-muted-foreground mt-1 text-sm leading-6">
        分配后会把所选档案硬拷贝为项目根 {targetFile}
        ；此后文件归项目所有、可随时自行修改，重新应用会以档案内容覆盖。解除分配会保留项目文件、仅停止纳管。
      </p>
      {assignmentQuery.isPending || profilesQuery.isPending ? (
        <p role="status" className="mt-3 text-sm">
          正在读取提示词分配…
        </p>
      ) : null}
      {mutationError ? (
        <div className="mt-3">
          <BlockingState title="提示词分配不可用" description={mutationError} />
        </div>
      ) : null}
      {blocked ? (
        <div className="mt-3">
          <BlockingState title="项目目标受阻" description={blocked} />
        </div>
      ) : null}
      {profiles.length === 0 ? (
        <p className="text-muted-foreground mt-3 text-sm">
          {toolLabel(tool)} 还没有全局提示词档案；请先在侧边栏「提示词」页创建。
        </p>
      ) : assignableProfiles.length === 0 ? (
        <p className="text-muted-foreground mt-3 text-sm">
          暂无可分配的提示词档案；全局生效的档案不会分配到项目，请先在「提示词」页新增或切换生效档案。
        </p>
      ) : null}
      <ul className="mt-3 space-y-2">
        {assignableProfiles.map((profile) => {
          const selected = profile.id === assignedProfileId;
          return (
            <li
              key={profile.id}
              className={cn(
                "rounded-lg border p-3",
                selected && "border-primary bg-primary/5",
              )}
            >
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <p className="font-medium">
                    {profile.name}
                    {selected ? (
                      <span className="text-muted-foreground"> · 当前分配</span>
                    ) : null}
                    {profile.globalTools.includes(tool) ? (
                      <span className="text-muted-foreground"> · 全局生效</span>
                    ) : null}
                  </p>
                  <p className="text-muted-foreground mt-1 line-clamp-2 text-xs">
                    {profile.body}
                  </p>
                </div>
                <div className="flex gap-2">
                  {!selected ? (
                    <Button
                      size="sm"
                      aria-label={`分配 ${profile.name} 为项目提示词`}
                      disabled={
                        Boolean(blocked) || assignmentMutation.isPending
                      }
                      onClick={() => assignmentMutation.mutate(profile.id)}
                    >
                      分配到此项目
                    </Button>
                  ) : null}
                </div>
              </div>
            </li>
          );
        })}
      </ul>
      <div className="mt-4 flex flex-wrap gap-2">
        <Button
          variant="outline"
          aria-label={
            directApply
              ? `${toolLabel(tool)} 提示词直接应用`
              : `${toolLabel(tool)} 提示词同步预览`
          }
          disabled={
            Boolean(blocked) ||
            assignedProfileId === null ||
            previewMutation.isPending
          }
          onClick={() => previewMutation.mutate()}
        >
          {previewMutation.isPending
            ? directApply
              ? "正在应用…"
              : "正在生成…"
            : directApply
              ? `直接应用项目 ${toolLabel(tool)} 提示词`
              : `预览项目 ${toolLabel(tool)} 提示词同步`}
        </Button>
        {assignedProfileId !== null ? (
          <Button
            variant="outline"
            aria-label="解除项目提示词分配"
            disabled={assignmentMutation.isPending}
            onClick={() => {
              if (
                globalThis.confirm(
                  `解除分配将保留项目根 ${targetFile} 的当前内容，仅停止纳管。确认解除？`,
                )
              ) {
                assignmentMutation.mutate(null);
              }
            }}
          >
            解除分配
          </Button>
        ) : null}
      </div>
    </article>
  );
}

function AssignmentCard({
  title,
  description,
  blocked,
  directApply,
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
  directApply: boolean;
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
  const actionLabel = directApply
    ? `直接应用项目 ${title} 同步`
    : `预览项目 ${title} 同步`;

  return (
    <article className="bg-card rounded-xl border p-5">
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
          若目标是应用新建且未跟踪，在应用时写入本机 .git/info/exclude
        </span>
      </label>
      <Button
        className="mt-4"
        variant="outline"
        aria-label={previewLabel}
        disabled={Boolean(blocked) || previewPending}
        onClick={onPreview}
      >
        {previewPending
          ? directApply
            ? "正在应用…"
            : "正在生成…"
          : actionLabel}
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

type ProjectSelectionState = "inherited" | "selected" | "available";

function ProjectOptionRow({
  name,
  state,
  actionLabel,
  actionDisabled,
  onToggle,
  children,
}: {
  name: string;
  state: ProjectSelectionState;
  actionLabel: string;
  actionDisabled: boolean;
  onToggle: () => void;
  children?: React.ReactNode;
}) {
  const assigned = state !== "available";

  return (
    <div className="flex items-center justify-between gap-3 rounded-lg border p-3 text-sm">
      <div className="flex min-w-0 flex-wrap items-center gap-1.5">
        <span className="shrink-0">{name}</span>
        <StateTags state={state} />
        {children}
      </div>
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="shrink-0 shadow-none"
        aria-label={actionLabel}
        aria-pressed={assigned}
        disabled={actionDisabled}
        onClick={onToggle}
      >
        {assigned ? "禁用" : "启用"}
      </Button>
    </div>
  );
}

function StateTags({ state }: { state: ProjectSelectionState }) {
  switch (state) {
    case "inherited":
      return (
        <>
          <OptionTag tone="info">全局继承</OptionTag>
          <OptionTag tone="muted">只读</OptionTag>
        </>
      );
    case "selected":
      return <OptionTag tone="success">项目追加</OptionTag>;
    case "available":
      return <OptionTag tone="muted">可追加</OptionTag>;
  }
}

function OptionTag({
  tone,
  children,
}: {
  tone: "muted" | "info" | "success" | "warning";
  children: React.ReactNode;
}) {
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 text-xs font-medium",
        tone === "muted" && "bg-muted text-muted-foreground border-transparent",
        tone === "info" &&
          "border-blue-200 bg-blue-50 text-blue-800 dark:border-blue-900/60 dark:bg-blue-950/40 dark:text-blue-300",
        tone === "success" &&
          "border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900/60 dark:bg-emerald-950/40 dark:text-emerald-300",
        tone === "warning" &&
          "border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/40 dark:text-amber-300",
      )}
    >
      {children}
    </span>
  );
}

function toolLabel(tool: Tool) {
  return toolMetadata(tool).label;
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
