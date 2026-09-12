import { Inbox } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

interface EmptyStateProps {
  icon?: LucideIcon;
  title: string;
  description?: string;
  action?: ReactNode;
}

/** 统一空状态：图标 + 标题 + 一句说明 + 一个明确的下一步操作。 */
export function EmptyState({
  icon: Icon = Inbox,
  title,
  description,
  action,
}: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center gap-1.5 py-10 text-center">
      <Icon aria-hidden="true" className="text-muted-foreground size-8" />
      <p className="text-[13px] font-medium">{title}</p>
      {description ? (
        <p className="text-muted-foreground">{description}</p>
      ) : null}
      {action ? <div className="mt-2">{action}</div> : null}
    </div>
  );
}
