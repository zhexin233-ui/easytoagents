import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactElement } from "react";

import {
  commands,
  type Tool,
  type UpdateAppSettingsInput,
} from "@/bindings/commands";
import { type ThemePreference } from "@/components/use-theme";
import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { appSettingsQueryOptions, settingsKeys } from "@/lib/settings-api";
import { filterEnabledTools, toolMetadata } from "@/lib/tool-metadata";
import { cn } from "@/lib/utils";

// 启用的工具固定按 claude → codex → cursor → zcode 顺序展示与提交。
const ENABLED_TOOL_ORDER = [
  "claude",
  "codex",
  "cursor",
  "zcode",
] as const satisfies readonly Tool[];

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
  themePreference: ThemePreference;
  onThemePreferenceChange: (preference: ThemePreference) => void;
}

export function SettingsDialog({
  open,
  onClose,
  themePreference,
  onThemePreferenceChange,
}: SettingsDialogProps) {
  const queryClient = useQueryClient();
  const settingsQuery = useQuery({
    ...appSettingsQueryOptions(),
    enabled: open,
  });
  const updateMutation = useMutation({
    mutationFn: async (input: UpdateAppSettingsInput) =>
      unwrapResult(await commands.updateAppSettings(input)),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: settingsKeys.all });
    },
  });

  const { dialogRef, onKeyDown } = useDialogFocus(open, onClose);

  if (!open) {
    return null;
  }

  const directApply = settingsQuery.data?.applyMode === "direct";
  const enabledTools = settingsQuery.data?.enabledTools;

  const toggleApplyMode = (nextDirect: boolean) => {
    const settings = settingsQuery.data;
    if (!settings) {
      return;
    }
    updateMutation.mutate({
      applyMode: nextDirect ? "direct" : "preview_confirm",
      enabledTools: settings.enabledTools,
    });
  };

  const toggleEnabledTool = (tool: Tool, enabled: boolean) => {
    const settings = settingsQuery.data;
    if (!settings) {
      return;
    }
    const next = new Set(settings.enabledTools);
    if (enabled) {
      next.add(tool);
    } else {
      next.delete(tool);
    }
    updateMutation.mutate({
      applyMode: settings.applyMode,
      enabledTools: filterEnabledTools(ENABLED_TOOL_ORDER, next),
    });
  };

  return (
    <div
      role="presentation"
      className="fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4"
    >
      <section
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-dialog-title"
        aria-describedby="settings-dialog-description"
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="bg-card max-h-[calc(100dvh-2rem)] w-full max-w-2xl min-w-0 overflow-auto rounded-xl shadow-xl"
      >
        <div className="flex items-start justify-between gap-4 border-b p-6">
          <div className="min-w-0">
            <p className="text-muted-foreground text-sm">应用偏好</p>
            <h2
              id="settings-dialog-title"
              className="mt-1 text-xl font-semibold"
            >
              设置
            </h2>
            <p
              id="settings-dialog-description"
              className="text-muted-foreground mt-2 text-sm"
            >
              设置立即生效并保存在本机应用数据库中。
            </p>
          </div>
          <Button type="button" variant="outline" size="sm" onClick={onClose}>
            关闭
          </Button>
        </div>

        <div className="space-y-4 p-6">
          <section
            aria-labelledby="settings-appearance-title"
            className="rounded-lg border p-4"
          >
            <h3 id="settings-appearance-title" className="font-semibold">
              外观模式
            </h3>
            <div className="mt-3">
              <ThemeToggleGroup
                preference={themePreference}
                onPreferenceChange={onThemePreferenceChange}
              />
            </div>
          </section>

          <section
            aria-labelledby="settings-apply-mode-title"
            className="rounded-lg border p-4"
          >
            <h3 id="settings-apply-mode-title" className="font-semibold">
              应用方式
            </h3>
            {settingsQuery.isPending ? (
              <p role="status" className="mt-3 text-sm">
                正在读取设置…
              </p>
            ) : null}
            {settingsQuery.isError ? (
              <p
                role="alert"
                className="mt-3 text-sm text-red-700 dark:text-red-300"
              >
                {profileErrorText(settingsQuery.error)}
              </p>
            ) : null}
            {updateMutation.isError ? (
              <p
                role="alert"
                className="mt-3 text-sm text-red-700 dark:text-red-300"
              >
                {profileErrorText(updateMutation.error)}
              </p>
            ) : null}
            {settingsQuery.data ? (
              <label className="mt-4 flex items-start gap-3 text-sm">
                <input
                  type="checkbox"
                  className="mt-1"
                  aria-label="直接应用（跳过预览确认对话框）"
                  checked={directApply}
                  disabled={updateMutation.isPending}
                  onChange={(event) => toggleApplyMode(event.target.checked)}
                />
                <span>
                  <span className="font-medium">
                    直接应用（跳过预览确认对话框）
                  </span>
                  <span className="text-muted-foreground mt-1 block leading-6">
                    开启后，各受支持工具的 MCP 与 Skills
                    全局同步和项目追加，以及 Claude/Codex 的 Provider
                    与提示词同步仍会照常生成持久化预览，但在没有冲突或错误时直接应用，不再弹出确认对话框；中央列表的分配、启停与保存、删除、导入等操作也会自动同步到目标，页面上的手动全局同步按钮随之隐藏。每次应用仍会先创建快照并可回滚。存在冲突、错误或目标受阻时仍会打开预览对话框并阻止应用。
                  </span>
                </span>
              </label>
            ) : null}
          </section>

          <section
            aria-labelledby="settings-enabled-tools-title"
            className="rounded-lg border p-4"
          >
            <h3 id="settings-enabled-tools-title" className="font-semibold">
              启用的工具
            </h3>
            <p className="text-muted-foreground mt-1 text-sm">
              关闭的工具将不再显示在顶部工具入口、中央列表与项目详情中；已有配置数据不会被删除，同步行为保持不变。
            </p>
            {enabledTools ? (
              <div className="mt-3 space-y-2">
                {ENABLED_TOOL_ORDER.map((tool) => {
                  const metadata = toolMetadata(tool);
                  return (
                    <label
                      key={tool}
                      className="flex items-center gap-3 text-sm"
                    >
                      <input
                        type="checkbox"
                        checked={enabledTools.includes(tool)}
                        disabled={updateMutation.isPending}
                        onChange={(event) =>
                          toggleEnabledTool(tool, event.target.checked)
                        }
                      />
                      <img
                        src={metadata.icon}
                        alt=""
                        aria-hidden="true"
                        draggable={false}
                        className="size-4 rounded-[4px] object-contain"
                      />
                      <span>{metadata.label}</span>
                    </label>
                  );
                })}
              </div>
            ) : null}
          </section>
        </div>
      </section>
    </div>
  );
}

