import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";

import {
  commands,
  type ConfirmMcpImportInput,
  type McpImportCandidateStatus,
  type McpImportResultDto,
  type Tool,
} from "@/bindings/commands";
import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { mcpImportQueryOptions } from "@/lib/mcp-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";

interface McpImportDialogProps {
  tool: Tool;
  requestId: string;
  onClose: () => void;
  onRescan: () => void;
  onImported: (result: McpImportResultDto) => Promise<void>;
}

const candidateLabels: Record<McpImportCandidateStatus, string> = {
  importable: "可导入",
  already_managed: "已纳入管理",
  name_conflict: "名称冲突",
  disabled: "原生已停用",
  unsupported: "暂不支持",
  invalid: "配置无效",
};

export function McpImportDialog(props: McpImportDialogProps) {
  const query = useQuery(mcpImportQueryOptions(props.tool, props.requestId));
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const confirm = useMutation({
    mutationFn: async (input: ConfirmMcpImportInput) =>
      unwrapResult(await commands.confirmMcpImport(input)),
    onSuccess: props.onImported,
  });
  const close = () => {
    if (!confirm.isPending) props.onClose();
  };
  const { dialogRef, onKeyDown } = useDialogFocus(true, close);
  const preview = query.data;
  const error = profileErrorText(query.error ?? confirm.error);

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/40 p-4">
      <section
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="mcp-import-title"
        aria-describedby="mcp-import-description"
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="max-h-[90vh] w-full max-w-3xl overflow-auto rounded-xl bg-white p-6 shadow-xl"
      >
        <div className="flex items-start justify-between gap-4">
          <h2 id="mcp-import-title" className="text-xl font-semibold">
            导入 {props.tool === "claude" ? "Claude" : "Codex"} 全局 MCP
          </h2>
          <Button
            variant="outline"
            disabled={confirm.isPending}
            onClick={close}
            aria-label="关闭 MCP 导入"
          >
            关闭
          </Button>
        </div>
        <p
          id="mcp-import-description"
          className="text-muted-foreground mt-3 text-sm"
        >
          仅将勾选项纳入中央库并分配到来源工具；不修改原生配置。后续写入仍需单独预览并
          Apply。
        </p>
        {query.isPending ? (
          <p role="status" className="mt-4">
            正在检测已有全局 MCP…
          </p>
        ) : null}
        {error ? (
          <p role="alert" className="mt-4 text-sm text-red-700">
            {error} 请重新检测后再确认。
          </p>
        ) : null}
        {preview ? (
          <>
            <code className="mt-4 block text-xs break-all">
              {preview.targetPath}
            </code>
            {preview.message ? (
              <p role="status" className="mt-4 text-sm">
                {preview.message}
              </p>
            ) : null}
            {preview.candidates.length > 0 && !preview.previewId ? (
              <p role="status" className="mt-4 text-sm">
                没有可导入项，请查看各条目的状态和原因。
              </p>
            ) : null}
            <div className="mt-4 space-y-3">
              {preview.candidates.map((candidate) => (
                <article
                  key={candidate.candidateId}
                  className="rounded-lg border p-4 text-sm"
                >
                  <label className="flex items-center gap-2 font-medium">
                    <input
                      type="checkbox"
                      aria-label={`导入 ${candidate.name}`}
                      checked={selectedIds.includes(candidate.candidateId)}
                      disabled={
                        candidate.status !== "importable" ||
                        confirm.isPending ||
                        confirm.isError
                      }
                      onChange={(event) => {
                        const checked = event.target.checked;
                        setSelectedIds((current) =>
                          checked
                            ? [...current, candidate.candidateId]
                            : current.filter(
                                (id) => id !== candidate.candidateId,
                              ),
                        );
                      }}
                    />
                    {candidate.name}
                  </label>
                  <p className="text-muted-foreground mt-2 text-xs">
                    {candidateLabels[candidate.status]}
                    {candidate.transport ? ` · ${candidate.transport}` : ""}
                  </p>
                  {candidate.action ? (
                    <p className="mt-2 text-xs">
                      {candidate.action === "reuse"
                        ? "复用相同配置的中央记录，并添加来源工具全局分配。"
                        : "新建中央记录，并添加来源工具全局分配。"}
                    </p>
                  ) : null}
                  {candidate.reason ? (
                    <p className="mt-2 text-xs text-amber-800">
                      {candidate.reason}
                    </p>
                  ) : null}
                  {candidate.redactedProjection !== null ? (
                    <pre className="bg-muted mt-3 overflow-auto rounded p-2 text-xs">
                      {JSON.stringify(candidate.redactedProjection, null, 2)}
                    </pre>
                  ) : null}
                </article>
              ))}
            </div>
          </>
        ) : null}
        <div className="mt-6 flex flex-wrap justify-end gap-3">
          <Button
            variant="outline"
            disabled={confirm.isPending || query.isPending}
            onClick={props.onRescan}
          >
            重新检测
          </Button>
          <Button
            disabled={
              !preview?.previewId ||
              selectedIds.length === 0 ||
              confirm.isPending ||
              confirm.isError ||
              query.isPending
            }
            onClick={() => {
              if (preview?.previewId) {
                confirm.mutate({
                  previewId: preview.previewId,
                  candidateIds: selectedIds,
                });
              }
            }}
          >
            {confirm.isPending
              ? "正在导入…"
              : `确认导入所选项（${selectedIds.length}）`}
          </Button>
        </div>
      </section>
    </div>
  );
}
