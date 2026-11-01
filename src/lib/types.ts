// TS mirrors of the Rust DTOs. Keep the two in step.

/** What every failed command rejects with. */
export type AppError = {
  code:
    | "not_found"
    | "not_a_git_repo"
    | "git_failed"
    | "agent_not_found"
    | "agent_failed"
    | "pty_failed"
    | "db"
    | "invalid";
  message: string;
};

export const isAppError = (e: unknown): e is AppError =>
  typeof e === "object" && e !== null && "code" in e && "message" in e;

/** A failure as one plain line for a toast or an inline error, never "[object Object]". */
export function errorText(e: unknown): string {
  if (isAppError(e)) return String(e.message);
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e !== null && "message" in e)
    return String((e as { message: unknown }).message);
  if (typeof e === "string") return e;
  try {
    return JSON.stringify(e) ?? String(e);
  } catch {
    return String(e);
  }
}

export type Project = {
  id: string;
  name: string;
  path: string;
  defaultBranch: string;
  addedAt: string;
  lastOpenedAt: string;
  /** "folder" is any folder, versioned by Gond's shadow repo out of sight; "git" is a repo. */
  vcs?: "folder" | "git";
  /** Where Ship (push, PR) goes; none until Connect to GitHub. Missing = unknown (old). */
  remote?: { name: string; url: string; host?: "github" } | null;
  /** Whether Ship (push, PR) can run here, and why not (Anvil). */
  ship?: { available: boolean; reason?: "folder" | "no-remote" };
  /** False once closed (project.close); opening it again (project.select, project.add) reopens. */
  open?: boolean;
  /** Versions Gond keeps of the folder (History). */
  versions?: number;
};

export type AgentKind = "claude" | "codex" | "custom";
export type AgentInfo = {
  id: string;
  kind: AgentKind;
  name: string;
  command: string;
  /** Brand id for ProviderIcon: "claude", "codex", or a provider/agent id. */
  provider?: string;
  installed: boolean;
  version?: string;
};

export type Tint = "blue" | "green" | "purple" | "pink" | "amber" | "orange" | "red";
/** The terminals' font: a Nerd Font the app ships. */
export type TerminalFont = "jetbrains-mono" | "hack" | "fira-code";
export type Settings = {
  tint: Tint;
  /** The window's X hides Gond Code to the tray, agents still working. Default true. */
  closeToTray?: boolean;
  /** Quitting leaves the terminals (and their agents) running, to come back. Default true. */
  keepTerminals?: boolean;
  /** A plain terminal's shell (shell.list id: "pwsh", "cmd", "wsl:Ubuntu"…); none = PowerShell. */
  shell?: string | null;
  /** Where a `file:line` link opens: a file.editors id, "system", none = the first editor found. */
  editor?: string | null;
  /** The terminals' font (not the app's). Default JetBrains Mono. */
  terminalFont?: TerminalFont;
};
