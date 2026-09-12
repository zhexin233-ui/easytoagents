import { useId, type FormEvent, type ReactNode } from "react";

import {
  DialogBody,
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
      >
        <DialogHeader>
          <div className="min-w-0">
            <h2 id={titleId} className="text-[15px] font-semibold">
              {title}
            </h2>
            <p id={descriptionId} className="text-muted-foreground mt-1">
              {description}
            </p>
          </div>
        </DialogHeader>
        <form
          aria-labelledby={titleId}
          className="flex min-h-0 flex-1 flex-col"
          onSubmit={(event) => {
            event.preventDefault();
            if (!pending) {
              // 提交按钮即将禁用，先保留弹窗焦点，避免浏览器将焦点移回页面。
              dialogRef.current?.focus();
              onSubmit(event);
            }
          }}
        >
          <DialogBody className="space-y-4">{children}</DialogBody>
          <DialogFooter>
            {error ? (
              <p role="alert" className="text-destructive mr-auto">
                {error}
              </p>
            ) : null}
            {pending ? (
              <p role="status" className="text-muted-foreground mr-auto">
                正在保存，请稍候…
              </p>
            ) : null}
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
