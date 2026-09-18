import { useEffect, useRef } from "react";
import { useMutation } from "@tanstack/react-query";

import {
  type AppError,
  type ApplyResult,
  type ArtifactKind,
  type PreviewPlan,
  type Result,
  type SyncScopeDto,
  type Tool,
} from "@/bindings/commands";
import { useNotify } from "@/components/use-notify";
import {
  ProfileRpcError,
  profileErrorText,
  unwrapResult,
} from "@/lib/profile-api";

export interface SyncScopeFailure {
  scope: SyncScopeDto;
  error: unknown;
}

export interface SyncScopeRunResult {
  status: "success" | "partial_failure" | "failure";
  scopes: SyncScopeDto[];
  succeeded: SyncScopeDto[];
  failures: SyncScopeFailure[];
  appliedTargets: number;
  snapshotCount: number;
}

export interface SyncPreviewFlowOptions {
  artifactKind: ArtifactKind;
  /** `projectId` is the exact scope returned by the central mutation. */
  preview: (
    tool: Tool,
    projectId?: string | null,
  ) => Promise<Result<PreviewPlan, AppError>>;
  apply: (input: {
    previewId: string;
    tool: Tool;
    projectId?: string | null;
  }) => Promise<Result<ApplyResult, AppError>>;
  invalidate: () => Promise<void>;
  messages: {
    previewFailed: string;
    applyFailed: string;
    applied: (result: ApplyResult) => string;
    empty?: string;
    partialFailed?: (failures: SyncScopeFailure[]) => string;
    failed?: (failures: SyncScopeFailure[]) => string;
  };
}

interface ScopePreviewRequest {
  scope: SyncScopeDto;
}

interface ApplyRequest {
  previewId: string;
  scope: SyncScopeDto;
}

function isStalePreviewError(error: unknown): boolean {
  return (
    error instanceof ProfileRpcError && error.appError.code === "STALE_PREVIEW"
  );
}

function scopeKey(scope: SyncScopeDto): string {
  return `${scope.artifactKind}\u0000${scope.tool}\u0000${scope.projectId ?? ""}`;
}

const STABLE_ARTIFACT_ORDER: readonly ArtifactKind[] = [
  "provider",
  "prompt",
  "mcp",
  "skill",
  "hook",
  "agent",
];

const STABLE_TOOL_ORDER: readonly Tool[] = [
  "claude",
  "codex",
  "cursor",
  "zcode",
  "opencode",
  "pi",
];

function compareStableValues<T extends string>(
  left: T,
  right: T,
  order: readonly T[],
): number {
  const leftIndex = order.indexOf(left);
  const rightIndex = order.indexOf(right);
  if (leftIndex !== rightIndex) return leftIndex - rightIndex;
  return left === right ? 0 : left < right ? -1 : 1;
}

/** Scope 的唯一排序入口；不要在页面按 DTO 中的 globalTools 猜测项目范围。 */
export function stableSyncScopes(
  scopes: readonly SyncScopeDto[],
): SyncScopeDto[] {
  const unique = new Map<string, SyncScopeDto>();
  for (const scope of scopes) {
    unique.set(scopeKey(scope), scope);
  }
  return [...unique.values()].sort((left, right) => {
    const artifact = compareStableValues(
      left.artifactKind,
      right.artifactKind,
      STABLE_ARTIFACT_ORDER,
    );
    if (artifact !== 0) return artifact;
    const tool = compareStableValues(left.tool, right.tool, STABLE_TOOL_ORDER);
    if (tool !== 0) return tool;
    if (left.projectId === right.projectId) return 0;
    if (left.projectId === null) return -1;
    if (right.projectId === null) return 1;
    return left.projectId < right.projectId ? -1 : 1;
  });
}

function canApplyPreview(plan: PreviewPlan): boolean {
  return (
    plan.targets.length > 0 &&
    plan.targets.every(
      (target) => target.changeKind !== "conflict" && target.errorCode === null,
    )
  );
}

function scopeLabel(scope: SyncScopeDto): string {
  return `${scope.tool}${scope.projectId ? `/${scope.projectId}` : ""}`;
}

function isSyncScopeArray(
  input: readonly SyncScopeDto[] | SyncScopeDto | Tool,
): input is readonly SyncScopeDto[] {
  return Array.isArray(input);
}

/**
 * Persisted preview lifecycle for all central and project assignment flows.
 * Each request consumes the exact returned scope, serializes scopes, and
 * rebuilds a stale preview at most once. The queue promise is returned so the
 * originating mutation can await all native writes before reporting success.
 */
