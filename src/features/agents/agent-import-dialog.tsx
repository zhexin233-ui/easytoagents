import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";

import {
  commands,
  type AgentImportCandidateDto,
  type AgentImportPreviewDto,
  type AgentImportResultDto,
  type ConfirmAgentImportInput,
  type Tool,
} from "@/bindings/commands";
import {
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { agentImportQueryOptions } from "@/lib/agents-api";
import { profileErrorText, unwrapResult } from "@/lib/rpc";
import { toolMetadata } from "@/lib/tool-metadata";

interface AgentImportDialogProps {
  tool: Tool;
  requestId: string;
  onClose: () => void;
  onRescan: () => void;
  onImported: (result: AgentImportResultDto) => Promise<void>;
}

interface AgentImportCandidateCardProps {
  candidate: AgentImportCandidateDto;
  selected: boolean;
  disabled: boolean;
  onToggle: (checked: boolean) => void;
}

/** 只读扫描工具全局 Agents 目录，并把用户明确选中的定义复制到中央库。 */
export function AgentImportDialog({
  tool,
  requestId,
  onClose,
  onRescan,
  onImported,
}: AgentImportDialogProps) {
  const query = useQuery(agentImportQueryOptions(tool, requestId));
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const confirm = useMutation({
    mutationFn: async (input: ConfirmAgentImportInput) =>
      unwrapResult(await commands.confirmAgentImport(input)),
    onSuccess: onImported,
  });
  const close = () => {
    if (!confirm.isPending) onClose();
  };
  const { dialogRef } = useDialogFocus(true, close);
  const preview = query.data;
  const error = profileErrorText(query.error ?? confirm.error);
  const selectableIds = new Set(
    (preview?.candidates ?? [])
      .filter((candidate) => candidate.importable)
      .map((candidate) => candidate.candidateId),
  );
  const selectedCount = selectedIds.filter((id) =>
    selectableIds.has(id),
  ).length;

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={close}
        labelledBy="agent-import-title"
        describedBy="agent-import-description"
        size="lg"
      >
        <DialogHeader>
          <div className="min-w-0">
            <h2 id="agent-import-title" className="text-[15px] font-semibold">
              导入 {toolMetadata(tool).label} 全局 Agents
            </h2>
          </div>
        </DialogHeader>
        <DialogBody className="space-y-4">
          <p id="agent-import-description" className="text-muted-foreground">
            只读扫描工具全局 Agents
            目录；确认后复制到中央库并记住当前内容，不修改原生文件或自动分配。
          </p>
          {query.isPending ? (
            <p role="status">正在检测已有全局 Agents…</p>
          ) : null}
          {error ? (
            <p role="alert" className="text-destructive">
              {error} 请重新检测后再确认。
            </p>
          ) : null}
          {preview ? (
            <AgentImportPreview
              preview={preview}
              selectedIds={selectedIds}
              confirmPending={confirm.isPending || confirm.isError}
              onToggle={(candidateId, checked) =>
                setSelectedIds((current) =>
                  checked
                    ? [...current, candidateId]
                    : current.filter((id) => id !== candidateId),
                )
              }
            />
          ) : null}
          {preview && preview.candidates.length === 0 ? (
            <p className="text-muted-foreground">未发现可扫描的 Agent 文件。</p>
          ) : null}
        </DialogBody>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={confirm.isPending || query.isPending}
            onClick={onRescan}
          >
            重新检测
          </Button>
          <Button
            type="button"
            disabled={
              selectedCount === 0 ||
              confirm.isPending ||
              confirm.isError ||
              query.isPending
            }
            onClick={() => {
              const selected = (preview?.candidates ?? []).filter(
                (candidate) =>
                  candidate.importable &&
                  selectedIds.includes(candidate.candidateId),
              );
              if (selected.length === 0) return;
              confirm.mutate({
                tool,
                agents: selected.map((candidate) => ({
                  definition: {
                    name: candidate.name,
                    description: candidate.description,
                    prompt: candidate.prompt,
                    enabled: true,
                  },
                  toolSettings: candidate.toolSettings,
                })),
              });
            }}
          >
            {confirm.isPending
              ? "正在导入…"
              : `确认导入所选项（${selectedCount}）`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </DialogOverlay>
  );
}

function AgentImportPreview({
  preview,
  selectedIds,
  confirmPending,
  onToggle,
}: {
  preview: AgentImportPreviewDto;
  selectedIds: readonly string[];
  confirmPending: boolean;
  onToggle: (candidateId: string, checked: boolean) => void;
}) {
  return (
    <>
      <code className="block text-xs break-all">{preview.directoryPath}</code>
      {preview.message ? <p role="status">{preview.message}</p> : null}
      <div className="space-y-3">
        {preview.candidates.map((candidate) => (
          <AgentImportCandidateCard
            key={candidate.candidateId}
            candidate={candidate}
            selected={selectedIds.includes(candidate.candidateId)}
            disabled={confirmPending}
            onToggle={(checked) => onToggle(candidate.candidateId, checked)}
          />
        ))}
      </div>
    </>
  );
}

function AgentImportCandidateCard({
  candidate,
  selected,
  disabled,
  onToggle,
}: AgentImportCandidateCardProps) {
  const importable = candidate.importable;
  return (
    <article className="rounded-lg border p-4 text-sm">
      <label className="flex items-start gap-2 font-medium">
        <input
          type="checkbox"
          aria-label={`导入 ${candidate.name || candidate.sourcePath}`}
          checked={selected}
          disabled={!importable || disabled}
          onChange={(event) => onToggle(event.target.checked)}
        />
        <span className="min-w-0 break-all">
          {candidate.name || "未命名 Agent"}
          {!importable ? "（不可导入）" : ""}
        </span>
      </label>
      <p className="text-muted-foreground mt-2 text-xs break-all">
        {candidate.sourcePath}
      </p>
      {candidate.description ? (
        <p className="mt-2 line-clamp-2 text-xs">{candidate.description}</p>
      ) : null}
      {candidate.droppedFields.length > 0 ? (
        <p className="text-warning mt-2 text-xs">
          将丢弃工具特有字段：{candidate.droppedFields.join("、")}
        </p>
      ) : null}
      {candidate.retainedFields.length > 0 ? (
        <p className="text-success mt-2 text-xs">
          将保留工具特有字段：{candidate.retainedFields.join("、")}
        </p>
      ) : null}
      {!importable && candidate.diagnosticCode ? (
        <p role="alert" className="text-destructive mt-2 text-xs break-all">
          诊断：{candidate.diagnosticCode}
          {candidate.reason ? ` · ${candidate.reason}` : ""}
        </p>
      ) : candidate.reason ? (
        <p className="text-warning mt-2 text-xs">{candidate.reason}</p>
      ) : null}
    </article>
  );
}
