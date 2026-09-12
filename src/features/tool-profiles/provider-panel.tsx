import { useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type ClaudeCredentialEnvKey,
  type ProviderAuthKind,
  type ProviderImportPreviewDto,
  type ProviderProfileDto,
  type Tool,
} from "@/bindings/commands";
import { FormDialog } from "@/components/form-dialog";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { useNotify } from "@/components/use-notify";
import { OfficialLoginSection } from "@/features/tool-profiles/official-login-section";
import {
  isOfficialLoginProfile,
  OFFICIAL_LOGIN_TOOLS,
  providerCredentialText,
  providerImportCredentialText,
  providerModelText,
} from "@/features/tool-profiles/provider-text";
import { useSubmitGuard } from "@/hooks/use-submit-guard";
import {
  profileErrorText,
  profileKeys,
  providerProfilesQueryOptions,
  unwrapResult,
} from "@/lib/profile-api";
import { toneClass } from "@/lib/tone-class";
import { toolMetadata } from "@/lib/tool-metadata";

interface ProviderPanelProps {
  tool: Tool;
  directApply: boolean;
  onPreview: () => void;
}

interface ProviderFormState {
  authKind: ProviderAuthKind;
  name: string;
  apiBaseUrl: string;
  apiKey: string;
  defaultModel: string;
  credentialEnvKey: ClaudeCredentialEnvKey;
  extraEnvText: string;
  wireApi: string;
  zcodeKind: string;
  opencodeNpm: string;
  opencodeApi: string;
}

const emptyForm: ProviderFormState = {
  authKind: "api_key",
  name: "",
  apiBaseUrl: "",
  apiKey: "",
  defaultModel: "",
  credentialEnvKey: "ANTHROPIC_API_KEY",
  extraEnvText: "",
  wireApi: "",
  zcodeKind: "anthropic",
  opencodeNpm: "@ai-sdk/openai-compatible",
  opencodeApi: "openai-compatible",
};

