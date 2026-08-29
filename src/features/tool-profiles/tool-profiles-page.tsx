import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";

import type { ArtifactKind, PreviewPlan, Tool } from "@/bindings/commands";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import { ProviderPanel } from "@/features/tool-profiles/provider-panel";
import { PromptPanel } from "@/features/tool-profiles/prompt-panel";
import {
  profileErrorText,
  toolProfileStatusQueryOptions,
  unwrapResult,
} from "@/lib/profile-api";
import { commands } from "@/bindings/commands";

interface ToolProfilesPageProps {
  tool: Tool;
}

interface OpenPreview {
  plan: PreviewPlan;
  artifactKind: ArtifactKind;
}

export function ToolProfilesPage({ tool }: ToolProfilesPageProps) {
  const statusQuery = useQuery(toolProfileStatusQueryOptions(tool));
  const [openPreview, setOpenPreview] = useState<OpenPreview | null>(null);
  const [applyMessage, setApplyMessage] = useState<string | null>(null);
  const applyMutation = useMutation({
    mutationFn: async (preview: OpenPreview) =>
      unwrapResult(
        await commands.applyProfilePreview({
          previewId: preview.plan.previewId,
          tool,
          artifactKind: preview.artifactKind,
        }),
      ),
    onSuccess: (result) => {
      setApplyMessage(`已应用 ${result.appliedTargets} 个目标，可从快照恢复。`);
      setOpenPreview(null);
    },
  });

  const title = tool === "claude" ? "Claude" : "Codex";
  const applyError = profileErrorText(applyMutation.error);

  return (
    <main className="p-6 lg:p-8">
      <header className="mx-auto max-w-6xl">
        <p className="text-muted-foreground text-sm">工具配置</p>
        <h1 className="mt-1 text-2xl font-semibold">{title}</h1>
        <p className="text-muted-foreground mt-2 max-w-3xl text-sm leading-6">
          中央档案的 CRUD
          不会直接改写原生配置；切换后先生成持久化预览，再由你确认 Apply。
        </p>
      </header>

      <div className="mx-auto mt-6 max-w-6xl space-y-4" aria-live="polite">
        {statusQuery.data ? (
          <section className="rounded-lg border bg-white p-4 text-sm">
            {statusQuery.data.availability === "installed" ? (
              <p className="font-medium text-emerald-800">
                已安全检测到 {title}
                {statusQuery.data.installationVersion
                  ? ` ${statusQuery.data.installationVersion}`
                  : ""}
              </p>
            ) : null}
            {statusQuery.data.availability === "unavailable" ? (
              <p className="font-medium text-red-700">
                未在发布进程的安全搜索路径中检测到 {title}
                ；原生目标保持不可应用。
              </p>
            ) : null}
            {statusQuery.data.availability === "unsupported" ? (
              <p className="font-medium text-amber-800">
                {title}
                安装探针未能安全确认版本；可能是输出异常、超时或不可执行，原生目标保持不可应用。
              </p>
            ) : null}
            <p>{statusQuery.data.newSessionNotice}</p>
            {statusQuery.data.bearerTokenWarning ? (
              <p className="mt-2 text-amber-800">
                {statusQuery.data.bearerTokenWarning}
              </p>
            ) : null}
            {statusQuery.data.promptOverride === "present" ? (
              <p className="mt-2 font-medium text-amber-800">
                检测到更高优先级的 Codex 指令来源（如 AGENTS.override.md）；当前
                AGENTS.md 可能被遮蔽。
              </p>
            ) : null}
            {statusQuery.data.promptOverride === "unknown" ? (
              <p className="mt-2 font-medium text-amber-800">
                无法安全确认 Codex 指令遮蔽状态，请检查 AGENTS.override.md
                后再应用。
              </p>
            ) : null}
            {statusQuery.data.providerPolicy === "blocked" ? (
              <p className="mt-2 font-medium text-red-700">
                Claude Provider 由宿主平台管理，本应用不会覆盖渠道配置。
              </p>
            ) : null}
            {statusQuery.data.providerPolicy === "unknown" ? (
              <p className="mt-2 font-medium text-amber-800">
                无法确认 Claude Provider
                是否由宿主管理；渠道预览将保持阻止状态。
              </p>
            ) : null}
          </section>
        ) : null}
        {statusQuery.isPending ? (
          <p role="status" className="rounded-lg border bg-white p-4 text-sm">
            正在检测工具配置状态…
          </p>
        ) : null}
        {statusQuery.isError ? (
          <p
            role="alert"
            className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm"
          >
            {profileErrorText(statusQuery.error)}
          </p>
        ) : null}
        {applyMessage ? (
          <p className="rounded-lg border border-emerald-200 bg-emerald-50 p-4 text-sm">
            {applyMessage}
          </p>
        ) : null}
        {applyError ? (
          <p
            role="alert"
            className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm"
          >
            {applyError}
          </p>
        ) : null}
      </div>

      <div className="mx-auto mt-6 grid max-w-6xl gap-6 xl:grid-cols-2">
        <ProviderPanel
          tool={tool}
          onPreview={(plan) =>
            setOpenPreview({ plan, artifactKind: "provider" })
          }
        />
        <PromptPanel
          tool={tool}
          onPreview={(plan) => setOpenPreview({ plan, artifactKind: "prompt" })}
        />
      </div>

      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={tool}
        artifactKind={openPreview?.artifactKind ?? "provider"}
        applying={applyMutation.isPending}
        onClose={() => setOpenPreview(null)}
        onApply={() => {
          if (openPreview) {
            applyMutation.mutate(openPreview);
          }
        }}
      />
    </main>
  );
}
