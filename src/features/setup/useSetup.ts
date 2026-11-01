import { useCallback, useEffect, useState } from "react";
import { liveBackend, runCommand, useAppState } from "@/lib/appState";
import type { GithubStatus } from "@/lib/github";
import type { IdentityStatus, JobState, Prereq } from "@/lib/setup";

/** Git and the GitHub CLI on this PC (Settings → GitHub); Node and Bun join when an agent needs them. */
const FIRST_RUN = ["git", "gh"];

/**
 * Set up, live: `prereq.list`, the job in `/installs/{jobId}` from the state tree,
 * `git.identity` and `github.status`. Re-reads when a job ends, so Ready shows by itself.
 */
export function useSetup() {
  const live = liveBackend();
  const [prereqs, setPrereqs] = useState<Prereq[]>();
  const [identity, setIdentity] = useState<IdentityStatus>();
  const [github, setGithub] = useState<GithubStatus>();
  const [jobId, setJobId] = useState<string>();
  const installs = useAppState((t) => t.installs as Record<string, JobState> | undefined);
  const job = jobId
    ? installs?.[jobId]
    : Object.values(installs ?? {}).find((j) => j.status === "running" || j.status === "pending");

  const refresh = useCallback(() => {
    if (!live) return;
    runCommand<Prereq[]>("prereq.list").then(
      (l) => setPrereqs(l.filter((p) => FIRST_RUN.includes(p.id))),
      () => setPrereqs([]),
    );
    runCommand<IdentityStatus>("git.identity").then(setIdentity, () => undefined);
    runCommand<GithubStatus>("github.status").then(setGithub, () => undefined);
  }, [live]);
  useEffect(refresh, [refresh]);

  const ended = job?.status === "done" || job?.status === "failed";
  useEffect(() => {
    if (ended) refresh();
  }, [ended, refresh]);

  /** prereq.install: one job for the batch; its id picks the job the card and toast follow. */
  const install = useCallback((input: Record<string, unknown>) => {
    return runCommand<{ jobId: string }>("prereq.install", input).then((r) => setJobId(r.jobId));
  }, []);

  const missing = (prereqs ?? []).some((p) => !p.installed);
  return { live, loaded: prereqs !== undefined, missing, prereqs, identity, github, job, install };
}
