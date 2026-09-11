import { useCallback, useContext, useEffect, useState } from "react";

import {
  notifyDurationMs,
  NotifyContext,
  type NotifyMessage,
} from "@/components/notify-context";

export { notifyDurationMs };

export function useNotify() {
  const [notification, setNotification] = useState<NotifyMessage | null>(null);
  const context = useContext(NotifyContext);

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
  const clear = useCallback(() => setNotification(null), []);

  if (context) {
    return {
      notification: context.notifications.at(-1) ?? null,
      notify: context.notify,
      clear: context.clear,
    };
  }
  return { notification, notify, clear };
}
