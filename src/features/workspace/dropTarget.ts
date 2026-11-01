import { useSyncExternalStore } from "react";

// The pane files are being dragged over (FileDrop sets it; the pane lights up).

type Target = { paneId: string; count: number } | undefined;
let target: Target;
const listeners = new Set<() => void>();
export const setDropTarget = (t: Target) => {
  if (t?.paneId === target?.paneId && t?.count === target?.count) return;
  target = t;
  listeners.forEach((l) => l());
};
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

/** The pane files are being dragged over now, and how many. */
export const useDropTarget = () => useSyncExternalStore(subscribe, () => target);
