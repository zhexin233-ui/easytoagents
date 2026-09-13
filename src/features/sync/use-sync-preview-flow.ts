import { useEffect, useRef, useState } from "react";
import { useMutation } from "@tanstack/react-query";

import {
  type AppError,
  type ApplyResult,
  type ArtifactKind,
  type PreviewPlan,
  type Result,
  type Tool,
} from "@/bindings/commands";
import { useNotify } from "@/components/use-notify";
import { canAutoApplyPreview } from "@/lib/settings-api";
import {
  ProfileRpcError,
  profileErrorText,
  unwrapResult,
} from "@/lib/profile-api";

export interface OpenSyncPreview {
  plan: PreviewPlan;
  tool: Tool;
}

export interface SyncPreviewFlowOptions<TReadopt = never> {
  artifactKind: ArtifactKind;
  preview: (tool: Tool) => Promise<Result<PreviewPlan, AppError>>;
  apply: (input: {
    previewId: string;
    tool: Tool;
  }) => Promise<Result<ApplyResult, AppError>>;
  readopt?: (
    tool: Tool,
    targetPath?: string,
  ) => Promise<Result<TReadopt, AppError>>;
  invalidate: () => Promise<void>;
  messages: {
    previewFailed: string;
    applyFailed: string;
    applied: (result: ApplyResult) => string;
    empty?: string;
    readoptFailed?: string;
  };
  directApply: boolean;
  onReadopted?: (result: TReadopt, tool: Tool) => Promise<void> | void;
}

interface PreviewRequest {
  tool: Tool;
  autoApply: boolean;
  staleRetry?: number;
}

interface ApplyRequest {
  previewId: string;
  tool: Tool;
  retryOnStale?: boolean;
}

type ReadoptRequest =
  | {
      tool: Tool;
      targetPath?: string;
    }
  // 保留旧的单目标调用形式；Agents 需要额外传入 targetPath。
  | Tool;

function isStalePreviewError(error: unknown): boolean {
  return (
    error instanceof ProfileRpcError && error.appError.code === "STALE_PREVIEW"
  );
}

/**
 * Owns the persisted preview lifecycle shared by global and project resource
 * pages. A page supplies only typed RPC callbacks and invalidation; this hook
 * never turns CRUD success into an implicit native write.
 */
export function useSyncPreviewFlow<TReadopt = never>(
  options: SyncPreviewFlowOptions<TReadopt>,
) {
  const { notify } = useNotify();
  const [openPreview, setOpenPreview] = useState<OpenSyncPreview | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const closePreview = () => setOpenPreview(null);
  const openPersistedPreview = (plan: PreviewPlan, tool: Tool) =>
    setOpenPreview({ plan, tool });

  const applyMutation = useMutation({
    mutationFn: async ({ previewId, tool }: ApplyRequest) =>
      unwrapResult(await options.apply({ previewId, tool })),
    onSuccess: async (result) => {
      if (!mountedRef.current) return;
      closePreview();
      await options.invalidate();
      if (!mountedRef.current) return;
      notify({ kind: "success", message: options.messages.applied(result) });
    },
    onError: (error, variables) => {
      // Direct mode retries one stale preview with a fresh persisted plan;
      // suppress the intermediate error and report only a failed retry.
      if (variables.retryOnStale && isStalePreviewError(error)) {
        return;
      }
      if (!mountedRef.current) return;
      notify({
        kind: "error",
        message: profileErrorText(error) ?? options.messages.applyFailed,
      });
    },
  });

  const submitPersistedPreview = async (
    plan: PreviewPlan,
    tool: Tool,
    autoApply: boolean,
    staleRetry = 0,
  ) => {
    if (plan.targets.length === 0) {
      closePreview();
      if (options.messages.empty) {
        notify({ kind: "success", message: options.messages.empty });
      }
      return;
    }
    if (options.directApply && autoApply && canAutoApplyPreview(plan)) {
      // Keep the preview → Apply chain alive until the native write has
      // completed. This matters when a central-list mutation invalidates and
      // rerenders the page while direct mode is applying a deletion preview.
      try {
        await applyMutation.mutateAsync({
          previewId: plan.previewId,
          tool,
          retryOnStale: staleRetry === 0,
        });
      } catch (error) {
        if (
          staleRetry === 0 &&
          isStalePreviewError(error) &&
          mountedRef.current
        ) {
          // A query invalidation or environment refresh can make the persisted
          // plan stale between Preview and Apply. Rebuild once in the same
          // mutation chain so two central-list changes cannot leave cleanup
          // work stranded in an old preview.
          await previewMutation
            .mutateAsync({ tool, autoApply, staleRetry: 1 })
            .catch(() => undefined);
        }
      }
      return;
    }
    setOpenPreview({ plan, tool });
  };

  const previewMutation = useMutation({
    mutationFn: async ({ tool }: PreviewRequest) => ({
      tool,
      plan: unwrapResult(await options.preview(tool)),
    }),
    onSuccess: async ({ plan, tool }, { autoApply, staleRetry = 0 }) => {
      if (!mountedRef.current) return;
      await submitPersistedPreview(plan, tool, autoApply, staleRetry);
    },
    onError: (error) => {
      if (!mountedRef.current) return;
      notify({
        kind: "error",
        message: profileErrorText(error) ?? options.messages.previewFailed,
      });
    },
  });

  const readoptMutation = useMutation({
    mutationFn: async (request: ReadoptRequest) => {
      if (!options.readopt) {
        throw new Error("当前预览不支持重新接管。");
      }
      const { tool, targetPath } =
        typeof request === "string" ? { tool: request } : request;
      return unwrapResult(await options.readopt(tool, targetPath));
    },
    onSuccess: async (result, request) => {
      if (!mountedRef.current) return;
      closePreview();
      await options.invalidate();
      if (!mountedRef.current) return;
      const tool = typeof request === "string" ? request : request.tool;
      await options.onReadopted?.(result, tool);
    },
    onError: (error) => {
      if (!mountedRef.current) return;
      notify({
        kind: "error",
        message:
          profileErrorText(error) ??
          options.messages.readoptFailed ??
          "重新接管目标失败。",
      });
    },
  });

  const previewQueueRef = useRef(Promise.resolve());
  const requestPreview = (tool: Tool, autoApply: boolean): void => {
    // Serialize preview → Apply chains. TanStack mutations can run multiple
    // calls concurrently, but each persisted preview claims the same mutable
    // database state; concurrent calls otherwise leave an older plan behind.
    const queued = previewQueueRef.current
      .catch(() => undefined)
      .then(() =>
        previewMutation.mutateAsync({ tool, autoApply }).then(() => undefined),
      )
      .catch(() => undefined);
    previewQueueRef.current = queued;
    void queued;
  };

  return {
    openPreview,
    requestPreview,
    previewMutation,
    applyMutation,
    readoptMutation,
    closePreview,
    openPersistedPreview,
    submitPersistedPreview,
  };
}
