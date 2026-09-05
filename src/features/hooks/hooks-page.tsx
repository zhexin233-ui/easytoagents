import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Power, PowerOff, Trash2 } from "lucide-react";

import {
  commands,
  type ApplyHookPreviewInput,
  type HookDto,
  type HookEvent,
  type PreviewPlan,
  type Tool,
  type UpdateHookInput,
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
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { useNotify } from "@/components/use-notify";
import { usePersistedCentralListLayout } from "@/components/use-persisted-central-list-layout";
import { globalTargetStatusPresentation } from "@/lib/global-target-status-ui";
import {
  globalHookStatusesQueryOptions,
  hooksKeys,
  hooksQueryOptions,
} from "@/lib/hooks-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import {
  HOOK_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";
import {
  appSettingsQueryOptions,
  canAutoApplyPreview,
} from "@/lib/settings-api";
import { HookImportDialog } from "@/features/hooks/hook-import-dialog";

interface HookFormState {
  id: string | null;
  rowVersion: number | null;
  name: string;
  event: HookEvent;
  matcher: string;
  command: string;
  timeout: string;
  enabled: boolean;
}

interface OpenHookPreview {
  plan: PreviewPlan;
  tool: Tool;
}

interface HookPreviewRequest {
  tool: Tool;
  autoApply: boolean;
}

interface HookApplyRequest {
  input: ApplyHookPreviewInput;
}

const emptyForm: HookFormState = {
  id: null,
  rowVersion: null,
  name: "",
  event: "PreToolUse",
  matcher: "",
  command: "",
  timeout: "",
  enabled: true,
};

const HOOK_EVENT_OPTIONS: HookEvent[] = [
  "SessionStart",
  "SessionEnd",
  "UserPromptSubmit",
  "PreToolUse",
  "PermissionRequest",
  "PostToolUse",
  "PostToolUseFailure",
  "SubagentStart",
  "SubagentStop",
  "PreCompact",
  "PostCompact",
  "Stop",
  "Notification",
];

export function HooksPage() {
  const queryClient = useQueryClient();
  const hooksQuery = useQuery(hooksQueryOptions());
  const statusesQuery = useQuery(globalHookStatusesQueryOptions());
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const directApply = settingsQuery.data?.applyMode === "direct";
  const enabledTools = useEnabledTools();
  const visibleStatuses = statusesQuery.data?.filter((status) =>
    enabledTools.has(status.tool),
  );
  const [form, setForm] = useState<HookFormState>(emptyForm);
  const [formOpen, setFormOpen] = useState(false);
  const saveInFlight = useRef(false);
  const [formError, setFormError] = useState<string | null>(null);
  const { notification, notify } = useNotify();
  const [listLayout, setListLayout] = usePersistedCentralListLayout("hooks");
  const [openPreview, setOpenPreview] = useState<OpenHookPreview | null>(null);
  const [openImport, setOpenImport] = useState<{
    tool: Tool;
    requestId: string;
  } | null>(null);
  const invalidateHooks = async () => {
    await queryClient.invalidateQueries({ queryKey: hooksKeys.all });
  };

  const saveMutation = useMutation({
    mutationFn: async (state: HookFormState) => {
      if (state.id && state.rowVersion !== null) {
        return unwrapResult(await commands.updateHook(updateInput(state)));
      }
      return unwrapResult(await commands.createHook(createInput(state)));
    },
    onSuccess: async () => {
      await invalidateHooks();
      setForm(emptyForm);
      setFormError(null);
      setFormOpen(false);
      notify({
        kind: "success",
        message: directApply
          ? "中央 Hook 已保存；已分配工具会在下次同步时生效。"
          : "中央 Hook 已保存；原生配置尚未修改。请分配工具并生成预览后再 Apply。",
      });
    },
    onSettled: () => {
      saveInFlight.current = false;
    },
  });

  const openForm = (state: HookFormState) => {
    if (saveInFlight.current || saveMutation.isPending) return;
    saveMutation.reset();
    setFormError(null);
    setForm(state);
    setFormOpen(true);
  };

  const closeForm = () => {
    if (saveInFlight.current || saveMutation.isPending) return;
    setFormOpen(false);
    setForm(emptyForm);
    setFormError(null);
    saveMutation.reset();
  };

  const enabledMutation = useMutation({
    mutationFn: async (hook: HookDto) =>
      unwrapResult(
        await commands.setHookEnabled(
          { id: hook.id, rowVersion: hook.rowVersion },
          !hook.enabled,
        ),
      ),
    onSuccess: async () => {
      await invalidateHooks();
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新 Hook 启停状态失败。",
      });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: async (hook: HookDto) =>
      unwrapResult(
        await commands.deleteHook({
          id: hook.id,
          rowVersion: hook.rowVersion,
        }),
      ),
    onSuccess: async () => {
      await invalidateHooks();
      notify({
        kind: "success",
        message: directApply
          ? "中央 Hook 已删除；已分配工具会在下次同步时清理条目。"
          : "中央 Hook 已删除；仍需预览并 Apply 才会安全清理旧受管条目。",
      });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "删除中央 Hook 失败。",
      });
    },
  });

  const globalAssignmentMutation = useMutation({
    mutationFn: async ({ hook, tool }: { hook: HookDto; tool: Tool }) =>
      unwrapResult(
        await commands.setGlobalHookAssignment({
          tool,
          hookId: hook.id,
          assigned: !hook.globalTools.includes(tool),
          rowVersion: hook.rowVersion,
        }),
      ),
    onSuccess: async () => {
      await invalidateHooks();
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新 Hook 全局分配失败。",
      });
    },
  });

  const previewMutation = useMutation({
    mutationFn: async ({ tool }: HookPreviewRequest) => ({
      tool,
      plan: unwrapResult(
        await commands.previewHookSync({
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
          message:
            "暂无启用且已分配到该工具的中央 Hook。已有原生配置可通过“检测并导入已有 Hooks”纳入中央库，也可先创建并分配 Hook。",
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
        message: profileErrorText(error) ?? "生成 Hooks 全局预览失败。",
      });
    },
  });

  const applyMutation = useMutation({
    mutationFn: async ({ input }: HookApplyRequest) =>
      unwrapResult(await commands.applyHookPreview(input)),
    onSuccess: async (result) => {
      const successMessage = `已应用 ${result.appliedTargets} 个 Hooks 目标，并创建 ${result.snapshotCount} 份快照。`;
      setOpenPreview(null);
      await invalidateHooks();
      notify({ kind: "success", message: successMessage });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "应用 Hooks 全局同步失败。",
      });
    },
  });

  const readoptMutation = useMutation({
    mutationFn: async ({ tool }: { tool: Tool }) =>
      unwrapResult(await commands.readoptHookTarget({ tool, projectId: null })),
    onSuccess: async (result, { tool }) => {
      setOpenPreview(null);
      await invalidateHooks();
      notify({
        kind: "success",
        message: `已以当前内容重新接管（刷新 ${result.updatedItemCount} 个、清理 ${result.removedItemCount} 个条目基线）；正在重新生成预览。`,
      });
      previewMutation.mutate({ tool, autoApply: directApply });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "重新接管 Hooks 目标失败。",
      });
    },
  });

  return (
    <main className="p-6 lg:p-8">
      <Notify notification={notification} />
      <header className="mx-auto max-w-6xl">
        <p className="text-muted-foreground text-sm">中央配置库</p>
        <h1 className="mt-1 text-2xl font-semibold">Hooks</h1>
        <p className="text-muted-foreground mt-2 max-w-3xl text-sm leading-6">
          生命周期钩子的 CRUD、启停和分配只更新中央意图。四工具的 hooks
          原生合同不同（Claude/ZCode 为 settings/config 内的 hooks
          子树，Codex/Cursor 为独立 hooks.json，Cursor 事件为
          camelCase）；不兼容的事件组合会被拒绝，原生写入必须经过持久化预览。
        </p>
      </header>

      <div className="mx-auto mt-6 max-w-6xl">
        <section
          className="bg-card rounded-xl border p-5"
          aria-labelledby="hooks-list-title"
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 id="hooks-list-title" className="text-lg font-semibold">
              中央列表
            </h2>
            <div className="flex flex-wrap items-center gap-2">
              <CentralListLayoutToggle
                value={listLayout}
                onChange={setListLayout}
              />
              <Button size="sm" onClick={() => openForm(emptyForm)}>
                新增 Hook
              </Button>
            </div>
          </div>
          {hooksQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在读取 Hooks…
            </p>
          ) : null}
          {hooksQuery.isError ? (
            <p
              role="alert"
              className="mt-4 text-sm text-red-700 dark:text-red-300"
            >
              {profileErrorText(hooksQuery.error)}
            </p>
          ) : null}
          {hooksQuery.data?.length === 0 ? (
            <p className="text-muted-foreground mt-4 text-sm">
              中央库尚无 Hook。点击“新增
              Hook”创建，或通过全局目标中的“检测并导入已有
              Hooks”纳入已有工具配置。
            </p>
          ) : null}
          <CentralList layout={listLayout}>
            {hooksQuery.data?.map((hook) => {
              const hookActions = (
                <div className="flex min-w-0 flex-wrap gap-2">
                  <Button
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label="编辑"
                    title="编辑"
                    onClick={() => openForm(editForm(hook))}
                  >
                    <Pencil aria-hidden="true" className="size-4" />
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label={hook.enabled ? "停用" : "启用"}
                    aria-pressed={hook.enabled}
                    title={hook.enabled ? "停用" : "启用"}
                    onClick={() => enabledMutation.mutate(hook)}
                  >
                    {hook.enabled ? (
                      <PowerOff aria-hidden="true" className="size-4" />
                    ) : (
                      <Power aria-hidden="true" className="size-4" />
                    )}
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label="删除"
                    title="删除"
                    onClick={() => deleteMutation.mutate(hook)}
                  >
                    <Trash2 aria-hidden="true" className="size-4" />
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
                  aria-label={`${hook.name} 全局平台分配`}
                >
                  {filterEnabledTools(HOOK_TOOLS, enabledTools).map((tool) => {
                    const eventSupported = hookEventSupportedByTool(tool, hook);
                    return (
                      <PlatformAssignmentButton
                        key={tool}
                        tool={tool}
                        assigned={hook.globalTools.includes(tool)}
                        disabled={
                          !eventSupported || globalAssignmentMutation.isPending
                        }
                        onClick={() =>
                          globalAssignmentMutation.mutate({ hook, tool })
                        }
                      />
                    );
                  })}
                </div>
              );

              return (
                <CentralListCard key={hook.id} layout={listLayout}>
                  <CentralListCardBody layout={listLayout}>
                    <div className="flex min-w-0 flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0">
                        <h3
                          className={
                            listLayout === "grid"
                              ? "truncate font-medium"
                              : "font-medium"
                          }
                          title={hook.name}
                        >
                          {hook.name}
                        </h3>
                        <p className="text-muted-foreground mt-1 text-xs">
                          {hook.event} · {hook.enabled ? "已启用" : "已停用"}
                        </p>
                      </div>
                      {listLayout === "list" ? hookActions : null}
                    </div>
                    {listLayout === "list" ? (
                      <dl className="mt-3 grid gap-2 text-xs sm:grid-cols-2">
                        <div>
                          <dt className="text-muted-foreground">命令</dt>
                          <dd className="break-all">{hook.command}</dd>
                        </div>
                        <div>
                          <dt className="text-muted-foreground">匹配器</dt>
                          <dd className="break-all">
                            {hook.matcher ?? "匹配全部"}
                            {hook.timeoutSeconds
                              ? ` · 超时 ${hook.timeoutSeconds}s`
                              : ""}
                          </dd>
                        </div>
                      </dl>
                    ) : (
                      <div className="mt-4 min-w-0 space-y-3">
                        <p className="text-muted-foreground text-xs">
                          命令摘要
                        </p>
                        <code
                          className="mt-1 line-clamp-2 block text-xs break-all"
                          title={hook.command}
                        >
                          {hook.command}
                        </code>
                      </div>
                    )}
                  </CentralListCardBody>
                  <CentralListCardFooter
                    layout={listLayout}
                    label={`${hook.name} 操作`}
                  >
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      {listLayout === "grid" ? hookActions : null}
                      {platformActions}
                    </div>
                  </CentralListCardFooter>
                </CentralListCard>
              );
            })}
          </CentralList>
        </section>
      </div>

      <section
        className="bg-card mx-auto mt-6 max-w-6xl rounded-xl border p-5"
        aria-labelledby="hooks-target-title"
      >
        <h2 id="hooks-target-title" className="text-lg font-semibold">
          全局目标状态
        </h2>
        {statusesQuery.isPending ? (
          <p role="status" className="mt-3 text-sm">
            正在检测全局 Hooks 目标…
          </p>
        ) : null}
        {statusesQuery.isError ? (
          <p
            role="alert"
            className="mt-3 text-sm text-red-700 dark:text-red-300"
          >
            {profileErrorText(statusesQuery.error)}
          </p>
        ) : null}
        {statusesQuery.data != null && visibleStatuses?.length === 0 ? (
          <p className="text-muted-foreground mt-3 text-sm">
            当前没有可检查的全局 Hooks 目标。
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
                  <p className="font-medium">
                    {toolMetadata(status.tool).label}
                  </p>
                  <code className="mt-2 block text-xs break-all">
                    {status.targetPath ?? "目标位置未经 capability probe 证明"}
                  </code>
                  <div className="mt-2">
                    <SyncStatusBadge
                      label={presentation.label}
                      status={status.status}
                      tone={presentation.tone}
                    />
                  </div>
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
                  <Button
                    className="mt-3 mr-2"
                    size="sm"
                    variant="outline"
                    onClick={() => {
                      if (openImport) return;
                      setOpenImport({
                        tool: status.tool,
                        requestId: crypto.randomUUID(),
                      });
                    }}
                  >
                    检测并导入已有 Hooks
                  </Button>
                  {!directApply ? (
                    <Button
                      className="mt-3"
                      size="sm"
                      disabled={
                        previewMutation.isPending || presentation.previewBlocked
                      }
                      onClick={() =>
                        previewMutation.mutate({
                          tool: status.tool,
                          autoApply: directApply,
                        })
                      }
                    >
                      {previewMutation.isPending ? "正在生成…" : "生成全局预览"}
                    </Button>
                  ) : null}
                </article>
              );
            })}
          </div>
        ) : null}
      </section>

      <FormDialog
        open={formOpen}
        title={form.id ? "编辑 Hook" : "新增 Hook"}
        description={
          directApply
            ? "保存只更新中央 Hook；已分配工具会按直接应用模式自动同步。"
            : "保存只更新中央 Hook，不会修改原生配置；原生写入仍需预览后确认 Apply。"
        }
        submitLabel="保存中央意图"
        pending={saveMutation.isPending}
        error={formError ?? profileErrorText(saveMutation.error)}
        onClose={closeForm}
        onSubmit={(event) => {
          event.preventDefault();
          if (saveInFlight.current || saveMutation.isPending) return;
          setFormError(null);
          saveMutation.reset();
          try {
            validateForm(form);
            saveInFlight.current = true;
            saveMutation.mutate(form);
          } catch (error) {
            setFormError(
              error instanceof Error ? error.message : "表单内容无效。",
            );
          }
        }}
      >
        <Field label="名称">
          <input
            className="field"
            value={form.name}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                name: event.target.value,
              }))
            }
            required
          />
        </Field>
        <Field label="事件">
          <select
            className="field"
            value={form.event}
            onChange={(event) => {
              const next = event.target.value;
              setForm((current) => ({
                ...current,
                event: isHookEvent(next) ? next : current.event,
              }));
            }}
          >
            {HOOK_EVENT_OPTIONS.map((event) => (
              <option key={event} value={event}>
                {event}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Matcher（可选，正则或文本）">
          <input
            className="field font-mono text-xs"
            value={form.matcher}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                matcher: event.target.value,
              }))
            }
            placeholder="留空匹配全部，例如 Bash|Write"
          />
        </Field>
        <Field label="命令">
          <input
            className="field font-mono text-xs"
            value={form.command}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                command: event.target.value,
              }))
            }
            required
          />
        </Field>
        <Field label="超时秒数（可选，1–3600）">
          <input
            className="field"
            type="number"
            min={1}
            max={3600}
            value={form.timeout}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                timeout: event.target.value,
              }))
            }
          />
        </Field>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={form.enabled}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                enabled: event.target.checked,
              }))
            }
          />
          启用（停用后下一份预览会安全移除已应用条目）
        </label>
      </FormDialog>

      {openImport ? (
        <HookImportDialog
          key={openImport.requestId}
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
            setOpenImport(null);
            await invalidateHooks();
            notify({
              kind: "success",
              message: `已导入 ${result.createdCount} 个 Hook 到中央库；分配后请生成全局预览。`,
            });
          }}
        />
      ) : null}

      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? "claude"}
        artifactKind="hook"
        applying={applyMutation.isPending}
        readopting={readoptMutation.isPending}
        onReadopt={() => {
          if (openPreview) {
            readoptMutation.mutate({ tool: openPreview.tool });
          }
        }}
        onClose={() => setOpenPreview(null)}
        onApply={(previewId, tool) => {
          applyMutation.mutate({
            input: { previewId, tool, projectId: null },
          });
        }}
      />
    </main>
  );
}

