import { useCallback, useEffect, useState } from "react";
import { runCommand } from "@/lib/appState";

/** What `harness.check` says per harness. */
export type HarnessCheck = {
  id: string;
  installed?: string;
  latest?: string;
  updateAvailable: boolean;
  changelog: string;
};

/** How often Gond looks for newer agent versions while it runs. */
const EVERY_MS = 6 * 60 * 60 * 1000;

/**
 * Updates are found by themselves and installed by you: `harness.check` runs
 * at start and every few hours; a harness with a newer version shows its Update button.
 */
export function useHarnessChecks(live: boolean) {
  const [checks, setChecks] = useState<Record<string, HarnessCheck>>({});
  const check = useCallback(() => {
    if (!live) return;
    runCommand<HarnessCheck[]>("harness.check", {})
      .then((list) => setChecks(Object.fromEntries(list.map((c) => [c.id, c]))))
      .catch(() => undefined);
  }, [live]);
  useEffect(() => {
    check();
    const t = window.setInterval(check, EVERY_MS);
    return () => window.clearInterval(t);
  }, [check]);
  return { checks, check };
}
