import { useId, useState } from "react";

import { FormDialog } from "@/components/form-dialog";
import { Field } from "@/components/ui/field";
import { validateMcpForm, type McpFormState } from "@/features/mcp/mcp-form";

interface McpFormDialogProps {
  /** 打开时的初始草稿；组件只在挂载时读取一次，页面通过重新挂载来换草稿。 */
  initialState: McpFormState;
  directApply: boolean;
  pending: boolean;
  /** 保存 RPC 的错误文案；表单自身的校验错误由组件内部持有。 */
  saveError: string | null;
  onClose: () => void;
  /** 只在本地校验通过后调用；重复提交由页面的 useSubmitGuard 阻止。 */
  onSubmit: (state: McpFormState) => void;
}

/** MCP 新增/编辑弹窗。表单草稿与校验错误留在这里，击键不会触发整页重渲染。 */
export function McpFormDialog({
  initialState,
  directApply,
  pending,
  saveError,
  onClose,
  onSubmit,
}: McpFormDialogProps) {
  const [form, setForm] = useState<McpFormState>(initialState);
  const [formError, setFormError] = useState<string | null>(null);

  return (
    <FormDialog
      open
      title={form.id ? "编辑 MCP" : "新增 MCP"}
      description={
        directApply
          ? "保存只更新中央 MCP；已分配工具会按直接应用模式自动同步。"
          : "保存只更新中央 MCP，不会修改原生配置；原生写入仍需预览后确认 Apply。"
      }
      submitLabel="保存中央意图"
      pending={pending}
      error={formError ?? saveError}
      onClose={onClose}
      onSubmit={(event) => {
        event.preventDefault();
        setFormError(null);
        try {
          validateMcpForm(form);
        } catch (error) {
          setFormError(
            error instanceof Error ? error.message : "表单内容无效。",
          );
          return;
        }
        onSubmit(form);
      }}
    >
      <Field label="名称">
        <input
          className="field"
          value={form.name}
          onChange={(event) =>
            setForm((current) => ({
              ...current,
              name: event.target.value,
            }))
          }
          required
        />
      </Field>
      <Field label="传输方式">
        <select
          className="field"
          value={form.transport}
          onChange={(event) =>
            setForm((current) => ({
              ...current,
              transport:
                event.target.value === "streamable_http"
                  ? "streamable_http"
                  : "stdio",
            }))
          }
        >
          <option value="stdio">stdio</option>
          <option value="streamable_http">streamable_http</option>
        </select>
      </Field>
      {form.transport === "stdio" ? (
        <>
          <Field label="Command">
            <input
              className="field"
              value={form.command}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  command: event.target.value,
                }))
              }
              required
            />
          </Field>
          <Field label="Args（每行一项）">
            <textarea
              className="field min-h-24"
              value={form.args}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  args: event.target.value,
                }))
              }
            />
          </Field>
          <SensitiveField
            label="Env JSON"
            value={form.env}
            keep={form.keepEnv}
            editing={form.id !== null}
            onKeep={(keep) =>
              setForm((current) => ({ ...current, keepEnv: keep }))
            }
            onChange={(env) => setForm((current) => ({ ...current, env }))}
          />
        </>
      ) : (
        <>
          <Field label="URL">
            <input
              className="field"
              type="url"
              value={form.url}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  url: event.target.value,
                }))
              }
              required
            />
          </Field>
          <SensitiveField
            label="Headers JSON"
            value={form.headers}
            keep={form.keepHeaders}
            editing={form.id !== null}
            onKeep={(keep) =>
              setForm((current) => ({ ...current, keepHeaders: keep }))
            }
            onChange={(headers) =>
              setForm((current) => ({ ...current, headers }))
            }
          />
        </>
      )}
      <div className="space-y-2 text-sm">
        <label htmlFor="mcp-extra-json" className="block font-medium">
          扩展字段 JSON
        </label>
        {form.id ? (
          <label className="mb-2 flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={form.keepExtra}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  keepExtra: event.target.checked,
                }))
              }
            />
            保持数据库中的扩展字段（不会回填敏感原值）
          </label>
        ) : null}
        <textarea
          id="mcp-extra-json"
          className="field min-h-24 font-mono text-xs"
          value={form.extra}
          disabled={form.id !== null && form.keepExtra}
          onChange={(event) =>
            setForm((current) => ({
              ...current,
              extra: event.target.value,
            }))
          }
        />
      </div>
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={form.enabled}
          onChange={(event) =>
            setForm((current) => ({
              ...current,
              enabled: event.target.checked,
            }))
          }
        />
        启用（停用后下一份预览会安全移除已应用条目）
      </label>
    </FormDialog>
  );
}

function SensitiveField({
  label,
  value,
  keep,
  editing,
  onKeep,
  onChange,
}: {
  label: string;
  value: string;
  keep: boolean;
  editing: boolean;
  onKeep: (keep: boolean) => void;
  onChange: (value: string) => void;
}) {
  // 用 useId 关联 label 与输入框，不再按 label 文案推断 id；改文案不会破坏可访问性关联。
  const id = useId();
  return (
    <div className="space-y-2 text-sm">
      <label htmlFor={id} className="block font-medium">
        {label}
      </label>
      {editing ? (
        <label className="flex items-center gap-2 text-xs">
          <input
            type="checkbox"
            checked={keep}
            onChange={(event) => onKeep(event.target.checked)}
          />
          保持数据库中的敏感值（不会回填原值）
        </label>
      ) : null}
      <input
        id={id}
        className="field font-mono text-xs"
        type="password"
        autoComplete="off"
        value={value}
        disabled={editing && keep}
        onChange={(event) => onChange(event.target.value)}
      />
    </div>
  );
}
