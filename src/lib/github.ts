// GitHub through the GitHub CLI only: Gond has no OAuth app, and the
// tokens stay in gh's store. The shapes of (Anvil).

export type GithubAccount = {
  login: string;
  host: string;
  /** The account Git pushes use (`gh auth switch`). */
  active: boolean;
  scopes: string[];
  /** Proposed (not yet): how and when Gond added it; gh itself doesn't record either. */
  method?: "web" | "token";
  addedAt?: string;
};

/** `state.github`, from `github.status`. */
export type GithubStatus = {
  ghInstalled: boolean;
  ghVersion?: string;
  accounts: GithubAccount[];
  /** A browser sign-in waiting for GitHub's OK. */
  pending?: { host: string; userCode: string; verificationUri: string; expiresAt: string };
  /** Why the last browser sign-in didn't finish, one sentence. */
  loginError?: string;
};

/** `github.login.web` → the one-time code to show; the sign-in completes in the background. */
export type DeviceCode = { userCode: string; verificationUri: string; expiresAt: string };

/** A failed `github.*` call: `gh_missing`, `github_failed` (one sentence), or another code. */
export type GithubError = { code: string; message: string };

/** One plain sentence for a failed call; gh_missing turns into an offer to install it. */
export function errorSentence(e: unknown): { text: string; ghMissing: boolean } {
  const err = (e && typeof e === "object" && "code" in e ? e : undefined) as
    GithubError | undefined;
  if (err?.code === "gh_missing")
    return {
      text: "The GitHub CLI isn't on this PC yet. Install it, then sign in.",
      ghMissing: true,
    };
  const message = err?.message ?? (e instanceof Error ? e.message : undefined);
  return {
    text: message?.trim() || "GitHub didn't finish signing you in. Try again.",
    ghMissing: false,
  };
}

/** What sign-in asks for, and why. */
export const SCOPES = ["repo", "read:org", "workflow"];

export function statusOf(s: GithubStatus): { label: string; tone: "ok" | "warn" | "neutral" } {
  if (!s.ghInstalled) return { label: "GitHub CLI missing", tone: "warn" };
  return s.accounts.length
    ? { label: "Connected", tone: "ok" }
    : { label: "Not signed in", tone: "neutral" };
}

/** "Signed in with browser · 3 days ago". */
export function addedLine(a: GithubAccount, now = Date.now()): string {
  if (!a.method || !a.addedAt) return "Already signed in with GitHub CLI";
  const days = Math.round((now - Date.parse(a.addedAt)) / 86_400_000);
  const when = days < 1 ? "today" : days === 1 ? "yesterday" : `${days} days ago`;
  return a.method === "web" ? `Signed in with browser · ${when}` : `Token · added ${when}`;
}

/** mm:ss left on a device code. */
export function expiresIn(iso: string, now = Date.now()): string {
  const s = Math.max(0, Math.round((Date.parse(iso) - now) / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** "S" for sam, "SW" for sam-work: accounts on one PC often share a first letter. */
export function initialsOf(login: string): string {
  const [first, second] = login.split(/[-_.]/).filter(Boolean);
  return ((first?.[0] ?? "") + (second?.[0] ?? "")).toUpperCase();
}