const themeToggleOptions = [
  { value: "light", label: "亮色模式", Icon: SunIcon },
  { value: "dark", label: "暗色模式", Icon: MoonIcon },
  { value: "system", label: "跟随系统外观", Icon: MonitorIcon },
] as const satisfies readonly {
  value: ThemePreference;
  label: string;
  Icon: () => ReactElement;
}[];

interface ThemeToggleGroupProps {
  preference: ThemePreference;
  onPreferenceChange: (preference: ThemePreference) => void;
}

function ThemeToggleGroup({
  preference,
  onPreferenceChange,
}: ThemeToggleGroupProps) {
  return (
    <div
      role="group"
      aria-label="外观模式"
      className="inline-flex items-center gap-0.5 rounded-md border p-0.5"
    >
      {themeToggleOptions.map(({ value, label, Icon }) => {
        const selected = preference === value;
        return (
          <button
            key={value}
            type="button"
            aria-label={label}
            aria-pressed={selected}
            title={label}
            onClick={() => onPreferenceChange(value)}
            className={cn(
              "flex size-6 items-center justify-center rounded transition-colors",
              selected
                ? "bg-muted text-foreground"
                : "text-muted-foreground hover:bg-muted",
            )}
          >
            <Icon />
          </button>
        );
      })}
    </div>
  );
}

function SunIcon() {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="size-3.5"
    >
      <circle cx="8" cy="8" r="3" />
      <path d="M8 1v1.5M8 13.5V15M1 8h1.5M13.5 8H15M3.2 3.2l1.1 1.1M11.7 11.7l1.1 1.1M12.8 3.2l-1.1 1.1M4.3 11.7l-1.1 1.1" />
    </svg>
  );
}

function MoonIcon() {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="size-3.5"
    >
      <path d="M13.4 9.6A6 6 0 1 1 6.4 2.6a4.8 4.8 0 0 0 7 7Z" />
    </svg>
  );
}

function MonitorIcon() {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="size-3.5"
    >
      <rect x="1.75" y="2.75" width="12.5" height="8.5" rx="1" />
      <path d="M5.5 13.75h5M8 11.25v2.5" />
    </svg>
  );
}
