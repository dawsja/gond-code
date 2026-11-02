// How each agent's status shows in a pane, from its file in `agents/` (`[status]`), as
// agent.detect reports it: its title's reader, its prompts' own words, where it asks, its
// image-paste key. A custom agent, or one not reported yet, gets the defaults.
import type { AgentInfo, AgentStatus } from "./types";

/** An agent's status facts, with its prompts compiled. */
export type StatusSpec = {
  /** Its title's reader in `status.ts` (`claude`, `codex`, `minimax`). */
  title?: string;
  /** Its prompts' own words: on screen, it needs you. */
  prompts?: RegExp;
  /** Where it asks: the pane's bottom and cursor rows, or anywhere on screen. */
  scan: "bottom" | "screen";
  /** The key it pastes an image on. */
  paste: "ctrl+v" | "alt+v";
};

const DEFAULT: StatusSpec = { scan: "screen", paste: "ctrl+v" };

let specs: Record<string, StatusSpec> = {};

function compile(s: AgentStatus): StatusSpec {
  let prompts: RegExp | undefined;
  try {
    prompts = s.prompts.length ? new RegExp(s.prompts.join("|"), "im") : undefined;
  } catch {
    // A pattern this browser can't read: the agent's generic prompts still count.
    prompts = undefined;
  }
  return {
    title: s.title ?? undefined,
    prompts,
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
