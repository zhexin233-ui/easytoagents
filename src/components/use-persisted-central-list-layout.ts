import { useCallback, useState } from "react";

import type { CentralListLayout } from "@/components/central-list-layout";

export const centralListLayoutStorageKeys = {
  mcp: "easytoagents.mcp.central-list-layout.v1",
  skills: "easytoagents.skills.central-list-layout.v1",
};

type CentralListLayoutPreference = keyof typeof centralListLayoutStorageKeys;
type PersistedCentralListLayoutState = readonly [
  CentralListLayout,
  (layout: CentralListLayout) => void,
];

export function usePersistedCentralListLayout(
  preference: CentralListLayoutPreference,
): PersistedCentralListLayoutState {
  const storageKey = centralListLayoutStorageKeys[preference];
  const [layout, setLayout] = useState<CentralListLayout>(() =>
    readPersistedLayout(storageKey),
  );
  const setPersistedLayout = useCallback(
    (nextLayout: CentralListLayout) => {
      setLayout(nextLayout);
      try {
        localStorage.setItem(storageKey, nextLayout);
      } catch {
        // 存储不可用时仍保留本次页面会话内的布局选择。
      }
    },
    [storageKey],
  );

  return [layout, setPersistedLayout];
}

function readPersistedLayout(storageKey: string): CentralListLayout {
  try {
    const saved = localStorage.getItem(storageKey);
    return saved === "list" || saved === "grid" ? saved : "list";
  } catch {
    return "list";
  }
}
