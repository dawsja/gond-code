import { useEffect, useState } from "react";

/** Seconds since `since`, ticking once a second. */
export function useTicking(since: string | undefined) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!since) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [since]);
  return since ? Math.max(0, Math.round((now - Date.parse(since)) / 1000)) : 0;
}
