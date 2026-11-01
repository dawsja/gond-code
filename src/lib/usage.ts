// Plan limits as Settings → Usage shows them: what each agent's own sign-in reports through
// usage.refresh, at `/usage/default:<agent>`.

/** The agents Gond can read a plan for (usage.refresh), in their Usage order. */
export const PLAN_AGENTS = ["claude", "codex", "grok", "agy", "copilot", "cursor"];

/** The usage key an agent's own sign-in reports under. */
export const usageKey = (agentId: string) => `default:${agentId}`;

/** `/usage/default:<agent>`: the plan windows an agent's own sign-in reports. */
export type PlanUsage = {
  harness: string;
  /** Missing until the agent reports a window. */
  windows?: {
    name: "session" | "weekly" | "monthly";
    /** A window the plan scopes, e.g. "Weekly · Fable". */
    label?: string;
    percentLeft: number;
    resetsAt?: string;
    status?: string;
  }[];
  updatedAt: string;
};

/** When a used-up window resets, if one is used up: the agent is limited until then. */
export function limitedUntil(u?: PlanUsage, now = new Date()): string | undefined {
  const w = u?.windows?.find(
    (x) =>
      (x.status === "rejected" || x.percentLeft <= 0) &&
      (!x.resetsAt || new Date(x.resetsAt) > now),
  );
  if (!w) return undefined;
  return w.resetsAt
    ? new Date(w.resetsAt).toLocaleString(undefined, {
        weekday: "short",
        hour: "numeric",
        minute: "2-digit",
      })
    : "it resets";
}

const WINDOW = { session: "Session", weekly: "Weekly", monthly: "Monthly" } as const;

/** At or below this much left, a plan window is running low. */
export const LOW_PERCENT = 10;

/**
 * The window an agent will run out of first: the one with the least left, of those that
 * haven't already reset. `level` says how close: "out" (nothing left), "low" (10% or less),
 * else "ok". What the sidebar shows on an agent's row.
 */
export function tightest(u?: PlanUsage, now = new Date()) {
  const live = (u?.windows ?? []).filter(
    (w) => !w.resetsAt || Number.isNaN(Date.parse(w.resetsAt)) || new Date(w.resetsAt) > now,
  );
  if (!live.length) return undefined;
  const w = live.reduce((a, b) => (b.percentLeft < a.percentLeft ? b : a));
  const line = windowLine(w, now);
  const level: "out" | "low" | "ok" =
    w.status === "rejected" || line.left <= 0 ? "out" : line.left <= LOW_PERCENT ? "low" : "ok";
  return {
    ...line,
    name: w.name,
    resetsAt: w.resetsAt,
    level,
    resetsShort: short(w.resetsAt, now),
  };
}

/** "resets 3 PM", "resets Tue 3:40 AM": a reset as short as it can be said, for the sidebar. */
function short(at: string | undefined, now: Date) {
  if (!at) return undefined;
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return undefined;
  const opts: Intl.DateTimeFormatOptions = {
    hour: "numeric",
    ...(d.getMinutes() ? { minute: "2-digit" } : {}),
    ...(d.toDateString() === now.toDateString() ? {} : { weekday: "short" }),
  };
  return `resets ${d.toLocaleString(undefined, opts)}`;
}

/**
 * "Session 6% used · resets 3:40 PM": a window as its Usage card says it. Bars fill as the
 * plan is used, from empty to full.
 */
export function windowLine(w: NonNullable<PlanUsage["windows"]>[number], now = new Date()) {
  const left = Math.max(0, Math.min(100, Math.round(w.percentLeft)));
  let resets: string | undefined;
  if (w.resetsAt) {
    const at = new Date(w.resetsAt);
    if (!Number.isNaN(at.getTime())) {
      const sameDay = at.toDateString() === now.toDateString();
      resets = `resets ${at.toLocaleString(undefined, sameDay ? { hour: "numeric", minute: "2-digit" } : { weekday: "short", hour: "numeric", minute: "2-digit" })}`;
    }
  }
  const used = 100 - left;
  return { label: w.label ?? WINDOW[w.name] ?? w.name, left, used, text: `${used}% used`, resets };
}
