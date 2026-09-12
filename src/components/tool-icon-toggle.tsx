import type { Tool } from "@/bindings/commands";
import { Button } from "@/components/ui/button";
import { toolMetadata } from "@/lib/tool-metadata";
import { cn } from "@/lib/utils";

interface ToolIconToggleProps {
  tool: Tool;
  active: boolean;
  label: string;
  disabled?: boolean;
  onClick: () => void;
}

/** Brand-icon toggle used by global assignment and project tool views. */
export function ToolIconToggle({
  tool,
  active,
  label,
  disabled = false,
  onClick,
}: ToolIconToggleProps) {
  const metadata = toolMetadata(tool);
  return (
    <Button
      type="button"
      size="sm"
      variant="outline"
      className={cn(
        "size-8 p-0 shadow-none",
        active ? "border-border bg-muted" : "border-border bg-transparent",
      )}
      aria-label={label}
      aria-pressed={active}
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
          active ? "opacity-100" : "opacity-25 grayscale",
        )}
      />
    </Button>
  );
}
