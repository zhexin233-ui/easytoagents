import { useId, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Power, PowerOff, Trash2 } from "lucide-react";

import {
  commands,
  type AgentDto,
  type AgentToolSettingsDto,
  type AgentImportResultDto,
  type AgentToolTargetStatusDto,
  type ReadoptAgentTargetResultDto,
  type Tool,
} from "@/bindings/commands";
import {
  CentralList,
  CentralListCard,
  CentralListCardBody,
  CentralListCardFooter,
  CentralListLayoutToggle,
} from "@/components/central-list-layout";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import { EmptyState } from "@/components/empty-state";
import { FormDialog } from "@/components/form-dialog";
import { PageHeader } from "@/components/page-header";
import { PlatformAssignmentButton } from "@/components/platform-assignment-button";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { ToolIconToggle } from "@/components/tool-icon-toggle";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { useNotify } from "@/components/use-notify";
import { usePersistedCentralListLayout } from "@/components/use-persisted-central-list-layout";
import { useImportDialogState } from "@/features/sync/use-import-dialog-state";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { useSubmitGuard } from "@/hooks/use-submit-guard";
import {
  agentsKeys,
  agentsQueryOptions,
  globalAgentStatusesQueryOptions,
  setAgentToolSettings,
} from "@/lib/agents-api";
import { dashboardKeys } from "@/lib/dashboard-api";
import { globalTargetStatusPresentation } from "@/lib/global-target-status-ui";
import { profileErrorText, unwrapResult } from "@/lib/rpc";
import { appSettingsQueryOptions } from "@/lib/settings-api";
import {
  AGENT_TOOLS,
  AGENT_TOOL_SETTINGS_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";
import { AgentImportDialog } from "@/features/agents/agent-import-dialog";
import { AgentToolSettingsForm } from "@/features/agents/agent-tool-settings-form";
import {
  EMPTY_AGENT_TOOL_SETTINGS,
  validateAgentToolSettingsDraft,
} from "@/features/agents/agent-tool-settings-validation";

interface AgentFormState {
  id: string | null;
  rowVersion: number | null;
  name: string;
  description: string;
  prompt: string;
  enabled: boolean;
  toolSettings: AgentToolSettingsDto;
}

interface AgentSaveVariables {
  state: AgentFormState;
  globalTools: Tool[];
}

const emptyForm: AgentFormState = {
  id: null,
  rowVersion: null,
  name: "",
  description: "",
  prompt: "",
  enabled: true,
  toolSettings: EMPTY_AGENT_TOOL_SETTINGS,
};

export function AgentsPage() {
  const queryClient = useQueryClient();
  const agentsQuery = useQuery(agentsQueryOptions());
  const statusesQuery = useQuery(globalAgentStatusesQueryOptions());
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const directApply = settingsQuery.data?.applyMode === "direct";
  const enabledTools = useEnabledTools();
  const visibleTools = filterEnabledTools(AGENT_TOOLS, enabledTools);
  const [selectedTool, setSelectedTool] = useState<Tool>("claude");
  const activeTool = visibleTools.includes(selectedTool)
    ? selectedTool
    : (visibleTools[0] ?? selectedTool);
  const [listLayout, setListLayout] = usePersistedCentralListLayout("agents");
  const [form, setForm] = useState<AgentFormState | null>(null);
  const [statusOpen, setStatusOpen] = useState<Set<Tool>>(() => new Set());
  const importDialog = useImportDialogState();
  const saveGuard = useSubmitGuard();
  const { notify } = useNotify();

  const invalidateAgents = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: agentsKeys.all }),
      queryClient.invalidateQueries({ queryKey: dashboardKeys.all }),
    ]);
  };

  const saveMutation = useMutation({
    mutationFn: async ({ state }: AgentSaveVariables) => {
      const current =
        state.id === null
          ? null
          : (agentsQuery.data?.find((agent) => agent.id === state.id) ?? null);
      let saved: AgentDto;
      if (state.id !== null && state.rowVersion !== null) {
        saved = unwrapResult(
          await commands.updateAgent({
            id: state.id,
            name: state.name,
            description: state.description,
            prompt: state.prompt,
            enabled: state.enabled,
            rowVersion: state.rowVersion,
          }),
        );
      } else {
        saved = unwrapResult(
          await commands.createAgent({
            name: state.name,
            description: state.description,
            prompt: state.prompt,
            enabled: state.enabled,
          }),
        );
      }
      const previous = current?.toolSettings ?? EMPTY_AGENT_TOOL_SETTINGS;
      for (const tool of AGENT_TOOL_SETTINGS_TOOLS) {
        const before = tool === "claude" ? previous.claude : previous.codex;
        const after =
          tool === "claude"
            ? state.toolSettings.claude
            : state.toolSettings.codex;
        if (JSON.stringify(before) === JSON.stringify(after)) continue;
        saved = await setAgentToolSettings({
          agentId: saved.id,
          tool,
          settings: after,
          rowVersion: saved.rowVersion,
        });
      }
      return saved;
    },
    onSuccess: async (_updated, { globalTools }) => {
      await invalidateAgents();
      setForm(null);
      if (directApply && globalTools.length > 0) {
        notify({
          kind: "success",
          message: "中央 Agent 已保存；正在自动同步已分配工具。",
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
        message: directApply
          ? "中央 Agent 已保存；已分配工具会按直接应用模式自动同步。"
          : "中央 Agent 已保存；原生目录未修改，请在全局目标状态中生成预览并确认应用。",
      });
    },
    onSettled: () => saveGuard.end(),
  });

  const enabledMutation = useMutation({
    mutationFn: async (agent: AgentDto) =>
      unwrapResult(
        await commands.setAgentEnabled(
          { id: agent.id, rowVersion: agent.rowVersion },
          !agent.enabled,
        ),
      ),
    onSuccess: async (_result, agent) => {
      await invalidateAgents();
      if (!directApply) return;
      for (const tool of agent.globalAssignments) {
        await previewMutation
          .mutateAsync({ tool, autoApply: true })
          .catch(() => undefined);
      }
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新 Agent 启停状态失败。",
      });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: async (agent: AgentDto) =>
      unwrapResult(
        await commands.deleteAgent({
          id: agent.id,
          rowVersion: agent.rowVersion,
        }),
      ),
    onSuccess: async (_result, agent) => {
      await invalidateAgents();
      if (directApply && agent.globalAssignments.length > 0) {
        notify({
          kind: "success",
          message: "中央 Agent 已删除；正在自动清理旧受管文件。",
        });
        for (const tool of agent.globalAssignments) {
          await previewMutation
            .mutateAsync({ tool, autoApply: true })
            .catch(() => undefined);
        }
        return;
      }
      notify({
        kind: "success",
        message: directApply
          ? "中央 Agent 已删除；已分配工具会自动清理受管文件。"
          : "中央 Agent 已删除；仍需生成预览并确认应用后才会清理受管文件。",
      });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "删除中央 Agent 失败。",
      });
    },
  });

  const assignmentMutation = useMutation({
    mutationFn: async ({ agent, tool }: { agent: AgentDto; tool: Tool }) =>
      unwrapResult(
        await commands.setGlobalAgentAssignment({
          tool,
          agentId: agent.id,
          assigned: !agent.globalAssignments.includes(tool),
          rowVersion: agent.rowVersion,
        }),
      ),
    onSuccess: async (_result, variables) => {
      await invalidateAgents();
      if (!directApply) {
        notify({
          kind: "success",
          message:
            "全局 Agent 分配已更新；这只改变中央意图，请生成全局预览并确认应用。",
        });
        return;
      }
      requestPreview(variables.tool, true);
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新 Agent 全局分配失败。",
      });
    },
  });

  const {
    openPreview,
    requestPreview,
    previewMutation,
    applyMutation,
    readoptMutation,
    closePreview,
  } = useSyncPreviewFlow<ReadoptAgentTargetResultDto>({
    artifactKind: "agent",
    directApply,
    preview: (tool) =>
      commands.previewAgentSync({
        tool,
        projectId: null,
        excludeFromGit: false,
      }),
    apply: ({ previewId, tool }) =>
      commands.applyAgentPreview({
        previewId,
        tool,
        projectId: null,
      }),
    readopt: (tool, targetPath) => {
      if (!targetPath) {
        throw new Error("重新接管 Agent 目标缺少文件路径。");
      }
      return commands.readoptAgentTarget({
        tool,
        projectId: null,
        targetPath,
      });
    },
    invalidate: invalidateAgents,
    messages: {
      previewFailed: "生成 Agents 全局预览失败。",
      applyFailed: "应用 Agents 全局同步失败。",
      readoptFailed: "重新接管 Agents 目标失败。",
      empty: "当前工具没有需要同步的全局 Agent。",
      applied: (result) =>
        `已应用 ${result.appliedTargets} 个 Agents 目标，并创建 ${result.snapshotCount} 份快照。`,
    },
    onReadopted: (_result, tool) => {
      notify({
        kind: "success",
        message: "已以当前内容重新接管 Agent 目标；正在重新生成预览。",
      });
      requestPreview(tool, directApply);
    },
  });

  const openForm = (nextForm: AgentFormState) => {
    if (saveGuard.isInFlight() || saveMutation.isPending) return;
    saveMutation.reset();
    setForm(nextForm);
  };
  const closeForm = () => {
    if (saveGuard.isInFlight() || saveMutation.isPending) return;
    saveMutation.reset();
    setForm(null);
  };
  const submitForm = (nextForm: AgentFormState) => {
    if (saveMutation.isPending || !saveGuard.begin()) return;
    saveMutation.mutate({
      state: nextForm,
      globalTools:
        nextForm.id === null
          ? []
          : (agentsQuery.data?.find((agent) => agent.id === nextForm.id)
              ?.globalAssignments ?? []),
    });
  };
  const toggleStatus = (tool: Tool) => {
    setStatusOpen((current) => {
      const next = new Set(current);
      if (next.has(tool)) next.delete(tool);
      else next.add(tool);
      return next;
    });
  };

  const statuses = (statusesQuery.data ?? []).filter((status) =>
    enabledTools.has(status.tool),
  );

  return (
    <>
      <PageHeader
        title="Agents"
        actions={
          <>
            <CentralListLayoutToggle
              value={listLayout}
              onChange={setListLayout}
            />
            <Button onClick={() => openForm(emptyForm)}>新增 Agent</Button>
          </>
        }
      >
        <div
          className="mt-3 flex items-center gap-2"
          role="group"
          aria-label="Agents 工具视图"
        >
          {visibleTools.map((tool) => (
            <AgentToolViewButton
              key={tool}
              tool={tool}
              selected={activeTool === tool}
              onClick={() => setSelectedTool(tool)}
            />
          ))}
        </div>
      </PageHeader>
      <main className="space-y-6 px-8 py-6">
        <section
          className="bg-card rounded-lg border p-5"
          aria-labelledby="agents-list-title"
        >
          <h2 id="agents-list-title" className="text-[15px] font-semibold">
            中央列表
          </h2>
          {agentsQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在读取 Agents…
            </p>
          ) : null}
          {agentsQuery.isError ? (
            <p role="alert" className="text-destructive mt-4 text-sm">
              {profileErrorText(agentsQuery.error) ?? "读取 Agents 失败。"}
            </p>
          ) : null}
          {agentsQuery.data?.length === 0 ? (
            <div className="mt-4">
              <EmptyState
                title="尚无 Agent"
                description="可新增 Agent，或从工具的全局 Agents 目录导入。"
                action={
                  <Button onClick={() => openForm(emptyForm)}>
                    新增 Agent
                  </Button>
                }
              />
            </div>
          ) : null}
          <CentralList layout={listLayout}>
            {agentsQuery.data?.map((agent) => {
              const assignedSummary =
                agent.globalAssignments.length === 0
                  ? "未分配"
                  : agent.globalAssignments
                      .map((tool) => toolMetadata(tool).label)
                      .join("、");
              const actions = (
                <div className="flex min-w-0 flex-wrap gap-2">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label={`编辑 ${agent.name}`}
                    title={`编辑 ${agent.name}`}
                    onClick={() => openForm(editForm(agent))}
                  >
                    <Pencil aria-hidden="true" className="size-4" />
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label={
                      agent.enabled
                        ? `停用 ${agent.name}`
                        : `启用 ${agent.name}`
                    }
                    title={
                      agent.enabled
                        ? `停用 ${agent.name}`
                        : `启用 ${agent.name}`
                    }
                    aria-pressed={agent.enabled}
                    disabled={enabledMutation.isPending}
                    onClick={() => enabledMutation.mutate(agent)}
                  >
                    {agent.enabled ? (
                      <PowerOff aria-hidden="true" className="size-4" />
                    ) : (
                      <Power aria-hidden="true" className="size-4" />
                    )}
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    className="size-8 p-0"
                    aria-label={`删除 ${agent.name}`}
                    title={`删除 ${agent.name}`}
                    disabled={deleteMutation.isPending}
                    onClick={() => deleteMutation.mutate(agent)}
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
                  aria-label={`${agent.name} 全局平台分配`}
                >
                  {visibleTools.map((tool) => (
                    <PlatformAssignmentButton
                      key={tool}
                      tool={tool}
                      assigned={agent.globalAssignments.includes(tool)}
                      disabled={assignmentMutation.isPending}
                      onClick={() => assignmentMutation.mutate({ agent, tool })}
                    />
                  ))}
                </div>
              );
              return (
                <CentralListCard key={agent.id} layout={listLayout}>
                  <CentralListCardBody layout={listLayout}>
                    <div className="flex min-w-0 flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0">
                        <h3
                          className={
                            listLayout === "grid"
                              ? "truncate font-medium"
                              : "font-medium"
                          }
                          title={agent.name}
                        >
                          {agent.name}
                        </h3>
                        <p className="text-muted-foreground mt-1 text-xs">
                          {agent.enabled ? "已启用" : "已停用"} · 全局分配：
                          {assignedSummary}
                        </p>
                        {countToolSettings(agent.toolSettings) > 0 ? (
                          <span className="bg-muted text-muted-foreground mt-2 inline-flex rounded-full px-2 py-0.5 text-[11px]">
                            {countToolSettings(agent.toolSettings)}{" "}
                            个工具有特有设置
                          </span>
                        ) : null}
                      </div>
                      {listLayout === "list" ? actions : null}
                    </div>
                    {listLayout === "list" ? (
                      <>
                        <p className="bg-muted rounded-control mt-3 p-2 text-xs">
                          {agent.description}
                        </p>
                        <pre className="bg-muted/50 rounded-control mt-3 max-h-32 overflow-auto p-3 text-xs leading-5">
                          {agent.prompt}
                        </pre>
                      </>
                    ) : (
                      <p
                        className="text-muted-foreground mt-4 line-clamp-3 text-sm leading-6"
                        title={agent.description}
                      >
                        {agent.description}
                      </p>
                    )}
                  </CentralListCardBody>
                  <CentralListCardFooter
                    layout={listLayout}
                    label={`${agent.name} 操作`}
                  >
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      {listLayout === "grid" ? actions : null}
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
          aria-labelledby="agents-target-title"
        >
          <h2 id="agents-target-title" className="text-[15px] font-semibold">
            全局目标状态
          </h2>
          {statusesQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在检查全局 Agents 目标…
            </p>
          ) : null}
          {statusesQuery.isError ? (
            <p role="alert" className="text-destructive mt-4 text-sm">
              {profileErrorText(statusesQuery.error) ??
                "读取 Agents 目标失败。"}
            </p>
          ) : null}
          {statuses.length === 0 && statusesQuery.data ? (
            <p className="text-muted-foreground mt-4 text-sm">
              当前没有可检查的全局 Agents 目标。
            </p>
          ) : null}
          {statuses.length > 0 ? (
            <div className="mt-3 divide-y">
              {statuses.map((status) => (
                <AgentStatusCard
                  key={status.tool}
                  status={status}
                  directApply={directApply}
                  expanded={statusOpen.has(status.tool)}
                  previewPending={previewMutation.isPending}
                  onToggle={() => toggleStatus(status.tool)}
                  onImport={() => importDialog.open(status.tool)}
                  onPreview={() => requestPreview(status.tool, directApply)}
                />
              ))}
            </div>
          ) : null}
        </section>

        {form ? (
          <AgentFormDialog
            key={`${form.id ?? "new"}-${formOpenKey(form)}`}
            open
            initialValue={form}
            directApply={directApply}
            pending={saveMutation.isPending}
            error={profileErrorText(saveMutation.error)}
            onClose={closeForm}
            onSubmit={submitForm}
          />
        ) : null}

        {importDialog.state ? (
          <AgentImportDialog
            key={importDialog.state.requestId}
            tool={importDialog.state.tool}
            requestId={importDialog.state.requestId}
            onClose={importDialog.close}
            onRescan={importDialog.rescan}
            onImported={async (result) => {
              importDialog.close();
              await invalidateAgents();
              showImportSuccess(notify, result);
            }}
          />
        ) : null}

        <ChangePreviewDialog
          preview={openPreview?.plan ?? null}
          tool={openPreview?.tool ?? activeTool}
          artifactKind="agent"
          applying={applyMutation.isPending}
          readopting={readoptMutation.isPending}
          onReadopt={(targetPath) => {
            if (openPreview) {
              readoptMutation.mutate({
                tool: openPreview.tool,
                targetPath,
              });
            }
          }}
          onClose={closePreview}
          onApply={(previewId, tool) =>
            applyMutation.mutate({ previewId, tool })
          }
        />
      </main>
    </>
  );
}

function formOpenKey(form: AgentFormState) {
  return form.id === null ? "create" : "edit";
}

function editForm(agent: AgentDto): AgentFormState {
  return {
    id: agent.id,
    rowVersion: agent.rowVersion,
    name: agent.name,
    description: agent.description,
    prompt: agent.prompt,
    enabled: agent.enabled,
    toolSettings: agent.toolSettings,
  };
}

function countToolSettings(settings: AgentToolSettingsDto): number {
  return [settings.claude, settings.codex].filter(
    (value) => value !== null && Object.keys(value).length > 0,
  ).length;
}

function showImportSuccess(
  notify: ReturnType<typeof useNotify>["notify"],
  result: AgentImportResultDto,
) {
  notify({
    kind: "success",
    message: `已导入 ${result.createdCount} 个 Agent 到中央库；原生文件未修改，也未自动分配或同步。`,
  });
}

interface AgentToolViewButtonProps {
  tool: Tool;
  selected: boolean;
  onClick: () => void;
}

function AgentToolViewButton({
  tool,
  selected,
  onClick,
}: AgentToolViewButtonProps) {
  return (
    <ToolIconToggle
      tool={tool}
      active={selected}
      label={`管理 ${toolMetadata(tool).label} Agents`}
      onClick={onClick}
    />
  );
}

interface AgentStatusCardProps {
  status: AgentToolTargetStatusDto;
  directApply: boolean;
  expanded: boolean;
  previewPending: boolean;
  onToggle: () => void;
  onImport: () => void;
  onPreview: () => void;
}

function AgentStatusCard({
  status,
  directApply,
  expanded,
  previewPending,
  onToggle,
  onImport,
  onPreview,
}: AgentStatusCardProps) {
  const label = toolMetadata(status.tool).label;
  const diagnosticCode =
    status.diagnosticCode ??
    status.files.find((file) => file.diagnosticCode !== null)?.diagnosticCode ??
    null;
  const presentation = globalTargetStatusPresentation(
    status.aggregateStatus,
    diagnosticCode,
    { directApply },
  );
  const hasFiles = status.files.length > 0;
  return (
    <article className="hover:bg-muted/50 px-1 py-3 text-sm">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <strong>{toolMetadata(status.tool).label}</strong>
        <div className="flex items-center gap-2">
          <SyncStatusBadge
            label={presentation.label}
            status={status.aggregateStatus}
            tone={presentation.tone}
          />
          <Button
            type="button"
            size="icon"
            variant="ghost"
            aria-label={
              expanded ? `收起 ${label} 文件状态` : `展开 ${label} 文件状态`
            }
            title={
              expanded ? `收起 ${label} 文件状态` : `展开 ${label} 文件状态`
            }
            aria-expanded={expanded}
            onClick={onToggle}
          >
            {expanded ? "−" : "+"}
          </Button>
        </div>
      </div>
      <code className="mt-2 block text-xs break-all">
        {status.directoryPath ?? "目标目录不可用"}
      </code>
      {presentation.description ? (
        <p className="text-muted-foreground mt-2 text-xs">
          {presentation.description}
        </p>
      ) : null}
      {diagnosticCode ? (
        <p className="text-warning mt-2 text-xs">
          诊断码：<code>{diagnosticCode}</code>
        </p>
      ) : null}
      {expanded ? (
        <div className="mt-3 space-y-2" aria-label="Agent 文件状态">
          {hasFiles ? (
            status.files.map((file) => (
              <AgentFileStatusRow
                key={file.targetPath}
                file={file}
                directApply={directApply}
              />
            ))
          ) : (
            <p className="text-muted-foreground text-xs">
              尚无受管 Agent 文件。
            </p>
          )}
        </div>
      ) : null}
      <div className="mt-3 flex flex-wrap gap-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={presentation.previewBlocked}
          aria-label={`检测并导入 ${label} 全局 Agents`}
          onClick={onImport}
        >
          检测并导入已有 Agents
        </Button>
        {!directApply || status.aggregateStatus === "external_owned_change" ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={previewPending || presentation.previewBlocked}
            aria-label={
              directApply
                ? `处理 ${label} Agents 同步冲突`
                : `${label} Agents 同步预览`
            }
            onClick={onPreview}
          >
            {previewPending
              ? "正在生成…"
              : directApply
                ? "处理同步冲突"
                : "预览全局同步"}
          </Button>
        ) : null}
      </div>
    </article>
  );
}

