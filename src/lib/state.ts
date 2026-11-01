// Mirror of the Rust state tree: a snapshot from
// `state_get`, kept current by `state:patch` events. Agents read the same document.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type JsonPatchOp =
  { op: "add" | "replace"; path: string; value: unknown } | { op: "remove"; path: string };

export type Patch = { epoch: string; version: number; ops: JsonPatchOp[]; cause: unknown };

export type Snapshot = { epoch: string; version: number; value: Record<string, unknown> };

const unescape = (key: string) => key.replace(/~1/g, "/").replace(/~0/g, "~");

/** Applies ops immutably: changed objects are copied along the path, the rest is shared,
 * so React selectors on untouched subtrees keep their identity. */
export function applyOps<T>(doc: T, ops: JsonPatchOp[]): T {
  let root: unknown = doc;
  for (const op of ops) {
    if (op.path === "") {
      root = op.op === "remove" ? null : op.value;
      continue;
    }
    const keys = op.path.slice(1).split("/").map(unescape);
    const next = { ...(root as Record<string, unknown>) };
    let node = next;
    for (const k of keys.slice(0, -1)) {
      const child = node[k];
      if (typeof child !== "object" || child === null) throw new Error(`no object at ${op.path}`);
      node[k] = { ...(child as Record<string, unknown>) };
      node = node[k] as Record<string, unknown>;
    }
    const last = keys[keys.length - 1];
    if (op.op === "remove") delete node[last];
    else node[last] = op.value;
    root = next;
  }
  return root as T;
}

export type StateStore = {
  get: () => Snapshot;
  subscribe: (cb: () => void) => () => void;
  /** Applies one patch; returns false when it doesn't follow on (caller resyncs). */
  receive: (p: Patch) => boolean;
  reset: (s: Snapshot) => void;
};

export function createStateStore(initial: Snapshot): StateStore {
  let snap = initial;
  const subs = new Set<() => void>();
  const notify = () => subs.forEach((cb) => cb());
  return {
    get: () => snap,
    subscribe: (cb) => {
      subs.add(cb);
      return () => subs.delete(cb);
    },
    receive: (p) => {
      if (p.epoch !== snap.epoch || p.version !== snap.version + 1)
        return p.version <= snap.version && p.epoch === snap.epoch;
      snap = { epoch: p.epoch, version: p.version, value: applyOps(snap.value, p.ops) };
      notify();
      return true;
    },
    reset: (s) => {
      snap = s;
      notify();
    },
  };
}

/** Connects a store to the app: snapshot, then patches; a gap or new epoch refetches. */
export async function connectState(): Promise<StateStore> {
  if (!isTauri()) return createStateStore({ epoch: "mock", version: 0, value: {} });
  const store = createStateStore(await invoke<Snapshot>("state_get"));
  await listen<Patch>("state:patch", async (e) => {
    if (!store.receive(e.payload)) store.reset(await invoke<Snapshot>("state_get"));
  });
  return store;
}
