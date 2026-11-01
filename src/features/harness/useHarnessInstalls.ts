import { useCallback, useEffect, useRef, useState } from "react";
import { runCommand, useAppState } from "@/lib/appState";
import type { JobState } from "@/lib/setup";
import { errorText } from "@/lib/types";

/** One harness's install, update or uninstall as its card shows it: starting, the job,
 *  or why it stopped. */
export type InstallState = {
  kind: "install" | "update" | "uninstall";
  starting: boolean;
  job?: JobState;
  error?: string;
};

type Tracked = { kind: InstallState["kind"]; jobId?: string; error?: string };

/**
 * Installing a harness from the UI: `harness.install` answers `{ jobId }` at once, the job
 * runs in the background into `/installs/{jobId}`, and the card follows it. A refused call or a
 * failed job is one plain sentence with Retry. When a job ends, `onDone` re-detects the agents so
 * the card turns Installed with the version. `harness.uninstall` runs the same way; once
 * it's done the card simply goes back to Not installed.
 */
export function useHarnessInstalls(onDone: () => void) {
  const [tracked, setTracked] = useState<Record<string, Tracked>>({});
  const installs = useAppState((t) => t.installs as Record<string, JobState> | undefined);
  const onDoneRef = useRef(onDone);
  useEffect(() => {
    onDoneRef.current = onDone;
  });

  // Nothing shows as running until the command is accepted; a refusal is the card's error.
  const start = useCallback(
    (harnessId: string, kind: Tracked["kind"], input: Record<string, unknown>) => {
      setTracked((m) => ({ ...m, [harnessId]: { kind } }));
      runCommand<{ jobId: string }>(`harness.${kind}`, { harnessId, ...input }).then(
        (r) => setTracked((m) => ({ ...m, [harnessId]: { kind, jobId: r.jobId } })),
        (e) => setTracked((m) => ({ ...m, [harnessId]: { kind, error: errorText(e) } })),
      );
    },
    [],
  );
  const install = useCallback((harnessId: string) => start(harnessId, "install", {}), [start]);
  const uninstall = useCallback(
    (harnessId: string, removeEverything = false) =>
      start(harnessId, "uninstall", { keepData: !removeEverything }),
    [start],
  );
  const update = useCallback((harnessId: string) => start(harnessId, "update", {}), [start]);

  // Re-detect once per job that ends, so the list shows what's installed now.
  const ended = Object.values(tracked)
    .map((t) => (t.jobId ? installs?.[t.jobId] : undefined))
    .filter((j): j is JobState => !!j && (j.status === "done" || j.status === "failed"))
    .map((j) => j.id)
    .join(",");
  useEffect(() => {
    if (ended) onDoneRef.current();
  }, [ended]);

  const state = (harnessId: string): InstallState | undefined => {
    const t = tracked[harnessId];
    if (!t) return undefined;
    const job = t.jobId ? installs?.[t.jobId] : undefined;
    // A finished uninstall has nothing left to show: the card reads Not installed.
    if (t.kind === "uninstall" && job?.status === "done") return undefined;
    const fallback = t.kind === "uninstall" ? "The uninstall stopped." : "The install stopped.";
    return {
      kind: t.kind,
      starting: !t.jobId && !t.error,
      job,
      error: t.error ?? (job?.status === "failed" ? (job.error ?? fallback) : undefined),
    };
  };

  return { install, update, uninstall, state };
}
