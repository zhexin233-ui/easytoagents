import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Power, PowerOff, Trash2 } from "lucide-react";

import {
  commands,
  type ApplyMcpPreviewInput,
  type JsonValue,
  type McpServerDto,
  type McpServerInput,
  type McpTransport,
  type PreviewPlan,
  type Tool,
  type UpdateMcpServerInput,
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
import { useNotify } from "@/components/use-notify";
import { usePersistedCentralListLayout } from "@/components/use-persisted-central-list-layout";
import {
  globalMcpStatusesQueryOptions,
  mcpKeys,
  mcpServersQueryOptions,
} from "@/lib/mcp-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { MCP_TOOLS, toolMetadata } from "@/lib/tool-metadata";
import { globalTargetStatusPresentation } from "@/lib/global-target-status-ui";
import {
  appSettingsQueryOptions,
  canAutoApplyPreview,
} from "@/lib/settings-api";
import { McpImportDialog } from "@/features/mcp/mcp-import-dialog";

interface McpFormState {
  id: string | null;
  rowVersion: number | null;
  name: string;
  transport: McpTransport;
  command: string;
  args: string;
  url: string;
  headers: string;
  env: string;
  extra: string;
  keepHeaders: boolean;
  keepEnv: boolean;
  keepExtra: boolean;
  enabled: boolean;
}

interface OpenMcpPreview {
  plan: PreviewPlan;
  tool: Tool;
}

interface McpPreviewRequest {
  tool: Tool;
  notifyResult: boolean;
}

interface McpApplyRequest {
  input: ApplyMcpPreviewInput;
  notifyResult: boolean;
}

const emptyForm: McpFormState = {
  id: null,
  rowVersion: null,
  name: "",
  transport: "stdio",
  command: "",
  args: "",
  url: "",
  headers: "{}",
  env: "{}",
  extra: "{}",
  keepHeaders: false,
  keepEnv: false,
  keepExtra: false,
  enabled: true,
};

export function McpPage() {
  const queryClient = useQueryClient();
  const serversQuery = useQuery(mcpServersQueryOptions());
  const statusesQuery = useQuery(globalMcpStatusesQueryOptions());
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const directApply = settingsQuery.data?.applyMode === "direct";
  const [form, setForm] = useState<McpFormState>(emptyForm);
  const [formOpen, setFormOpen] = useState(false);
  const saveInFlight = useRef(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const { notification, notify } = useNotify();
  const [listLayout, setListLayout] = usePersistedCentralListLayout("mcp");
  const [openPreview, setOpenPreview] = useState<OpenMcpPreview | null>(null);
  const [openImport, setOpenImport] = useState<{
    tool: Tool;
    requestId: string;
  } | null>(null);
  const invalidateMcp = async () => {
    await queryClient.invalidateQueries({ queryKey: mcpKeys.all });
  };

  const saveMutation = useMutation({
    mutationFn: async (state: McpFormState) => {
      if (state.id && state.rowVersion !== null) {
        return unwrapResult(await commands.updateMcpServer(updateInput(state)));
      }
      return unwrapResult(await commands.createMcpServer(createInput(state)));
    },
    onSuccess: async () => {
      await invalidateMcp();
      setMessage(
        directApply
          ? "中央 MCP 已保存；原生配置尚未修改。点击「直接应用全局同步」写入。"
          : "中央 MCP 已保存；原生配置尚未修改。请生成预览后再 Apply。",
      );
      setForm(emptyForm);
      setFormError(null);
      setFormOpen(false);
    },
    onSettled: () => {
      saveInFlight.current = false;
    },
  });

  const openForm = (state: McpFormState) => {
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
    mutationFn: async (server: McpServerDto) =>
      unwrapResult(
        await commands.setMcpEnabled(
          { id: server.id, rowVersion: server.rowVersion },
          !server.enabled,
        ),
      ),
    onSuccess: async (_result, server) => {
      await invalidateMcp();
      if (!directApply) return;
      // 启停改变已分配工具的期望投影；逐个工具自动同步，未分配则无需同步。
      for (const tool of server.globalTools) {
        await previewMutation
          .mutateAsync({ tool, notifyResult: true })
          .catch(() => undefined);
      }
    },
  });

  const deleteMutation = useMutation({
    mutationFn: async (server: McpServerDto) =>
      unwrapResult(
        await commands.deleteMcpServer({
          id: server.id,
          rowVersion: server.rowVersion,
        }),
      ),
    onSuccess: async () => {
      setMessage(
        directApply
          ? "中央 MCP 已删除；点击「直接应用全局同步」安全清理旧受管条目。"
          : "中央 MCP 已删除；仍需预览并 Apply 才会安全清理旧受管条目。",
      );
      await invalidateMcp();
    },
  });

  const globalAssignmentMutation = useMutation({
    mutationFn: async ({
      server,
      tool,
    }: {
      server: McpServerDto;
      tool: Tool;
    }) =>
      unwrapResult(
        await commands.setGlobalMcpAssignment({
          tool,
          mcpId: server.id,
          assigned: !server.globalTools.includes(tool),
          rowVersion: server.rowVersion,
        }),
      ),
    onSuccess: async (_result, { tool }) => {
      await invalidateMcp();
      if (directApply) {
        previewMutation.mutate({ tool, notifyResult: true });
      }
    },
  });

  const previewMutation = useMutation({
    mutationFn: async ({ tool }: McpPreviewRequest) => ({
      tool,
      plan: unwrapResult(
        await commands.previewMcpSync({
          tool,
          projectId: null,
          excludeFromGit: false,
        }),
      ),
    }),
    onSuccess: ({ plan, tool }, { notifyResult }) => {
      if (plan.targets.length === 0) {
        const emptyTargetsMessage =
          "暂无启用且已分配到该工具的中央 MCP。已有原生配置可通过“检测并导入已有 MCP”纳入管理，也可先创建并分配 MCP。";
        if (notifyResult) {
          notify({ kind: "success", message: emptyTargetsMessage });
        } else {
          setMessage(emptyTargetsMessage);
        }
        setOpenPreview(null);
        return;
      }
      if (notifyResult && canAutoApplyPreview(plan)) {
        applyMutation.mutate({
          input: {
            previewId: plan.previewId,
            tool,
            projectId: null,
          },
          notifyResult: true,
        });
        return;
      }
      setOpenPreview({ plan, tool });
    },
    onError: (error, { notifyResult }) => {
      if (notifyResult) {
        notify({
          kind: "error",
          message: profileErrorText(error) ?? "生成 MCP 全局预览失败。",
        });
      }
    },
  });

  const applyMutation = useMutation({
    mutationFn: async ({ input }: McpApplyRequest) =>
      unwrapResult(await commands.applyMcpPreview(input)),
    onSuccess: async (result, { notifyResult }) => {
      const successMessage = `已应用 ${result.appliedTargets} 个 MCP 目标，并创建 ${result.snapshotCount} 份快照。`;
      if (notifyResult) {
        notify({ kind: "success", message: successMessage });
      } else {
        setMessage(successMessage);
      }
      setOpenPreview(null);
      await invalidateMcp();
    },
    onError: (error, { notifyResult }) => {
      if (notifyResult) {
        notify({
          kind: "error",
          message: profileErrorText(error) ?? "应用 MCP 全局同步失败。",
        });
      }
    },
  });

  const readoptMutation = useMutation({
    mutationFn: async ({ tool }: { tool: Tool }) =>
      unwrapResult(await commands.readoptMcpTarget({ tool, projectId: null })),
    onSuccess: async (result, { tool }) => {
      setOpenPreview(null);
      setMessage(
        `已以当前内容重新接管（刷新 ${result.updatedItemCount} 个、清理 ${result.removedItemCount} 个条目基线）；正在重新生成预览。`,
      );
      await invalidateMcp();
      previewMutation.mutate({ tool, notifyResult: directApply });
    },
  });

  const previewError = previewMutation.variables?.notifyResult
    ? null
    : previewMutation.error;
  const applyError = applyMutation.variables?.notifyResult
    ? null
    : applyMutation.error;

  const operationError = [
    enabledMutation.error,
    deleteMutation.error,
    globalAssignmentMutation.error,
    previewError,
    applyError,
    readoptMutation.error,
  ]
    .map(profileErrorText)
    .find(Boolean);

  return (
    <main className="p-6 lg:p-8">
      <Notify notification={notification} />
      <header className="mx-auto max-w-6xl">
        <p className="text-muted-foreground text-sm">中央配置库</p>
        <h1 className="mt-1 text-2xl font-semibold">MCP</h1>
        <p className="text-muted-foreground mt-2 max-w-3xl text-sm leading-6">
          MCP 的 CRUD、启停和分配只更新中央意图。header、env
          与识别出的敏感扩展不会从后端回填到普通
          DTO；原生写入必须经过持久化预览。
        </p>
      </header>

      <div className="mx-auto mt-6 max-w-6xl space-y-4" aria-live="polite">
        {message ? (
          <p className="rounded-lg border border-emerald-200 bg-emerald-50 p-4 text-sm dark:border-emerald-900/60 dark:bg-emerald-950/40">
            {message}
          </p>
        ) : null}
        {operationError ? (
          <p
            role="alert"
            className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm dark:border-red-900/60 dark:bg-red-950/40"
          >
            {operationError}
          </p>
        ) : null}
      </div>

      <div className="mx-auto mt-6 max-w-6xl">
        <section
          className="bg-card rounded-xl border p-5"
          aria-labelledby="mcp-list-title"
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 id="mcp-list-title" className="text-lg font-semibold">
              中央列表
            </h2>
            <div className="flex flex-wrap items-center gap-2">
              <CentralListLayoutToggle
                value={listLayout}
                onChange={setListLayout}
              />
              <Button size="sm" onClick={() => openForm(emptyForm)}>
                新增 MCP
              </Button>
            </div>
          </div>
          {serversQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在读取 MCP…
            </p>
          ) : null}
          {serversQuery.isError ? (
            <p
              role="alert"
              className="mt-4 text-sm text-red-700 dark:text-red-300"
            >
              {profileErrorText(serversQuery.error)}
            </p>
          ) : null}
          {serversQuery.data?.length === 0 ? (
            <p className="text-muted-foreground mt-4 text-sm">
              中央库尚无 MCP。点击“新增
              MCP”创建，或通过全局目标中的“检测并导入已有 MCP”纳入已有工具配置。
            </p>
          ) : null}
          <CentralList layout={listLayout}>
            {serversQuery.data?.map((server) => {
              const serverActions = (
                <div className="flex min-w-0 flex-wrap gap-2">
                  <Button
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label="编辑"
                    title="编辑"
                    onClick={() => openForm(editForm(server))}
                  >
                    <Pencil aria-hidden="true" className="size-4" />
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label={server.enabled ? "停用" : "启用"}
                    aria-pressed={server.enabled}
                    title={server.enabled ? "停用" : "启用"}
                    onClick={() => enabledMutation.mutate(server)}
                  >
                    {server.enabled ? (
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
                    onClick={() => deleteMutation.mutate(server)}
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
                  aria-label={`${server.name} 全局平台分配`}
                >
                  {MCP_TOOLS.map((tool) => (
                    <PlatformAssignmentButton
                      key={tool}
                      tool={tool}
                      assigned={server.globalTools.includes(tool)}
                      disabled={globalAssignmentMutation.isPending}
                      onClick={() =>
                        globalAssignmentMutation.mutate({ server, tool })
                      }
                    />
                  ))}
                </div>
              );

              return (
                <CentralListCard key={server.id} layout={listLayout}>
                  <CentralListCardBody layout={listLayout}>
                    <div className="flex min-w-0 flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0">
                        <h3
                          className={
                            listLayout === "grid"
                              ? "truncate font-medium"
                              : "font-medium"
                          }
                          title={server.name}
                        >
                          {server.name}
                        </h3>
                        <p className="text-muted-foreground mt-1 text-xs">
                          {server.transport} ·{" "}
                          {server.enabled ? "已启用" : "已停用"}
                        </p>
                      </div>
                      {serverActions}
                    </div>
                    {listLayout === "list" ? (
                      <>
                        <dl className="mt-3 grid gap-2 text-xs sm:grid-cols-2">
                          <div>
                            <dt className="text-muted-foreground">入口</dt>
                            <dd className="break-all">
                              {server.command ?? server.url}
                            </dd>
                          </div>
                          <div>
                            <dt className="text-muted-foreground">敏感字段</dt>
                            <dd>
                              headers: {server.headerNames.join(", ") || "无"}
                              ；env: {server.envNames.join(", ") || "无"}
                            </dd>
                          </div>
                        </dl>
                        <pre className="bg-muted mt-3 overflow-auto rounded p-2 text-xs">
                          {JSON.stringify(server.redactedExtra, null, 2)}
                        </pre>
                      </>
                    ) : (
                      <div className="mt-4 min-w-0 space-y-3">
                        <div>
                          <p className="text-muted-foreground text-xs">
                            入口摘要
                          </p>
                          <code
                            className="mt-1 line-clamp-2 block text-xs break-all"
                            title={server.command ?? server.url ?? undefined}
                          >
                            {server.command ?? server.url ?? "未配置"}
                          </code>
                        </div>
                        <p className="text-muted-foreground text-xs leading-5">
                          敏感字段{" "}
                          {server.headerNames.length + server.envNames.length}{" "}
                          项 · 扩展信息已脱敏
                        </p>
                      </div>
                    )}
                  </CentralListCardBody>
                  <CentralListCardFooter
                    layout={listLayout}
                    label={`${server.name} 操作`}
                  >
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
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
        aria-labelledby="mcp-target-title"
      >
        <h2 id="mcp-target-title" className="text-lg font-semibold">
          全局目标状态
        </h2>
        {statusesQuery.isPending ? (
          <p role="status" className="mt-3 text-sm">
            正在检测全局 MCP 目标…
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
        <div className="mt-4 grid gap-3 md:grid-cols-2">
          {statusesQuery.data?.map((status) => {
            const presentation = globalTargetStatusPresentation(
              status.status,
              status.diagnosticCode,
            );
            return (
              <article
                key={status.tool}
                className="rounded-lg border p-4 text-sm"
              >
                <p className="font-medium">{toolMetadata(status.tool).label}</p>
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
                  disabled={presentation.previewBlocked}
                  onClick={() => {
                    if (openImport) return;
                    setMessage(null);
                    setOpenImport({
                      tool: status.tool,
                      requestId: crypto.randomUUID(),
                    });
                  }}
                >
                  检测并导入已有 MCP
                </Button>
                <Button
                  className="mt-3"
                  size="sm"
                  disabled={
                    previewMutation.isPending || presentation.previewBlocked
                  }
                  onClick={() =>
                    previewMutation.mutate({
                      tool: status.tool,
                      notifyResult: directApply,
                    })
                  }
                >
                  {previewMutation.isPending
                    ? directApply
                      ? "正在应用…"
                      : "正在生成…"
                    : directApply
                      ? "直接应用全局同步"
                      : "生成全局预览"}
                </Button>
              </article>
            );
          })}
        </div>
      </section>

      <FormDialog
        open={formOpen}
        title={form.id ? "编辑 MCP" : "新增 MCP"}
        description="保存只更新中央 MCP，不会修改原生配置；原生写入仍需预览后确认 Apply。"
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
        <Field label="传输方式">
          <select
            className="field"
            value={form.transport}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                transport:
                  event.target.value === "streamable_http"
                    ? "streamable_http"
                    : "stdio",
              }))
            }
          >
            <option value="stdio">stdio</option>
            <option value="streamable_http">streamable_http</option>
          </select>
        </Field>
        {form.transport === "stdio" ? (
          <>
            <Field label="Command">
              <input
                className="field"
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
            <Field label="Args（每行一项）">
              <textarea
                className="field min-h-24"
                value={form.args}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    args: event.target.value,
                  }))
                }
              />
            </Field>
            <SensitiveField
              label="Env JSON"
              value={form.env}
              keep={form.keepEnv}
              editing={form.id !== null}
              onKeep={(keep) =>
                setForm((current) => ({ ...current, keepEnv: keep }))
              }
              onChange={(env) => setForm((current) => ({ ...current, env }))}
            />
          </>
        ) : (
          <>
            <Field label="URL">
              <input
                className="field"
                type="url"
                value={form.url}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    url: event.target.value,
                  }))
                }
                required
              />
            </Field>
            <SensitiveField
              label="Headers JSON"
              value={form.headers}
              keep={form.keepHeaders}
              editing={form.id !== null}
              onKeep={(keep) =>
                setForm((current) => ({ ...current, keepHeaders: keep }))
              }
              onChange={(headers) =>
                setForm((current) => ({ ...current, headers }))
              }
            />
          </>
        )}
        <div className="space-y-2 text-sm">
          <label htmlFor="mcp-extra-json" className="block font-medium">
            扩展字段 JSON
          </label>
          {form.id ? (
            <label className="mb-2 flex items-center gap-2 text-xs">
              <input
                type="checkbox"
                checked={form.keepExtra}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    keepExtra: event.target.checked,
                  }))
                }
              />
              保持数据库中的扩展字段（不会回填敏感原值）
            </label>
          ) : null}
          <textarea
            id="mcp-extra-json"
            className="field min-h-24 font-mono text-xs"
            value={form.extra}
            disabled={form.id !== null && form.keepExtra}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                extra: event.target.value,
              }))
            }
          />
        </div>
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
        <McpImportDialog
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
            setMessage(
              directApply
                ? `已导入 ${result.createdCount + result.reusedCount} 项 MCP（新建 ${result.createdCount} 项，复用 ${result.reusedCount} 项），已分配到 ${toolMetadata(result.tool).label} 全局。原生配置未改写，可点击「直接应用全局同步」写入。`
                : `已导入 ${result.createdCount + result.reusedCount} 项 MCP（新建 ${result.createdCount} 项，复用 ${result.reusedCount} 项），已分配到 ${toolMetadata(result.tool).label} 全局。原生配置未改写，请单独生成全局预览。`,
            );
            await invalidateMcp();
          }}
        />
      ) : null}

      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? "claude"}
        artifactKind="mcp"
        applying={applyMutation.isPending}
        readopting={readoptMutation.isPending}
        onReadopt={() => {
          if (openPreview) {
            readoptMutation.mutate({ tool: openPreview.tool });
          }
        }}
        onClose={() => setOpenPreview(null)}
        onApply={() => {
          if (openPreview) {
            applyMutation.mutate({
              input: {
                previewId: openPreview.plan.previewId,
                tool: openPreview.tool,
                projectId: null,
              },
              notifyResult: false,
            });
          }
        }}
      />
    </main>
  );
}

