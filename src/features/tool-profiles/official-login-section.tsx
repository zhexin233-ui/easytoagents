import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type OfficialLoginPhase,
  type OfficialLoginStatusDto,
  type Tool,
} from "@/bindings/commands";
import { Button } from "@/components/ui/button";
import {
  officialLoginStatusQueryOptions,
  profileErrorText,
  profileKeys,
  unwrapResult,
} from "@/lib/profile-api";
import { toneClass } from "@/lib/tone-class";
import { toolMetadata } from "@/lib/tool-metadata";

interface OfficialLoginSectionProps {
  tool: Tool;
}

const LOGIN_POLL_INTERVAL_MS = 2000;

/** 登录子进程各阶段的用户可读结论；`idle` 没有会话，不额外说明。 */
const PHASE_TEXT: Record<OfficialLoginPhase, string | null> = {
  idle: null,
  running: "正在等待浏览器完成官方账号授权…",
  succeeded: "登录流程已完成。",
  failed: "登录流程失败。",
  cancelled: "登录已取消。",
  timed_out: "登录等待超时，已终止子进程。",
};

const TOOL_LOGIN_NOTE: Record<Tool, string> = {
  claude:
    "应用该渠道后，settings.json 中的第三方接入地址与凭据会被移除，Claude Code 使用 claude.ai 账号登录。",
  codex:
    "应用该渠道后，config.toml 回到内置 openai provider，Codex 使用 ChatGPT 账号（auth.json）登录。注意：开始登录时 Codex 会先清除当前的登录凭据，中途取消需要重新登录。",
  cursor: "",
  zcode: "",
  opencode: "",
};

/** 已登录时再次发起登录会覆盖或清除当前凭据；先让用户确认。 */
const START_LOGIN_CONFIRM: Record<Tool, string> = {
  claude: "当前已登录官方账号。重新登录会覆盖现有登录状态，继续？",
  codex:
    "当前已登录 ChatGPT 账号。Codex 会在开始登录时立即清除现有凭据，中途取消将需要重新登录。继续？",
  cursor: "",
  zcode: "",
  opencode: "",
};

function loginStateText(status: OfficialLoginStatusDto): string {
  if (status.loggedIn === null) {
    return "无法确认当前登录状态。";
  }
  if (!status.loggedIn) {
    return "当前未登录官方账号。";
  }
  const method = status.authMethod ? `（${status.authMethod}）` : "";
  const account = status.account ? ` · ${status.account}` : "";
  return `已登录官方账号${method}${account}`;
}

/**
 * 官方账号登录区块：登录由官方 CLI（`claude auth login` / `codex login`）在浏览器中
 * 完成，本应用只启动、监视与取消子进程并展示状态，不保存任何凭据。
 */
export function OfficialLoginSection({ tool }: OfficialLoginSectionProps) {
  const queryClient = useQueryClient();
  const queryKey = profileKeys.officialLogin(tool);
  const statusQuery = useQuery({
    ...officialLoginStatusQueryOptions(tool),
    refetchInterval: (query) =>
      query.state.data?.phase === "running" ? LOGIN_POLL_INTERVAL_MS : false,
  });
  const startMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(await commands.startOfficialLogin(tool)),
    onSuccess: (status) => {
      queryClient.setQueryData(queryKey, status);
    },
  });
  const cancelMutation = useMutation({
    mutationFn: async () =>
      unwrapResult(await commands.cancelOfficialLogin(tool)),
    onSuccess: (status) => {
      queryClient.setQueryData(queryKey, status);
    },
  });

  const status = statusQuery.data;
  const running = status?.phase === "running";
  const busy = startMutation.isPending || cancelMutation.isPending;
  const mutationError = [startMutation.error, cancelMutation.error]
    .map(profileErrorText)
    .find(Boolean);
  const label = toolMetadata(tool).label;

  const startLogin = () => {
    if (status?.loggedIn && !globalThis.confirm(START_LOGIN_CONFIRM[tool])) {
      return;
    }
    startMutation.mutate();
  };

  return (
    <section
      aria-labelledby={`${tool}-official-login-title`}
      className={`rounded-lg border p-4 text-sm ${toneClass("info")}`}
    >
      <h3 id={`${tool}-official-login-title`} className="font-medium">
        官方账号登录
      </h3>
      <p className="text-muted-foreground mt-1 text-xs">
        {TOOL_LOGIN_NOTE[tool]}
        登录由官方 CLI 在浏览器中完成，本应用不保存任何凭据。
      </p>
      {statusQuery.isPending ? (
        <p role="status" className="text-muted-foreground mt-3">
          正在探测 {label} 登录状态…
        </p>
      ) : null}
      {statusQuery.isError ? (
        <p role="alert" className="text-destructive mt-3">
          {profileErrorText(statusQuery.error)}
        </p>
      ) : null}
      {status ? (
        <div className="mt-3 space-y-2">
          {status.supported ? (
            <p className={status.loggedIn ? "text-success font-medium" : ""}>
              {loginStateText(status)}
            </p>
          ) : (
            <p className="text-warning font-medium">
              当前 {label} CLI 未安装或不提供登录子命令；请在终端手动运行{" "}
              <code>{status.manualCommand}</code> 后刷新状态。
            </p>
          )}
          {PHASE_TEXT[status.phase] ? (
            <p
              role={running ? "status" : undefined}
              className={
                status.phase === "failed" || status.phase === "timed_out"
                  ? "text-destructive"
                  : ""
              }
            >
              {PHASE_TEXT[status.phase]}
            </p>
          ) : null}
          {status.diagnostic ? (
            <pre className="bg-card overflow-auto rounded p-2 text-xs whitespace-pre-wrap dark:bg-slate-900/60">
              {status.diagnostic}
            </pre>
          ) : null}
          {running && status.loginUrl ? (
            <p className="text-xs">
              如果浏览器没有自动打开，请手动访问：
              <code className="mt-1 block break-all select-all">
                {status.loginUrl}
              </code>
            </p>
          ) : null}
          {status.supported ? (
            <p className="text-muted-foreground text-xs">
              也可以在终端运行 <code>{status.manualCommand}</code> 完成登录。
            </p>
          ) : null}
        </div>
      ) : null}
      {mutationError ? (
        <p role="alert" className="text-destructive mt-3">
          {mutationError}
        </p>
      ) : null}
      <div className="mt-3 flex flex-wrap gap-2">
        {running ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={busy}
            onClick={() => cancelMutation.mutate()}
          >
            取消登录
          </Button>
        ) : (
          <Button
            type="button"
            size="sm"
            disabled={busy || !status?.supported}
            onClick={startLogin}
          >
            登录官方账号
          </Button>
        )}
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy || statusQuery.isFetching}
          onClick={() => {
            void statusQuery.refetch();
          }}
        >
          刷新状态
        </Button>
      </div>
    </section>
  );
}
