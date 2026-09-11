import type { ProjectDto, Tool } from "@/bindings/commands";

export function projectBlocked(project: ProjectDto, tool: Tool): string | null {
  if (project.pathStatus !== "valid") {
    return "项目根路径无效，必须先重新扫描。";
  }
  if (tool === "codex" && project.codexTrustStatus !== "trusted") {
    return "Codex 项目尚未受信任；应用不会声称项目配置已生效。";
  }
  if (tool === "claude" && project.claudePolicyStatus !== "allowed") {
    return "Claude 管理策略尚未证明允许项目自定义。";
  }
  return null;
}
