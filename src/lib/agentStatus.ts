// How each agent's status shows in a pane, from its file in `agents/` (`[status]`), as
// agent.detect reports it: its title's reader, its prompts' own words, where it asks, its
// image-paste key. A custom agent, or one not reported yet, gets the defaults.
import type { AgentInfo, AgentStatus } from "./types";

/** An agent's status facts, with its prompts compiled. */
export type StatusSpec = {
  /** Its title's reader in `status.ts` (`claude`, `codex`, `grok`). */
  title?: string;
  /** Its prompts' own words: on screen, it needs you. */
  prompts?: RegExp;
  /** Its own working line's words: on screen, it's working. */
  busy?: RegExp;
  /** Where it asks: the pane's bottom and cursor rows, or anywhere on screen. */
  scan: "bottom" | "screen";
  /** The key it pastes an image on. */
  paste: "ctrl+v" | "alt+v";
};

const DEFAULT: StatusSpec = { scan: "screen", paste: "ctrl+v" };

let specs: Record<string, StatusSpec> = {};

/** Patterns as one RegExp (any case, a line at a time); none for a pattern this browser can't
 * read, so the generic signs still count. */
function joined(patterns: string[] | undefined): RegExp | undefined {
  if (!patterns?.length) return undefined;
  try {
    return new RegExp(patterns.join("|"), "im");
  } catch {
    return undefined;
  }
}

function compile(s: AgentStatus): StatusSpec {
  return {
    title: s.title ?? undefined,
    prompts: joined(s.prompts),
    busy: joined(s.busy),
    scan: s.scan === "bottom" ? "bottom" : "screen",
    paste: s.paste === "alt+v" ? "alt+v" : "ctrl+v",
  };
}

/** Keeps what agent.detect reported. */
export function setAgentStatuses(agents: AgentInfo[]) {
  specs = Object.fromEntries(
    agents.filter((a) => a.status).map((a) => [a.id, compile(a.status!)] as const),
  );
}

/** Agent `id`'s status facts (the defaults for a custom or unknown agent). */
export const statusSpec = (id: string | undefined): StatusSpec => (id && specs[id]) || DEFAULT;