export function ProviderPanel({
  tool,
  directApply,
  onPreview,
}: ProviderPanelProps) {
  const queryClient = useQueryClient();
  const enabledTools = useEnabledTools();
  const counterpartTool: Tool = tool === "claude" ? "codex" : "claude";
  const profilesQuery = useQuery(providerProfilesQueryOptions(tool));
  const [editing, setEditing] = useState<ProviderProfileDto | null>(null);
  const [form, setForm] = useState<ProviderFormState>(emptyForm);
  const [formOpen, setFormOpen] = useState(false);
  const submitGuard = useSubmitGuard();
  const [importPreview, setImportPreview] =
    useState<ProviderImportPreviewDto | null>(null);
  const { notify } = useNotify();
  const supportsOfficialLogin = OFFICIAL_LOGIN_TOOLS.has(tool);
  const official = supportsOfficialLogin && form.authKind === "official_login";

  const refresh = async () => {
    await queryClient.invalidateQueries({
      queryKey: profileKeys.providers(tool),
    });
  };

  const saveMutation = useMutation({
    mutationFn: async () => {
      const extraEnv = parseExtraEnv(form.extraEnvText);
      // 官方账号登录渠道不保存接入地址、密钥与传输协议；额外 env 只属于 Claude。
      const options = {
        authKind: official ? ("official_login" as const) : ("api_key" as const),
        credentialEnvKey:
          tool === "claude" && !official ? form.credentialEnvKey : null,
        extraEnv: tool === "claude" ? extraEnv : {},
        wireApi:
          tool === "codex" && !official && form.wireApi ? form.wireApi : null,
        zcodeKind: tool === "zcode" ? form.zcodeKind : null,
        opencodeNpm: tool === "opencode" ? form.opencodeNpm : null,
        opencodeApi: tool === "opencode" ? form.opencodeApi : null,
      };
      const apiBaseUrl = official ? "" : form.apiBaseUrl;
      if (editing) {
        return unwrapResult(
          await commands.updateProviderProfile({
            id: editing.id,
            name: form.name,
            apiBaseUrl,
            apiKey:
              !official && form.apiKey
                ? { action: "replace", value: form.apiKey }
                : { action: "keep" },
            defaultModel: form.defaultModel,
            options,
            rowVersion: editing.rowVersion,
          }),
        );
      }
      return unwrapResult(
        await commands.createProviderProfile({
          tool,
          name: form.name,
          apiBaseUrl,
          apiKey: official ? "" : form.apiKey,
          defaultModel: form.defaultModel,
          options,
          activate: (profilesQuery.data?.length ?? 0) === 0,
        }),
      );
    },
    onSuccess: async () => {
      await refresh();
      setEditing(null);
      setForm(emptyForm);
      setFormOpen(false);
      notify({
        kind: "success",
        message: "中央渠道档案已保存，原生配置尚未修改。",
      });
    },
    onSettled: () => {
      submitGuard.end();
    },
  });

  const openForm = (profile: ProviderProfileDto | null) => {
    if (submitGuard.isInFlight() || saveMutation.isPending) return;
    saveMutation.reset();
    if (profile) {
      editProfile(profile, setEditing, setForm);
    } else {
      setEditing(null);
      setForm(emptyForm);
    }
    setFormOpen(true);
  };

  const closeForm = () => {
    if (submitGuard.isInFlight() || saveMutation.isPending) return;
    setFormOpen(false);
    setEditing(null);
    setForm(emptyForm);
    saveMutation.reset();
  };

  const activateMutation = useMutation({
    mutationFn: async (profile: ProviderProfileDto) =>
      unwrapResult(
        await commands.setActiveProviderProfile(tool, {
          id: profile.id,
          rowVersion: profile.rowVersion,
        }),
      ),
    onSuccess: onPreview,
    // 生效档案的中央写入发生在预览之前；即使预览因策略或路径状态失败，
    // 也必须刷新列表，避免 UI 继续把旧档案显示为生效。
    onSettled: refresh,
  });
  const copyMutation = useMutation({
    mutationFn: async (profile: ProviderProfileDto) =>
      unwrapResult(
        await commands.copyProviderProfile({
          sourceId: profile.id,
          targetTool: tool === "claude" ? "codex" : "claude",
          targetName: `${profile.name}（复制）`,
          activate: false,
        }),
      ),
    onSuccess: async (_copied, profile) => {
      const targetTool = profile.tool === "claude" ? "codex" : "claude";
      await queryClient.invalidateQueries({
        queryKey: profileKeys.providers(targetTool),
      });
      notify({
        kind: "success",
        message: "已按目标工具重新校验并创建独立渠道档案。",
      });
    },
  });
  const deleteMutation = useMutation({
    mutationFn: async (profile: ProviderProfileDto) =>
      unwrapResult(
        await commands.deleteProviderProfile({
          id: profile.id,
          rowVersion: profile.rowVersion,
        }),
      ),
    onSuccess: async () => {
      notify({
        kind: "success",
        message: "中央渠道档案已删除；如需清理原生字段，请生成新的渠道预览。",
      });
      await refresh();
    },
  });
  const discoverMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(await commands.discoverProviderImport(tool)),
    onMutate: () => {
      setImportPreview(null);
    },
    onSuccess: (preview) => {
      setImportPreview(preview);
      if (!preview) {
        notify({
          kind: "success",
          message: profilesQuery.data?.length
            ? "已有中央渠道档案，暂不支持再次接管原生渠道。"
            : tool === "opencode"
              ? "未检测到可导入渠道。请确认配置中的默认模型（model）引用了自定义 provider，且该渠道包含名称、npm 和 baseURL；内置渠道暂不支持导入。"
              : "未检测到可导入的已有渠道配置。",
        });
      }
    },
  });
  const confirmImportMutation = useMutation({
    mutationFn: async () => {
      if (!importPreview) {
        throw new Error("导入预览已关闭");
      }
      return unwrapResult(
        await commands.confirmProviderImport({
          previewId: importPreview.previewId,
          name: importPreview.suggestedName,
        }),
      );
    },
    onSuccess: async () => {
      setImportPreview(null);
      notify({
        kind: "success",
        message: "已有渠道已无写入接管，原生文件内容保持不变。",
      });
      await refresh();
    },
  });

  const mutationError = [
    activateMutation.error,
    copyMutation.error,
    deleteMutation.error,
    discoverMutation.error,
    confirmImportMutation.error,
  ]
    .map(profileErrorText)
    .find(Boolean);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (submitGuard.isInFlight() || saveMutation.isPending) return;
    if (!submitGuard.begin()) return;
    saveMutation.mutate();
  };

  const baseUrlRequired = tool === "zcode" || tool === "opencode";

  return (
    <section
      aria-labelledby={`${tool}-providers-title`}
      className="bg-card rounded-xl border p-5"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 id={`${tool}-providers-title`} className="text-xl font-semibold">
            渠道
          </h2>
          <p className="text-muted-foreground mt-1 text-sm">
            每个工具最多一个中央档案处于生效状态。
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={() => openForm(null)}>
            新增渠道
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={discoverMutation.isPending}
            onClick={() => discoverMutation.mutate()}
          >
            {discoverMutation.isPending ? "正在检测…" : "检测已有配置"}
          </Button>
          <Button size="sm" onClick={onPreview}>
            {directApply ? "直接应用渠道同步" : "预览渠道同步"}
          </Button>
        </div>
      </div>

      {mutationError ? (
        <p role="alert" className="text-destructive mt-4 text-sm">
          {mutationError}
        </p>
      ) : null}
      {profilesQuery.isPending ? (
        <p role="status" className="text-muted-foreground mt-5 text-sm">
          正在加载渠道档案…
        </p>
      ) : null}
      {profilesQuery.isError ? (
        <p role="alert" className="text-destructive mt-5 text-sm">
          {profileErrorText(profilesQuery.error)}
        </p>
      ) : null}
      {profilesQuery.data?.length === 0 ? (
        <p className="text-muted-foreground mt-5 rounded-lg border border-dashed p-4 text-sm">
          尚无渠道档案。点击“新增渠道”创建第一份档案，或先检测已有配置。
        </p>
      ) : null}

      <ul className="mt-5 space-y-3">
        {profilesQuery.data?.map((profile) => (
          <li key={profile.id} className="rounded-lg border p-3">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <div className="flex flex-wrap items-center gap-2">
                  <p className="font-medium">{profile.name}</p>
                  {profile.isActive ? (
                    <span
                      className={`text-success inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 text-xs font-medium ${toneClass("success")}`}
                    >
                      当前生效
                    </span>
                  ) : null}
                </div>
                <p className="text-muted-foreground mt-1 text-xs">
                  {providerModelText(profile.defaultModel)} ·{" "}
                  {providerCredentialText(profile)}
                </p>
              </div>
              <div className="flex flex-wrap gap-2">
                {!profile.isActive ? (
                  <Button
                    size="sm"
                    onClick={() => activateMutation.mutate(profile)}
                  >
                    {directApply ? "切换并直接应用" : "切换并预览"}
                  </Button>
                ) : null}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => openForm(profile)}
                >
                  编辑
                </Button>
                {enabledTools.has(counterpartTool) ? (
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={isOfficialLoginProfile(profile)}
                    onClick={() => copyMutation.mutate(profile)}
                  >
                    复制到{tool === "claude" ? " Codex" : " Claude"}
                  </Button>
                ) : null}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    if (
                      globalThis.confirm(
                        "删除中央渠道档案？原生配置不会在此步骤修改。",
                      )
                    ) {
                      deleteMutation.mutate(profile);
                    }
                  }}
                >
                  删除
                </Button>
              </div>
            </div>
          </li>
        ))}
      </ul>

      {importPreview ? (
        <div className={`mt-5 rounded-lg border p-4 ${toneClass("warning")}`}>
          <p className="font-medium">发现已有渠道，仅生成了导入预览</p>
          <p className="mt-1 text-sm break-all">{importPreview.targetPath}</p>
          <p className="text-muted-foreground mt-1 text-xs">
            {providerModelText(importPreview.defaultModel)} ·{" "}
            {providerImportCredentialText(importPreview)}
          </p>
          {importPreview.skippedEnvKeys.length > 0 ? (
            <p className="mt-2 text-xs">
              以下 env 疑似凭据或格式不受支持，不纳入管理并保持原样：
              {importPreview.skippedEnvKeys.join("、")}
            </p>
          ) : null}
          <pre className="bg-card mt-3 overflow-auto rounded p-3 text-xs dark:bg-slate-900/60">
            {JSON.stringify(importPreview.redactedProjection, null, 2)}
          </pre>
          <div className="mt-3 flex gap-2">
            <Button size="sm" onClick={() => confirmImportMutation.mutate()}>
              确认无写入接管
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setImportPreview(null)}
            >
              跳过
            </Button>
          </div>
        </div>
      ) : null}

      <FormDialog
        open={formOpen}
        title={`${editing ? "编辑" : "新增"} ${toolMetadata(tool).label} 渠道`}
        description="保存只更新中央渠道档案，不会修改原生配置；原生写入仍需预览后确认 Apply。"
        submitLabel={editing ? "保存编辑" : "创建渠道"}
        pending={saveMutation.isPending}
        error={profileErrorText(saveMutation.error)}
        onClose={closeForm}
        onSubmit={submit}
      >
        {supportsOfficialLogin ? (
          <fieldset className="space-y-2 text-sm">
            <legend className="font-medium">认证方式</legend>
            <div className="flex flex-wrap gap-4">
              {AUTH_KIND_OPTIONS.map((option) => (
                <label
                  key={option.value}
                  className="flex items-center gap-2 text-sm"
                >
                  <input
                    type="radio"
                    name={`${tool}-auth-kind`}
                    value={option.value}
                    checked={form.authKind === option.value}
                    disabled={editing !== null}
                    onChange={() =>
                      setForm({ ...form, authKind: option.value })
                    }
                  />
                  {option.label}
                </label>
              ))}
            </div>
            {editing ? (
              <p className="text-muted-foreground text-xs">
                认证方式创建后不可更改；如需切换，请新建渠道。
              </p>
            ) : null}
          </fieldset>
        ) : null}
        <Field label="名称" id={`${tool}-provider-name`}>
          <input
            id={`${tool}-provider-name`}
            required
            className="field"
            value={form.name}
            onChange={(event) =>
              setForm({ ...form, name: event.currentTarget.value })
            }
          />
        </Field>
        {official ? (
          <OfficialLoginSection tool={tool} />
        ) : (
          <>
            <Field label="API 地址" id={`${tool}-provider-url`}>
              <input
                id={`${tool}-provider-url`}
                required={baseUrlRequired}
                type="url"
                className="field"
                placeholder={baseUrlRequired ? "" : "留空则使用官方端点"}
                value={form.apiBaseUrl}
                onChange={(event) =>
                  setForm({ ...form, apiBaseUrl: event.currentTarget.value })
                }
              />
            </Field>
            <Field label="API Key（默认遮罩）" id={`${tool}-provider-key`}>
              <input
                id={`${tool}-provider-key`}
                required={!editing}
                type="password"
                autoComplete="off"
                className="field"
                placeholder={apiKeyPlaceholder(editing)}
                value={form.apiKey}
                onChange={(event) =>
                  setForm({ ...form, apiKey: event.currentTarget.value })
                }
              />
            </Field>
          </>
        )}
        <Field label="默认模型" id={`${tool}-provider-model`}>
          <input
            id={`${tool}-provider-model`}
            required={baseUrlRequired}
            className="field"
            placeholder={baseUrlRequired ? "" : "留空则使用工具默认模型"}
            value={form.defaultModel}
            onChange={(event) =>
              setForm({ ...form, defaultModel: event.currentTarget.value })
            }
          />
        </Field>
        {tool === "zcode" ? (
          <Field label="API 格式" id={`${tool}-zcode-kind`}>
            <select
              id={`${tool}-zcode-kind`}
              className="field"
              value={form.zcodeKind}
              onChange={(event) =>
                setForm({ ...form, zcodeKind: event.currentTarget.value })
              }
            >
              <option value="anthropic">anthropic</option>
              <option value="openai">openai</option>
              <option value="gemini">gemini</option>
            </select>
          </Field>
        ) : null}
        {tool === "claude" ? (
          <>
            {official ? null : (
              <Field label="认证 env key" id={`${tool}-credential-key`}>
                <select
                  id={`${tool}-credential-key`}
                  className="field"
                  value={form.credentialEnvKey}
                  onChange={(event) =>
                    setForm({
                      ...form,
                      credentialEnvKey:
                        event.currentTarget.value === "ANTHROPIC_AUTH_TOKEN"
                          ? "ANTHROPIC_AUTH_TOKEN"
                          : "ANTHROPIC_API_KEY",
                    })
                  }
                >
                  <option value="ANTHROPIC_API_KEY">ANTHROPIC_API_KEY</option>
                  <option value="ANTHROPIC_AUTH_TOKEN">
                    ANTHROPIC_AUTH_TOKEN
                  </option>
                </select>
              </Field>
            )}
            <Field label="额外 env（每行 KEY=VALUE）" id={`${tool}-extra-env`}>
              <textarea
                id={`${tool}-extra-env`}
                className="field min-h-24 resize-y font-mono text-sm"
                placeholder={
                  "CLAUDE_CODE_MAX_OUTPUT_TOKENS=32000\nANTHROPIC_DEFAULT_SONNET_MODEL=claude-sonnet"
                }
                value={form.extraEnvText}
                onChange={(event) =>
                  setForm({ ...form, extraEnvText: event.currentTarget.value })
                }
              />
            </Field>
          </>
        ) : null}
        {tool === "opencode" ? (
          <>
            <Field label="npm SDK" id={`${tool}-npm`}>
              <input
                id={`${tool}-npm`}
                required
                className="field"
                value={form.opencodeNpm}
                onChange={(event) =>
                  setForm({ ...form, opencodeNpm: event.currentTarget.value })
                }
                placeholder="@ai-sdk/openai-compatible"
              />
            </Field>
            <Field label="API 协议" id={`${tool}-api`}>
              <select
                id={`${tool}-api`}
                className="field"
                value={form.opencodeApi}
                onChange={(event) =>
                  setForm({ ...form, opencodeApi: event.currentTarget.value })
                }
              >
                <option value="openai-compatible">openai-compatible</option>
                <option value="openai">openai</option>
                <option value="anthropic">anthropic</option>
              </select>
            </Field>
          </>
        ) : null}
        {tool === "codex" && !official ? (
          <Field label="wire_api" id={`${tool}-wire-api`}>
            <select
              id={`${tool}-wire-api`}
              className="field"
              value={form.wireApi}
              onChange={(event) =>
                setForm({ ...form, wireApi: event.currentTarget.value })
              }
            >
              <option value="">默认</option>
              <option value="responses">responses</option>
              <option value="chat">chat</option>
            </select>
          </Field>
        ) : null}
      </FormDialog>
    </section>
  );
}