export function useSyncPreviewFlow(options: SyncPreviewFlowOptions) {
  const { notify } = useNotify();
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const previewMutation = useMutation({
    mutationFn: async ({ scope }: ScopePreviewRequest) =>
      unwrapResult(await options.preview(scope.tool, scope.projectId)),
  });

  const applyMutation = useMutation({
    mutationFn: async ({ previewId, scope }: ApplyRequest) =>
      unwrapResult(
        await options.apply({
          previewId,
          tool: scope.tool,
          projectId: scope.projectId,
        }),
      ),
  });

  const submitPersistedPreview = async (
    plan: PreviewPlan,
    scopeOrTool: SyncScopeDto | Tool,
    staleRetry = 0,
  ): Promise<ApplyResult | null> => {
    const scope: SyncScopeDto =
      typeof scopeOrTool === "string"
        ? {
            artifactKind: options.artifactKind,
            tool: scopeOrTool,
            projectId: null,
          }
        : scopeOrTool;
    if (plan.targets.length === 0) return null;
    if (!canApplyPreview(plan)) {
      throw new Error(options.messages.applyFailed);
    }
    try {
      return await applyMutation.mutateAsync({
        previewId: plan.previewId,
        scope,
      });
    } catch (error) {
      if (staleRetry === 0 && isStalePreviewError(error)) {
        const freshPlan = unwrapResult(
          await options.preview(scope.tool, scope.projectId),
        );
        return submitPersistedPreview(freshPlan, scope, 1);
      }
      throw error;
    }
  };

  const executeScope = async (scope: SyncScopeDto) => {
    const plan = await previewMutation.mutateAsync({ scope });
    const result = await submitPersistedPreview(plan, scope);
    return { plan, result };
  };

  const previewQueueRef = useRef(
    Promise.resolve<SyncScopeRunResult>({
      status: "success",
      scopes: [],
      succeeded: [],
      failures: [],
      appliedTargets: 0,
      snapshotCount: 0,
    }),
  );

  const requestPreview = (
    input: readonly SyncScopeDto[] | SyncScopeDto | Tool,
  ): Promise<SyncScopeRunResult> => {
    const requested: SyncScopeDto[] =
      typeof input === "string"
        ? [
            {
              artifactKind: options.artifactKind,
              tool: input,
              projectId: null,
            },
          ]
        : isSyncScopeArray(input)
          ? [...input]
          : [input];
    const scopes = stableSyncScopes(
      requested.filter((scope) => scope.artifactKind === options.artifactKind),
    );
    const queued = previewQueueRef.current
      .catch(() => undefined)
      .then(async () => {
        const succeeded: SyncScopeDto[] = [];
        const failures: SyncScopeFailure[] = [];
        let appliedTargets = 0;
        let snapshotCount = 0;

        for (const scope of scopes) {
          try {
            const { result } = await executeScope(scope);
            succeeded.push(scope);
            if (result) {
              appliedTargets += result.appliedTargets;
              snapshotCount += result.snapshotCount;
            }
          } catch (error) {
            failures.push({ scope, error });
          }
        }

        if (scopes.length > 0) {
          await options.invalidate();
        }

        const status =
          failures.length === 0
            ? "success"
            : succeeded.length === 0
              ? "failure"
              : "partial_failure";
        const result: SyncScopeRunResult = {
          status,
          scopes,
          succeeded,
          failures,
          appliedTargets,
          snapshotCount,
        };

        if (!mountedRef.current || scopes.length === 0) return result;
        if (failures.length > 0) {
          const failureSummary = failures
            .map(({ scope, error }) => {
              const detail = profileErrorText(error);
              return `${scopeLabel(scope)}：${detail ?? options.messages.previewFailed}`;
            })
            .join("；");
          const message =
            status === "partial_failure"
              ? (options.messages.partialFailed?.(failures) ??
                `部分同步失败：${failureSummary}`)
              : (options.messages.failed?.(failures) ??
                `同步失败：${failureSummary}`);
          notify({ kind: "error", message });
        } else if (appliedTargets > 0) {
          notify({
            kind: "success",
            message: options.messages.applied({
              runId: "",
              status: "succeeded",
              appliedTargets,
              snapshotCount,
            }),
          });
        } else if (options.messages.empty) {
          notify({ kind: "success", message: options.messages.empty });
        }
        return result;
      });
    previewQueueRef.current = queued;
    return queued;
  };

  return {
    requestPreview,
    previewMutation,
    applyMutation,
    submitPersistedPreview,
  };
}
