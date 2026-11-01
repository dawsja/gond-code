// In-memory stand-in for the Rust side, so the UI runs in a plain browser.
import type { EventCallback } from "@tauri-apps/api/event";
import type { Commands, Events } from "./ipc";
import { startShell } from "./mockShell";
import type { AgentInfo, Project, Settings } from "./types";

const now = () => new Date().toISOString();
let seq = 0;
const id = () => `mock-${++seq}`;

const projects: Project[] = [
  {
    id: "p1",
    name: "gond-code",
    path: "C:\\src\\gond-code",
    defaultBranch: "main",
    addedAt: now(),
    lastOpenedAt: now(),
  },
  {
    id: "p2",
    name: "website",
    path: "C:\\src\\website",
    defaultBranch: "main",
    addedAt: now(),
    lastOpenedAt: now(),
  },
];

const agents: AgentInfo[] = [
  {
    id: "claude",
    kind: "claude",
    name: "Claude Code",
    command: "claude",
    provider: "claude",
    installed: true,
    version: "2.1.3",
  },
  {
    id: "codex",
    kind: "codex",
    name: "Codex",
    command: "codex",
    provider: "codex",
    installed: true,
    version: "0.58.0",
  },
  {
    id: "qwen-code",
    kind: "custom",
    name: "Qwen Code",
    command: "qwen",
    provider: "qwen-code",
    installed: true,
    version: "0.2.1",
  },
  {
    id: "my-agent",
    kind: "custom",
    name: "My agent",
    command: "./agent.ps1",
    provider: "custom",
    installed: true,
  },
  // Not installed, to show the disabled state.
  {
    id: "aider",
    kind: "custom",
    name: "Aider",
    command: "aider",
    provider: "custom",
    installed: false,
  },
];

let settings: Settings = {
  tint: "blue",
  closeToTray: true,
  keepTerminals: true,
  terminalFont: "jetbrains-mono",
};

type Listener = (payload: unknown) => void;
const listeners = new Map<string, Set<Listener>>();

function emit<K extends keyof Events>(event: K, payload: Events[K]) {
  listeners.get(event)?.forEach((cb) => cb(payload));
}

const shells = new Map<string, (data: string) => void>();
let typed = 0;

const handlers: { [K in keyof Commands]: (args: Commands[K][0]) => Commands[K][1] } = {
  app_version: () => "0.1.0-dev",
  project_add: ({ path }) => {
    if (/downloads/i.test(path)) {
      throw { code: "not_a_git_repo", message: `${path} is not a git repository.` };
    }
    const name = path.split(/[\\/]/).pop() || path;
    const project = {
      id: id(),
      name,
      path,
      defaultBranch: "main",
      addedAt: now(),
      lastOpenedAt: now(),
    };
    projects.unshift(project);
    return project;
  },
  project_list: () => [...projects].sort((a, b) => b.lastOpenedAt.localeCompare(a.lastOpenedAt)),
  project_remove: ({ projectId }) => {
    projects.splice(
      projects.findIndex((p) => p.id === projectId),
      1,
    );
    return null;
  },
  project_touch: ({ projectId }) => {
    const p = projects.find((x) => x.id === projectId);
    if (p) p.lastOpenedAt = now();
    return null;
  },
  agent_detect: () => agents,
  pty_open: ({ projectId }) => {
    const ptyId = id();
    const cwd = projects.find((p) => p.id === projectId)?.path ?? "C:\\";
    const write = startShell(
      cwd,
      (data) => setTimeout(() => emit("pty:data", { ptyId, data })),
      (code) => {
        shells.delete(ptyId);
        setTimeout(() => emit("pty:exit", { ptyId, code }));
      },
    );
    shells.set(ptyId, write);
    // `?typed` (dev) runs a command in each new shell, for screenshots.
    if (new URLSearchParams(globalThis.location?.search).has("typed")) {
      setTimeout(() => write(++typed % 2 ? "git status\r" : "ls\r"), 300);
    }
    return { ptyId };
  },
  pty_write: ({ ptyId, data }) => {
    shells.get(ptyId)?.(data);
    return null;
  },
  pty_resize: () => null,
  pty_close: ({ ptyId }) => {
    shells.delete(ptyId);
    return null;
  },
  settings_get: () => ({ ...settings }),
  settings_set: ({ patch }) => (settings = { ...settings, ...patch }),
};

// Alternates a git repo with a plain folder, so both paths of "Open folder" can be tried.
const picks = ["C:\\src\\api-server", "C:\\Users\\you\\Downloads"];
let pick = 0;
export async function mockPickFolder() {
  return picks[pick++ % picks.length];
}

export async function mockCall<K extends keyof Commands>(cmd: K, args: Commands[K][0]) {
  return structuredClone(handlers[cmd](args));
}

export async function mockOn<K extends keyof Events>(event: K, cb: EventCallback<Events[K]>) {
  const listener: Listener = (payload) => cb({ event, id: 0, payload: payload as Events[K] });
  const set = listeners.get(event) ?? new Set();
  set.add(listener);
  listeners.set(event, set);
  return () => void set.delete(listener);
}
