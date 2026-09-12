import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Monitor, Moon, Sun, type LucideIcon } from "lucide-react";

import {
  commands,
  type Tool,
  type UpdateAppSettingsInput,
} from "@/bindings/commands";
import { RefreshEnvironmentButton } from "@/components/refresh-environment-button";
import { type ThemePreference } from "@/components/use-theme";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { appSettingsQueryOptions, settingsKeys } from "@/lib/settings-api";
import { filterEnabledTools, toolMetadata } from "@/lib/tool-metadata";
import { cn } from "@/lib/utils";

// 启用的工具固定按 claude → codex → cursor → zcode → opencode 顺序展示与提交。
const ENABLED_TOOL_ORDER = [
  "claude",
  "codex",
  "cursor",
  "zcode",
  "opencode",
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

  const { dialogRef } = useDialogFocus(open, onClose);

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
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={onClose}
        labelledBy="settings-dialog-title"
      >
        <DialogHeader>
          <div className="min-w-0">
            <h2
              id="settings-dialog-title"
              className="text-[15px] font-semibold"
            >
              设置
            </h2>
          </div>
        </DialogHeader>

        <DialogBody className="space-y-4">
          <section aria-labelledby="settings-appearance-title">
            <h3
              id="settings-appearance-title"
              className="text-muted-foreground text-[11px] font-semibold tracking-wide uppercase"
            >
              外观模式
            </h3>
            <div className="bg-card mt-2 overflow-hidden rounded-lg border">
              <div className="flex min-h-11 items-center justify-between gap-3 px-4 py-2.5">
                <span className="text-[13px]">外观</span>
                <ThemeToggleGroup
                  preference={themePreference}
                  onPreferenceChange={onThemePreferenceChange}
                />
              </div>
            </div>
          </section>

          <section aria-labelledby="settings-apply-mode-title">
            <h3
              id="settings-apply-mode-title"
              className="text-muted-foreground text-[11px] font-semibold tracking-wide uppercase"
            >
              应用方式
            </h3>
            {settingsQuery.isPending ? (
              <p role="status" className="mt-3 text-sm">
                正在读取设置…
              </p>
            ) : null}
            {settingsQuery.isError ? (
              <p role="alert" className="text-destructive mt-3 text-sm">
                {profileErrorText(settingsQuery.error)}
              </p>
            ) : null}
            {updateMutation.isError ? (
              <p role="alert" className="text-destructive mt-3 text-sm">
                {profileErrorText(updateMutation.error)}
              </p>
            ) : null}
            {settingsQuery.data ? (
              <div className="bg-card mt-2 overflow-hidden rounded-lg border">
                <label className="hover:bg-muted/50 flex min-h-11 items-center justify-between gap-3 px-4 py-2.5">
                  <span className="min-w-0">
                    <span className="text-[13px] font-medium">
                      直接应用（跳过预览确认对话框）
                    </span>
                    <span className="text-muted-foreground mt-0.5 block text-xs">
                      无冲突时跳过确认直接应用，操作后自动同步。每次应用仍会创建快照；有冲突时仍会弹出预览。
                    </span>
                  </span>
                  <input
                    type="checkbox"
                    className="shrink-0"
                    aria-label="直接应用（跳过预览确认对话框）"
                    checked={directApply}
                    disabled={updateMutation.isPending}
                    onChange={(event) => toggleApplyMode(event.target.checked)}
                  />
                </label>
              </div>
            ) : null}
          </section>

          <section aria-labelledby="settings-tool-probe-title">
            <h3
              id="settings-tool-probe-title"
              className="text-muted-foreground text-[11px] font-semibold tracking-wide uppercase"
            >
              工具检测
            </h3>
            <div className="bg-card mt-2 overflow-hidden rounded-lg border">
              <div className="hover:bg-muted/50 flex min-h-11 items-center justify-between gap-3 px-4 py-2.5">
                <p className="text-muted-foreground text-xs">
                  安装或卸载工具后，重新检测即可更新状态。
                </p>
                <RefreshEnvironmentButton showToolList />
              </div>
            </div>
          </section>

          <section aria-labelledby="settings-enabled-tools-title">
            <h3
              id="settings-enabled-tools-title"
              className="text-muted-foreground text-[11px] font-semibold tracking-wide uppercase"
            >
              启用的工具
            </h3>
            <p className="text-muted-foreground mt-2 text-xs">
              关闭的工具不再显示；已有配置不会删除。
            </p>
            {enabledTools ? (
              <div className="bg-card mt-2 overflow-hidden rounded-lg border">
                {ENABLED_TOOL_ORDER.map((tool) => {
                  const metadata = toolMetadata(tool);
                  return (
                    <label
                      key={tool}
                      className="hover:bg-muted/50 flex min-h-11 items-center justify-between gap-3 px-4 py-2.5"
                    >
                      <span className="flex items-center gap-2 text-[13px]">
                        <img
                          src={metadata.icon}
                          alt=""
                          aria-hidden="true"
                          draggable={false}
                          className="rounded-control size-4 object-contain"
                        />
                        {metadata.label}
                      </span>
                      <input
                        type="checkbox"
                        className="shrink-0"
                        checked={enabledTools.includes(tool)}
                        disabled={updateMutation.isPending}
                        onChange={(event) =>
                          toggleEnabledTool(tool, event.target.checked)
                        }
                      />
                    </label>
                  );
                })}
              </div>
            ) : null}
          </section>
        </DialogBody>
        <DialogFooter>
          <Button type="button" onClick={onClose}>
            完成
          </Button>
        </DialogFooter>
      </DialogContent>
    </DialogOverlay>
  );
}

const themeToggleOptions = [
  { value: "light", label: "亮色模式", Icon: Sun },
  { value: "dark", label: "暗色模式", Icon: Moon },
  { value: "system", label: "跟随系统外观", Icon: Monitor },
] as const satisfies readonly {
  value: ThemePreference;
  label: string;
  Icon: LucideIcon;
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
      className="rounded-control inline-flex items-center overflow-hidden border p-0.5"
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
              "flex h-7 items-center justify-center px-2 transition-colors",
              selected
                ? "bg-muted text-foreground"
                : "text-muted-foreground hover:bg-muted",
            )}
          >
            <Icon aria-hidden="true" className="size-3.5" />
          </button>
        );
      })}
    </div>
  );
}
