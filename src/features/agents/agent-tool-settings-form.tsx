import { useMemo, useState } from "react";

import type {
  AgentToolSettingsDto,
  ClaudeAgentColor,
  ClaudeAgentSettings,
  CodexAgentSettings,
  CodexReasoningEffort,
} from "@/bindings/commands";
import { Field } from "@/components/ui/field";
import { AGENT_TOOL_SETTINGS_TOOLS, toolMetadata } from "@/lib/tool-metadata";
import { validateAgentToolSettingsDraft } from "@/features/agents/agent-tool-settings-validation";

const CLAUDE_COLORS: ClaudeAgentColor[] = [
  "red",
  "blue",
  "green",
  "yellow",
  "purple",
  "orange",
  "pink",
  "cyan",
];
const CODEX_EFFORTS: CodexReasoningEffort[] = [
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "ultra",
];
const MODEL_ALIASES = ["inherit", "sonnet", "opus", "haiku", "fable"];

function isClaudeColor(value: string): value is ClaudeAgentColor {
  return CLAUDE_COLORS.some((color) => color === value);
}

function isCodexEffort(value: string): value is CodexReasoningEffort {
  return CODEX_EFFORTS.some((effort) => effort === value);
}

interface AgentToolSettingsFormProps {
  value: AgentToolSettingsDto;
  onChange: (value: AgentToolSettingsDto) => void;
}

export function AgentToolSettingsForm({
  value,
  onChange,
}: AgentToolSettingsFormProps) {
  const [open, setOpen] = useState(false);
  const validationError = useMemo(
    () => validateAgentToolSettingsDraft(value),
    [value],
  );
  if (AGENT_TOOL_SETTINGS_TOOLS.length === 0) return null;

  return (
    <details
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary className="cursor-pointer text-sm font-medium">
        工具特有设置
      </summary>
      <div className="mt-3 space-y-4 rounded-lg border p-3">
        {AGENT_TOOL_SETTINGS_TOOLS.includes("claude") ? (
          <ClaudeSettingsPanel
            value={value.claude}
            onChange={(claude) => onChange({ ...value, claude })}
          />
        ) : null}
        {AGENT_TOOL_SETTINGS_TOOLS.includes("codex") ? (
          <CodexSettingsPanel
            value={value.codex}
            onChange={(codex) => onChange({ ...value, codex })}
          />
        ) : null}
        {validationError ? (
          <p role="alert" className="text-destructive text-xs">
            {validationError}
          </p>
        ) : null}
      </div>
    </details>
  );
}

function ClaudeSettingsPanel({
  value,
  onChange,
}: {
  value: ClaudeAgentSettings | null;
  onChange: (value: ClaudeAgentSettings | null) => void;
}) {
  const [customModelSelected, setCustomModelSelected] = useState(false);
  const settings = value ?? {};
  const model = settings.model ?? "";
  const alias =
    model === ""
      ? customModelSelected
        ? "custom"
        : ""
      : MODEL_ALIASES.includes(model)
        ? model
        : "custom";
  const tools = settings.tools?.join(", ") ?? "";
  return (
    <section
      aria-labelledby="claude-agent-settings-title"
      className="space-y-3"
    >
      <h3 id="claude-agent-settings-title" className="text-sm font-semibold">
        {toolMetadata("claude").label}
      </h3>
      <Field label="模型">
        <select
          className="field"
          aria-label="Claude 模型别名"
          value={alias}
          onChange={(event) => {
            const next = event.target.value;
            if (next === "custom") {
              setCustomModelSelected(true);
              onChange({
                ...settings,
                model: alias === "custom" ? model || null : null,
              });
            } else {
              setCustomModelSelected(false);
              onChange({ ...settings, model: next || null });
            }
          }}
        >
          <option value="">继承默认模型</option>
          {MODEL_ALIASES.map((item) => (
            <option key={item} value={item}>
              {item}
            </option>
          ))}
          <option value="custom">自定义</option>
        </select>
        {alias === "custom" ? (
          <input
            className="field mt-2"
            aria-label="Claude 自定义模型"
            value={model}
            onChange={(event) =>
              onChange({ ...settings, model: event.target.value || null })
            }
            placeholder="例如 claude-opus-5"
          />
        ) : null}
      </Field>
      <Field label="颜色">
        <select
          className="field"
          aria-label="Claude Agent 颜色"
          value={settings.color ?? ""}
          onChange={(event) =>
            onChange({
              ...settings,
              color: isClaudeColor(event.target.value)
                ? event.target.value
                : null,
            })
          }
        >
          <option value="">不设置</option>
          {CLAUDE_COLORS.map((color) => (
            <option key={color} value={color}>
              {color}
            </option>
          ))}
        </select>
      </Field>
      <Field label="可用工具（逗号分隔）">
        <input
          className="field"
          aria-label="Claude Agent 工具列表"
          value={tools}
          onChange={(event) => {
            const text = event.target.value;
            const next =
              text.trim() === ""
                ? null
                : [...new Set(text.split(",").map((item) => item.trim()))];
            onChange({ ...settings, tools: next });
          }}
          placeholder="Agent, Read, Bash"
        />
      </Field>
    </section>
  );
}

