import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { toneClass } from "@/lib/tone-class";
import { cn } from "@/lib/utils";

export type ProjectSelectionState = "inherited" | "selected" | "available";

export function ProjectOptionRow({
  name,
  state,
  actionLabel,
  actionDisabled,
  onToggle,
  children,
}: {
  name: string;
  state: ProjectSelectionState;
  actionLabel: string;
  actionDisabled: boolean;
  onToggle: () => void;
  children?: ReactNode;
}) {
  const assigned = state !== "available";

  return (
    <div className="hover:bg-muted/50 flex min-h-11 items-center justify-between gap-3 px-1 py-2.5 text-sm">
      <div className="flex min-w-0 flex-wrap items-center gap-1.5">
        <span className="shrink-0">{name}</span>
        <StateTags state={state} />
        {children}
      </div>
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="shrink-0 shadow-none"
        aria-label={actionLabel}
        aria-pressed={assigned}
        disabled={actionDisabled}
        onClick={onToggle}
      >
        {assigned ? "禁用" : "启用"}
      </Button>
    </div>
  );
}

function StateTags({ state }: { state: ProjectSelectionState }) {
  switch (state) {
    case "inherited":
      return (
        <>
          <OptionTag tone="info">全局继承</OptionTag>
          <OptionTag tone="muted">只读</OptionTag>
        </>
      );
    case "selected":
      return <OptionTag tone="success">项目追加</OptionTag>;
    case "available":
      return <OptionTag tone="muted">可追加</OptionTag>;
  }
}

export function OptionTag({
  tone,
  children,
}: {
  tone: "muted" | "info" | "success" | "warning";
  children: ReactNode;
}) {
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 text-xs font-medium",
        toneClass(tone === "muted" ? "neutral" : tone),
      )}
    >
      {children}
    </span>
  );
}
