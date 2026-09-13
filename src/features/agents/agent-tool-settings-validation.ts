import type {
  AgentToolSettingsDto,
  ClaudeAgentSettings,
  CodexAgentSettings,
} from "@/bindings/commands";

const MODEL_PATTERN = /^\S+$/;
const TOOL_PATTERN = /^[A-Za-z][A-Za-z0-9_:*()./\- ]{0,127}$/;
const FEATURE_PATTERN = /^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$/;

export const EMPTY_AGENT_TOOL_SETTINGS: AgentToolSettingsDto = {
  claude: null,
  codex: null,
};

export function validateAgentToolSettingsDraft(
  settings: AgentToolSettingsDto,
): string | null {
  const modelError = (model: string | null | undefined) => {
    if (model === null || model === undefined) return null;
    if (
      model.trim() === "" ||
      new TextEncoder().encode(model.trim()).length > 128 ||
      model.includes("\0") ||
      !MODEL_PATTERN.test(model.trim())
    ) {
      return "模型名必须非空、不能包含空白或 NUL，且不超过 128 字节。";
    }
    return null;
  };
  const modelErrorText =
    modelError(settings.claude?.model) ?? modelError(settings.codex?.model);
  if (modelErrorText) return modelErrorText;
  const claude: ClaudeAgentSettings | null = settings.claude;
  if (claude?.tools) {
    if (
      claude.tools.length > 64 ||
      claude.tools.some((tool) => !TOOL_PATTERN.test(tool))
    ) {
      return "Claude 工具列表最多 64 项，且每项格式无效。";
    }
  }
  const codex: CodexAgentSettings | null = settings.codex;
  if (codex?.features) {
    const entries = Object.keys(codex.features);
    if (
      entries.length > 64 ||
      entries.some((key) => !FEATURE_PATTERN.test(key))
    ) {
      return "Codex features 最多 64 个键，且键名格式无效。";
    }
  }
  if (new TextEncoder().encode(JSON.stringify(settings)).length > 16 * 1024) {
    return "工具特有设置序列化后不能超过 16 KiB。";
  }
  return null;
}