/// 前端侧事件支持矩阵（与后端 HookEvent::supported_for_tool 同一口径），
/// 用于禁用不兼容工具的分配按钮。
const HOOK_EVENT_SET: ReadonlySet<string> = new Set(HOOK_EVENT_OPTIONS);

function isHookEvent(value: string): value is HookEvent {
  return HOOK_EVENT_SET.has(value);
}

function hookEventSupportedByTool(tool: Tool, hook: HookDto): boolean {
  const supported: Record<Tool, HookEvent[]> = {
    claude: [
      "SessionStart",
      "SessionEnd",
      "UserPromptSubmit",
      "PreToolUse",
      "PermissionRequest",
      "PostToolUse",
      "Notification",
      "SubagentStop",
      "Stop",
      "PreCompact",
    ],
    codex: [
      "SessionStart",
      "SessionEnd",
      "UserPromptSubmit",
      "PreToolUse",
      "PermissionRequest",
      "PostToolUse",
      "PreCompact",
      "PostCompact",
      "SubagentStart",
      "SubagentStop",
      "Stop",
    ],
    cursor: [
      "SessionStart",
      "SessionEnd",
      "PreToolUse",
      "PostToolUse",
      "PostToolUseFailure",
      "SubagentStart",
      "SubagentStop",
      "PreCompact",
      "Stop",
    ],
    zcode: [
      "SessionStart",
      "UserPromptSubmit",
      "PreToolUse",
      "PermissionRequest",
      "PostToolUse",
      "PostToolUseFailure",
      "Stop",
    ],
  };
  return supported[tool].includes(hook.event);
}