const AUTH_KIND_OPTIONS: ReadonlyArray<{
  value: ProviderAuthKind;
  label: string;
}> = [
  { value: "api_key", label: "API Key（第三方 / 自定义接入）" },
  { value: "official_login", label: "官方账号登录（OAuth）" },
];

function apiKeyPlaceholder(editing: ProviderProfileDto | null): string {
  return editing?.apiKeyConfigured ? "留空以保留现有密钥" : "输入密钥";
}

function editProfile(
  profile: ProviderProfileDto,
  setEditing: (profile: ProviderProfileDto) => void,
  setForm: (form: ProviderFormState) => void,
) {
  setEditing(profile);
  setForm({
    authKind: profile.options.authKind,
    name: profile.name,
    apiBaseUrl: profile.apiBaseUrl,
    apiKey: "",
    defaultModel: profile.defaultModel,
    credentialEnvKey: profile.options.credentialEnvKey ?? "ANTHROPIC_API_KEY",
    extraEnvText: Object.entries(profile.options.extraEnv)
      .map(([key, value]) => `${key}=${value}`)
      .join("\n"),
    wireApi: profile.options.wireApi ?? "",
    zcodeKind: profile.options.zcodeKind ?? "anthropic",
    opencodeNpm: profile.options.opencodeNpm ?? "@ai-sdk/openai-compatible",
    opencodeApi: profile.options.opencodeApi ?? "openai-compatible",
  });
}

function parseExtraEnv(text: string): Record<string, string> {
  const entries: Record<string, string> = {};
  for (const line of text.split("\n")) {
    if (!line.trim()) {
      continue;
    }
    const separator = line.indexOf("=");
    if (separator <= 0) {
      throw new Error("额外 env 必须按每行 KEY=VALUE 填写。");
    }
    const key = line.slice(0, separator).trim();
    if (!key) {
      throw new Error("额外 env key 不能为空。");
    }
    if (Object.hasOwn(entries, key)) {
      throw new Error(`额外 env key 不能重复：${key}`);
    }
    entries[key] = line.slice(separator + 1);
  }
  return entries;
}
