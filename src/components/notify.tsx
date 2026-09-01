import { cn } from "@/lib/utils";

export interface NotifyMessage {
  kind: "success" | "error";
  message: string;
}

interface NotifyProps {
  notification: NotifyMessage | null;
}

export function Notify({ notification }: NotifyProps) {
  if (!notification) return null;

  const failure = notification.kind === "error";

  return (
    <div
      role={failure ? "alert" : "status"}
      aria-atomic="true"
      className={cn(
        "fixed top-4 right-4 z-[60] w-[min(calc(100vw-2rem),24rem)] rounded-lg border p-4 text-sm shadow-lg",
        failure
          ? "border-red-200 bg-red-50 text-red-950 dark:border-red-900/60 dark:bg-red-950/90 dark:text-red-200"
          : "border-emerald-200 bg-emerald-50 text-emerald-950 dark:border-emerald-900/60 dark:bg-emerald-950/90 dark:text-emerald-200",
      )}
    >
      {notification.message}
    </div>
  );
}
