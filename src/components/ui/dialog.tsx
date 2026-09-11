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
        "fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4",
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

interface DialogContentProps extends Omit<
  HTMLAttributes<HTMLElement>,
  "onKeyDown"
> {
  children: ReactNode;
  onClose?: () => void;
  labelledBy?: string;
  describedBy?: string;
  dialogRef?: RefObject<HTMLElement | null>;
}

export function DialogContent({
  children,
  className,
  onClose,
  labelledBy,
  describedBy,
  dialogRef: externalDialogRef,
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
        "bg-card max-h-[90vh] w-full max-w-3xl overflow-auto rounded-xl p-6 shadow-xl",
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
      className={cn("flex items-start justify-between gap-4", className)}
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
      className={cn("mt-6 flex flex-wrap justify-end gap-3", className)}
    >
      {children}
    </div>
  );
}
