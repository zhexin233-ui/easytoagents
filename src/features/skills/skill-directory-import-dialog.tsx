import { useId, useRef, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";

import { commands, type SkillDto } from "@/bindings/commands";
import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";

interface SkillDirectoryImportDialogProps {
  onClose: () => void;
  onImported: (skill: SkillDto) => Promise<void>;
}

export function SkillDirectoryImportDialog(
  props: SkillDirectoryImportDialogProps,
) {
  const titleId = useId();
  const descriptionId = useId();
  const [sourcePath, setSourcePath] = useState("");
  const [directoryError, setDirectoryError] = useState<string | null>(null);
  const importInFlight = useRef(false);
  const importMutation = useMutation({
    mutationFn: async (path: string) =>
      unwrapResult(await commands.importSkill({ sourcePath: path })),
    retry: false,
    onSuccess: async (skill) => {
      setSourcePath("");
      await props.onImported(skill);
      props.onClose();
    },
    onSettled: () => {
      importInFlight.current = false;
    },
  });
  const close = () => {
    if (!importInFlight.current) props.onClose();
  };
  const { dialogRef, onKeyDown } = useDialogFocus(true, close);

  const selectDirectory = () => {
    setDirectoryError(null);
    void open({
      directory: true,
      multiple: false,
      title: "选择包含 SKILL.md 的目录",
    })
      .then((selected) => {
        if (typeof selected === "string") {
          setSourcePath(selected);
        }
      })
      .catch((error: unknown) => {
        setDirectoryError(`选择目录失败：${profileErrorText(error)}`);
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
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="bg-card flex max-h-[calc(100dvh-2rem)] w-full max-w-2xl min-w-0 flex-col overflow-hidden rounded-xl shadow-xl"
      >
        <div className="flex shrink-0 items-start justify-between gap-4 border-b p-6">
          <div className="min-w-0">
            <h2 id={titleId} className="text-xl font-semibold">
              从本地目录导入
            </h2>
            <p
              id={descriptionId}
              className="text-muted-foreground mt-2 text-sm"
            >
              目录必须包含合法 SKILL.md
              frontmatter。循环、断裂、逃逸链接和特殊文件会被拒绝。
            </p>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={importMutation.isPending}
            onClick={close}
            aria-label="关闭本地目录导入"
          >
            关闭
          </Button>
        </div>
        <form
          aria-labelledby={titleId}
          className="flex min-h-0 flex-col"
          onSubmit={(event) => {
            event.preventDefault();
            if (importInFlight.current || !sourcePath) {
              return;
            }
            // 提交按钮即将禁用，先保留弹窗焦点，避免浏览器将焦点移回页面。
            dialogRef.current?.focus();
            importInFlight.current = true;
            importMutation.mutate(sourcePath);
          }}
        >
          <div className="min-h-0 space-y-4 overflow-y-auto p-6">
            <label
              htmlFor="skill-source-path"
              className="block text-sm font-medium"
            >
              已选择目录
            </label>
            <input
              id="skill-source-path"
              className="field"
              value={sourcePath}
              readOnly
              placeholder="尚未选择"
            />
            <Button
              type="button"
              variant="outline"
              disabled={importMutation.isPending}
              onClick={selectDirectory}
            >
              选择目录
            </Button>
            {directoryError ? (
              <p
                role="alert"
                className="text-sm text-red-700 dark:text-red-300"
              >
                {directoryError}
              </p>
            ) : null}
            {importMutation.isError ? (
              <p
                role="alert"
                className="text-sm text-red-700 dark:text-red-300"
              >
                {profileErrorText(importMutation.error)}
              </p>
            ) : null}
            {importMutation.isPending ? (
              <p role="status" className="text-sm">
                正在安全导入…
              </p>
            ) : null}
          </div>
          <div className="flex shrink-0 flex-wrap justify-end gap-3 border-t px-6 py-4">
            <Button
              type="button"
              variant="outline"
              disabled={importMutation.isPending}
              onClick={close}
            >
              取消
            </Button>
            <Button
              type="submit"
              disabled={!sourcePath || importMutation.isPending}
            >
              {importMutation.isPending ? "正在安全导入…" : "复制到中央库"}
            </Button>
          </div>
        </form>
      </section>
    </div>
  );
}
