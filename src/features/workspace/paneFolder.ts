import { useSyncExternalStore } from "react";
import { isMachine } from "@/features/machines/machines";
import type { Pane } from "./layout";

// The folder a shell pane's prompt shows right now (`promptFolder`), per pane. Terminal keeps
// it up to date; while an agent runs, the last known folder stays available.

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

/** A local folder action never receives a project id or another machine's path. */
export function folderForPane(
  pane: Pane,
  projects: readonly { id: string; path: string }[],
): string | undefined {
  if (isMachine(pane.shell)) return undefined;
  return (
    paneFolder(pane.id) ||
    pane.resume?.cwd ||
    projects.find((p) => p.id === pane.projectId)?.path ||
    "~"
  );
}
