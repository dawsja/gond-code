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

/**
 * What only an agent's own prompt shows: its choices under a pointer (`❯ 1. Yes`), a yes or
 * no, a key to press. Not the question in words: "Would you like to…" or "approve" is just as
 * likely the agent's reply, idle at its input, and read as asking it never stopped. These are
 * the shapes many agents share; an agent's own words are in its file in `agents/`
 * (`[status] prompts`).
 */
const CHOICE = new RegExp(
  [
    String.raw`^\s*[❯›>▶]\s*\d+\.\s`,
    "trust and continue",
    "allow (once|always)",
    String.raw`\[y\/n\]`,
    String.raw`\(y\/n\)`,
    String.raw`\[y\]\s*(yes|approve)`,
    "enter to confirm",
    "esc to cancel",
    "press enter to continue",
    // A status line that says only this ("Waiting for approval…"), not a reply that says it.
    String.raw`^\W*waiting for (your )?(approval|input)\W*$`,
  ].join("|"),
  "im",
);

/** An agent asking you something: a yes or no, an approval or a choice, or its own prompt's
 * words (`own`, from its file). */
export const asks = (screen: string, own?: RegExp) =>
  CHOICE.test(screen) || (!!own && own.test(screen));

/** What an agent says it's doing in the terminal's title, which it sets itself. */
export type Signal = "working" | "needs" | "idle";

/** A spinner: braille (Claude Code up to 2.1.227, Codex) or half circles (Claude Code since). */
const SPINNER = /^[⠀-⣿◐-◓] /;
const CODEX_SPINNER = /(?:^| )[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏](?: |$)/;

/**
 * What a title says, by its reader (the agent's file's `[status] title`): Claude Code's and
 * Codex's spinner while they work, Claude's `✳ ` once it's waiting at its input, Codex's
 * "Action Required" when it asks. The surest sign there is, where the screen only suggests.
 * Other agents' titles say nothing.
 */
export function fromTitle(reader: string | undefined, title: string): Signal | undefined {
  if (!title) return undefined;
  if (reader === "claude") {
    if (SPINNER.test(title)) return "working";
    if (title.startsWith("✳ ")) return "idle";
  }
  // Grok Build's items joined by " - ": "⚠ Action Required" when it asks, its spinner while it
  // works ("⠋ - Waiting for response… - <session> - grok"), its name last; "grok" alone idle.
  if (reader === "grok") {
    if (title.includes("Action Required")) return "needs";
    if (/^[⠀-⣿] /.test(title)) return "working";
    return /(^| - )grok$/i.test(title) ? "idle" : undefined;
  }
  if (reader === "codex") {
    if (title.includes("Action Required")) return "needs";
    if (CODEX_SPINNER.test(title)) return "working";
    return "idle";
  }
  return undefined;
}

/** An OSC 7501 report's pairs (`key=value`, `:` between, split on the first `=`: a base64
 * value can end in `=`); the last of a repeated key wins. */
function pairs(data: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const pair of data.split(":")) {
    const at = pair.indexOf("=");
    if (at > 0) out[pair.slice(0, at).trim().toLowerCase()] = pair.slice(at + 1).trim();
  }
  return out;
}

/** Whether an OSC 7501 report is the program's own record, not a session's or a subagent's
 * (those carry an `id`). */
export const ownRecord = (data: string) => !pairs(data).id;

/**
 * What an agent reports of itself on OSC 7501, the program status protocol (Devin, Grok
 * Build): `state=working`, `state=blocked:kind=permission`, `state=idle`, `state=done`… with
 * `app`, `msg` and the like beside it. Its own word, like a title, but meant for the terminal:
 * it comes first. `state=clear` takes it back.
 */
export function fromProgram(data: string): Signal | undefined {
  const state = pairs(data).state?.toLowerCase();
  if (state === "working") return "working";
  if (state === "blocked") return "needs";
  if (state === "idle" || state === "done" || state === "error") return "idle";
  return undefined;
}

const RULE = /^\s*─{8,}\s*$/;
/** A live choice on screen, which takes the input box's place while it's open. */
const CHOOSING = /esc to cancel|enter to (select|confirm)/i;

/**
 * Claude Code's input box, waiting for you: a `❯` line between the last two rules, with no
 * choice open. While it's there, a choice-looking line above it (`[y/n]`, `❯ 1.` in a reply
 * or a tool's output) is the conversation, not a question.
 */
export function atInput(rows: string[]): boolean {
  const rules = rows.flatMap((row, i) => (RULE.test(row) ? [i] : []));
  if (rules.length < 2) return false;
  const [top, bottom] = rules.slice(-2);
  const box = rows.slice(top + 1, bottom);
  return box.some((row) => /^\s*❯/.test(row)) && !rows.some((row) => CHOOSING.test(row));
}

/**
 * What the terminal answers on its own, not you: focus in and out (an agent that asks for
 * focus reports gets one each time the pane or the window gains or loses focus), cursor and
 * device reports, colour and mode replies.
 */
const REPORT =
  // eslint-disable-next-line no-control-regex -- escape sequences are what it reads
  /^(?:\x1b\[[IO]|\x1b\[\??\d+(?:;\d+)*[Rn]|\x1b\[[?>=]?[\d;]*c|\x1b\[\??\d+;\d+\$y|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1bP[^\x1b]*\x1b\\)+$/;

/** Whether what the terminal sends to the pty is you typing, not one of its own reports. */
export const typed = (data: string) => !REPORT.test(data);

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
