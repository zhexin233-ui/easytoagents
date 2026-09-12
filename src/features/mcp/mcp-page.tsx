import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Power, PowerOff, Trash2 } from "lucide-react";

import { commands, type McpServerDto, type Tool } from "@/bindings/commands";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
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
import { useEnabledTools } from "@/components/use-enabled-tools";
import { useNotify } from "@/components/use-notify";
import { usePersistedCentralListLayout } from "@/components/use-persisted-central-list-layout";
import { useImportDialogState } from "@/features/sync/use-import-dialog-state";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { useSubmitGuard } from "@/hooks/use-submit-guard";
import {
  globalMcpStatusesQueryOptions,
  mcpKeys,
  mcpServersQueryOptions,
} from "@/lib/mcp-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import {
  MCP_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";
import { globalTargetStatusPresentation } from "@/lib/global-target-status-ui";
import { appSettingsQueryOptions } from "@/lib/settings-api";
import { McpFormDialog } from "@/features/mcp/mcp-form-dialog";
import {
  createMcpInput,
  editMcpForm,
  emptyMcpForm,
  type McpFormState,
  updateMcpInput,
} from "@/features/mcp/mcp-form";
import { McpImportDialog } from "@/features/mcp/mcp-import-dialog";

interface McpSaveVariables {
  state: McpFormState;
  globalTools: Tool[];
}

