import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

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
import {
  DialogBody,
  DialogContent,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { useEnabledTools } from "@/components/use-enabled-tools";
import {
  providerImportCredentialText,
  providerModelText,
} from "@/features/tool-profiles/provider-text";
import { dashboardKeys } from "@/lib/dashboard-api";
import { profileErrorText, profileKeys, unwrapResult } from "@/lib/profile-api";
import { appSettingsQueryOptions } from "@/lib/settings-api";
import { toneClass } from "@/lib/tone-class";
import {
  PROFILE_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";

const storageKey = "easytoagents.onboarding.selections.v1";
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
  cursor: { provider: boolean; prompt: boolean; skip: boolean };
  zcode: { provider: boolean; prompt: boolean; skip: boolean };
  opencode: { provider: boolean; prompt: boolean; skip: boolean };
  pi: { provider: boolean; prompt: boolean; skip: boolean };
}

interface WizardPreview {
  tool: Tool;
  artifactKind: ArtifactKind;
  plan: PreviewPlan;
}

interface PendingToolConfiguration {
  /** 仅返回尚未被中央 active 档案接管的原生证据。 */
  provider: ProviderImportPreviewDto | null;
  prompt: PromptImportPreviewDto | null;
  /** 工具是否已经没有任何需要用户处理的受支持配置。 */
  resolved: boolean;
}

interface DetectionRequest {
  requestId: number;
  tools: readonly ProfileTool[];
}

const emptyChoices: Choices = {
  claude: { provider: false, prompt: false, skip: false },
  codex: { provider: false, prompt: false, skip: false },
  cursor: { provider: false, prompt: false, skip: false },
  zcode: { provider: false, prompt: false, skip: false },
  opencode: { provider: false, prompt: false, skip: false },
  pi: { provider: false, prompt: false, skip: false },
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
  // useEnabledTools 在设置尚未返回时会回落默认集合；检测必须等真实设置
  // 到位后再启动，否则启用 Pi 时首轮检测会在 Pi 加入 tools 前结束。
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const tools = filterEnabledTools(PROFILE_TOOLS, useEnabledTools());
  const toolsKey = tools.join(",");
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
  const detectionRequestId = useRef(0);
  const { dialogRef } = useDialogFocus(true, onClose);

  const detectMutation = useMutation({
    mutationFn: async ({ tools: detectionTools }: DetectionRequest) => {
      const entries = await Promise.all(
        detectionTools.map(
          async (tool): Promise<[ProfileTool, ToolDiscovery]> => {
            const errors: string[] = [];
            const providerSupported = toolMetadata(tool).capabilities.provider;
            const [
              statusResult,
              providerResult,
              promptResult,
              providersResult,
              promptsResult,
            ] = await Promise.allSettled([
              commands.getToolProfileStatus(tool).then(unwrapResult),
              // Provider 不受支持的工具（Cursor）不发起 Provider 导入发现，保持
              // fail closed：原生 Provider 目标不会被读取。
              providerSupported
                ? commands.discoverProviderImport(tool).then(unwrapResult)
                : Promise.resolve(null),
              commands.discoverPromptImport(tool).then(unwrapResult),
              providerSupported
                ? commands.listProviderProfiles(tool).then(unwrapResult)
                : Promise.resolve(null),
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
              settledValue(
                promptsResult,
                errors,
                "中央提示词状态读取失败",
              )?.some((profile) => profile.globalTools.includes(tool)) ?? false;
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
          },
        ),
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
        cursor: {
          availability: "unsupported",
          installationVersion: null,
          provider: null,
          prompt: null,
          providerManaged: false,
          promptManaged: false,
          errors: [],
        },
        zcode: {
          availability: "unsupported",
          installationVersion: null,
          provider: null,
          prompt: null,
          providerManaged: false,
          promptManaged: false,
          errors: [],
        },
        opencode: {
          availability: "unsupported",
          installationVersion: null,
          provider: null,
          prompt: null,
          providerManaged: false,
          promptManaged: false,
          errors: [],
        },
        pi: {
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
    onSuccess: (result, variables) => {
      // 设置变化或用户重扫可能让多个只读检测同时在途；旧响应不能覆盖
      // 最新工具集合和中央档案状态。
      if (variables.requestId !== detectionRequestId.current) return;
      setDiscovery(result);
      setChoices((current) =>
        reconcileChoices(current, result, variables.tools),
      );
      setStep("select");
    },
  });
  const startDetection = () => {
    const requestId = detectionRequestId.current + 1;
    detectionRequestId.current = requestId;
    // 新一轮检测开始后，旧 discovery/Preview 不再是可提交证据；先清空
    // 它们，避免新请求失败时把旧选择重新暴露给用户。
    setDiscovery(null);
    setPreviews([]);
    setHasAppliedPreview(false);
    setStep("detect");
    if (!prepareMutation.isPending) prepareMutation.reset();
    if (!applyMutation.isPending) applyMutation.reset();
    detectMutation.mutate({ requestId, tools });
  };
  const prepareMutation = useMutation({
    mutationFn: async () => {
      const prepared: WizardPreview[] = [];
      const currentChoices = reconcileChoices(choices, discovery, tools);
      for (const tool of tools) {
        const selected = currentChoices[tool];
        const found = discovery?.[tool];
        if (selected.skip || !found) continue;
        const pending = pendingToolConfiguration(tool, found);
        if (selected.provider && pending.provider) {
          unwrapResult(
            await commands.confirmProviderImport({
              previewId: pending.provider.previewId,
              name: pending.provider.suggestedName,
            }),
          );
        }
        if (selected.provider && pending.provider) {
          prepared.push({
            tool,
            artifactKind: "provider",
            plan: unwrapResult(await commands.previewProviderSync(tool)),
          });
        }
        if (selected.prompt && pending.prompt) {
          unwrapResult(
            await commands.confirmPromptImport({
              previewId: pending.prompt.previewId,
              name: pending.prompt.suggestedName,
            }),
          );
        }
        if (selected.prompt && pending.prompt) {
          prepared.push({
            tool,
            artifactKind: "prompt",
            plan: unwrapResult(await commands.previewPromptSync(tool)),
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
    if (settingsQuery.isPending) return;
    let active = true;
    // 让首次检测在当前渲染提交后启动；这样清空旧证据不会在 effect
    // 体内同步触发级联渲染，设置变化时仍保持 fail closed。
    queueMicrotask(() => {
      if (active) startDetection();
    });
    return () => {
      active = false;
    };
    // 组件每次打开都会重新挂载；启用工具设置变化时也必须重新读取原生目标。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settingsQuery.isPending, toolsKey]);

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
  const isDetecting = detectMutation.isPending;
  const operationError = isDetecting
    ? null
    : profileErrorText(
        detectMutation.error ?? prepareMutation.error ?? applyMutation.error,
      );
  // localStorage 中的选择只是用户偏好，不是当前检测证据。中央档案状态
  // 变化后，已接管的旧选择在渲染、prepare 和 canPrepare 三处均被收敛掉。
  const effectiveChoices = reconcileChoices(choices, discovery, tools);
  const canPrepare =
    discovery !== null &&
    !isDetecting &&
    tools.every((tool) => {
      const choice = effectiveChoices[tool];
      const found = discovery[tool];
      const pending = pendingToolConfiguration(tool, found);
      return (
        choice.skip ||
        pending.resolved ||
        (found.availability === "installed" &&
          ((choice.provider && pending.provider !== null) ||
            (choice.prompt && pending.prompt !== null)))
      );
    });

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={onClose}
        labelledBy="onboarding-title"
        describedBy="onboarding-description"
        size="lg"
      >
        <DialogHeader>
          <div className="min-w-0">
            <p className="text-muted-foreground">首次接管向导</p>
            <h2
              id="onboarding-title"
              className="mt-1 text-[15px] font-semibold"
            >
              检测 → 选择 → 预览 → 应用
            </h2>
          </div>
          <Button variant="outline" size="sm" onClick={onClose}>
            暂停向导
          </Button>
        </DialogHeader>
        <DialogBody>
          <p id="onboarding-description" className="text-muted-foreground">
            暂停会保留选择；下次继续时会重新检测。跳过工具不会创建档案，也不会写入其配置。
          </p>
          <ol
            className="mt-4 flex flex-wrap gap-2 text-xs"
            aria-label="向导步骤"
          >
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
                      onAction: startDetection,
                    }
                  : {})}
              />
            </div>
          ) : null}

          {step === "detect" || isDetecting ? (
            <p role="status" className="mt-6 text-sm">
              正在只读检测各工具的 Provider 与全局提示词…
            </p>
          ) : null}

          {step === "select" && discovery && !isDetecting ? (
            <div className="mt-6 grid min-w-0 gap-4 md:grid-cols-2">
              {tools.map((tool) => {
                const found = discovery[tool];
                const choice = effectiveChoices[tool];
                const pending = pendingToolConfiguration(tool, found);
                // 已完成接管的工具不再要求用户“跳过”，也不显示空卡片。
                if (pending.resolved) return null;
                const providerSupported =
                  toolMetadata(tool).capabilities.provider;
                const promptSupported =
                  toolMetadata(tool).capabilities.promptGlobal;
                const providerDisabledReason = providerSupported
                  ? providerChoiceDisabledReason(found, pending.provider)
                  : null;
                const promptDisabledReason = promptSupported
                  ? promptChoiceDisabledReason(found, pending.prompt)
                  : null;
                const providerReasonId = `${tool}-provider-choice-reason`;
                const promptReasonId = `${tool}-prompt-choice-reason`;
                return (
                  <fieldset
                    key={tool}
                    className="min-w-0 rounded-lg border p-4"
                  >
                    <legend className="px-1 font-semibold">
                      {toolLabel(tool)}
                    </legend>
                    <p className="text-muted-foreground text-sm">
                      {found.availability === "unavailable"
                        ? "未检测到安装，请跳过。"
                        : found.availability === "unsupported"
                          ? "无法确认版本，请检查安装后重试。"
                          : pending.provider || pending.prompt
                            ? `已检测到${found.installationVersion ? `版本 ${found.installationVersion}，` : ""}可接管的原生配置。`
                            : found.providerManaged || found.promptManaged
                              ? "已有中央档案；其余项目可选择跳过。"
                              : "未发现可导入配置。"}
                    </p>
                    {pending.provider ? (
                      <div className="bg-muted rounded-control mt-3 max-w-full min-w-0 p-3 text-xs">
                        <p className="font-medium">发现 Provider</p>
                        <code className="mt-1 block break-all">
                          {pending.provider.targetPath}
                        </code>
                        <p className="text-muted-foreground mt-1">
                          {providerModelText(pending.provider.defaultModel)} ·{" "}
                          {providerImportCredentialText(pending.provider)}
                        </p>
                        {pending.provider.skippedEnvKeys.length > 0 ? (
                          <p className="text-muted-foreground mt-1">
                            以下 env
                            疑似凭据或格式不受支持，不纳入管理并保持原样：
                            {pending.provider.skippedEnvKeys.join("、")}
                          </p>
                        ) : null}
                        <pre className="mt-2 max-w-full overflow-auto">
                          {JSON.stringify(
                            pending.provider.redactedProjection,
                            null,
                            2,
                          )}
                        </pre>
                      </div>
                    ) : null}
                    {pending.prompt ? (
                      <div className="bg-muted rounded-control mt-3 max-w-full min-w-0 p-3 text-xs">
                        <p className="font-medium">发现全局提示词</p>
                        <code className="mt-1 block break-all">
                          {pending.prompt.targetPath}
                        </code>
                      </div>
                    ) : null}
                    {found.errors.map((error) => (
                      <p
                        key={error}
                        role="alert"
                        className="text-warning mt-2 text-xs"
                      >
                        {error}
                      </p>
                    ))}
                    {providerSupported && !found.providerManaged ? (
                      <>
                        <label className="mt-4 flex items-center gap-2 text-sm">
                          <input
                            type="checkbox"
                            checked={choice.provider}
                            disabled={providerDisabledReason !== null}
                            aria-describedby={
                              providerDisabledReason
                                ? providerReasonId
                                : undefined
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
                      </>
                    ) : null}
                    {promptSupported && !found.promptManaged ? (
                      <>
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
                      </>
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

          {step === "preview" && !isDetecting ? (
            <div className="mt-6 space-y-4">
              {previewWarnings.length > 0 ? (
                <ul
                  className={`text-warning list-disc rounded-lg border p-4 pl-9 text-sm ${toneClass("warning")}`}
                >
                  {previewWarnings.map((warning, index) => (
                    <li key={`${warning}-${index}`}>{warning}</li>
                  ))}
                </ul>
              ) : null}
              {previews.map((preview) => (
                <article
                  key={`${preview.tool}-${preview.artifactKind}`}
                  className="min-w-0 rounded-lg border p-4"
                >
                  <h3 className="font-medium">
                    {toolLabel(preview.tool)} ·{" "}
                    {artifactLabel(preview.artifactKind)}
                  </h3>
                  <div className="mt-3 space-y-2">
                    {preview.plan.targets.map((target) => (
                      <div
                        key={target.targetId}
                        className="rounded-control min-w-0 border p-3"
                      >
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
                            className="text-destructive mt-2 text-xs"
                          >
                            阻止应用：{target.errorCode}
                          </p>
                        ) : null}
                        {target.warningCodes.length > 0 ? (
                          <ul className="text-warning mt-2 list-disc pl-5 text-xs">
                            {target.warningCodes.map((warning) => (
                              <li key={warning}>{warning}</li>
                            ))}
                          </ul>
                        ) : null}
                        <pre className="bg-muted rounded-control mt-3 max-w-full overflow-auto p-3 text-xs">
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

          {step === "done" && !isDetecting ? (
            <div
              className={`mt-6 rounded-lg border p-5 ${toneClass("success")}`}
            >
              <p className="font-semibold">向导已完成</p>
              <p className="mt-2 text-sm">跳过的工具未做任何修改。</p>
              <Button className="mt-4" onClick={onClose}>
                返回总览
              </Button>
            </div>
          ) : null}
        </DialogBody>
      </DialogContent>
    </DialogOverlay>
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
      cursor: readToolChoice(parsed.cursor),
      zcode: readToolChoice(parsed.zcode),
      opencode: readToolChoice(parsed.opencode),
      pi: readToolChoice(parsed.pi),
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

function pendingToolConfiguration(
  tool: ProfileTool,
  found: ToolDiscovery,
): PendingToolConfiguration {
  const capabilities = toolMetadata(tool).capabilities;
  const provider =
    capabilities.provider && !found.providerManaged ? found.provider : null;
  const prompt =
    capabilities.promptGlobal && !found.promptManaged ? found.prompt : null;
  const resolved =
    (!capabilities.provider || found.providerManaged) &&
    (!capabilities.promptGlobal || found.promptManaged);
  return { provider, prompt, resolved };
}

function reconcileChoices(
  choices: Choices,
  discovery: Record<ProfileTool, ToolDiscovery> | null,
  tools: readonly ProfileTool[],
): Choices {
  if (!discovery) return choices;
  const next = { ...choices };
  for (const tool of tools) {
    const found = discovery[tool];
    const pending = pendingToolConfiguration(tool, found);
    const choice = choices[tool];
    next[tool] = {
      ...choice,
      // 旧选择只代表用户偏好，不能重新获得已接管项的提交资格。
      provider: choice.provider && pending.provider !== null,
      prompt: choice.prompt && pending.prompt !== null,
    };
  }
  return next;
}

function providerChoiceDisabledReason(
  found: ToolDiscovery,
  pending: ProviderImportPreviewDto | null,
): string | null {
  const availabilityReason = availabilityDisabledReason(found.availability);
  if (availabilityReason) {
    return availabilityReason;
  }
  if (!pending) {
    return "未发现可导入的 Provider。";
  }
  return null;
}

function promptChoiceDisabledReason(
  found: ToolDiscovery,
  pending: PromptImportPreviewDto | null,
): string | null {
  const availabilityReason = availabilityDisabledReason(found.availability);
  if (availabilityReason) {
    return availabilityReason;
  }
  if (!pending) {
    return "未发现可导入的全局提示词。";
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
      return "未检测到安装，无法读取或应用原生目标。";
    case "unsupported":
      return "无法确认版本，无法读取或应用原生目标。";
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
