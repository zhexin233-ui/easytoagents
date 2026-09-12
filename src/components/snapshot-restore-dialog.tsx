import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type RestorePreview,
  type SnapshotSummary,
} from "@/bindings/commands";
import { BlockingState } from "@/components/blocking-state";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogContent,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { dashboardKeys } from "@/lib/dashboard-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { snapshotsQueryOptions, syncKeys } from "@/lib/sync-api";

interface DeleteSummary {
  deleted: number;
  failed: number;
}

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
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [deleteConfirm, setDeleteConfirm] = useState<string[] | null>(null);
  const [deleteSummary, setDeleteSummary] = useState<DeleteSummary | null>(
    null,
  );
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
  const deleteMutation = useMutation({
    mutationFn: async (ids: string[]) =>
      unwrapResult(await commands.deleteSnapshots({ snapshotIds: ids })),
    onSuccess: async (result) => {
      setDeleteSummary({
        deleted: result.deletedIds.length,
        failed: result.failures.length,
      });
      setSelectedIds(new Set());
      setDeleteConfirm(null);
      await queryClient.invalidateQueries({ queryKey: syncKeys.all });
      await queryClient.invalidateQueries({ queryKey: dashboardKeys.all });
    },
  });
  const handleClose = () => {
    setPreview(null);
    previewMutation.reset();
    restoreMutation.reset();
    deleteMutation.reset();
    setSelectedIds(new Set());
    setDeleteConfirm(null);
    setDeleteSummary(null);
    onClose();
  };
  const { dialogRef } = useDialogFocus(open, handleClose);

  if (!open) {
    return null;
  }

  const snapshots = prioritizeSnapshot(snapshotsQuery.data, initialSnapshotId);
  // 选择集合按当前列表求交集：列表变化后过期的 ID 不参与计数与删除。
  const selectableIds = new Set(
    snapshots?.map((snapshot) => snapshot.snapshotId) ?? [],
  );
  const selectedCount = [...selectedIds].filter((id) =>
    selectableIds.has(id),
  ).length;
  const deleteSummaryText = deleteSummary
    ? `已删除 ${deleteSummary.deleted} 个恢复点${
        deleteSummary.failed > 0 ? `，${deleteSummary.failed} 个删除失败` : ""
      }。`
    : null;
  const error = profileErrorText(
    previewMutation.error ??
      restoreMutation.error ??
      deleteMutation.error ??
      snapshotsQuery.error,
  );

  const toggleSelected = (snapshotId: string) => {
    setSelectedIds((current) => {
      const next = new Set([...current].filter((id) => selectableIds.has(id)));
      if (next.has(snapshotId)) {
        next.delete(snapshotId);
      } else {
        next.add(snapshotId);
      }
      return next;
    });
  };
  const requestDeleteSelected = () => {
    if (!snapshots) {
      return;
    }
    setDeleteSummary(null);
    setDeleteConfirm(
      snapshots
        .map((snapshot) => snapshot.snapshotId)
        .filter((id) => selectedIds.has(id)),
    );
  };
  const requestDeleteAll = () => {
    if (!snapshots) {
      return;
    }
    setDeleteSummary(null);
    setDeleteConfirm(snapshots.map((snapshot) => snapshot.snapshotId));
  };

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={handleClose}
        labelledBy="snapshot-restore-title"
        describedBy="snapshot-restore-description"
        size="lg"
      >
        <DialogHeader>
          <h2 id="snapshot-restore-title" className="text-[15px] font-semibold">
            恢复原生目标快照
          </h2>
        </DialogHeader>
        <DialogBody className="space-y-4">
          <p
            id="snapshot-restore-description"
            className="text-muted-foreground leading-6"
          >
            恢复前会自动创建当前状态快照。
          </p>

          {snapshotsQuery.isPending ? (
            <p role="status">正在读取恢复点…</p>
          ) : null}
          {error ? (
            <BlockingState
              title="恢复流程暂不可用"
              description={error}
              actionLabel="重新读取"
              onAction={() => void snapshotsQuery.refetch()}
            />
          ) : null}

          {preview ? (
            <section className="rounded-lg border p-4" aria-label="恢复预览">
              <p className="font-medium">确认恢复此目标</p>
              <code className="mt-2 block text-xs break-all">
                {preview.targetPath}
              </code>
              <p className="mt-2 text-sm">
                当前类型：{preview.currentType} · 快照类型：
                {preview.snapshotType}
              </p>
              <p className="text-muted-foreground mt-2 text-sm">
                存储方式：{snapshotStorageLabel(preview.storageKind)}
              </p>
              {preview.storageKind === "directory_tree" ? (
                <p className="text-warning mt-2 text-sm">
                  该恢复会重新放回完整目录树。恢复后此 Skill
                  不再指向中央副本，后续同步会把它识别为外部拥有变更。
                </p>
              ) : null}
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
          ) : deleteConfirm ? (
            <section className="rounded-lg border p-4" aria-label="删除确认">
              <p className="font-medium">确认删除恢复点</p>
              <p className="mt-2 text-sm">
                {`将永久删除 ${deleteConfirm.length} 个恢复点，删除后无法再回滚到这些快照。`}
              </p>
              <div className="mt-4 flex justify-end gap-3">
                <Button
                  variant="outline"
                  disabled={deleteMutation.isPending}
                  onClick={() => setDeleteConfirm(null)}
                >
                  取消
                </Button>
                <Button
                  disabled={deleteMutation.isPending}
                  onClick={() => deleteMutation.mutate(deleteConfirm)}
                >
                  {deleteMutation.isPending ? "正在删除…" : "确认删除"}
                </Button>
              </div>
            </section>
          ) : (
            <div className="divide-y">
              {deleteSummary ? <p role="status">{deleteSummaryText}</p> : null}
              {snapshots && snapshots.length > 0 ? (
                <div className="flex justify-end gap-3 py-2.5">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={selectedCount === 0 || deleteMutation.isPending}
                    onClick={requestDeleteSelected}
                  >
                    删除选中 ({selectedCount})
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={deleteMutation.isPending}
                    onClick={requestDeleteAll}
                  >
                    全部删除
                  </Button>
                </div>
              ) : null}
              {snapshots?.map((snapshot) => (
                <article
                  key={snapshot.snapshotId}
                  className="hover:bg-muted/50 px-1 py-2.5"
                >
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="flex min-w-0 items-start gap-3">
                      <input
                        type="checkbox"
                        className="mt-0.5"
                        aria-label={`选择 ${snapshot.targetPath}`}
                        checked={selectedIds.has(snapshot.snapshotId)}
                        disabled={deleteMutation.isPending}
                        onChange={() => toggleSelected(snapshot.snapshotId)}
                      />
                      <div className="min-w-0">
                        <code className="text-xs break-all">
                          {snapshot.targetPath}
                        </code>
                        <p className="text-muted-foreground mt-2 text-xs">
                          {snapshot.createdAt} · {snapshot.targetType} ·{" "}
                          {snapshotStorageLabel(snapshot.storageKind)}
                        </p>
                        {!snapshot.restorable ? (
                          <p className="text-warning mt-2 text-xs">
                            旧目录占位快照不含目录内容，只能删除，不能恢复。
                          </p>
                        ) : null}
                      </div>
                    </div>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={
                        previewMutation.isPending || !snapshot.restorable
                      }
                      onClick={() => previewMutation.mutate(snapshot)}
                    >
                      {snapshot.restorable ? "预览恢复" : "不可恢复"}
                    </Button>
                  </div>
                </article>
              ))}
              {snapshots?.length === 0 ? (
                <p className="text-muted-foreground">尚无恢复点。</p>
              ) : null}
            </div>
          )}
        </DialogBody>
      </DialogContent>
    </DialogOverlay>
  );
}

function snapshotStorageLabel(storageKind: SnapshotSummary["storageKind"]) {
  switch (storageKind) {
    case "payload_file":
      return "完整文件";
    case "directory_tree":
      return "完整目录树";
    case "metadata_only":
      return "仅元数据";
  }
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
