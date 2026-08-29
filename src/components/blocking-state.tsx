import { Button } from "@/components/ui/button";

export function BlockingState({
  title,
  description,
  code,
  actionLabel,
  onAction,
}: {
  title: string;
  description: string;
  code?: string | null;
  actionLabel?: string;
  onAction?: () => void;
}) {
  return (
    <div
      role="alert"
      className="rounded-lg border border-amber-200 bg-amber-50 p-4 text-sm text-amber-950 dark:border-amber-900/60 dark:bg-amber-950/40 dark:text-amber-300"
    >
      <p className="font-semibold">⛔ {title}</p>
      <p className="mt-1 leading-6">{description}</p>
      {code ? <code className="mt-2 block text-xs">{code}</code> : null}
      {actionLabel && onAction ? (
        <Button className="mt-3" size="sm" variant="outline" onClick={onAction}>
          {actionLabel}
        </Button>
      ) : null}
    </div>
  );
}
