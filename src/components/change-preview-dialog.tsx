import type {
  ArtifactKind,
  DatabaseRowVersion,
  PreviewPlan,
  Tool,
} from "@/bindings/commands";
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
  onReadopt?: (targetPath: string) => void;
  adoptingNative?: boolean;
  /**
   * 把目标文件当前内容写回中央档案并刷新基线。
   *
   * 与 `onReadopt` 的区别：重新接管只刷新基线，档案保持旧内容，因此下一次 Apply
   * 会把用户手改的原生内容改回去；本操作让「文件怎样就以文件为准」。
   * 传入预览绑定的行版本，供服务端做乐观并发校验。
   */
  onAdoptNative?: (
    targetPath: string,
    rowVersions: DatabaseRowVersion[],
  ) => void;
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
  adoptingNative = false,
  onAdoptNative,
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
          <div className="min-w-0 divide-y">
            {preview.targets.map((target) => (
              <article
                key={target.targetId}
                className="hover:bg-muted/50 min-w-0 px-1 py-2.5"
              >
                <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
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
                    {target.readoptAvailable &&
                    (onReadopt || onAdoptNative) &&
                    target.descriptor.path ? (
                      <div className="mt-3 space-y-2">
                        <p className="text-muted-foreground text-xs leading-5">
                          重新接管只更新基线，不会立即修改文件。
                        </p>
                        <div className="flex flex-wrap gap-2">
                          {onReadopt ? (
                            <Button
                              variant="outline"
                              size="sm"
                              disabled={
                                readopting || adoptingNative || applying
                              }
                              aria-label={`以当前内容重新接管 ${target.descriptor.path ?? "目标"}`}
                              onClick={() => {
                                const targetPath = target.descriptor.path;
                                if (targetPath) onReadopt(targetPath);
                              }}
                            >
                              {readopting
                                ? "正在重新接管…"
                                : "以当前内容重新接管"}
                            </Button>
                          ) : null}
                          {onAdoptNative ? (
                            <Button
                              variant="outline"
                              size="sm"
                              disabled={
                                readopting || adoptingNative || applying
                              }
                              aria-label={`按原生内容接管渠道档案 ${target.descriptor.path ?? "目标"}`}
                              onClick={() => {
                                const targetPath = target.descriptor.path;
                                if (targetPath)
                                  onAdoptNative(targetPath, target.rowVersions);
                              }}
                            >
                              {adoptingNative
                                ? "正在按原生内容接管…"
                                : "按原生内容接管渠道档案"}
                            </Button>
                          ) : null}
                        </div>
                        {onAdoptNative ? (
                          <p className="text-muted-foreground text-xs leading-5">
                            按原生内容接管会把文件当前内容写回中央渠道档案，
                            之后同步不再改写该文件。
                          </p>
                        ) : null}
                      </div>
                    ) : null}
                  </div>
                ) : null}
                <pre className="bg-muted rounded-control mt-3 max-w-full overflow-auto p-3 text-xs leading-5">
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
            非受管字段与表会被保留。
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
