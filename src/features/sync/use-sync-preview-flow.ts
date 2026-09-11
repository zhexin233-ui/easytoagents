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
import { profileErrorText, unwrapResult } from "@/lib/profile-api";

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
  readopt?: (tool: Tool) => Promise<Result<TReadopt, AppError>>;
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
}

interface ApplyRequest {
  previewId: string;
  tool: Tool;
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
    onError: (error) => {
      if (!mountedRef.current) return;
      notify({
        kind: "error",
        message: profileErrorText(error) ?? options.messages.applyFailed,
      });
    },
  });

  const submitPersistedPreview = (
    plan: PreviewPlan,
    tool: Tool,
    autoApply: boolean,
  ) => {
    if (plan.targets.length === 0) {
      closePreview();
      if (options.messages.empty) {
        notify({ kind: "success", message: options.messages.empty });
      }
      return;
    }
    if (options.directApply && autoApply && canAutoApplyPreview(plan)) {
      applyMutation.mutate({ previewId: plan.previewId, tool });
      return;
    }
    setOpenPreview({ plan, tool });
  };

  const previewMutation = useMutation({
    mutationFn: async ({ tool }: PreviewRequest) => ({
      tool,
      plan: unwrapResult(await options.preview(tool)),
    }),
    onSuccess: ({ plan, tool }, { autoApply }) => {
      if (!mountedRef.current) return;
      submitPersistedPreview(plan, tool, autoApply);
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
    mutationFn: async (tool: Tool) => {
      if (!options.readopt) {
        throw new Error("当前预览不支持重新接管。");
      }
      return unwrapResult(await options.readopt(tool));
    },
    onSuccess: async (result, tool) => {
      if (!mountedRef.current) return;
      closePreview();
      await options.invalidate();
      if (!mountedRef.current) return;
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

  const requestPreview = (tool: Tool, autoApply: boolean) => {
    previewMutation.mutate({ tool, autoApply });
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