function createInput(form: HookFormState) {
  return {
    name: form.name,
    event: form.event,
    matcher: form.matcher.trim() ? form.matcher.trim() : null,
    command: form.command,
    timeoutSeconds: parseTimeout(form.timeout),
    enabled: form.enabled,
  };
}

function updateInput(form: HookFormState): UpdateHookInput {
  if (!form.id || form.rowVersion === null) {
    throw new Error("编辑记录缺少 row_version。");
  }
  const base = createInput(form);
  return {
    id: form.id,
    name: base.name,
    event: base.event,
    matcher: base.matcher,
    command: base.command,
    timeoutSeconds: base.timeoutSeconds,
    enabled: base.enabled,
    rowVersion: form.rowVersion,
  };
}

function editForm(hook: HookDto): HookFormState {
  return {
    id: hook.id,
    rowVersion: hook.rowVersion,
    name: hook.name,
    event: hook.event,
    matcher: hook.matcher ?? "",
    command: hook.command,
    timeout: hook.timeoutSeconds ? String(hook.timeoutSeconds) : "",
    enabled: hook.enabled,
  };
}

function validateForm(form: HookFormState) {
  if (!form.name.trim()) throw new Error("名称不能为空。");
  if (!form.command.trim()) throw new Error("命令不能为空。");
  parseTimeout(form.timeout);
}

function parseTimeout(text: string): number | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const value = Number(trimmed);
  if (!Number.isInteger(value) || value <= 0 || value > 3600) {
    throw new Error("超时秒数必须是 1–3600 的整数，或留空。");
  }
  return value;
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block space-y-2 text-sm">
      <span className="font-medium">{label}</span>
      {children}
    </label>
  );
}
