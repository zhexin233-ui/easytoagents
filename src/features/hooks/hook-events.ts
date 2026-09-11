import {
  HOOK_EVENT_SUPPORT,
  type HookEvent,
  type Tool,
} from "@/bindings/commands";

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

/// 后端导出的支持记录用于过滤每个工具可见的事件分组与分配入口。
export function hookEventSupportedByTool(
  tool: Tool,
  event: HookEvent,
): boolean {
  return HOOK_EVENT_SUPPORT.some(
    (support) => support.tool === tool && support.event === event,
  );
}
