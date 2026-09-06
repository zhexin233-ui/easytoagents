import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Power, PowerOff, Trash2 } from "lucide-react";

import {
  commands,
  type ApplyHookPreviewInput,
  type HookDto,
  type HookEvent,
  type PreviewPlan,
  type SyncStatus,
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
import { HookAssignmentPickerDialog } from "@/features/hooks/hook-assignment-picker-dialog";

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

/// 事件分组（分类标题 → 事件 + 中文名）；只向各工具展示其官方支持的事件。
const HOOK_EVENT_GROUPS: {
  label: string;
  events: { event: HookEvent; label: string }[];
}[] = [
  {
    label: "会话生命周期",
    events: [
      { event: "SessionStart", label: "会话开始" },
      { event: "SessionEnd", label: "会话结束" },
      { event: "Stop", label: "会话停止" },
    ],
  },
  {
    label: "提示词与通知",
    events: [
      { event: "UserPromptSubmit", label: "提示词提交" },
      { event: "Notification", label: "通知" },
    ],
  },
  {
    label: "工具调用",
    events: [
      { event: "PreToolUse", label: "工具调用前" },
      { event: "PermissionRequest", label: "权限请求" },
      { event: "PostToolUse", label: "工具调用后" },
      { event: "PostToolUseFailure", label: "工具调用失败" },
    ],
  },
  {
    label: "子代理",
    events: [
      { event: "SubagentStart", label: "子代理启动" },
      { event: "SubagentStop", label: "子代理结束" },
    ],
  },
  {
    label: "上下文压缩",
    events: [
      { event: "PreCompact", label: "压缩前" },
      { event: "PostCompact", label: "压缩后" },
    ],
  },
];

export function HooksPage() {
  const queryClient = useQueryClient();
  const hooksQuery = useQuery(hooksQueryOptions());
  const statusesQuery = useQuery(globalHookStatusesQueryOptions());
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const directApply = settingsQuery.data?.applyMode === "direct";
  const enabledTools = useEnabledTools();
  const visibleTools = filterEnabledTools(HOOK_TOOLS, enabledTools);
  const [activeTool, setActiveTool] = useState<Tool>("claude");
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
  const [openPicker, setOpenPicker] = useState<{
    tool: Tool;
    event: HookEvent;
    eventLabel: string;
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
          ? "中央 Hook 已保存；在下方事件分组添加后自动同步。"
          : "中央 Hook 已保存；在下方事件分组添加后生成预览再 Apply。",
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

  const assignmentMutation = useMutation({
    mutationFn: async ({
      hook,
      tool,
      event,
      assigned,
    }: {
      hook: HookDto;
      tool: Tool;
      event: HookEvent;
      assigned: boolean;
    }) =>
      unwrapResult(
        await commands.setGlobalHookAssignment({
          tool,
          hookId: hook.id,
          event,
          assigned,
          rowVersion: hook.rowVersion,
        }),
      ),
    onSuccess: async () => {
      await invalidateHooks();
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新 Hook 分配失败。",
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
            "暂无启用且已分配到该工具的中央 Hook。可先在事件分组中添加，或通过“检测并导入已有 Hooks”纳入已有配置。",
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

  const toolStatus = statusesQuery.data?.find(
    (status) => status.tool === activeTool,
  );
  const toolPresentation = toolStatus
    ? globalTargetStatusPresentation(
        toolStatus.status,
        toolStatus.diagnosticCode,
        { directApply },
      )
    : undefined;

  return (
    <main className="p-6 lg:p-8">
      <Notify notification={notification} />
      <header className="mx-auto max-w-6xl">
        <p className="text-muted-foreground text-sm">中央配置库</p>
        <h1 className="mt-1 text-2xl font-semibold">Hooks</h1>
        <p className="text-muted-foreground mt-2 max-w-3xl text-sm leading-6">
          中央 Hook 不绑定单一事件：在下方选择工具，把中央 Hook
          添加到具体的事件分组（同一 Hook 在不同工具可以使用不同事件）。
          分配只更新中央意图，原生写入必须经过持久化预览。
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
              Hook”创建，或在下方工具事件分组中“检测并导入已有 Hooks”。
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
              const assignmentSummary =
                hook.globalAssignments.length === 0
                  ? "未分配"
                  : hook.globalAssignments
                      .map(
                        (assignment) =>
                          `${toolMetadata(assignment.tool).label} · ${assignment.event}`,
                      )
                      .join("，");
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
                          默认 {hook.event} ·{" "}
                          {hook.enabled ? "已启用" : "已停用"}
                          {hook.scriptName ? " · 受管脚本" : ""}
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
                        {hook.scriptName ? (
                          <div>
                            <dt className="text-muted-foreground">受管脚本</dt>
                            <dd className="break-all">{hook.scriptName}</dd>
                          </div>
                        ) : null}
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
                      <p className="text-muted-foreground text-xs">
                        全局生效：{assignmentSummary}
                      </p>
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
          工具事件分组
        </h2>
        <div
          className="mt-3 flex items-center gap-2"
          role="group"
          aria-label="Hooks 工具视图"
        >
          {visibleTools.map((tool) => (
            <Button
              key={tool}
              type="button"
              size="sm"
              variant={activeTool === tool ? "default" : "outline"}
              aria-label={`查看 ${toolMetadata(tool).label} Hooks`}
              aria-pressed={activeTool === tool}
              onClick={() => setActiveTool(tool)}
            >
              {toolMetadata(tool).label}
            </Button>
          ))}
        </div>
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
        {toolStatus ? (
          <article className="mt-4 rounded-lg border p-4 text-sm">
            <p className="font-medium">{toolMetadata(activeTool).label}</p>
            <code className="mt-2 block text-xs break-all">
              {toolStatus.targetPath ?? "目标位置未经 capability probe 证明"}
            </code>
            <div className="mt-2">
              <SyncStatusBadge
                label={toolPresentation?.label}
                status={toolStatus.status}
                tone={toolPresentation?.tone}
              />
            </div>
            {toolPresentation?.description ? (
              <p className="text-muted-foreground mt-2 text-xs">
                {toolPresentation.description}
              </p>
            ) : null}
            {toolStatus.diagnosticCode ? (
              <p className="mt-2 text-xs text-amber-800 dark:text-amber-300">
                诊断码：<code>{toolStatus.diagnosticCode}</code>
              </p>
            ) : null}
            <Button
              className="mt-3 mr-2"
              size="sm"
              variant="outline"
              onClick={() => {
                if (openImport) return;
                setOpenImport({
                  tool: activeTool,
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
                  previewMutation.isPending || toolPresentation?.previewBlocked
                }
                onClick={() =>
                  previewMutation.mutate({
                    tool: activeTool,
                    autoApply: directApply,
                  })
                }
              >
                {previewMutation.isPending ? "正在生成…" : "生成全局预览"}
              </Button>
            ) : null}
          </article>
        ) : null}
        <div className="mt-5 space-y-5">
          {HOOK_EVENT_GROUPS.map((group) => {
            const supportedEvents = group.events.filter((item) =>
              hookEventSupportedByTool(activeTool, item.event),
            );
            if (supportedEvents.length === 0) return null;
            return (
              <div key={group.label}>
                <h3 className="text-sm font-semibold text-slate-500 dark:text-slate-400">
                  {group.label}
                </h3>
                <div className="mt-2 space-y-3">
                  {supportedEvents.map(({ event, label }) => {
                    const assigned = (hooksQuery.data ?? []).filter((hook) =>
                      hook.globalAssignments.some(
                        (assignment) =>
                          assignment.tool === activeTool &&
                          assignment.event === event,
                      ),
                    );
                    return (
                      <article
                        key={event}
                        className="rounded-lg border p-4 text-sm"
                        aria-label={`${label}（${event}）分组`}
                      >
                        <div className="flex items-center justify-between gap-3">
                          <p className="font-medium">
                            {label}
                            <span className="text-muted-foreground ml-2 text-xs">
                              {event}
                            </span>
                          </p>
                          <Button
                            size="sm"
                            variant="outline"
                            aria-label={`往 ${label} 分组添加 Hook`}
                            onClick={() =>
                              setOpenPicker({
                                tool: activeTool,
                                event,
                                eventLabel: label,
                              })
                            }
                          >
                            从中央列表添加
                          </Button>
                        </div>
                        {assigned.length === 0 ? (
                          <p className="text-muted-foreground mt-2 text-xs">
                            该分组暂无 Hook。
                          </p>
                        ) : (
                          <ul className="mt-3 space-y-2">
                            {assigned.map((hook) => (
                              <li
                                key={hook.id}
                                className="flex items-center justify-between gap-3 rounded border bg-slate-50 px-3 py-2 text-xs dark:bg-slate-900"
                              >
                                <span
                                  className="min-w-0 truncate"
                                  title={hook.command}
                                >
                                  {hook.name}
                                  {!hook.enabled ? "（已停用）" : ""}
                                  <span className="text-muted-foreground ml-2 break-all">
                                    {hook.command}
                                  </span>
                                </span>
                                <Button
                                  size="sm"
                                  variant="outline"
                                  aria-label={`从 ${label} 分组移除 ${hook.name}`}
                                  disabled={assignmentMutation.isPending}
                                  onClick={() =>
                                    assignmentMutation.mutate({
                                      hook,
                                      tool: activeTool,
                                      event,
                                      assigned: false,
                                    })
                                  }
                                >
                                  移除
                                </Button>
                              </li>
                            ))}
                          </ul>
                        )}
                      </article>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      </section>

      <FormDialog
        open={formOpen}
        title={form.id ? "编辑 Hook" : "新增 Hook"}
        description={
          directApply
            ? "保存只更新中央 Hook；在事件分组添加后会按直接应用模式自动同步。"
            : "保存只更新中央 Hook，不会修改原生配置；事件在分配时选择。"
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
        <Field label="默认事件（添加到分组时的预选项）">
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
              message: `已导入 ${result.createdCount} 个 Hook 到中央库；在事件分组中添加后生成全局预览。`,
            });
          }}
        />
      ) : null}

      {openPicker ? (
        <HookAssignmentPickerDialog
          tool={openPicker.tool}
          event={openPicker.event}
          eventLabel={openPicker.eventLabel}
          hooks={hooksQuery.data ?? []}
          onClose={() => setOpenPicker(null)}
          onAssigned={(message) => {
            setOpenPicker(null);
            notify({ kind: "success", message });
            if (directApply) {
              previewMutation.mutate({ tool: activeTool, autoApply: true });
            }
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

const HOOK_EVENT_SET: ReadonlySet<string> = new Set(HOOK_EVENT_OPTIONS);

function isHookEvent(value: string): value is HookEvent {
  return HOOK_EVENT_SET.has(value);
}

/// 前端侧事件支持矩阵（与后端 HookEvent::supported_for_tool 同一口径），
/// 用于过滤每个工具可见的事件分组。
function hookEventSupportedByTool(tool: Tool, event: HookEvent): boolean {
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
  return supported[tool].includes(event);
}

function createInput(form: HookFormState) {
  return {
    name: form.name,
    event: form.event,
    matcher: form.matcher.trim() ? form.matcher.trim() : null,
    command: form.command,
    timeoutSeconds: parseTimeout(form.timeout),
    enabled: form.enabled,
    // 手动新增不做脚本接管；接管仅来自导入流程。
    scriptSourcePath: null,
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
