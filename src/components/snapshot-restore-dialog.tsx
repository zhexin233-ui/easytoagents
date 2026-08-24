import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type RestorePreview,
  type SnapshotSummary,
} from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { snapshotsQueryOptions, syncKeys } from "@/lib/sync-api";

export function SnapshotRestoreDialog({
  open,
  onClose,
  initialSnapshotId,
}: {
  open: boolean;
  onClose: () => void;
  initialSnapshotId?: string | null;
}) {
  const queryClient = useQueryClient();
  const snapshotsQuery = useQuery({
    ...snapshotsQueryOptions(),
    enabled: open,
  });
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const previewMutation = useMutation({
    mutationFn: async (snapshot: SnapshotSummary) =>
      unwrapResult(
        await commands.previewSnapshotRestore({
          snapshotId: snapshot.snapshotId,
        }),
      ),
    onSuccess: setPreview,
  });
  const restoreMutation = useMutation({
    mutationFn: async (restore: RestorePreview) =>
      unwrapResult(
        await commands.restoreSnapshot({
          previewId: restore.previewId,
          snapshotId: restore.snapshotId,
        }),
      ),
    onSuccess: async () => {
      setPreview(null);
      await queryClient.invalidateQueries({ queryKey: syncKeys.all });
      onClose();
    },
  });
  const handleClose = () => {
    setPreview(null);
    previewMutation.reset();
    restoreMutation.reset();
    onClose();
  };
  const { dialogRef, onKeyDown } = useDialogFocus(open, handleClose);

  if (!open) {
    return null;
  }

  const snapshots = prioritizeSnapshot(snapshotsQuery.data, initialSnapshotId);
  const error = profileErrorText(
    previewMutation.error ?? restoreMutation.error ?? snapshotsQuery.error,
  );

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4">
      <section
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="snapshot-restore-title"
        aria-describedby="snapshot-restore-description"
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="max-h-[88vh] w-full max-w-3xl overflow-auto rounded-xl bg-white p-6 shadow-xl"
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="text-muted-foreground text-sm">私有恢复点</p>
            <h2
              id="snapshot-restore-title"
              className="mt-1 text-xl font-semibold"
            >
              恢复原生目标快照
            </h2>
          </div>
          <Button variant="outline" size="sm" onClick={handleClose}>
            关闭
          </Button>
        </div>
        <p
          id="snapshot-restore-description"
          className="text-muted-foreground mt-3 text-sm leading-6"
        >
          恢复前会再次创建当前状态快照并生成一次性持久化预览；此处不展示快照内容。
        </p>

        {snapshotsQuery.isPending ? (
          <p role="status" className="mt-4 text-sm">
            正在读取恢复点…
          </p>
        ) : null}
        {error ? (
          <div className="mt-4">
            <BlockingState
              title="恢复流程暂不可用"
              description={error}
              actionLabel="重新读取"
              onAction={() => void snapshotsQuery.refetch()}
            />
          </div>
        ) : null}

        {preview ? (
          <section className="mt-5 rounded-lg border p-4" aria-label="恢复预览">
            <p className="font-medium">确认恢复此目标</p>
            <code className="mt-2 block text-xs break-all">
              {preview.targetPath}
            </code>
            <p className="mt-2 text-sm">
              当前类型：{preview.currentType} · 快照类型：{preview.snapshotType}
            </p>
            <div className="mt-4 flex justify-end gap-3">
              <Button variant="outline" onClick={() => setPreview(null)}>
                返回列表
              </Button>
              <Button
                disabled={restoreMutation.isPending}
                onClick={() => restoreMutation.mutate(preview)}
              >
                {restoreMutation.isPending ? "正在恢复…" : "执行恢复"}
              </Button>
            </div>
          </section>
        ) : (
          <div className="mt-5 space-y-3">
            {snapshots?.map((snapshot) => (
              <article
                key={snapshot.snapshotId}
                className="rounded-lg border p-4"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <code className="text-xs break-all">
                      {snapshot.targetPath}
                    </code>
                    <p className="text-muted-foreground mt-2 text-xs">
                      {snapshot.createdAt} · {snapshot.targetType}
                    </p>
                  </div>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={previewMutation.isPending}
                    onClick={() => previewMutation.mutate(snapshot)}
                  >
                    预览恢复
                  </Button>
                </div>
              </article>
            ))}
            {snapshots?.length === 0 ? (
              <p className="text-muted-foreground text-sm">
                尚无快照。首次成功应用原生变更后会在这里出现恢复点。
              </p>
            ) : null}
          </div>
        )}
      </section>
    </div>
  );
}

function prioritizeSnapshot(
  snapshots: SnapshotSummary[] | undefined,
  initialSnapshotId: string | null | undefined,
) {
  if (!snapshots || !initialSnapshotId) {
    return snapshots;
  }
  let initial: SnapshotSummary | undefined;
  const remaining: SnapshotSummary[] = [];
  for (const snapshot of snapshots) {
    if (snapshot.snapshotId === initialSnapshotId) {
      initial = snapshot;
    } else {
      remaining.push(snapshot);
    }
  }
  return initial ? [initial, ...remaining] : remaining;
}
