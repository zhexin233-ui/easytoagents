import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

export const themeStorageKey = "easytoagents.theme.v1";

export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

const themePreferences: readonly ThemePreference[] = [
  "light",
  "dark",
  "system",
];

const darkMediaQuery = "(prefers-color-scheme: dark)";

export interface ThemeState {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
}

function readStoredPreference(): ThemePreference {
  try {
    const saved = localStorage.getItem(themeStorageKey);
    return themePreferences.find((item) => item === saved) ?? "system";
  } catch {
    return "system";
  }
}

function readSystemDark(): boolean {
  if (typeof window.matchMedia !== "function") {
    return false;
  }
  return window.matchMedia(darkMediaQuery).matches;
}

function resolveTheme(preference: ThemePreference): ResolvedTheme {
  if (preference !== "system") {
    return preference;
  }
  return readSystemDark() ? "dark" : "light";
}

function applyResolvedTheme(
  resolved: ResolvedTheme,
  preference?: ThemePreference,
): void {
  document.documentElement.classList.toggle("dark", resolved === "dark");
  // 让 Tauri 窗口的原生外观（含侧栏 NSVisualEffect 材质）跟随应用主题：
  // system 偏好传 null 让窗口回到跟随系统。vitest / 浏览器环境无 Tauri
  // IPC（__TAURI_INTERNALS__ 不存在），静默跳过。
  if (preference && "__TAURI_INTERNALS__" in window) {
    void getCurrentWindow()
      .setTheme(preference === "system" ? null : resolved)
      .catch(() => {});
  }
}

/**
 * 读取持久化偏好并把解析结果同步挂到 documentElement。
 * main.tsx 在 createRoot().render() 之前同步调用，保证首帧即为目标主题
 * （Tauri CSP 禁止 index.html 内联脚本，因此不用 head 内联引导方案）。
 */
export function applyThemeFromStorage(): void {
  const preference = readStoredPreference();
  applyResolvedTheme(resolveTheme(preference), preference);
}

export function useTheme(): ThemeState {
  const [preference, setPreferenceState] =
    useState<ThemePreference>(readStoredPreference);
  const [resolvedTheme, setResolvedTheme] = useState<ResolvedTheme>(() =>
    resolveTheme(preference),
  );

  // 同步外部系统：把当前解析结果落到 <html> 的 dark class 与持久化存储。
  useEffect(() => {
    applyResolvedTheme(resolvedTheme, preference);
    try {
      localStorage.setItem(themeStorageKey, preference);
    } catch {
      // 存储不可用时仅保留本次会话内的主题选择。
    }
  }, [preference, resolvedTheme]);

  // 仅在 system 模式订阅系统外观变化；离开该模式或卸载时移除监听。
  useEffect(() => {
    if (preference !== "system" || typeof window.matchMedia !== "function") {
      return undefined;
    }
    const media = window.matchMedia(darkMediaQuery);
    const onMediaChange = (event: MediaQueryListEvent) => {
      setResolvedTheme(event.matches ? "dark" : "light");
    };
    media.addEventListener("change", onMediaChange);
    return () => {
      media.removeEventListener("change", onMediaChange);
    };
  }, [preference]);

  // 切回 system 时在事件时机重新读取系统外观，避免携带过期快照。
  const setPreference = useCallback((next: ThemePreference) => {
    setPreferenceState(next);
    setResolvedTheme(resolveTheme(next));
  }, []);

  return { preference, resolvedTheme, setPreference };
}
