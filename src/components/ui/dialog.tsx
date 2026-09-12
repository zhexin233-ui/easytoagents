import {
  useId,
  type HTMLAttributes,
  type KeyboardEvent,
  type RefObject,
  type ReactNode,
} from "react";

import { useDialogFocus } from "@/components/use-dialog-focus";
import { cn } from "@/lib/utils";

interface DialogOverlayProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
  onClose?: () => void;
  closeOnOutsideClick?: boolean;
}

/** Shared modal backdrop. Keeping the backdrop here prevents subtle z-index and
 * light/dark color drift between feature dialogs. */
export function DialogOverlay({
  children,
  className,
  onClose,
  closeOnOutsideClick = false,
  ...props
}: DialogOverlayProps) {
  return (
    <div
      {...props}
      role="presentation"
      className={cn(
        "bg-foreground/30 fixed inset-0 z-50 grid place-items-center p-4 backdrop-blur-[2px] dark:bg-black/50",
        className,
      )}
      onMouseDown={(event) => {
        props.onMouseDown?.(event);
        if (closeOnOutsideClick && event.target === event.currentTarget) {
          onClose?.();
        }
      }}
    >
      {children}
    </div>
  );
}

const dialogSizeClasses = {
  sm: "max-w-md",
  md: "max-w-xl",
  lg: "max-w-3xl",
} as const;

interface DialogContentProps extends Omit<
  HTMLAttributes<HTMLElement>,
  "onKeyDown"
> {
  children: ReactNode;
  onClose?: () => void;
  labelledBy?: string;
  describedBy?: string;
  dialogRef?: RefObject<HTMLElement | null>;
  /** 内容宽度档位：sm 448（确认类）/ md 576（表单、选择器，默认）/ lg 768（预览、导入）。 */
  size?: keyof typeof dialogSizeClasses;
}

/** Three-part sheet: fixed header / scrollable body / fixed footer. Callers
 * compose DialogHeader + DialogBody + DialogFooter inside; the content itself
 * never scrolls so titles and actions stay visible. */
export function DialogContent({
  children,
  className,
  onClose,
  labelledBy,
  describedBy,
  dialogRef: externalDialogRef,
  size = "md",
  ...props
}: DialogContentProps) {
  const generatedTitleId = useId();
  const { dialogRef, onKeyDown } = useDialogFocus(
    true,
    onClose ?? (() => {}),
    externalDialogRef,
  );

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    onKeyDown(event);
  };

  return (
    <section
      {...props}
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby={labelledBy ?? generatedTitleId}
      aria-describedby={describedBy}
      tabIndex={-1}
      onKeyDown={handleKeyDown}
      className={cn(
        "rounded-dialog bg-card flex max-h-[calc(100dvh-4rem)] w-full flex-col overflow-hidden p-0 shadow-xl",
        dialogSizeClasses[size],
        className,
      )}
    >
      {children}
    </section>
  );
}

export function DialogHeader({
  children,
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      {...props}
      className={cn(
        "flex shrink-0 items-start justify-between gap-4 px-5 pt-5 pb-3",
        className,
      )}
    >
      {children}
    </div>
  );
}

export function DialogBody({
  children,
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      {...props}
      className={cn("min-h-0 flex-1 overflow-y-auto px-5 py-3", className)}
    >
      {children}
    </div>
  );
}

export function DialogFooter({
  children,
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      {...props}
      className={cn(
        "flex shrink-0 flex-wrap items-center justify-end gap-2 px-5 pt-3 pb-5",
        className,
      )}
    >
      {children}
    </div>
  );
}
