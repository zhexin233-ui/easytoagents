import type { ArtifactKind, PreviewPlan, Tool } from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";

interface ChangePreviewDialogProps {
  preview: PreviewPlan | null;
  tool: Tool;
  artifactKind: ArtifactKind;
  applying: boolean;
  onClose: () => void;
  onApply: (previewId: string, tool: Tool, artifactKind: ArtifactKind) => void;
}

export function ChangePreviewDialog({
  preview,
  tool,
  artifactKind,
  applying,
  onClose,
  onApply,
}: ChangePreviewDialogProps) {
  const { dialogRef, onKeyDown } = useDialogFocus(preview !== null, onClose);

  if (!preview) {
    return null;
  }

  const blocked = preview.targets.some(
    (target) => target.changeKind === "conflict" || target.errorCode !== null,
  );

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4"
      role="presentation"
    >
      <section
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="change-preview-title"
        aria-describedby="change-preview-description"
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="max-h-[88vh] w-full max-w-3xl overflow-auto rounded-xl bg-white p-6 shadow-xl"
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="text-muted-foreground text-sm">持久化预览</p>
            <h2
              id="change-preview-title"
              className="mt-1 text-xl font-semibold"
            >
              确认原生配置变更
            </h2>
          </div>
          <Button variant="outline" size="sm" onClick={onClose}>
            关闭
          </Button>
        </div>

        <div className="mt-5 space-y-4">
          {preview.warningCodes.length > 0 ? (
            <section
              aria-label="预览警告"
              className="rounded-lg border border-amber-200 bg-amber-50 p-4"
            >
              <ul className="list-disc pl-5 text-sm text-amber-800">
                {preview.warningCodes.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            </section>
          ) : null}
          {preview.targets.map((target) => (
            <article key={target.targetId} className="rounded-lg border p-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <code className="text-xs break-all">
                  {target.descriptor.path ?? "目标路径不可用"}
                </code>
                <SyncStatusBadge
                  changeKind={target.changeKind}
                  status={target.status}
                />
              </div>
              {target.warningCodes.length > 0 ? (
                <ul className="mt-3 list-disc pl-5 text-sm text-amber-800">
                  {target.warningCodes.map((warning) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              ) : null}
              {target.errorCode ? (
                <div className="mt-3">
                  <BlockingState
                    title="该目标阻止应用"
                    description="请先重新扫描或处理冲突，再生成一份新的预览。"
                    code={target.errorCode}
                  />
                </div>
              ) : null}
              <pre className="bg-muted mt-3 overflow-auto rounded-md p-3 text-xs leading-5">
                {JSON.stringify(target.redactedDiff, null, 2)}
              </pre>
            </article>
          ))}
        </div>

        <p
          id="change-preview-description"
          className="text-muted-foreground mt-4 text-sm"
        >
          非受管字段与表会被保留。Apply 会再次校验目标 hash 与数据库版本。
        </p>
        <div className="mt-6 flex justify-end gap-3">
          <Button variant="outline" onClick={onClose}>
            取消
          </Button>
          <Button
            disabled={blocked || applying}
            onClick={() => onApply(preview.previewId, tool, artifactKind)}
          >
            {applying ? "正在应用…" : "应用这份预览"}
          </Button>
        </div>
      </section>
    </div>
  );
}
