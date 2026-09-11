import { useRef } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";

import {
  commands,
  type PreviewPlan,
  type ProjectDto,
  type ProjectNativeResourceAction,
  type ProjectNativeResourceDto,
  type Tool,
} from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { Button } from "@/components/ui/button";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { projectNativeResourcesQueryOptions } from "@/lib/projects-api";

import type { ProjectResourceView } from "./resource-types";

export function ProjectNativeResources({
  project,
  tool,
  artifactKind,
  writerBlocked,
  applyPending,
  onPreview,
}: {
  project: ProjectDto;
  tool: Tool;
  artifactKind: ProjectResourceView;
  writerBlocked: boolean;
  applyPending: boolean;
  onPreview: (
    plan: PreviewPlan,
    tool: Tool,
    artifactKind: ProjectResourceView,
  ) => void;
}) {
  const nativeQuery = useQuery({
    ...projectNativeResourcesQueryOptions(project.id, tool, artifactKind),
  });
  const previewInFlight = useRef(false);
  const nativePreview = useMutation({
    mutationFn: async (resource: ProjectNativeResourceDto) => {
      const action: ProjectNativeResourceAction = resource.canDisable
        ? "disable"
        : "restore";
      return unwrapResult(
        await commands.previewProjectNativeResourceAction({
          resourceId: resource.id,
          rowVersion: resource.rowVersion,
          action,
        }),
      );
    },
  });

  const pending = writerBlocked || applyPending || nativePreview.isPending;
  const queryError = profileErrorText(nativeQuery.error);
  const previewError = profileErrorText(nativePreview.error);

  const runPreview = (resource: ProjectNativeResourceDto) => {
    if (pending || previewInFlight.current) return;
    if (!resource.canDisable && !resource.canRestore) return;
    previewInFlight.current = true;
    nativePreview.mutate(resource, {
      onSuccess: (plan) => {
        onPreview(plan, tool, artifactKind);
      },
      onSettled: () => {
        previewInFlight.current = false;
      },
    });
  };

  return (
    <section
      className="space-y-3"
      aria-labelledby="project-native-resources-title"
    >
      <div>
        <h2
          id="project-native-resources-title"
          className="text-xl font-semibold"
        >
          项目原生资源
        </h2>
        <p className="text-muted-foreground mt-1 text-sm">
          只读识别项目自带资源。临时禁用与恢复必须审阅持久化预览，即使已开启直接应用也不会自动写入。
        </p>
      </div>
      {nativeQuery.isPending ? (
        <p role="status" className="text-sm">
          正在读取项目原生资源…
        </p>
      ) : null}
      {queryError ? (
        <BlockingState
          title="无法读取项目原生资源"
          description={queryError}
          actionLabel="重试"
          onAction={() => void nativeQuery.refetch()}
        />
      ) : null}
      {previewError ? (
        <BlockingState
          title="无法生成原生资源预览"
          description={previewError}
        />
      ) : null}
      {nativeQuery.data?.length === 0 ? (
        <div className="rounded-lg border border-dashed p-4 text-sm">
          <p className="font-medium">当前组合没有项目原生资源</p>
          <p className="text-muted-foreground mt-1">
            登记与扫描不会改写这些文件。中央追加资源显示在下方。
          </p>
        </div>
      ) : null}
      <div className="space-y-3">
        {nativeQuery.data?.map((resource) => (
          <NativeResourceRow
            key={resource.id}
            resource={resource}
            pending={pending}
            onAction={() => runPreview(resource)}
          />
        ))}
      </div>
    </section>
  );
}

function NativeResourceRow({
  resource,
  pending,
  onAction,
}: {
  resource: ProjectNativeResourceDto;
  pending: boolean;
  onAction: () => void;
}) {
  const actionLabel = resource.canDisable
    ? `临时禁用 ${resource.displayName}`
    : resource.canRestore
      ? `恢复 ${resource.displayName}`
      : `${resource.displayName} 当前不可操作`;
  const actionText = resource.canDisable
    ? "临时禁用"
    : resource.canRestore
      ? "恢复"
      : "不可操作";

  return (
    <article className="rounded-lg border p-4 text-sm">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 space-y-2">
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            <span className="font-medium">{resource.displayName}</span>
            <OptionTag tone={nativeStateTone(resource.state)}>
              {nativeStateLabel(resource.state)}
            </OptionTag>
            <OptionTag tone="muted">
              {entryTypeLabel(resource.entryType)}
            </OptionTag>
          </div>
          <code className="text-muted-foreground block text-xs break-all">
            {resource.targetPath}
          </code>
          {resource.disabledAt ? (
            <p className="text-muted-foreground text-xs">
              禁用时间：{resource.disabledAt}
            </p>
          ) : null}
          {resource.diagnosticCodes.map((code) => (
            <p key={code} className="text-xs">
              诊断：{code}
            </p>
          ))}
          {resource.state === "missing" ? (
            <p className="text-muted-foreground text-xs">
              资源已被外部移除，且没有可恢复的禁用快照。
            </p>
          ) : null}
          {resource.state === "conflict" ? (
            <p className="text-muted-foreground text-xs">
              生效位置被重新占用或发生外部变化。恢复材料已保留，请先处理冲突。
            </p>
          ) : null}
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="shrink-0 shadow-none"
          aria-label={actionLabel}
          disabled={pending || (!resource.canDisable && !resource.canRestore)}
          onClick={onAction}
        >
          {actionText}
        </Button>
      </div>
    </article>
  );
}

function nativeStateLabel(state: ProjectNativeResourceDto["state"]) {
  switch (state) {
    case "active":
      return "项目原生 · 已启用";
    case "disabled":
      return "项目原生 · 已禁用";
    case "missing":
      return "项目原生 · 已缺失";
    case "conflict":
      return "项目原生 · 冲突";
  }
}

function nativeStateTone(
  state: ProjectNativeResourceDto["state"],
): "muted" | "info" | "success" | "warning" {
  switch (state) {
    case "active":
      return "success";
    case "disabled":
      return "info";
    case "missing":
      return "muted";
    case "conflict":
      return "warning";
  }
}

function entryTypeLabel(entryType: ProjectNativeResourceDto["entryType"]) {
  switch (entryType) {
    case "mcp_entry":
      return "MCP 条目";
    case "directory":
      return "技能目录";
    case "symlink":
      return "符号链接";
  }
}

function OptionTag({
  tone,
  children,
}: {
  tone: "muted" | "info" | "success" | "warning";
  children: React.ReactNode;
}) {
  const toneStyles = {
    muted: "border-slate-200 bg-slate-50 text-slate-700",
    info: "border-info/30 bg-info/10 text-info",
    success: "border-success/30 bg-success/10 text-success",
    warning: "border-warning/30 bg-warning/10 text-warning",
  } as const;

  return (
    <span
      className={`inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 text-xs font-medium ${toneStyles[tone]}`}
    >
      {children}
    </span>
  );
}
