import type { HookEvent, Tool } from "@/bindings/commands";

export const HOOK_EVENT_OPTIONS: HookEvent[] = [
  "SessionStart",
  "SessionEnd",
  "UserPromptSubmit",
  "PreToolUse",
  "PermissionRequest",
  "PostToolUse",
  "PostToolUseFailure",
  "SubagentStart",
  "SubagentStop",
  "PreCompact",
  "PostCompact",
  "Stop",
  "Notification",
];

export interface HookEventGroup {
  label: string;
  events: { event: HookEvent; label: string }[];
}

/// 事件分组（分类标题 → 事件 + 中文名）；只向各工具展示其官方支持的事件。
export const HOOK_EVENT_GROUPS: HookEventGroup[] = [
  {
    label: "会话生命周期",
    events: [
      { event: "SessionStart", label: "会话开始" },
      { event: "SessionEnd", label: "会话结束" },
      { event: "Stop", label: "会话停止" },
    ],
  },
  {
    label: "提示词与通知",
    events: [
      { event: "UserPromptSubmit", label: "提示词提交" },
      { event: "Notification", label: "通知" },
    ],
  },
  {
    label: "工具调用",
    events: [
      { event: "PreToolUse", label: "工具调用前" },
      { event: "PermissionRequest", label: "权限请求" },
      { event: "PostToolUse", label: "工具调用后" },
      { event: "PostToolUseFailure", label: "工具调用失败" },
    ],
  },
  {
    label: "子代理",
    events: [
      { event: "SubagentStart", label: "子代理启动" },
      { event: "SubagentStop", label: "子代理结束" },
    ],
  },
  {
    label: "上下文压缩",
    events: [
      { event: "PreCompact", label: "压缩前" },
      { event: "PostCompact", label: "压缩后" },
    ],
  },
];

const HOOK_EVENT_SET: ReadonlySet<string> = new Set(HOOK_EVENT_OPTIONS);

export function isHookEvent(value: string): value is HookEvent {
  return HOOK_EVENT_SET.has(value);
}

/// 前端侧事件支持矩阵（与后端 HookEvent::supported_for_tool 同一口径），
/// 用于过滤每个工具可见的事件分组与分配入口。
export function hookEventSupportedByTool(
  tool: Tool,
  event: HookEvent,
): boolean {
  const supported: Record<Tool, HookEvent[]> = {
    claude: [
      "SessionStart",
      "SessionEnd",
      "UserPromptSubmit",
      "PreToolUse",
      "PermissionRequest",
      "PostToolUse",
      "Notification",
      "SubagentStop",
      "Stop",
      "PreCompact",
    ],
    codex: [
      "SessionStart",
      "SessionEnd",
      "UserPromptSubmit",
      "PreToolUse",
      "PermissionRequest",
      "PostToolUse",
      "PreCompact",
      "PostCompact",
      "SubagentStart",
      "SubagentStop",
      "Stop",
    ],
    cursor: [
      "SessionStart",
      "SessionEnd",
      "PreToolUse",
      "PostToolUse",
      "PostToolUseFailure",
      "SubagentStart",
      "SubagentStop",
      "PreCompact",
      "Stop",
    ],
    zcode: [
      "SessionStart",
      "UserPromptSubmit",
      "PreToolUse",
      "PermissionRequest",
      "PostToolUse",
      "PostToolUseFailure",
      "Stop",
    ],
  };
  return supported[tool].includes(event);
}
