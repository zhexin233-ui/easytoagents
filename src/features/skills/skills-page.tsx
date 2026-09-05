import { useId, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Eye, FolderMinus, RefreshCw } from "lucide-react";

import {
  commands,
  type ApplySkillPreviewInput,
  type PreviewPlan,
  type SkillContentPreviewDto,
  type SkillDto,
  type Tool,
} from "@/bindings/commands";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import {
  CentralList,
  CentralListCard,
  CentralListCardBody,
  CentralListCardFooter,
  CentralListLayoutToggle,
} from "@/components/central-list-layout";
import { Notify } from "@/components/notify";
import { PlatformAssignmentButton } from "@/components/platform-assignment-button";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { useNotify } from "@/components/use-notify";
import { usePersistedCentralListLayout } from "@/components/use-persisted-central-list-layout";
import { SkillDirectoryImportDialog } from "@/features/skills/skill-directory-import-dialog";
import { SkillImportDialog } from "@/features/skills/skill-import-dialog";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import {
  SKILL_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";
import { globalTargetStatusPresentation } from "@/lib/global-target-status-ui";
import {
  appSettingsQueryOptions,
  canAutoApplyPreview,
} from "@/lib/settings-api";
import {
  globalSkillStatusesQueryOptions,
  skillKeys,
  skillsQueryOptions,
} from "@/lib/skills-api";

interface OpenSkillPreview {
  plan: PreviewPlan;
  tool: Tool;
}

interface SkillPreviewRequest {
  tool: Tool;
  autoApply: boolean;
}

interface SkillApplyRequest {
  input: ApplySkillPreviewInput;
}

export function SkillsPage() {
  const queryClient = useQueryClient();
  const skillsQuery = useQuery(skillsQueryOptions());
  const statusesQuery = useQuery(globalSkillStatusesQueryOptions());
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const directApply = settingsQuery.data?.applyMode === "direct";
  const enabledTools = useEnabledTools();
  const visibleStatuses = statusesQuery.data?.filter((status) =>
    enabledTools.has(status.tool),
  );
  const [listLayout, setListLayout] = usePersistedCentralListLayout("skills");
  const [openDirectoryImport, setOpenDirectoryImport] = useState(false);
  const { notification, notify } = useNotify();
  const [contentPreview, setContentPreview] =
    useState<SkillContentPreviewDto | null>(null);
  const [openPreview, setOpenPreview] = useState<OpenSkillPreview | null>(null);
  const [openImport, setOpenImport] = useState<{
    tool: Tool;
    requestId: string;
  } | null>(null);
  const [adoptTarget, setAdoptTarget] = useState<SkillDto | null>(null);
  const adoptInFlight = useRef(false);
  const adoptTitleId = useId();
  const adoptDescriptionId = useId();
  const closeContentPreview = () => setContentPreview(null);
  const { dialogRef: contentDialogRef, onKeyDown: onContentDialogKeyDown } =
    useDialogFocus(contentPreview !== null, closeContentPreview);
  const closeAdoptDialog = () => {
    if (!adoptInFlight.current) setAdoptTarget(null);
  };
  const { dialogRef: adoptDialogRef, onKeyDown: onAdoptDialogKeyDown } =
    useDialogFocus(adoptTarget !== null, closeAdoptDialog);

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
    onSuccess: async () => {
      await invalidateSkills();
      notify({
        kind: "success",
        message: "Skill 已安全移出中央库，来源目录保持不变。",
      });
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
      adoptInFlight.current = false;
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
    onSuccess: async (_result, { tool }) => {
      await invalidateSkills();
      if (!directApply) {
        notify({
          kind: "success",
          message:
            "全局分配已更新；这只改变中央配置，分配或取消分配不会自动写入工具目录。请预览全局同步并确认应用。",
        });
        return;
      }
      previewMutation.mutate({ tool, autoApply: true });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新 Skill 全局分配失败。",
      });
    },
  });

  const previewMutation = useMutation({
    mutationFn: async ({ tool }: SkillPreviewRequest) => ({
      tool,
      plan: unwrapResult(
        await commands.previewSkillSync({
          tool,
          projectId: null,
          excludeFromGit: false,
        }),
      ),
    }),
    onSuccess: ({ plan, tool }, { autoApply }) => {
      if (plan.targets.length === 0) {
        notify({
          kind: "success",
          message: "当前工具没有需要同步的全局 Skill。",
        });
        setOpenPreview(null);
        return;
      }
      if (autoApply && canAutoApplyPreview(plan)) {
        applyMutation.mutate({
          input: {
            previewId: plan.previewId,
            tool,
            projectId: null,
          },
        });
        return;
      }
      setOpenPreview({ plan, tool });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "生成 Skills 全局预览失败。",
      });
    },
  });

  const applyMutation = useMutation({
    mutationFn: async ({ input }: SkillApplyRequest) =>
      unwrapResult(await commands.applySkillPreview(input)),
    onSuccess: async (result) => {
      const successMessage = `已应用 ${result.appliedTargets} 个 Skills 目标，并创建 ${result.snapshotCount} 份快照。`;
      setOpenPreview(null);
      await invalidateSkills();
      notify({ kind: "success", message: successMessage });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "应用 Skills 全局同步失败。",
      });
    },
  });

  return (
    <main className="p-6 lg:p-8">
      <Notify notification={notification} />
      <header className="mx-auto max-w-6xl">
        <p className="text-muted-foreground text-sm">应用私有中央库</p>
        <h1 className="mt-1 text-2xl font-semibold">Skills</h1>
        <p className="text-muted-foreground mt-2 max-w-3xl text-sm leading-6">
          导入只复制本地目录，不移动或修改来源。各工具目标始终使用指向中央副本的符号链接，
          并且只能通过持久化预览 Apply。
        </p>
      </header>

      <div className="mx-auto mt-6 max-w-6xl">
        <section
          className="bg-card rounded-xl border p-5"
          aria-labelledby="skill-list-title"
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 id="skill-list-title" className="text-lg font-semibold">
              中央列表
            </h2>
            <div className="flex flex-wrap items-center gap-2">
              <CentralListLayoutToggle
                value={listLayout}
                onChange={setListLayout}
              />
              <Button size="sm" onClick={() => setOpenDirectoryImport(true)}>
                从本地目录导入
              </Button>
            </div>
          </div>
          {skillsQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在读取 Skills…
            </p>
          ) : null}
          {skillsQuery.isError ? (
            <p
              role="alert"
              className="mt-4 text-sm text-red-700 dark:text-red-300"
            >
              {profileErrorText(skillsQuery.error)}
            </p>
          ) : null}
          {skillsQuery.data?.length === 0 ? (
            <p className="text-muted-foreground mt-4 text-sm">
              尚无 Skill。可在下方全局目标卡片选择“检测并导入已有
              Skills”，或点击“从本地目录导入”复制本地目录。
            </p>
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
                        {skill.diagnosticCode ? (
                          <p className="mt-1 text-xs break-all text-red-700 dark:text-red-300">
                            {skill.diagnosticCode}
                          </p>
                        ) : null}
                      </div>
                      {listLayout === "list" ? skillActions : null}
                    </div>
                    {listLayout === "list" ? (
                      <>
                        <dl className="mt-3 grid gap-2 text-xs">
                          <div>
                            <dt className="text-muted-foreground">
                              原来源（只读溯源）
                            </dt>
                            <dd className="break-all">{skill.sourcePath}</dd>
                          </div>
                          <div>
                            <dt className="text-muted-foreground">中央副本</dt>
                            <dd className="break-all">{skill.centralPath}</dd>
                          </div>
                        </dl>
                        <p className="bg-muted mt-3 rounded p-2 text-xs">
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
                    {listLayout === "list" ? (
                      <p className="text-muted-foreground mt-2 text-xs leading-5">
                        {directApply
                          ? "全局分配只更新中央配置；直接应用模式下会自动同步写入工具目录。"
                          : "全局分配只更新中央配置，不会写入工具目录；请在下方预览全局同步并确认应用。"}
                      </p>
                    ) : null}
                  </CentralListCardFooter>
                </CentralListCard>
              );
            })}
          </CentralList>
        </section>
      </div>

      <section
        className="bg-card mx-auto mt-6 max-w-6xl rounded-xl border p-5"
        aria-labelledby="skill-target-title"
      >
        <h2 id="skill-target-title" className="text-lg font-semibold">
          全局目标状态
        </h2>
        {statusesQuery.isPending ? (
          <p role="status" className="mt-4 text-sm">
            正在检查全局 Skills 目标…
          </p>
        ) : null}
        {statusesQuery.isError ? (
          <p
            role="alert"
            className="mt-4 text-sm text-red-700 dark:text-red-300"
          >
            {profileErrorText(statusesQuery.error)}
          </p>
        ) : null}
        {statusesQuery.data != null && visibleStatuses?.length === 0 ? (
          <p className="text-muted-foreground mt-4 text-sm">
            当前没有可检查的全局 Skills 目标。
          </p>
        ) : null}
        {visibleStatuses && visibleStatuses.length > 0 ? (
          <div className="mt-4 grid gap-3 md:grid-cols-2">
            {visibleStatuses.map((status) => {
              const presentation = globalTargetStatusPresentation(
                status.status,
                status.diagnosticCode,
                { directApply },
              );
              return (
                <article
                  key={status.tool}
                  className="rounded-lg border p-4 text-sm"
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
                  {status.diagnosticCode ? (
                    <p className="mt-2 text-xs text-amber-800 dark:text-amber-300">
                      诊断码：<code>{status.diagnosticCode}</code>
                    </p>
                  ) : null}
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={presentation.previewBlocked}
                      aria-label={`检测并导入 ${toolMetadata(status.tool).label} 全局 Skills`}
                      onClick={() => {
                        if (openImport) return;
                        setOpenImport({
                          tool: status.tool,
                          requestId: crypto.randomUUID(),
                        });
                      }}
                    >
                      检测并导入已有 Skills
                    </Button>
                    {!directApply ? (
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={
                          previewMutation.isPending ||
                          presentation.previewBlocked
                        }
                        onClick={() =>
                          previewMutation.mutate({
                            tool: status.tool,
                            autoApply: directApply,
                          })
                        }
                      >
                        {previewMutation.isPending
                          ? "正在生成…"
                          : "预览全局同步"}
                      </Button>
                    ) : null}
                  </div>
                </article>
              );
            })}
          </div>
        ) : null}
      </section>

      {contentPreview ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4">
          <section
            ref={contentDialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby="skill-content-title"
            tabIndex={-1}
            onKeyDown={onContentDialogKeyDown}
            className="bg-card max-h-[88vh] w-full max-w-3xl overflow-auto rounded-xl p-6 shadow-xl"
          >
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-muted-foreground text-sm">
                  中央副本只读内容
                </p>
                <h2
                  id="skill-content-title"
                  className="mt-1 text-xl font-semibold"
                >
                  {contentPreview.name}
                </h2>
              </div>
              <Button variant="outline" size="sm" onClick={closeContentPreview}>
                关闭
              </Button>
            </div>
            <pre className="bg-muted mt-4 overflow-auto rounded p-4 text-xs leading-5">
              {contentPreview.skillMd}
            </pre>
            <p className="mt-4 text-sm font-medium">目录文件</p>
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
          </section>
        </div>
      ) : null}

      {adoptTarget ? (
        <div
          role="presentation"
          className="fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4"
        >
          <section
            ref={adoptDialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby={adoptTitleId}
            aria-describedby={adoptDescriptionId}
            tabIndex={-1}
            onKeyDown={onAdoptDialogKeyDown}
            className="bg-card w-full max-w-lg rounded-xl p-6 shadow-xl"
          >
            <div className="flex items-start justify-between gap-3">
              <div>
                <h2 id={adoptTitleId} className="text-xl font-semibold">
                  同步更改
                </h2>
                <p
                  id={adoptDescriptionId}
                  className="text-muted-foreground mt-2 text-sm"
                >
                  是否将当前中央文件采纳为权威内容？这只会更新应用内记录，不会改写工具目录中的符号链接。
                </p>
              </div>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={adoptMutation.isPending}
                onClick={closeAdoptDialog}
              >
                关闭
              </Button>
            </div>
            {adoptMutation.isPending ? (
              <p role="status" className="text-muted-foreground mt-4 text-sm">
                正在采纳当前中央文件…
              </p>
            ) : null}
            <div className="mt-6 flex flex-wrap justify-end gap-3">
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
                  if (!adoptTarget || adoptInFlight.current) return;
                  adoptDialogRef.current?.focus();
                  adoptInFlight.current = true;
                  adoptMutation.mutate(adoptTarget);
                }}
              >
                {adoptMutation.isPending ? "正在采纳…" : "是"}
              </Button>
            </div>
          </section>
        </div>
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

      {openImport ? (
        <SkillImportDialog
          key={openImport.tool}
          tool={openImport.tool}
          requestId={openImport.requestId}
          onClose={() => setOpenImport(null)}
          onRescan={() =>
            setOpenImport({
              tool: openImport.tool,
              requestId: crypto.randomUUID(),
            })
          }
          onImported={async (result) => {
            await queryClient.invalidateQueries(
              { queryKey: skillKeys.all },
              { throwOnError: true },
            );
            notify({
              kind: "success",
              message: `已复制 ${result.createdCount} 项 Skill 到中央库；原有安装未变，尚未自动分配或同步。中央副本不会随原安装自动更新。`,
            });
            setOpenImport(null);
          }}
          onTakeoverPrepared={async (result) => {
            await queryClient.invalidateQueries(
              { queryKey: skillKeys.all },
              { throwOnError: true },
            );
            notify({
              kind: "success",
              message: `已为 ${result.assignedCount + result.reusedCount} 项 Skill 准备接管；请审阅持久化预览后显式应用。`,
            });
            setOpenImport(null);
            // 接管无条件进入预览，即使全局偏好是 direct 也不会自动 Apply。
            setOpenPreview({ plan: result.plan, tool: result.tool });
          }}
        />
      ) : null}

      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? "claude"}
        artifactKind="skill"
        applying={applyMutation.isPending}
        onClose={() => setOpenPreview(null)}
        onApply={(previewId, tool) =>
          applyMutation.mutate({
            input: {
              previewId,
              tool,
              projectId: null,
            },
          })
        }
      />
    </main>
  );
}
