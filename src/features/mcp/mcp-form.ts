import type {
  JsonValue,
  McpServerDto,
  McpServerInput,
  McpTransport,
  UpdateMcpServerInput,
} from "@/bindings/commands";

/** MCP 新增/编辑表单的草稿状态；敏感字段只保存用户本次输入，不回填原值。 */
export interface McpFormState {
  id: string | null;
  rowVersion: number | null;
  name: string;
  transport: McpTransport;
  command: string;
  args: string;
  url: string;
  headers: string;
  env: string;
  extra: string;
  keepHeaders: boolean;
  keepEnv: boolean;
  keepExtra: boolean;
  enabled: boolean;
}

export const emptyMcpForm: McpFormState = {
  id: null,
  rowVersion: null,
  name: "",
  transport: "stdio",
  command: "",
  args: "",
  url: "",
  headers: "{}",
  env: "{}",
  extra: "{}",
  keepHeaders: false,
  keepEnv: false,
  keepExtra: false,
  enabled: true,
};

export function createMcpInput(form: McpFormState): McpServerInput {
  return {
    name: form.name,
    transport: form.transport,
    command: form.transport === "stdio" ? form.command : null,
    args: form.transport === "stdio" ? lines(form.args) : [],
    url: form.transport === "streamable_http" ? form.url : null,
    headers:
      form.transport === "streamable_http"
        ? parseStringMap(form.headers, "Headers")
        : {},
    env: form.transport === "stdio" ? parseStringMap(form.env, "Env") : {},
    extra: parseJsonValue(form.extra, "扩展字段"),
    enabled: form.enabled,
  };
}

export function updateMcpInput(form: McpFormState): UpdateMcpServerInput {
  if (!form.id || form.rowVersion === null) {
    throw new Error("编辑记录缺少 row_version。");
  }
  const base = createMcpInput({
    ...form,
    headers: form.headers || "{}",
    env: form.env || "{}",
    extra: form.extra || "{}",
  });
  return {
    id: form.id,
    name: base.name,
    transport: base.transport,
    command: base.command,
    args: base.args,
    url: base.url,
    headers:
      form.transport === "streamable_http"
        ? form.keepHeaders
          ? { action: "keep" }
          : { action: "replace", value: base.headers }
        : { action: "clear" },
    env:
      form.transport === "stdio"
        ? form.keepEnv
          ? { action: "keep" }
          : { action: "replace", value: base.env }
        : { action: "clear" },
    extra: form.keepExtra
      ? { action: "keep" }
      : { action: "replace", value: base.extra },
    enabled: base.enabled,
    rowVersion: form.rowVersion,
  };
}

export function editMcpForm(server: McpServerDto): McpFormState {
  return {
    id: server.id,
    rowVersion: server.rowVersion,
    name: server.name,
    transport: server.transport,
    command: server.command ?? "",
    args: server.args.join("\n"),
    url: server.url ?? "",
    headers: "{}",
    env: "{}",
    extra: "{}",
    keepHeaders: true,
    keepEnv: true,
    keepExtra: true,
    enabled: server.enabled,
  };
}

export function validateMcpForm(form: McpFormState) {
  if (!form.name.trim()) throw new Error("名称不能为空。");
  if (form.transport === "stdio" && !form.command.trim())
    throw new Error("stdio 必须填写 Command。");
  if (form.transport === "streamable_http" && !form.url.trim())
    throw new Error("streamable_http 必须填写 URL。");
  if (!form.keepHeaders) parseStringMap(form.headers || "{}", "Headers");
  if (!form.keepEnv) parseStringMap(form.env || "{}", "Env");
  if (!form.keepExtra) parseJsonValue(form.extra || "{}", "扩展字段");
}

function lines(value: string): string[] {
  return value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

function parseStringMap(text: string, label: string): Record<string, string> {
  const value = parseJsonValue(text || "{}", label);
  if (!isJsonObject(value)) {
    throw new Error(`${label} 必须是字符串值 JSON 对象。`);
  }
  const result: Record<string, string> = {};
  for (const [key, item] of Object.entries(value)) {
    if (typeof item !== "string") {
      throw new Error(`${label} 必须是字符串值 JSON 对象。`);
    }
    result[key] = item;
  }
  return result;
}

function parseJsonValue(text: string, label: string): JsonValue {
  let value: unknown;
  try {
    value = JSON.parse(text || "{}");
  } catch {
    throw new Error(`${label} 不是合法 JSON。`);
  }
  if (!isJsonValue(value)) throw new Error(`${label} 包含不支持的 JSON 值。`);
  return value;
}

function isJsonValue(value: unknown): value is JsonValue {
  if (value === null || ["boolean", "number", "string"].includes(typeof value))
    return true;
  if (Array.isArray(value)) return value.every(isJsonValue);
  return isJsonObject(value) && Object.values(value).every(isJsonValue);
}

function isJsonObject(value: unknown): value is Record<string, JsonValue> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
