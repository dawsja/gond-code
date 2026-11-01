// The agent programs (harnesses) as Settings → Agents reads them: agent.detect and
// harness.check (docs/HARNESSES.md).

export type Harness = {
  id: string;
  label: string;
  /** Brand id for its icon (the CLI's own mark). */
  brand: string;
  installed: boolean;
  version?: string;
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
