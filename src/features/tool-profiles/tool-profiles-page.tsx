import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";

import { commands, type SyncScopeDto, type Tool } from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { PageHeader } from "@/components/page-header";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { ExternalChangeActions } from "@/features/sync/external-change-actions";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { ProviderPanel } from "@/features/tool-profiles/provider-panel";
import {
  profileErrorText,
  globalProfileTargetStatusesQueryOptions,
  profileKeys,
  toolProfileStatusQueryOptions,
} from "@/lib/profile-api";
import { globalTargetStatusPresentation } from "@/lib/global-target-status-ui";
import { toneClass } from "@/lib/tone-class";
import { toolMetadata } from "@/lib/tool-metadata";

interface ToolProfilesPageProps {
  tool: Tool;
}

// 安装探针诊断码 → 用户可读原因。未知码只展示原始码，不猜测语义。
const INSTALLATION_PROBE_DIAGNOSTIC_TEXT: Record<string, string> = {
  INSTALLATION_PROBE_SKIPPED_PATH_ENTRIES:
    "PATH 中存在被跳过的不安全条目（相对路径、`.` 或同名目录），这些位置没有被搜索；如工具安装在那里，请改用绝对路径。",
  INSTALLATION_PROBE_NO_SAFE_PATH_ENTRIES:
    "PATH 为空或没有任何安全的绝对路径条目，探针无处可搜。",
  INSTALLATION_PROBE_UNSAFE_CANDIDATE:
    "PATH 中首个同名文件无法安全解析、不是普通文件或不可执行。",
};

function installationProbeDiagnosticText(code: string): string {
  return INSTALLATION_PROBE_DIAGNOSTIC_TEXT[code] ?? "安装探针报告了额外诊断。";
}