export function McpPage() {
  const queryClient = useQueryClient();
  const serversQuery = useQuery(mcpServersQueryOptions());
  const statusesQuery = useQuery(globalMcpStatusesQueryOptions());
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const directApply = settingsQuery.data?.applyMode === "direct";
  const enabledTools = useEnabledTools();
  const visibleStatuses = statusesQuery.data?.filter((status) =>
    enabledTools.has(status.tool),
  );
  // 页面只持有弹窗开关与初始草稿；字段编辑状态由 McpFormDialog 自己维护。
  const [formOpen, setFormOpen] = useState(false);
  const [formInitial, setFormInitial] = useState<McpFormState>(emptyMcpForm);
  const submitGuard = useSubmitGuard();
  const { notify } = useNotify();
  const [listLayout, setListLayout] = usePersistedCentralListLayout("mcp");
  const importDialog = useImportDialogState();
  const invalidateMcp = async () => {
    await queryClient.invalidateQueries({ queryKey: mcpKeys.all });
  };

  const saveMutation = useMutation({
    mutationFn: async ({ state }: McpSaveVariables) => {
      if (state.id && state.rowVersion !== null) {
        return unwrapResult(
          await commands.updateMcpServer(updateMcpInput(state)),
        );
      }
      return unwrapResult(
        await commands.createMcpServer(createMcpInput(state)),
      );
    },
    onSuccess: async (_result, { globalTools }) => {
      await invalidateMcp();
      setFormOpen(false);
      setFormInitial(emptyMcpForm);
      if (directApply && globalTools.length > 0) {
        notify({
          kind: "success",
          message: "中央 MCP 已保存；正在自动同步已分配工具。",
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
          ? "中央 MCP 已保存；尚未分配到任何工具，分配后会自动同步。"
          : "中央 MCP 已保存；原生配置尚未修改。请生成预览后再 Apply。",
      });
    },
    onSettled: () => {
      submitGuard.end();
    },
  });

  const openForm = (state: McpFormState) => {
    if (submitGuard.isInFlight() || saveMutation.isPending) return;
    saveMutation.reset();
    setFormInitial(state);
    setFormOpen(true);
  };

  const closeForm = () => {
    if (submitGuard.isInFlight() || saveMutation.isPending) return;
    setFormOpen(false);
    setFormInitial(emptyMcpForm);
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
          .mutateAsync({ tool, autoApply: true })
          .catch(() => undefined);
      }
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新 MCP 启停状态失败。",
      });
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
    onSuccess: async (_result, server) => {
      await invalidateMcp();
      if (directApply && server.globalTools.length > 0) {
        notify({
          kind: "success",
          message: "中央 MCP 已删除；正在自动清理旧受管条目。",
        });
        for (const tool of server.globalTools) {
          await previewMutation
            .mutateAsync({ tool, autoApply: true })
            .catch(() => undefined);
        }
        return;
      }
      notify({
        kind: "success",
        message: directApply
          ? "中央 MCP 已删除；该条目未分配到任何工具，无需同步清理。"
          : "中央 MCP 已删除；仍需预览并 Apply 才会安全清理旧受管条目。",
      });
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "删除中央 MCP 失败。",
      });
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
        requestPreview(tool, true);
      }
    },
    onError: (error) => {
      notify({
        kind: "error",
        message: profileErrorText(error) ?? "更新 MCP 全局分配失败。",
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
  } = useSyncPreviewFlow({
    artifactKind: "mcp",
    directApply,
    preview: (tool) =>
      commands.previewMcpSync({
        tool,
        projectId: null,
        excludeFromGit: false,
      }),
    apply: ({ previewId, tool }) =>
      commands.applyMcpPreview({
        previewId,
        tool,
        projectId: null,
      }),
    readopt: (tool) => commands.readoptMcpTarget({ tool, projectId: null }),
    invalidate: invalidateMcp,
    messages: {
      previewFailed: "生成 MCP 全局预览失败。",
      applyFailed: "应用 MCP 全局同步失败。",
      readoptFailed: "重新接管 MCP 目标失败。",
      empty:
        "暂无启用且已分配到该工具的中央 MCP。已有原生配置可通过“检测并导入已有 MCP”纳入管理，也可先创建并分配 MCP。",
      applied: (result) =>
        `已应用 ${result.appliedTargets} 个 MCP 目标，并创建 ${result.snapshotCount} 份快照。`,
    },
    onReadopted: (result, tool) => {
      notify({
        kind: "success",
        message: `已以当前内容重新接管（刷新 ${result.updatedItemCount} 个、清理 ${result.removedItemCount} 个条目基线）；正在重新生成预览。`,
      });
      requestPreview(tool, directApply);
    },
  });

  return (
    <>
      <PageHeader
        title="MCP"
        actions={
          <>
            <CentralListLayoutToggle
              value={listLayout}
              onChange={setListLayout}
            />
            <Button onClick={() => openForm(emptyMcpForm)}>新增 MCP</Button>
          </>
        }
      />
      <main className="space-y-6 px-8 py-6">
        <section
          className="bg-card rounded-lg border p-5"
          aria-labelledby="mcp-list-title"
        >
          <h2 id="mcp-list-title" className="text-[15px] font-semibold">
            中央列表
          </h2>
          {serversQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在读取 MCP…
            </p>
          ) : null}
          {serversQuery.isError ? (
            <p role="alert" className="text-destructive mt-4 text-sm">
              {profileErrorText(serversQuery.error)}
            </p>
          ) : null}
          {serversQuery.data?.length === 0 ? (
            <div className="mt-4">
              <EmptyState title="尚无 MCP" description="可新增或从工具导入。" />
            </div>
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
                    onClick={() => openForm(editMcpForm(server))}
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
                  {filterEnabledTools(MCP_TOOLS, enabledTools).map((tool) => (
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
                      {listLayout === "list" ? serverActions : null}
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
                        {server.redactedExtra !== null ? (
                          <pre className="bg-muted rounded-control mt-3 overflow-auto p-2 text-xs">
                            {JSON.stringify(server.redactedExtra, null, 2)}
                          </pre>
                        ) : null}
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
                      {listLayout === "grid" ? serverActions : null}
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
          aria-labelledby="mcp-target-title"
        >
          <h2 id="mcp-target-title" className="text-[15px] font-semibold">
            全局目标状态
          </h2>
          {statusesQuery.isPending ? (
            <p role="status" className="mt-3 text-sm">
              正在检测全局 MCP 目标…
            </p>
          ) : null}
          {statusesQuery.isError ? (
            <p role="alert" className="text-destructive mt-3 text-sm">
              {profileErrorText(statusesQuery.error)}
            </p>
          ) : null}
          {statusesQuery.data != null && visibleStatuses?.length === 0 ? (
            <p className="text-muted-foreground mt-3 text-sm">
              当前没有可检查的全局 MCP 目标。
            </p>
          ) : null}
          {visibleStatuses && visibleStatuses.length > 0 ? (
            <div className="mt-3 divide-y">
              {visibleStatuses.map((status) => {
                const presentation = globalTargetStatusPresentation(
                  status.status,
                  status.diagnosticCode,
                  { directApply },
                );
                return (
                  <article
                    key={status.tool}
                    className="hover:bg-muted/50 px-1 py-2.5 text-sm"
                  >
                    <p className="font-medium">
                      {toolMetadata(status.tool).label}
                    </p>
                    <code className="mt-2 block text-xs break-all">
                      {status.targetPath ?? "目标位置不可用"}
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
                      <p className="text-warning mt-2 text-xs">
                        诊断码：<code>{status.diagnosticCode}</code>
                      </p>
                    ) : null}
                    <Button
                      className="mt-3 mr-2"
                      size="sm"
                      variant="outline"
                      disabled={presentation.previewBlocked}
                      onClick={() => {
                        if (importDialog.state) return;
                        importDialog.open(status.tool);
                      }}
                    >
                      检测并导入已有 MCP
                    </Button>
                    {!directApply ? (
                      <Button
                        className="mt-3"
                        size="sm"
                        disabled={
                          previewMutation.isPending ||
                          presentation.previewBlocked
                        }
                        onClick={() => requestPreview(status.tool, directApply)}
                      >
                        {previewMutation.isPending
                          ? "正在生成…"
                          : "生成全局预览"}
                      </Button>
                    ) : null}
                  </article>
                );
              })}
            </div>
          ) : null}
        </section>

        {formOpen ? (
          <McpFormDialog
            // 按记录 id 重挂载：切换编辑对象时草稿与本地校验错误必定重置，不依赖弹窗先关闭。
            key={formInitial.id ?? "new"}
            initialState={formInitial}
            directApply={directApply}
            pending={saveMutation.isPending}
            saveError={profileErrorText(saveMutation.error)}
            onClose={closeForm}
            onSubmit={(state) => {
              if (submitGuard.isInFlight() || saveMutation.isPending) return;
              saveMutation.reset();
              if (!submitGuard.begin()) return;
              saveMutation.mutate({
                state,
                globalTools: state.id
                  ? (serversQuery.data?.find((item) => item.id === state.id)
                      ?.globalTools ?? [])
                  : [],
              });
            }}
          />
        ) : null}

        {importDialog.state ? (
          <McpImportDialog
            key={importDialog.state.requestId}
            tool={importDialog.state.tool}
            requestId={importDialog.state.requestId}
            onClose={importDialog.close}
            onRescan={importDialog.rescan}
            onImported={async (result) => {
              importDialog.close();
              await invalidateMcp();
              const summary = `已导入 ${result.createdCount + result.reusedCount} 项 MCP（新建 ${result.createdCount} 项，复用 ${result.reusedCount} 项），已分配到 ${toolMetadata(result.tool).label} 全局。`;
              if (!directApply) {
                notify({
                  kind: "success",
                  message: `${summary}原生配置未改写，请单独生成全局预览。`,
                });
                return;
              }
              notify({
                kind: "success",
                message: `${summary}正在自动同步写入。`,
              });
              await previewMutation
                .mutateAsync({ tool: result.tool, autoApply: true })
                .catch(() => undefined);
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
              readoptMutation.mutate(openPreview.tool);
            }
          }}
          onClose={closePreview}
          onApply={() => {
            if (openPreview) {
              applyMutation.mutate({
                previewId: openPreview.plan.previewId,
                tool: openPreview.tool,
              });
            }
          }}
        />
      </main>
    </>
  );
}
