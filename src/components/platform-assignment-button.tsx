import type { Tool } from "@/bindings/commands";
import claudeIconUrl from "@/assets/brand/claude-icon-square.svg";
import codexIconUrl from "@/assets/brand/codex-icon-light.png";
import { Button } from "@/components/ui/button";
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
  const platform = tool === "claude" ? "Claude" : "Codex";
  const label = `${platform} 全局${assigned ? "已分配" : "未分配"}`;

  return (
    <Button
      type="button"
      size="sm"
      variant="outline"
      className={cn(
        "size-8 p-0 shadow-none",
        assigned
          ? "border-slate-300 bg-slate-50 shadow-sm"
          : "border-slate-200 bg-transparent",
      )}
      aria-label={label}
      aria-pressed={assigned}
      title={label}
      disabled={disabled}
      onClick={onClick}
    >
      <img
        src={tool === "claude" ? claudeIconUrl : codexIconUrl}
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
