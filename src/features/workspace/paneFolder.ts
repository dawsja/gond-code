import { useSyncExternalStore } from "react";

// The folder a shell pane's prompt shows right now (`promptFolder`), per pane. Terminal keeps
// it up to date; the status bar reads the focused pane's. Agents don't move, so they have none.

const folders = new Map<string, string>();
const listeners = new Set<() => void>();

export function setPaneFolder(paneId: string, folder: string | undefined) {
  if (folders.get(paneId) === folder) return;
  if (folder) folders.set(paneId, folder);
  else folders.delete(paneId);
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

/** The folder `paneId`'s prompt shows, if it's a shell that shows one. */
export function usePaneFolder(paneId: string | undefined) {
  return useSyncExternalStore(subscribe, () => (paneId ? folders.get(paneId) : undefined));
}

/** The folder `paneId`'s prompt shows now, outside React (a link resolves from it). */
export const paneFolder = (paneId: string) => folders.get(paneId);
