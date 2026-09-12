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
import {
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { mcpImportQueryOptions } from "@/lib/mcp-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { toolMetadata } from "@/lib/tool-metadata";

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
  const { dialogRef } = useDialogFocus(true, close);
  const preview = query.data;
  const error = profileErrorText(query.error ?? confirm.error);

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={close}
        labelledBy="mcp-import-title"
        describedBy="mcp-import-description"
        size="lg"
      >
        <DialogHeader>
          <h2 id="mcp-import-title" className="text-[15px] font-semibold">
            导入 {toolMetadata(props.tool).label} 全局 MCP
          </h2>
        </DialogHeader>
        <DialogBody className="space-y-4">
          <p id="mcp-import-description" className="text-muted-foreground">
            仅将勾选项纳入中央库并分配到来源工具；不修改原生配置。后续写入仍需单独预览并
            Apply。
          </p>
          {query.isPending ? <p role="status">正在检测已有全局 MCP…</p> : null}
          {error ? (
            <p role="alert" className="text-destructive">
              {error} 请重新检测后再确认。
            </p>
          ) : null}
          {preview ? (
            <>
              <code className="block text-xs break-all">
                {preview.targetPath}
              </code>
              {preview.message ? <p role="status">{preview.message}</p> : null}
              {preview.candidates.length > 0 && !preview.previewId ? (
                <p role="status">没有可导入项，请查看各条目的状态和原因。</p>
              ) : null}
              <div className="space-y-3">
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
                      <p className="text-warning mt-2 text-xs">
                        {candidate.reason}
                      </p>
                    ) : null}
                    {candidate.redactedProjection !== null ? (
                      <pre className="bg-muted rounded-control mt-3 overflow-auto p-2 text-xs">
                        {JSON.stringify(candidate.redactedProjection, null, 2)}
                      </pre>
                    ) : null}
                  </article>
                ))}
              </div>
            </>
          ) : null}
        </DialogBody>
        <DialogFooter>
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
        </DialogFooter>
      </DialogContent>
    </DialogOverlay>
  );
}
