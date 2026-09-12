import type { ReactNode } from "react";

import { BlockingState } from "@/components/blocking-state";
import { Button } from "@/components/ui/button";

export interface ProjectAssignmentsSectionProps<TItem> {
  title: string;
  description: string;
  blocked: string | null;
  directApply: boolean;
  error: string | null;
  pending: boolean;
  empty: boolean;
  excludeFromGit: boolean;
  onExcludeFromGit: (value: boolean) => void;
  previewPending: boolean;
  previewLabel: string;
  onPreview: () => void;
  items?: readonly TItem[];
  renderItem?: (item: TItem) => ReactNode;
  children?: ReactNode;
}

/**
 * 项目追加资源共用的状态壳。
 *
 * `items`/`renderItem` 让简单资源可以直接配置渲染；Hooks 等需要额外
 * 分组交互的资源则通过 `children` 注入其专属内容，仍共享阻断、排除和预览行为。
 */
export function ProjectAssignmentsSection<TItem = never>({
  title,
  description,
  blocked,
  directApply,
  error,
  pending,
  empty,
  excludeFromGit,
  onExcludeFromGit,
  previewPending,
  previewLabel,
  onPreview,
  items,
  renderItem,
  children,
}: ProjectAssignmentsSectionProps<TItem>) {
  const actionLabel = directApply
    ? `直接应用项目 ${title} 同步`
    : `预览项目 ${title} 同步`;

  return (
    <article className="bg-card rounded-lg border p-5">
      <h3 className="font-semibold">{title}</h3>
      <p className="text-muted-foreground mt-1 text-sm leading-6">
        {description}
      </p>
      {pending ? (
        <p role="status" className="mt-3 text-sm">
          正在读取选择项…
        </p>
      ) : null}
      {error ? (
        <div className="mt-3">
          <BlockingState title="选择器不可用" description={error} />
        </div>
      ) : null}
      {blocked ? (
        <div className="mt-3">
          <BlockingState title="项目目标受阻" description={blocked} />
        </div>
      ) : null}
      {empty ? (
        <p className="text-muted-foreground mt-3 text-sm">
          中央库暂无可追加项。
        </p>
      ) : null}
      <div className="mt-3 divide-y">
        {items?.map((item, index) =>
          renderItem ? (
            <span key={index} className="contents">
              {renderItem(item)}
            </span>
          ) : null,
        )}
        {children}
      </div>
      <label className="mt-4 flex items-start gap-2 text-sm">
        <input
          type="checkbox"
          checked={excludeFromGit}
          onChange={(event) => onExcludeFromGit(event.target.checked)}
        />
        <span>新建文件写入 .git/info/exclude（不提交到仓库）</span>
      </label>
      <Button
        className="mt-4"
        variant="outline"
        aria-label={previewLabel}
        disabled={Boolean(blocked) || previewPending}
        onClick={onPreview}
      >
        {previewPending
          ? directApply
            ? "正在应用…"
            : "正在生成…"
          : actionLabel}
      </Button>
    </article>
  );
}
