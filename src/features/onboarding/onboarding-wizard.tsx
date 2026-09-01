import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type ArtifactKind,
  type PreviewPlan,
  type PromptImportPreviewDto,
  type ProviderImportPreviewDto,
  type Tool,
  type ToolAvailabilityState,
} from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { dashboardKeys } from "@/lib/dashboard-api";
import { profileErrorText, profileKeys, unwrapResult } from "@/lib/profile-api";
import { PROFILE_TOOLS, toolMetadata } from "@/lib/tool-metadata";

const storageKey = "easytoagents.onboarding.selections.v1";
const tools = PROFILE_TOOLS;
type ProfileTool = (typeof PROFILE_TOOLS)[number];

interface ToolDiscovery {
  availability: ToolAvailabilityState;
  installationVersion: string | null;
  provider: ProviderImportPreviewDto | null;
  prompt: PromptImportPreviewDto | null;
  providerManaged: boolean;
  promptManaged: boolean;
  errors: string[];
}

interface Choices {
  claude: { provider: boolean; prompt: boolean; skip: boolean };
  codex: { provider: boolean; prompt: boolean; skip: boolean };
}

interface WizardPreview {
  tool: Tool;
  artifactKind: ArtifactKind;
  plan: PreviewPlan;
}

const emptyChoices: Choices = {
  claude: { provider: false, prompt: false, skip: false },
  codex: { provider: false, prompt: false, skip: false },
};

