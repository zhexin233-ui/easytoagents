import { useCallback, useEffect, useState } from "react";

import type { NotifyMessage } from "@/components/notify";

export const notifyDurationMs = 3_000;

export function useNotify() {
  const [notification, setNotification] = useState<NotifyMessage | null>(null);

  useEffect(() => {
    if (!notification) return undefined;

    const timeoutId = globalThis.setTimeout(
      () => setNotification(null),
      notifyDurationMs,
    );
    return () => globalThis.clearTimeout(timeoutId);
  }, [notification]);

  const notify = useCallback((next: NotifyMessage) => {
    setNotification({ ...next });
  }, []);

  return { notification, notify };
}
