// Harnesses, models, thinking levels and accounts as the UI reads them (docs/HARNESSES.md
// ). Lists come from the drivers (harness.list, model.list { harness },
// account.list); nothing here hard-codes what a model can do.

export const LEVELS = [
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "ultra",
] as const;
export type Level = (typeof LEVELS)[number];

export type ThinkingSupport = {
  levels: Level[];
  default: Level;
  how: "flag" | "config" | "modelSlug" | "mode";
};

export type ModelInfo = {
  id: string;
  label: string;
  /** Brand id of who serves it (the model's icon). */
  provider: string;
  goodFor?: string;
  isDefault?: boolean;
  /** null: the model has no thinking control. */
  thinking: ThinkingSupport | null;
  /** The context window in tokens, where it's known for certain. */
  contextLimit?: number | null;
};

export type Harness = {
  id: string;
  label: string;
  /** Brand id for its icon (the CLI's own mark). */
  brand: string;
  installed: boolean;
  version?: string;
  /** Which providers' accounts it can use. */
  providers: string[];
  /** harness.check: where it comes from and what's newer. */
  source?: { kind: "npm" | "winget" | "vendor" | "bun"; name: string; verified?: boolean };
  latest?: string;
  /** Up to three lines from the changelog. */
  changelog?: string[];
  /** Its release notes (`harness.check`'s changelog link). */
  notesUrl?: string;
  /** A version can be pinned, so an update keeps the old one for rollback. */
  pinnable?: boolean;
  /** agy, Cursor and Amp update themselves: no update button, no rollback. */
  selfUpdating?: boolean;
  broken?: string;
};

export type HarnessStatus = "up-to-date" | "update" | "missing" | "broken" | "self";

export function statusOf(h: Harness): HarnessStatus {
  if (!h.installed) return "missing";
  if (h.broken) return "broken";
  if (h.selfUpdating) return "self";
  return h.latest && h.version && h.latest !== h.version ? "update" : "up-to-date";
}

export type Usage =
  | { kind: "window" | "weekly"; usedPercent: number; resetsAt?: string; windowLabel?: string }
  | { kind: "unknown" };

export type Account = {
  id: string;
  /** Sign-ins belong to one harness; keys work in every harness for the provider. */
  harness?: string;
  provider: string;
  label: string;
  kind: "oauth" | "apiKey" | "token" | "gateway";
  isDefault?: boolean;
  usage: Usage;
  /** Set while it can't be used: rate limited until this time. */
  limitedUntil?: string;
  /** Plan and masked identity, as account.list shows them (never the key). */
  plan?: string;
  identity?: string;
  /** The sign-in expired: the row offers Sign in. */
  expired?: boolean;
  /** The harness's own sign-in (the CLI's login, found at startup): no Gond home of its own. */
  own?: boolean;
  /** What Gond saw when the harness reports nothing ("Hit a limit at 17:15"). */
  seen?: string;
};

export const LEVEL_LABEL: Record<Level, string> = {
  none: "None",
  minimal: "Minimal",
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "X-High",
  max: "Max",
  ultra: "Ultra",
};

/** One model as `model.list { harness }` returns it. */
export type ListedModel = {
  id: string;
  label: string;
  provider: string;
  thinking: {
    levels: Level[];
    default?: Level | null;
    note?: string | null;
    via: "flag" | "config" | "modelSlug";
  };
};

/** model.list → the pickers' shape: no levels means no thinking control; an unknown default is
 *  the middle level the model takes, so the picker never preselects an extreme. */
export function fromModelList(models: ListedModel[]): ModelInfo[] {
  return models.map((m, i) => {
    const { levels, via } = m.thinking;
    const fallback = levels[Math.floor((levels.length - 1) / 2)];
    return {
      id: m.id,
      label: m.label,
      provider: m.provider,
      isDefault: i === 0,
      thinking: levels.length
        ? { levels, default: m.thinking.default ?? fallback, how: via }
        : null,
    };
  });
}