export function OnboardingWizard({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  return open ? <OnboardingWizardContent onClose={onClose} /> : null;
}

function OnboardingWizardContent({ onClose }: { onClose: () => void }) {
  const queryClient = useQueryClient();
  const [step, setStep] = useState<"detect" | "select" | "preview" | "done">(
    "detect",
  );
  const [discovery, setDiscovery] = useState<Record<
    ProfileTool,
    ToolDiscovery
  > | null>(null);
  const [choices, setChoices] = useState<Choices>(() => readChoices());
  const [previews, setPreviews] = useState<WizardPreview[]>([]);
  const [hasAppliedPreview, setHasAppliedPreview] = useState(false);
  const { dialogRef, onKeyDown } = useDialogFocus(true, onClose);

  const detectMutation = useMutation({
    mutationFn: async () => {
      const entries = await Promise.all(
        tools.map(async (tool): Promise<[ProfileTool, ToolDiscovery]> => {
          const errors: string[] = [];
          const [
            statusResult,
            providerResult,
            promptResult,
            providersResult,
            promptsResult,
          ] = await Promise.allSettled([
            commands.getToolProfileStatus(tool).then(unwrapResult),
            commands.discoverProviderImport(tool).then(unwrapResult),
            commands.discoverPromptImport(tool).then(unwrapResult),
            commands.listProviderProfiles(tool).then(unwrapResult),
            commands.listPromptProfiles().then(unwrapResult),
          ]);
          const status = settledValue(
            statusResult,
            errors,
            "工具安装状态读取失败",
          );
          const provider = settledValue(
            providerResult,
            errors,
            "Provider 检测失败",
          );
          const prompt = settledValue(promptResult, errors, "提示词检测失败");
          const providerManaged =
            settledValue(
              providersResult,
              errors,
              "中央 Provider 状态读取失败",
            )?.some((profile) => profile.isActive) ?? false;
          const promptManaged =
            settledValue(promptsResult, errors, "中央提示词状态读取失败")?.some(
              (profile) => profile.globalTools.includes(tool),
            ) ?? false;
          return [
            tool,
            {
              availability: status?.availability ?? "unsupported",
              installationVersion: status?.installationVersion ?? null,
              provider,
              prompt,
              providerManaged,
              promptManaged,
              errors,
            },
          ];
        }),
      );
      const result: Record<ProfileTool, ToolDiscovery> = {
        claude: {
          availability: "unsupported",
          installationVersion: null,
          provider: null,
          prompt: null,
          providerManaged: false,
          promptManaged: false,
          errors: [],
        },
        codex: {
          availability: "unsupported",
          installationVersion: null,
          provider: null,
          prompt: null,
          providerManaged: false,
          promptManaged: false,
          errors: [],
        },
      };
      for (const [tool, toolDiscovery] of entries) {
        result[tool] = toolDiscovery;
      }
      return result;
    },
    onSuccess: (result) => {
      setDiscovery(result);
      setStep("select");
    },
  });
  const prepareMutation = useMutation({
    mutationFn: async () => {
      const prepared: WizardPreview[] = [];
      for (const tool of tools) {
        const selected = choices[tool];
        const found = discovery?.[tool];
        if (selected.skip || !found) continue;
        if (selected.provider && found.provider) {
          unwrapResult(
            await commands.confirmProviderImport({
              previewId: found.provider.previewId,
              name: found.provider.suggestedName,
            }),
          );
        }
        if (selected.provider && (found.provider || found.providerManaged)) {
          prepared.push({
            tool,
            artifactKind: "provider",
            plan: unwrapResult(await commands.previewProviderSync(tool)),
          });
        }
        if (selected.prompt && found.prompt) {
          unwrapResult(
            await commands.confirmPromptImport({
              previewId: found.prompt.previewId,
              name: found.prompt.suggestedName,
            }),
          );
        }
        if (selected.prompt && (found.prompt || found.promptManaged)) {
          prepared.push({
            tool,
            artifactKind: "prompt",
            plan: unwrapResult(await commands.previewPromptSync(tool, null)),
          });
        }
      }
      if (prepared.length === 0) {
        unwrapResult(await commands.completeOnboarding());
      }
      return prepared;
    },
    onSuccess: async (result) => {
      setPreviews(result);
      setStep(result.length === 0 ? "done" : "preview");
      if (result.length === 0) {
        localStorage.removeItem(storageKey);
        await queryClient.invalidateQueries({ queryKey: dashboardKeys.all });
      }
    },
  });
  const applyMutation = useMutation({
    mutationFn: async () => {
      const remaining = [...previews];
      while (remaining.length > 0) {
        const preview = remaining[0];
        if (!preview) break;
        unwrapResult(
          await commands.applyProfilePreview({
            previewId: preview.plan.previewId,
            tool: preview.tool,
            artifactKind: preview.artifactKind,
            projectId: null,
          }),
        );
        // 多份持久化预览按顺序消费。部分成功后只保留未消费项，确保重试
        // 不会再次提交已经 consumed 的 preview。
        remaining.shift();
        setHasAppliedPreview(true);
        setPreviews([...remaining]);
      }
    },
    onSuccess: async () => {
      setStep("done");
      localStorage.removeItem(storageKey);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: dashboardKeys.all }),
        queryClient.invalidateQueries({ queryKey: profileKeys.all }),
      ]);
    },
  });

  useEffect(() => {
    detectMutation.mutate();
    // 组件每次打开都会重新挂载，因此必须重新读取原生目标，不能复用旧 discovery。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    localStorage.setItem(storageKey, JSON.stringify(choices));
  }, [choices]);

  const blockedPreview = previews.some((preview) =>
    preview.plan.targets.some(
      (target) => target.changeKind === "conflict" || target.errorCode !== null,
    ),
  );
  const previewWarnings = previews.flatMap(
    (preview) => preview.plan.warningCodes,
  );
  const operationError = profileErrorText(
    detectMutation.error ?? prepareMutation.error ?? applyMutation.error,
  );
  const canPrepare =
    discovery !== null &&
    tools.every((tool) => {
      const choice = choices[tool];
      const found = discovery[tool];
      return (
        choice.skip ||
        (found.availability === "installed" &&
          ((choice.provider &&
            (found.provider !== null || found.providerManaged)) ||
            (choice.prompt && (found.prompt !== null || found.promptManaged))))
      );
    });

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4">
      <section
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-title"
        aria-describedby="onboarding-description"
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="bg-card max-h-[90vh] w-full max-w-4xl overflow-auto rounded-xl p-6 shadow-xl"
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="text-muted-foreground text-sm">首次接管向导</p>
            <h2 id="onboarding-title" className="mt-1 text-xl font-semibold">
              检测 → 选择 → 预览 → 应用
            </h2>
          </div>
          <Button variant="outline" size="sm" onClick={onClose}>
            暂停向导
          </Button>
        </div>
        <p
          id="onboarding-description"
          className="text-muted-foreground mt-3 text-sm"
        >
          暂停会保留选择；下次继续时会重新检测。跳过工具不会创建档案，也不会写入其配置。
        </p>
        <ol className="mt-4 flex flex-wrap gap-2 text-xs" aria-label="向导步骤">
          {(["detect", "select", "preview", "done"] as const).map(
            (item, index) => (
              <li
                key={item}
                aria-current={step === item ? "step" : undefined}
                className={
                  step === item ? "font-semibold" : "text-muted-foreground"
                }
              >
                {index + 1}. {stepLabel(item)}
              </li>
            ),
          )}
        </ol>

        {operationError ? (
          <div className="mt-4">
            <BlockingState
              title="向导操作未完成"
              description={operationError}
              {...(step === "detect"
                ? {
                    actionLabel: "重新检测",
                    onAction: () => detectMutation.mutate(),
                  }
                : {})}
            />
          </div>
        ) : null}

        {step === "detect" ? (
          <p role="status" className="mt-6 text-sm">
            正在只读检测 Claude 与 Codex 的 Provider 和全局提示词…
          </p>
        ) : null}

        {step === "select" && discovery ? (
          <div className="mt-6 grid gap-4 md:grid-cols-2">
            {tools.map((tool) => {
              const found = discovery[tool];
              const choice = choices[tool];
              const providerDisabledReason =
                providerChoiceDisabledReason(found);
              const promptDisabledReason = promptChoiceDisabledReason(found);
              const providerReasonId = `${tool}-provider-choice-reason`;
              const promptReasonId = `${tool}-prompt-choice-reason`;
              return (
                <fieldset key={tool} className="rounded-lg border p-4">
                  <legend className="px-1 font-semibold">
                    {toolLabel(tool)}
                  </legend>
                  <p className="text-muted-foreground text-sm">
                    {found.availability === "unavailable"
                      ? "未检测到工具安装；原生目标不会被读取或应用，请跳过并保持非受管。"
                      : found.availability === "unsupported"
                        ? "安装探针未能安全确认版本；原生目标保持阻止，请跳过并检查工具安装。"
                        : found.provider || found.prompt
                          ? `已安全检测到${found.installationVersion ? `版本 ${found.installationVersion}，` : ""}可接管的原生配置。`
                          : found.providerManaged || found.promptManaged
                            ? "已存在中央档案，可继续生成新的持久化同步预览。"
                            : "未发现可导入配置；可保持非受管。"}
                  </p>
                  {found.provider ? (
                    <div className="bg-muted mt-3 rounded p-3 text-xs">
                      <p className="font-medium">发现 Provider</p>
                      <code className="mt-1 block break-all">
                        {found.provider.targetPath}
                      </code>
                      <p className="text-muted-foreground mt-1">
                        {found.provider.defaultModel} ·{" "}
                        {providerImportCredentialText(tool, found.provider)}
                      </p>
                      <pre className="mt-2 overflow-auto">
                        {JSON.stringify(
                          found.provider.redactedProjection,
                          null,
                          2,
                        )}
                      </pre>
                    </div>
                  ) : null}
                  {found.prompt ? (
                    <div className="bg-muted mt-3 rounded p-3 text-xs">
                      <p className="font-medium">发现全局提示词</p>
                      <code className="mt-1 block break-all">
                        {found.prompt.targetPath}
                      </code>
                    </div>
                  ) : null}
                  {found.errors.map((error) => (
                    <p
                      key={error}
                      role="alert"
                      className="mt-2 text-xs text-amber-800 dark:text-amber-300"
                    >
                      {error}
                    </p>
                  ))}
                  <label className="mt-4 flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={choice.provider}
                      disabled={providerDisabledReason !== null}
                      aria-describedby={
                        providerDisabledReason ? providerReasonId : undefined
                      }
                      onChange={(event) =>
                        updateChoice(
                          setChoices,
                          tool,
                          "provider",
                          event.target.checked,
                        )
                      }
                    />
                    导入并接管 Provider
                  </label>
                  {providerDisabledReason ? (
                    <p
                      id={providerReasonId}
                      className="text-muted-foreground mt-1 pl-6 text-xs"
                    >
                      {providerDisabledReason}
                    </p>
                  ) : null}
                  <label className="mt-3 flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={choice.prompt}
                      disabled={promptDisabledReason !== null}
                      aria-describedby={
                        promptDisabledReason ? promptReasonId : undefined
                      }
                      onChange={(event) =>
                        updateChoice(
                          setChoices,
                          tool,
                          "prompt",
                          event.target.checked,
                        )
                      }
                    />
                    无损导入并接管全局提示词
                  </label>
                  {promptDisabledReason ? (
                    <p
                      id={promptReasonId}
                      className="text-muted-foreground mt-1 pl-6 text-xs"
                    >
                      {promptDisabledReason}
                    </p>
                  ) : null}
                  <label className="mt-3 flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={choice.skip}
                      onChange={(event) =>
                        setChoices((current) => ({
                          ...current,
                          [tool]: {
                            provider: false,
                            prompt: false,
                            skip: event.target.checked,
                          },
                        }))
                      }
                    />
                    跳过 {toolLabel(tool)}，保持非受管
                  </label>
                </fieldset>
              );
            })}
            <div className="flex justify-end md:col-span-2">
              <Button
                disabled={!canPrepare || prepareMutation.isPending}
                onClick={() => prepareMutation.mutate()}
              >
                {prepareMutation.isPending
                  ? "正在生成预览…"
                  : "确认选择并生成预览"}
              </Button>
            </div>
          </div>
        ) : null}

        {step === "preview" ? (
          <div className="mt-6 space-y-4">
            {previewWarnings.length > 0 ? (
              <ul className="list-disc rounded-lg border border-amber-200 bg-amber-50 p-4 pl-9 text-sm text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/40 dark:text-amber-300">
                {previewWarnings.map((warning, index) => (
                  <li key={`${warning}-${index}`}>{warning}</li>
                ))}
              </ul>
            ) : null}
            {previews.map((preview) => (
              <article
                key={`${preview.tool}-${preview.artifactKind}`}
                className="rounded-lg border p-4"
              >
                <h3 className="font-medium">
                  {toolLabel(preview.tool)} ·{" "}
                  {artifactLabel(preview.artifactKind)}
                </h3>
                <div className="mt-3 space-y-2">
                  {preview.plan.targets.map((target) => (
                    <div key={target.targetId} className="rounded border p-3">
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <code className="text-xs break-all">
                          {target.descriptor.path ?? "目标路径不可用"}
                        </code>
                        <SyncStatusBadge
                          status={target.status}
                          changeKind={target.changeKind}
                        />
                      </div>
                      {target.errorCode ? (
                        <p
                          role="alert"
                          className="mt-2 text-xs text-red-700 dark:text-red-300"
                        >
                          阻止应用：{target.errorCode}
                        </p>
                      ) : null}
                      {target.warningCodes.length > 0 ? (
                        <ul className="mt-2 list-disc pl-5 text-xs text-amber-800 dark:text-amber-300">
                          {target.warningCodes.map((warning) => (
                            <li key={warning}>{warning}</li>
                          ))}
                        </ul>
                      ) : null}
                      <pre className="bg-muted mt-3 overflow-auto rounded p-3 text-xs">
                        {JSON.stringify(target.redactedDiff, null, 2)}
                      </pre>
                    </div>
                  ))}
                </div>
              </article>
            ))}
            <div className="flex justify-end gap-3">
              <Button
                variant="outline"
                disabled={hasAppliedPreview}
                onClick={() => setStep("select")}
              >
                {hasAppliedPreview ? "已有应用，不能返回选择" : "返回选择"}
              </Button>
              <Button
                disabled={blockedPreview || applyMutation.isPending}
                onClick={() => applyMutation.mutate()}
              >
                {applyMutation.isPending ? "正在应用…" : "应用全部预览"}
              </Button>
            </div>
          </div>
        ) : null}

        {step === "done" ? (
          <div className="mt-6 rounded-lg border border-emerald-200 bg-emerald-50 p-5 dark:border-emerald-900/60 dark:bg-emerald-950/40">
            <p className="font-semibold">向导已完成</p>
            <p className="mt-2 text-sm">
              已选择项完成导入与显式应用；跳过或未选择的工具保持非受管。
            </p>
            <Button className="mt-4" onClick={onClose}>
              返回总览
            </Button>
          </div>
        ) : null}
      </section>
    </div>
  );
}

