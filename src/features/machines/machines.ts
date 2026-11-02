// Machines: where a terminal runs besides this PC (a WSL distribution, an SSH machine). A pane's
// `shell` is the machine's id (`wsl:<distro>`, `ssh:<id>`); a session's `machine` is where its
// new terminals open. The connections' state is in the tree at `/machines/<id>`, and a question
// ssh asks while signing in at `/askpass/<id>`.
import { useCallback, useEffect, useState } from "react";
import { isRegistered, liveBackend, runCommand, useAppState } from "@/lib/appState";

/** A machine to open terminals on (`machine.list`). */
export type Machine = {
  id: string;
  kind: "wsl" | "ssh";
  name: string;
  address?: string;
  /** The key an SSH machine signs in with. */
  key?: string;
};

/** An SSH key in ~/.ssh (`machine.key.list`). */
export type SshKey = {
  name: string;
  path: string;
  kind: string;
  fingerprint: string;
  comment: string;
  passphrase: boolean;
  publicKey: string;
  default: boolean;
  /** The SSH machines that sign in with it. */
  usedBy: string[];
};

/** A machine's connection (`/machines/<id>`). */
export type Link = {
  id: string;
  kind: "wsl" | "ssh";
  name: string;
  state: "connecting" | "up" | "down" | "off";
  step?: string;
  error?: string;
  retrying?: boolean;
  /** It asked for a password when it signed in: a key would spare you that. */
  password?: boolean;
  /** How many times it has come up: a pane picks its terminal back up when this changes. */
  up: number;
  probe?: {
    os: string;
    arch: string;
    user: string;
    home: string;
    distro?: string;
    hostname?: string;
  };
};

/** A question ssh is asking (`/askpass/<id>`). */
export type Question = {
  id: string;
  machine: string;
  machineName: string;
  prompt: string;
  kind: "confirm" | "secret";
};

/** Whether a shell id is a machine's rather than a shell on this PC. */
export const isMachine = (id?: string) => !!id && (id.startsWith("wsl:") || id.startsWith("ssh:"));

/** Every connection, by machine. */
export function useLinks(): Record<string, Link> {
  return useAppState((t) => t.machines as Record<string, Link> | undefined) ?? NONE;
}
const NONE = {};

export function useLink(id?: string): Link | undefined {
  const links = useLinks();
  return id ? links[id] : undefined;
}

export function useQuestions(): Question[] {
  const q = useAppState((t) => t.askpass as Record<string, Question> | undefined);
  return q ? Object.values(q) : [];
}

/** The machines (`machine.list`), read again on `refresh` and whenever one is added or removed. */
let listeners = new Set<() => void>();
export const machinesChanged = () => listeners.forEach((l) => l());

export function useMachines() {
  const [list, setList] = useState<{
    machines: Machine[];
    sshConfig: string[];
    configFile?: string;
  }>({
    machines: [],
    sshConfig: [],
  });
  const refresh = useCallback(() => {
    if (!liveBackend() || !isRegistered("machine.list")) return;
    runCommand<{ machines: Machine[]; sshConfig: string[]; configFile: string }>(
      "machine.list",
      {},
    ).then(setList, () => undefined);
  }, []);
  useEffect(() => {
    refresh();
    listeners.add(refresh);
    return () => void listeners.delete(refresh);
  }, [refresh]);
  return { ...list, refresh };
}

/** Your SSH keys (`machine.key.list`), read again with the machines. */
export function useKeys() {
  const [keys, setKeys] = useState<SshKey[]>([]);
  const refresh = useCallback(() => {
    if (!liveBackend() || !isRegistered("machine.key.list")) return;
    runCommand<{ keys: SshKey[] }>("machine.key.list", {}).then(
      (r) => setKeys(r.keys),
      () => undefined,
    );
  }, []);
  useEffect(() => {
    refresh();
    listeners.add(refresh);
    return () => void listeners.delete(refresh);
  }, [refresh]);
  return keys;
}

/** A machine's name from its id, as far as it's known. */
export function machineName(id: string, machines: Machine[], links: Record<string, Link>) {
  return (
    machines.find((m) => m.id === id)?.name ??
    links[id]?.name ??
    (id.startsWith("wsl:") ? id.slice(4) : "SSH machine")
  );
}

const OPEN = "gond:machines";
const ADD = "gond:add-machine";

/** Opens the machine switcher (Ctrl+Shift+M). */
export const showSwitcher = () => window.dispatchEvent(new Event(OPEN));
export const onShowSwitcher = (f: () => void) => {
  window.addEventListener(OPEN, f);
  return () => window.removeEventListener(OPEN, f);
};

/** Opens "Add SSH machine", with a host to start from. */
export const showAddMachine = (host?: string) =>
  window.dispatchEvent(new CustomEvent(ADD, { detail: host }));
export const onShowAddMachine = (f: (host?: string) => void) => {
  const h = (e: Event) => f((e as CustomEvent<string | undefined>).detail);
  window.addEventListener(ADD, h);
  return () => window.removeEventListener(ADD, h);
};

const EDIT = "gond:edit-machine";

/** Opens "Edit <machine>": where Gond reaches an SSH machine. */
export const showEditMachine = (m: Machine) =>
  window.dispatchEvent(new CustomEvent(EDIT, { detail: m }));
export const onShowEditMachine = (f: (m: Machine) => void) => {
  const h = (e: Event) => f((e as CustomEvent<Machine>).detail);
  window.addEventListener(EDIT, h);
  return () => window.removeEventListener(EDIT, h);
};

const CREATE_KEY = "gond:create-key";
const SET_UP_KEY = "gond:set-up-key";

/** Opens "Create SSH key", with the machines to set it up on ticked. */
export const showCreateKey = (machines: string[] = []) =>
  window.dispatchEvent(new CustomEvent(CREATE_KEY, { detail: machines }));
export const onShowCreateKey = (f: (machines: string[]) => void) => {
  const h = (e: Event) => f((e as CustomEvent<string[]>).detail ?? []);
  window.addEventListener(CREATE_KEY, h);
  return () => window.removeEventListener(CREATE_KEY, h);
};

/** Opens "Sign in to <machine> with a key", with a key to start from. */
export const showSetUpKey = (machine: string, key?: string) =>
  window.dispatchEvent(new CustomEvent(SET_UP_KEY, { detail: { machine, key } }));
export const onShowSetUpKey = (f: (machine: string, key?: string) => void) => {
  const h = (e: Event) => {
    const d = (e as CustomEvent<{ machine: string; key?: string }>).detail;
    f(d.machine, d.key);
  };
  window.addEventListener(SET_UP_KEY, h);
  return () => window.removeEventListener(SET_UP_KEY, h);
};

/** Resets the listener set (tests). */
export const _resetMachineListeners = () => (listeners = new Set());
