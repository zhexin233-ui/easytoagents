import { useRef, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type PreviewPlan,
  type PromptImportPreviewDto,
  type PromptProfileDto,
  type Tool,
} from "@/bindings/commands";
import { FormDialog } from "@/components/form-dialog";
import { Button } from "@/components/ui/button";
import {
  profileErrorText,
  profileKeys,
  promptProfilesQueryOptions,
  unwrapResult,
} from "@/lib/profile-api";

interface PromptPanelProps {
  tool: Tool;
  onPreview: (preview: PreviewPlan) => void;
}

export function PromptPanel({ tool, onPreview }: PromptPanelProps) {
  const queryClient = useQueryClient();
  const profilesQuery = useQuery(promptProfilesQueryOptions(tool));
  const [editing, setEditing] = useState<PromptProfileDto | null>(null);
  const [name, setName] = useState("");
  const [body, setBody] = useState("");
  const [formOpen, setFormOpen] = useState(false);
  const saveInFlight = useRef(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [importPreview, setImportPreview] =
    useState<PromptImportPreviewDto | null>(null);

  const refresh = async () => {
    await queryClient.invalidateQueries({
      queryKey: profileKeys.prompts(tool),
    });
  };
  const saveMutation = useMutation({
    mutationFn: async () =>
      editing
        ? unwrapResult(
            await commands.updatePromptProfile({
              id: editing.id,
              name,
              body,
              rowVersion: editing.rowVersion,
            }),
          )
        : unwrapResult(
            await commands.createPromptProfile({
              tool,
              name,
              body,
              activate: (profilesQuery.data?.length ?? 0) === 0,
            }),
          ),
    onSuccess: async () => {
      await refresh();
      setEditing(null);
      setName("");
      setBody("");
      setFormOpen(false);
      setNotice("中央提示词档案已保存，原生文件尚未修改。");
    },
    onSettled: () => {
      saveInFlight.current = false;
    },
  });

  const openForm = (profile: PromptProfileDto | null) => {
    if (saveInFlight.current || saveMutation.isPending) return;
    saveMutation.reset();
    setEditing(profile);
    setName(profile?.name ?? "");
    setBody(profile?.body ?? "");
    setFormOpen(true);
  };

  const closeForm = () => {
    if (saveInFlight.current || saveMutation.isPending) return;
    setFormOpen(false);
    setEditing(null);
    setName("");
    setBody("");
    saveMutation.reset();
  };

  const activateMutation = useMutation({
    mutationFn: async (profile: PromptProfileDto) => {
      unwrapResult(
        await commands.setActivePromptProfile(tool, {
          id: profile.id,
          rowVersion: profile.rowVersion,
        }),
      );
      return unwrapResult(await commands.previewPromptSync(tool));
    },
    onSuccess: onPreview,
    // 生效档案的中央写入发生在预览之前；预览失败不应让查询缓存停留在旧状态。
    onSettled: refresh,
  });
  const previewMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(await commands.previewPromptSync(tool)),
    onSuccess: onPreview,
  });
  const deleteMutation = useMutation({
    mutationFn: async (profile: PromptProfileDto) =>
      unwrapResult(
        await commands.deletePromptProfile({
          id: profile.id,
          rowVersion: profile.rowVersion,
        }),
      ),
    onSuccess: async () => {
      setNotice("中央提示词已删除；生成新预览后才会清理已接管文件。");
      await refresh();
    },
  });
  const discoverMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(await commands.discoverPromptImport(tool)),
    onSuccess: setImportPreview,
  });
  const confirmImportMutation = useMutation({
    mutationFn: async () => {
      if (!importPreview) {
        throw new Error("导入预览已关闭");
      }
      return unwrapResult(
        await commands.confirmPromptImport({
          previewId: importPreview.previewId,
          name: importPreview.suggestedName,
        }),
      );
    },
    onSuccess: async () => {
      setImportPreview(null);
      setNotice("已有提示词已无损导入，原生文件保持不变。");
      await refresh();
    },
  });
  const mutationError = [
    activateMutation.error,
    previewMutation.error,
    deleteMutation.error,
    discoverMutation.error,
    confirmImportMutation.error,
  ]
    .map(profileErrorText)
    .find(Boolean);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (saveInFlight.current || saveMutation.isPending) return;
    saveInFlight.current = true;
    saveMutation.mutate();
  };

  return (
    <section
      aria-labelledby={`${tool}-prompts-title`}
      className="rounded-xl border bg-white p-5"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 id={`${tool}-prompts-title`} className="text-xl font-semibold">
            全局提示词
          </h2>
          <p className="text-muted-foreground mt-1 text-sm">
            Markdown 正文原样写入工具的全局指令文件。
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={() => openForm(null)}>
            新增提示词
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => discoverMutation.mutate()}
          >
            检测已有提示词
          </Button>
          <Button size="sm" onClick={() => previewMutation.mutate()}>
            预览提示词同步
          </Button>
        </div>
      </div>

      {notice ? (
        <p className="mt-4 text-sm text-emerald-700">{notice}</p>
      ) : null}
      {mutationError ? (
        <p role="alert" className="mt-4 text-sm text-red-700">
          {mutationError}
        </p>
      ) : null}

      {profilesQuery.isPending ? (
        <p role="status" className="text-muted-foreground mt-5 text-sm">
          正在加载提示词档案…
        </p>
      ) : null}
      {profilesQuery.isError ? (
        <p role="alert" className="mt-5 text-sm text-red-700">
          {profileErrorText(profilesQuery.error)}
        </p>
      ) : null}
      {profilesQuery.data?.length === 0 ? (
        <p className="text-muted-foreground mt-5 rounded-lg border border-dashed p-4 text-sm">
          尚无提示词档案。点击“新增提示词”创建第一份档案，或先检测已有提示词。
        </p>
      ) : null}

      <ul className="mt-5 space-y-3">
        {profilesQuery.data?.map((profile) => (
          <li key={profile.id} className="rounded-lg border p-3">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <p className="font-medium">
                  {profile.name}{" "}
                  {profile.isActive ? <span>· 当前生效</span> : null}
                </p>
                <p className="text-muted-foreground mt-1 line-clamp-2 text-xs">
                  {profile.body}
                </p>
              </div>
              <div className="flex gap-2">
                {!profile.isActive ? (
                  <Button
                    size="sm"
                    onClick={() => activateMutation.mutate(profile)}
                  >
                    切换并预览
                  </Button>
                ) : null}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => openForm(profile)}
                >
                  编辑
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    if (
                      globalThis.confirm(
                        "删除中央提示词档案？原生文件不会在此步骤修改。",
                      )
                    ) {
                      deleteMutation.mutate(profile);
                    }
                  }}
                >
                  删除
                </Button>
              </div>
            </div>
          </li>
        ))}
      </ul>

      {importPreview ? (
        <div className="mt-5 rounded-lg border border-amber-200 bg-amber-50 p-4">
          <p className="font-medium">发现已有提示词，仅生成了无写入导入预览</p>
          <p className="mt-1 text-sm break-all">{importPreview.targetPath}</p>
          <pre className="mt-3 max-h-40 overflow-auto rounded bg-white p-3 text-xs">
            {importPreview.body}
          </pre>
          <div className="mt-3 flex gap-2">
            <Button size="sm" onClick={() => confirmImportMutation.mutate()}>
              确认无损导入
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setImportPreview(null)}
            >
              跳过
            </Button>
          </div>
        </div>
      ) : null}

      <FormDialog
        open={formOpen}
        title={`${editing ? "编辑" : "新增"} ${tool === "claude" ? "Claude" : "Codex"} 提示词`}
        description="保存只更新中央提示词档案，不会修改原生文件；原生写入仍需预览后确认 Apply。"
        submitLabel={editing ? "保存编辑" : "创建提示词"}
        pending={saveMutation.isPending}
        error={profileErrorText(saveMutation.error)}
        onClose={closeForm}
        onSubmit={submit}
      >
        <div>
          <label
            htmlFor={`${tool}-prompt-name`}
            className="mb-1 block text-sm font-medium"
          >
            名称
          </label>
          <input
            id={`${tool}-prompt-name`}
            required
            className="field"
            value={name}
            onChange={(event) => setName(event.currentTarget.value)}
          />
        </div>
        <div>
          <label
            htmlFor={`${tool}-prompt-body`}
            className="mb-1 block text-sm font-medium"
          >
            Markdown 正文
          </label>
          <textarea
            id={`${tool}-prompt-body`}
            required
            className="field min-h-44 resize-y font-mono text-sm"
            value={body}
            onChange={(event) => setBody(event.currentTarget.value)}
          />
        </div>
      </FormDialog>
    </section>
  );
}
