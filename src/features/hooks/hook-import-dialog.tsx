import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";

import {
  commands,
  type ConfirmHookImportInput,
  type HookEvent,
  type HookImportCandidateDto,
  type HookImportCandidateStatus,
  type HookImportResultDto,
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
import { hookImportQueryOptions } from "@/lib/hooks-api";
import { profileErrorText, unwrapResult } from "@/lib/rpc";
import { toolMetadata } from "@/lib/tool-metadata";

interface HookImportDialogProps {
  tool: Tool;
  requestId: string;
  onClose: () => void;
  onRescan: () => void;
  onImported: (result: HookImportResultDto) => Promise<void>;
}

const candidateLabels: Record<HookImportCandidateStatus, string> = {
  importable: "可导入",
  already_managed: "中央库已存在",
  name_conflict: "名称冲突",
  unsupported_event: "事件不受支持",
  invalid: "配置无效",
};

export function HookImportDialog(props: HookImportDialogProps) {
  const query = useQuery(hookImportQueryOptions(props.tool, props.requestId));
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const confirm = useMutation({
    mutationFn: async (input: ConfirmHookImportInput) =>
      unwrapResult(await commands.confirmHookImport(input)),
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
        labelledBy="hook-import-title"
        describedBy="hook-import-description"
        size="lg"
      >
        <DialogHeader>
          <h2 id="hook-import-title" className="text-[15px] font-semibold">
            导入 {toolMetadata(props.tool).label} 全局 Hooks
          </h2>
        </DialogHeader>
        <DialogBody className="space-y-4">
          <p id="hook-import-description" className="text-muted-foreground">
            只导入到中央库，不修改原生配置。
          </p>
          {query.isPending ? (
            <p role="status">正在检测已有全局 Hooks…</p>
          ) : null}
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
              {preview.candidates.length > 0 ? (
                <div className="space-y-3">
                  {preview.candidates.map((candidate) => (
                    <article
                      key={candidate.candidateId}
                      className="rounded-lg border p-4 text-sm"
                    >
                      <label className="flex items-center gap-2 font-medium">
                        <input
                          type="checkbox"
                          aria-label={`导入 ${candidate.name || candidate.command}`}
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
                        {candidate.name || candidate.command}
                      </label>
                      <p className="text-muted-foreground mt-2 text-xs">
                        {candidateLabels[candidate.status]}
                        {candidate.event ? ` · ${candidate.event}` : ""}
                        {candidate.matcher
                          ? ` · matcher: ${candidate.matcher}`
                          : ""}
                        {candidate.timeoutSeconds
                          ? ` · ${candidate.timeoutSeconds}s`
                          : ""}
                      </p>
                      <code className="mt-2 block text-xs break-all">
                        {candidate.command}
                      </code>
                      {candidate.reason ? (
                        <p className="text-warning mt-2 text-xs">
                          {candidate.reason}
                        </p>
                      ) : null}
                    </article>
                  ))}
                </div>
              ) : null}
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
              selectedIds.length === 0 ||
              confirm.isPending ||
              confirm.isError ||
              query.isPending
            }
            onClick={() => {
              const selected = (preview?.candidates ?? []).filter(
                (
                  candidate,
                ): candidate is HookImportCandidateDto & {
                  event: HookEvent;
                } =>
                  selectedIds.includes(candidate.candidateId) &&
                  candidate.status === "importable" &&
                  candidate.event !== null,
              );
              if (selected.length === 0) return;
              confirm.mutate({
                tool: props.tool,
                hooks: selected.map((candidate) => ({
                  name: candidate.name,
                  event: candidate.event,
                  matcher: candidate.matcher,
                  command: candidate.command,
                  timeoutSeconds: candidate.timeoutSeconds,
                  enabled: true,
                  scriptSourcePath: candidate.scriptSourcePath,
                })),
              });
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