export function ToolProfilesPage({ tool }: ToolProfilesPageProps) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const metadata = toolMetadata(tool);
  const title = metadata.label;
  const statusQuery = useQuery(toolProfileStatusQueryOptions(tool));
  const targetStatusesQuery = useQuery(
    globalProfileTargetStatusesQueryOptions(tool),
  );
  const { requestPreview } = useSyncPreviewFlow({
    artifactKind: "provider",
    preview: (previewTool) => commands.previewProviderSync(previewTool),
    apply: ({ previewId, tool: previewTool }) =>
      commands.applyProfilePreview({
        previewId,
        tool: previewTool,
        artifactKind: "provider",
      }),
    invalidate: async () => {
      await queryClient.invalidateQueries({ queryKey: profileKeys.all });
    },
    messages: {
      previewFailed: "生成渠道预览失败。",
      applyFailed: "应用渠道预览失败。",
      applied: (result) =>
        `已应用 ${result.appliedTargets} 个目标，可从快照恢复。`,
    },
  });

  // Provider 与提示词均不支持的工具整页 fail closed；Cursor 这类「仅 Provider
  // 不支持」的工具仍进入正常布局（状态区 + 全局提示词入口），但不渲染 Provider 面板。
  if (!metadata.capabilities.provider && !metadata.capabilities.promptGlobal) {
    return (
      <>
        <PageHeader title={title} />
        <main className="px-8 py-6">
          <BlockingState
            title={`${title} 渠道不受支持`}
            description={`${title} 不支持渠道配置。`}
            code="CURSOR_PROVIDER_UNSUPPORTED"
          />
        </main>
      </>
    );
  }

  return (
    <>
      <PageHeader title={title} />
      <main className="space-y-6 px-8 py-6">
        <div className="space-y-4" aria-live="polite">
          {statusQuery.data ? (
            <section className="bg-card rounded-lg border p-4 text-sm">
              {statusQuery.data.availability === "installed" ? (
                <p className="text-success font-medium">
                  已检测到 {title}
                  {statusQuery.data.installationVersion
                    ? ` ${statusQuery.data.installationVersion}`
                    : ""}
                </p>
              ) : null}
              {statusQuery.data.availability === "unavailable" ? (
                <p className="text-destructive font-medium">
                  未检测到 {title}。
                </p>
              ) : null}
              {statusQuery.data.availability === "unsupported" ? (
                <p className="text-warning font-medium">
                  无法确认 {title} 版本。
                </p>
              ) : null}
              {statusQuery.data.installationProbeDiagnostic ? (
                <p className="text-warning mt-2 text-xs">
                  {installationProbeDiagnosticText(
                    statusQuery.data.installationProbeDiagnostic,
                  )}
                  （诊断码：
                  <code>{statusQuery.data.installationProbeDiagnostic}</code>）
                </p>
              ) : null}
              <p>{statusQuery.data.newSessionNotice}</p>
              {statusQuery.data.bearerTokenWarning ? (
                <p className="text-warning mt-2">
                  {statusQuery.data.bearerTokenWarning}
                </p>
              ) : null}
              {statusQuery.data.promptOverride === "present" ? (
                <p className="text-warning mt-2 font-medium">
                  检测到更高优先级的 Codex 指令来源（如
                  AGENTS.override.md）；当前 AGENTS.md 可能被遮蔽。
                </p>
              ) : null}
              {statusQuery.data.promptOverride === "unknown" ? (
                <p className="text-warning mt-2 font-medium">
                  无法安全确认 Codex 指令遮蔽状态，请检查 AGENTS.override.md
                  后再应用。
                </p>
              ) : null}
              {statusQuery.data.providerPolicy === "blocked" ? (
                <p className="text-destructive mt-2 font-medium">
                  Claude Provider 由宿主平台管理，本应用不会覆盖渠道配置。
                </p>
              ) : null}
              {statusQuery.data.providerPolicy === "unknown" ? (
                <p className="text-warning mt-2 font-medium">
                  无法确认 Claude Provider
                  是否由宿主管理；渠道预览将保持阻止状态。
                </p>
              ) : null}
            </section>
          ) : null}
          {targetStatusesQuery.data?.map((target) => {
            const presentation = globalTargetStatusPresentation(
              target.status,
              target.diagnosticCode,
            );
            const artifactLabel =
              target.artifactKind === "provider" ? "Provider" : "提示词";
            return (
              <section
                key={`${target.artifactKind}-${target.tool}`}
                className="bg-card rounded-lg border p-4 text-sm"
                aria-label={`${title} ${artifactLabel}原生状态`}
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <h2 className="font-semibold">{artifactLabel}原生状态</h2>
                  <SyncStatusBadge
                    status={target.status}
                    label={presentation.label}
                    tone={presentation.tone}
                  />
                </div>
                <code className="mt-2 block text-xs break-all">
                  {target.targetPath ?? "目标路径不可用"}
                </code>
                {presentation.description ? (
                  <p className="text-muted-foreground mt-2 text-xs">
                    {presentation.description}
                  </p>
                ) : null}
                {target.diagnosticCode ? (
                  <p className="text-warning mt-2 text-xs">
                    诊断码：<code>{target.diagnosticCode}</code>
                  </p>
                ) : null}
                <ExternalChangeActions
                  artifactKind={target.artifactKind}
                  tool={target.tool}
                  status={target.status}
                  onInvalidate={async () => {
                    await Promise.all([
                      queryClient.invalidateQueries({
                        queryKey: profileKeys.all,
                      }),
                      queryClient.invalidateQueries({
                        queryKey: profileKeys.targetStatuses(tool),
                      }),
                    ]);
                  }}
                  onMatchOrImport={
                    target.artifactKind === "prompt"
                      ? () => {
                          void navigate("/prompts");
                        }
                      : undefined
                  }
                />
              </section>
            );
          })}
          {statusQuery.isPending ? (
            <p role="status" className="bg-card rounded-lg border p-4 text-sm">
              正在检测工具配置状态…
            </p>
          ) : null}
          {statusQuery.isError ? (
            <p
              role="alert"
              className={`rounded-lg border p-4 text-sm ${toneClass("destructive")}`}
            >
              {profileErrorText(statusQuery.error)}
            </p>
          ) : null}
        </div>

        <div>
          {metadata.capabilities.provider ? (
            <ProviderPanel
              tool={tool}
              onPreview={(scopes: readonly SyncScopeDto[]) =>
                requestPreview(scopes)
              }
            />
          ) : null}
          {!metadata.capabilities.provider &&
          metadata.capabilities.promptGlobal ? (
            <section
              className="bg-card rounded-lg border p-4 text-sm"
              aria-labelledby="tool-prompt-entry-title"
            >
              <h2 id="tool-prompt-entry-title" className="font-semibold">
                {title} 提示词
              </h2>
              <p className="text-muted-foreground mt-2">
                正文会写入 <code className="mx-1">.mdc</code> 文件。
              </p>
              {statusQuery.data?.promptTargetPath ? (
                <code className="mt-2 block text-xs break-all">
                  {statusQuery.data.promptTargetPath}
                </code>
              ) : null}
              <Button asChild className="mt-3" size="sm" variant="outline">
                <Link to="/prompts">管理提示词</Link>
              </Button>
            </section>
          ) : null}
        </div>
      </main>
    </>
  );
}