function createInput(form: McpFormState): McpServerInput {
  return {
    name: form.name,
    transport: form.transport,
    command: form.transport === "stdio" ? form.command : null,
    args: form.transport === "stdio" ? lines(form.args) : [],
    url: form.transport === "streamable_http" ? form.url : null,
    headers:
      form.transport === "streamable_http"
        ? parseStringMap(form.headers, "Headers")
        : {},
    env: form.transport === "stdio" ? parseStringMap(form.env, "Env") : {},
    extra: parseJsonValue(form.extra, "扩展字段"),
    enabled: form.enabled,
  };
}

function updateInput(form: McpFormState): UpdateMcpServerInput {
  if (!form.id || form.rowVersion === null) {
    throw new Error("编辑记录缺少 row_version。");
  }
  const base = createInput({
    ...form,
    headers: form.headers || "{}",
    env: form.env || "{}",
    extra: form.extra || "{}",
  });
  return {
    id: form.id,
    name: base.name,
    transport: base.transport,
    command: base.command,
    args: base.args,
    url: base.url,
    headers:
      form.transport === "streamable_http"
        ? form.keepHeaders
          ? { action: "keep" }
          : { action: "replace", value: base.headers }
        : { action: "clear" },
    env:
      form.transport === "stdio"
        ? form.keepEnv
          ? { action: "keep" }
          : { action: "replace", value: base.env }
        : { action: "clear" },
    extra: form.keepExtra
      ? { action: "keep" }
      : { action: "replace", value: base.extra },
    enabled: base.enabled,
    rowVersion: form.rowVersion,
  };
}

