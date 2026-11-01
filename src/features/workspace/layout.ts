// The panes: a tree of splits, right (side
// by side) or down (one above the other), whose leaves are terminals: an agent's own CLI on its
// session (a thread), or a plain shell, each in the folder picked for it. Kept on this PC, so the
// app reopens as you left it and each agent resumes its session.

/** A terminal: an agent's when it has a thread, else a shell in the folder. */
export type Pane = {
  kind: "pane";
  id: string;
  projectId: string;
  /** The agent's session (thread.create); none for a shell. */
  threadId?: string;
  agentId?: string;
  /** A terminal's shell (shell.list id), once it has run: it keeps it, whatever Settings says. */
  shell?: string;
  /** Your name for it (Rename); else the agent's name. */
  title?: string;
  /** What it runs, for its header ("Opus 5.5 · High · Auto"). */
  detail?: string;
};

/** Two panes or splits: side by side ("right") or stacked ("down"); `ratio` is the first's share. */
export type Split = {
  kind: "split";
  id: string;
  dir: "right" | "down";
  ratio: number;
  a: Node;
  b: Node;
};

export type Node = Pane | Split;

/** One folder's panes, the one you're in and the one zoomed to fill the folder, if any. */
export type Layout = { root: Node | null; focus?: string; zoom?: string };

export const newId = () => `p${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;

/** Every pane, in reading order. */
export function panes(node: Node | null): Pane[] {
  if (!node) return [];
  return node.kind === "pane" ? [node] : [...panes(node.a), ...panes(node.b)];
}

export function find(node: Node | null, id: string): Pane | undefined {
  return panes(node).find((p) => p.id === id);
}

/** `pane` beside `at` (to its right or below it); the first pane when the folder has none. */
export function split(
  root: Node | null,
  at: string | undefined,
  dir: Split["dir"],
  pane: Pane,
): Node {
  if (!root) return pane;
  const target = at && find(root, at) ? at : panes(root).at(-1)!.id;
  const walk = (n: Node): Node => {
    if (n.kind === "pane")
      return n.id === target ? { kind: "split", id: newId(), dir, ratio: 0.5, a: n, b: pane } : n;
    return { ...n, a: walk(n.a), b: walk(n.b) };
  };
  return walk(root);
}

/** Without pane `id`: its sibling takes the split's place. */
export function remove(root: Node | null, id: string): Node | null {
  if (!root) return null;
  if (root.kind === "pane") return root.id === id ? null : root;
  if (root.a.kind === "pane" && root.a.id === id) return root.b;
  if (root.b.kind === "pane" && root.b.id === id) return root.a;
  return { ...root, a: remove(root.a, id)!, b: remove(root.b, id)! };
}

/** Changes pane `id` (rename, a new thread on restart). */
export function update(root: Node | null, id: string, change: Partial<Pane>): Node | null {
  if (!root) return null;
  if (root.kind === "pane") return root.id === id ? { ...root, ...change } : root;
  return { ...root, a: update(root.a, id, change)!, b: update(root.b, id, change)! };
}

/** A split's new share, kept so neither side vanishes. */
export function resize(root: Node | null, splitId: string, ratio: number): Node | null {
  if (!root || root.kind === "pane") return root;
  if (root.id === splitId) return { ...root, ratio: Math.min(0.85, Math.max(0.15, ratio)) };
  return { ...root, a: resize(root.a, splitId, ratio)!, b: resize(root.b, splitId, ratio)! };
}

/**
 * The one workspace from what was saved: `all`, or (saved by the first build, one per folder)
 * every folder's panes side by side, so nothing that was open goes missing.
 */
export function one(saved: Record<string, Layout>): Layout {
  if (saved.all) return saved.all;
  let root: Node | null = null;
  for (const s of Object.values(saved)) {
    if (!s?.root) continue;
    root = root
      ? { kind: "split", id: newId(), dir: "right", ratio: 0.5, a: root, b: s.root }
      : s.root;
  }
  return { root, focus: panes(root).at(-1)?.id };
}

/**
 * A session: a named group of panes you switch between.
 */
export type Session = { id: string; name: string; layout: Layout };

/** What's kept: every session, in order, and the one you're in. */
export type Saved = { sessions: Session[]; current?: string };

/**
 * The sessions from what was saved. Older saves still open: the build that called them spaces
 * (`groups`, each with a `space`), and before that one workspace, or one per folder, as "Main".
 */
export function fromSaved(v: unknown): Saved {
  const o = (v && typeof v === "object" ? v : {}) as Partial<Saved> & {
    groups?: { id: string; name: string; space: Layout }[];
  } & Record<string, unknown>;
  if (Array.isArray(o.sessions) && o.sessions.length)
    return { sessions: o.sessions, current: o.current };
  if (Array.isArray(o.groups) && o.groups.length)
    return {
      sessions: o.groups.map((g) => ({ id: g.id, name: g.name, layout: g.space })),
      current: o.current,
    };
  return {
    sessions: [{ id: newId(), name: "Main", layout: one(o as Record<string, Layout>) }],
  };
}

const KEY = "gond.sessions";

/** The sessions as this PC's WebView last saved them (the app's own copy comes first); a
 *  save from before the rename is read too. */
export function load(): unknown {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? localStorage.getItem("gond.spaces") ?? "{}");
    return v && typeof v === "object" ? v : {};
  } catch {
    return {};
  }
}

export function save(saved: Saved) {
  try {
    localStorage.setItem(KEY, JSON.stringify(saved));
  } catch {
    // Storage full or blocked: the panes still work, they just won't come back next time.
  }
}
