import { useId, useState } from "react";
import { useMutation } from "@tanstack/react-query";

import { commands, type SkillDto } from "@/bindings/commands";
import { Button } from "@/components/ui/button";
import {
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { useSubmitGuard } from "@/hooks/use-submit-guard";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";

interface SkillGithubImportDialogProps {
  onClose: () => void;
  onImported: (skill: SkillDto) => Promise<void>;
}

export function SkillGithubImportDialog(props: SkillGithubImportDialogProps) {
  const titleId = useId();
  const descriptionId = useId();
  const [url, setUrl] = useState("");
  const [committed, setCommitted] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const importGuard = useSubmitGuard();
  const importMutation = useMutation({
    mutationFn: async (githubUrl: string) =>
      unwrapResult(await commands.importGithubSkill({ url: githubUrl })),
    retry: false,
    onSuccess: async (skill) => {
      // RPC 已成功后令牌不再可重放；即使刷新失败，也必须锁住再次导入。
      setCommitted(true);
      try {
        await props.onImported(skill);
        props.onClose();
      } catch (error: unknown) {
        setRefreshError(profileErrorText(error));
      }
    },
    onSettled: () => {
      importGuard.end();
    },
  });
  const close = () => {
    if (!importGuard.isInFlight()) props.onClose();
  };
  const { dialogRef } = useDialogFocus(true, close);
  const normalizedInput = url.trim();

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={close}
        labelledBy={titleId}
        describedBy={descriptionId}
        className="flex max-h-[calc(100dvh-2rem)] max-w-2xl min-w-0 flex-col overflow-hidden p-0"
      >
        <DialogHeader className="shrink-0 border-b p-6">
          <div className="min-w-0">
            <h2 id={titleId} className="text-xl font-semibold">
              从 GitHub 导入
            </h2>
            <p
              id={descriptionId}
              className="text-muted-foreground mt-2 text-sm leading-6"
            >
              支持公开仓库的单个 Skill
              目录链接。下载会固定到一次解析的提交，只复制到应用私有中央库，不执行脚本，也不会自动分配或同步。
            </p>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={importMutation.isPending}
            onClick={close}
            aria-label="关闭 GitHub 导入"
          >
            关闭
          </Button>
        </DialogHeader>
        <form
          aria-labelledby={titleId}
          className="flex min-h-0 flex-col"
          onSubmit={(event) => {
            event.preventDefault();
            if (committed || normalizedInput.length === 0) return;
            if (!importGuard.begin()) return;
            dialogRef.current?.focus();
            setRefreshError(null);
            importMutation.mutate(normalizedInput);
          }}
        >
          <div className="min-h-0 space-y-4 overflow-y-auto p-6">
            <label
              htmlFor="skill-github-url"
              className="block text-sm font-medium"
            >
              GitHub Skill 目录链接
            </label>
            <input
              id="skill-github-url"
              type="text"
              inputMode="url"
              className="field"
              value={url}
              disabled={importMutation.isPending || committed}
              placeholder="https://github.com/owner/repo/tree/main/path/to/skill"
              autoComplete="off"
              onChange={(event) => {
                setUrl(event.target.value);
                importMutation.reset();
              }}
            />
            {importMutation.isError && !committed ? (
              <p role="alert" className="text-destructive text-sm">
                {profileErrorText(importMutation.error)}
              </p>
            ) : null}
            {importMutation.isPending ? (
              <p role="status" className="text-sm">
                正在从 GitHub 下载并安全导入…
              </p>
            ) : null}
            {committed && refreshError ? (
              <p role="alert" className="text-warning text-sm">
                Skill 已复制到中央库，但列表刷新失败：{refreshError}
                。请关闭后刷新页面查看；为避免重复导入，本次链接不能再次提交。
              </p>
            ) : null}
          </div>
          <DialogFooter className="shrink-0 border-t px-6 py-4">
            <Button
              type="button"
              variant="outline"
              disabled={importMutation.isPending}
              onClick={close}
            >
              {committed ? "关闭" : "取消"}
            </Button>
            <Button
              type="submit"
              disabled={
                normalizedInput.length === 0 ||
                importMutation.isPending ||
                committed
              }
            >
              {importMutation.isPending ? "正在下载并导入…" : "复制到中央库"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </DialogOverlay>
  );
}
