import { useEffect, useRef } from "react";

import type { ArtifactKind, PreviewPlan, Tool } from "@/bindings/commands";
import { Button } from "@/components/ui/button";

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
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (preview) {
      previousFocusRef.current =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
      closeButtonRef.current?.focus();
      return () => previousFocusRef.current?.focus();
    }
    return undefined;
  }, [preview]);

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
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          onClose();
        }
      }}
    >
      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby="change-preview-title"
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
          <Button
            ref={closeButtonRef}
            variant="outline"
            size="sm"
            onClick={onClose}
          >
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
                <span className="bg-muted rounded-full px-2 py-1 text-xs">
                  {target.changeKind} · {target.status}
                </span>
              </div>
              {target.warningCodes.length > 0 ? (
                <ul className="mt-3 list-disc pl-5 text-sm text-amber-800">
                  {target.warningCodes.map((warning) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              ) : null}
              {target.errorCode ? (
                <p role="alert" className="mt-3 text-sm text-red-700">
                  阻止应用：{target.errorCode}
                </p>
              ) : null}
              <pre className="bg-muted mt-3 overflow-auto rounded-md p-3 text-xs leading-5">
                {JSON.stringify(target.redactedDiff, null, 2)}
              </pre>
            </article>
          ))}
        </div>

        <p className="text-muted-foreground mt-4 text-sm">
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
