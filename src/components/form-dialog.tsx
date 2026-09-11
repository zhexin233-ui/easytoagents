import { useId, type FormEvent, type ReactNode } from "react";

import {
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";

interface FormDialogProps {
  open: boolean;
  title: string;
  description: string;
  submitLabel: string;
  pending: boolean;
  submitDisabled?: boolean;
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
  submitDisabled = false,
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
  const { dialogRef } = useDialogFocus(open, close);

  if (!open) return null;

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={close}
        labelledBy={titleId}
        describedBy={descriptionId}
        className="flex max-h-[calc(100dvh-2rem)] max-w-2xl min-w-0 flex-col overflow-hidden p-0"
      >
        <DialogHeader className="shrink-0 border-b p-6">
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
        </DialogHeader>
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
              <p role="alert" className="text-destructive text-sm">
                {error}
              </p>
            ) : null}
            {pending ? (
              <p role="status" className="text-muted-foreground text-sm">
                正在保存，请稍候…
              </p>
            ) : null}
          </div>
          <DialogFooter className="shrink-0 border-t px-6 py-4">
            <Button
              type="button"
              variant="outline"
              disabled={pending}
              onClick={close}
            >
              取消
            </Button>
            <Button type="submit" disabled={pending || submitDisabled}>
              {pending ? "正在保存…" : submitLabel}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </DialogOverlay>
  );
}
