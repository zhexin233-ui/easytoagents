import { useCallback, useState } from "react";

import type { Tool } from "@/bindings/commands";

export interface ImportDialogState {
  tool: Tool;
  requestId: string;
}

export function useImportDialogState() {
  const [state, setState] = useState<ImportDialogState | null>(null);

  const open = useCallback((tool: Tool) => {
    setState({ tool, requestId: crypto.randomUUID() });
  }, []);
  const rescan = useCallback(() => {
    setState((current) =>
      current ? { ...current, requestId: crypto.randomUUID() } : null,
    );
  }, []);
  const close = useCallback(() => setState(null), []);

  return { state, open, rescan, close };
}
