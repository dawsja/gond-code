// API accounts and endpoints. What `account.add` takes for an `apiKey` account, and which of it
// each harness can use.

export type Header = { name: string; value: string };

/** What `account.add` takes: a plain value, or a vault reference. */
export type HeaderValue = string | { secret: string };

/** The form's options; `headers` are rows here and a record on the wire. */
export type AccountOptions = {
  /** "Send key as Bearer": most gateways (LiteLLM, OpenRouter) want it for Anthropic. */
  bearer?: boolean;
  /** Extra headers as the form's rows; plain values only until secrets can be picked here. */
  headers?: Header[];
  /** OpenAI org and project ids. */
  organization?: string;
  project?: string;
  /** Set: the endpoint is Azure OpenAI. */
  azureApiVersion?: string;
  /** Advanced, Codex only. */
  wireApi?: "responses" | "chat";
  /** Claude only: a Bedrock API key, or Vertex with the machine's Google credentials (no key). */
  cloud?: { kind: "bedrock"; region: string } | { kind: "vertex"; region: string; project: string };
};

export type Capability =
  "endpoint" | "bearer" | "headers" | "organization" | "azure" | "wireApi" | "cloud";

/** 's matrix: what each harness maps. Anything else isn't offered. */
export const CAPS: Record<string, Capability[]> = {
  claude: ["endpoint", "bearer", "headers", "cloud"],
  codex: ["endpoint", "headers", "organization", "azure", "wireApi"],
  agy: ["endpoint"],
  grok: [],
  copilot: ["endpoint", "bearer", "headers", "azure", "wireApi"],
  cursor: [],
};

export const can = (harness: string, c: Capability) => (CAPS[harness] ?? []).includes(c);

/** https://, or http:// only for a server on this PC (localhost, 127.0.0.1). */
export function endpointOk(url: string): boolean {
  if (/^https:\/\/[^\s/]+\S*$/.test(url)) return true;
  return /^http:\/\/(localhost|127\.0\.0\.1)(:\d+)?(\/\S*)?$/.test(url);
}

const AUTH = /^(authorization|proxy-authorization|x-api-key|api-key|cookie)$/i;

/** Why a header can't be added, or undefined when it's fine. */
export function headerProblem(h: Header, all: Header[]): string | undefined {
  const name = h.name.trim();
  if (!name) return "Give it a name";
  if (!/^[A-Za-z0-9-]+$/.test(name)) return "Letters, digits and dashes only";
  if (AUTH.test(name) || /token/i.test(name))
    return "Auth headers come from the key, so they can't be set here";
  if (all.filter((x) => x.name.trim().toLowerCase() === name.toLowerCase()).length > 1)
    return "Already added";
  return undefined;
}

/** Vertex runs on the machine's Google credentials, so it needs no key. */
export const needsKey = (o: Pick<AccountOptions, "cloud">) => o.cloud?.kind !== "vertex";

/** `options` as `account.add` takes them: headers as a record. */
export type WireOptions = Omit<AccountOptions, "headers"> & {
  headers?: Record<string, HeaderValue>;
};

/** Only the options this harness maps, and only the ones that are set. */
export function optionsFor(harness: string, o: AccountOptions): WireOptions | undefined {
  const out: WireOptions = {};
  if (can(harness, "bearer") && o.bearer) out.bearer = true;
  const headers = (o.headers ?? []).filter((h) => h.name.trim());
  if (can(harness, "headers") && headers.length)
    out.headers = Object.fromEntries(headers.map((h) => [h.name.trim(), h.value]));
  if (can(harness, "organization")) {
    if (o.organization?.trim()) out.organization = o.organization.trim();
    if (o.project?.trim()) out.project = o.project.trim();
  }
  if (can(harness, "azure") && o.azureApiVersion?.trim())
    out.azureApiVersion = o.azureApiVersion.trim();
  if (can(harness, "wireApi") && o.wireApi) out.wireApi = o.wireApi;
  if (can(harness, "cloud") && o.cloud) out.cloud = o.cloud;
  return Object.keys(out).length ? out : undefined;
}

/** `account.test`: `{ accountId }` for a saved account, or the draft's fields. */
export type TestResult = { ok: boolean; models?: string[]; ms?: number; error?: string };

/** One line for a test: "Connected · 12 models · 340 ms", or the backend's own sentence. */
export function testLine(r: TestResult): string {
  if (!r.ok) return r.error?.trim() || "Couldn't connect.";
  const n = r.models?.length;
  return [
    "Connected",
    n !== undefined && `${n} model${n === 1 ? "" : "s"}`,
    r.ms !== undefined && `${r.ms} ms`,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** `/usage/{accountId}` (AGENT-API account.usage): plan windows the harness reports. */
export type PlanUsage = {
  harness: string;
  accountId: string;
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

/** The usage key an account's windows go under: its id, or `default:<agent>`. */
export const usageKey = (a: { id: string; own?: boolean; harness?: string }) =>
  a.own && a.harness ? `default:${a.harness}` : a.id;

/** When a used-up window resets, if one is used up: the account is limited until then. */
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
 * "Session 6% used · resets 3:40 PM": a window as the account row says it. Bars fill as the
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