function AgentFileStatusRow({
  file,
  directApply,
}: {
  file: AgentToolTargetStatusDto["files"][number];
  directApply: boolean;
}) {
  const presentation = globalTargetStatusPresentation(
    file.status,
    file.diagnosticCode,
    { directApply },
  );
  return (
    <div className="bg-muted/40 rounded-control border px-3 py-2 text-xs">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <code className="min-w-0 break-all">{file.targetPath}</code>
        <SyncStatusBadge
          status={file.status}
          label={presentation.label}
          tone={presentation.tone}
        />
      </div>
      {presentation.description ? (
        <p className="text-muted-foreground mt-2">{presentation.description}</p>
      ) : null}
      {file.diagnosticCode ? (
        <p className="text-warning mt-2">
          诊断码：<code>{file.diagnosticCode}</code>
        </p>
      ) : null}
    </div>
  );
}

interface AgentFormDialogProps {
  open: boolean;
  initialValue: AgentFormState;
  directApply: boolean;
  pending: boolean;
  error: string | null;
  onClose: () => void;
  onSubmit: (state: AgentFormState) => void;
}

function AgentFormDialog({
  open,
  initialValue,
  directApply,
  pending,
  error,
  onClose,
  onSubmit,
}: AgentFormDialogProps) {
  const [draft, setDraft] = useState(initialValue);
  const [nameError, setNameError] = useState<string | null>(null);
  const nameId = useId();
  const descriptionId = useId();
  const promptId = useId();

  const validateName = (value: string) => {
    if (!AGENT_NAME_PATTERN.test(value)) {
      setNameError("名称只能使用小写字母、数字和连字符，长度为 1–64 个字符。");
      return false;
    }
    setNameError(null);
    return true;
  };
  const validate = () => {
    const nameValid = validateName(draft.name);
    if (draft.description.trim().length === 0) {
      return nameValid ? "描述不能为空。" : null;
    }
    if (draft.prompt.trim().length === 0) {
      return nameValid ? "系统提示正文不能为空。" : null;
    }
    const settingsError = validateAgentToolSettingsDraft(draft.toolSettings);
    if (settingsError) return settingsError;
    return nameValid ? null : "名称无效。";
  };

  return (
    <FormDialog
      open={open}
      title={draft.id === null ? "新增 Agent" : "编辑 Agent"}
      description={
        directApply
          ? "保存更新中央 Agent；已分配工具会按直接应用模式自动同步。"
          : "保存只更新中央 Agent，不会直接修改工具目录。"
      }
      submitLabel="保存中央意图"
      pending={pending}
      error={error ?? nameError}
      onClose={onClose}
      onSubmit={() => {
        const validationError = validate();
        if (validationError) return;
        onSubmit(draft);
      }}
    >
      <Field label="名称" id={nameId}>
        <input
          id={nameId}
          className="field"
          value={draft.name}
          maxLength={64}
          autoComplete="off"
          onChange={(event) => {
            setDraft((current) => ({ ...current, name: event.target.value }));
            if (nameError) setNameError(null);
          }}
          onBlur={() => validateName(draft.name)}
          required
        />
      </Field>
      <Field label="描述" id={descriptionId}>
        <textarea
          id={descriptionId}
          className="field min-h-20"
          value={draft.description}
          maxLength={1000}
          onChange={(event) =>
            setDraft((current) => ({
              ...current,
              description: event.target.value,
            }))
          }
          required
        />
      </Field>
      <Field label="系统提示正文" id={promptId}>
        <textarea
          id={promptId}
          className="field min-h-48 font-mono text-xs leading-5"
          value={draft.prompt}
          maxLength={65536}
          onChange={(event) =>
            setDraft((current) => ({ ...current, prompt: event.target.value }))
          }
          required
        />
      </Field>
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={draft.enabled}
          onChange={(event) =>
            setDraft((current) => ({
              ...current,
              enabled: event.target.checked,
            }))
          }
        />
        启用
      </label>
      <AgentToolSettingsForm
        value={draft.toolSettings}
        onChange={(toolSettings) =>
          setDraft((current) => ({ ...current, toolSettings }))
        }
      />
    </FormDialog>
  );
}

const AGENT_NAME_PATTERN = /^[a-z0-9][a-z0-9-]{0,63}$/;
