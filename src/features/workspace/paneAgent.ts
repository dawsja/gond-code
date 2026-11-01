import { useSyncExternalStore } from "react";

// The agent running in each pane, one you typed into its shell (`claude`, `codex`…), as
// pty.agent finds it. Terminal keeps it up to date; the pane's header, the sidebar's Agents
// and Settings → Agents read it. A pane with none is a plain shell.

const agents = new Map<string, string>();
const listeners = new Set<() => void>();
let snapshot: Record<string, string> = {};

export function setPaneAgent(paneId: string, agentId: string | undefined) {
  if (agents.get(paneId) === agentId) return;
  if (agentId) agents.set(paneId, agentId);
  else agents.delete(paneId);
  snapshot = Object.fromEntries(agents);
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

/** The agent in each pane that's running one, by pane id. */
export const usePaneAgents = () => useSyncExternalStore(subscribe, () => snapshot);
