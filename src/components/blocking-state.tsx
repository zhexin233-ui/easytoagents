import { OctagonAlert } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toneClass } from "@/lib/tone-class";

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
      className={`rounded-lg border p-4 text-sm ${toneClass("warning")}`}
    >
      <p className="flex items-center gap-1.5 font-semibold">
        <OctagonAlert aria-hidden="true" className="size-4 shrink-0" />
        {title}
      </p>
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
