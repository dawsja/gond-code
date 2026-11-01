// The panes: a tree of splits, right (side by side) or down (one above the other), whose leaves
// are terminals, each a shell in its folder; an agent is whatever you start in one. Kept on this
// PC, so the app reopens as you left it.

/** A terminal: a shell in the folder. */
export type Pane = {
  kind: "pane";
  id: string;
  projectId: string;
  /** A terminal's shell (shell.list id), once it has run: it keeps it, whatever Settings says. */
  shell?: string;
  /** Your name for it (Rename); else the running agent's name, or "Terminal". */
  title?: string;
  /** The agent running in it and how to pick its conversation back up (agent.resume), kept
   *  while it runs: if the PC restarts, the pane's new shell resumes it. */
  resume?: Resume;
};

/** How a pane's agent comes back: its conversation, and the command that resumes it. */
export type Resume = { agent: string; session?: string; command: string; cwd?: string };

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

/** Panes `a` and `b` trade places; the splits stay as they are. */
export function swap(root: Node | null, a: string, b: string): Node | null {
  const pa = find(root, a);
  const pb = find(root, b);
  if (!root || !pa || !pb || a === b) return root;
  const walk = (n: Node): Node =>
    n.kind === "pane"
      ? n.id === a
        ? pb
        : n.id === b
          ? pa
          : n
      : { ...n, a: walk(n.a), b: walk(n.b) };
  return walk(root);
}

/** Which side of a pane another lands on when it's dropped by that edge. */
export type Side = "left" | "right" | "up" | "down";

/** Pane `id` taken out of its place and put beside pane `target`, on `side`, half and half. */
export function moveBeside(root: Node | null, id: string, target: string, side: Side): Node | null {
  const moving = find(root, id);
  if (!root || !moving || id === target || !find(root, target)) return root;
  const dir: Split["dir"] = side === "left" || side === "right" ? "right" : "down";
  const first = side === "left" || side === "up";
  const walk = (n: Node): Node => {
    if (n.kind === "split") return { ...n, a: walk(n.a), b: walk(n.b) };
    if (n.id !== target) return n;
    return {
      kind: "split",
      id: newId(),
      dir,
      ratio: 0.5,
      a: first ? moving : n,
      b: first ? n : moving,
    };
  };
  return walk(remove(root, id)!);
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
  const saved: Saved =
    Array.isArray(o.sessions) && o.sessions.length
      ? { sessions: o.sessions, current: o.current }
      : Array.isArray(o.groups) && o.groups.length
        ? {
            sessions: o.groups.map((g) => ({ id: g.id, name: g.name, layout: g.space })),
            current: o.current,
          }
        : { sessions: [{ id: newId(), name: "Main", layout: one(o as Record<string, Layout>) }] };
  return {
    ...saved,
    sessions: saved.sessions.map((g) => ({
      ...g,
      layout: { ...g.layout, root: g.layout.root && shells(g.layout.root) },
    })),
  };
}

/**
 * Every pane as a shell: a pane saved when panes ran an agent directly (its thread, agent and
 * model line) opens as a terminal in its folder, for you to start the agent again.
 */
function shells(n: Node): Node {
  if (n.kind === "split") return { ...n, a: shells(n.a), b: shells(n.b) };
  const { kind, id, projectId, shell, title, resume } = n;
  return {
    kind,
    id,
    projectId,
    ...(shell && { shell }),
    ...(title && { title }),
    ...(resume?.command && { resume }),
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
