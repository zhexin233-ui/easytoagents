import type { Tool } from "@/bindings/commands";
import { Button } from "@/components/ui/button";
import { toolMetadata } from "@/lib/tool-metadata";
import { cn } from "@/lib/utils";

interface PlatformAssignmentButtonProps {
  tool: Tool;
  assigned: boolean;
  disabled?: boolean;
  onClick: () => void;
}

export function PlatformAssignmentButton({
  tool,
  assigned,
  disabled = false,
  onClick,
}: PlatformAssignmentButtonProps) {
  const metadata = toolMetadata(tool);
  const label = `${metadata.label} 全局${assigned ? "已分配" : "未分配"}`;

  return (
    <Button
      type="button"
      size="sm"
      variant="outline"
      className={cn(
        "size-8 p-0 shadow-none",
        assigned
          ? "border-slate-300 bg-slate-50 shadow-sm dark:border-slate-600 dark:bg-slate-800"
          : "border-slate-200 bg-transparent dark:border-slate-700",
      )}
      aria-label={label}
      aria-pressed={assigned}
      title={label}
      disabled={disabled}
      onClick={onClick}
    >
      <img
        src={metadata.icon}
        alt=""
        aria-hidden="true"
        draggable={false}
        className={cn(
          "size-5 object-contain transition-[opacity,filter]",
          assigned ? "opacity-100" : "opacity-25 grayscale",
        )}
      />
    </Button>
  );
}