function readChoices(): Choices {
  try {
    const saved = localStorage.getItem(storageKey);
    if (!saved) return emptyChoices;
    const parsed: unknown = JSON.parse(saved);
    if (!isRecord(parsed)) return emptyChoices;
    return {
      claude: readToolChoice(parsed.claude),
      codex: readToolChoice(parsed.codex),
    };
  } catch {
    return emptyChoices;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readToolChoice(value: unknown) {
  if (!isRecord(value)) {
    return { provider: false, prompt: false, skip: false };
  }
  return {
    provider: value.provider === true,
    prompt: value.prompt === true,
    skip: value.skip === true,
  };
}

function updateChoice(
  setChoices: React.Dispatch<React.SetStateAction<Choices>>,
  tool: ProfileTool,
  field: "provider" | "prompt",
  value: boolean,
) {
  setChoices((current) => ({
    ...current,
    [tool]: { ...current[tool], [field]: value, skip: false },
  }));
}

function providerChoiceDisabledReason(found: ToolDiscovery): string | null {
  const availabilityReason = availabilityDisabledReason(found.availability);
  if (availabilityReason) {
    return availabilityReason;
  }
  if (!found.provider && !found.providerManaged) {
    return "未发现可导入 Provider，也没有生效的中央 Provider 档案。";
  }
  return null;
}

function promptChoiceDisabledReason(found: ToolDiscovery): string | null {
  const availabilityReason = availabilityDisabledReason(found.availability);
  if (availabilityReason) {
    return availabilityReason;
  }
  if (!found.prompt && !found.promptManaged) {
    return "未发现可导入全局提示词，也没有生效的中央提示词档案。";
  }
  return null;
}

function availabilityDisabledReason(
  availability: ToolAvailabilityState,
): string | null {
  switch (availability) {
    case "installed":
      return null;
    case "unavailable":
      return "未检测到工具安装，无法读取或应用原生目标。";
    case "unsupported":
      return "安装探针未能安全确认版本，无法读取或应用原生目标。";
  }
}

function settledValue<T>(
  result: PromiseSettledResult<T>,
  errors: string[],
  fallback: string,
): T | null {
  if (result.status === "fulfilled") {
    return result.value;
  }
  errors.push(profileErrorText(result.reason) ?? fallback);
  return null;
}

function toolLabel(tool: Tool) {
  return toolMetadata(tool).label;
}

function providerImportCredentialText(
  tool: Tool,
  preview: ProviderImportPreviewDto,
): string {
  if (tool === "codex" && !preview.apiKeyConfigured) {
    return "使用 Codex OAuth 登录";
  }
  return preview.apiKeyConfigured ? "密钥已遮罩保存" : "密钥未配置";
}

function artifactLabel(kind: ArtifactKind) {
  return kind === "provider" ? "Provider" : "全局提示词";
}

function stepLabel(step: "detect" | "select" | "preview" | "done") {
  switch (step) {
    case "detect":
      return "检测";
    case "select":
      return "选择导入/接管";
    case "preview":
      return "预览并应用";
    case "done":
      return "完成";
  }
}
