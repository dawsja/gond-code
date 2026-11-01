// The UI's one view of the backend: the state tree, mirrored by state.ts, and
// command_run for every registered command. Agents read and run the same; nothing here is UI-only.
import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import generated from "./commands.generated.json";
import { call, mockBackend, type Commands } from "./ipc";
import { connectState, createStateStore, type StateStore } from "./state";

let store: StateStore = createStateStore({ epoch: "mock", version: 0, value: {} });
let connecting: Promise<void> | undefined;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());

/** Connects once, on first use, in the real app; the browser mock has no tree. */
function connect() {
  if (connecting || mockBackend()) return;
  connecting = connectState()
    .then((s) => {
      store = s;
      s.subscribe(notify);
      notify();
    })
    .catch(() => undefined);
}

function subscribe(cb: () => void) {
  connect();
  listeners.add(cb);
  return () => void listeners.delete(cb);
}

/**
 * A subtree of the state tree, re-rendering only when it changes. Select a subtree, not a derived
 * value: patches keep untouched subtrees' identity, so `v => v.projects` is stable between patches.
 */
export function useAppState<T>(select: (tree: Record<string, unknown>) => T): T {
  return useSyncExternalStore(subscribe, () => select(store.get().value));
}

/** For tests: swap in a store. */
export function setAppStateStore(s: StateStore) {
  store = s;
  connecting = Promise.resolve();
  s.subscribe(notify);
  notify();
}

const REGISTERED = new Set(generated.map((c) => c.name));

/** Whether the backend runs this command through the registry (commands.generated.json). */
export const isRegistered = (name: string) => REGISTERED.has(name);

/** Runs a registered command as the human: one path for the UI, agents and the CLI. */
export function runCommand<T = unknown>(name: string, input: Record<string, unknown> = {}) {
  return invoke<T>("command_run", { name, input });
}

/** True when the UI talks to the real backend (registry + tree), not the browser mock. */
export const liveBackend = () => !mockBackend();

/**
 * One call site for both backends: the registered command through command_run in the app, the
 * old per-feature IPC on the browser mock. Same input and output ("Was").
 */
export function callOrRun<K extends keyof Commands>(
  cmd: K,
  name: string,
  args: Commands[K][0],
): Promise<Commands[K][1]> {
  return liveBackend() && isRegistered(name)
    ? runCommand<Commands[K][1]>(name, args as Record<string, unknown>)
    : call(cmd, args);
}
