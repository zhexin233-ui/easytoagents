import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { commands, type ApplyMode } from "@/bindings/commands";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { appSettingsQueryOptions, settingsKeys } from "@/lib/settings-api";

export function SettingsPage() {
  const queryClient = useQueryClient();
  const settingsQuery = useQuery(appSettingsQueryOptions());
  const updateMutation = useMutation({
    mutationFn: async (applyMode: ApplyMode) =>
      unwrapResult(await commands.updateAppSettings({ applyMode })),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: settingsKeys.all });
    },
  });

  const directApply = settingsQuery.data?.applyMode === "direct";

  return (
    <main className="p-6 lg:p-8">
      <header className="mx-auto max-w-6xl">
        <p className="text-muted-foreground text-sm">应用偏好</p>
        <h1 className="mt-1 text-2xl font-semibold">设置</h1>
        <p className="text-muted-foreground mt-2 max-w-3xl text-sm leading-6">
          设置立即生效并保存在本机应用数据库中。
        </p>
      </header>

      <div className="mx-auto mt-6 max-w-6xl">
        <section
          className="bg-card rounded-xl border p-5"
          aria-labelledby="settings-apply-mode-title"
        >
          <h2 id="settings-apply-mode-title" className="text-lg font-semibold">
            应用方式
          </h2>
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
                onChange={(event) =>
                  updateMutation.mutate(
                    event.target.checked ? "direct" : "preview_confirm",
                  )
                }
              />
              <span>
                <span className="font-medium">
                  直接应用（跳过预览确认对话框）
                </span>
                <span className="text-muted-foreground mt-1 block leading-6">
                  开启后，MCP 与 Skills 的全局同步和项目追加、Claude/Codex 的
                  Provider
                  与提示词同步仍会照常生成持久化预览，但在没有冲突或错误时直接应用，不再弹出确认对话框；中央列表的分配与启停操作也会自动同步到目标。每次应用仍会先创建快照并可回滚。存在冲突、错误或目标受阻时仍会打开预览对话框并阻止应用。
                </span>
              </span>
            </label>
          ) : null}
        </section>
      </div>
    </main>
  );
}
