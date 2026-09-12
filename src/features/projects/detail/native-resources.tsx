import { useMutation, useQuery } from "@tanstack/react-query";

import {
  commands,
  type PreviewPlan,
  type ProjectDto,
  type ProjectNativeResourceAction,
  type ProjectNativeResourceDto,
  type Tool,
} from "@/bindings/commands";
import { FolderX } from "lucide-react";

import { BlockingState } from "@/components/blocking-state";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/empty-state";
import { useSubmitGuard } from "@/hooks/use-submit-guard";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { projectNativeResourcesQueryOptions } from "@/lib/projects-api";
import type { ProjectResourceKind } from "@/lib/projects-api";

import type { ProjectResourceView } from "./resource-types";
import { OptionTag } from "./option-row";

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
  artifactKind: ProjectResourceKind;
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
  const previewGuard = useSubmitGuard();
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
    if (pending) return;
    if (!resource.canDisable && !resource.canRestore) return;
    if (!previewGuard.begin()) return;
    nativePreview.mutate(resource, {
      onSuccess: (plan) => {
        onPreview(plan, tool, artifactKind);
      },
      onSettled: () => {
        previewGuard.end();
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
          className="text-[15px] font-semibold"
        >
          项目原生资源
        </h2>
        <p className="text-muted-foreground mt-1 text-xs">
          禁用与恢复始终需要确认预览。
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
        <EmptyState
          icon={FolderX}
          title="没有项目原生资源"
          description="中央追加资源显示在下方。"
        />
      ) : null}
      <div className="divide-y">
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
  const isHook = resource.entryType === "hook_entry";
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
    <article className="hover:bg-muted/50 px-1 py-2.5 text-sm">
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
          {isHook ? <HookSummary resource={resource} /> : null}
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
        {isHook ? null : (
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="shrink-0"
            aria-label={actionLabel}
            disabled={pending || (!resource.canDisable && !resource.canRestore)}
            onClick={onAction}
          >
            {actionText}
          </Button>
        )}
      </div>
    </article>
  );
}

/// Hook 条目展示：原生事件、matcher、命令（可识别凭据只提示已脱敏）与超时。
/// Hook 是匿名数组条目，无法按条目定位改写，因此不提供禁用/恢复。
function HookSummary({ resource }: { resource: ProjectNativeResourceDto }) {
  const fields = hookSummaryFields(resource.safeSummary);
  const command = typeof fields.command === "string" ? fields.command : null;
  const matcher = typeof fields.matcher === "string" ? fields.matcher : null;
  const timeout = typeof fields.timeout === "number" ? fields.timeout : null;
  const redacted = fields.commandRedacted === true;

  return (
    <div className="space-y-1">
      {matcher ? (
        <p className="text-xs">
          matcher：<code className="break-all">{matcher}</code>
        </p>
      ) : null}
      {command ? (
        <p className="text-xs">
          命令：<code className="break-all">{command}</code>
        </p>
      ) : redacted ? (
        <p className="text-muted-foreground text-xs">
          命令包含可识别凭据，已脱敏。
        </p>
      ) : null}
      {timeout !== null ? <p className="text-xs">超时：{timeout} 秒</p> : null}
      <p className="text-muted-foreground text-xs">
        Hooks 暂不支持临时禁用与恢复。
      </p>
    </div>
  );
}

/// `safeSummary` 是宽松 JSON；只有对象形态的 Hook 摘要才提供字段。
function hookSummaryFields(summary: ProjectNativeResourceDto["safeSummary"]) {
  return typeof summary === "object" &&
    summary !== null &&
    !Array.isArray(summary)
    ? summary
    : {};
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
    case "hook_entry":
      return "Hook 条目";
  }
}
