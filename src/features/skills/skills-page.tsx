import { useId, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Eye, FolderMinus, RefreshCw } from "lucide-react";

import {
  commands,
  type PreviewPlan,
  type SkillContentPreviewDto,
  type SkillDto,
  type SkillTakeoverPreviewResultDto,
  type Tool,
} from "@/bindings/commands";
import {
  CentralList,
  CentralListCard,
  CentralListCardBody,
  CentralListCardFooter,
  CentralListLayoutToggle,
} from "@/components/central-list-layout";
import { EmptyState } from "@/components/empty-state";
import { PageHeader } from "@/components/page-header";
import { PlatformAssignmentButton } from "@/components/platform-assignment-button";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { useNotify } from "@/components/use-notify";
import { usePersistedCentralListLayout } from "@/components/use-persisted-central-list-layout";
import { useImportDialogState } from "@/features/sync/use-import-dialog-state";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { useSubmitGuard } from "@/hooks/use-submit-guard";
import { SkillDirectoryImportDialog } from "@/features/skills/skill-directory-import-dialog";
import { SkillGithubImportDialog } from "@/features/skills/skill-github-import-dialog";
import { SkillImportDialog } from "@/features/skills/skill-import-dialog";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import {
  SKILL_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";
import { globalTargetStatusPresentation } from "@/lib/global-target-status-ui";
import { presentTargetDiagnostic } from "@/lib/diagnostic-presentations";
import { ExternalChangeActions } from "@/features/sync/external-change-actions";
import {
  globalSkillStatusesQueryOptions,
  skillKeys,
  skillsQueryOptions,
} from "@/lib/skills-api";

export function SkillsPage() {
  const queryClient = useQueryClient();
  const skillsQuery = useQuery(skillsQueryOptions());
  const statusesQuery = useQuery(globalSkillStatusesQueryOptions());
  const enabledTools = useEnabledTools();
  const visibleStatuses = statusesQuery.data?.filter((status) =>
    enabledTools.has(status.tool),
  );
  const [listLayout, setListLayout] = usePersistedCentralListLayout("skills");
  const [openDirectoryImport, setOpenDirectoryImport] = useState(false);
  const [openGithubImport, setOpenGithubImport] = useState(false);
  const { notify } = useNotify();
  const [contentPreview, setContentPreview] =
    useState<SkillContentPreviewDto | null>(null);
  const importDialog = useImportDialogState();
  const [adoptTarget, setAdoptTarget] = useState<SkillDto | null>(null);
  const adoptGuard = useSubmitGuard();
  const adoptTitleId = useId();
  const adoptDescriptionId = useId();
  const closeContentPreview = () => setContentPreview(null);
  const { dialogRef: contentDialogRef } = useDialogFocus(
    contentPreview !== null,
    closeContentPreview,
  );
  const closeAdoptDialog = () => {
    if (!adoptGuard.isInFlight()) setAdoptTarget(null);
  };
  const { dialogRef: adoptDialogRef } = useDialogFocus(
    adoptTarget !== null,
    closeAdoptDialog,
  );

  const invalidateSkills = async () => {
    await queryClient.invalidateQueries({ queryKey: skillKeys.all });
  };

  const deleteMutation = useMutation({
    mutationFn: async (skill: SkillDto) =>
      unwrapResult(
        await commands.deleteSkill({
          id: skill.id,
          rowVersion: skill.rowVersion,
        }),
      ),
    onSuccess: async (result) => {
      await invalidateSkills();
      notify({
        kind: "success",
        message: "Skill 已安全移出中央库，来源目录保持不变。",
      });
      await requestPreview(result.affectedSyncScopes ?? []);
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: `移出中央库失败：${profileErrorText(error) ?? "未知错误"}`,
      });
    },
  });

  const contentMutation = useMutation({
    mutationFn: async (id: string) =>
      unwrapResult(await commands.previewSkillContent(id)),
    onSuccess: setContentPreview,
    onError: (error) => {
      notify({
        kind: "error",
        message: `内容预览失败：${profileErrorText(error) ?? "未知错误"}`,
      });
    },
  });

  const adoptMutation = useMutation({
    mutationFn: async (skill: SkillDto) =>
      unwrapResult(
        await commands.adoptSkillContent({
          id: skill.id,
          rowVersion: skill.rowVersion,
        }),
      ),
    onSuccess: async () => {
      await invalidateSkills();
      setAdoptTarget(null);
      notify({
        kind: "success",
        message: "已采纳当前中央文件为权威内容；工具目录中的符号链接未被改写。",
      });
    },
    onError: async (error) => {
      await invalidateSkills();
      setAdoptTarget(null);
      notify({
        kind: "error",
        message: `同步更改失败：${profileErrorText(error) ?? "未知错误"}`,
      });
    },
    onSettled: () => {
      adoptGuard.end();
    },
  });

  const globalAssignmentMutation = useMutation({
    mutationFn: async ({ skill, tool }: { skill: SkillDto; tool: Tool }) =>
      unwrapResult(
        await commands.setGlobalSkillAssignment({
          tool,
          skillId: skill.id,
          assigned: !skill.globalTools.includes(tool),
          rowVersion: skill.rowVersion,
        }),
      ),
    onSuccess: async (result) => {
      await invalidateSkills();
      await requestPreview(result.affectedSyncScopes ?? []);
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新 Skill 全局分配失败。",
      });
    },
  });

  const takeoverApplyMutation = useMutation({
    mutationFn: async (result: SkillTakeoverPreviewResultDto) => {
      if (!canApplyPreview(result.plan)) {
        throw new Error("接管预览包含阻断目标，请重新检测后再试。");
      }
      return unwrapResult(
        await commands.applySkillPreview({
          previewId: result.plan.previewId,
          tool: result.tool,
          projectId: null,
        }),
      );
    },
    onSuccess: async (result) => {
      await invalidateSkills();
      importDialog.close();
      notify({
        kind: "success",
        message: `已接管并应用 ${result.appliedTargets} 个 Skill 目标，并创建 ${result.snapshotCount} 份快照。`,
      });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: `接管 Skill 失败：${profileErrorText(error) ?? "未知错误"}`,
      });
    },
  });

  const { requestPreview } = useSyncPreviewFlow({
    artifactKind: "skill",
    preview: (tool, projectId) =>
      commands.previewSkillSync({
        tool,
        projectId: projectId ?? null,
        excludeFromGit: false,
      }),
    apply: ({ previewId, tool, projectId }) =>
      commands.applySkillPreview({
        previewId,
        tool,
        projectId: projectId ?? null,
      }),
    invalidate: invalidateSkills,
    messages: {
      previewFailed: "生成 Skills 全局预览失败。",
      applyFailed: "应用 Skills 全局同步失败。",
      empty: "当前工具没有需要同步的全局 Skill。",
      applied: (result) =>
        `已应用 ${result.appliedTargets} 个 Skills 目标，并创建 ${result.snapshotCount} 份快照。`,
    },
  });

  return (
    <>
      <PageHeader
        title="Skills"
        actions={
          <>
            <CentralListLayoutToggle
              value={listLayout}
              onChange={setListLayout}
            />
            <Button size="sm" onClick={() => setOpenDirectoryImport(true)}>
              从本地目录导入
            </Button>
            <Button size="sm" onClick={() => setOpenGithubImport(true)}>
              从 GitHub 导入
            </Button>
          </>
        }
      />
      <main className="space-y-6 px-8 py-6">
        <section
          className="bg-card rounded-lg border p-5"
          aria-labelledby="skill-list-title"
        >
          <h2 id="skill-list-title" className="text-[15px] font-semibold">
            中央列表
          </h2>
          {skillsQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在读取 Skills…
            </p>
          ) : null}
          {skillsQuery.isError ? (
            <p role="alert" className="text-destructive mt-4 text-sm">
              {profileErrorText(skillsQuery.error)}
            </p>
          ) : null}
          {skillsQuery.data?.length === 0 ? (
            <div className="mt-4">
              <EmptyState
                title="尚无 Skill"
                description="可从工具、本地目录或 GitHub 导入。"
              />
            </div>
          ) : null}
          <CentralList layout={listLayout}>
            {skillsQuery.data?.map((skill) => {
              const isReadingContent =
                contentMutation.isPending &&
                contentMutation.variables === skill.id;
              const isRemovingSkill =
                deleteMutation.isPending &&
                deleteMutation.variables?.id === skill.id;
              const isAdoptingSkill =
                adoptMutation.isPending &&
                adoptMutation.variables?.id === skill.id;
              const skillDiagnosticPresentation = skill.diagnosticCode
                ? presentTargetDiagnostic(
                    skill.status === "invalid"
                      ? "failed"
                      : skill.status === "missing"
                        ? "missing"
                        : "in_sync",
                    skill.diagnosticCode,
                    { artifactKind: "skill" },
                  )
                : null;
              const skillActions = (
                <div className="flex min-w-0 flex-wrap gap-2">
                  <Button
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label={isReadingContent ? "正在读取…" : "内容预览"}
                    title={isReadingContent ? "正在读取…" : "内容预览"}
                    disabled={contentMutation.isPending}
                    onClick={() => contentMutation.mutate(skill.id)}
                  >
                    <Eye aria-hidden="true" className="size-4" />
                  </Button>
                  {skill.diagnosticCode === "CENTRAL_SKILL_CONTENT_CHANGED" ? (
                    <Button
                      size="sm"
                      variant="outline"
                      className="size-8 p-0"
                      aria-haspopup="dialog"
                      aria-label={isAdoptingSkill ? "正在采纳…" : "同步更改"}
                      title={isAdoptingSkill ? "正在采纳…" : "同步更改"}
                      disabled={adoptMutation.isPending}
                      onClick={() => setAdoptTarget(skill)}
                    >
                      <RefreshCw aria-hidden="true" className="size-4" />
                    </Button>
                  ) : null}
                  <Button
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label={isRemovingSkill ? "正在移出…" : "移出中央库"}
                    title={isRemovingSkill ? "正在移出…" : "移出中央库"}
                    disabled={
                      deleteMutation.isPending &&
                      deleteMutation.variables?.id === skill.id
                    }
                    onClick={() => deleteMutation.mutate(skill)}
                  >
                    <FolderMinus aria-hidden="true" className="size-4" />
                  </Button>
                </div>
              );
              const platformActions = (
                <div
                  className={
                    listLayout === "grid"
                      ? "ml-auto flex shrink-0 items-center gap-2"
                      : "flex items-center gap-2"
                  }
                  role="group"
                  aria-label={`${skill.name} 全局平台分配`}
                >
                  {filterEnabledTools(SKILL_TOOLS, enabledTools).map((tool) => (
                    <PlatformAssignmentButton
                      key={tool}
                      tool={tool}
                      assigned={skill.globalTools.includes(tool)}
                      disabled={
                        globalAssignmentMutation.isPending ||
                        (skill.status !== "ready" &&
                          !skill.globalTools.includes(tool))
                      }
                      onClick={() =>
                        globalAssignmentMutation.mutate({ skill, tool })
                      }
                    />
                  ))}
                </div>
              );

              return (
                <CentralListCard key={skill.id} layout={listLayout}>
                  <CentralListCardBody layout={listLayout}>
                    <div className="flex min-w-0 flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0">
                        <h3
                          className={
                            listLayout === "grid"
                              ? "truncate font-medium"
                              : "font-medium"
                          }
                          title={skill.name}
                        >
                          {skill.name}
                        </h3>
                        <p className="text-muted-foreground mt-1 text-xs">
                          {skill.status} · hash {skill.contentHash.slice(0, 12)}
                          …
                        </p>
                        {skillDiagnosticPresentation ? (
                          <p className="text-destructive mt-1 text-xs">
                            {skillDiagnosticPresentation.description}{" "}
                            {skillDiagnosticPresentation.nextStep}
                          </p>
                        ) : null}
                      </div>
                      {listLayout === "list" ? skillActions : null}
                    </div>
                    {listLayout === "list" ? (
                      <>
                        <dl className="mt-3 grid gap-2 text-xs">
                          <div>
                            <dt className="text-muted-foreground">来源</dt>
                            <dd className="break-all">{skill.sourcePath}</dd>
                          </div>
                          <div>
                            <dt className="text-muted-foreground">中央副本</dt>
                            <dd className="break-all">{skill.centralPath}</dd>
                          </div>
                        </dl>
                        <p className="bg-muted rounded-control mt-3 p-2 text-xs">
                          {skill.description}
                        </p>
                      </>
                    ) : (
                      <p
                        className="text-muted-foreground mt-4 line-clamp-3 text-sm leading-6"
                        title={skill.description}
                      >
                        {skill.description}
                      </p>
                    )}
                  </CentralListCardBody>
                  <CentralListCardFooter
                    layout={listLayout}
                    label={`${skill.name} 操作`}
                  >
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      {listLayout === "grid" ? skillActions : null}
                      {platformActions}
                    </div>
                  </CentralListCardFooter>
                </CentralListCard>
              );
            })}
          </CentralList>
        </section>

        <section
          className="bg-card rounded-lg border p-5"
          aria-labelledby="skill-target-title"
        >
          <h2 id="skill-target-title" className="text-[15px] font-semibold">
            全局目标状态
          </h2>
          {statusesQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在检查全局 Skills 目标…
            </p>
          ) : null}
          {statusesQuery.isError ? (
            <p role="alert" className="text-destructive mt-4 text-sm">
              {profileErrorText(statusesQuery.error)}
            </p>
          ) : null}
          {statusesQuery.data != null && visibleStatuses?.length === 0 ? (
            <p className="text-muted-foreground mt-4 text-sm">
              当前没有可检查的全局 Skills 目标。
            </p>
          ) : null}
          {visibleStatuses && visibleStatuses.length > 0 ? (
            <div className="mt-3 divide-y">
              {visibleStatuses.map((status) => {
                const presentation = globalTargetStatusPresentation(
                  status.status,
                  status.diagnosticCode,
                  { tool: status.tool, artifactKind: "skill" },
                );
                const diagnosticPresentation = status.diagnosticCode
                  ? presentTargetDiagnostic(
                      status.status,
                      status.diagnosticCode,
                      { tool: status.tool, artifactKind: "skill" },
                    )
                  : null;
                return (
                  <article
                    key={status.tool}
                    className="hover:bg-muted/50 px-1 py-2.5 text-sm"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <strong>{toolMetadata(status.tool).label}</strong>
                      <SyncStatusBadge
                        label={presentation.label}
                        status={status.status}
                        tone={presentation.tone}
                      />
                    </div>
                    <code className="mt-2 block text-xs break-all">
                      {status.targetPath ?? "目标不可用"}
                    </code>
                    {presentation.description ? (
                      <p className="text-muted-foreground mt-2 text-xs">
                        {presentation.description}
                      </p>
                    ) : null}
                    {diagnosticPresentation ? (
                      <p className="text-warning mt-2 text-xs">
                        {diagnosticPresentation.nextStep}
                      </p>
                    ) : null}
                    <ExternalChangeActions
                      artifactKind="skill"
                      tool={status.tool}
                      status={status.status}
                      onInvalidate={invalidateSkills}
                      onMatchOrImport={() => {
                        if (importDialog.state) return;
                        importDialog.open(status.tool);
                      }}
                    />
                    <div className="mt-3 flex flex-wrap gap-2">
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={presentation.previewBlocked}
                        aria-label={`检测并导入 ${toolMetadata(status.tool).label} 全局 Skills`}
                        onClick={() => {
                          if (importDialog.state) return;
                          importDialog.open(status.tool);
                        }}
                      >
                        {status.diagnosticCode ===
                        "SKILL_TARGET_INITIAL_TAKEOVER_REQUIRED"
                          ? "检测并接管已有 Skills"
                          : "检测并导入已有 Skills"}
                      </Button>
                    </div>
                  </article>
                );
              })}
            </div>
          ) : null}
        </section>

        {contentPreview ? (
          <DialogOverlay>
            <DialogContent
              dialogRef={contentDialogRef}
              onClose={closeContentPreview}
              labelledBy="skill-content-title"
              size="lg"
            >
              <DialogHeader>
                <div>
                  <p className="text-muted-foreground">内容预览</p>
                  <h2
                    id="skill-content-title"
                    className="mt-1 text-[15px] font-semibold"
                  >
                    {contentPreview.name}
                  </h2>
                </div>
              </DialogHeader>
              <DialogBody>
                <pre className="bg-muted rounded-control overflow-auto p-4 text-xs leading-5">
                  {contentPreview.skillMd}
                </pre>
                <p className="mt-4 font-medium">目录文件</p>
                <ul className="mt-2 list-disc pl-5 text-xs">
                  {contentPreview.files.map((file) => (
                    <li key={file}>{file}</li>
                  ))}
                </ul>
                {contentPreview.files.length === 0 ? (
                  <p className="text-muted-foreground mt-2 text-xs">
                    目录文件列表为空。
                  </p>
                ) : null}
              </DialogBody>
            </DialogContent>
          </DialogOverlay>
        ) : null}

        {adoptTarget ? (
          <DialogOverlay>
            <DialogContent
              dialogRef={adoptDialogRef}
              onClose={closeAdoptDialog}
              labelledBy={adoptTitleId}
              describedBy={adoptDescriptionId}
              size="sm"
            >
              <DialogHeader>
                <div>
                  <h2 id={adoptTitleId} className="text-[15px] font-semibold">
                    同步更改
                  </h2>
                  <p
                    id={adoptDescriptionId}
                    className="text-muted-foreground mt-1"
                  >
                    是否将当前中央文件采纳为权威内容？这只会更新应用内记录，不会改写工具目录中的符号链接。
                  </p>
                </div>
              </DialogHeader>
              <DialogBody>
                {adoptMutation.isPending ? (
                  <p role="status" className="text-muted-foreground">
                    正在采纳当前中央文件…
                  </p>
                ) : null}
              </DialogBody>
              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  disabled={adoptMutation.isPending}
                  onClick={closeAdoptDialog}
                >
                  取消
                </Button>
                <Button
                  type="button"
                  disabled={adoptMutation.isPending}
                  onClick={() => {
                    if (!adoptTarget) return;
                    if (!adoptGuard.begin()) return;
                    adoptDialogRef.current?.focus();
                    adoptMutation.mutate(adoptTarget);
                  }}
                >
                  {adoptMutation.isPending ? "正在采纳…" : "是"}
                </Button>
              </DialogFooter>
            </DialogContent>
          </DialogOverlay>
        ) : null}

        {openDirectoryImport ? (
          <SkillDirectoryImportDialog
            onClose={() => setOpenDirectoryImport(false)}
            onImported={async () => {
              await invalidateSkills();
              notify({
                kind: "success",
                message:
                  "Skill 已复制到应用私有中央库；来源目录未修改，原生目标也尚未写入。",
              });
            }}
          />
        ) : null}

        {openGithubImport ? (
          <SkillGithubImportDialog
            onClose={() => setOpenGithubImport(false)}
            onImported={async () => {
              await queryClient.invalidateQueries(
                { queryKey: skillKeys.all },
                { throwOnError: true },
              );
              notify({
                kind: "success",
                message:
                  "GitHub Skill 已复制到应用私有中央库；未执行脚本，也未自动分配或同步。",
              });
            }}
          />
        ) : null}

        {importDialog.state ? (
          <SkillImportDialog
            key={importDialog.state.requestId}
            tool={importDialog.state.tool}
            requestId={importDialog.state.requestId}
            onClose={importDialog.close}
            onRescan={importDialog.rescan}
            onImported={async (result) => {
              await queryClient.invalidateQueries(
                { queryKey: skillKeys.all },
                { throwOnError: true },
              );
              notify({
                kind: "success",
                message: `已复制 ${result.createdCount} 项 Skill 到中央库；原有安装未变，尚未自动分配或同步。中央副本不会随原安装自动更新。`,
              });
              importDialog.close();
            }}
            onTakeoverPrepared={async (result) => {
              await queryClient.invalidateQueries(
                { queryKey: skillKeys.all },
                { throwOnError: true },
              );
              // 选择接管所选项就是用户的授权边界。准备阶段已经持久化
              // 完整的 takeover evidence，随后只消费这份精确 previewId。
              await takeoverApplyMutation.mutateAsync(result);
            }}
          />
        ) : null}
      </main>
    </>
  );
}

function canApplyPreview(plan: PreviewPlan): boolean {
  return (
    plan.targets.length > 0 &&
    plan.targets.every(
      (target) => target.changeKind !== "conflict" && target.errorCode === null,
    )
  );
}
