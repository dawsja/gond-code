import { useSyncExternalStore } from "react";

/**
 * What each pane's agent is up to, read from its terminal the way you would: output still coming
 * means it's working; a question on screen ("Do you want to…", "[y/n]") means it needs you; it
 * stopping while you were elsewhere means it's done, until you look.
 */
export type Status = "working" | "needs" | "done" | "idle" | "exited";

/** A pane's status and since when (ms), for "Working · 3m". */
export type PaneStatus = { status: Status; since: number };

export const STATUS_LABEL: Record<Status, string> = {
  working: "Working",
  needs: "Needs you",
  done: "Done",
  idle: "Idle",
  exited: "Exited",
};

/** The agents' own prompts for a yes or no, an approval or a choice. */
const ASKING =
  /do you want to|would you like to|do you trust|trust this folder|trust and continue|allow (this|once|always)|\[y\/n\]|\(y\/n\)|\[y\]\s*yes|❯\s*1\.\s*yes|approve|press enter to continue|waiting for (your )?(approval|input)|permission to/i;

export const asks = (screen: string) => ASKING.test(screen);

const statuses = new Map<string, PaneStatus>();
const listeners = new Set<() => void>();
let snapshot: Record<string, PaneStatus> = {};

export function setStatus(paneId: string, s: Status | undefined) {
  if (statuses.get(paneId)?.status === s) return;
  if (s) statuses.set(paneId, { status: s, since: Date.now() });
  else statuses.delete(paneId);
  snapshot = Object.fromEntries(statuses);
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

/** Every pane's status, by pane id. */
export const useStatuses = () => useSyncExternalStore(subscribe, () => snapshot);

/** "now", "40s", "3m", "1h", "2d": how long a status has held. */
export function ago(since: number, now = Date.now()) {
  const s = Math.max(0, Math.round((now - since) / 1000));
  if (s < 10) return "now";
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}