function editForm(server: McpServerDto): McpFormState {
  return {
    id: server.id,
    rowVersion: server.rowVersion,
    name: server.name,
    transport: server.transport,
    command: server.command ?? "",
    args: server.args.join("\n"),
    url: server.url ?? "",
    headers: "{}",
    env: "{}",
    extra: "{}",
    keepHeaders: true,
    keepEnv: true,
    keepExtra: true,
    enabled: server.enabled,
  };
}

function validateForm(form: McpFormState) {
  if (!form.name.trim()) throw new Error("名称不能为空。");
  if (form.transport === "stdio" && !form.command.trim())
    throw new Error("stdio 必须填写 Command。");
  if (form.transport === "streamable_http" && !form.url.trim())
    throw new Error("streamable_http 必须填写 URL。");
  if (!form.keepHeaders) parseStringMap(form.headers || "{}", "Headers");
  if (!form.keepEnv) parseStringMap(form.env || "{}", "Env");
  if (!form.keepExtra) parseJsonValue(form.extra || "{}", "扩展字段");
}

function lines(value: string): string[] {
  return value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

function parseStringMap(text: string, label: string): Record<string, string> {
  const value = parseJsonValue(text || "{}", label);
  if (!isJsonObject(value)) {
    throw new Error(`${label} 必须是字符串值 JSON 对象。`);
  }
  const result: Record<string, string> = {};
  for (const [key, item] of Object.entries(value)) {
    if (typeof item !== "string") {
      throw new Error(`${label} 必须是字符串值 JSON 对象。`);
    }
    result[key] = item;
  }
  return result;
}

function parseJsonValue(text: string, label: string): JsonValue {
  let value: unknown;
  try {
    value = JSON.parse(text || "{}");
  } catch {
    throw new Error(`${label} 不是合法 JSON。`);
  }
  if (!isJsonValue(value)) throw new Error(`${label} 包含不支持的 JSON 值。`);
  return value;
}

function isJsonValue(value: unknown): value is JsonValue {
  if (value === null || ["boolean", "number", "string"].includes(typeof value))
    return true;
  if (Array.isArray(value)) return value.every(isJsonValue);
  return isJsonObject(value) && Object.values(value).every(isJsonValue);
}

function isJsonObject(value: unknown): value is Record<string, JsonValue> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
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

function SensitiveField({
  label,
  value,
  keep,
  editing,
  onKeep,
  onChange,
}: {
  label: string;
  value: string;
  keep: boolean;
  editing: boolean;
  onKeep: (keep: boolean) => void;
  onChange: (value: string) => void;
}) {
  const id = label === "Headers JSON" ? "mcp-headers-json" : "mcp-env-json";
  return (
    <div className="space-y-2 text-sm">
      <label htmlFor={id} className="block font-medium">
        {label}
      </label>
      {editing ? (
        <label className="flex items-center gap-2 text-xs">
          <input
            type="checkbox"
            checked={keep}
            onChange={(event) => onKeep(event.target.checked)}
          />
          保持数据库中的敏感值（不会回填原值）
        </label>
      ) : null}
      <input
        id={id}
        className="field font-mono text-xs"
        type="password"
        autoComplete="off"
        value={value}
        disabled={editing && keep}
        onChange={(event) => onChange(event.target.value)}
      />
    </div>
  );
}
