import { useId, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";

import {
  commands,
  type ConfirmSkillImportInput,
  type PrepareSkillTakeoverInput,
  type SkillImportCandidateDto,
  type SkillImportResultDto,
  type SkillImportSourceDto,
  type SkillTakeoverPreviewResultDto,
  type Tool,
} from "@/bindings/commands";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { useSubmitGuard } from "@/hooks/use-submit-guard";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { skillImportQueryOptions } from "@/lib/skills-api";
import { toneClass } from "@/lib/tone-class";
import { toolMetadata } from "@/lib/tool-metadata";

interface SkillImportDialogProps {
  tool: Tool;
  requestId: string;
  onClose: () => void;
  onRescan: () => void;
  onImported: (result: SkillImportResultDto) => Promise<void>;
  onTakeoverPrepared: (result: SkillTakeoverPreviewResultDto) => Promise<void>;
}

const candidateLabels: Record<SkillImportCandidateDto["status"], string> = {
  importable: "可导入",
  already_imported: "已在中央库",
  name_conflict: "名称冲突",
  invalid: "技能无效",
};

const sourceLabels: Record<SkillImportSourceDto["kind"], string> = {
  claude_global: "Claude 全局目录",
  codex_home: "Codex 官方目录（正式同步目标）",
  codex_agents: "Codex Agents 通用目录（仅导入来源）",
  cursor_home: "Cursor 官方目录（正式同步目标）",
  cursor_agents: "Cursor Agents 通用目录（仅导入来源）",
  zcode_home: "ZCode 官方目录（正式同步目标）",
  zcode_agents: "ZCode Agents 通用目录（仅导入来源）",
  opencode_global: "OpenCode 全局 skills 目录（正式同步目标）",
};

const sourceStatusLabels: Record<SkillImportSourceDto["status"], string> = {
  ready: "已检测",
  missing: "来源目录不存在",
  empty: "没有可导入的用户技能",
  unavailable: "来源不可用，检测未完成",
};

export function SkillImportDialog(props: SkillImportDialogProps) {
  const titleId = useId();
  const descriptionId = useId();
  const query = useQuery(skillImportQueryOptions(props.tool, props.requestId));
  const [selectedImportIds, setSelectedImportIds] = useState<string[]>([]);
  const [selectedTakeoverIds, setSelectedTakeoverIds] = useState<string[]>([]);
  const [copied, setCopied] = useState(false);
  // 复制与接管共用一个在途守卫：同一时刻只允许一个 RPC 在途。
  const operationGuard = useSubmitGuard();
  // 一次性尝试闩：同一份预览凭据只允许尝试一次，仅在重新检测后复位；语义不同于在途守卫。
  const confirmAttempted = useRef(false);
  const takeoverAttempted = useRef(false);
  const confirm = useMutation({
    mutationFn: async (input: ConfirmSkillImportInput) =>
      unwrapResult(await commands.confirmSkillImport(input)),
    retry: false,
    onSuccess: async (result) => {
      setCopied(true);
      await props.onImported(result);
    },
    onSettled: () => {
      operationGuard.end();
    },
  });
  const takeover = useMutation({
    mutationFn: async (input: PrepareSkillTakeoverInput) =>
      unwrapResult(await commands.prepareSkillTakeover(input)),
    retry: false,
    onSuccess: props.onTakeoverPrepared,
    onSettled: () => {
      operationGuard.end();
    },
  });
  const close = () => {
    if (!operationGuard.isInFlight()) props.onClose();
  };
  const { dialogRef } = useDialogFocus(true, close);
  const preview = query.data;
  const error = profileErrorText(
    query.error ?? confirm.error ?? takeover.error,
  );
  const importable = preview?.candidates.filter(
    (candidate) => candidate.status === "importable",
  );
  const takeoverCandidates = preview?.candidates.filter(
    (candidate) => candidate.takeoverEligible,
  );
  const copyCandidates = preview?.candidates.filter(
    (candidate) => !candidate.takeoverEligible,
  );
  const selectedImportCandidateIds =
    importable
      ?.filter((candidate) => selectedImportIds.includes(candidate.candidateId))
      .map((candidate) => candidate.candidateId) ?? [];
  const selectedTakeoverCandidateIds =
    takeoverCandidates
      ?.filter((candidate) =>
        selectedTakeoverIds.includes(candidate.candidateId),
      )
      .map((candidate) => candidate.candidateId) ?? [];
  const busy = confirm.isPending || takeover.isPending;
  const canConfirm =
    Boolean(preview?.previewId) &&
    selectedImportCandidateIds.length > 0 &&
    !busy &&
    !confirm.isError &&
    !query.isPending;
  const canTakeover =
    Boolean(preview?.previewId) &&
    selectedTakeoverCandidateIds.length > 0 &&
    !busy &&
    !takeover.isError &&
    !query.isPending;

  const candidateCard = (
    candidate: SkillImportCandidateDto,
    mode: "copy" | "takeover",
  ) => {
    const selectedIds =
      mode === "copy" ? selectedImportIds : selectedTakeoverIds;
    const setSelectedIds =
      mode === "copy" ? setSelectedImportIds : setSelectedTakeoverIds;
    const selectable =
      mode === "copy"
        ? candidate.status === "importable"
        : candidate.takeoverEligible;
    return (
      <article
        key={candidate.candidateId}
        className="rounded-lg border p-4 text-sm"
      >
        <label className="flex items-center gap-2 font-medium">
          <input
            type="checkbox"
            aria-label={`${mode === "copy" ? "导入" : "接管"} ${candidate.name}`}
            checked={selectedIds.includes(candidate.candidateId)}
            disabled={
              !selectable ||
              !preview?.previewId ||
              busy ||
              (mode === "copy" ? confirm.isError : takeover.isError)
            }
            onChange={(event) => {
              if (operationGuard.isInFlight()) return;
              const checked = event.target.checked;
              setSelectedIds((current) =>
                checked
                  ? [...current, candidate.candidateId]
                  : current.filter((id) => id !== candidate.candidateId),
              );
            }}
          />
          {candidate.name}
        </label>
        <p className="text-muted-foreground mt-2 text-xs">
          {candidateLabels[candidate.status]}
        </p>
        <p className="mt-2">{candidate.description}</p>
        <ul className="mt-2 space-y-1 text-xs">
          {candidate.sourcePaths.map((path) => (
            <li key={path}>
              <code className="break-all">{path}</code>
            </li>
          ))}
        </ul>
        {candidate.status === "already_imported" ? (
          <p className="mt-2 text-xs">
            {mode === "takeover"
              ? "中央已有相同内容；接管会复用该副本并建立当前工具的全局分配。"
              : "中央已有相同内容，不会新增副本或分配。"}
          </p>
        ) : null}
        {candidate.reason ? (
          <p className="text-warning mt-2 text-xs">{candidate.reason}</p>
        ) : null}
      </article>
    );
  };

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={close}
        labelledBy={titleId}
        describedBy={descriptionId}
        size="lg"
      >
        <DialogHeader>
          <div className="min-w-0">
            <h2 id={titleId} className="text-[15px] font-semibold">
              导入 {toolMetadata(props.tool).label} 全局 Skills
            </h2>
            <p id={descriptionId} className="text-muted-foreground mt-1">
              “复制”只新增中央副本，不修改来源；“接管”只适用于正式目录中与中央副本完全一致的外链或目录，并且一定先进入持久化预览，不会直接应用。
            </p>
          </div>
        </DialogHeader>
        <form
          aria-labelledby={titleId}
          className="flex min-h-0 flex-1 flex-col"
          onSubmit={(event) => {
            event.preventDefault();
            if (
              confirmAttempted.current ||
              !canConfirm ||
              !preview?.previewId
            ) {
              return;
            }
            if (!operationGuard.begin()) return;
            confirmAttempted.current = true;
            // 禁用全部控件前保留容器焦点，覆盖提交与列表刷新阶段。
            dialogRef.current?.focus();
            confirm.mutate({
              previewId: preview.previewId,
              candidateIds: selectedImportCandidateIds,
            });
          }}
        >
          <DialogBody className="space-y-4">
            {props.tool === "codex" ? (
              <p className="text-muted-foreground text-sm">
                Codex .system 内置技能不在本次导入范围，不会生成可选候选。
              </p>
            ) : null}
            {query.isPending ? (
              <p role="status">正在检测已有全局 Skills…</p>
            ) : null}
            {error ? (
              <p role="alert" className="text-destructive text-sm">
                {copied ? "已复制到中央库，但列表刷新失败：" : ""}
                {error} 请重新检测后再确认。
              </p>
            ) : null}
            {confirm.isPending ? (
              <p role="status" className="text-sm">
                {copied
                  ? "已复制到中央库，正在刷新列表…"
                  : "正在安全导入所选 Skills…"}
              </p>
            ) : null}
            {takeover.isPending ? (
              <p role="status" className="text-sm">
                正在校验接管证据并生成持久化预览…
              </p>
            ) : null}
            {preview ? (
              <>
                <div className="space-y-3" aria-label="检测来源">
                  {preview.sources.map((source) => (
                    <article
                      key={source.kind}
                      className="rounded-lg border p-3 text-sm"
                    >
                      <h3 className="font-medium">
                        {sourceLabels[source.kind]}
                      </h3>
                      <code className="mt-2 block text-xs break-all">
                        {source.path}
                      </code>
                      <p
                        role={
                          source.status === "unavailable" ? "alert" : "status"
                        }
                        className="mt-2"
                      >
                        {sourceStatusLabels[source.status]}
                      </p>
                      {source.message ? (
                        <p className="mt-2">{source.message}</p>
                      ) : null}
                      {source.diagnosticCode ? (
                        <p className="text-warning mt-2 text-xs">
                          诊断码：<code>{source.diagnosticCode}</code>
                        </p>
                      ) : null}
                    </article>
                  ))}
                </div>
                {preview.message ? (
                  <p role="status" className="text-sm">
                    {preview.message}
                  </p>
                ) : null}
                {!importable?.length && !takeoverCandidates?.length ? (
                  <p role="status" className="text-sm">
                    没有可复制或接管的用户技能，请查看来源和候选状态；处理后可重新检测。
                  </p>
                ) : !preview.previewId ? (
                  <p role="status" className="text-sm">
                    当前检测结果不能确认导入，请处理来源诊断后重新检测。
                  </p>
                ) : null}
                {copyCandidates?.length ? (
                  <section
                    className="space-y-3"
                    aria-labelledby="copy-skills-title"
                  >
                    <div>
                      <h3 id="copy-skills-title" className="font-semibold">
                        复制到中央库
                      </h3>
                      <p className="text-muted-foreground mt-1 text-xs">
                        只复制勾选项；原安装、分配和原生目标均保持不变。
                      </p>
                    </div>
                    {copyCandidates.map((candidate) =>
                      candidateCard(candidate, "copy"),
                    )}
                  </section>
                ) : null}
                {takeoverCandidates?.length ? (
                  <section
                    className={`space-y-3 rounded-lg border p-4 ${toneClass("warning")}`}
                    aria-labelledby="takeover-skills-title"
                  >
                    <div>
                      <h3 id="takeover-skills-title" className="font-semibold">
                        接管正式目录
                      </h3>
                      <p className="text-warning mt-1 text-xs">
                        继续后只生成变更预览。确认 Apply
                        时，入口才会替换为中央链接；外链源不变，目录原件会先保存为完整树快照。
                      </p>
                    </div>
                    {takeoverCandidates.map((candidate) =>
                      candidateCard(candidate, "takeover"),
                    )}
                  </section>
                ) : null}
              </>
            ) : null}
          </DialogBody>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={close}
            >
              取消
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={busy || query.isPending}
              onClick={() => {
                if (operationGuard.isInFlight() || query.isPending) return;
                // 保留同一个弹窗与原始触发焦点，只更换扫描证据和选择状态。
                dialogRef.current?.focus();
                setSelectedImportIds([]);
                setSelectedTakeoverIds([]);
                setCopied(false);
                confirmAttempted.current = false;
                takeoverAttempted.current = false;
                confirm.reset();
                takeover.reset();
                props.onRescan();
              }}
            >
              重新检测
            </Button>
            <Button type="submit" disabled={!canConfirm}>
              {confirm.isPending
                ? "正在导入…"
                : `复制所选项（${selectedImportCandidateIds.length}）`}
            </Button>
            <Button
              type="button"
              disabled={!canTakeover}
              onClick={() => {
                if (
                  takeoverAttempted.current ||
                  !canTakeover ||
                  !preview?.previewId
                ) {
                  return;
                }
                if (!operationGuard.begin()) return;
                takeoverAttempted.current = true;
                dialogRef.current?.focus();
                takeover.mutate({
                  previewId: preview.previewId,
                  candidateIds: selectedTakeoverCandidateIds,
                });
              }}
            >
              {takeover.isPending
                ? "正在生成预览…"
                : `预览接管所选项（${selectedTakeoverCandidateIds.length}）`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </DialogOverlay>
  );
}
