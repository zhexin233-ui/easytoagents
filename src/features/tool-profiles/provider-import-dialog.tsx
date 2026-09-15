import { useState } from "react";
import { useMutation } from "@tanstack/react-query";

import {
  commands,
  type ProviderImportCandidateDto,
  type ProviderImportPreviewDto,
  type ProviderImportResultDto,
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
import {
  providerCandidateCredentialText,
  providerCandidateReasonText,
  providerCandidateStatusText,
  providerModelText,
} from "@/features/tool-profiles/provider-text";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { toolMetadata } from "@/lib/tool-metadata";

interface ProviderImportDialogProps {
  tool: Tool;
  preview: ProviderImportPreviewDto;
  onClose: () => void;
  onImported: (result: ProviderImportResultDto) => Promise<void>;
}

/** 默认勾选：优先默认渠道；没有默认渠道时选中唯一的可导入候选。 */
function initialSelection(
  candidates: readonly ProviderImportCandidateDto[],
): string[] {
  const importable = candidates.filter(
    (candidate) => candidate.status === "importable",
  );
  const preferred = importable.find((candidate) => candidate.defaultProvider);
  if (preferred) return [preferred.candidateId];
  const [only] = importable;
  if (only && importable.length === 1) return [only.candidateId];
  return [];
}

export function ProviderImportDialog({
  tool,
  preview,
  onClose,
  onImported,
}: ProviderImportDialogProps) {
  const [selectedIds, setSelectedIds] = useState<string[]>(() =>
    initialSelection(preview.candidates),
  );
  const [names, setNames] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      preview.candidates.map((candidate) => [
        candidate.candidateId,
        candidate.suggestedName,
      ]),
    ),
  );
  const confirm = useMutation({
    mutationFn: async () =>
      unwrapResult(
        await commands.confirmProviderImport({
          previewId: preview.previewId ?? "",
          items: selectedIds.map((candidateId) => ({
            candidateId,
            name: names[candidateId] ?? "",
          })),
        }),
      ),
    onSuccess: onImported,
  });
  const close = () => {
    if (!confirm.isPending) onClose();
  };
  const { dialogRef } = useDialogFocus(true, close);
  const error = profileErrorText(confirm.error);

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={close}
        labelledBy="provider-import-title"
        describedBy="provider-import-description"
        size="lg"
      >
        <DialogHeader>
          <h2 id="provider-import-title" className="text-[15px] font-semibold">
            导入 {toolMetadata(tool).label} 已有渠道
          </h2>
        </DialogHeader>
        <DialogBody className="space-y-4">
          <p id="provider-import-description" className="text-muted-foreground">
            只接管到中央档案并建立受管基线，原生文件内容保持不变。
          </p>
          <code className="block text-xs break-all">{preview.targetPath}</code>
          {preview.message ? <p role="status">{preview.message}</p> : null}
          {error ? (
            <p role="alert" className="text-destructive">
              {error} 请重新检测后再确认。
            </p>
          ) : null}
          <div className="space-y-3">
            {preview.candidates.map((candidate) => {
              const selectable = candidate.status === "importable";
              return (
                <article
                  key={candidate.candidateId}
                  className="rounded-lg border p-4 text-sm"
                >
                  <label className="flex items-center gap-2 font-medium">
                    <input
                      type="checkbox"
                      aria-label={`导入 ${candidate.suggestedName}`}
                      checked={selectedIds.includes(candidate.candidateId)}
                      disabled={!selectable || confirm.isPending}
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
                    {candidate.suggestedName}
                    {candidate.defaultProvider ? (
                      <span className="text-muted-foreground text-xs">
                        默认渠道
                      </span>
                    ) : null}
                  </label>
                  <p className="text-muted-foreground mt-2 text-xs">
                    {providerCandidateStatusText[candidate.status]}
                    {candidate.reason
                      ? ` · ${providerCandidateReasonText(candidate.reason)}`
                      : ""}
                    {` · ${providerModelText(candidate.defaultModel)}`}
                    {` · ${providerCandidateCredentialText(candidate)}`}
                  </p>
                  {candidate.apiFormat ? (
                    <p className="text-muted-foreground mt-1 text-xs">
                      {`API 格式 ${candidate.apiFormat} · ${candidate.modelCount} 个模型`}
                    </p>
                  ) : null}
                  {candidate.skippedEnvKeys.length > 0 ? (
                    <p className="mt-1 text-xs">
                      以下 env 疑似凭据或格式不受支持，不纳入管理并保持原样：
                      {candidate.skippedEnvKeys.join("、")}
                    </p>
                  ) : null}
                  {selectable ? (
                    <label className="mt-3 block text-xs">
                      导入名称
                      <input
                        className="border-input mt-1 w-full rounded border px-2 py-1 text-sm"
                        value={names[candidate.candidateId] ?? ""}
                        disabled={confirm.isPending}
                        onChange={(event) =>
                          setNames((current) => ({
                            ...current,
                            [candidate.candidateId]: event.target.value,
                          }))
                        }
                      />
                    </label>
                  ) : null}
                </article>
              );
            })}
          </div>
          <details className="text-xs">
            <summary className="cursor-pointer">查看脱敏后的原生投影</summary>
            {preview.candidates.map((candidate) => (
              <pre
                key={candidate.candidateId}
                className="bg-card rounded-control mt-2 max-w-full overflow-auto p-3"
              >
                {JSON.stringify(candidate.redactedProjection, null, 2)}
              </pre>
            ))}
          </details>
        </DialogBody>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={close}>
            取消
          </Button>
          <Button
            size="sm"
            disabled={selectedIds.length === 0 || confirm.isPending}
            onClick={() => confirm.mutate()}
          >
            {confirm.isPending
              ? "正在导入…"
              : `确认导入 ${selectedIds.length} 个渠道`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </DialogOverlay>
  );
}
