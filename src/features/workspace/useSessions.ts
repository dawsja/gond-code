import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isRegistered, liveBackend, runCommand } from "@/lib/appState";
import { notify } from "@/lib/notify";
import { onBeforeRestart } from "@/lib/restart";
import { errorText, type Project } from "@/lib/types";
import {
  find,
  fromSaved,
  load,
  newId,
  panes,
  remove,
  resize,
  save,
  split,
  update,
  type Session,
  type Pane,
  type Saved,
  type Layout,
} from "./layout";
import { dictate } from "./Dictation";
import { showKeys } from "./KeyHelp";
import type { Shortcut } from "./Terminal";

/** The folder you last opened a terminal in. */
const remembered = (key: string) => {
  try {
    return localStorage.getItem(key) ?? undefined;
  } catch {
    return undefined;
  }
};

/**
 * The sessions and their panes: named groups you switch between, each with its own splits.
 * Every pane is a terminal in your default shell; an agent is whatever you start in one
 * (`claude`, `codex`…). Open a terminal beside a pane, close, zoom, rename and restart panes,
 * resize splits.
 */
export function useSessions({ projects }: { projects: Project[] }) {
  const [saved, setSaved] = useState<Saved>(() => fromSaved(load()));
  const [restarts, setRestarts] = useState<Record<string, number>>({});
  const [renaming, setRenaming] = useState<string>();
  // Kept by the app (sessions.json beside its database), so a crash or a forced close can't lose
  // them; this PC's WebView storage is only the fallback. Nothing is saved until they're read,
  // so an empty start never overwrites them.
  const [loaded, setLoaded] = useState(() => !liveBackend() || !isRegistered("sessions.get"));
  useEffect(() => {
    if (loaded) return;
    runCommand<unknown>("sessions.get", {}).then(
      (v) => {
        const read = v && typeof v === "object" && Object.keys(v).length ? fromSaved(v) : saved;
        setSaved(read);
        setLoaded(true);
        // Terminals still running for panes that are gone (closed while the app couldn't save
        // them) are closed; every pane that exists finds its own again.
        if (isRegistered("pty.prune"))
          void runCommand("pty.prune", {
            keep: read.sessions.flatMap((g) => panes(g.layout.root).map((p) => p.id)),
          }).catch(() => undefined);
      },
      () => setLoaded(true),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (!loaded) return;
    save(saved);
    if (!liveBackend() || !isRegistered("sessions.set")) return;
    const t = window.setTimeout(
      () => void runCommand("sessions.set", { sessions: saved }).catch(() => undefined),
      200,
    );
    return () => window.clearTimeout(t);
  }, [saved, loaded]);

  const sessions = saved.sessions;
  const current = sessions.find((g) => g.id === saved.current) ?? sessions[0];
  const savedRef = useRef(saved);
  useEffect(() => {
    savedRef.current = saved;
  }, [saved]);
  // Restart to update saves them at once rather than after the usual pause.
  useEffect(
    () =>
      onBeforeRestart(async () => {
        save(savedRef.current);
        if (liveBackend() && isRegistered("sessions.set"))
          await runCommand("sessions.set", { sessions: savedRef.current });
      }),
    [],
  );

  /** Changes session `id`'s panes (the one you're in when none is named). */
  const change = useCallback(
    (f: (s: Layout) => Layout, id?: string) =>
      setSaved((v) => {
        const target = id ?? v.current ?? v.sessions[0]?.id;
        return {
          ...v,
          sessions: v.sessions.map((g) => (g.id === target ? { ...g, layout: f(g.layout) } : g)),
        };
      }),
    [],
  );
  /** The pane and the session it's in. */
  const paneOf = (paneId: string) => {
    for (const g of savedRef.current.sessions) {
      const pane = find(g.layout.root, paneId);
      if (pane) return { pane, session: g.id };
    }
  };

  const selectSession = useCallback((id: string) => setSaved((v) => ({ ...v, current: id })), []);
  const addSession = useCallback(() => {
    const id = newId();
    setSaved((v) => {
      // The lowest number not in use: counting sessions repeats a name once one is closed.
      const taken = new Set(v.sessions.map((g) => g.name));
      let n = 1;
      while (taken.has(`Session ${n}`)) n++;
      return {
        sessions: [...v.sessions, { id, name: `Session ${n}`, layout: { root: null } }],
        current: id,
      };
    });
    setRenaming(id);
  }, []);
  const renameSession = (id: string, name: string) => {
    setRenaming(undefined);
    if (name.trim())
      setSaved((v) => ({
        ...v,
        sessions: v.sessions.map((g) => (g.id === id ? { ...g, name: name.trim() } : g)),
      }));
  };
  /** Closes a session and every terminal in it; there's always one left. */
  const removeSession = (id: string) => {
    setSaved((v) => {
      const left = v.sessions.filter((x) => x.id !== id);
      const sessions: Session[] = left.length
        ? left
        : [{ id: newId(), name: "Main", layout: { root: null } }];
      return { sessions, current: v.current === id ? sessions[0].id : v.current };
    });
  };

  // The folder a new terminal starts in: the focused pane's, else the last you used, else the first.
  const known = (id?: string) => (id && projects.some((p) => p.id === id) ? id : undefined);
  const focused = current?.layout.focus
    ? find(current.layout.root, current.layout.focus)
    : undefined;
  const folder =
    known(focused?.projectId) ?? known(remembered("gond.lastFolder")) ?? projects[0]?.id;

  const focus = useCallback(
    (paneId: string) => {
      const at = paneOf(paneId);
      if (!at) return;
      setSaved((v) => ({
        current: at.session,
        sessions: v.sessions.map((g) =>
          g.id === at.session && g.layout.focus !== paneId
            ? { ...g, layout: { ...g.layout, focus: paneId } }
            : g,
        ),
      }));
    },

    [],
  );

  /**
   * A terminal in your default shell beside pane `at` (or the session's focused one), right or
   * down, in the same folder.
   */
  const openShell = useCallback(
    (place: "right" | "down", at?: string) => {
      const from = at ? paneOf(at) : undefined;
      const session = from?.session ?? savedRef.current.current;
      change((sp) => {
        const beside = from?.pane ?? (sp.focus ? find(sp.root, sp.focus) : undefined);
        const pane: Pane = {
          kind: "pane",
          id: newId(),
          projectId: beside?.projectId ?? folder ?? "",
        };
        return { root: split(sp.root, beside?.id, place, pane), focus: pane.id, zoom: undefined };
      }, session);
    },
    [change, folder],
  );

  /**
   * An empty session's terminal: a session never sits empty; it opens your
   * default shell (Settings → General) in the folder you used last, else your home folder.
   */
  const startShell = useCallback(
    (sessionId: string) => {
      const pane: Pane = { kind: "pane", id: newId(), projectId: folder ?? "" };
      change((sp) => (sp.root ? sp : { root: pane, focus: pane.id, zoom: undefined }), sessionId);
    },
    [change, folder],
  );

  const close = useCallback(
    (paneId: string) => {
      const at = paneOf(paneId);
      if (!at) return;
      change((s) => {
        const root = remove(s.root, paneId);
        // Closing a session's last terminal gives you a fresh one in the same shell, not the
        // Settings default.
        if (!root) {
          const pane: Pane = {
            kind: "pane",
            id: newId(),
            projectId: at.pane.projectId,
            shell: at.pane.shell,
          };
          return { root: pane, focus: pane.id, zoom: undefined };
        }
        return {
          root,
          focus: panes(root).at(-1)?.id,
          zoom: s.zoom === paneId ? undefined : s.zoom,
        };
      }, at.session);
    },

    [change],
  );

  const shortcut = useCallback(
    (paneId: string, s: Shortcut) => {
      if (s === "dictate") return dictate();
      if (s === "keys") return showKeys();
      const at = paneOf(paneId);
      if (!at) return;
      if (s === "close") return close(paneId);
      if (s === "zoom")
        return change(
          (sp) => ({ ...sp, zoom: sp.zoom === paneId ? undefined : paneId, focus: paneId }),
          at.session,
        );
      openShell(s === "down" ? "down" : "right", paneId);
    },

    [close, change, openShell],
  );

  const actions = useMemo(
    () => ({
      focus,
      shortcut,
      rename: (paneId: string) => setRenaming(paneId),
      restart: (paneId: string) => setRestarts((r) => ({ ...r, [paneId]: (r[paneId] ?? 0) + 1 })),
      openFolder: (pid: string) =>
        void runCommand("folder.open", { projectId: pid }).catch((e) =>
          notify.error("Couldn't open the folder", { description: errorText(e) }),
        ),
      resize: (splitId: string, ratio: number) =>
        change((s) => ({ ...s, root: resize(s.root, splitId, ratio) })),
    }),

    [focus, shortcut, change],
  );

  /** A pane's new name (a session's goes through renameSession). */
  const renamed = (paneId: string, title: string) => {
    const at = paneOf(paneId);
    setRenaming(undefined);
    if (at)
      change(
        (s) => ({ ...s, root: update(s.root, paneId, { title: title.trim() || undefined }) }),
        at.session,
      );
  };

  /**
   * How a pane's shell starts, in its folder: the one it ran before, else Settings' pick, which
   * it then keeps.
   */
  const opener = useCallback(
    (p: Pane) =>
      async ({ cols, rows, cwd }: { cols: number; rows: number; cwd?: string }) => {
        const opened = await runCommand<{ ptyId: string; shell?: string }>("pty.open", {
          key: p.id,
          ...(p.projectId ? { projectId: p.projectId } : {}),
          ...(p.shell ? { shell: p.shell } : {}),
          ...(cwd ? { cwd } : {}),
          cols,
          rows,
        });
        const at = paneOf(p.id);
        if (at && opened.shell && opened.shell !== at.pane.shell)
          change(
            (s) => ({ ...s, root: update(s.root, p.id, { shell: opened.shell }) }),
            at.session,
          );
        return opened;
      },
    [change],
  );

  return {
    sessions,
    current,
    loaded,
    folder,
    selectSession,
    addSession,
    renameSession,
    removeSession,
    restarts,
    renaming,
    renamed,
    openShell,
    startShell,
    actions,
    opener,
  };
}
