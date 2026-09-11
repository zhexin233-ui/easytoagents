import { useCallback, useRef } from "react";

/** Synchronous duplicate-submit guard for forms whose mutation state updates later. */
export function useSubmitGuard() {
  const inFlight = useRef(false);

  const begin = useCallback(() => {
    if (inFlight.current) return false;
    inFlight.current = true;
    return true;
  }, []);
  const end = useCallback(() => {
    inFlight.current = false;
  }, []);
  const isInFlight = useCallback(() => inFlight.current, []);

  return { begin, end, isInFlight };
}
