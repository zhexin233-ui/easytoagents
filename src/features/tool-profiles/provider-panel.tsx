import { useRef, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type ClaudeCredentialEnvKey,
  type PreviewPlan,
  type ProviderImportPreviewDto,
  type ProviderProfileDto,
  type Tool,
} from "@/bindings/commands";
import { FormDialog } from "@/components/form-dialog";
import { Button } from "@/components/ui/button";
import {
  profileErrorText,
  profileKeys,
  providerProfilesQueryOptions,
  unwrapResult,
} from "@/lib/profile-api";

interface ProviderPanelProps {
  tool: Tool;
  onPreview: (preview: PreviewPlan) => void;
}

interface ProviderFormState {
  name: string;
  apiBaseUrl: string;
  apiKey: string;
  defaultModel: string;
  credentialEnvKey: ClaudeCredentialEnvKey;
  extraEnvText: string;
  wireApi: string;
}

const emptyForm: ProviderFormState = {
  name: "",
  apiBaseUrl: "",
  apiKey: "",
  defaultModel: "",
  credentialEnvKey: "ANTHROPIC_API_KEY",
  extraEnvText: "",
  wireApi: "",
};

export function ProviderPanel({ tool, onPreview }: ProviderPanelProps) {
  const queryClient = useQueryClient();
  const profilesQuery = useQuery(providerProfilesQueryOptions(tool));
  const [editing, setEditing] = useState<ProviderProfileDto | null>(null);
  const [form, setForm] = useState<ProviderFormState>(emptyForm);
  const [formOpen, setFormOpen] = useState(false);
  const saveInFlight = useRef(false);
  const [importPreview, setImportPreview] =
    useState<ProviderImportPreviewDto | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = async () => {
    await queryClient.invalidateQueries({
      queryKey: profileKeys.providers(tool),
    });
  };

  const saveMutation = useMutation({
    mutationFn: async () => {
      const extraEnv = parseExtraEnv(form.extraEnvText);
      const options = {
        credentialEnvKey: tool === "claude" ? form.credentialEnvKey : null,
        extraEnv: tool === "claude" ? extraEnv : {},
        wireApi: tool === "codex" && form.wireApi ? form.wireApi : null,
      };
      if (editing) {
        return unwrapResult(
          await commands.updateProviderProfile({
            id: editing.id,
            name: form.name,
            apiBaseUrl: form.apiBaseUrl,
            apiKey: form.apiKey
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
          apiBaseUrl: form.apiBaseUrl,
          apiKey: form.apiKey,
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
      setNotice("中央渠道档案已保存，原生配置尚未修改。");
    },
    onSettled: () => {
      saveInFlight.current = false;
    },
  });

  const openForm = (profile: ProviderProfileDto | null) => {
    if (saveInFlight.current || saveMutation.isPending) return;
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
    if (saveInFlight.current || saveMutation.isPending) return;
    setFormOpen(false);
    setEditing(null);
    setForm(emptyForm);
    saveMutation.reset();
  };

  const activateMutation = useMutation({
    mutationFn: async (profile: ProviderProfileDto) => {
      unwrapResult(
        await commands.setActiveProviderProfile(tool, {
          id: profile.id,
          rowVersion: profile.rowVersion,
        }),
      );
      return unwrapResult(await commands.previewProviderSync(tool));
    },
    onSuccess: onPreview,
    // 生效档案的中央写入发生在预览之前；即使预览因策略或路径状态失败，
    // 也必须刷新列表，避免 UI 继续把旧档案显示为生效。
    onSettled: refresh,
  });
  const previewMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(await commands.previewProviderSync(tool)),
    onSuccess: onPreview,
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
      setNotice("已按目标工具重新校验并创建独立渠道档案。");
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
      setNotice("中央渠道档案已删除；如需清理原生字段，请生成新的渠道预览。");
      await refresh();
    },
  });
  const discoverMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(await commands.discoverProviderImport(tool)),
    onSuccess: setImportPreview,
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
      setNotice("已有渠道已无写入接管，原生文件内容保持不变。");
      await refresh();
    },
  });

  const mutationError = [
    activateMutation.error,
    previewMutation.error,
    copyMutation.error,
    deleteMutation.error,
    discoverMutation.error,
    confirmImportMutation.error,
  ]
    .map(profileErrorText)
    .find(Boolean);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (saveInFlight.current || saveMutation.isPending) return;
    saveInFlight.current = true;
    saveMutation.mutate();
  };

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
            onClick={() => discoverMutation.mutate()}
          >
            检测已有配置
          </Button>
          <Button size="sm" onClick={() => previewMutation.mutate()}>
            预览渠道同步
          </Button>
        </div>
      </div>

      {notice ? (
        <p className="mt-4 text-sm text-emerald-700 dark:text-emerald-300">
          {notice}
        </p>
      ) : null}
      {mutationError ? (
        <p role="alert" className="mt-4 text-sm text-red-700 dark:text-red-300">
          {mutationError}
        </p>
      ) : null}

      {profilesQuery.isPending ? (
        <p role="status" className="text-muted-foreground mt-5 text-sm">
          正在加载渠道档案…
        </p>
      ) : null}
      {profilesQuery.isError ? (
        <p role="alert" className="mt-5 text-sm text-red-700 dark:text-red-300">
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
                <p className="font-medium">
                  {profile.name}{" "}
                  {profile.isActive ? <span>· 当前生效</span> : null}
                </p>
                <p className="text-muted-foreground mt-1 text-xs">
                  {profile.defaultModel} · {providerCredentialText(profile)}
                </p>
              </div>
              <div className="flex flex-wrap gap-2">
                {!profile.isActive ? (
                  <Button
                    size="sm"
                    onClick={() => activateMutation.mutate(profile)}
                  >
                    切换并预览
                  </Button>
                ) : null}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => openForm(profile)}
                >
                  编辑
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={isCodexOAuthProfile(profile)}
                  onClick={() => copyMutation.mutate(profile)}
                >
                  复制到{tool === "claude" ? " Codex" : " Claude"}
                </Button>
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
        <div className="mt-5 rounded-lg border border-amber-200 bg-amber-50 p-4 dark:border-amber-900/60 dark:bg-amber-950/40">
          <p className="font-medium">发现已有渠道，仅生成了导入预览</p>
          <p className="mt-1 text-sm break-all">{importPreview.targetPath}</p>
          <p className="text-muted-foreground mt-1 text-xs">
            {importPreview.defaultModel} ·{" "}
            {importCredentialText(tool, importPreview)}
          </p>
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
        title={`${editing ? "编辑" : "新增"} ${tool === "claude" ? "Claude" : "Codex"} 渠道`}
        description="保存只更新中央渠道档案，不会修改原生配置；原生写入仍需预览后确认 Apply。"
        submitLabel={editing ? "保存编辑" : "创建渠道"}
        pending={saveMutation.isPending}
        error={profileErrorText(saveMutation.error)}
        onClose={closeForm}
        onSubmit={submit}
      >
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
        <Field label="API 地址" id={`${tool}-provider-url`}>
          <input
            id={`${tool}-provider-url`}
            required
            type="url"
            className="field"
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
            disabled={editing ? isCodexOAuthProfile(editing) : false}
            placeholder={apiKeyPlaceholder(tool, editing)}
            value={form.apiKey}
            onChange={(event) =>
              setForm({ ...form, apiKey: event.currentTarget.value })
            }
          />
        </Field>
        <Field label="默认模型" id={`${tool}-provider-model`}>
          <input
            id={`${tool}-provider-model`}
            required
            className="field"
            value={form.defaultModel}
            onChange={(event) =>
              setForm({ ...form, defaultModel: event.currentTarget.value })
            }
          />
        </Field>
        {tool === "claude" ? (
          <>
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
            <Field label="额外 env（每行 KEY=VALUE）" id={`${tool}-extra-env`}>
              <textarea
                id={`${tool}-extra-env`}
                className="field min-h-24 resize-y font-mono text-sm"
                placeholder={
                  "ANTHROPIC_DEFAULT_OPUS_MODEL=claude-opus\nANTHROPIC_DEFAULT_SONNET_MODEL=claude-sonnet"
                }
                value={form.extraEnvText}
                onChange={(event) =>
                  setForm({ ...form, extraEnvText: event.currentTarget.value })
                }
              />
            </Field>
          </>
        ) : (
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
            </select>
          </Field>
        )}
      </FormDialog>
    </section>
  );
}

