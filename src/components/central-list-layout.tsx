import { LayoutGrid, List } from "lucide-react";
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

/** macOS segmented control：仅图标的布局切换，选中态共享外框高亮。 */
export function CentralListLayoutToggle({
  value,
  onChange,
}: CentralListLayoutToggleProps) {
  return (
    <div
      className="rounded-control flex items-center overflow-hidden border p-0.5"
      role="group"
      aria-label="中央列表显示方式"
    >
      <Button
        type="button"
        size="icon"
        variant="ghost"
        className={cn(
          "text-muted-foreground hover:text-foreground",
          value === "list" && "bg-muted text-foreground hover:bg-muted",
        )}
        aria-label="单列显示"
        aria-pressed={value === "list"}
        title="单列显示"
        onClick={() => onChange("list")}
      >
        <List aria-hidden="true" className="size-4" />
      </Button>
      <Button
        type="button"
        size="icon"
        variant="ghost"
        className={cn(
          "text-muted-foreground hover:text-foreground",
          value === "grid" && "bg-muted text-foreground hover:bg-muted",
        )}
        aria-label="三列网格显示"
        aria-pressed={value === "grid"}
        title="三列网格显示"
        onClick={() => onChange("grid")}
      >
        <LayoutGrid aria-hidden="true" className="size-4" />
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
        "bg-card min-w-0 rounded-lg border",
        layout === "grid"
          ? "flex h-full flex-col overflow-hidden"
          : "px-4 py-3",
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
