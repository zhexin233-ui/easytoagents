import type { Tool } from "@/bindings/commands";
import { ToolIconToggle } from "@/components/tool-icon-toggle";
import { toolMetadata } from "@/lib/tool-metadata";

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
  return (
    <ToolIconToggle
      tool={tool}
      active={assigned}
      label={`${toolMetadata(tool).label} 全局${assigned ? "已分配" : "未分配"}`}
      disabled={disabled}
      onClick={onClick}
    />
  );
}