function isCodexOAuthProfile(profile: ProviderProfileDto): boolean {
  return profile.tool === "codex" && profile.options.providerId === "openai";
}

function providerCredentialText(profile: ProviderProfileDto): string {
  if (isCodexOAuthProfile(profile)) {
    return "使用 Codex OAuth 登录";
  }
  return profile.apiKeyConfigured ? "密钥已遮罩保存" : "密钥未配置";
}

function importCredentialText(
  tool: Tool,
  preview: ProviderImportPreviewDto,
): string {
  if (tool === "codex" && !preview.apiKeyConfigured) {
    return "使用 Codex OAuth 登录";
  }
  return preview.apiKeyConfigured ? "密钥已遮罩保存" : "密钥未配置";
}

function apiKeyPlaceholder(
  tool: Tool,
  editing: ProviderProfileDto | null,
): string {
  if (editing && isCodexOAuthProfile(editing)) {
    return "留空以继续使用 Codex OAuth 登录";
  }
  return editing?.apiKeyConfigured ? "留空以保留现有密钥" : "输入密钥";
}

function editProfile(
  profile: ProviderProfileDto,
  setEditing: (profile: ProviderProfileDto) => void,
  setForm: (form: ProviderFormState) => void,
) {
  setEditing(profile);
  setForm({
    name: profile.name,
    apiBaseUrl: profile.apiBaseUrl,
    apiKey: "",
    defaultModel: profile.defaultModel,
    credentialEnvKey: profile.options.credentialEnvKey ?? "ANTHROPIC_API_KEY",
    extraEnvText: Object.entries(profile.options.extraEnv)
      .map(([key, value]) => `${key}=${value}`)
      .join("\n"),
    wireApi: profile.options.wireApi ?? "",
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

function Field({
  label,
  id,
  children,
}: {
  label: string;
  id: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label htmlFor={id} className="mb-1 block text-sm font-medium">
        {label}
      </label>
      {children}
    </div>
  );
}
