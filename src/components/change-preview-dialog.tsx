import type { ArtifactKind, PreviewPlan, Tool } from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { toneClass } from "@/lib/tone-class";

interface ChangePreviewDialogProps {
  preview: PreviewPlan | null;
  tool: Tool;
  artifactKind: ArtifactKind;
  applying: boolean;
  readopting?: boolean;
  onReadopt?: () => void;
  onClose: () => void;
  onApply: (previewId: string, tool: Tool, artifactKind: ArtifactKind) => void;
}

export function ChangePreviewDialog({
  preview,
  tool,
  artifactKind,
  applying,
  readopting = false,
  onReadopt,
  onClose,
  onApply,
}: ChangePreviewDialogProps) {
  const { dialogRef } = useDialogFocus(preview !== null, onClose);

  if (!preview) {
    return null;
  }

  const blocked = preview.targets.some(
    (target) => target.changeKind === "conflict" || target.errorCode !== null,
  );

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={onClose}
        labelledBy="change-preview-title"
        describedBy="change-preview-description"
        size="lg"
      >
        <DialogHeader>
          <h2 id="change-preview-title" className="text-[15px] font-semibold">
            确认原生配置变更
          </h2>
        </DialogHeader>

        <DialogBody>
          {preview.warningCodes.length > 0 ? (
            <section
              aria-label="预览警告"
              className={`mb-3 rounded-lg border p-4 ${toneClass("warning")}`}
            >
              <ul className="text-warning list-disc pl-5 text-sm">
                {preview.warningCodes.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            </section>
          ) : null}
          <div className="divide-y">
            {preview.targets.map((target) => (
              <article
                key={target.targetId}
                className="hover:bg-muted/50 px-1 py-2.5"
              >
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
                  <ul className="text-warning mt-3 list-disc pl-5 text-sm">
                    {target.warningCodes.map((warning) => (
                      <li key={warning}>{warning}</li>
                    ))}
                  </ul>
                ) : null}
                {target.baselineMismatchedItems.length > 0 ? (
                  <p className="text-warning mt-3 text-sm">
                    内容不一致的受管条目：
                    {target.baselineMismatchedItems.join("、")}
                  </p>
                ) : null}
                {target.errorCode ? (
                  <div className="mt-3">
                    <BlockingState
                      title="该目标阻止应用"
                      description="请先重新扫描或处理冲突，再生成一份新的预览。"
                      code={target.errorCode}
                    />
                    {target.readoptAvailable && onReadopt ? (
                      <div className="mt-3 space-y-2">
                        <p className="text-muted-foreground text-xs leading-5">
                          若接受当前文件内容作为新基线，可重新接管；之后重新同步会把中央意图写回受管条目。只调整基线，不会立即修改文件。
                        </p>
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={readopting || applying}
                          aria-label={`以当前内容重新接管 ${target.descriptor.path ?? "目标"}`}
                          onClick={onReadopt}
                        >
                          {readopting ? "正在重新接管…" : "以当前内容重新接管"}
                        </Button>
                      </div>
                    ) : null}
                  </div>
                ) : null}
                <pre className="bg-muted rounded-control mt-3 overflow-auto p-3 text-xs leading-5">
                  {JSON.stringify(target.redactedDiff, null, 2)}
                </pre>
              </article>
            ))}
          </div>
        </DialogBody>

        <DialogFooter>
          <p
            id="change-preview-description"
            className="text-muted-foreground mr-auto"
          >
            非受管字段与表会被保留。Apply 会再次校验目标 hash 与数据库版本。
          </p>
          <Button variant="outline" onClick={onClose}>
            取消
          </Button>
          <Button
            disabled={blocked || applying}
            onClick={() => onApply(preview.previewId, tool, artifactKind)}
          >
            {applying ? "正在应用…" : "应用这份预览"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </DialogOverlay>
  );
}
