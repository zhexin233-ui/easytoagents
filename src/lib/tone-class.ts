export type Tone = "neutral" | "info" | "success" | "warning" | "destructive";

const toneClasses: Record<Tone, string> = {
  neutral: "border-transparent bg-muted text-muted-foreground",
  // Keep the established light-mode utilities alongside semantic tokens. The
  // former are part of the visual regression contract while the latter drive
  // the dark theme without page-specific palette branches.
  info: "border-blue-200 bg-blue-50 text-blue-800 border-info/30 bg-info/10 text-info",
  success:
    "border-emerald-200 bg-emerald-50 text-emerald-800 border-success/30 bg-success/10 text-success",
  warning:
    "border-amber-200 bg-amber-50 text-amber-800 border-warning/30 bg-warning/10 text-warning",
  destructive:
    "border-red-200 bg-red-50 text-red-800 border-destructive/30 bg-destructive/10 text-destructive",
};

export function toneClass(tone: Tone): string {
  return toneClasses[tone];
}
