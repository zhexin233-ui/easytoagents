import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";

import {
  commands,
  type ApplySkillPreviewInput,
  type PreviewPlan,
  type SetProjectSkillAssignmentInput,
  type SkillContentPreviewDto,
  type SkillDto,
  type Tool,
} from "@/bindings/commands";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import { SyncStatusBadge } from "@/components/sync-status-badge";
import { Button } from "@/components/ui/button";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import {
  globalSkillStatusesQueryOptions,
  skillKeys,
  skillProjectOptionsQueryOptions,
  skillProjectsQueryOptions,
  skillsQueryOptions,
} from "@/lib/skills-api";

interface OpenSkillPreview {
  plan: PreviewPlan;
  tool: Tool;
  projectId: string | null;
}

export function SkillsPage() {
  const queryClient = useQueryClient();
  const skillsQuery = useQuery(skillsQueryOptions());
  const projectsQuery = useQuery(skillProjectsQueryOptions());
  const statusesQuery = useQuery(globalSkillStatusesQueryOptions());
  const [sourcePath, setSourcePath] = useState("");
  const [directoryError, setDirectoryError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [projectId, setProjectId] = useState("");
  const [projectTool, setProjectTool] = useState<Tool>("claude");
  const [contentPreview, setContentPreview] =
    useState<SkillContentPreviewDto | null>(null);
  const [openPreview, setOpenPreview] = useState<OpenSkillPreview | null>(null);
  const closeContentPreview = () => setContentPreview(null);
  const { dialogRef: contentDialogRef, onKeyDown: onContentDialogKeyDown } =
    useDialogFocus(contentPreview !== null, closeContentPreview);
  const projectOptionsQuery = useQuery(
    skillProjectOptionsQueryOptions(projectId, projectTool),
  );
  const selectedProject = useMemo(
    () => projectsQuery.data?.find((project) => project.id === projectId),
    [projectId, projectsQuery.data],
  );
  const projectTrustBlocked =
    projectId.length > 0 &&
    projectTool === "codex" &&
    selectedProject?.codexTrustStatus !== "trusted";

  const invalidateSkills = async () => {
    await queryClient.invalidateQueries({ queryKey: skillKeys.all });
  };

  const importMutation = useMutation({
    mutationFn: async (path: string) =>
      unwrapResult(await commands.importSkill({ sourcePath: path })),
    onSuccess: async () => {
      setSourcePath("");
      setMessage(
        "Skill 已复制到应用私有中央库；来源目录未修改，原生目标也尚未写入。",
      );
      await invalidateSkills();
    },
  });

  const deleteMutation = useMutation({
    mutationFn: async (skill: SkillDto) =>
      unwrapResult(
        await commands.deleteSkill({
          id: skill.id,
          rowVersion: skill.rowVersion,
        }),
      ),
    onSuccess: async () => {
      setMessage("Skill 已安全移出中央库，来源目录保持不变。");
      await invalidateSkills();
    },
  });

  const contentMutation = useMutation({
    mutationFn: async (id: string) =>
      unwrapResult(await commands.previewSkillContent(id)),
    onSuccess: setContentPreview,
  });

  const globalAssignmentMutation = useMutation({
    mutationFn: async ({ skill, tool }: { skill: SkillDto; tool: Tool }) =>
      unwrapResult(
        await commands.setGlobalSkillAssignment({
          tool,
          skillId: skill.id,
          assigned: !skill.globalTools.includes(tool),
          rowVersion: skill.rowVersion,
        }),
      ),
    onSuccess: invalidateSkills,
  });

  const projectAssignmentMutation = useMutation({
    mutationFn: async (input: SetProjectSkillAssignmentInput) =>
      unwrapResult(await commands.setProjectSkillAssignment(input)),
    onSuccess: invalidateSkills,
  });

  const previewMutation = useMutation({
    mutationFn: async ({
      tool,
      targetProjectId,
    }: {
      tool: Tool;
      targetProjectId: string | null;
    }) => ({
      tool,
      projectId: targetProjectId,
      plan: unwrapResult(
        await commands.previewSkillSync({
          tool,
          projectId: targetProjectId,
          excludeFromGit: false,
        }),
      ),
    }),
    onSuccess: ({ plan, tool, projectId: previewProjectId }) => {
      if (plan.targets.length === 0) {
        setMessage(
          previewProjectId
            ? "该项目只有全局继承项，无需创建项目 Skill 链接。"
            : "当前工具没有需要同步的全局 Skill。",
        );
        setOpenPreview(null);
        return;
      }
      setOpenPreview({ plan, tool, projectId: previewProjectId });
    },
  });

  const applyMutation = useMutation({
    mutationFn: async (input: ApplySkillPreviewInput) =>
      unwrapResult(await commands.applySkillPreview(input)),
    onSuccess: async (result) => {
      setMessage(
        `已应用 ${result.appliedTargets} 个 Skills 目标，并创建 ${result.snapshotCount} 份快照。`,
      );
      setOpenPreview(null);
      await invalidateSkills();
    },
  });

  const operationError = [
    globalAssignmentMutation.error,
    projectAssignmentMutation.error,
    previewMutation.error,
    applyMutation.error,
  ]
    .map(profileErrorText)
    .find(Boolean);

  return (
    <main className="p-5 sm:p-8">
      <header className="mx-auto max-w-6xl">
        <p className="text-muted-foreground text-sm">应用私有中央库</p>
        <h1 className="mt-1 text-3xl font-semibold">Skills</h1>
        <p className="text-muted-foreground mt-2 max-w-3xl text-sm leading-6">
          导入只复制本地目录，不移动或修改来源。Claude/Codex
          目标始终使用指向中央副本的符号链接，并且只能通过持久化预览 Apply。
        </p>
      </header>

      <div className="mx-auto mt-6 max-w-6xl space-y-4" aria-live="polite">
        {message ? (
          <p className="rounded-lg border border-emerald-200 bg-emerald-50 p-4 text-sm">
            {message}
          </p>
        ) : null}
        {operationError ? (
          <p
            role="alert"
            className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm"
          >
            {operationError}
          </p>
        ) : null}
      </div>

      <div className="mx-auto mt-6 grid max-w-6xl gap-6 xl:grid-cols-[0.8fr_1.2fr]">
        <section
          className="rounded-xl border bg-white p-5"
          aria-labelledby="skill-import-title"
        >
          <h2 id="skill-import-title" className="text-lg font-semibold">
            从本地目录导入
          </h2>
          <p className="text-muted-foreground mt-2 text-sm leading-6">
            目录必须包含合法 SKILL.md
            frontmatter。循环、断裂、逃逸链接和特殊文件会被拒绝。
          </p>
          <label
            htmlFor="skill-source-path"
            className="mt-4 block text-sm font-medium"
          >
            已选择目录
          </label>
          <input
            id="skill-source-path"
            className="field mt-2"
            value={sourcePath}
            readOnly
            placeholder="尚未选择"
          />
          <div className="mt-3 flex flex-wrap gap-3">
            <Button
              type="button"
              variant="outline"
              onClick={() => {
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
                    setDirectoryError(
                      `选择目录失败：${profileErrorText(error)}`,
                    );
                  });
              }}
            >
              选择目录
            </Button>
            <Button
              type="button"
              disabled={!sourcePath || importMutation.isPending}
              onClick={() => importMutation.mutate(sourcePath)}
            >
              {importMutation.isPending ? "正在安全导入…" : "复制到中央库"}
            </Button>
          </div>
          {directoryError ? (
            <p role="alert" className="mt-3 text-sm text-red-700">
              {directoryError}
            </p>
          ) : null}
          {importMutation.isError ? (
            <p role="alert" className="mt-3 text-sm text-red-700">
              {profileErrorText(importMutation.error)}
            </p>
          ) : null}
        </section>

        <section
          className="rounded-xl border bg-white p-5"
          aria-labelledby="skill-list-title"
        >
          <h2 id="skill-list-title" className="text-lg font-semibold">
            中央列表
          </h2>
          {skillsQuery.isPending ? (
            <p role="status" className="mt-4 text-sm">
              正在读取 Skills…
            </p>
          ) : null}
          {skillsQuery.isError ? (
            <p role="alert" className="mt-4 text-sm text-red-700">
              {profileErrorText(skillsQuery.error)}
            </p>
          ) : null}
          {skillsQuery.data?.length === 0 ? (
            <p className="text-muted-foreground mt-4 text-sm">
              尚无 Skill。请先选择本地目录。
            </p>
          ) : null}
          {contentMutation.isError ? (
            <p role="alert" className="mt-4 text-sm text-red-700">
              内容预览失败：{profileErrorText(contentMutation.error)}
            </p>
          ) : null}
          {deleteMutation.isError ? (
            <p role="alert" className="mt-4 text-sm text-red-700">
              移出中央库失败：{profileErrorText(deleteMutation.error)}
            </p>
          ) : null}
          <div className="mt-4 space-y-3">
            {skillsQuery.data?.map((skill) => (
              <article key={skill.id} className="rounded-lg border p-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <h3 className="font-medium">{skill.name}</h3>
                    <p className="text-muted-foreground mt-1 text-xs">
                      {skill.status} · hash {skill.contentHash.slice(0, 12)}…
                    </p>
                    {skill.diagnosticCode ? (
                      <p className="mt-1 text-xs text-red-700">
                        {skill.diagnosticCode}
                      </p>
                    ) : null}
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={contentMutation.isPending}
                      onClick={() => contentMutation.mutate(skill.id)}
                    >
                      {contentMutation.isPending &&
                      contentMutation.variables === skill.id
                        ? "正在读取…"
                        : "内容预览"}
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={
                        deleteMutation.isPending &&
                        deleteMutation.variables?.id === skill.id
                      }
                      onClick={() => deleteMutation.mutate(skill)}
                    >
                      {deleteMutation.isPending &&
                      deleteMutation.variables?.id === skill.id
                        ? "正在移出…"
                        : "移出中央库"}
                    </Button>
                  </div>
                </div>
                <dl className="mt-3 grid gap-2 text-xs">
                  <div>
                    <dt className="text-muted-foreground">
                      原来源（只读溯源）
                    </dt>
                    <dd className="break-all">{skill.sourcePath}</dd>
                  </div>
                  <div>
                    <dt className="text-muted-foreground">中央副本</dt>
                    <dd className="break-all">{skill.centralPath}</dd>
                  </div>
                </dl>
                <p className="bg-muted mt-3 rounded p-2 text-xs">
                  {skill.description}
                </p>
                <div className="mt-3 flex flex-wrap gap-2">
                  {(["claude", "codex"] as const).map((tool) => (
                    <Button
                      key={tool}
                      size="sm"
                      variant={
                        skill.globalTools.includes(tool) ? "default" : "outline"
                      }
                      disabled={
                        globalAssignmentMutation.isPending ||
                        (skill.status !== "ready" &&
                          !skill.globalTools.includes(tool))
                      }
                      onClick={() =>
                        globalAssignmentMutation.mutate({ skill, tool })
                      }
                    >
                      {tool === "claude" ? "Claude" : "Codex"} 全局
                      {skill.globalTools.includes(tool) ? "已分配" : "未分配"}
                    </Button>
                  ))}
                </div>
              </article>
            ))}
          </div>
        </section>
      </div>

      <section
        className="mx-auto mt-6 max-w-6xl rounded-xl border bg-white p-5"
        aria-labelledby="skill-target-title"
      >
        <h2 id="skill-target-title" className="text-lg font-semibold">
          全局目标状态
        </h2>
        {statusesQuery.isPending ? (
          <p role="status" className="mt-4 text-sm">
            正在检查全局 Skills 目标…
          </p>
        ) : null}
        {statusesQuery.isError ? (
          <p role="alert" className="mt-4 text-sm text-red-700">
            {profileErrorText(statusesQuery.error)}
          </p>
        ) : null}
        {statusesQuery.data?.length === 0 ? (
          <p className="text-muted-foreground mt-4 text-sm">
            当前没有可检查的全局 Skills 目标。
          </p>
        ) : null}
        <div className="mt-4 grid gap-3 md:grid-cols-2">
          {statusesQuery.data?.map((status) => (
            <article
              key={status.tool}
              className="rounded-lg border p-4 text-sm"
            >
              <div className="flex items-center justify-between gap-2">
                <strong>{status.tool === "claude" ? "Claude" : "Codex"}</strong>
                <SyncStatusBadge status={status.status} />
              </div>
              <code className="mt-2 block text-xs break-all">
                {status.targetPath ?? "目标不可用"}
              </code>
              {status.diagnosticCode ? (
                <p className="mt-2 text-xs text-amber-800">
                  {status.diagnosticCode}
                </p>
              ) : null}
              <Button
                className="mt-3"
                size="sm"
                variant="outline"
                disabled={previewMutation.isPending}
                onClick={() =>
                  previewMutation.mutate({
                    tool: status.tool,
                    targetProjectId: null,
                  })
                }
              >
                预览全局同步
              </Button>
            </article>
          ))}
        </div>
      </section>

      <section
        className="mx-auto mt-6 max-w-6xl rounded-xl border bg-white p-5"
        aria-labelledby="skill-project-title"
      >
        <h2 id="skill-project-title" className="text-lg font-semibold">
          项目追加与全局继承
        </h2>
        {projectsQuery.isPending ? (
          <p role="status" className="mt-4 text-sm">
            正在读取已登记项目…
          </p>
        ) : null}
        {projectsQuery.isError ? (
          <p role="alert" className="mt-4 text-sm text-red-700">
            {profileErrorText(projectsQuery.error)}
          </p>
        ) : null}
        {projectsQuery.data?.length === 0 ? (
          <p className="text-muted-foreground mt-4 text-sm">
            尚无已登记项目。请先在项目功能中登记本地目录。
          </p>
        ) : null}
        <div className="mt-4 grid gap-4 md:grid-cols-2">
          <label className="text-sm font-medium">
            项目
            <select
              className="field mt-2"
              value={projectId}
              onChange={(event) => setProjectId(event.target.value)}
            >
              <option value="">请选择已登记项目</option>
              {projectsQuery.data?.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.displayName}
                </option>
              ))}
            </select>
          </label>
          <label className="text-sm font-medium">
            工具
            <select
              className="field mt-2"
              value={projectTool}
              onChange={(event) =>
                setProjectTool(
                  event.target.value === "codex" ? "codex" : "claude",
                )
              }
            >
              <option value="claude">Claude</option>
              <option value="codex">Codex</option>
            </select>
          </label>
        </div>
        <div className="mt-4 space-y-2">
          {!projectId ? (
            <p className="text-muted-foreground text-sm">
              选择项目后可查看项目追加项和只读的全局继承项。
            </p>
          ) : null}
          {projectId && projectOptionsQuery.isPending ? (
            <p role="status" className="text-sm">
              正在读取项目 Skill 选项…
            </p>
          ) : null}
          {projectOptionsQuery.isError ? (
            <p role="alert" className="text-sm text-red-700">
              {profileErrorText(projectOptionsQuery.error)}
            </p>
          ) : null}
          {projectId && projectOptionsQuery.data?.length === 0 ? (
            <p className="text-muted-foreground text-sm">
              中央库暂无可用于该项目的 Skill。
            </p>
          ) : null}
          {projectTrustBlocked ? (
            <p role="alert" className="text-sm text-amber-800">
              Codex 项目尚未受信任，不能预览或应用项目 Skill 链接。
            </p>
          ) : null}
          {projectOptionsQuery.data?.map((option) => (
            <label
              key={option.skillId}
              className="flex items-center justify-between gap-3 rounded-lg border p-3 text-sm"
            >
              <span>
                {option.name} ·{" "}
                {option.state === "inherited"
                  ? "全局继承（只读）"
                  : option.state === "selected"
                    ? "项目追加"
                    : "可选"}
                {option.status !== "ready" ? ` · ${option.status}` : ""}
              </span>
              <input
                type="checkbox"
                checked={option.state !== "available"}
                disabled={
                  !option.selectable || projectAssignmentMutation.isPending
                }
                onChange={(event) =>
                  selectedProject
                    ? projectAssignmentMutation.mutate({
                        projectId: selectedProject.id,
                        tool: projectTool,
                        skillId: option.skillId,
                        assigned: event.target.checked,
                        skillRowVersion: option.rowVersion,
                        projectRowVersion: selectedProject.rowVersion,
                      })
                    : undefined
                }
              />
            </label>
          ))}
        </div>
        <Button
          className="mt-4"
          variant="outline"
          disabled={
            !projectId || projectTrustBlocked || previewMutation.isPending
          }
          onClick={() =>
            previewMutation.mutate({
              tool: projectTool,
              targetProjectId: projectId,
            })
          }
        >
          预览项目同步
        </Button>
      </section>

      {contentPreview ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4">
          <section
            ref={contentDialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby="skill-content-title"
            tabIndex={-1}
            onKeyDown={onContentDialogKeyDown}
            className="max-h-[88vh] w-full max-w-3xl overflow-auto rounded-xl bg-white p-6 shadow-xl"
          >
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-muted-foreground text-sm">
                  中央副本只读内容
                </p>
                <h2
                  id="skill-content-title"
                  className="mt-1 text-xl font-semibold"
                >
                  {contentPreview.name}
                </h2>
              </div>
              <Button variant="outline" size="sm" onClick={closeContentPreview}>
                关闭
              </Button>
            </div>
            <pre className="bg-muted mt-4 overflow-auto rounded p-4 text-xs leading-5">
              {contentPreview.skillMd}
            </pre>
            <p className="mt-4 text-sm font-medium">目录文件</p>
            <ul className="mt-2 list-disc pl-5 text-xs">
              {contentPreview.files.map((file) => (
                <li key={file}>{file}</li>
              ))}
            </ul>
            {contentPreview.files.length === 0 ? (
              <p className="text-muted-foreground mt-2 text-xs">
                目录文件列表为空。
              </p>
            ) : null}
          </section>
        </div>
      ) : null}

      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? "claude"}
        artifactKind="skill"
        applying={applyMutation.isPending}
        onClose={() => setOpenPreview(null)}
        onApply={(previewId, tool) =>
          applyMutation.mutate({
            previewId,
            tool,
            projectId: openPreview?.projectId ?? null,
          })
        }
      />
    </main>
  );
}
