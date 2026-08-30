import { queryOptions } from "@tanstack/react-query";

import { commands, type PreviewPlan } from "@/bindings/commands";
import { unwrapResult } from "@/lib/profile-api";

export const settingsKeys = {
  all: ["settings"] as const,
};

export function appSettingsQueryOptions() {
  return queryOptions({
    queryKey: settingsKeys.all,
    queryFn: async () => unwrapResult(await commands.getAppSettings()),
  });
}

// 与 ChangePreviewDialog 的 Apply 可用条件保持一致：有目标，且每个目标
// 既非 conflict 也没有错误码。存在警告时仍允许直接应用，与对话框行为相同。
export function canAutoApplyPreview(plan: PreviewPlan): boolean {
  return (
    plan.targets.length > 0 &&
    plan.targets.every(
      (target) => target.changeKind !== "conflict" && target.errorCode === null,
    )
  );
}
