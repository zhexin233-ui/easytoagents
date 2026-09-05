import { useRef, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Trash2 } from "lucide-react";

import {
  commands,
  type PreviewPlan,
  type PromptImportPreviewDto,
  type PromptProfileDto,
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
import { FormDialog } from "@/components/form-dialog";
import { Notify } from "@/components/notify";
import { PlatformAssignmentButton } from "@/components/platform-assignment-button";
import { Button } from "@/components/ui/button";
import { useNotify } from "@/components/use-notify";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { usePersistedCentralListLayout } from "@/components/use-persisted-central-list-layout";
import {
  profileErrorText,
  profileKeys,
  promptProfilesQueryOptions,
  toolProfileStatusQueryOptions,
  unwrapResult,
} from "@/lib/profile-api";
import {
  appSettingsQueryOptions,
  canAutoApplyPreview,
} from "@/lib/settings-api";
import {
  PROFILE_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";

interface OpenPreview {
  plan: PreviewPlan;
  tool: Tool;
}

interface PromptPreviewRequest {
  tool: Tool;
  autoApply: boolean;
}

interface PromptApplyRequest {
  preview: OpenPreview;
}

interface PromptSaveVariables {
  globalTools: Tool[];
}

export function PromptsPage() {
  const queryClient = useQueryClient();
  const profilesQuery = useQuery(promptProfilesQueryOptions());
  const statusQueries = {
    claude: useQuery(toolProfileStatusQueryOptions("claude")),
    codex: useQuery(toolProfileStatusQueryOptions("codex")),
    zcode: useQuery(toolProfileStatusQueryOptions("zcode")),
  };
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const directApply = settingsQuery.data?.applyMode === "direct";
  const tools = filterEnabledTools(PROFILE_TOOLS, useEnabledTools());
  const [listLayout, setListLayout] = usePersistedCentralListLayout("prompts");
  const [editing, setEditing] = useState<PromptProfileDto | null>(null);
  const [name, setName] = useState("");
  const [body, setBody] = useState("");
  const [formOpen, setFormOpen] = useState(false);
  const saveInFlight = useRef(false);
  const { notification, notify } = useNotify();
  const [importPreview, setImportPreview] =
    useState<PromptImportPreviewDto | null>(null);
  const [openPreview, setOpenPreview] = useState<OpenPreview | null>(null);

  const refresh = async () => {
    await queryClient.invalidateQueries({ queryKey: profileKeys.prompts });
  };

  const saveMutation = useMutation<
    PromptProfileDto,
    Error,
    PromptSaveVariables
  >({
    mutationFn: async () =>
      editing
        ? unwrapResult(
            await commands.updatePromptProfile({
              id: editing.id,
              name,
              body,
              rowVersion: editing.rowVersion,
            }),
          )
        : unwrapResult(await commands.createPromptProfile({ name, body })),
    onSuccess: async (_result, { globalTools }) => {
      await refresh();
      setEditing(null);
      setName("");
      setBody("");
      setFormOpen(false);
      if (directApply && globalTools.length > 0) {
        notify({
          kind: "success",
          message: "中央提示词档案已保存；正在自动同步已分配工具。",
        });
        for (const tool of globalTools) {
          await previewMutation
            .mutateAsync({ tool, autoApply: true })
            .catch(() => undefined);
        }
        return;
      }
      notify({
        kind: "success",
        message: "中央提示词档案已保存，原生文件尚未修改。",
      });
    },
    onSettled: () => {
      saveInFlight.current = false;
    },
  });

  const openForm = (profile: PromptProfileDto | null) => {
    if (saveInFlight.current || saveMutation.isPending) return;
    saveMutation.reset();
    setEditing(profile);
    setName(profile?.name ?? "");
    setBody(profile?.body ?? "");
    setFormOpen(true);
  };

  const closeForm = () => {
    if (saveInFlight.current || saveMutation.isPending) return;
    setFormOpen(false);
    setEditing(null);
    setName("");
    setBody("");
    saveMutation.reset();
  };

  const assignmentMutation = useMutation({
    mutationFn: async ({
      profile,
      tool,
    }: {
      profile: PromptProfileDto;
      tool: Tool;
    }) =>
      unwrapResult(
        await commands.setGlobalPromptAssignment({
          tool,
          promptProfileId: profile.id,
          assigned: !profile.globalTools.includes(tool),
          rowVersion: profile.rowVersion,
        }),
      ),
    onSuccess: async (_result, { tool }) => {
      await refresh();
      if (!directApply) {
        notify({
          kind: "success",
          message:
            "全局启用已更新；这只改变中央配置，原生全局文件尚未写入。请在该工具卡片预览全局同步并确认应用。",
        });
        return;
      }
      previewMutation.mutate({ tool, autoApply: true });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新提示词全局启用失败。",
      });
    },
  });

  const previewMutation = useMutation({
    mutationFn: async ({ tool }: PromptPreviewRequest) =>
      unwrapResult(await commands.previewPromptSync(tool, null)),
    onSuccess: (plan, { tool, autoApply }) => {
      if (autoApply && canAutoApplyPreview(plan)) {
        applyMutation.mutate({
          preview: { plan, tool },
        });
        return;
      }
      setOpenPreview({ plan, tool });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "生成提示词全局预览失败。",
      });
    },
  });

  const applyMutation = useMutation({
    mutationFn: async ({ preview }: PromptApplyRequest) =>
      unwrapResult(
        await commands.applyProfilePreview({
          previewId: preview.plan.previewId,
          tool: preview.tool,
          artifactKind: "prompt",
          projectId: null,
        }),
      ),
    onSuccess: (result) => {
      const successMessage = `已应用 ${result.appliedTargets} 个目标，可从快照恢复。`;
      setOpenPreview(null);
      notify({ kind: "success", message: successMessage });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "应用提示词全局同步失败。",
      });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: async (profile: PromptProfileDto) =>
      unwrapResult(
        await commands.deletePromptProfile({
          id: profile.id,
          rowVersion: profile.rowVersion,
        }),
      ),
    onSuccess: async (_result, profile) => {
      await refresh();
      if (directApply && profile.globalTools.length > 0) {
        notify({
          kind: "success",
          message: "中央提示词已删除；正在自动清理已接管文件。",
        });
        for (const tool of profile.globalTools) {
          await previewMutation
            .mutateAsync({ tool, autoApply: true })
            .catch(() => undefined);
        }
        return;
      }
      notify({
        kind: "success",
        message: directApply
          ? "中央提示词已删除；该档案未分配到任何工具，无需清理。"
          : "中央提示词已删除；生成新预览后才会清理已接管文件。",
      });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "删除中央提示词失败。",
      });
    },
  });

  const discoverMutation = useMutation({
    mutationFn: async (tool: Tool) =>
      unwrapResult(await commands.discoverPromptImport(tool)),
    onSuccess: (preview) => {
      if (!preview) {
        notify({
          kind: "success",
          message: "未发现可导入的已有提示词。",
        });
        return;
      }
      setImportPreview(preview);
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "检测已有提示词失败。",
      });
    },
  });

  const confirmImportMutation = useMutation({
    mutationFn: async () => {
      if (!importPreview) {
        throw new Error("导入预览已关闭");
      }
      return unwrapResult(
        await commands.confirmPromptImport({
          previewId: importPreview.previewId,
          name: importPreview.suggestedName,
        }),
      );
    },
    onSuccess: async () => {
      setImportPreview(null);
      await refresh();
      notify({
        kind: "success",
        message: "已有提示词已无损导入并启用到来源工具，原生文件保持不变。",
      });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "导入已有提示词失败。",
      });
    },
  });

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (saveInFlight.current || saveMutation.isPending) return;
    saveInFlight.current = true;
    saveMutation.mutate({ globalTools: editing?.globalTools ?? [] });
  };

  return (
    <main className="p-6 lg:p-8">
      <Notify notification={notification} />
      <header className="mx-auto max-w-6xl">
        <p className="text-muted-foreground text-sm">提示词</p>
        <h1 className="mt-1 text-2xl font-semibold">全局提示词档案</h1>
        <p className="text-muted-foreground mt-2 max-w-3xl text-sm leading-6">
          集中维护提示词指令文档，并按工具通过图标启用或停用（每个工具同时只有一份生效，启用新档案会自动替换原生效档案）。中央修改不会直接改写原生配置，同步需经持久化预览确认。项目级的提示词分配在各项目详情页中进行。
        </p>
      </header>

      <div className="mx-auto mt-6 max-w-6xl">
        <section
          className="bg-card rounded-xl border p-5"
          aria-labelledby="prompt-list-title"
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h2 id="prompt-list-title" className="text-xl font-semibold">
                中央列表
              </h2>
              <p className="text-muted-foreground mt-1 text-sm">
                Markdown 正文原样写入工具的全局指令文件。
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <CentralListLayoutToggle
                value={listLayout}
                onChange={setListLayout}
              />
              <Button size="sm" onClick={() => openForm(null)}>
                新增提示词
              </Button>
            </div>
          </div>

          {profilesQuery.isPending ? (
            <p role="status" className="text-muted-foreground mt-5 text-sm">
              正在加载提示词档案…
            </p>
          ) : null}
          {profilesQuery.isError ? (
            <p
              role="alert"
              className="mt-5 text-sm text-red-700 dark:text-red-300"
            >
              {profileErrorText(profilesQuery.error)}
            </p>
          ) : null}
          {profilesQuery.data?.length === 0 ? (
            <p className="text-muted-foreground mt-5 rounded-lg border border-dashed p-4 text-sm">
              尚无提示词档案。点击“新增提示词”创建第一份档案，或在下方工具卡片检测已有提示词。
            </p>
          ) : null}

          <CentralList layout={listLayout}>
            {profilesQuery.data?.map((profile) => {
              const cardActions = (
                <>
                  <Button
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label="编辑"
                    title="编辑"
                    onClick={() => openForm(profile)}
                  >
                    <Pencil aria-hidden="true" className="size-4" />
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label="删除"
                    title="删除"
                    onClick={() => {
                      if (
                        globalThis.confirm(
                          "删除中央提示词档案？原生文件不会在此步骤修改。",
                        )
                      ) {
                        deleteMutation.mutate(profile);
                      }
                    }}
                  >
                    <Trash2 aria-hidden="true" className="size-4" />
                  </Button>
                </>
              );
              return (
                <CentralListCard key={profile.id} layout={listLayout}>
                  <CentralListCardBody layout={listLayout}>
                    <div className="flex min-w-0 flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0">
                        <h3 className="font-medium">{profile.name}</h3>
                        <p className="text-muted-foreground mt-1 text-xs">
                          {profile.globalTools.length > 0
                            ? `生效中：${profile.globalTools
                                .map((tool) => toolMetadata(tool).label)
                                .join("、")}`
                            : "未启用"}
                          {profile.importedFromPath
                            ? ` · 导入自 ${profile.importedFromPath}`
                            : null}
                        </p>
                      </div>
                      {listLayout === "list" ? (
                        <div className="flex shrink-0 gap-2">{cardActions}</div>
                      ) : null}
                    </div>
                    {listLayout === "list" ? (
                      <p className="bg-muted mt-3 line-clamp-6 rounded p-2 text-xs leading-5 whitespace-pre-wrap">
                        {profile.body}
                      </p>
                    ) : (
                      <p
                        className="text-muted-foreground mt-4 line-clamp-3 text-sm leading-6 whitespace-pre-wrap"
                        title={profile.body}
                      >
                        {profile.body}
                      </p>
                    )}
                  </CentralListCardBody>
                  <CentralListCardFooter
                    layout={listLayout}
                    label={`${profile.name} 提示词操作`}
                  >
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      {listLayout === "grid" ? cardActions : null}
                      <div
                        className={
                          listLayout === "grid"
                            ? "ml-auto flex shrink-0 items-center gap-2"
                            : "flex items-center gap-2"
                        }
                        role="group"
                        aria-label={`${profile.name} 全局启用`}
                      >
                        {tools.map((tool) => (
                          <PlatformAssignmentButton
                            key={tool}
                            tool={tool}
                            assigned={profile.globalTools.includes(tool)}
                            disabled={assignmentMutation.isPending}
                            onClick={() =>
                              assignmentMutation.mutate({ profile, tool })
                            }
                          />
                        ))}
                      </div>
                    </div>
                    {listLayout === "list" ? (
                      <p className="text-muted-foreground mt-2 text-xs leading-5">
                        图标启用或停用只更新中央配置；原生全局文件仍需在该工具卡片预览同步后写入。
                      </p>
                    ) : null}
                  </CentralListCardFooter>
                </CentralListCard>
              );
            })}
          </CentralList>

          {importPreview ? (
            <div className="mt-5 rounded-lg border border-amber-200 bg-amber-50 p-4 dark:border-amber-900/60 dark:bg-amber-950/40">
              <p className="font-medium">
                发现已有提示词，仅生成了无写入导入预览
              </p>
              <p className="mt-1 text-sm break-all">
                {importPreview.targetPath}
              </p>
              <pre className="bg-card mt-3 max-h-40 overflow-auto rounded p-3 text-xs dark:bg-slate-900/60">
                {importPreview.body}
              </pre>
              <div className="mt-3 flex gap-2">
                <Button
                  size="sm"
                  onClick={() => confirmImportMutation.mutate()}
                >
                  确认无损导入
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setImportPreview(null)}
                >
                  跳过
                </Button>
              </div>
            </div>
          ) : null}
        </section>
      </div>

      {tools.length > 0 ? (
        <section
          className="bg-card mx-auto mt-6 max-w-6xl rounded-xl border p-5"
          aria-labelledby="prompt-target-status-title"
        >
          <h2 id="prompt-target-status-title" className="text-lg font-semibold">
            全局目标状态
          </h2>
          <div className="mt-4 grid gap-3 md:grid-cols-2">
            {tools.map((tool) => {
              const statusQuery = statusQueries[tool];
              const toolLabel = toolMetadata(tool).label;
              return (
                <article key={tool} className="rounded-lg border p-4 text-sm">
                  <div className="flex items-center justify-between gap-2">
                    <strong>{toolLabel}</strong>
                    {statusQuery.data ? (
                      <span className="text-xs">
                        {statusQuery.data.availability === "installed"
                          ? "已检测到"
                          : statusQuery.data.availability === "unavailable"
                            ? "未检测到"
                            : "版本未确认"}
                      </span>
                    ) : null}
                  </div>
                  {statusQuery.isPending ? (
                    <p role="status" className="mt-2 text-xs">
                      正在检测工具配置状态…
                    </p>
                  ) : null}
                  {statusQuery.isError ? (
                    <p
                      role="alert"
                      className="mt-2 text-xs text-red-700 dark:text-red-300"
                    >
                      {profileErrorText(statusQuery.error)}
                    </p>
                  ) : null}
                  {statusQuery.data ? (
                    <>
                      <code className="mt-2 block text-xs break-all">
                        {statusQuery.data.promptTargetPath}
                      </code>
                      <p className="text-muted-foreground mt-2 text-xs">
                        {statusQuery.data.newSessionNotice}
                      </p>
                      {statusQuery.data.promptOverride === "present" ? (
                        <p className="mt-2 text-xs font-medium text-amber-800 dark:text-amber-300">
                          检测到更高优先级的 Codex 指令来源（如
                          AGENTS.override.md）；当前 AGENTS.md 可能被遮蔽。
                        </p>
                      ) : null}
                      {statusQuery.data.promptOverride === "unknown" ? (
                        <p className="mt-2 text-xs font-medium text-amber-800 dark:text-amber-300">
                          无法安全确认 Codex 指令遮蔽状态，请检查
                          AGENTS.override.md 后再应用。
                        </p>
                      ) : null}
                    </>
                  ) : null}
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={discoverMutation.isPending}
                      onClick={() => discoverMutation.mutate(tool)}
                    >
                      检测并导入已有提示词
                    </Button>
                    {!directApply ? (
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={previewMutation.isPending}
                        onClick={() =>
                          previewMutation.mutate({
                            tool,
                            autoApply: directApply,
                          })
                        }
                      >
                        {previewMutation.isPending
                          ? "正在生成…"
                          : `预览 ${toolLabel} 全局同步`}
                      </Button>
                    ) : null}
                  </div>
                </article>
              );
            })}
          </div>
        </section>
      ) : null}

      <FormDialog
        open={formOpen}
        title={`${editing ? "编辑" : "新增"}提示词`}
        description={
          directApply
            ? "保存只更新中央提示词档案；已分配工具会按直接应用模式自动同步。"
            : "保存只更新中央提示词档案，不会修改原生文件；原生写入仍需预览后确认 Apply。"
        }
        submitLabel={editing ? "保存编辑" : "创建提示词"}
        pending={saveMutation.isPending}
        error={profileErrorText(saveMutation.error)}
        onClose={closeForm}
        onSubmit={submit}
      >
        <div>
          <label
            htmlFor="prompt-name"
            className="mb-1 block text-sm font-medium"
          >
            名称
          </label>
          <input
            id="prompt-name"
            required
            className="field"
            value={name}
            onChange={(event) => setName(event.currentTarget.value)}
          />
        </div>
        <div>
          <label
            htmlFor="prompt-body"
            className="mb-1 block text-sm font-medium"
          >
            Markdown 正文
          </label>
          <textarea
            id="prompt-body"
            required
            className="field min-h-44 resize-y font-mono text-sm"
            value={body}
            onChange={(event) => setBody(event.currentTarget.value)}
          />
        </div>
      </FormDialog>

      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? "claude"}
        artifactKind="prompt"
        applying={applyMutation.isPending}
        onClose={() => setOpenPreview(null)}
        onApply={() => {
          if (openPreview) {
            applyMutation.mutate({
              preview: openPreview,
            });
          }
        }}
      />
    </main>
  );
}
