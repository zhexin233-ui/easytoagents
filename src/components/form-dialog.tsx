import { useId, type FormEvent, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";

interface FormDialogProps {
  open: boolean;
  title: string;
  description: string;
  submitLabel: string;
  pending: boolean;
  error: string | null;
  onClose: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  children: ReactNode;
}

export function FormDialog({
  open,
  title,
  description,
  submitLabel,
  pending,
  error,
  onClose,
  onSubmit,
  children,
}: FormDialogProps) {
  const titleId = useId();
  const descriptionId = useId();
  const close = () => {
    if (!pending) onClose();
  };
  const { dialogRef, onKeyDown } = useDialogFocus(open, close);

  if (!open) return null;

  return (
    <div
      role="presentation"
      className="fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4"
    >
      <section
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="bg-card flex max-h-[calc(100dvh-2rem)] w-full max-w-2xl min-w-0 flex-col overflow-hidden rounded-xl shadow-xl"
      >
        <div className="flex shrink-0 items-start justify-between gap-4 border-b p-6">
          <div className="min-w-0">
            <h2 id={titleId} className="text-xl font-semibold">
              {title}
            </h2>
            <p
              id={descriptionId}
              className="text-muted-foreground mt-2 text-sm"
            >
              {description}
            </p>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={pending}
            onClick={close}
          >
            关闭
          </Button>
        </div>
        <form
          aria-labelledby={titleId}
          className="flex min-h-0 flex-col"
          onSubmit={(event) => {
            event.preventDefault();
            if (!pending) {
              // 提交按钮即将禁用，先保留弹窗焦点，避免浏览器将焦点移回页面。
              dialogRef.current?.focus();
              onSubmit(event);
            }
          }}
        >
          <div className="min-h-0 space-y-4 overflow-y-auto p-6">
            {children}
            {error ? (
              <p
                role="alert"
                className="text-sm text-red-700 dark:text-red-300"
              >
                {error}
              </p>
            ) : null}
            {pending ? (
              <p role="status" className="text-muted-foreground text-sm">
                正在保存，请稍候…
              </p>
            ) : null}
          </div>
          <div className="flex shrink-0 flex-wrap justify-end gap-3 border-t px-6 py-4">
            <Button
              type="button"
              variant="outline"
              disabled={pending}
              onClick={close}
            >
              取消
            </Button>
            <Button type="submit" disabled={pending}>
              {pending ? "正在保存…" : submitLabel}
            </Button>
          </div>
        </form>
      </section>
    </div>
  );
}
