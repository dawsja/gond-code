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
