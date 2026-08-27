import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export type CentralListLayout = "list" | "grid";

interface CentralListLayoutToggleProps {
  value: CentralListLayout;
  onChange: (value: CentralListLayout) => void;
}

interface CentralListProps {
  layout: CentralListLayout;
  children: ReactNode;
}

interface CentralListCardProps {
  layout: CentralListLayout;
  children: ReactNode;
}

interface CentralListCardSectionProps {
  layout: CentralListLayout;
  children: ReactNode;
  className?: string;
}

interface CentralListCardFooterProps extends CentralListCardSectionProps {
  label: string;
}

export function CentralListLayoutToggle({
  value,
  onChange,
}: CentralListLayoutToggleProps) {
  return (
    <div
      className="flex items-center gap-1 rounded-lg border p-1"
      role="group"
      aria-label="中央列表显示方式"
    >
      <Button
        type="button"
        size="sm"
        variant={value === "list" ? "default" : "outline"}
        className="h-7 gap-1.5 px-2"
        aria-label="单列显示"
        aria-pressed={value === "list"}
        title="单列显示"
        onClick={() => onChange("list")}
      >
        <ListIcon />
        单列
      </Button>
      <Button
        type="button"
        size="sm"
        variant={value === "grid" ? "default" : "outline"}
        className="h-7 gap-1.5 px-2"
        aria-label="三列网格显示"
        aria-pressed={value === "grid"}
        title="三列网格显示"
        onClick={() => onChange("grid")}
      >
        <GridIcon />
        三列
      </Button>
    </div>
  );
}

export function CentralList({ layout, children }: CentralListProps) {
  return (
    <div
      data-layout={layout}
      data-slot="central-list"
      className={cn(
        "mt-4 [&>*]:min-w-0",
        layout === "grid"
          ? "grid auto-rows-fr items-stretch gap-3 md:grid-cols-2 lg:grid-cols-3"
          : "space-y-3",
      )}
    >
      {children}
    </div>
  );
}

export function CentralListCard({ layout, children }: CentralListCardProps) {
  return (
    <article
      data-layout={layout}
      data-slot="central-list-card"
      className={cn(
        "min-w-0 rounded-lg border",
        layout === "grid" ? "flex h-full flex-col overflow-hidden" : "p-4",
      )}
    >
      {children}
    </article>
  );
}

export function CentralListCardBody({
  layout,
  children,
  className,
}: CentralListCardSectionProps) {
  return (
    <div
      data-slot="central-list-card-body"
      className={cn(
        "min-w-0",
        layout === "grid" && "flex flex-1 flex-col p-4",
        className,
      )}
    >
      {children}
    </div>
  );
}

export function CentralListCardFooter({
  layout,
  label,
  children,
  className,
}: CentralListCardFooterProps) {
  return (
    <footer
      data-slot="central-list-card-actions"
      aria-label={label}
      className={cn(
        "min-w-0",
        layout === "grid" ? "bg-muted/20 mt-auto border-t px-4 py-3" : "mt-3",
        className,
      )}
    >
      {children}
    </footer>
  );
}

function ListIcon() {
  return (
    <svg
      aria-hidden="true"
      className="size-3.5"
      viewBox="0 0 16 16"
      fill="currentColor"
    >
      <rect x="1" y="2" width="14" height="3" rx="1" />
      <rect x="1" y="6.5" width="14" height="3" rx="1" />
      <rect x="1" y="11" width="14" height="3" rx="1" />
    </svg>
  );
}

function GridIcon() {
  return (
    <svg
      aria-hidden="true"
      className="size-3.5"
      viewBox="0 0 16 16"
      fill="currentColor"
    >
      <rect x="1" y="2" width="4" height="12" rx="1" />
      <rect x="6" y="2" width="4" height="12" rx="1" />
      <rect x="11" y="2" width="4" height="12" rx="1" />
    </svg>
  );
}
