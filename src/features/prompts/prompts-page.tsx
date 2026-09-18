import { useState, type FormEvent } from "react";
import {
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { Pencil, Trash2 } from "lucide-react";

import {
  commands,
  type PromptImportPreviewDto,
  type PromptProfileDto,
  type SyncScopeDto,
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
import { FormDialog } from "@/components/form-dialog";
import { PageHeader } from "@/components/page-header";
import { PlatformAssignmentButton } from "@/components/platform-assignment-button";
import { Button } from "@/components/ui/button";
import { useNotify } from "@/components/use-notify";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { usePersistedCentralListLayout } from "@/components/use-persisted-central-list-layout";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { useSubmitGuard } from "@/hooks/use-submit-guard";
import {
  profileErrorText,
  profileKeys,
  promptProfilesQueryOptions,
  toolProfileStatusQueryOptions,
  unwrapResult,
} from "@/lib/profile-api";
import { toneClass } from "@/lib/tone-class";
import {
  PROFILE_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";

function isSyncScope(value: unknown): value is SyncScopeDto {
  if (typeof value !== "object" || value === null) return false;
  if (
    !("artifactKind" in value) ||
    !("tool" in value) ||
    !("projectId" in value)
  ) {
    return false;
  }
  return (
    (value.artifactKind === "provider" ||
      value.artifactKind === "prompt" ||
      value.artifactKind === "mcp" ||
      value.artifactKind === "skill" ||
      value.artifactKind === "hook" ||
      value.artifactKind === "agent") &&
    (value.tool === "claude" ||
      value.tool === "codex" ||
      value.tool === "cursor" ||
      value.tool === "zcode" ||
      value.tool === "opencode" ||
      value.tool === "pi") &&
    (value.projectId === null || typeof value.projectId === "string")
  );
}

/** 中央 mutation 结果附带后端签发的精确同步范围；列表响应没有此字段。 */
function affectedSyncScopes(result: object): SyncScopeDto[] {
  if (
    !("affectedSyncScopes" in result) ||
    !Array.isArray(result.affectedSyncScopes)
  ) {
    return [];
  }
  return result.affectedSyncScopes.filter(isSyncScope);
}

export function PromptsPage() {
  const queryClient = useQueryClient();
  const profilesQuery = useQuery(promptProfilesQueryOptions());
  const enabledTools = useEnabledTools();
  const statusQueries = useQueries({
    queries: PROFILE_TOOLS.map((tool) => ({
      ...toolProfileStatusQueryOptions(tool),
      enabled: enabledTools.has(tool),
    })),
  });
  const statusQueryByTool = new Map(
    PROFILE_TOOLS.map((tool, index) => [tool, statusQueries[index]] as const),
  );
  const tools = filterEnabledTools(PROFILE_TOOLS, enabledTools);
  const [listLayout, setListLayout] = usePersistedCentralListLayout("prompts");
  const [editing, setEditing] = useState<PromptProfileDto | null>(null);
  const [name, setName] = useState("");
  const [body, setBody] = useState("");
  const [formOpen, setFormOpen] = useState(false);
  const submitGuard = useSubmitGuard();
  const { notify } = useNotify();
  const [importPreview, setImportPreview] =
    useState<PromptImportPreviewDto | null>(null);

  const refresh = async () => {
    await queryClient.invalidateQueries({ queryKey: profileKeys.prompts });
  };

  const saveMutation = useMutation<PromptProfileDto, Error, void>({
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
    onSuccess: async (result) => {
      await refresh();
      setEditing(null);
      setName("");
      setBody("");
      setFormOpen(false);
      const scopes = affectedSyncScopes(result);
      if (scopes.length > 0) {
        notify({
          kind: "success",
          message: "中央提示词档案已保存；正在自动同步已分配工具。",
        });
        await requestPreview(scopes);
        return;
      }
      notify({
        kind: "success",
        message: "中央提示词档案已保存；未分配工具不会写入原生配置。",
      });
    },
    onSettled: () => {
      submitGuard.end();
    },
  });

  const openForm = (profile: PromptProfileDto | null) => {
    if (submitGuard.isInFlight() || saveMutation.isPending) return;
    saveMutation.reset();
    setEditing(profile);
    setName(profile?.name ?? "");
    setBody(profile?.body ?? "");
    setFormOpen(true);
  };

  const closeForm = () => {
    if (submitGuard.isInFlight() || saveMutation.isPending) return;
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
    onSuccess: async (result) => {
      await refresh();
      const scopes = affectedSyncScopes(result);
      if (scopes.length > 0) {
        notify({
          kind: "success",
          message: "提示词启用范围已更新；正在自动同步原生配置。",
        });
        await requestPreview(scopes);
        return;
      }
      notify({
        kind: "success",
        message: "提示词启用范围已更新；没有受影响的原生配置。",
      });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新提示词全局启用失败。",
      });
    },
  });

  const { requestPreview } = useSyncPreviewFlow({
    artifactKind: "prompt",
    preview: (tool) => commands.previewPromptSync(tool),
    apply: ({ previewId, tool }) =>
      commands.applyProfilePreview({
        previewId,
        tool,
        artifactKind: "prompt",
      }),
    invalidate: refresh,
    messages: {
      previewFailed: "生成提示词全局预览失败。",
      applyFailed: "应用提示词全局同步失败。",
      applied: (result) =>
        `已应用 ${result.appliedTargets} 个目标，可从快照恢复。`,
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
    onSuccess: async (result) => {
      await refresh();
      const scopes = affectedSyncScopes(result);
      if (scopes.length > 0) {
        notify({
          kind: "success",
          message: "中央提示词已删除；正在自动清理已接管文件。",
        });
        await requestPreview(scopes);
        return;
      }
      notify({
        kind: "success",
        message: "中央提示词已删除；该档案未分配到任何工具，无需清理。",
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
    if (submitGuard.isInFlight() || saveMutation.isPending) return;
    if (!submitGuard.begin()) return;
    saveMutation.mutate();
  };

  return (
    <>
      <PageHeader
        title="全局提示词档案"
        actions={
          <>
            <CentralListLayoutToggle
              value={listLayout}
              onChange={setListLayout}
            />
            <Button onClick={() => openForm(null)}>新增提示词</Button>
          </>
        }
      />
      <main className="space-y-6 px-8 py-6">
        <section
          className="bg-card rounded-lg border p-5"
          aria-labelledby="prompt-list-title"
        >
          <h2 id="prompt-list-title" className="text-[15px] font-semibold">
            中央列表
          </h2>

          {profilesQuery.isPending ? (
            <p role="status" className="text-muted-foreground mt-5 text-sm">
              正在加载提示词档案…
            </p>
          ) : null}
          {profilesQuery.isError ? (
            <p role="alert" className="text-destructive mt-5 text-sm">
              {profileErrorText(profilesQuery.error)}
            </p>
          ) : null}
          {profilesQuery.data?.length === 0 ? (
            <div className="mt-5">
              <EmptyState
                title="尚无提示词"
                description="可新增或从工具导入。"
              />
            </div>
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
                      <p className="bg-muted rounded-control mt-3 line-clamp-6 p-2 text-xs leading-5 whitespace-pre-wrap">
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
                  </CentralListCardFooter>
                </CentralListCard>
              );
            })}
          </CentralList>

          {importPreview ? (
            <div
              className={`mt-5 rounded-lg border p-4 ${toneClass("warning")}`}
            >
              <p className="font-medium">发现已有提示词</p>
              <p className="mt-1 text-sm break-all">
                {importPreview.targetPath}
              </p>
              <pre className="bg-card rounded-control mt-3 max-h-40 overflow-auto p-3 text-xs">
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

        {tools.length > 0 ? (
          <section
            className="bg-card rounded-lg border p-5"
            aria-labelledby="prompt-target-status-title"
          >
            <h2
              id="prompt-target-status-title"
              className="text-[15px] font-semibold"
            >
              全局目标状态
            </h2>
            <div className="mt-3 divide-y">
              {tools.map((tool) => {
                const statusQuery = statusQueryByTool.get(tool);
                if (!statusQuery) return null;
                const toolLabel = toolMetadata(tool).label;
                return (
                  <article
                    key={tool}
                    className="hover:bg-muted/50 px-1 py-2.5 text-sm"
                  >
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
                      <p role="alert" className="text-destructive mt-2 text-xs">
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
                          <p className="text-warning mt-2 text-xs font-medium">
                            检测到更高优先级的 Codex 指令来源（如
                            AGENTS.override.md）；当前 AGENTS.md 可能被遮蔽。
                          </p>
                        ) : null}
                        {statusQuery.data.promptOverride === "unknown" ? (
                          <p className="text-warning mt-2 text-xs font-medium">
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
          description="保存中央提示词档案后，已分配工具会自动同步；未分配工具不会写入原生配置。"
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
      </main>
    </>
  );
}
