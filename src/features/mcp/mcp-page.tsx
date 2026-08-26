import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type ApplyMcpPreviewInput,
  type JsonValue,
  type McpServerDto,
  type McpServerInput,
  type McpTransport,
  type PreviewPlan,
  type SetProjectMcpAssignmentInput,
  type Tool,
  type UpdateMcpServerInput,
} from "@/bindings/commands";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import {
  globalMcpStatusesQueryOptions,
  mcpKeys,
  mcpProjectOptionsQueryOptions,
  mcpProjectsQueryOptions,
  mcpServersQueryOptions,
} from "@/lib/mcp-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { globalTargetStatusPresentation } from "@/lib/global-target-status-ui";
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
  projectId: string | null;
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
  const projectsQuery = useQuery(mcpProjectsQueryOptions());
  const statusesQuery = useQuery(globalMcpStatusesQueryOptions());
  const [form, setForm] = useState<McpFormState>(emptyForm);
  const [formError, setFormError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [projectId, setProjectId] = useState("");
  const [projectTool, setProjectTool] = useState<Tool>("claude");
  const [openPreview, setOpenPreview] = useState<OpenMcpPreview | null>(null);
  const [openImport, setOpenImport] = useState<{
    tool: Tool;
    requestId: string;
  } | null>(null);
  const projectOptionsQuery = useQuery(
    mcpProjectOptionsQueryOptions(projectId, projectTool),
  );
  const selectedProject = useMemo(
    () => projectsQuery.data?.find((project) => project.id === projectId),
    [projectId, projectsQuery.data],
  );

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
      setMessage("中央 MCP 已保存；原生配置尚未修改。请生成预览后再 Apply。");
      setForm(emptyForm);
      await invalidateMcp();
    },
  });

  const enabledMutation = useMutation({
    mutationFn: async (server: McpServerDto) =>
      unwrapResult(
        await commands.setMcpEnabled(
          { id: server.id, rowVersion: server.rowVersion },
          !server.enabled,
        ),
      ),
    onSuccess: invalidateMcp,
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
      setMessage("中央 MCP 已删除；仍需预览并 Apply 才会安全清理旧受管条目。");
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
    onSuccess: invalidateMcp,
  });

  const projectAssignmentMutation = useMutation({
    mutationFn: async (input: SetProjectMcpAssignmentInput) =>
      unwrapResult(await commands.setProjectMcpAssignment(input)),
    onSuccess: invalidateMcp,
  });

  const previewMutation = useMutation({
    mutationFn: async ({
      tool,
      projectId,
    }: {
      tool: Tool;
      projectId: string | null;
    }) => ({
      tool,
      projectId,
      plan: unwrapResult(
        await commands.previewMcpSync({
          tool,
          projectId,
          excludeFromGit: false,
        }),
      ),
    }),
    onSuccess: ({ plan, tool, projectId }) => {
      if (plan.targets.length === 0) {
        setMessage(
          projectId
            ? "该项目只有全局继承项，无需创建或修改项目配置。"
            : "暂无启用且已分配到该工具的中央 MCP。已有原生配置可通过“检测并导入已有 MCP”纳入管理，也可先创建并分配 MCP。",
        );
        setOpenPreview(null);
        return;
      }
      setOpenPreview({ plan, tool, projectId });
    },
  });

  const applyMutation = useMutation({
    mutationFn: async (input: ApplyMcpPreviewInput) =>
      unwrapResult(await commands.applyMcpPreview(input)),
    onSuccess: async (result) => {
      setMessage(
        `已应用 ${result.appliedTargets} 个 MCP 目标，并创建 ${result.snapshotCount} 份快照。`,
      );
      setOpenPreview(null);
      await invalidateMcp();
    },
  });

  const operationError = [
    saveMutation.error,
    enabledMutation.error,
    deleteMutation.error,
    globalAssignmentMutation.error,
    projectAssignmentMutation.error,
    previewMutation.error,
    applyMutation.error,
  ]
    .map(profileErrorText)
    .find(Boolean);

  return (
    <main className="p-5 sm:p-8">
      <header className="mx-auto max-w-6xl">
        <p className="text-muted-foreground text-sm">中央配置库</p>
        <h1 className="mt-1 text-3xl font-semibold">MCP</h1>
        <p className="text-muted-foreground mt-2 max-w-3xl text-sm leading-6">
          MCP 的 CRUD、启停和分配只更新中央意图。header、env
          与识别出的敏感扩展不会从后端回填到普通
          DTO；原生写入必须经过持久化预览。
        </p>
      </header>

      <div className="mx-auto mt-6 max-w-6xl space-y-4" aria-live="polite">
        {message ? (
          <p className="rounded-lg border border-emerald-200 bg-emerald-50 p-4 text-sm">
            {message}
          </p>
        ) : null}
        {formError || operationError ? (
          <p
            role="alert"
            className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm"
          >
            {formError ?? operationError}
          </p>
        ) : null}
      </div>

      <div className="mx-auto mt-6 grid max-w-6xl gap-6 xl:grid-cols-[1fr_1.15fr]">
        <section
          className="rounded-xl border bg-white p-5"
          aria-labelledby="mcp-form-title"
        >
          <h2 id="mcp-form-title" className="text-lg font-semibold">
            {form.id ? "编辑 MCP" : "新增 MCP"}
          </h2>
          <form
            className="mt-4 space-y-4"
            onSubmit={(event) => {
              event.preventDefault();
              setFormError(null);
              try {
                validateForm(form);
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
                  onChange={(env) =>
                    setForm((current) => ({ ...current, env }))
                  }
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
            <div className="flex gap-3">
              <Button type="submit" disabled={saveMutation.isPending}>
                {saveMutation.isPending ? "正在保存…" : "保存中央意图"}
              </Button>
              {form.id ? (
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setForm(emptyForm)}
                >
                  取消编辑
                </Button>
              ) : null}
            </div>
          </form>
        </section>

        <section
          className="rounded-xl border bg-white p-5"
          aria-labelledby="mcp-list-title"
        >
          <h2 id="mcp-list-title" className="text-lg font-semibold">
            中央列表
          </h2>
          {serversQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在读取 MCP…
            </p>
          ) : null}
          {serversQuery.isError ? (
            <p role="alert" className="mt-4 text-sm text-red-700">
              {profileErrorText(serversQuery.error)}
            </p>
          ) : null}
          {serversQuery.data?.length === 0 ? (
            <p className="text-muted-foreground mt-4 text-sm">
              中央库尚无 MCP。已有工具配置可通过下方“检测并导入已有
              MCP”纳入管理，也可使用左侧表单创建。
            </p>
          ) : null}
          <div className="mt-4 space-y-3">
            {serversQuery.data?.map((server) => (
              <article key={server.id} className="rounded-lg border p-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <h3 className="font-medium">{server.name}</h3>
                    <p className="text-muted-foreground mt-1 text-xs">
                      {server.transport} ·{" "}
                      {server.enabled ? "已启用" : "已停用"}
                    </p>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => setForm(editForm(server))}
                    >
                      编辑
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => enabledMutation.mutate(server)}
                    >
                      {server.enabled ? "停用" : "启用"}
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => deleteMutation.mutate(server)}
                    >
                      删除
                    </Button>
                  </div>
                </div>
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
                      headers: {server.headerNames.join(", ") || "无"}；env:{" "}
                      {server.envNames.join(", ") || "无"}
                    </dd>
                  </div>
                </dl>
                <pre className="bg-muted mt-3 overflow-auto rounded p-2 text-xs">
                  {JSON.stringify(server.redactedExtra, null, 2)}
                </pre>
                <div className="mt-3 flex flex-wrap gap-2">
                  {(["claude", "codex"] as const).map((tool) => (
                    <Button
                      key={tool}
                      size="sm"
                      variant={
                        server.globalTools.includes(tool)
                          ? "default"
                          : "outline"
                      }
                      disabled={globalAssignmentMutation.isPending}
                      onClick={() =>
                        globalAssignmentMutation.mutate({ server, tool })
                      }
                    >
                      {tool === "claude" ? "Claude" : "Codex"} 全局
                      {server.globalTools.includes(tool) ? "已分配" : "未分配"}
                    </Button>
                  ))}
                </div>
              </article>
            ))}
          </div>
        </section>
      </div>

      <section
        className="mx-auto mt-6 max-w-6xl rounded-xl border bg-white p-5"
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
          <p role="alert" className="mt-3 text-sm text-red-700">
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
                <p className="font-medium">
                  {status.tool === "claude" ? "Claude" : "Codex"}
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
                  <p className="mt-2 text-xs text-amber-800">
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
                      projectId: null,
                    })
                  }
                >
                  生成全局预览
                </Button>
              </article>
            );
          })}
        </div>
      </section>

      <section
        className="mx-auto mt-6 max-w-6xl rounded-xl border bg-white p-5"
        aria-labelledby="mcp-project-title"
      >
        <h2 id="mcp-project-title" className="text-lg font-semibold">
          项目追加选择器
        </h2>
        <div className="mt-4 grid gap-3 sm:grid-cols-2">
          <Field label="项目">
            <select
              className="field"
              value={projectId}
              onChange={(event) => setProjectId(event.target.value)}
            >
              <option value="">选择已登记项目</option>
              {projectsQuery.data?.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.displayName}
                </option>
              ))}
            </select>
          </Field>
          <Field label="工具">
            <select
              className="field"
              value={projectTool}
              onChange={(event) =>
                setProjectTool(
                  event.target.value === "codex" ? "codex" : "claude",
                )
              }
            >
              <option value="claude">Claude</option>
              <option value="codex">Codex</option>
            </select>
          </Field>
        </div>
        {projectsQuery.isPending ? (
          <p role="status" className="mt-4 text-sm">
            正在读取已登记项目…
          </p>
        ) : null}
        {projectsQuery.data?.length === 0 ? (
          <p className="text-muted-foreground mt-4 text-sm">
            尚无已登记项目；项目登记将在“项目”页面完成。
          </p>
        ) : null}
        {projectsQuery.isError || projectOptionsQuery.isError ? (
          <p role="alert" className="mt-4 text-sm text-red-700">
            {profileErrorText(projectsQuery.error ?? projectOptionsQuery.error)}
          </p>
        ) : null}
        {selectedProject ? (
          <p className="text-muted-foreground mt-4 text-xs">
            {selectedProject.rootPath} · Codex trust:{" "}
            {selectedProject.codexTrustStatus}
          </p>
        ) : null}
        {projectId && projectOptionsQuery.isPending ? (
          <p role="status" className="mt-4 text-sm">
            正在读取项目 MCP 选择状态…
          </p>
        ) : null}
        {projectId && projectOptionsQuery.data?.length === 0 ? (
          <p className="text-muted-foreground mt-4 text-sm">
            中央库尚无可供该项目继承或追加的 MCP；请先创建 MCP。
          </p>
        ) : null}
        <div className="mt-4 space-y-2">
          {projectOptionsQuery.data?.map((option) => (
            <div
              key={option.mcpId}
              className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3 text-sm"
            >
              <div>
                <p className="font-medium">{option.name}</p>
                <p className="text-muted-foreground text-xs">
                  {option.state === "inherited"
                    ? "全局继承（项目不可禁用或重复选择）"
                    : option.state === "selected"
                      ? "项目追加"
                      : "可追加"}
                  {!option.enabled ? " · 中央项已停用" : ""}
                </p>
              </div>
              <Button
                size="sm"
                variant={option.state === "selected" ? "default" : "outline"}
                disabled={
                  !option.selectable ||
                  !selectedProject ||
                  projectAssignmentMutation.isPending
                }
                onClick={() =>
                  selectedProject
                    ? projectAssignmentMutation.mutate({
                        projectId: selectedProject.id,
                        tool: projectTool,
                        mcpId: option.mcpId,
                        assigned: option.state !== "selected",
                        mcpRowVersion: option.rowVersion,
                        projectRowVersion: selectedProject.rowVersion,
                      })
                    : undefined
                }
              >
                {option.state === "inherited"
                  ? "只读继承"
                  : option.state === "selected"
                    ? "移除追加"
                    : "追加到项目"}
              </Button>
            </div>
          ))}
        </div>
        <Button
          className="mt-4"
          disabled={
            !projectId ||
            previewMutation.isPending ||
            (projectTool === "codex" &&
              selectedProject?.codexTrustStatus !== "trusted")
          }
          onClick={() =>
            previewMutation.mutate({
              tool: projectTool,
              projectId: projectId || null,
            })
          }
        >
          生成项目预览
        </Button>
      </section>

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
              `已导入 ${result.createdCount + result.reusedCount} 项 MCP（新建 ${result.createdCount} 项，复用 ${result.reusedCount} 项），已分配到 ${result.tool === "claude" ? "Claude" : "Codex"} 全局。原生配置未改写，请单独生成全局预览。`,
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
        onClose={() => setOpenPreview(null)}
        onApply={() => {
          if (openPreview) {
            applyMutation.mutate({
              previewId: openPreview.plan.previewId,
              tool: openPreview.tool,
              projectId: openPreview.projectId,
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
