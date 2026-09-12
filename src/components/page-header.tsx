import { ChevronLeft } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router-dom";

import { Button } from "@/components/ui/button";

interface PageHeaderProps {
  /** 页面主标题，渲染为 h1。 */
  title: ReactNode;
  /** 标题下一行元信息（项目路径、Git/Trust chip 等）。 */
  meta?: ReactNode;
  /** 右侧操作区：一颗主操作 + 次要操作/布局切换。 */
  actions?: ReactNode;
  /** 头部下方的 tab / 过滤条。 */
  children?: ReactNode;
  /** 可选返回按钮的跳转目标（如项目详情返回 /projects）。 */
  backTo?: string;
}

/** macOS 工具栏式页面头部：标题 + 元信息 + 右侧操作，无描述 slot，
 * 页面机制说明不允许从这里回流。头部内容与主体共用 `max-w-6xl` 列宽，
 * 窄窗口下操作区自动换行，不挤压标题。 */
export function PageHeader({
  title,
  meta,
  actions,
  children,
  backTo,
}: PageHeaderProps) {
  return (
    <header className="bg-background/85 sticky top-0 z-10 border-b backdrop-blur">
      <div className="max-w-6xl px-8 py-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2">
            {backTo ? (
              <Button
                asChild
                variant="ghost"
                size="icon"
                aria-label="返回"
                title="返回"
              >
                <Link to={backTo}>
                  <ChevronLeft aria-hidden="true" className="size-4" />
                </Link>
              </Button>
            ) : null}
            <div className="min-w-0">
              <h1 className="truncate text-[22px] font-semibold tracking-tight">
                {title}
              </h1>
              {meta}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        </div>
        {children}
      </div>
    </header>
  );
}
