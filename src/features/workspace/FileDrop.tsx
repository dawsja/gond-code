import { useEffect, useRef } from "react";
import { isRegistered, liveBackend, runCommand } from "@/lib/appState";
import { onFileDrag } from "@/lib/ipc";
import { notify } from "@/lib/notify";
import { errorText } from "@/lib/types";
import { setDropTarget as set } from "./dropTarget";
import { handles } from "./Terminal";

// Files and folders dragged onto a pane: while they're over one it lights up, and dropping
// types their paths into it, as its shell writes them (pane.drop), the way a paste would. An
// agent there reads them; a picture's path is how Claude Code, Codex and the rest attach one.

/** The pane (not a sidebar row) under a point. */
const paneAt = (x: number, y: number) =>
  (document.elementFromPoint(x, y)?.closest('section[data-entity="pane"]') as HTMLElement | null)
    ?.dataset.id;

/** Listens for files dragged onto the window; mounted once, beside the workspace. */
export function FileDrop({
  shellOf,
  onDropped,
}: {
  /** The pane's shell (`shell.list` id), which decides how a path is written. */
  shellOf: (paneId: string) => string | undefined;
  /** Focuses the pane the files were dropped on. */
  onDropped: (paneId: string) => void;
}) {
  const live = useRef({ shellOf, onDropped });
  useEffect(() => {
    live.current = { shellOf, onDropped };
  });
  useEffect(() => {
    const stop = onFileDrag((e) => {
      if (e.type === "leave") return set(undefined);
      const paneId = paneAt(e.x, e.y);
      if (e.type === "over") return set(paneId ? { paneId, count: e.paths.length } : undefined);
      set(undefined);
      if (!paneId || !e.paths.length) return;
      if (!liveBackend() || !isRegistered("pane.drop")) return;
      const shell = live.current.shellOf(paneId);
      runCommand<string>("pane.drop", { paths: e.paths, ...(shell ? { shell } : {}) }).then(
        (text) => {
          live.current.onDropped(paneId);
          const h = handles.get(paneId);
          h?.insert(text);
          h?.focus();
        },
        (err) => notify.error("Couldn't drop that", { description: errorText(err) }),
      );
    });
    return () => void stop.then((f) => f());
  }, []);
  return null;
}