function CodexSettingsPanel({
  value,
  onChange,
}: {
  value: CodexAgentSettings | null;
  onChange: (value: CodexAgentSettings | null) => void;
}) {
  const settings = value ?? {};
  const features = settings.features ?? {};
  const featureEntries = Object.entries(features);
  return (
    <section aria-labelledby="codex-agent-settings-title" className="space-y-3">
      <h3 id="codex-agent-settings-title" className="text-sm font-semibold">
        {toolMetadata("codex").label}
      </h3>
      <Field label="模型">
        <input
          className="field"
          aria-label="Codex Agent 模型"
          value={settings.model ?? ""}
          onChange={(event) =>
            onChange({ ...settings, model: event.target.value || null })
          }
          placeholder="留空以继承默认模型"
        />
      </Field>
      <Field label="推理强度">
        <select
          className="field"
          aria-label="Codex Agent 推理强度"
          value={settings.modelReasoningEffort ?? ""}
          onChange={(event) =>
            onChange({
              ...settings,
              modelReasoningEffort: isCodexEffort(event.target.value)
                ? event.target.value
                : null,
            })
          }
        >
          <option value="">不设置</option>
          {CODEX_EFFORTS.map((effort) => (
            <option key={effort} value={effort}>
              {effort}
            </option>
          ))}
        </select>
      </Field>
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-sm">features</span>
          <button
            type="button"
            className="text-primary text-xs underline"
            onClick={() => {
              let index = featureEntries.length + 1;
              let key = `feature_${index}`;
              while (Object.hasOwn(features, key)) {
                index += 1;
                key = `feature_${index}`;
              }
              onChange({
                ...settings,
                features: { ...features, [key]: false },
              });
            }}
          >
            添加键
          </button>
        </div>
        {featureEntries.map(([key, enabled]) => (
          <div key={key} className="flex items-center gap-2">
            <input
              className="field min-w-0 flex-1"
              aria-label={`Codex feature 键 ${key}`}
              value={key}
              onChange={(event) => {
                const nextKey = event.target.value;
                const next = { ...features };
                delete next[key];
                if (nextKey) next[nextKey] = enabled;
                onChange({ ...settings, features: next });
              }}
            />
            <label className="flex items-center gap-1 text-xs">
              <input
                type="checkbox"
                checked={enabled}
                aria-label={`${key} 启用`}
                onChange={(event) =>
                  onChange({
                    ...settings,
                    features: { ...features, [key]: event.target.checked },
                  })
                }
              />
              {enabled ? "true" : "false"}
            </label>
            <button
              type="button"
              className="text-destructive text-xs"
              aria-label={`删除 Codex feature ${key}`}
              onClick={() => {
                const next = { ...features };
                delete next[key];
                onChange({ ...settings, features: next });
              }}
            >
              删除
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}
